#!/usr/bin/env node
// Bounded compatibility projection into frozen #121/#102 contracts; no model calls.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {createHash} from 'node:crypto';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
import {canonical,digest} from '../lib/framework-lift-executor.mjs';
const repo=fileURLToPath(new URL('../../',import.meta.url)),sha=b=>createHash('sha256').update(b).digest('hex');
const unknown=()=>({state:'unknown'}),known=value=>({state:'known',value});
export const historyMetricMap={durationMs:'durationMs',filesRead:'filesRead',filesChanged:'filesChanged',toolCalls:'toolCalls',retryCount:'retryCount',replanCount:'replanCount',inputTokens:'tokens.input',outputTokens:'tokens.output',totalTokens:'tokens.total'};

/** Values are copied from admitted arms, never invented from missing fields. */
export function projectObservation(arm,row,campaign,timestamp){
 assert.equal(arm.schemaVersion,'lekalo/framework-lift-arm/v1.1.0');
 assert.deepEqual(row.armDigest,known(digest(arm)));assert.equal(arm.profileDigest,digest(campaign.profile));
 assert.deepEqual(arm.campaignRef,{id:campaign.campaignId,digest:digest(campaign)});
 assert.deepEqual(row.slot,arm.slot);assert.deepEqual(row.metrics,arm.metrics);
 assert.ok(['success','infrastructure'].includes(row.status),'this read-once projection does not reinterpret other failure classes');
 const armPin=digest(arm),metrics={},measurementSources=[];
 for(const [source,path]of Object.entries(historyMetricMap)){
  const state=arm.metrics[source];assert.ok(state&&['known','unknown','withheld','unsupported'].includes(state.state));
  const parts=path.split('.');let target=metrics;for(const p of parts.slice(0,-1))target=target[p]??={};target[parts.at(-1)]=structuredClone(state);
  if(state.state==='known')measurementSources.push({field:'metrics.'+path,sourceKind:'derived',sourceId:'admitted-arm-'+armPin.slice(7),sourceVersion:known('1.1.0')});
 }
 // #121 0.4's reasoning leaf is disjoint/additive; this campaign reports a
 // subset of output. Preserve the parent arm rather than change token totals.
 metrics.tokens.reasoning={state:'unsupported'};
 const completed=row.status==='success';
 assert.equal(row.verifiedSuccess,completed);
 const assertions=arm.assertions.map(a=>({assertionId:a.id,subjectSemanticId:null,kind:'gate',outcome:a.outcome==='pass'?'pass':a.outcome==='infrastructure'?'infrastructure':a.outcome==='unsupported'?'unsupported':'fail',evidenceRef:null}));
 assert.ok(!assertions.some(a=>a.assertionId==='executor-completion'));assertions.push({assertionId:'executor-completion',subjectSemanticId:null,kind:'gate',outcome:completed?'pass':'infrastructure',evidenceRef:null});
 return {schema_version:'lekalo/run-observation/v0.4.0',identity:'dev.lekalo.run-observation@0.4.0',runId:armPin.slice(7,39),timestamp,pilot:{mode:'brownfield',scopeState:'observed'},operation:{kind:'evaluation',affectedSemanticIds:[]},status:{outcome:completed?'pass':'infrastructure',coverageState:completed?'complete':'incomplete'},provenance:{git:{},model:{},lock:{},core:{},adapters:[],profile:{id:known(campaign.campaignId),version:unknown(),digest:known(digest(campaign.profile))},harness:{id:known(campaign.profile.executor.id),version:unknown(),modelId:known(campaign.profile.model.id),modelRevision:unknown()}},dataSensitivity:'internal',metrics,measurementSources,testGateSummaries:[],diagnostics:[],assertions:{rows:assertions},repeatParentRunId:null};
}

function verifiedFiles(root,pin){
 assert.match(pin,/^[0-9a-f]{64}$/);const bytes=fs.readFileSync(join(root,'SHA256-MANIFEST.json'));assert.equal(sha(bytes),pin,'admission archive manifest drift');const manifest=JSON.parse(bytes),files=new Map();assert.equal(manifest.algorithm,'sha256');
 for(const entry of manifest.files){assert.ok(entry.path.split('/').every(p=>p&&p!=='.'&&p!=='..'&&!p.includes('\\')&&!p.includes(':')));let path=root;for(const p of entry.path.split('/')){path=join(path,p);assert.ok(!fs.lstatSync(path).isSymbolicLink());}const data=fs.readFileSync(path);assert.equal(data.length,entry.bytes);assert.equal(sha(data),entry.sha256,'admission evidence drift');assert.ok(!files.has(entry.path));files.set(entry.path,data);}
 return files;
}

