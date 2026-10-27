#!/usr/bin/env node
// Explicitly allowlisted public-fixture research. Offline, no privacy authorization.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {createHash} from 'node:crypto';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {publicResult} from './framework-lift-public-result.mjs';
import {verifiedFiles} from './framework-lift-history-preview.mjs';
import {digest} from '../lib/framework-lift-executor.mjs';
import {observations} from './framework-lift-runner.mjs';
const sha=b=>createHash('sha256').update(b).digest('hex');
const integer=v=>{assert.ok(Number.isSafeInteger(v)&&v>=0,'invalid measurement');return v;};
const rational=(n,d)=>{if(d===0)return {state:'unknown'};let a=BigInt(n),b=BigInt(d),x=a<0n?-a:a,y=b;while(y){const r=x%y;x=y;y=r;}return {state:'known',numerator:String(a/x),denominator:String(b/x)};};

export function exposureSummary(obs,units,kind){
 let semanticBytesLowerBound=0,unverifiedSemanticMarkers=0;
 for(const o of obs)for(const match of o.content.matchAll(/^@@semantic-content ([^\r\n]+)\r?\n/gm)){
  const receipt=JSON.parse(match[1]);integer(receipt.bytes);assert.match(receipt.digest,/^[a-f0-9]{64}$/);
  const rest=o.content.slice(match.index+match[0].length),candidates=[rest,rest.replaceAll('\r\n','\n')];
  if(candidates.some(text=>{const body=Buffer.from(text).subarray(0,receipt.bytes);return body.length===receipt.bytes&&sha(body)===receipt.digest;}))semanticBytesLowerBound+=receipt.bytes;else unverifiedSemanticMarkers++;
 }
 assert.ok(['devin-agent-step','codex-turn-completed-aggregate'].includes(kind));
 return {usageUnitKind:kind,usageUnitCount:units.length,largestUsageUnitOutputTokens:units.length?{state:'known',value:Math.max(...units.map(u=>integer(u.output)))}:{state:'unknown'},fullInstructionReadDetected:obs.some(o=>o.content.includes('# Work with Lekalo in this project')&&o.content.includes('Read source once per version and needed range')),confinedReaderRefusals:obs.filter(o=>o.content.includes('confined src/**/*.ts path required')).length,semanticBytesLowerBound,unverifiedSemanticMarkers};
}

export function assertFreshReportPath(out,archives){
 assert.ok(!fs.existsSync(out),'report output must be new');
 const norm=p=>{const s=p.replaceAll('\\','/');return process.platform==='win32'?s.toLowerCase():s;};let parent=resolve(out),tail=[];
 while(!fs.existsSync(parent)){tail.unshift(parent.slice(dirname(parent).length+1));parent=dirname(parent);}
 const physical=norm(resolve(fs.realpathSync(parent),...tail));
 for(const archive of archives){const source=norm(fs.realpathSync(archive));assert.ok(physical!==source&&!physical.startsWith(source+'/'),'report output must be outside evidence archives');}
}

export function pairedStatistics(rows,profiles){
 const result=[];
 for(const profile of profiles){
  const group=rows.filter(r=>r.profile===profile),repetitions=[...new Set(group.map(r=>r.repetition))].sort((a,b)=>a-b),pairs=[];
  for(const repetition of repetitions){
   const a=group.filter(r=>r.repetition===repetition&&r.arm==='A'),b=group.filter(r=>r.repetition===repetition&&r.arm==='B');
   assert.equal(a.length,1,'one scheduled A required');assert.equal(b.length,1,'one scheduled B required');
   assert.ok([a[0],b[0]].every(r=>r.state==='attempted'),'unattempted pair requires a future recipe');
   pairs.push({repetition,a:a[0],b:b[0]});
  }
  assert.ok(pairs.length);assert.equal(pairs.length*2,group.length);
  const sum=(arm,key)=>pairs.reduce((n,p)=>integer(n+integer(p[arm][key])),0);
  result.push({profile,pairs:pairs.length,successes:{A:pairs.filter(p=>p.a.success).length,B:pairs.filter(p=>p.b.success).length},metrics:Object.fromEntries(['input','output','total','cachedInput','uncachedInput','agentDurationMs','sourceFilesLowerBound'].map(key=>{
   const A=sum('a',key),B=sum('b',key);
   return [key,{sums:{A,B},means:{A:rational(A,pairs.length),B:rational(B,pairs.length)},relativeChangeBvsA:rational(B-A,A),pairedDeltas:pairs.map(p=>({repetition:p.repetition,BminusA:integer(p.b[key])-integer(p.a[key])}))}];
  }))});
 }
 assert.equal(result.reduce((n,r)=>n+r.pairs*2,0),rows.length,'unknown profile');return result;
}

