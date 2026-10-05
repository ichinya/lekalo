// Explicit authoring helper. Runtime gates consume committed bytes, never regenerate.
import { writeFileSync } from 'node:fs';
const v = '0.6.4';
const obj = properties => ({type:'object',additionalProperties:false,required:Object.keys(properties),properties});
const list = (items,maxItems=256,minItems=0) => ({type:'array',items,minItems,maxItems,uniqueItems:true});
const en = (...values) => ({enum:values});
const token = {type:'string',minLength:1,maxLength:96,pattern:'^[a-z][a-z0-9-]*$'};
const digest = {type:'string',pattern:'^sha256:[0-9a-f]{64}$'};
const count = {type:'integer',minimum:0,maximum:9007199254740991};
const positive = {...count,minimum:1};
const state = value => ({oneOf:[obj({state:{const:'known'},value}),obj({state:en('unknown','withheld','unsupported')})]});
const ref = name => ({$ref:`#/$defs/${name}`});
const header = (family,props) => obj({schemaVersion:{const:`lekalo/framework-lift-${family}/v${v}`},identity:{const:`dev.lekalo.framework-lift-${family}@${v}`},...props});
const defs = {};
defs.pilot = en('greenfield-contracted','brownfield-observed');
defs.arm = en('A','B');
defs.origin = en('recorded-simulation','recorded-external');
defs.ref = obj({id:token,digest});
defs.component = obj({id:token,revision:token,digest});
defs.file = obj({path:{type:'string',minLength:1,maxLength:192,pattern:'^[a-zA-Z0-9][a-zA-Z0-9._/-]*$'},digest});
defs.limits = obj({maxTokens:positive,maxToolCalls:positive,deadlineMs:positive,maxFixCycles:count,maxProviderRetries:count});
defs.profile = obj({
  executor:ref('component'),provider:ref('component'),model:ref('component'),harness:ref('component'),runtime:ref('component'),
  sampling:obj({temperatureMicros:count,topPMicros:count,reasoning:token,seed:state(count),maxOutputTokens:positive}),
  network:en('local-only','public-synthetic'),telemetry:{const:'disabled'},
  architectureProfile:state(ref('ref')),pricing:obj({digest,currency:{type:'string',pattern:'^[A-Z]{3}$'},basis:en('reported','estimated')}),
  limits:ref('limits'),measurementRecipe:{const:'framework-lift-metrics-1'},
});
defs.slot = obj({pairId:token,arm:ref('arm'),repetition:positive});
defs.assertion = obj({id:token,outcome:en('pass','fail','unsupported','infrastructure'),candidateDigest:digest,oracleDigest:digest,receiptDigest:digest});
defs.failure = obj({stage:en('agent','tool','verifier','provider','setup','custody'),class:en('task','provider','infrastructure','unsupported','custody-security','interrupted'),reason:token,evidenceDigest:digest});
defs.event = obj({sequence:count,kind:en('submit','fix','replan','provider-attempt','tool-call','read','human'),tool:en('none','native','lekalo','read','search'),evidenceDigest:digest});
const metricNames = ['filesRead','filesChanged','unrelatedFiles','unrelatedAddedLoc','unrelatedDeletedLoc','toolCalls','retryCount','replanCount','fixCycles','escapedRegressions','durationMs','humanInterventions','humanDurationMs','capsuleBytes','capsuleEstimatedTokens','includedFacts','candidateFacts','requiredFacts','includedRequiredFacts','inputTokens','outputTokens','reasoningTokens','cachedInputTokens','totalTokens','costMicros'];
defs.metrics = obj(Object.fromEntries(metricNames.map(name=>[name,state(count)])));
defs.source = obj({metric:en(...metricNames),component:ref('component'),evidenceDigest:digest});
defs.row = obj({slot:ref('slot'),attempt:count,armDigest:state(digest),origin:state(ref('origin')),status:en('success','task','provider','infrastructure','unsupported','custody-security','interrupted','not-started'),verifiedSuccess:{type:'boolean'},firstPassSuccess:{type:'boolean'},metrics:ref('metrics')});
defs.ratio = obj({numerator:count,denominator:positive});
// Conservative integer endpoints and exact rational lift avoid float JSON drift.
defs.interval = obj({method:{const:'wilson-95'},unit:{const:'parts-per-million'},lower:{type:'integer',minimum:0,maximum:1000000},upper:{type:'integer',minimum:0,maximum:1000000}});
defs.lift = obj({numerator:{type:'integer',minimum:-12800,maximum:12800},denominator:{type:'integer',minimum:2,maximum:128}});
defs.summary = obj({arm:ref('arm'),scheduled:count,started:count,attempts:count,successes:count,firstPassSuccesses:count,costKnownRuns:count,totalCostMicros:state(count),costPerSuccess:obj({state:en('known','unknown'),reason:en('complete','no-success','cost-incomplete'),value:state(ref('ratio'))}),successRate:ref('ratio'),successInterval:ref('interval')});
const families = {
 baseline:header('baseline',{baselineId:token,pilot:ref('pilot'),revision:{type:'string',pattern:'^[0-9a-f]{40}$'},files:list(ref('file'),256,1),dataSeedDigest:digest,semanticBundleDigest:digest,oracleDigest:digest,approval:obj({contentDigest:digest,role:token,revision:positive})}),
 task:header('task',{taskId:token,taskVersion:positive,taskClass:en('priority','status-transition','command-event','authorization','relation-many','idempotency-concurrency','service-extraction'),pilot:ref('pilot'),baselineRef:ref('ref'),requestDigest:digest,oracleDigest:digest,requiredAssertions:list(token,256,1),regressionAssertions:list(token,256,1),holdoutAssertions:list(token,256,1),limits:ref('limits')}),
 campaign:header('campaign',{campaignId:token,taskRef:ref('ref'),baselineRef:ref('ref'),profile:ref('profile'),slots:list(ref('slot'),256,4),randomizationSeed:count,analysis:{const:'intent-to-run-wilson-95-paired-counts-1'},approval:obj({contentDigest:digest,role:token,revision:positive})}),
 arm:header('arm',{runId:token,campaignRef:ref('ref'),taskRef:ref('ref'),baselineRef:ref('ref'),slot:ref('slot'),attempt:count,origin:ref('origin'),profileDigest:digest,applicationDigest:digest,policy:obj({lekalo:{type:'boolean'},semanticExposure:{type:'boolean'},network:en('local-only','public-synthetic')}),candidateDigest:digest,coverage:en('complete','incomplete','unknown'),assertions:list(ref('assertion'),768),failures:list(ref('failure'),32),events:list(ref('event'),4096),metrics:ref('metrics'),measurementSources:list(ref('source'),metricNames.length),optionalJudge:state(count)}),
 result:header('result',{campaignRef:ref('ref'),taskRef:ref('ref'),baselineRef:ref('ref'),profileDigest:digest,evidenceStatus:en('recorded-simulation','recorded-unverified'),exportDisposition:{const:'local-private'},consumerRole:{const:'consumer-repository'},consumerAlias:token,pilot:ref('pilot'),analysis:{const:'intent-to-run-wilson-95-paired-counts-1'},rows:list(ref('row'),1024),summaries:list(ref('summary'),2,2),paired:obj({bothSuccess:count,aOnly:count,bOnly:count,neither:count,incomplete:count}),liftPercentagePoints:ref('lift'),uncertainty:obj({method:{const:'attrition-bounds'},minimumLift:ref('lift'),maximumLift:ref('lift')}),claim:{const:'tested-task-profile-only'}}),
};
for (const [family,shape] of Object.entries(families)) {
 const used = new Set(); const visit = x => {if(!x||typeof x!=='object')return; if(x.$ref){const name=x.$ref.split('/').at(-1);if(!used.has(name)){used.add(name);visit(defs[name]);}} for(const [k,val]of Object.entries(x))if(k!=='$ref')Array.isArray(val)?val.forEach(visit):visit(val);}; visit(shape);
 const schema={$schema:'https://json-schema.org/draft/2020-12/schema',$id:`dev.lekalo.framework-lift-${family}-schema@${v}`,description:'Closed local-only recorded evaluation evidence. Simulation is never live agent evidence.',...shape,$defs:Object.fromEntries([...used].sort().map(name=>[name,defs[name]]))};
 writeFileSync(`contracts/framework-lift-${family}.schema.v${v}.json`,JSON.stringify(schema,null,2)+'\n');
}
