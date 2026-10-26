#!/usr/bin/env node
// Independent receipts + supplementary public native maintenance, no inference.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {repo,sha,write,inventory} from './framework-lift-rounding.mjs';
import {usage,observations,completionAudit,markedReads} from './framework-lift-runner.mjs';
const read=p=>JSON.parse(fs.readFileSync(p,'utf8'));
export function maintain(root){
 const c=read(join(root,'campaign.json')),runs=read(join(root,'results.json')).results,profile=read(join(root,'scan-profile.json')),rows=[];
 for(const run of runs){
  const work=join(root,'maintenance',run.id,'workspace');assert.ok(!fs.existsSync(work),'do not overwrite maintenance');
  fs.mkdirSync(work,{recursive:true});fs.cpSync(join(root,'adopted'),work,{recursive:true});fs.rmSync(join(work,'src'),{recursive:true});fs.cpSync(join(root,'runs',run.id,'workspace/src'),join(work,'src'),{recursive:true});
  const before=inventory(join(work,'src')),indexPath=join(work,'.lekalo/import/observed/index.json'),accepted=fs.readFileSync(indexPath),receipts=[];
  function cli(args){const start=performance.now(),r=spawnSync(c.paths.binary,['--json','--no-cache',...args,'--project','.'],{cwd:work,encoding:'utf8',timeout:120000,maxBuffer:16e6,windowsHide:true});let doc;try{doc=JSON.parse(r.stdout.trim()||r.stderr.trim());}catch{doc=null;}const row={args,exit:r.status,durationMs:performance.now()-start,stdout:r.stdout,stderr:r.stderr,document:doc};receipts.push(row);return row;}
  const changed=cli(['bindings','audit']);
  const scan=cli(['scan','--target','node-typescript','--profile','standalone',process.execPath,join(repo,'adapters/node-typescript/adapter.mjs'),'--lekalo-project-profile-json',JSON.stringify(profile)]);
  let plan=null,confirmation=null,after=null,repeat=null,deterministic=null;
  if(scan.exit===0){
   plan=cli(['bindings','confirm','--batch','--preview']);if(plan.exit===0&&plan.document.entries.length)confirmation=cli(['bindings','confirm','--batch','--confirm',plan.document.plan]);
   after=cli(['bindings','audit']);
   const b=fs.readFileSync(indexPath);repeat=cli(['scan','--target','node-typescript','--profile','standalone',process.execPath,join(repo,'adapters/node-typescript/adapter.mjs'),'--lekalo-project-profile-json',JSON.stringify(profile)]);
   deterministic=b.equals(fs.readFileSync(indexPath));
  }
  const refusedPreservesIndex=scan.exit!==0?accepted.equals(fs.readFileSync(indexPath)):null;
  assert.deepEqual(inventory(join(work,'src')),before,'maintenance changed source');
  const row={id:run.id,arm:run.arm,modelMetadata:'frozen authored baseline; canonical semantic/native join remains unqualified',preScanAuditExit:changed.exit,scanExit:scan.exit,roots:scan.document?.symbols??null,nativeDeclarations:scan.exit===0?read(indexPath).native_symbols.length:null,staled:scan.document?.staled??null,proposals:plan?.document?.entries?.length??null,reconfirmationExit:confirmation?.exit??null,auditExit:after?.exit??null,current:after?.document?.current??null,repeatExit:repeat?.exit??null,deterministic,refusedPreservesIndex,cliMs:receipts.reduce((n,r)=>n+r.durationMs,0),success:scan.exit===0&&after?.exit===0&&repeat?.exit===0&&deterministic,inference:false,humanReviewCost:'unknown'};
  write(root,'maintenance/'+run.id+'/receipts.json',receipts);write(root,'maintenance/'+run.id+'/result.json',row);rows.push(row);
 }
 write(root,'maintenance-results.json',{campaign:sha(fs.readFileSync(join(root,'campaign.json'))),rows});return rows;
}
export function audit(root){
 const c=read(join(root,'campaign.json')),scheduled=c.slots,done=read(join(root,'results.json')).results,rows=[];
 assert.equal(sha(fs.readFileSync(join(root,'campaign.json'))),fs.readFileSync(join(root,'campaign.sha256'),'utf8').trim());
 for(const r of done){
  const cli=c.profiles.find(p=>p.id===r.profile).cli,dir=join(root,'runs',r.id),u=usage(dir,cli),obs=observations(dir,cli),complete=completionAudit(dir,cli),markers=[],markerFailures=[];
  for(const key of ['input','output','cachedInput','reasoningOutput'])assert.equal(u[key],r.usage[key],'usage '+r.id+' '+key);
  if(u.cachedInput!==null)assert.ok(u.input>=u.cachedInput);
  if(cli==='devin'){const t=read(join(dir,'trajectory.json'));for(const [to,from]of [['input','prompt_tokens'],['output','completion_tokens'],['cachedInput','cached_tokens']])assert.equal(t.steps.filter(s=>s.metrics).reduce((n,s)=>n+(s.metrics[from]??0),0),u[to]);}
  for(const o of obs)for(const m of o.content.matchAll(/@@native-content ([^\r\n]+)\r?\n([\s\S]*?)@@native-end ([^\r\n]+)/g)){const d=JSON.parse(m[1]),bodies=[m[2],m[2].replaceAll('\r\n','\n')];assert.equal(d.path,m[3].trim());if(bodies.some(body=>d.bytes===Buffer.byteLength(body)&&d.sha256===sha(Buffer.from(body))))markers.push(d);else markerFailures.push(d);}
  const sources=[...new Set(markers.map(m=>m.path))].sort(),recorded=read(join(dir,'marked-reads.json'));assert.ok(recorded.uniqueSourceFiles.every(p=>sources.includes(p)));write(dir,'marker-interpretation-audit.json',{policy:'Exact raw bytes first; LF fallback only when both advertised hash and byte count verify. Frozen collector outputs remain unchanged.',legacyRejections:recorded.unverified,remainingUnverified:markerFailures,verified:markers});
  const helper=obs.findIndex(o=>(o.content.includes('# context capsule')&&o.content.includes('Observed mappings:'))||/^@@semantic-(content|reuse) /m.test(o.content)),firstRead=obs.findIndex(o=>/^@@native-content /m.test(o.content)||(o.kind==='read'&&/[\\/]src[\\/].*\.ts$/.test(o.arguments?.file_path??'')));
  const workflow=r.arm==='A'?helper===-1:helper>=0&&(firstRead<0||helper<firstRead);
  const instructionObservation=obs.findIndex(o=>o.content.includes('# Work with Lekalo in this project')&&o.content.includes('Read source once per version and needed range'));
  const instructionLoaded=r.arm==='A'||!c.agentInstructionPath||(instructionObservation>=0&&(firstRead<0||instructionObservation<firstRead));
  const potentialUnmarked=obs.filter(o=>!o.content.includes('@@native-content')&&/src[\\/].*\.ts/i.test(JSON.stringify(o.arguments??{})+' '+(o.command??''))).map(o=>({kind:o.kind,arguments:o.arguments,command:o.command,outputCharacters:o.content.length,preview:o.content.slice(0,500)}));
  write(dir,'unmarked-source-delivery-review.json',potentialUnmarked);
  const readPolicy=markedReads(obs),normalize=p=>p.replaceAll('\\','/').replace(/\/+/g,'/'),workspace=normalize(scheduled.find(s=>s.id===r.id).workspace),directNativeReadCandidates=[];
  for(let i=0;i<obs.length;i++){const o=obs[i];if(o.kind!=='read'||typeof o.arguments?.file_path!=='string')continue;const raw=normalize(o.arguments.file_path),path=raw.toLowerCase().startsWith(workspace.toLowerCase()+'/')?raw.slice(workspace.length+1):raw;if(path.startsWith('src/')&&path.endsWith('.ts'))directNativeReadCandidates.push({observation:i,path,outputCharacters:o.content.length});}
  const semanticEmissions=obs.reduce((n,o)=>n+[...o.content.matchAll(/^@@semantic-content (.+)$/gm)].reduce((sum,m)=>sum+(JSON.parse(m[1]).bytes??0),0),0),semanticReuseRequests=obs.filter(o=>/^@@semantic-reuse /m.test(o.content)).length;
  const policy={unexplainedMarkedRepeats:readPolicy.unexplainedRepeatedRanges.length,markedReplays:readPolicy.overlappingSourceRanges.filter(x=>x.reason).length,directNativeReadCandidates,semanticEmittedBytes:semanticEmissions,semanticReuseRequests,instructionObservation,instructionLoadedBeforeSource:instructionLoaded};write(dir,'read-policy-audit.json',policy);
  const nativeChanged=r.grade.changed.filter(p=>p.startsWith('src/')),metadataChanged=r.grade.changed.filter(p=>p.startsWith('lekalo/'));
  rows.push({id:r.id,profile:r.profile,arm:r.arm,repetition:r.repetition,cliExit:r.cli.exitCode,complete:complete.complete,confirmationRejected:complete.confirmationRejected,nativePass:r.grade.accepted,publicExit:r.grade.publicExit,typecheckExit:r.grade.typecheckExit,holdoutPassed:r.grade.holdout.passed,holdoutTotal:r.grade.holdout.total,unexpected:r.grade.unexpected,workflow,success:r.success&&workflow,input:u.input,output:u.output,total:u.input+u.output,cachedInput:u.cachedInput,uncachedInput:u.cachedInput===null?null:u.input-u.cachedInput,reasoningOutput:u.reasoningOutput,toolCalls:u.toolCalls,durationMs:r.durationMs,sourceFilesLowerBound:sources.length,sourceBytes:markers.reduce((n,m)=>n+m.bytes,0),sourceFiles:sources,readPolicy:policy,unmarkedReviewItems:potentialUnmarked.length,nativeChanged,metadataChanged,monetaryCharge:{state:'unknown'},finalSubmissions:complete.complete?1:0,postSubmissionFixCycles:complete.complete?0:null,humanInterventions:0});
 }
 const mean=(a,k)=>a.every(r=>Number.isFinite(r[k]))?a.reduce((n,r)=>n+r[k],0)/a.length:null,groups=[],deltas=[];
 for(const p of c.profiles)for(const arm of ['A','B']){const a=rows.filter(r=>r.profile===p.id&&r.arm===arm);if(!a.length)continue;groups.push({profile:p.id,arm,scheduled:2,attempted:a.length,successes:a.filter(r=>r.success).length,nativePasses:a.filter(r=>r.nativePass).length,...Object.fromEntries(['input','output','total','cachedInput','uncachedInput','durationMs','sourceFilesLowerBound','sourceBytes','toolCalls'].map(k=>[k,mean(a,k)]))});}
 for(const p of c.profiles){const a=groups.find(g=>g.profile===p.id&&g.arm==='A'),b=groups.find(g=>g.profile===p.id&&g.arm==='B');if(a&&b)deltas.push({profile:p.id,...Object.fromEntries(['input','output','total','uncachedInput','durationMs','sourceFilesLowerBound','sourceBytes'].map(k=>[k,a[k]!==0&&a[k]!==null&&b[k]!==null?100*(b[k]-a[k])/a[k]:null]))});}
 const summary={campaign:sha(fs.readFileSync(join(root,'campaign.json'))),scheduled:scheduled.length,attempted:rows.length,notRun:scheduled.filter(s=>!rows.some(r=>r.id===s.id)).map(s=>s.id),rows,groups,deltas,adoption:read(join(root,'preparation.json')).adoption,maintenance:fs.existsSync(join(root,'maintenance-results.json'))?read(join(root,'maintenance-results.json')).rows:null,limitations:['Two pairs per CLI on one task, no population efficiency inference.','Cold semantic authoring labor/tokens and monetary costs unknown.','Native/canonical semantic join unqualified; strict baseline authorization gap retained.','Source markers are verified lower bounds until manual review of unmarked tool deliveries. All-files read count unknown.','API input-cache and reasoning-output are subsets, never added twice.','Native maintenance is automated controller work outside warm agent duration; human review and semantic Model maintenance costs unknown.','Earlier planner run used different rendering/harness; comparisons across campaigns are descriptive, not isolated patch effects.']};
 write(root,'summary.public.json',summary);return summary;
}
if(process.argv[2]==='--maintain')console.log(JSON.stringify(maintain(resolve(process.argv[3]))));
else if(process.argv[2]==='--audit'){const r=audit(resolve(process.argv[3]));console.log(JSON.stringify({groups:r.groups,deltas:r.deltas,attempted:r.attempted,notRun:r.notRun}));}
