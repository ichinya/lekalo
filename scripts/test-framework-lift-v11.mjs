// Real CLI + independent Ajv gate. These authored records remain simulations.
import assert from 'node:assert/strict';
import fs from 'node:fs';import {createRequire} from 'node:module';import {spawnSync} from 'node:child_process';
import {tmpdir} from 'node:os';import {join} from 'node:path';import {fileURLToPath} from 'node:url';
import {canonical,digest} from './lib/framework-lift-executor.mjs';
const repo=fileURLToPath(new URL('../',import.meta.url)),require=createRequire(import.meta.url);
assert.equal(require('ajv/package.json').version,'8.20.0');const Ajv=require('ajv/dist/2020').default,ajv=new Ajv({strict:true,allErrors:true});
const validators=Object.fromEntries(['baseline','task','campaign','arm','result'].map(f=>[f,ajv.compile(JSON.parse(fs.readFileSync(join(repo,'contracts/framework-lift-'+f+'.schema.v1.1.0.json'),'utf8')))]));
const read=p=>JSON.parse(fs.readFileSync(join(repo,p),'utf8')),fixture=(f,n)=>read('tests/fixtures/framework-lift-'+f+'/golden/'+n+'.json');
const upgrade=v=>JSON.parse(JSON.stringify(v).replaceAll('0.6.4','1.1.0'));
const approve=v=>{const b=structuredClone(v);delete b.approval;v.approval.contentDigest=digest(b);return v;};
const temp=fs.mkdtempSync(join(tmpdir(),'lekalo-evaluation-v11-')),binary=process.env.LEKALO_BIN??join(repo,'target/debug/lekalo'+(process.platform==='win32'?'.exe':''));let checks=0;
function write(n,v){const p=join(temp,n+'.json');fs.writeFileSync(p,canonical(v));return p;}
function run(args,exit=0){const r=spawnSync(binary,['--json','evaluation',...args],{encoding:'utf8',timeout:30000,maxBuffer:8*1024*1024});assert.equal(r.status,exit,r.stdout+r.stderr);checks++;return JSON.parse(exit===0?r.stdout:r.stderr);}
function validate(f,v,ok=true){assert.equal(!!validators[f](v),ok,JSON.stringify(validators[f].errors));return run(['validate','--family',f,'--input',write('validate',v)],ok?0:1);}
try{
 const b=upgrade(fixture('baseline','approved'));b.pilot='public-synthetic-observed';b.revision={state:'unknown'};b.files[0].path='.application.txt';approve(b);
 const t=upgrade(fixture('task','priority'));t.pilot=b.pilot;t.baselineRef.digest=digest(b);t.limits={maxTokens:{state:'unbounded'},maxToolCalls:{state:'unbounded'},deadlineMs:120000,maxFixCycles:{state:'unbounded'},maxProviderRetries:0};
 const c=upgrade(fixture('campaign','scheduled'));c.baselineRef.digest=digest(b);c.taskRef.digest=digest(t);c.profile.limits=structuredClone(t.limits);c.profile.pricing={state:'unknown'};c.profile.serverModelRevision={state:'unknown'};c.profile.measurementRecipe='framework-lift-agent-window-1';for(const k of ['temperatureMicros','topPMicros','maxOutputTokens'])c.profile.sampling[k]={state:'unknown'};c.provenance={kind:'post-run-import',sourceCampaignDigest:digest(fixture('campaign','scheduled')),archiveManifestDigest:digest(b),translator:structuredClone(c.profile.harness)};approve(c);
 const a=upgrade(fixture('arm','b-neutral'));a.campaignRef.digest=digest(c);a.taskRef.digest=digest(t);a.baselineRef.digest=digest(b);a.profileDigest=digest(c.profile);a.applicationDigest=b.approval.contentDigest;a.metrics.costMicros={state:'unknown'};a.metrics.filesRead={state:'unknown'};a.metrics.sourceFilesReadLowerBound={state:'known',value:9};a.metrics.sourceMarkerBytes={state:'known',value:4005};a.measurementSources=a.measurementSources.filter(s=>!['costMicros','filesRead'].includes(s.metric));for(const metric of ['sourceFilesReadLowerBound','sourceMarkerBytes'])a.measurementSources.push({...structuredClone(a.measurementSources[0]),metric});
 for(const [family,value]of Object.entries({baseline:b,task:t,campaign:c,arm:a}))validate(family,value);
 const common=()=>['--baseline',write('baseline',b),'--task',write('task',t),'--campaign',write('campaign',c)];
 const workspace=join(temp,'workspace');fs.mkdirSync(workspace);fs.copyFileSync(join(repo,'tests/fixtures/framework-lift-baseline/workspace/application.txt'),join(workspace,'.application.txt'));
 run(['preflight',...common(),'--workspace',workspace]);
 const admitted=run(['record-arm',...common(),'--input',write('arm',a)]);assert.deepEqual(admitted,a);
 const compare=()=>run(['compare',...common(),'--arm',write('arm',a),'--consumer-alias','public-synthetic-test']);
 const result=compare();assert.ok(validators.result(result),JSON.stringify(validators.result.errors));assert.equal(result.profileCompleteness,'partial');assert.deepEqual(result.campaignProvenance,c.provenance);assert.equal(result.evidenceStatus,'recorded-simulation');assert.equal(result.exportDisposition,'local-private');assert.equal(result.rows.length,4);assert.equal(result.rows.filter(r=>r.status==='not-started').length,3);assert.equal(result.rows.find(r=>r.status!=='not-started').status,'success');assert.ok(result.summaries.every(s=>s.totalCostMicros.state==='unknown'));assert.deepEqual(compare(),result);validate('result',result);
 // Unknown pricing cannot admit a fabricated known charge, even zero.
 const charged=structuredClone(a);charged.metrics.costMicros={state:'known',value:0};charged.measurementSources.push({...structuredClone(charged.measurementSources[0]),metric:'costMicros'});
 assert.ok(run(['record-arm',...common(),'--input',write('charged',charged)],1).diagnostics.some(d=>d.id==='evaluation.metric-inconsistent'));
 const count=structuredClone(a);count.metrics.filesRead={state:'known',value:8};count.measurementSources.push({...structuredClone(count.measurementSources[0]),metric:'filesRead'});
 assert.ok(run(['record-arm',...common(),'--input',write('count',count)],1).diagnostics.some(d=>d.id==='evaluation.metric-inconsistent'));
 // Known limits still reject: unavailable policy cannot erase an observed breach.
 const prior=structuredClone(t.limits);t.limits.maxTokens=1;c.profile.limits=structuredClone(t.limits);c.taskRef.digest=digest(t);approve(c);a.taskRef.digest=digest(t);a.campaignRef.digest=digest(c);a.profileDigest=digest(c.profile);assert.equal(compare().rows.find(r=>r.status!=='not-started').status,'task');
 t.limits=prior;t.limits.maxTokens={state:'unknown'};c.profile.limits=structuredClone(t.limits);c.taskRef.digest=digest(t);approve(c);a.taskRef.digest=digest(t);a.campaignRef.digest=digest(c);a.profileDigest=digest(c.profile);assert.equal(compare().rows.find(r=>r.status!=='not-started').status,'unsupported');
 // Private brownfield cannot be relabelled as public by changing network alone.
 b.pilot='brownfield-observed';approve(b);t.pilot=b.pilot;t.baselineRef.digest=digest(b);c.baselineRef.digest=digest(b);c.taskRef.digest=digest(t);approve(c);assert.ok(run(['preflight',...common(),'--workspace',workspace],1).diagnostics.some(d=>d.id==='evaluation.private-egress-denied'));
 c.profile.network='local-only';c.profile.telemetry='unknown';approve(c);assert.ok(run(['preflight',...common(),'--workspace',workspace],1).diagnostics.some(d=>d.id==='evaluation.private-egress-denied'));
 // State objects remain closed in both schema implementations.
 const bad=structuredClone(c);bad.profile.sampling.topPMicros={state:'unknown',value:0};approve(bad);validate('campaign',bad,false);
 const altered=structuredClone(result);altered.pricing={state:'withheld'};assert.ok(validators.result(altered));assert.ok(run(['validate','--family','result','--input',write('altered',altered)],1).diagnostics.some(d=>d.id==='evaluation.metric-inconsistent'));
 console.log(JSON.stringify({ok:true,gate:'framework-lift-v11',checks,origin:'recorded-simulation'}));
}finally{fs.rmSync(temp,{recursive:true,force:true});}
