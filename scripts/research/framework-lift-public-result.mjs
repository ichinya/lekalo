#!/usr/bin/env node
// Read-only #167 receipt audit. No inference, network, or edits to archived evidence.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {createHash} from 'node:crypto';
import {join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
const hash=b=>createHash('sha256').update(b).digest('hex');
const number=n=>{assert.ok(Number.isSafeInteger(n)&&n>=0,'invalid count');return n;};
const token=s=>{assert.match(s,/^[a-z][a-z0-9-]{0,95}$/);return s;};
const read=p=>JSON.parse(fs.readFileSync(p,'utf8'));

export function publicResult(root,expectedManifest){
 assert.match(expectedManifest,/^[0-9a-f]{64}$/,'explicit manifest digest required');
 const bytes=fs.readFileSync(join(root,'SHA256-MANIFEST.json'));
 assert.equal(hash(bytes),expectedManifest,'archive manifest drift');
 const manifest=JSON.parse(bytes);assert.equal(manifest.algorithm,'sha256');
 const pins=new Map();
 for(const f of manifest.files){
  assert.match(f.path,/^[a-zA-Z0-9._/-]+$/);assert.ok(f.path.split('/').every(p=>p&&p!=='.'&&p!=='..'),'unsafe archive path');
  assert.ok(!pins.has(f.path),'duplicate archive path');
  let path=root;
  for(const part of f.path.split('/')){path=join(path,part);assert.ok(!fs.lstatSync(path).isSymbolicLink(),'linked archive entry');}
  assert.ok(fs.statSync(path).isFile(),'non-file archive entry');
  const content=fs.readFileSync(path);assert.equal(content.length,f.bytes,'archive length drift');assert.equal(hash(content),f.sha256,'archive content drift');pins.set(f.path,f.sha256);
 }
 const evidence=p=>{assert.ok(pins.has(p),'unmanifested evidence');return pins.get(p);};
 const c=read(join(root,'campaign/campaign.json')),summary=read(join(root,'summary.public.json')),results=read(join(root,'campaign/results.json'));
 assert.equal(evidence('campaign/campaign.json'),summary.campaign);assert.equal(results.campaignDigest,summary.campaign);
 assert.equal(fs.readFileSync(join(root,'campaign/campaign.sha256'),'utf8').trim(),summary.campaign);
 assert.equal(summary.rows.length,results.results.length);assert.equal(summary.scheduled,c.slots.length);
 const rows=[],ids=new Set();
 for(const slot of c.slots){
  token(slot.id);assert.ok(!ids.has(slot.id),'duplicate slot');ids.add(slot.id);
  const r=results.results.find(r=>r.id===slot.id),s=summary.rows.find(r=>r.id===slot.id);
  if(!r){assert.ok(!s);rows.push({id:slot.id,profile:token(slot.profile),arm:slot.arm,repetition:number(slot.repetition),state:'not-run'});continue;}
  assert.ok(s);assert.equal(r.profile,slot.profile);assert.equal(r.arm,slot.arm);assert.equal(r.repetition,slot.repetition);
  const profile=c.profiles.find(p=>p.id===slot.profile),dir='campaign/runs/'+slot.id+'/';let input,output,cached,rawRef;
  if(profile.cli==='devin'){
   rawRef=dir+'trajectory.json';const tr=read(join(root,rawRef));
   const sum=k=>tr.steps.reduce((n,s)=>n+(s.metrics?.[k]??0),0);
   input=sum('prompt_tokens');output=sum('completion_tokens');cached=sum('cached_tokens');
   assert.equal(input,tr.final_metrics.total_prompt_tokens);assert.equal(output,tr.final_metrics.total_completion_tokens);assert.equal(cached,tr.final_metrics.total_cached_tokens);
  }else{
   assert.equal(profile.cli,'codex');rawRef=dir+'stdout.jsonl';
   const turns=fs.readFileSync(join(root,rawRef),'utf8').split(/\r?\n/).filter(Boolean).map(l=>JSON.parse(l)).filter(e=>e.type==='turn.completed');assert.ok(turns.length);
   const sum=k=>turns.reduce((n,e)=>n+number(e.usage[k]),0);
   input=sum('input_tokens');output=sum('output_tokens');cached=sum('cached_input_tokens');
  }
  for(const [k,n]of Object.entries({input,output,cachedInput:cached})){number(n);assert.equal(r.usage[k],n,'provider/collector usage mismatch');assert.equal(s[k],n,'provider/summary usage mismatch');}
  assert.ok(cached<=input);assert.equal(s.total,input+output);assert.equal(s.uncachedInput,input-cached);
  const holdout=read(join(root,dir+'holdout.stdout.txt'));assert.deepEqual(holdout,r.grade.holdout);assert.equal(holdout.total,holdout.results.length);assert.equal(holdout.passed,holdout.results.filter(t=>t.pass).length);
  assert.equal(s.success,r.success);assert.equal(s.complete,r.completion.complete);
  assert.equal(s.nativePass,r.grade.accepted);assert.equal(s.durationMs,r.durationMs);
  assert.equal(s.sourceFilesLowerBound,s.sourceFiles.length);
  rows.push({id:slot.id,profile:token(slot.profile),arm:slot.arm,repetition:number(slot.repetition),state:'attempted',complete:!!s.complete,success:!!s.success,confirmationRejected:!!s.confirmationRejected,nativePass:!!s.nativePass,holdoutPassed:number(holdout.passed),holdoutTotal:number(holdout.total),input:number(input),output:number(output),total:input+output,cachedInput:cached,uncachedInput:input-cached,reasoningOutput:s.reasoningOutput===null?{state:'unknown'}:{state:'known',value:number(s.reasoningOutput)},agentDurationMs:number(s.durationMs),sourceFilesLowerBound:number(s.sourceFilesLowerBound),sourceMarkerBytes:number(s.sourceBytes),toolCalls:number(s.toolCalls),completionEvidence:{collector:evidence(dir+'result.json'),provider:evidence(rawRef),holdout:evidence(dir+'holdout.stdout.txt'),candidateTree:evidence(dir+'candidate-tree.json'),readAudit:evidence(dir+'read-policy-audit.json')}});
 }
 assert.equal(results.results.length,rows.filter(r=>r.state==='attempted').length,'unscheduled result');
 const attempted=rows.filter(r=>r.state==='attempted'),sum=k=>attempted.reduce((n,r)=>n+r[k],0),groups=[];
 for(const p of c.profiles)for(const arm of ['A','B']){const slots=rows.filter(r=>r.profile===p.id&&r.arm===arm),a=slots.filter(r=>r.state==='attempted');groups.push({profile:token(p.id),arm,scheduled:slots.length,attempted:a.length,successes:a.filter(r=>r.success).length,nativePasses:a.filter(r=>r.nativePass).length,means:Object.fromEntries(['total','uncachedInput','agentDurationMs','sourceFilesLowerBound'].map(k=>[k,a.length?a.reduce((n,r)=>n+r[k],0)/a.length:null]))});}
 const profiles=c.profiles.map(p=>({id:token(p.id),cli:token(p.cli),model:token(p.model),cliVersion:p.version,reasoning:token(p.reasoning??'provider-default')}));
 for(const p of profiles)assert.match(p.cliVersion,/^[0-9]+\.[0-9]+\.[0-9]+$/);
 const executables=Object.fromEntries(['node','binary','tsc','devin','codex','adapter','manifest'].map(k=>{assert.match(c.executables[k],/^[0-9a-f]{64}$/);return [k,c.executables[k]];}));
 const nativePin=read(join(root,'campaign/executor-native-pin.json'));evidence('campaign/executor-native-pin.json');
 assert.match(nativePin.nativeCodex.sha256,/^[0-9a-f]{64}$/);executables.codexNative=nativePin.nativeCodex.sha256;
 const runtime={node:c.runtime.node,platform:token(c.runtime.platform),architecture:token(c.runtime.architecture)};assert.match(runtime.node,/^v[0-9]+\.[0-9]+\.[0-9]+$/);
 return {format:'lekalo-167-public-research-result/1',status:'empirical-local-receipts-not-formally-admitted',task:'public-synthetic-priority-planner',preregistration:'https://github.com/ichinya/lekalo/issues/167#issuecomment-6413809580',publishedDiscussion:'https://github.com/ichinya/lekalo/issues/167#issuecomment-6414284604',archive:{manifestSha256:expectedManifest,verifiedFiles:manifest.files.length,campaignSha256:summary.campaign,summarySha256:evidence('summary.public.json')},pins:{task:c.taskDigest,oracle:c.oracleDigest,native:c.nativeDigest,model:c.modelDigest,ir:c.irDigest,builder:c.builderDigest,runner:c.runnerDigest,sourceReader:c.sourceReaderDigest,agentInstruction:c.agentInstructionDigest,executables,runtime},profiles,rows,groups,totals:{scheduled:rows.length,attempted:attempted.length,completed:attempted.filter(r=>r.complete).length,strictSuccesses:attempted.filter(r=>r.success).length,nativePasses:attempted.filter(r=>r.nativePass).length,...Object.fromEntries(['input','output','total','cachedInput','uncachedInput'].map(k=>[k,sum(k)])),cost:{state:'unknown'}},admission:{recordArm:'NOTRUN',compare:'NOTRUN',reason:'v0.6.4 requires numeric sampling and bounded limits plus a pricing snapshot; actual CLI defaults/prices are unknown and budget is uncapped. Never fill these with invented values.',measurementWindow:'Agent start to close excludes independent grading; differs from framework-lift-metrics-1 through acceptance.',approval:'Any future protocol translation is a post-run import, not a new preregistration.'},limitations:['One small public synthetic task; two A/B pairs per CLI, nonrandom A1/B1/B2/A2 order. No general token-savings claim.','Devin B1 passes native tests but is incomplete after a confirmation refusal; retained in every denominator.','Cached input is a subset of input, reasoning a subset of output; subsets are never added again. Money, hidden provider revision, authoring and human review costs are unknown.','Source counts are audited src/**/*.ts lower bounds, not all-file delivery counts; edits and changed-code diff echoes are separate.','Manifest checks establish local receipt integrity, not provider signatures or authentication of execution. Raw account exports remain private.','Partial authored Model retains strict authorization refusal and lacks canonical/native ID join; full semantic adoption is unqualified.','Research disclosure is an explicit allowlist; it is not a product privacy-export receipt under issue #102.']};
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
 const [root,pin,out]=process.argv.slice(2);assert.ok(root&&pin&&out,'usage: ROOT MANIFEST_SHA256 OUTPUT_JSON');
 const result=publicResult(resolve(root),pin);fs.writeFileSync(out,JSON.stringify(result,null,2)+'\n',{flag:'wx'});
 console.log(JSON.stringify({ok:true,verifiedFiles:result.archive.verifiedFiles,totals:result.totals,sha256:hash(fs.readFileSync(out))}));
}
