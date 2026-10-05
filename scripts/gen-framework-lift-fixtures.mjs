// Explicit synthetic recorded-evidence authoring. These are NOT model runs.
import { createHash } from 'node:crypto';
import { mkdirSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
if(!["", "--result"].includes(process.argv.slice(2).join(' ')))throw new Error('unknown authoring arguments');
const canonical = value => JSON.stringify(sort(value))+'\n';
function sort(value){return Array.isArray(value)?value.map(sort):value&&typeof value==='object'?Object.fromEntries(Object.keys(value).sort().map(k=>[k,sort(value[k])])):value;}
const sha = bytes => 'sha256:'+createHash('sha256').update(bytes).digest('hex');
const digest = value => sha(canonical(value));
const content = value => {const {approval,...body}=value;return digest(body);};
const known = value => ({state:'known',value});
const unknown = () => ({state:'unknown'});
const pin = id => ({id,revision:'synthetic-v-one',digest:sha(id)});
const header = family => ({schemaVersion:`lekalo/framework-lift-${family}/v0.6.4`,identity:`dev.lekalo.framework-lift-${family}@0.6.4`});
const write=(family,name,value)=>{const dir=`tests/fixtures/framework-lift-${family}/golden`;mkdirSync(dir,{recursive:true});writeFileSync(`${dir}/${name}.json`,canonical(value));};
mkdirSync('tests/fixtures/framework-lift-baseline/workspace',{recursive:true});
const application='Synthetic evaluation workspace; no real consumer or agent output.\n';
writeFileSync('tests/fixtures/framework-lift-baseline/workspace/application.txt',application);
const baseline={...header('baseline'),baselineId:'planner-base',pilot:'greenfield-contracted',revision:'a'.repeat(40),files:[{path:'application.txt',digest:sha(application)}],dataSeedDigest:sha('synthetic-seed'),semanticBundleDigest:sha('synthetic-model'),oracleDigest:sha('independent-synthetic-oracle')};
baseline.approval={contentDigest:content(baseline),role:'fixture-maintainer',revision:1};write('baseline','approved',baseline);
const limits={maxTokens:10000,maxToolCalls:100,deadlineMs:120000,maxFixCycles:4,maxProviderRetries:2};
const task={...header('task'),taskId:'planner-priority',taskVersion:1,taskClass:'priority',pilot:baseline.pilot,baselineRef:{id:baseline.baselineId,digest:digest(baseline)},requestDigest:sha('synthetic-request'),oracleDigest:baseline.oracleDigest,requiredAssertions:['priority-roundtrip'],regressionAssertions:['old-flow'],holdoutAssertions:['no-extra-effect'],limits};write('task','priority',task);
const profile={executor:pin('neutral-executor'),provider:pin('recorded-provider'),model:pin('recorded-model-revision'),harness:pin('recorded-harness'),runtime:pin('synthetic-runtime'),sampling:{temperatureMicros:0,topPMicros:1000000,reasoning:'disabled',seed:known(1),maxOutputTokens:2048},network:'public-synthetic',telemetry:'disabled',architectureProfile:{state:'unsupported'},pricing:{digest:sha('synthetic-tariff'),currency:'USD',basis:'estimated'},limits,measurementRecipe:'framework-lift-metrics-1'};
const slots=[{pairId:'pair-one',arm:'A',repetition:1},{pairId:'pair-one',arm:'B',repetition:1},{pairId:'pair-two',arm:'B',repetition:2},{pairId:'pair-two',arm:'A',repetition:2}];
const campaign={...header('campaign'),campaignId:'pilot-recorded',taskRef:{id:task.taskId,digest:digest(task)},baselineRef:task.baselineRef,profile,slots,randomizationSeed:1,analysis:'intent-to-run-wilson-95-paired-counts-1'};
campaign.approval={contentDigest:content(campaign),role:'fixture-maintainer',revision:1};write('campaign','scheduled',campaign);
const metricNames=['filesRead','filesChanged','unrelatedFiles','unrelatedAddedLoc','unrelatedDeletedLoc','toolCalls','retryCount','replanCount','fixCycles','escapedRegressions','durationMs','humanInterventions','humanDurationMs','capsuleBytes','capsuleEstimatedTokens','includedFacts','candidateFacts','requiredFacts','includedRequiredFacts','inputTokens','outputTokens','reasoningTokens','cachedInputTokens','totalTokens','costMicros'];
function arm(slot,name,{failure,assertionFail,cost=100,attempt=0}={}){
 const metrics=Object.fromEntries(metricNames.map(k=>[k,unknown()]));for(const [k,v]of Object.entries({toolCalls:0,replanCount:0,fixCycles:0,humanInterventions:0,escapedRegressions:0,costMicros:cost,filesRead:2,inputTokens:40,outputTokens:10,cachedInputTokens:20,totalTokens:50,durationMs:1000}))metrics[k]=known(v);
 const candidate=sha('synthetic-candidate-'+name);const assisted=slot.arm==='B';
 return {...header('arm'),runId:name,campaignRef:{id:campaign.campaignId,digest:digest(campaign)},taskRef:campaign.taskRef,baselineRef:task.baselineRef,slot,attempt,origin:'recorded-simulation',profileDigest:digest(profile),applicationDigest:content(baseline),policy:{lekalo:assisted,semanticExposure:assisted,network:profile.network},candidateDigest:candidate,coverage:'complete',assertions:[...task.requiredAssertions,...task.regressionAssertions,...task.holdoutAssertions].map((id,i)=>({id,outcome:assertionFail&&i===0?'fail':'pass',candidateDigest:candidate,oracleDigest:task.oracleDigest,receiptDigest:sha('synthetic-receipt-'+id)})),failures:failure?[{stage:'provider',class:'provider',reason:'service-unavailable',evidenceDigest:sha('synthetic-outage')}]:[],events:[{sequence:0,kind:'submit',tool:'none',evidenceDigest:sha('synthetic-submission')}],metrics,measurementSources:Object.keys(metrics).filter(k=>metrics[k].state==='known').map(metric=>({metric,component:pin('synthetic-recorder'),evidenceDigest:sha('synthetic-event-'+metric)})),optionalJudge:known(100)};
}
write('arm','a-provider',arm(slots[0],'a-outage',{failure:true,cost:100}));
write('arm','a-retry',arm(slots[0],'a-retry',{attempt:1,cost:200}));
write('arm','b-hard-fail',arm(slots[1],'b-regression',{assertionFail:true,cost:300}));
write('arm','b-neutral',arm(slots[2],'b-neutral',{cost:100}));
// pair-two/A intentionally absent: a missing scheduled run cannot disappear.
for(const family of ['baseline','task','campaign','arm','result']){
 mkdirSync(`tests/fixtures/framework-lift-${family}`,{recursive:true});
 writeFileSync(`tests/fixtures/framework-lift-${family}/README.md`,`# Framework Lift ${family} fixtures\n\nSynthetic recorded simulations for issue #100. No external agent, private consumer, prompt or billed run produced these files. The live CLI gate validates the closed family and its custody/arithmetic refusals.\n`);
}
if(process.argv[2]==='--result'){
 const f=(family,name)=>`tests/fixtures/framework-lift-${family}/golden/${name}.json`;
 const binary=resolve('target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
 const args=['--json','evaluation','compare','--baseline',f('baseline','approved'),'--task',f('task','priority'),'--campaign',f('campaign','scheduled'),...['a-provider','a-retry','b-hard-fail','b-neutral'].flatMap(name=>['--arm',f('arm',name)]),'--consumer-alias','consumer-greenfield-one'];
 const p=spawnSync(binary,args,{encoding:'utf8',timeout:30000});if(p.status!==0)throw new Error(p.stderr||'binary-missing');
 const result=JSON.parse(p.stdout);if(result.evidenceStatus!=='recorded-simulation')throw new Error('not a simulation');
 write('result','negative',result);
}
