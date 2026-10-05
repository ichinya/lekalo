// Live production gate for all five #100 closed families. Fixtures are simulations.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { readFileSync,writeFileSync,existsSync,mkdtempSync,cpSync,rmSync } from 'node:fs';
import { join,resolve,dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { canonical,digest,executeArm } from './lib/framework-lift-executor.mjs';
const require=createRequire(import.meta.url),root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const args=process.argv.slice(2),families=['baseline','task','campaign','arm','result'];
assert.ok(args.length===0||(args.length===2&&args[0]==='--family'&&families.includes(args[1])),'unknown gate arguments');
const selected=args[1]??'all';
assert.equal(require('ajv/package.json').version,'8.17.1','exact Ajv required');
const Ajv=require('ajv/dist/2020').default,ajv=new Ajv({strict:true,allErrors:true});
const schemas=new Map(families.map(f=>[f,ajv.compile(JSON.parse(readFileSync(join(root,`contracts/framework-lift-${f}.schema.v0.6.4.json`))))]));
const binary=join(root,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');assert.ok(existsSync(binary),'binary-missing');
const scratch=mkdtempSync(join(tmpdir(),'lekalo-framework-lift-'));
const read=p=>JSON.parse(readFileSync(join(root,p),'utf8'));
const fixture=(family,name)=>`tests/fixtures/framework-lift-${family}/golden/${name}.json`;
const baseline=fixture('baseline','approved'),task=fixture('task','priority'),campaign=fixture('campaign','scheduled');
const common=['--baseline',join(root,baseline),'--task',join(root,task),'--campaign',join(root,campaign)];
const armPaths=['a-provider','a-retry','b-hard-fail','b-neutral'].map(name=>join(root,fixture('arm',name)));
const temp=(name,value)=>{const p=join(scratch,name);writeFileSync(p,typeof value==='string'?value:canonical(value));return p;};
let checks=0;const codes=new Set();
function run(args,{exit=0,code}={}){const p=spawnSync(binary,['--json','evaluation',...args],{cwd:root,encoding:'utf8',timeout:30000,maxBuffer:8*1024*1024});assert.equal(p.status,exit,JSON.stringify({args,stdout:p.stdout,stderr:p.stderr,error:p.error?.code}));assert.equal(exit===0?p.stderr:p.stdout,'','stream contract');const value=JSON.parse(exit===0?p.stdout:p.stderr);if(code){assert.ok(value.diagnostics.some(d=>d.id===code),JSON.stringify(value));codes.add(code);}checks++;return {value,bytes:p.stdout};}
const denied=(family,value,code='evaluation.protocol-invalid')=>run(['validate','--family',family,'--input',temp('invalid.json',value)],{exit:1,code});
const record=(value,code)=>run(['record-arm',...common,'--input',temp('arm.json',value)],code?{exit:1,code}:{});
const compare=(paths=armPaths)=>run(['compare',...common,...paths.flatMap(p=>['--arm',p]),'--consumer-alias','consumer-greenfield-one']);
try {
 for(const [family,name]of [['baseline','approved'],['task','priority'],['campaign','scheduled'],['arm','b-hard-fail'],['result','negative']]){
  const p=fixture(family,name),value=read(p);assert.ok(schemas.get(family)(value),`${family}: ${JSON.stringify(schemas.get(family).errors)}`);
  assert.deepEqual(run(['validate','--family',family,'--input',join(root,p)]).value,value);
  const unknown=structuredClone(value);unknown.rawPrompt='private';assert.equal(schemas.get(family)(unknown),false);denied(family,unknown);
 }
 for(const name of ['a-provider','a-retry','b-neutral']){const value=read(fixture('arm',name));assert.ok(schemas.get('arm')(value));assert.deepEqual(run(['validate','--family','arm','--input',join(root,fixture('arm',name))]).value,value);}
 const provenance=read('tests/fixtures/fixture-provenance.json');for(const family of families)assert.ok(provenance.families.some(f=>f.family===`framework-lift-${family}`&&f.origin==='synthetic'));
 const registry=read('contracts/diagnostic-registry.v0.6.4.json'),predecessor=read('tests/fixtures/framework-lift-baseline/registry-predecessor.json');
 assert.equal(registry.entries.length,506);assert.equal(predecessor.entries,500);assert.equal(digest(registry.entries.filter(e=>!e.id.startsWith('evaluation.'))),predecessor.digest,'all predecessor entries unchanged');
 const ci=readFileSync(join(root,'.github/workflows/ci.yml'),'utf8');const build=ci.indexOf('cargo build --workspace --locked'),gate=ci.indexOf('node scripts/test-framework-lift-contracts.mjs');assert.ok(build>=0&&gate>build,'mandatory gate follows build');assert.match(ci.slice(gate-300,gate),/LEKALO_AJV_NODE_PATH/);
 const original=readFileSync(join(root,'tests/fixtures/framework-lift-baseline/workspace/application.txt'));
 const workspace=join(scratch,'workspace');cpSync(join(root,'tests/fixtures/framework-lift-baseline/workspace'),workspace,{recursive:true});
 run(['preflight',...common,'--workspace',workspace]);writeFileSync(join(workspace,'application.txt'),'drift');run(['preflight',...common,'--workspace',workspace],{exit:1,code:'evaluation.baseline-drift'});writeFileSync(join(workspace,'application.txt'),original);
 const b=read(baseline),t=read(task),c=read(campaign),a=read(fixture('arm','b-hard-fail'));
 const bad=structuredClone(b);bad.revision='b'.repeat(40);denied('baseline',bad,'evaluation.baseline-drift');
 const mixedBase=structuredClone(b);mixedBase.files[0].path='ClassFile.vue';const {approval:mbp,...mbc}=mixedBase;mixedBase.approval.contentDigest=digest(mbc);assert.ok(schemas.get('baseline')(mixedBase));writeFileSync(join(workspace,'ClassFile.vue'),original);
 const mixedTask=structuredClone(t);mixedTask.baselineRef.digest=digest(mixedBase);const mixedCampaign=structuredClone(c);mixedCampaign.baselineRef.digest=digest(mixedBase);mixedCampaign.taskRef.digest=digest(mixedTask);const {approval:mcp,...mcc}=mixedCampaign;mixedCampaign.approval.contentDigest=digest(mcc);run(['preflight','--baseline',temp('mixed-base.json',mixedBase),'--task',temp('mixed-task.json',mixedTask),'--campaign',temp('mixed-campaign.json',mixedCampaign),'--workspace',workspace]);
 const caseAlias=structuredClone(b);caseAlias.files.push({...caseAlias.files[0],path:'Application.txt'});const {approval:cap,...cac}=caseAlias;caseAlias.approval.contentDigest=digest(cac);denied('baseline',caseAlias);
 for(const path of ['src/../Task.php','src//Task.php','CON.txt','src/file.']){const unsafe=structuredClone(b);unsafe.files[0].path=path;const {approval,...body}=unsafe;unsafe.approval.contentDigest=digest(body);denied('baseline',unsafe);}
 denied('task',canonical(t).replace('"taskVersion":1','"taskVersion":1,"taskVersion":1'));
 const nested=structuredClone(a);nested.metrics.inputTokens.extra=0;assert.equal(schemas.get('arm')(nested),false);denied('arm',nested);
 const missingSource=structuredClone(a);missingSource.measurementSources=[];denied('arm',missingSource,'evaluation.metric-inconsistent');
 const stale=structuredClone(a);stale.assertions[0].candidateDigest='sha256:'+'0'.repeat(64);record(stale,'evaluation.required-evidence-missing');
 const wrong=structuredClone(a);wrong.profileDigest='sha256:'+'0'.repeat(64);record(wrong,'evaluation.arm-incomparable');
 const leakage=read(fixture('arm','a-retry'));leakage.events.push({sequence:1,kind:'tool-call',tool:'lekalo',evidenceDigest:'sha256:'+'0'.repeat(64)});record(leakage,'evaluation.arm-incomparable');
 const subset=structuredClone(a);subset.metrics.cachedInputTokens.value=41;denied('arm',subset,'evaluation.metric-inconsistent');
 // F1: the frozen normalizer totals input + output, never cache/reasoning again.
 const tokenArm=read(fixture('arm','b-neutral'));
 const rejectTokens=value=>{
  assert.ok(schemas.get('arm')(value),'contradiction is schema-shaped');
  denied('arm',value,'evaluation.metric-inconsistent');
  record(value,'evaluation.metric-inconsistent');
  run(['compare',...common,'--arm',temp('inconsistent-tokens.json',value),'--consumer-alias','consumer-greenfield-one'],{exit:1,code:'evaluation.metric-inconsistent'});
 };
 for(const mutate of [v=>v.metrics.totalTokens.value=0,v=>v.metrics.totalTokens.value=39,v=>v.metrics.totalTokens.value=51,v=>v.metrics.inputTokens.value=t.limits.maxTokens+1,v=>v.metrics.outputTokens.value=t.limits.maxTokens+1]){
  const inconsistent=structuredClone(tokenArm);mutate(inconsistent);rejectTokens(inconsistent);
 }
 const partialBelow=structuredClone(tokenArm);partialBelow.metrics.outputTokens={state:'unknown'};partialBelow.measurementSources=partialBelow.measurementSources.filter(s=>s.metric!=='outputTokens');partialBelow.metrics.totalTokens.value=39;rejectTokens(partialBelow);
 for(const missingState of ['unknown','unsupported','withheld']){
  for(const [input,output]of [[t.limits.maxTokens+1,10],[40,t.limits.maxTokens+1],[6000,6000],[40,10]]){
   const partial=structuredClone(tokenArm);partial.metrics.totalTokens={state:missingState};partial.measurementSources=partial.measurementSources.filter(s=>s.metric!=='totalTokens');partial.metrics.inputTokens.value=input;partial.metrics.outputTokens.value=output;
   record(partial);const row=compare([temp('partial-tokens.json',partial)]).value.rows.find(r=>r.slot.arm==='B'&&r.slot.pairId==='pair-two');
   assert.equal(row.status,input+output>t.limits.maxTokens?'task':'unsupported');assert.equal(row.verifiedSuccess,false);assert.deepEqual(row.metrics.totalTokens,{state:missingState});
  }
 }
 const inclusive=structuredClone(tokenArm);inclusive.metrics.inputTokens.value=t.limits.maxTokens-10;inclusive.metrics.totalTokens.value=t.limits.maxTokens;
 assert.equal(compare([temp('inclusive-token-cap.json',inclusive)]).value.rows.find(r=>r.slot.arm==='B'&&r.slot.pairId==='pair-two').verifiedSuccess,true);
 const subsets=structuredClone(tokenArm);subsets.metrics.reasoningTokens={state:'known',value:5};subsets.measurementSources.push({...subsets.measurementSources[0],metric:'reasoningTokens'});
 assert.equal(compare([temp('token-subsets.json',subsets)]).value.rows.find(r=>r.slot.arm==='B'&&r.slot.pairId==='pair-two').verifiedSuccess,true,'cache and reasoning are not charged twice');
 const forgedTokens=read(fixture('result','negative'));forgedTokens.rows[0].metrics.totalTokens.value=0;denied('result',forgedTokens,'evaluation.metric-inconsistent');
 const string=structuredClone(a);string.failures=[{stage:'provider',class:'provider',reason:'https://private.example',evidenceDigest:'sha256:'+'0'.repeat(64)}];denied('arm',string);
 const unpaired=structuredClone(c);unpaired.slots[0].arm='B';const {approval,...content}=unpaired;unpaired.approval.contentDigest=digest(content);denied('campaign',unpaired);
 const privateBase=structuredClone(b);privateBase.pilot='brownfield-observed';const {approval:bp,...bc}=privateBase;privateBase.approval.contentDigest=digest(bc);
 const privateTask=structuredClone(t);privateTask.pilot=privateBase.pilot;privateTask.baselineRef.digest=digest(privateBase);
 const privateCampaign=structuredClone(c);privateCampaign.baselineRef.digest=digest(privateBase);privateCampaign.taskRef.digest=digest(privateTask);const {approval:cp,...cc}=privateCampaign;privateCampaign.approval.contentDigest=digest(cc);
 run(['preflight','--baseline',temp('private-base.json',privateBase),'--task',temp('private-task.json',privateTask),'--campaign',temp('private-campaign.json',privateCampaign),'--workspace',workspace],{exit:1,code:'evaluation.private-egress-denied'});
 let privateCalls=0;const privateCallback=async()=>{privateCalls++;};const privateInput={binary,baseline:join(scratch,'private-base.json'),task:join(scratch,'private-task.json'),campaign:join(scratch,'private-campaign.json'),workspace,slot:privateCampaign.slots[0],executor:privateCallback};await assert.rejects(executeArm(privateInput));assert.equal(privateCalls,0,'private policy refuses before callback');
 privateCampaign.profile.network='local-only';const {approval:localApproval,...localContent}=privateCampaign;privateCampaign.approval.contentDigest=digest(localContent);writeFileSync(privateInput.campaign,canonical(privateCampaign));await executeArm(privateInput);assert.equal(privateCalls,1,'qualified local-only declaration is admitted');
 const first=compare(),second=compare();assert.equal(first.bytes,second.bytes);assert.deepEqual(first.value,read(fixture('result','negative')));assert.ok(schemas.get('result')(first.value));
 assert.equal(first.value.evidenceStatus,'recorded-simulation');assert.equal(first.value.exportDisposition,'local-private');assert.equal(first.value.rows.length,5);assert.equal(first.value.rows.find(r=>r.slot.pairId==='pair-two'&&r.slot.arm==='A').status,'not-started');
 const summaryA=first.value.summaries[0],summaryB=first.value.summaries[1];assert.equal(summaryA.successes,1);assert.equal(summaryB.successes,1);assert.deepEqual(summaryA.costPerSuccess.value.value,{numerator:300,denominator:1});assert.deepEqual(summaryB.costPerSuccess.value.value,{numerator:400,denominator:1});assert.deepEqual(first.value.liftPercentagePoints,{numerator:0,denominator:2});assert.equal(first.value.rows.find(r=>r.slot.arm==='B'&&r.slot.pairId==='pair-one').verifiedSuccess,false,'judge 100 cannot hide regression');
 assert.deepEqual(summaryA.successInterval,{method:'wilson-95',unit:'parts-per-million',lower:94531,upper:905469});
 assert.deepEqual(first.value.uncertainty,{method:'attrition-bounds',minimumLift:{numerator:-100,denominator:2},maximumLift:{numerator:0,denominator:2}});
 for(const mutate of [v=>v.summaries[0].successes++,v=>v.summaries[1].totalCostMicros.value++,v=>v.paired.aOnly++,v=>v.uncertainty.minimumLift.numerator++,v=>v.summaries[0].successInterval.lower++]){const forged=structuredClone(first.value);mutate(forged);denied('result',forged,'evaluation.metric-inconsistent');}
 compare([]);const duplicate=()=>compare([armPaths[0],armPaths[0]]);assert.throws(duplicate);
 const unknownCost=structuredClone(a);unknownCost.metrics.costMicros={state:'unknown'};unknownCost.measurementSources=unknownCost.measurementSources.filter(s=>s.metric!=='costMicros');const unknown=compare([armPaths[0],armPaths[1],temp('unknown-cost.json',unknownCost),armPaths[3]]).value;assert.equal(unknown.summaries[1].costPerSuccess.reason,'cost-incomplete');
 const noSuccess=compare([armPaths[2]]).value;assert.equal(noSuccess.summaries[1].costPerSuccess.reason,'no-success');
 const missed=structuredClone(a);missed.assertions=[];const incomplete=compare([temp('missed.json',missed)]).value;assert.equal(incomplete.rows.find(r=>r.slot.arm==='B'&&r.slot.pairId==='pair-one').verifiedSuccess,false);
 const unknownCap=read(fixture('arm','b-neutral'));unknownCap.metrics.totalTokens={state:'unknown'};unknownCap.measurementSources=unknownCap.measurementSources.filter(s=>s.metric!=='totalTokens');assert.equal(compare([temp('unknown-cap.json',unknownCap)]).value.rows.find(r=>r.slot.arm==='B'&&r.slot.pairId==='pair-two').status,'unsupported');
 const providerAndTask=structuredClone(a);providerAndTask.failures=read(fixture('arm','a-provider')).failures;assert.equal(compare([temp('provider-and-task.json',providerAndTask)]).value.rows.find(r=>r.slot.arm==='B'&&r.slot.pairId==='pair-one').status,'task');
 // F2: an infrastructure assertion is already evidence of that failure class.
 const infrastructureAssertion=read(fixture('arm','b-neutral'));infrastructureAssertion.assertions[0].outcome='infrastructure';
 assert.deepEqual(infrastructureAssertion.failures,[]);record(infrastructureAssertion);
 const infrastructureResult=compare([temp('infrastructure-assertion.json',infrastructureAssertion)]).value;
 const infrastructureRow=infrastructureResult.rows.find(r=>r.slot.arm==='B'&&r.slot.pairId==='pair-two');
 assert.equal(infrastructureRow.status,'infrastructure');assert.equal(infrastructureRow.verifiedSuccess,false);assert.equal(infrastructureRow.firstPassSuccess,false);assert.equal(infrastructureResult.paired.incomplete,2);
 const hardAndInfrastructure=structuredClone(a);hardAndInfrastructure.assertions[1].outcome='infrastructure';record(hardAndInfrastructure);
 assert.equal(compare([temp('hard-and-infrastructure.json',hardAndInfrastructure)]).value.rows.find(r=>r.slot.arm==='B'&&r.slot.pairId==='pair-one').status,'task','hard regression takes priority over verifier infrastructure');
 for(const [failureClass,expected]of [['provider','provider'],['custody-security','custody-security'],['unsupported','infrastructure'],['interrupted','infrastructure']]){
  const combined=structuredClone(infrastructureAssertion);combined.failures=[{stage:'verifier',class:failureClass,reason:'synthetic-failure',evidenceDigest:digest({failureClass})}];
  assert.equal(compare([temp('infrastructure-precedence.json',combined)]).value.rows.find(r=>r.slot.arm==='B'&&r.slot.pairId==='pair-two').status,expected);
 }
 const afterTerminal=structuredClone(a);afterTerminal.attempt=1;afterTerminal.runId='after-terminal';run(['compare',...common,'--arm',armPaths[2],'--arm',temp('after-terminal.json',afterTerminal),'--consumer-alias','consumer-greenfield-one'],{exit:1,code:'evaluation.protocol-invalid'});
 const external=read(fixture('arm','b-neutral'));external.origin='recorded-external';assert.equal(compare([temp('external.json',external)]).value.evidenceStatus,'recorded-unverified');
 const forgedTrust=structuredClone(first.value);forgedTrust.rows[0].origin.value='recorded-external';denied('result',forgedTrust,'evaluation.metric-inconsistent');
 const forgedPending=structuredClone(first.value);forgedPending.rows.at(-1).metrics.filesRead={state:'known',value:0};denied('result',forgedPending,'evaluation.metric-inconsistent');
 let calls=0;const requests=[];const callback=async request=>{calls++;requests.push(request);assert.equal(request.policy.lekalo,request.slot.arm==='B');assert.equal(request.policy.semanticExposure,request.slot.arm==='B');return request;};
 for(const slot of c.slots.slice(0,2))await executeArm({binary,baseline:join(root,baseline),task:join(root,task),campaign:join(root,campaign),workspace,slot,executor:callback});assert.equal(calls,2);
 assert.deepEqual(requests[0].profile,requests[1].profile);for(const key of ['profileDigest','requestDigest','applicationDigest','campaignRef','taskRef','baselineRef'])assert.deepEqual(requests[0][key],requests[1][key],`neutral ${key}`);
 const mutableCampaign=temp('mutable-campaign.json',c);await assert.rejects(executeArm({binary,baseline:join(root,baseline),task:join(root,task),campaign:mutableCampaign,workspace,slot:c.slots[0],executor:async()=>{const changed=structuredClone(c);changed.randomizationSeed++;const {approval,...content}=changed;changed.approval.contentDigest=digest(content);writeFileSync(mutableCampaign,canonical(changed));}}),/protocol-changed-during-execution/);
 writeFileSync(join(workspace,'application.txt'),'drift');await assert.rejects(executeArm({binary,baseline:join(root,baseline),task:join(root,task),campaign:join(root,campaign),workspace,slot:c.slots[0],executor:callback}));assert.equal(calls,2,'zero executor calls after baseline drift');
 assert.deepEqual(readFileSync(join(root,'tests/fixtures/framework-lift-baseline/workspace/application.txt')),original);
 assert.deepEqual([...codes].sort(),['evaluation.arm-incomparable','evaluation.baseline-drift','evaluation.metric-inconsistent','evaluation.private-egress-denied','evaluation.protocol-invalid','evaluation.required-evidence-missing']);
 console.log(JSON.stringify({ok:true,gate:'framework-lift-contracts',family:selected,checks,liveFamilies:families,negativeCodes:[...codes].sort(),origin:'recorded-simulation',externalAgentRuns:0}));
} finally {rmSync(scratch,{recursive:true,force:true});}
