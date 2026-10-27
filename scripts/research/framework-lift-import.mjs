#!/usr/bin/env node
// Offline post-run translation. Never launches an agent or rewrites source receipts.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {dirname,join,resolve,relative} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {canonical,digest} from '../lib/framework-lift-executor.mjs';
import {publicResult} from './framework-lift-public-result.mjs';
const repo=fileURLToPath(new URL('../../',import.meta.url));
const sha=b=>createHash('sha256').update(b).digest('hex');
const rawDigest=p=>'sha256:'+sha(fs.readFileSync(p));
const known=value=>({state:'known',value}),unknown=()=>({state:'unknown'});
const component=(id,revision,digest)=>({id,revision,digest});
const approve=v=>{const b=structuredClone(v);delete b.approval;v.approval.contentDigest=digest(b);return v;};
const envelope=f=>({schemaVersion:'lekalo/framework-lift-'+f+'/v1.1.0',identity:'dev.lekalo.framework-lift-'+f+'@1.1.0'});

export function importCampaign(archive,manifestPin,out,binary=process.env.LEKALO_BIN??join(repo,'target/debug/lekalo'+(process.platform==='win32'?'.exe':''))){
 assert.ok(!fs.existsSync(out),'do not overwrite an existing import');
 const normalized=p=>{const s=p.replaceAll('\\','/');return process.platform==='win32'?s.toLowerCase():s;};
 const archivePath=normalized(fs.realpathSync(archive));let parent=resolve(out),suffix=[];
 while(!fs.existsSync(parent)){suffix.unshift(parent.slice(dirname(parent).length+1));parent=dirname(parent);}
 const outputPath=normalized(resolve(fs.realpathSync(parent),...suffix));
 assert.ok(outputPath!==archivePath&&!outputPath.startsWith(archivePath+'/'),'output must be outside archive');
 const disclosure=publicResult(archive,manifestPin),read=p=>JSON.parse(fs.readFileSync(join(archive,p),'utf8'));
 const frozen=read('campaign/campaign.json'),prep=read('campaign/preparation.json'),done=read('campaign/results.json').results;
 fs.mkdirSync(out,{recursive:true});const receipts=[];
 const write=(name,v)=>{const p=join(out,name+'.json');fs.writeFileSync(p,canonical(v),{flag:'wx'});return p;};
 function cli(args){
  const r=spawnSync(binary,['--json','evaluation',...args],{encoding:'utf8',timeout:30000,maxBuffer:8*1024*1024});
  receipts.push({operation:args[0],inputs:args.filter(p=>fs.existsSync(p)&&fs.statSync(p).isFile()).map(p=>({file:relative(out,p).replaceAll('\\','/'),sha256:rawDigest(p)})),exit:r.status,stdoutSha256:sha(r.stdout??''),stderrSha256:sha(r.stderr??'')});
  assert.equal(r.status,0,r.stdout+r.stderr);return JSON.parse(r.stdout);
 }
 const translator=component('read-once-post-run-import','v-one',rawDigest(fileURLToPath(import.meta.url)));
 const baseline=approve({...envelope('baseline'),baselineId:'public-priority-planner',pilot:'public-synthetic-observed',revision:unknown(),files:prep.nativeFiles.map(f=>({path:f.path,digest:'sha256:'+f.sha256})),dataSeedDigest:'sha256:'+frozen.oracleDigest,semanticBundleDigest:'sha256:'+frozen.modelDigest,oracleDigest:'sha256:'+frozen.oracleDigest,approval:{role:'post-run-importer',revision:1}});
 const bPath=write('baseline',baseline),bRef={id:baseline.baselineId,digest:digest(baseline)};
 cli(['validate','--family','baseline','--input',bPath]);
 const oracleNames=done[0].grade.holdout.results.map(t=>t.name);assert.ok(oracleNames.includes('existing-fields-errors'));
 const limits={maxTokens:{state:'unbounded'},maxToolCalls:{state:'unbounded'},deadlineMs:frozen.budget.perRunMs,maxFixCycles:{state:'unbounded'},maxProviderRetries:frozen.budget.maxAutomaticReruns};
 const task={...envelope('task'),taskId:'public-priority-change',taskVersion:1,taskClass:'priority',pilot:baseline.pilot,baselineRef:bRef,requestDigest:'sha256:'+frozen.taskDigest,oracleDigest:baseline.oracleDigest,requiredAssertions:['public-tests','typecheck'],regressionAssertions:['existing-fields-errors'],holdoutAssertions:oracleNames.filter(n=>n!=='existing-fields-errors'),limits};
 const tPath=write('task',task),tRef={id:task.taskId,digest:digest(task)},all=[];
 cli(['validate','--family','task','--input',tPath]);
 for(const p of frozen.profiles){
  const runtime=component('node-'+frozen.runtime.platform+'-'+frozen.runtime.architecture,'node-'+frozen.runtime.node.replaceAll('.','-').replace(/^v/,''),'sha256:'+frozen.executables.node);
  const profile={executor:component(p.cli+'-cli',p.cli+'-'+p.version.replaceAll('.','-'),'sha256:'+frozen.executables[p.cli]),provider:component(p.cli+'-hosted','requested-alias','sha256:'+frozen.executables[p.cli]),model:component(p.model,'requested-alias',digest(p)),harness:component('read-once-runner','v-two','sha256:'+frozen.runnerDigest),runtime,sampling:{temperatureMicros:unknown(),topPMicros:unknown(),reasoning:p.reasoning??'provider-default',seed:{state:'unsupported'},maxOutputTokens:unknown()},network:'public-synthetic',telemetry:'unknown',architectureProfile:{state:'unsupported'},pricing:unknown(),limits,measurementRecipe:'framework-lift-agent-window-1',serverModelRevision:unknown()};
  const slots=frozen.slots.filter(s=>s.profile===p.id).map(s=>({arm:s.arm,pairId:'pair-'+s.repetition,repetition:s.repetition}));
  const campaign=approve({...envelope('campaign'),campaignId:'read-once-'+p.id,taskRef:tRef,baselineRef:bRef,profile,slots,randomizationSeed:{state:'unsupported'},analysis:'intent-to-run-wilson-95-paired-counts-1',provenance:{kind:'post-run-import',sourceCampaignDigest:'sha256:'+disclosure.archive.campaignSha256,archiveManifestDigest:'sha256:'+manifestPin,translator},approval:{role:'post-run-importer',revision:1}});
  const cPath=write(p.id+'-campaign',campaign),common=['--baseline',bPath,'--task',tPath,'--campaign',cPath],paths=[];
  cli(['preflight',...common,'--workspace',join(archive,'campaign/baseline')]);
  for(const r of done.filter(r=>r.profile===p.id)){
   const dir='campaign/runs/'+r.id+'/',raw=path=>rawDigest(join(archive,dir+path)),s=disclosure.rows.find(s=>s.id===r.id);
   const events=[],providerFile=p.cli==='devin'?'trajectory.json':'stdout.jsonl';
   const add=(kind,tool,evidenceDigest)=>events.push({sequence:events.length,kind,tool,evidenceDigest});
   const calls=p.cli==='devin'?read(dir+'trajectory.json').steps.flatMap(s=>s.tool_calls??[]):fs.readFileSync(join(archive,dir+'stdout.jsonl'),'utf8').split(/\r?\n/).filter(Boolean).map(l=>JSON.parse(l)).filter(e=>e.type==='item.completed'&&['command_execution','file_change','mcp_tool_call','web_search'].includes(e.item?.type));
   assert.equal(calls.length,s.toolCalls,'raw tool count drift');
   for(const call of calls)add('tool-call',JSON.stringify(call).includes('tools/semantic.mjs')?'lekalo':'native',digest(call));
   if(s.complete)add('submit','none',raw('result.json'));
   const candidateDigest=raw('candidate-tree.json'),assertions=r.grade.holdout.results.map(a=>({id:a.name,outcome:a.pass?'pass':'fail',candidateDigest,oracleDigest:baseline.oracleDigest,receiptDigest:raw('holdout.stdout.txt')}));
   assertions.push(...[['public-tests',r.grade.publicExit,'public-tests.stdout.txt'],['typecheck',r.grade.typecheckExit,'typecheck.stdout.txt']].map(([id,exit,file])=>({id,outcome:exit===0?'pass':'fail',candidateDigest,oracleDigest:baseline.oracleDigest,receiptDigest:raw(file)})));
   const schema=JSON.parse(fs.readFileSync(join(repo,'contracts/framework-lift-arm.schema.v1.1.0.json'),'utf8')),metrics=Object.fromEntries(schema.$defs.metrics.required.map(k=>[k,unknown()])),sources=[];
   const measured=(key,value,receipt)=>{metrics[key]=known(value);sources.push({metric:key,component:translator,evidenceDigest:raw(receipt)});};
   for(const [key,value]of [['inputTokens',s.input],['outputTokens',s.output],['totalTokens',s.total],['cachedInputTokens',s.cachedInput],['toolCalls',s.toolCalls]])measured(key,value,providerFile);
   if(s.reasoningOutput.state==='known')measured('reasoningTokens',s.reasoningOutput.value,providerFile);
   measured('durationMs',s.agentDurationMs,'result.json');measured('humanInterventions',r.humanInterventions,'result.json');if(r.postSubmissionFixCycles!==null)measured('fixCycles',r.postSubmissionFixCycles,'result.json');
   measured('escapedRegressions',r.grade.holdout.total-r.grade.holdout.passed,'holdout.stdout.txt');measured('sourceFilesReadLowerBound',s.sourceFilesLowerBound,'marker-interpretation-audit.json');measured('sourceMarkerBytes',s.sourceMarkerBytes,'marker-interpretation-audit.json');
   const arm={...envelope('arm'),runId:r.id,campaignRef:{id:campaign.campaignId,digest:digest(campaign)},taskRef:tRef,baselineRef:bRef,slot:{arm:r.arm,pairId:'pair-'+r.repetition,repetition:r.repetition},attempt:0,origin:'recorded-external',profileDigest:digest(profile),applicationDigest:baseline.approval.contentDigest,policy:{lekalo:r.arm==='B',semanticExposure:r.arm==='B',network:profile.network},candidateDigest,coverage:'complete',assertions,failures:s.complete?[]:[{stage:'agent',class:'infrastructure',reason:s.confirmationRejected?'confirmation-refused':'incomplete-response',evidenceDigest:raw('result.json')}],events,metrics,measurementSources:sources,optionalJudge:{state:'unsupported'}};
   const aPath=write(r.id+'-arm',arm);assert.deepEqual(cli(['record-arm',...common,'--input',aPath]),arm);paths.push(aPath);
  }
  const result=cli(['compare',...common,...paths.flatMap(p=>['--arm',p]),'--consumer-alias','public-synthetic-planner']);
  write(p.id+'-result',result);
  assert.deepEqual(cli(['compare',...common,...paths.flatMap(p=>['--arm',p]),'--consumer-alias','public-synthetic-planner']),result);
  assert.equal(result.evidenceStatus,'recorded-unverified');assert.equal(result.profileCompleteness,'partial');
  assert.ok(result.summaries.every(s=>s.totalCostMicros.state==='unknown'));assert.equal(result.rows.length,slots.length);
  const expected=disclosure.rows.filter(r=>r.profile===p.id);
  for(const row of result.rows){
   const source=expected.find(r=>r.arm===row.slot.arm&&r.repetition===row.slot.repetition);assert.equal(row.verifiedSuccess,source.success);
   for(const [metric,key]of [['inputTokens','input'],['outputTokens','output'],['totalTokens','total'],['cachedInputTokens','cachedInput']])assert.equal(row.metrics[metric].value,source[key]);
  }
  all.push({profile:p.id,resultDigest:digest(result),scheduled:result.rows.length,successes:result.summaries.reduce((n,s)=>n+s.successes,0),input:result.rows.reduce((n,r)=>n+r.metrics.inputTokens.value,0),output:result.rows.reduce((n,r)=>n+r.metrics.outputTokens.value,0),total:result.rows.reduce((n,r)=>n+r.metrics.totalTokens.value,0),evidenceStatus:result.evidenceStatus});
 }
 const report={format:'lekalo-167-post-run-admission/1',archiveManifestSha256:manifestPin,sourceCampaignSha256:disclosure.archive.campaignSha256,translatorSha256:translator.digest,binarySha256:rawDigest(binary),status:'recorded-unverified-local-private',recordedArms:done.length,profiles:all,parity:{totalTokens:all.reduce((n,r)=>n+r.total,0),strictSuccesses:all.reduce((n,r)=>n+r.successes,0)},limits:['Post-run translation, not preregistration of a new run.','Source snapshot and CLI/model aliases pinned; server model revision, provider sampling, telemetry and prices unknown.','Source filesRead remains unknown; verified src-only lower bounds have separate metrics.','No provider authentication or product privacy-export qualification. Historical 0.6.4 records unchanged.']};
 assert.equal(report.parity.totalTokens,disclosure.totals.total);assert.equal(report.parity.strictSuccesses,disclosure.totals.strictSuccesses);
 write('admission',report);write('cli-receipts',receipts);return report;
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
 const [root,pin,out]=process.argv.slice(2);assert.ok(root&&pin&&out,'usage: ARCHIVE MANIFEST_SHA256 NEW_OUTPUT_DIRECTORY');
 console.log(JSON.stringify(importCampaign(resolve(root),pin,resolve(out))));
}
