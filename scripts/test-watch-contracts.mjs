#!/usr/bin/env node
// Independent closed-schema gate. Behavioral/cross-field and duplicate-key
// checks also execute through production Rust decoding in the CI test job.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFileSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {dirname,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {assertRegistryUnion} from './lib/registry-union.mjs';
const require=createRequire(import.meta.url);
assert.equal(require('ajv/package.json').version,'8.17.1');
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
const unionPolicy={families:['format.','adoption.','portability.','worklog-changelog.','managed.','module-api.','tasklink.','watch.'],ownPrefix:'watch.',ownCount:7};
const union=registry=>assertRegistryUnion(predecessor,registry,unionPolicy);
union(successor);
for(const [mutate,reason] of [
 [j=>{j.entries.splice(j.entries.findIndex(e=>e.id===predecessor.entries[0].id),1);},/predecessor/],
 [j=>{j.entries.find(e=>e.id===predecessor.entries[0].id).default_message='changed';},/predecessor/],
 [j=>{j.entries.push({...j.entries[0],id:'foreign.input-invalid',code:'LEK-FOREIGN-001'});},/foreign-family/],
 [j=>{j.entries=j.entries.filter(e=>e.id!=='watch.adapter-recovery');},/own-family/],
 [j=>{j.entries.push(j.entries[0]);},/duplicate rule/],
]){const draft=structuredClone(successor);mutate(draft);assert.throws(()=>union(draft),reason);}
const check=spawnSync(process.execPath,[resolve(root,'scripts/gen-watch-contracts.mjs'),'--check'],{cwd:root,encoding:'utf8'});
assert.equal(check.status,0,check.stderr);
console.log(JSON.stringify({ok:true,positive,negative,registryEntries:successor.entries.length,watchRules:7,registryControls:5}));