export function previewHistory(archive,pin,original,out,binary=process.env.LEKALO_BIN??join(repo,'target/debug/lekalo'+(process.platform==='win32'?'.exe':''))){
 assert.ok(!fs.existsSync(out),'output project must be new');
 const norm=p=>{const s=p.replaceAll('\\','/');return process.platform==='win32'?s.toLowerCase():s;};let parent=resolve(out),tail=[];
 while(!fs.existsSync(parent)){tail.unshift(parent.slice(dirname(parent).length+1));parent=dirname(parent);}
 const physical=norm(resolve(fs.realpathSync(parent),...tail)),source=norm(fs.realpathSync(archive));assert.ok(physical!==source&&!physical.startsWith(source+'/'),'output must stay outside evidence archive');
 const files=verifiedFiles(archive,pin),read=p=>JSON.parse(files.get(p)??assert.fail('unmanifested input '+p));
 const admitted=read('admitted/admission.json');assert.equal(admitted.format,'lekalo-167-post-run-admission/1');
 const originalPhysical=norm(fs.realpathSync(original));assert.ok(physical!==originalPhysical&&!physical.startsWith(originalPhysical+'/'),'output must stay outside original archive');
 const originals=verifiedFiles(original,admitted.archiveManifestSha256);
 assert.equal(sha(originals.get('campaign/campaign.json')),admitted.sourceCampaignSha256);
 const originalResults=JSON.parse(originals.get('campaign/results.json')).results;
 fs.mkdirSync(out,{recursive:true});const receipts=[];
 const write=(name,v)=>{const p=join(out,name);fs.writeFileSync(p,typeof v==='string'?v:canonical(v),{flag:'wx'});return p;};
 function cli(args,input,working=out,expected=0){const r=spawnSync(binary,['--json',...args,'--project','.'],{cwd:working,input:input===undefined?undefined:canonical(input),encoding:'utf8',timeout:30000,maxBuffer:8*1024*1024});receipts.push({operation:args.slice(0,2),rehearsal:working!==out,exit:r.status,stdoutSha256:sha(r.stdout??''),stderrSha256:sha(r.stderr??'')});assert.equal(r.status,expected,r.stdout+r.stderr);return JSON.parse(r.stdout.trim()||r.stderr.trim());}
 cli(['history','init']);const scope=cli(['history','scope','create']).result.tenantScopeId;
 write('scope.local.json',{tenantScopeId:scope});
 const links=[],trials=[];let input=0,output=0,completed=0;
 for(const p of admitted.profiles){
  const campaign=read('admitted/'+p.profile+'-campaign.json'),comparison=read('admitted/'+p.profile+'-result.json');
  assert.equal(digest(comparison),p.resultDigest);assert.equal(comparison.evidenceStatus,'recorded-unverified');
  for(const row of comparison.rows){
   assert.equal(row.armDigest.state,'known');const entry=[...files.entries()].find(([name,bytes])=>name.startsWith('admitted/')&&name.endsWith('-arm.json')&&'sha256:'+sha(bytes)===row.armDigest.value);
   assert.ok(entry,'missing digest-bound admitted arm');const arm=JSON.parse(entry[1]);assert.equal(digest(arm),row.armDigest.value);
   const originalRun=originalResults.find(r=>r.id===arm.runId);assert.ok(originalRun,'no original timestamp receipt');
   const observation=projectObservation(arm,row,campaign,originalRun.startedAt);
   const recorded=cli(['history','record','--input','-','--scope',scope],observation);
   const shown=cli(['history','show',observation.runId,'--scope',scope]).result;
   assert.deepEqual(shown.record.metrics.tokens,observation.metrics.tokens);
   for(const key of ['durationMs','filesRead','filesChanged','toolCalls','retryCount','replanCount'])assert.deepEqual(shown.record.metrics[key],observation.metrics[key]);
   assert.equal(shown.record.status.outcome,observation.status.outcome);assert.equal(shown.record.timestamp,observation.timestamp);
   input+=shown.record.metrics.tokens.input.value;output+=shown.record.metrics.tokens.output.value;
   completed+=Number(row.status==='success');
   links.push({armDigest:row.armDigest.value,historyRunId:observation.runId,historyRecordDigest:recorded.result.recordDigest,omittedMetrics:Object.keys(arm.metrics).filter(k=>!Object.hasOwn(historyMetricMap,k))});
   write(observation.runId+'.observation.json',observation);
   trials.push({unit:p.profile+'-pair-'+arm.slot.repetition,arm:arm.slot.arm==='A'?'baseline':'lekalo-assisted',runId:observation.runId,requiredAssertions:observation.assertions.rows.map(r=>r.assertionId)});
  }
 }
 assert.equal(input+output,admitted.parity.totalTokens);assert.equal(links.length,admitted.recordedArms);assert.equal(completed,admitted.parity.strictSuccesses);
 const selection={schema_version:'lekalo/evaluation-export-input/v0.6.4',identity:'dev.lekalo.evaluation-export-input@0.6.4',protocol:'framework-lift-paired-trials/1',approved:true,trials};write('evaluation.json',selection);
 const db=join(out,'.lekalo/history/store.sqlite'),before=sha(fs.readFileSync(db));
 const preview=cli(['metrics','export','--evaluation','evaluation.json','--scope',scope,'--dry-run']);
 const repeated=cli(['metrics','export','--evaluation','evaluation.json','--scope',scope,'--dry-run']);assert.equal(repeated.payload,preview.payload);assert.equal(repeated.previewDigest,preview.previewDigest);
 assert.equal(sha(fs.readFileSync(db)),before,'privacy dry-run mutated history');assert.equal(preview.written,false);assert.equal(preview.status,'blocked');
 assert.equal(fs.existsSync(join(out,'.lekalo/privacy')),false,'dry-run created publication state');
 const payload=JSON.parse(preview.payload);assert.equal(payload.cohorts.length,4);
 for(const c of payload.cohorts){assert.equal(c.sample.state,'withheld');assert.equal(c.verifiedSuccess.state,'withheld');for(const metric of Object.values(c.metrics)){assert.equal(metric.sum.state,'withheld');assert.equal(metric.mean.state,'withheld');}}
 assert.equal(preview.decisionTemplate.derivedArtifact.aggregationDecision.decisionRef,null);
 for(const word of ['runId','tenantScopeId','admitted-arm-','recorded-unverified','devin','codex','gpt-6','swe-2','sourceRef','prompt','C:\\'])assert.ok(!preview.payload.includes(word),'source identity entered public payload');
 // Exercise deletion in a disposable copy only; keep all eight original records.
 const rehearsal=join(out,'invalidation-rehearsal');fs.mkdirSync(rehearsal);
 fs.cpSync(join(out,'.lekalo'),join(rehearsal,'.lekalo'),{recursive:true});fs.copyFileSync(join(out,'evaluation.json'),join(rehearsal,'evaluation.json'));
 cli(['history','delete',trials[0].runId,'--scope',scope,'--apply'],undefined,rehearsal);
 const refused=cli(['metrics','export','--evaluation','evaluation.json','--scope',scope,'--dry-run'],undefined,rehearsal,3);
 assert.ok(refused.diagnostics.some(d=>d.id==='metrics-export.source-invalidated'));
 assert.equal(sha(fs.readFileSync(db)),before,'rehearsal mutated original history');
 assert.equal(fs.existsSync(join(rehearsal,'.lekalo/privacy')),false);
 write('privacy-preview.local.json',preview);write('source-links.local.json',links);write('cli-receipts.local.json',receipts);
 const report={format:'lekalo-167-history-privacy-preview/1',sourceAdmissionManifest:pin,sourceOriginalManifest:admitted.archiveManifestSha256,sourceAdmissionCommit:JSON.parse(files.get('verification.json')).sourceCommit,projectionSha256:'sha256:'+sha(fs.readFileSync(fileURLToPath(import.meta.url))),binarySha256:'sha256:'+sha(fs.readFileSync(binary)),recordedRuns:links.length,completed,historyTokenParity:{input,output,total:input+output},publicPayloadSha256:preview.payloadDigest,previewSha256:preview.previewDigest,status:preview.status,written:preview.written,minimumSamples:5,cohorts:payload.cohorts.length,samplesPerArmProfile:2,numericPublication:'withheld',sourceInvalidation:{disposableCopy:true,missingSourceRefused:true,originalEightPreserved:true},authorization:'not supplied; non-authorizing decision template only',sourceSensitivity:'internal; never silently declassified',limits:['This is a bounded compatibility projection, not full 1.1.0 record ingestion.','Original admitted arms retain cached input, subset reasoning, fix cycles, source lower bounds and evidence refs; legacy reasoning is unsupported because its contract is additive.','Two trials per arm/profile are below the unchanged minimum five; no cohort pooling, duplicated samples or threshold reduction.','Aggregation/declassification approval is not minted; no confirmation, publication or network upload occurred.']};
 write('privacy-preview-report.json',report);return report;
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){const [archive,pin,original,out]=process.argv.slice(2);assert.ok(archive&&pin&&original&&out,'usage: ADMISSION_ARCHIVE MANIFEST_SHA256 ORIGINAL_ARCHIVE NEW_PROJECT');console.log(JSON.stringify(previewHistory(resolve(archive),pin,resolve(original),resolve(out))));}
