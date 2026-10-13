#!/usr/bin/env node
// Issue #95: closed Draft 2020-12 watch contracts, derived only from
// the accepted diagnostic item. No predecessor contract is changed.
import {readFileSync, writeFileSync} from 'node:fs';
import assert from 'node:assert/strict';
import {resolve, dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const string=(maxLength=128)=>({type:'string',minLength:1,maxLength});
const digest={type:'string',pattern:'^sha256:[0-9a-f]{64}$'};
const counter={type:'integer',minimum:0,maximum:Number.MAX_SAFE_INTEGER};
const token={...string(256),pattern:'^[a-z][a-z0-9_-]*(\\.[a-z][a-z0-9_-]*)*$'};
const path={...string(4096),pattern:'^(?:\\.lekalo/)?[a-z0-9][a-z0-9._-]*(/[a-z0-9._-]+)*$',not:{pattern:'(^|/)\\.\\.?(/|$)'}};
const statuses=['valid','invalid','denied','unsupported','unavailable','unsupported-version'];
const object=(properties,required=Object.keys(properties))=>({type:'object',properties,required,additionalProperties:false});
const array=(items,maxItems)=>({type:'array',items,maxItems});
function rewrite(value,prefix){
  if(Array.isArray(value)) return value.map(v=>rewrite(v,prefix));
  if(value&&typeof value==='object') return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,k==='$ref'&&v.startsWith('#/')?prefix+v.slice(1):rewrite(v,prefix)]));
  return value;
}
const diag=JSON.parse(readFileSync(resolve(root,'contracts/diagnostic.schema.v0.2.16.json'),'utf8'));
delete diag.$id; delete diag.$schema;
const impact=JSON.parse(readFileSync(resolve(root,'contracts/impact.schema.v0.2.16.json'),'utf8'));
delete impact.$id; delete impact.$schema;
const result={
 $schema:'https://json-schema.org/draft/2020-12/schema',
 $id:'dev.lekalo.watch-result@0.8.0',
 title:'Atomic watch diagnostic replacement v0.8.0',
 ...object({
  schemaVersion:{const:'lekalo/watch-result/v0.8.0'},identity:{const:'dev.lekalo.watch-result@0.8.0'},
  status:{enum:statuses},diagnosticsMode:{const:'replace'},sourceRevision:digest,previousSourceRevision:digest,
  diagnostics:array({$ref:'#/$defs/diagnostic'},512),
  stages:array(object({stage:{enum:['validate','impact','generate-check','target-scan','lock-preflight']},state:{enum:['executed','reused','not-requested','unavailable']},coverage:{enum:['complete','partial','unsupported']},reason:string()},['stage','state','coverage']),16),
  changedInputs:array(object({path,kind:{enum:['created','modified','deleted']},oldSha256:digest,newSha256:digest,renameOf:path,class:{enum:['semantic','generated']}},['path','kind','class']),4096),
  affected:object({scope:{enum:['selective','full']},reason:{enum:['baseline','project-input','target-input','unknown-owner','recovery','pending-overflow','parse-invalid','no-cache','graph-unavailable']},modules:array(token,4096)},['scope','modules']),
  work:array(object({module:token,sha256:digest,state:{enum:['executed','reused']}}),4096),
  counts:object({diagnostics:counter,executedUnits:counter,reusedUnits:counter,changedFiles:counter}),
  capsuleInvalidations:array(token,128),
  pins:object({modelVersion:{const:'0.2.16'},registryVersion:{const:'0.8.0'}},['registryVersion']),
  impact:{$ref:'#/$defs/impact'},
  drift:object({verdict:{enum:['clean','reported']},clean:counter,stale:counter,manualDrift:counter,missing:counter,orphan:counter,reported:counter,findings:counter}),
  pipelineCounts:object({decodedDocuments:counter,reusedDocuments:counter,decodedDefinitions:counter,reusedDefinitions:counter,emittedGraphUnits:counter,reusedGraphUnits:counter,assembledGraphNodes:counter,assembledGraphEdges:counter,graphCyclePasses:counter})
 },['schemaVersion','identity','status','diagnosticsMode','sourceRevision','diagnostics','stages','changedInputs','affected','work','counts','capsuleInvalidations','pins']),
 $defs:{diagnostic:rewrite(diag,'#/$defs/diagnostic'),impact:rewrite(impact,'#/$defs/impact')}
};
const common={schemaVersion:{const:'lekalo/watch-event/v0.8.0'},identity:{const:'dev.lekalo.watch-event@0.8.0'},sequence:{...counter,minimum:1},sessionId:{...string(64),pattern:'^[a-zA-Z0-9-]+$'}};
const run={runId:{...counter,minimum:1},generation:counter,sourceRevision:digest};
const arm=(event,props,required)=>object({...common,event:{const:event},...props},[...Object.keys(common),'event',...(required??Object.keys(props))]);
const nested=structuredClone(result); delete nested.$id; delete nested.$schema;
const event={
 $schema:'https://json-schema.org/draft/2020-12/schema',
 $id:'dev.lekalo.watch-event@0.8.0',title:'Watch JSON Lines event v0.8.0',
 oneOf:[
  arm('session-started',{backend:{enum:['native','poll']},fallback:{type:'boolean'},stages:array({enum:['validate','impact','generate-check']},3),module:token,debounceMs:{...counter,minimum:1,maximum:10000},maxWaitMs:{...counter,minimum:1,maximum:60000}},['backend','fallback','stages','debounceMs','maxWaitMs']),
  arm('run-started',{...run,changedFiles:counter,fullRescan:{type:'boolean'}}),
  arm('run-completed',{...run,diagnosticsMode:{const:'replace'},status:{enum:statuses},result:{$ref:'#/$defs/result'}}),
  arm('run-cancelled',{runId:run.runId,generation:counter,sourceRevision:digest,supersededByGeneration:counter,committed:{const:false}},['runId','generation','sourceRevision','supersededByGeneration','committed']),
  arm('recovery-required',{cause:{enum:['overflow','ambiguous-rename','subscription-lost','inventory-mismatch']},action:{const:'full-rescan'},backend:{enum:['native','poll']},status:{enum:statuses},diagnostics:array({$ref:'#/$defs/result/$defs/diagnostic'},512)},['cause','action']),
  arm('session-stopped',{reason:{enum:['interrupt','stop-after-runs','output-failure','terminal-refusal']},runs:counter,lastStatus:{enum:statuses}},['reason','runs'])
 ],
 $defs:{result:rewrite(nested,'#/$defs/result')}
};
assert(process.argv.slice(2).every(arg=>arg==='--check'),'only --check is supported');
for(const [name,value] of [['watch-result',result],['watch-event',event]]) {
 const file=resolve(root,`contracts/${name}.schema.v0.8.0.json`), bytes=JSON.stringify(value,null,2)+'\n';
 if(process.argv.includes('--check')) assert.equal(readFileSync(file,'utf8'),bytes,`${name}: stale generated schema`);
 else writeFileSync(file,bytes);
}
