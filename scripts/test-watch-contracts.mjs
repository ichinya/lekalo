#!/usr/bin/env node
// Independent closed-schema gate. Behavioral/cross-field and duplicate-key
// checks also execute through production Rust decoding in the CI test job.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFileSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {dirname,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {ACCEPTED_M8_REGISTRY_FAMILIES,assertRegistryUnion} from './lib/registry-union.mjs';
const require=createRequire(import.meta.url);
assert.equal(require('ajv/package.json').version,'8.20.0');
const Ajv=require('ajv/dist/2020').default;
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const load=name=>JSON.parse(readFileSync(resolve(root,`contracts/${name}.schema.v0.8.0.json`),'utf8'));
const ajv=new Ajv({strict:true,allErrors:true});
const validateResult=ajv.compile(load('watch-result'));
const validateEvent=ajv.compile(load('watch-event'));
const digest='sha256:'+ '0'.repeat(64);
const result={schemaVersion:'lekalo/watch-result/v0.8.0',identity:'dev.lekalo.watch-result@0.8.0',status:'valid',diagnosticsMode:'replace',sourceRevision:digest,diagnostics:[],stages:[{stage:'validate',state:'executed',coverage:'complete'}],changedInputs:[],affected:{scope:'full',reason:'baseline',modules:[]},work:[],counts:{diagnostics:0,executedUnits:0,reusedUnits:0,changedFiles:0},capsuleInvalidations:[],pins:{modelVersion:'0.2.16',registryVersion:'0.8.0',versions:{core:'0.8.0',ir:'0.2.16',graph:'0.2.16',impact:'0.2.16',validationProfile:'0.6.4',diagnostic:'lekalo/diagnostic/v0.2.16',artifactManifest:'lekalo/artifact-manifest/v0.2.16',lock:'lekalo/lock/v0.3.2',targetProtocol:'0.3.2',targetCapabilities:'dev.lekalo.target-capabilities@0.6.4'},revisions:{},adapters:[]},pipelineCounts:{decodedDocuments:0,reusedDocuments:0,decodedDefinitions:0,reusedDefinitions:0,emittedGraphUnits:0,reusedGraphUnits:0,assembledGraphNodes:0,assembledGraphEdges:0,graphCyclePasses:0}};
const common={schemaVersion:'lekalo/watch-event/v0.8.0',identity:'dev.lekalo.watch-event@0.8.0',sequence:1,sessionId:'contract-test'};
const run={runId:1,generation:0,sourceRevision:digest};
const events=[
 {...common,event:'session-started',backend:'native',fallback:false,stages:['validate'],debounceMs:150,maxWaitMs:1000},
 {...common,event:'run-started',...run,changedFiles:0,fullRescan:true},
 {...common,event:'run-completed',...run,diagnosticsMode:'replace',status:'valid',result},
 {...common,event:'run-cancelled',...run,supersededByGeneration:1,committed:false},
 {...common,event:'recovery-required',cause:'overflow',action:'full-rescan'},
 {...common,event:'session-stopped',reason:'interrupt',runs:1,lastStatus:'valid'},
];
const incomplete={...common,event:'recovery-required',...run,cause:'cancellation-incomplete',action:'stop',committed:false,status:'unavailable',diagnostics:[{schema_version:'lekalo/diagnostic/v0.2.16',registry_version:'0.8.0',id:'watch.cancellation-incomplete',code:'LEK-WATCH-007',severity:'error',category:'infrastructure',message_id:'watch.cancellation-incomplete',message:'Teardown unproven.',data:{stage:'worker'},related_locations:[],causes:[],fixes:[],metadata:{}}]};
events.push(incomplete);
let positive=0,negative=0;
function valid(validate,value){assert(validate(value),JSON.stringify(validate.errors));positive++;}
function invalid(validate,value){assert(!validate(value),'schema unexpectedly accepted invalid vector');negative++;}
valid(validateResult,result);
for(const event of events){valid(validateEvent,event);invalid(validateEvent,{...event,unknown:true});invalid(validateEvent,{...event,schemaVersion:'lekalo/watch-event/v0.7.0'});invalid(validateEvent,{...event,sequence:0});}
for(const status of ['valid','invalid','denied','unsupported','unavailable','unsupported-version'])valid(validateResult,{...result,status});
for(const value of [-1,1.5,Number.MAX_SAFE_INTEGER+1])invalid(validateResult,{...result,counts:{...result.counts,diagnostics:value}});
invalid(validateResult,{...result,pins:{modelVersion:'0.2.16',registryVersion:'0.6.4'}});
invalid(validateResult,{...result,pins:{modelVersion:'0.1.0',registryVersion:'0.8.0'}});
invalid(validateResult,{...result,pins:{modelVersion:'0.3.0',registryVersion:'0.8.0'}});
invalid(validateResult,{...result,pipelineCounts:{...result.pipelineCounts,filteredGraphUnits:1}});
invalid(validateResult,{...result,pipelineCounts:{decodedDocuments:0}});
invalid(validateResult,{...result,sourceRevision:'sha256:bad'});
invalid(validateResult,{...result,diagnosticsMode:'append'});
invalid(validateResult,{...result,pins:{...result.pins,versions:{...result.pins.versions,ir:'0.8.0'}}});
invalid(validateResult,{...result,pins:{...result.pins,revisions:{modelDigest:'bad'}}});
invalid(validateResult,{...result,pins:{...result.pins,adapters:[{target:'demo',id:'demo',version:'1x0x0',digest,declaredDigest:digest,entryDigest:digest,capabilityDigest:digest,protocolVersion:'0.3.2'}]}});
for(const path of ['/absolute','../escape','lekalo/../escape','C:/secret','lekalo/path\\escape'])invalid(validateResult,{...result,changedInputs:[{path,kind:'created',class:'semantic',newSha256:digest}]});
invalid(validateResult,{...result,work:[{module:'planner',sha256:digest,state:'filtered'}]});
invalid(validateResult,{...result,stages:[{stage:'generate',state:'executed',coverage:'complete'}]});
invalid(validateResult,{...result,capsuleInvalidations:Array(129).fill('planner.task')});
valid(validateResult,{...result,pins:{...result.pins,adapters:[{target:'demo',id:'demo',version:'0.8.0',digest,declaredDigest:digest,entryDigest:digest,capabilityDigest:digest,protocolVersion:'0.3.2'}]}});
invalid(validateEvent,{...incomplete,action:'full-rescan'});
invalid(validateEvent,{...incomplete,committed:true});
invalid(validateEvent,{...events[3],committed:true});
const noRevision=structuredClone(events[3]);delete noRevision.sourceRevision;invalid(validateEvent,noRevision);
invalid(validateEvent,{...events[2],result:{...result,rawSource:'secret'}});
// Count our additions only; preserve EVERY predecessor rule and admit
// only the live integrated families (not an arbitrary foreign namespace).
const predecessor=JSON.parse(readFileSync(resolve(root,'contracts/diagnostic-registry.v0.6.5.json'),'utf8'));
const successor=JSON.parse(readFileSync(resolve(root,'contracts/diagnostic-registry.v0.8.0.json'),'utf8'));
const unionPolicy={families:ACCEPTED_M8_REGISTRY_FAMILIES,ownPrefix:'watch.',ownCount:7};
const union=registry=>assertRegistryUnion(predecessor,registry,unionPolicy);
union(successor);
for(const [mutate,reason] of [
 [j=>{j.entries.splice(j.entries.findIndex(e=>e.id===predecessor.entries[0].id),1);},/predecessor/],
 [j=>{j.entries.find(e=>e.id===predecessor.entries[0].id).default_message='changed';},/predecessor/],
 [j=>{j.entries.push({...j.entries[0],id:'foreign.input-invalid',code:'LEK-FOREIGN-001'});},/foreign-family/],
 [j=>{j.entries=j.entries.filter(e=>e.id!=='watch.adapter-recovery');},/own-family/],
 [j=>{j.entries.push(j.entries[0]);},/duplicate rule/],
]){const draft=structuredClone(successor);mutate(draft);assert.throws(()=>union(draft),reason);}
// The selection seam is a successor, not a field smuggled into frozen
// requests. Ajv independently agrees with the production wire decoder.
const protocol=ajv.compile(JSON.parse(readFileSync(resolve(root,'contracts/target-protocol.schema.v0.8.0.json'),'utf8')));
const selection={source_revision:digest,pins:{model_version:'0.2.16',ir_version:'0.2.16',ir_digest:digest},scope:'module',module:'planner',symbols:['planner.title'],changed_paths:['lekalo/modules/planner/entities.yaml']};
const request={protocol:'lekalo.target/v1',protocol_version:'0.8.0',operation:'scan',request_id:'req-'+'0'.repeat(64),project_root:'.',target:'demo',ir_path:'ir/project.json',watch_request:selection};
valid(protocol,request);
valid(protocol,{...request,watch_request:{...selection,scope:'global',symbols:[],module:undefined}});
for(const mutate of [
 j=>{j.protocol_version='0.3.2';},j=>{j.operation='describe';},j=>{j.watch_request.unknown=true;},
 j=>{j.watch_request.source_revision='bad';},j=>{j.watch_request.changed_paths=['../escape'];},
 j=>{j.watch_request.scope='global';},j=>{j.watch_request.module=null;},j=>{delete j.target;},j=>{delete j.ir_path;},
 j=>{delete j.watch_request.pins;},j=>{j.watch_request.pins.model_version='0.8.1';},
 j=>{j.watch_request.pins.ir_version='0.8.0';},j=>{j.watch_request.pins.ir_digest='unknown';},
 j=>{j.watch_request.pins.ir_digest=null;},j=>{j.watch_request.pins.unknown=true;},
]){const candidate=structuredClone(request);mutate(candidate);invalid(protocol,candidate);}
const successorRequest=structuredClone(request);
successorRequest.watch_request.pins.model_version='0.8.0';
successorRequest.watch_request.pins.ir_version='0.8.0';
valid(protocol,successorRequest);
const refusalGolden=JSON.parse(readFileSync(resolve(root,'tests/fixtures/watch/golden/pin-refusals.json'),'utf8'));
for(const row of refusalGolden.cases){
 const candidate=structuredClone(successorRequest);
 if(row.case==='unknown')candidate.watch_request.pins.model_version='0.8.1';
 else if(row.case==='mixed')candidate.watch_request.pins.ir_version='0.2.16';
 else if(row.case==='stale')candidate.watch_request.pins.ir_digest='sha256:'+'1'.repeat(64);
 else if(row.case==='legacy')candidate.protocol_version='0.3.2';
 (row.schema==='valid'?valid:invalid)(protocol,candidate);
 assert.equal(row.spawned,false);
}
const adapter={id:'fixture',version:'0.8.0',digest};
const described={protocol:request.protocol,protocol_version:'0.8.0',operation:'describe',request_id:request.request_id,status:'ok',evidence:{adapter},capabilities:{adapter,protocol_versions:['0.3.2','0.8.0'],operations:['describe','scan','validate'],transports:['stdin'],ir_versions:['0.2.16','0.8.0'],watch:{selection:true,model_versions:['0.2.16','0.8.0']}}};
valid(protocol,described);
for(const versions of [[],['0.8.1'],['0.8.0','0.8.0']]){
 const candidate=structuredClone(described);candidate.capabilities.watch.model_versions=versions;invalid(protocol,candidate);
}
const successorResult=structuredClone(result);
successorResult.pins.modelVersion='0.8.0';
for(const family of ['ir','graph','impact'])successorResult.pins.versions[family]='0.8.0';
valid(validateResult,successorResult);
for(const family of ['ir','graph','impact']){
 const stale=structuredClone(successorResult);stale.pins.versions[family]='0.2.16';invalid(validateResult,stale);
}
for(const version of ['0.3.2','0.6.4']){
 const frozen=ajv.compile(JSON.parse(readFileSync(resolve(root,`contracts/target-protocol.schema.v${version}.json`),'utf8')));
 invalid(frozen,{...request,protocol_version:version});
}
const successorGolden=JSON.parse(readFileSync(resolve(root,'tests/fixtures/watch/golden/successor-result.json'),'utf8'));
valid(validateResult,successorGolden);
valid(ajv.compile(load('impact')),successorGolden.impact);
for(const pin of ['modelVersion','ir','graph','impact']){
 const stale=structuredClone(successorGolden);
 if(pin==='modelVersion')stale.pins.modelVersion='0.2.16';else stale.pins.versions[pin]='0.2.16';
 invalid(validateResult,stale);
}
const boundary=JSON.parse(readFileSync(resolve(root,'tests/fixtures/watch/golden/platform-qualification.json'),'utf8'));
assert.equal(boundary.decision,'declared-runtime-evidence-boundary');
assert.equal(boundary.crossCompilationIsRuntimeEvidence,false);
assert.equal(boundary.windowsTestsAreMacosEvidence,false);
assert.equal(boundary.deliveryEvidence.macos,'native-runtime-unavailable-no-macos-host');
assert.equal(boundary.deliveryEvidence.linux,'qualified-full-pass-later-wsl-revalidation-blocked');
assert.equal(boundary.linuxRevalidation.fullWatchGatePass,true);
assert.equal(boundary.linuxRevalidation.disposition,'retain-qualified-evidence-no-blanket-stable-host-claim');
assert.deepEqual(boundary.linuxRevalidation.subsequentBounds,['clean-validate-and-watch-oracle-30s-deadlines','wsl-exec-unresponsive-before-shell-start']);
assert.equal(boundary.macosPolicy.memory,'sampled-resident-supervision-degraded-not-hard-ceiling');
assert.equal(boundary.macosPolicy.missingEnforcement,'refuse-no-ambient-fallback');
const protocolCheck=spawnSync(process.execPath,[resolve(root,'scripts/gen-watch-protocol.mjs'),'--check'],{cwd:root,encoding:'utf8'});
assert.equal(protocolCheck.status,0,protocolCheck.stderr);
const check=spawnSync(process.execPath,[resolve(root,'scripts/gen-watch-contracts.mjs'),'--check'],{cwd:root,encoding:'utf8'});
assert.equal(check.status,0,check.stderr);
console.log(JSON.stringify({ok:true,positive,negative,registryEntries:successor.entries.length,watchRules:7,registryControls:5}));
