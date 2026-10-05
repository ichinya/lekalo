// Issue #84: explicit authoring; --check is read-only and used by the live gate.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync, writeFileSync} from 'node:fs';
import {resolve, dirname} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
export const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
export const version='0.6.5';
export const ordered=v=>Array.isArray(v)?v.map(ordered):v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,ordered(v[k])])):v;
export const canonical=v=>JSON.stringify(ordered(v));
export const digest=v=>'sha256:'+createHash('sha256').update(v).digest('hex');
export const hash=v=>digest(canonical(v));
export const known=value=>({state:'known',value});
export const unknown=()=>({state:'unknown'});
export const header=family=>({schemaVersion:`lekalo/${family}/v${version}`,identity:`dev.lekalo.${family}@${version}`});
const pretty=v=>JSON.stringify(v,null,2)+'\n';
const read=path=>JSON.parse(readFileSync(resolve(root,path),'utf8'));
// The catalog owns requirements; producers own the meaning and coverage of signals.
const definitions=[
 ['explicit-operation-contracts','unsupported','contracted/error-contract','operationContractCoverage','advisory','Declare kind-appropriate IO, errors and effects so a change has a visible behavioral boundary. Command output and native exception completeness are unavailable here.','Author the operation contract and formal error attachment, then attach current native conformance evidence.'],
 ['stable-semantic-ids','complete','compiled-ir','declaredSemanticIds','semantic','Canonical semantic identities must be unique and resolvable; ordinary compilation and semantic validation remain mandatory. This count does not prove lifetime stability in native code.','Declare stable semantic IDs and use explicit rename history when an identity changes.'],
 ['strict-native-types','unsupported','adapter','nativeTypeCoverage','advisory','Native type completeness requires an adapter-qualified inventory, beyond portable Model type declarations.','Select the target type checker and supply current operation-to-native bindings and typed results.'],
 ['immutable-defaults','unsupported','adapter','mutationCoverage','advisory','Unclassified native mutation cannot establish immutability by default; a style preference alone must not block.','Use immutable values where applicable and explicitly declare mutable owners and permitted writes.'],
 ['explicit-dependencies','complete','compiled-ir','fanOutModules','measured','Declared module imports expose dependencies and permit a calibrated fan-out bound. Native constructor and runtime dependency completeness remain outside this measurement.','Declare the direct dependency or reduce the witnessed imports while preserving required behavior.'],
 ['bounded-context','complete','context-budget/0.6.3','minimumRequiredSemanticTokens','measured','The maximum minimum-required semantic token estimate across a module links policy to complete declared closure. It is a structural estimate, not empirical agent comprehension.','Split the change boundary or reduce declared closure while retaining every required semantic fact.'],
 ['single-entrypoint','unsupported','adapter','entrypointCoverage','advisory','A single native behavior entrypoint needs exact operation bindings and dispatch coverage. Endpoint declarations alone cannot prove it.','Bind the operation to one explicit entrypoint and declare any boundary dispatch.'],
 ['no-hidden-observers','unsupported','adapter/ai-lint','observerCoverage','advisory','Hidden native observer activation needs complete current target evidence; a clean token scan is insufficient.','Declare observer activation, owners and effects explicitly or use a direct operation path.'],
 ['no-service-locator-magic','unsupported','adapter/ai-lint','locatorCoverage','advisory','Service locator and magic resolution require adapter-specific evidence and must not be inferred from language-neutral names.','Pass explicit dependencies and bind dynamic resolution at a documented boundary.'],
 // Repeated output equality is a semantic behavior obligation, not subjective style.
 // Missing receipts stay unsupported and deny managed-generated's required proof.
 ['deterministic-generation','unsupported','artifacts','repeatedGenerationEquality','semantic','Determinism requires independent repeated outputs with pinned inputs and generator identity; content ownership alone is insufficient.','Derive paths from stable IDs and declare nondeterministic inputs, then compare repeated generation receipts.'],
 ['explicit-target-bindings','unsupported','bindings/adapter','nativeBindingCoverage','advisory','A target name in Model does not establish exact operation-to-native binding or current target authority.','Attach an explicit confirmed operation binding with current source and adapter pins.'],
 ['scenario-linked-behavior','partial','compiled-ir/trace','publicOperationsWithoutScenario','advisory','Missing scenario declarations are visible, but covers lists alone do not prove branch coverage or successful native gates.','Add a scenario for the operation and link the behavior and error cases to current passing gates.'],
 ['errors-as-contracts','unsupported','error-contract/adapter','nativeErrorValueCoverage','advisory','Formal error declarations cannot prove that native failures return values rather than escape as exceptions.','Use the existing typed error union and explicit boundary conversion with current native evidence.'],
 ['justified-abstractions','partial','coupling/0.6.4','sharedAbstractionRadius','advisory','Shared abstraction radius is a structural proxy; it cannot determine whether abstraction is premature and never blocks here.','Keep a concrete local implementation or record the shared contract, named consumers and reuse rationale.'],
 ['tracked-duplication','unsupported','coupling/adapter','replicaTrackingCoverage','advisory','Declared replica obligations do not inventory every native source duplicate; full tracking coverage is unavailable here.','Record semantic replica obligations and synchronization review, or justify independent contracts.'],
];
const token={type:'string',pattern:'^[A-Za-z0-9_.:/@-]+$',minLength:1,maxLength:256};
const pin={type:'string',pattern:'^sha256:[a-f0-9]{64}$'};
const prose={type:'string',minLength:1,maxLength:2000,pattern:'^[^\\u0000-\\u001f\\u007f]+$'};
const en=(...values)=>({enum:values});
const ref=name=>({$ref:`#/$defs/${name}`});
const obj=properties=>({type:'object',properties,required:Object.keys(properties),additionalProperties:false});
const arr=(items,maxItems=256,minItems=0)=>({type:'array',items,maxItems,minItems});
const state=(value,states=['unknown','unsupported','withheld'])=>({oneOf:[obj({state:{const:'known'},value}),...states.map(s=>obj({state:{const:s}}))]});
const ruleIds=definitions.map(x=>'architecture.'+x[0]).sort();
const defs={
 reference:obj({id:token,version:token,digest:pin}),
 scope:obj({kind:en('project','module'),id:token}),
 limit:obj({maximum:{type:'integer',minimum:0,maximum:10_000_000},calibrationRef:token}),
 selection:obj({id:{enum:ruleIds},enabled:{type:'boolean'},severity:en('info','warning','error'),required:{type:'boolean'},limit:state(ref('limit'),['unknown'])}),
 profile:obj({id:token,version:{const:'1'},extends:state(ref('reference'),['unknown']),rules:arr(ref('selection'))}),
 assignment:obj({scope:ref('scope'),profileRef:ref('reference')}),
 rule:obj({id:{enum:ruleIds},revision:{const:1},owner:{const:'lekalo-core'},producer:token,measurement:token,coverage:en('complete','partial','unsupported'),mandatory:{type:'boolean'},blockingBasis:en('semantic','measured','advisory'),severity:en('info','warning','error'),rationale:prose,alternative:prose}),
};
const shape=(family,properties)=>obj({...Object.fromEntries(Object.entries(header(family)).map(([k,v])=>[k,{const:v}])),...properties});
defs.resolved=shape('architecture-profile-resolved',{profileRef:ref('reference'),sourceDigest:pin,catalogRef:ref('reference'),chain:arr(ref('reference'),8,1),rules:arr(ref('selection'),15,15)});
defs.row=obj({ruleId:{enum:ruleIds},module:token,producer:token,measurement:token,coverage:en('complete','partial','unsupported','disabled'),value:state({type:'integer',minimum:0,maximum:Number.MAX_SAFE_INTEGER}),witnesses:arr(token,10000),severity:en('info','warning','error'),required:{type:'boolean'},limit:state(ref('limit'),['unknown']),disposition:en('disabled','observed','evidence-gap','violation','adopted'),conditionDigest:pin,rationale:prose,alternative:prose});
defs.change=obj({scope:token,ruleId:token,member:en('enabled','severity','required','limit','membership','assignments','document'),strength:en('weakened','strengthened','incomparable'),before:{type:'string',maxLength:2000},after:{type:'string',maxLength:2000}});
defs.debt=obj({ruleId:{enum:ruleIds},module:token,conditionDigest:pin,owner:token,reason:prose,reviewRef:token,expiresOn:{type:'string',pattern:'^[0-9]{4}-[0-9]{2}-[0-9]{2}$'}});
export const shapes={
 'architecture-rule-catalog':{recipe:{const:'architecture-core/1'},registryRef:ref('reference'),rules:arr(ref('rule'),15,15)},
 'architecture-profile':{catalogRef:ref('reference'),profiles:arr(ref('profile'),256,1),assignments:arr(ref('assignment'))},
 'architecture-profile-resolved':defs.resolved.properties,
 'architecture-profile-lock':{documentDigest:pin,catalogRef:ref('reference'),profiles:arr(ref('resolved'),256,1),assignments:arr(ref('assignment'))},
 'architecture-profile-report':{recipe:{const:'architecture-core/1'},scope:ref('scope'),documentDigest:pin,modelRef:pin,irRef:pin,snapshotRef:pin,profiles:arr(ref('resolved'),256,1),rows:arr(ref('row'),10000),baselineRef:state(pin,['unknown']),assessment:en('advisory','denied','adopting','enforced-core')},
 'architecture-profile-diff':{baseRef:pin,candidateRef:pin,comparable:{const:true},changes:arr(ref('change'),20000)},
 'architecture-adoption':{snapshotRef:pin,baselineRef:pin,entries:arr(ref('debt'),10000)},
};
export function artifacts(){
 const files=new Map();
 const predecessor=read('contracts/diagnostic-registry.v0.6.4.json');
 assert.equal(predecessor.entries.length,500);
 const names=[['input-invalid','invalid','Architecture profile input is invalid.'],['version-unsupported','unsupported-version','Architecture profile contract version is unsupported.'],['inheritance-invalid','invalid','Architecture profile inheritance is invalid.'],['weakening-unacknowledged','invalid','Architecture profile inheritance or scope silently weakens policy.'],['evidence-incomplete','valid','Architecture profile evidence is incomplete.'],['policy-denied','denied','Architecture profile obligations are not satisfied.'],['adoption-invalid','invalid','Architecture adoption evidence is invalid.'],['baseline-incomparable','invalid','Architecture baseline is not comparable.']];
 const entries=names.map(([suffix,status,message],i)=>({id:`architecture-profile.${suffix}`,code:`LEK-APR-${String(i+1).padStart(3,'0')}`,category:'infrastructure',default_severity:status==='valid'?'info':'error',allowed_statuses:[status],message_id:`architecture-profile.${suffix}`,default_message:message,location_requirement:'none',data_fields:[{name:'detail',type:'token'}],allowed_fix_ids:[],lifecycle:'active'}));
 const registry={...predecessor,schema_version:`lekalo/diagnostic-registry/v${version}`,identity:`dev.lekalo.diagnostic-registry@${version}`,registry_version:version,entries:[...predecessor.entries,...entries].sort((a,b)=>a.id<b.id?-1:1)};
 // Same entry grammar; frozen predecessor is never written.
 const registrySchema=JSON.parse(JSON.stringify(read('contracts/diagnostic-registry.schema.v0.6.4.json')).replaceAll('0.6.4',version));
 registrySchema.description='Issue #84 additive union successor: the 500 predecessor entries and entry grammar are preserved. Architecture policy adds eight service diagnostics; wire diagnostics remain v0.2.16.';
 files.set(`contracts/diagnostic-registry.v${version}.json`,pretty(registry));
 files.set(`contracts/diagnostic-registry.schema.v${version}.json`,pretty(registrySchema));
 const catalog={...header('architecture-rule-catalog'),recipe:'architecture-core/1',registryRef:{id:'diagnostic-registry',version,digest:digest(pretty(registry))},rules:definitions.map(([id,coverage,producer,measurement,blockingBasis,rationale,alternative])=>({id:'architecture.'+id,revision:1,owner:'lekalo-core',producer,measurement,coverage,mandatory:id==='stable-semantic-ids',blockingBasis,severity:id==='stable-semantic-ids'?'error':'warning',rationale,alternative})).sort((a,b)=>a.id<b.id?-1:1)};
 files.set(`contracts/architecture-rule-catalog.v${version}.json`,pretty(catalog));
 const catalogRef={id:'architecture-core',version,digest:digest(pretty(catalog))};
 const rootRules=catalog.rules.map(r=>({id:r.id,enabled:r.mandatory,severity:r.mandatory?'error':'info',required:r.mandatory,limit:unknown()}));
 const legacy={id:'legacy-observed',version:'1',extends:unknown(),rules:rootRules};
 const contractRules=catalog.rules.filter(r=>!r.mandatory).map(r=>({id:r.id,enabled:true,severity:'info',required:false,limit:unknown()}));
 const standard={id:'contracted-standard',version:'1',extends:known({id:legacy.id,version:'1',digest:hash(rootRules)}),rules:contractRules};
 const standardResolved=rootRules.map(r=>contractRules.find(x=>x.id===r.id)??r);
 const strictRules=catalog.rules.filter(r=>r.coverage==='complete'&&!r.mandatory).map(r=>({id:r.id,enabled:true,severity:'warning',required:true,limit:unknown()}));
 const strict={id:'ai-strict',version:'1',extends:known({id:standard.id,version:'1',digest:hash(standardResolved)}),rules:strictRules};
 const strictResolved=standardResolved.map(r=>strictRules.find(x=>x.id===r.id)??r);
 const managed={id:'managed-generated',version:'1',extends:known({id:strict.id,version:'1',digest:hash(strictResolved)}),rules:[{id:'architecture.deterministic-generation',enabled:true,severity:'info',required:true,limit:unknown()}]};
 const profiles={...header('architecture-profile'),catalogRef,profiles:[strict,standard,legacy,managed],assignments:[]};
 files.set(`contracts/architecture-profile.v${version}.json`,pretty(profiles));
 for(const [family,properties] of Object.entries(shapes)) {
   const used=new Set(); const visit=value=>{if(Array.isArray(value))value.forEach(visit);else if(value&&typeof value==='object'){if(value.$ref){const name=value.$ref.split('/').at(-1);if(!used.has(name)){used.add(name);visit(defs[name]);}}Object.values(value).forEach(visit);}};
   const body=shape(family,properties);visit(body);
   files.set(`contracts/${family}.schema.v${version}.json`,pretty({$schema:'https://json-schema.org/draft/2020-12/schema',$id:`dev.lekalo.${family}-schema@${version}`,title:`Lekalo ${family} ${version}`,...body,$defs:Object.fromEntries([...used].sort().map(k=>[k,defs[k]]))}));
 }
 return files;
}
export function generate(check){
 const files=artifacts();
 for(const [path,bytes] of files){if(check)assert.equal(readFileSync(resolve(root,path),'utf8').replaceAll('\r\n','\n'),bytes,`generator drift: ${path}`);else writeFileSync(resolve(root,path),bytes);}
 return files.size;
}
if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href){
 assert.ok(process.argv.length===3&&['--check','--write'].includes(process.argv[2]),'choose --check or --write');
 console.log(`architecture profile generator: ${generate(process.argv[2]==='--check')} artifacts ${process.argv[2]}`);
}
