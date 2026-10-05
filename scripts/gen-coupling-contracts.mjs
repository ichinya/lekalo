#!/usr/bin/env node
// Deliberate authoring helper for the issue #77 closed Draft 2020-12 contracts.
// Gates read committed schemas; they never generate or rewrite them.
import { writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..'), version='0.6.4';
const obj=properties=>({type:'object',additionalProperties:false,properties,required:Object.keys(properties)});
const str=(maxLength=192)=>({type:'string',minLength:1,maxLength});
const token={...str(),pattern:'^[A-Za-z0-9][A-Za-z0-9._:/#-]*$'};
const digest={type:'string',pattern:'^sha256:[0-9a-f]{64}$'};
const num={type:'integer',minimum:0,maximum:9007199254740991};
const enumeration=(...values)=>({enum:values});
const array=(items,maxItems=50000)=>({type:'array',items,maxItems,uniqueItems:true});
const ref=name=>({$ref:'#/$defs/'+name});
const state=value=>({oneOf:[obj({state:{const:'known'},value}),obj({state:enumeration('unknown','withheld','unsupported')})]});
const optionalState=value=>({oneOf:[obj({state:{const:'known'},value}),obj({state:{const:'unknown'}})]});
const metrics=['fanInSymbols','fanOutSymbols','fanInModules','fanOutModules','publicContractsAffected','internalSymbolsAffected','semanticSymbolsAffected','modulesAffected','affectedArtifacts','affectedTests','affectedTargets','requiredChecks','crossModuleCycles','sharedMutableResources','transactionSpread','sharedAbstractionRadius','publicTargetExposure','duplicationDivergence'];
const defs={};
defs.rational=obj({numerator:num,denominator:{...num,minimum:1}});
defs.limits=obj({limits:array(obj({metric:enumeration(...metrics),maximum:num}),18),regressionLimits:array(obj({metric:enumeration(...metrics),absoluteIncrease:num,relativeIncrease:optionalState(ref('rational'))}),18)});
defs.profile=obj({
 schemaVersion:{const:`lekalo/coupling-profile/v${version}`},identity:{const:`dev.lekalo.coupling-profile@${version}`},
 profileId:token,profileRevision:{...num,minimum:1},measurement:obj({recipe:{const:'declared-architecture/1'},includeObserved:{type:'boolean'}}),
 project:ref('limits'),modules:array(obj({module:token,thresholds:ref('limits')}),2000),
 domainGroups:array(obj({id:token,modules:{...array(token,2000),minItems:1}}),2000),
 centralityDeclarations:array(obj({subject:token,reason:str(256),reviewRef:token,rules:array(enumeration('coupling.fan-exceeded','coupling.public-contract-amplification','coupling.shared-abstraction-radius'),3)}),256),
 gate:obj({mode:enumeration('advisory','strict'),failOn:array(enumeration('baseline-regression','required-incomplete','threshold-exceeded'),3),baselineReadyRef:optionalState(digest)}),
});
defs.pins=obj({project:token,modelVersion:token,sourceRevision:optionalState({...str(64),pattern:"^(?:[0-9a-f]{40}|[0-9a-f]{64})$"}),modelDigest:optionalState(digest),irDigest:digest,semanticDigest:digest,graphDigest:digest,effectDigest:digest});
defs.partition=obj({publicContracts:array(token),internalSymbols:array(token),supportingSymbols:array(token),unclassifiedSymbols:array(token)});
defs.metrics=obj(Object.fromEntries(metrics.map(m=>[m,state(num)])));
defs.edge=obj({key:str(1024),from:token,to:token,relation:enumeration('references','accepts','returns','reads','emits','exposes'),occurrence:num,role:enumeration('entity-field','value-object-field','event-payload','command-effect','effect-entity','command-input','query-returns','query-reads','endpoint-invokes','effect-emits'),confidence:{const:'canonical'}});
defs.witness=obj({id:digest,root:token,subject:token,direction:enumeration('reverse','forward','effects'),edges:{type:'array',items:ref('edge'),maxItems:256},evidenceRefs:array(str(1024)),confidence:{const:'canonical'}});
defs.subject=obj({subject:token,module:{type:'string',maxLength:192},measurementBasis:enumeration('exact-declared','owner-conservative','aggregate-declared'),metrics:ref('metrics'),impact:ref('partition'),possible:array(token),lowerBounds:{type:'object',additionalProperties:false,properties:Object.fromEntries(metrics.map(m=>[m,num]))},fanIn:array(token),fanOut:array(token),affectedModules:array(token),artifacts:array(str(1024)),tests:array(token),targets:array(token),checks:array(token),resources:array(token),sharedResources:array(token),targetExposure:array(token),replicaObligations:array(token),importEdgeKeys:array(str(1024)),transactionGroups:array(token),transactionScopes:array(obj({group:token,modules:array(token),domainGroups:array(token),classified:{type:'boolean'}})),witnessRefs:array(digest),gaps:array(token,256)});
const rules=['input-invalid','profile-unsupported','fan-exceeded','public-contract-amplification','cross-module-cycle','shared-mutable-state','change-amplification','transaction-spread','shared-abstraction-radius','public-target-exposure','duplication-divergence','evidence-incomplete','baseline-incomparable','baseline-regression','policy-denied'].map(s=>'coupling.'+s);
defs.symbolFact=obj({module:{type:'string',maxLength:192},class:enumeration('public','internal','supporting','unclassified'),targetSpecific:{type:'boolean'},effectOperation:enumeration('none','create','update','delete')});
const keyed=(value,maxProperties)=>({type:'object',propertyNames:token,additionalProperties:value,maxProperties});
defs.snapshot=obj({symbols:keyed(ref('symbolFact'),100000),edges:array(ref('edge'),1000000),fields:keyed(keyed(array(token,1),10000),100000)});
defs.report=obj({
 schemaVersion:{const:`lekalo/coupling-report/v${version}`},identity:{const:`dev.lekalo.coupling-report@${version}`},metricVersion:{const:'coupling-metrics/1'},
 scope:obj({kind:enumeration('symbol','module','all','changed'),selector:token,roots:array(token,2000),changedInput:optionalState(digest)}),recipe:{const:'declared-architecture/1'},profile:ref('profile'),
 provenance:obj({inputs:ref('pins'),evidence:optionalState(digest),profileDigest:digest,measurementDigest:digest,projectionDigest:digest}),
 projection:ref('snapshot'),coverage:obj(Object.fromEntries(['declared','observed','artifacts','tests','members','transactions'].map(k=>[k,state(token)]))),
 subjects:array(ref('subject'),2000),summary:ref('partition'),publicContractCounts:obj(Object.fromEntries(['type','entity','operation','event','endpoint'].map(k=>[k,num]))),
 cycles:array(obj({series:enumeration('symbols','modules'),members:array(token),modules:array(token),edgeKeys:array(str(1024),1000000)}),100000),
 witnesses:array(ref('witness'),50000),findings:array(obj({rule:enumeration(...rules),subject:token,metric:enumeration(...metrics),detail:token,witnessRefs:array(digest),centrality:optionalState(token)}),20000),
 suggestions:array(obj({subject:token,action:{const:'review-consumer-projection-or-boundary-cut'},witnessRefs:{...array(digest),minItems:1},preserve:array(enumeration('semantic-identity','public-contract-compatibility','types-nullability-presence','policy-errors','effects-transaction-boundaries','invariants-ownership'),6),applied:{const:false}}),16),
 planning:obj({publicContracts:array(token),internalSymbols:array(token),resources:array(token),transactionGroups:array(token),requiredChecks:array(token),evidenceGaps:array(token,256),contextBudget:state(str(32*1024*1024)),runtimeConflicts:state(str(32*1024*1024)),reviewOverlap:array(token)}),complete:{type:'boolean'},
});
defs.comparison=obj({schemaVersion:{const:`lekalo/coupling-comparison/v${version}`},identity:{const:`dev.lekalo.coupling-comparison@${version}`},metricVersion:{const:'coupling-metrics/1'},baselineDigest:digest,candidateDigest:digest,comparable:{type:'boolean'},rows:array(obj({subject:token,metric:enumeration(...metrics),state:enumeration('added','removed','incomparable','comparable'),base:state(num),candidate:state(num),absolute:state({type:'integer',minimum:-9007199254740991,maximum:9007199254740991}),relative:optionalState(ref('rational')),verdict:enumeration('indeterminate','unchanged-or-allowed','regression'),reason:enumeration('subject-added','subject-removed','measurement-mismatch','unavailable-evidence','zero-baseline','decrease','none')}),20000),regressions:num,reasons:array(token,16)});
defs.attachment=obj({digest,bytes:str(16*1024*1024)});
defs.evidence=obj({schemaVersion:{const:`lekalo/coupling-evidence/v${version}`},identity:{const:`dev.lekalo.coupling-evidence@${version}`},inputs:ref('pins'),artifacts:state(ref('attachment')),trace:state(ref('attachment')),queries:state(ref('attachment')),transactions:state(ref('attachment')),replicas:array(obj({obligation:token,subjects:{...array(token,64),minItems:2},reviewRef:token}),256)});
defs['change-input']=obj({schemaVersion:{const:`lekalo/coupling-change-input/v${version}`},identity:{const:`dev.lekalo.coupling-change-input@${version}`},mode:enumeration('committed','index','worktree','mixed'),baseRevision:optionalState(token),candidateRevision:optionalState(token),entries:{type:'array',minItems:1,maxItems:2000,items:obj({symbolIds:array(token,2000),members:array(obj({entity:token,field:token,change:enumeration('removed','type-narrowed')}),2000),path:str(1024),fromPath:optionalState(str(1024)),change:enumeration('added','modified','deleted','renamed'),evidence:enumeration('canonical','verified','extracted','stale','unknown')})}});
for(const family of ['profile','report','comparison','evidence','change-input']) {
 const used=new Set([family]);
 const walk=value=> {if(Array.isArray(value))value.forEach(walk);else if(value&&typeof value==='object'){if(value.$ref?.startsWith('#/$defs/')){const name=value.$ref.slice(8);if(!used.has(name)){used.add(name);walk(defs[name]);}}Object.values(value).forEach(walk);}};
 walk(defs[family]);
 const schema={$schema:'https://json-schema.org/draft/2020-12/schema',$id:`https://lekalo.dev/contracts/coupling-${family}.schema.v${version}.json`,title:`Coupling ${family} ${version}`, $ref:`#/$defs/${family}`,$defs:Object.fromEntries([...used].map(name=>[name,defs[name]]))};
 writeFileSync(resolve(root,`contracts/coupling-${family}.schema.v${version}.json`),JSON.stringify(schema,null,2)+'\n');
}
console.log(JSON.stringify({ok:true,families:5,version}));