export function researchReport(original,originalPin,admission,admissionPin,version='1'){
 assert.ok(['1','2'].includes(version),'unsupported research recipe version');
 const recipeBytes=fs.readFileSync(new URL('./public-fixture-research.recipe.v'+version+'.json',import.meta.url)),recipe=JSON.parse(recipeBytes);
 const admittedCampaign=recipe.campaigns.find(c=>c.originalManifestSha256===originalPin&&c.admissionManifestSha256===admissionPin);
 assert.ok(admittedCampaign,'campaign is not in the reviewed public-fixture allowlist');
 if(version==='2')assert.match(admittedCampaign.preregistration,/^https:\/\/github\.com\/ichinya\/lekalo\/issues\/167#issuecomment-[0-9]+$/);
 const source=publicResult(original,originalPin),files=verifiedFiles(admission,admissionPin);
 const read=p=>JSON.parse(files.get(p)??assert.fail('unmanifested admitted input'));
 assert.equal(source.archive.campaignSha256,admittedCampaign.campaignSha256);
 for(const key of ['task','oracle','native'])assert.equal(source.pins[key],admittedCampaign[key+'Sha256']);
 const audit=read('admitted/admission.json');assert.equal(audit.sourceCampaignSha256,source.archive.campaignSha256);assert.equal(audit.archiveManifestSha256,originalPin);
 assert.equal(audit.status,'recorded-unverified-local-private');assert.equal(audit.recordedArms,source.rows.length);
 assert.equal(audit.parity.totalTokens,source.totals.total);assert.equal(audit.parity.strictSuccesses,source.totals.strictSuccesses);
 assert.equal(audit.profiles.length,source.profiles.length);
 const admissionResults=[];
 for(const p of source.profiles){
  const summary=audit.profiles.find(r=>r.profile===p.id);assert.ok(summary);
  const comparison=read('admitted/'+p.id+'-result.json');assert.equal(digest(comparison),summary.resultDigest);
  assert.equal(comparison.evidenceStatus,'recorded-unverified');
  const expected=source.rows.filter(r=>r.profile===p.id);assert.equal(comparison.rows.length,expected.length);
  for(const row of expected){
   const a=read('admitted/'+row.id+'-arm.json'),matches=comparison.rows.filter(r=>r.slot.arm===row.arm&&r.slot.repetition===row.repetition);assert.equal(matches.length,1);
   assert.equal(matches[0].armDigest.value,digest(a));assert.deepEqual(matches[0].metrics,a.metrics);assert.equal(matches[0].verifiedSuccess,row.success);
   for(const [metric,key]of [['inputTokens','input'],['outputTokens','output'],['totalTokens','total'],['cachedInputTokens','cachedInput'],['durationMs','agentDurationMs'],['sourceFilesReadLowerBound','sourceFilesLowerBound']])assert.deepEqual(a.metrics[metric],{state:'known',value:row[key]});
  }
  admissionResults.push({profile:p.id,resultDigest:summary.resultDigest,evidenceStatus:comparison.evidenceStatus});
 }
 const interpretation={sample:'Two A/B pairs per CLI on one small public fixture; descriptive only.',claim:version==='1'?'No demonstrated general token savings. Both profiles used more total tokens in B.':'Descriptive within-campaign comparisons only; no general efficiency claim.',causality:version==='1'?'Added prompt/capsule overhead and conversation replay are plausible contributors; this campaign does not isolate their causal shares.':'Compact instruction and grouped pointers are a combined intervention. Historical comparisons also change adapter/harness pins; no pooling or isolated causal patch effect.',admission:'Post-run 1.1 admission preserves the original observations. It is not provider authentication.',export:'Research disclosure, not #102 public metrics export; internal history labels and existing disclosure thresholds remain unchanged.'};
 const limitations=version==='1'?source.limitations:['Two pairs per CLI on one small public fixture in nonrandom A1/B1/B2/A2 order. All incomplete/failed attempts stay in the denominator.','Cached input is a subset of input and reasoning a subset of output; never add them again. Money, hidden provider revision, authoring and human review costs remain unknown.','Source files are verified src-only lower bounds; exhaustive all-file delivery is unknown. Native Read/Edit/diff deliveries need separate review.','Manifest hashes establish local receipt integrity, not provider authentication. Raw account exports remain private.','Partial authored Model retains strict authorization gaps and lacks canonical/native ID joins. Native tests do not qualify full semantic adoption.','Research disclosure is an explicit allowlist, not a #102 privacy-export receipt.'];
 const workflowObservations=version==='1'?null:source.rows.map(row=>{
  const profile=source.profiles.find(p=>p.id===row.profile),dir=join(original,'campaign/runs',row.id),obs=observations(dir,profile.cli);
  const units=profile.cli==='devin'?JSON.parse(fs.readFileSync(join(dir,'trajectory.json'),'utf8')).steps.filter(s=>s.metrics?.prompt_tokens!==undefined).map(s=>({output:s.metrics.completion_tokens})):fs.readFileSync(join(dir,'stdout.jsonl'),'utf8').split(/\r?\n/).filter(Boolean).map(l=>JSON.parse(l)).filter(e=>e.type==='turn.completed').map(e=>({output:e.usage.output_tokens}));
  const failedHoldoutAssertions=JSON.parse(fs.readFileSync(join(dir,'holdout.stdout.txt'),'utf8')).results.filter(r=>!r.pass).map(r=>{assert.match(r.name,/^[a-z0-9-]+$/);return r.name;});
  const readAuditBytes=fs.readFileSync(join(dir,'read-policy-audit.json'));assert.equal(sha(readAuditBytes),row.completionEvidence.readAudit);const readAudit=JSON.parse(readAuditBytes);
  return {id:row.id,providerReceiptSha256:row.completionEvidence.provider,failedHoldoutAssertions,unexplainedMarkedSourceRepeats:integer(readAudit.unexplainedMarkedRepeats),reasonedMarkedSourceReplays:integer(readAudit.markedReplays),...exposureSummary(obs,units,profile.cli==='devin'?'devin-agent-step':'codex-turn-completed-aggregate')};
 });
 return {format:'lekalo-public-fixture-research/'+version,recipe:{id:recipe.recipe,sha256:sha(recipeBytes)},campaign:admittedCampaign.id,...(version==='2'?{preregistration:admittedCampaign.preregistration,pins:source.pins}:{}),evidence:'empirical-local-receipts-recorded-unverified',sources:{originalManifestSha256:originalPin,admissionManifestSha256:admissionPin,verifiedOriginalFiles:source.archive.verifiedFiles,verifiedAdmissionFiles:files.size,campaignSha256:source.archive.campaignSha256,admissionResults},profiles:source.profiles,rows:source.rows,totals:source.totals,comparisons:pairedStatistics(source.rows,source.profiles.map(p=>p.id)),...(workflowObservations?{workflowObservations}:{}),interpretation,limitations};
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
 const [original,originalPin,admission,admissionPin,out,version='1']=process.argv.slice(2);assert.ok(original&&originalPin&&admission&&admissionPin&&out,'usage: ORIGINAL MANIFEST_SHA256 ADMISSION MANIFEST_SHA256 NEW_OUTPUT_JSON [RECIPE_VERSION]');
 assertFreshReportPath(out,[original,admission]);const report=researchReport(resolve(original),originalPin,resolve(admission),admissionPin,version);fs.writeFileSync(out,JSON.stringify(report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify({ok:true,sha256:sha(fs.readFileSync(out)),attempts:report.totals.attempted}));
}
