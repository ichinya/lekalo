#!/usr/bin/env node
// Additive successors; the published 0.3.2 files remain unchanged.
import { readFileSync, writeFileSync } from 'node:fs';
const read = p => JSON.parse(readFileSync(p,'utf8'));
const closed = properties => ({type:'object',additionalProperties:false,required:Object.keys(properties),properties});
const sha = {type:'string',pattern:'^sha256:[a-f0-9]{64}$',maxLength:71};
const wire = read('contracts/target-protocol.schema.v0.3.2.json');
wire.$id = 'https://dev.lekalo/target-protocol.schema.v0.6.4.json';
wire.description += ' Successor 0.6.4 adds lint, lint_request and lint_evidence. This read-only static collector never executes application code or native gates.';
wire.$defs.protocolVersion = {const:'0.6.4'};
wire.$defs.operation.enum.push('lint');
wire.$defs.capabilities.properties.operations.maxItems=10;
const evidence = read('contracts/ai-lint-evidence.schema.v0.6.4.json');
Object.assign(wire.$defs,evidence.$defs);
wire.$defs.LintEvidence = Object.fromEntries(Object.entries(evidence).filter(([k])=>!['$schema','$id','$defs','title'].includes(k)));
wire.$defs.LintBinding = closed({path:{$ref:'#/$defs/logicalPath'},nativeId:{$ref:'#/$defs/StateString/oneOf/0/properties/value'},symbol:{$ref:'#/$defs/StateString/oneOf/0/properties/value'},kind:{enum:['command','entity']},fingerprint:sha});
wire.$defs.LintRequest = closed({scope:{type:'array',minItems:1,maxItems:10000,uniqueItems:true,items:{$ref:'#/$defs/StateString/oneOf/0/properties/value'}},pins:{$ref:'#/$defs/Pins'},bindings:{type:'array',maxItems:10000,items:{$ref:'#/$defs/LintBinding'}},files:{type:'array',maxItems:4096,uniqueItems:true,items:{$ref:'#/$defs/logicalPath'}}});
wire.$defs.requestEnvelope.properties.lint_request = {$ref:'#/$defs/LintRequest'};
wire.$defs.requestEnvelope.allOf = [{if:{properties:{operation:{const:'lint'}},required:['operation']},then:{required:['lint_request','target'],not:{anyOf:['ir_path','profile','profile_digest','profile_capabilities','native_request','dry_run','plan_id'].map(k=>({required:[k]}))}},else:{not:{required:['lint_request']}}}];
wire.$defs.operationResult.properties.lint_evidence = {$ref:'#/$defs/LintEvidence'};
wire.$defs.responseEnvelope.allOf = [{if:{properties:{operation:{const:'lint'}},required:['operation']},then:{not:{anyOf:['writes','capabilities','progress'].map(k=>({required:[k]}))},allOf:[{if:{properties:{status:{const:'ok'}},required:['status']},then:{required:['result'],properties:{result:{required:['lint_evidence'],not:{anyOf:['ok','truncated','entries','findings','bindings','native_plan'].map(k=>({required:[k]}))}}}},else:{required:['error'],not:{required:['result']}}}]},else:{properties:{result:{not:{required:['lint_evidence']}}}}}];
function strictConditions(v) {if(!v||typeof v!=='object')return; if(!Array.isArray(v)&&(v.required||v.properties)) {v.type??='object';v.properties??={};for(const k of v.required??[])v.properties[k]??={};} for(const x of Object.values(v))strictConditions(x);}
strictConditions(wire.$defs.requestEnvelope.allOf);strictConditions(wire.$defs.responseEnvelope.allOf);
const manifest = read('contracts/adapter-manifest.schema.v0.3.2.json');
function rewrite(v) {if(Array.isArray(v))return v.map(rewrite);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,x])=>[k,rewrite(x)]));return typeof v==='string'?v.replaceAll('adapter-manifest.schema.v0.3.2','adapter-manifest.schema.v0.6.4').replaceAll('adapter-manifest/v0.3.2','adapter-manifest/v0.6.4').replaceAll('adapter-manifest@0.3.2','adapter-manifest@0.6.4'):v;}
const successor = rewrite(manifest);
function operations(v) {if(!v||typeof v!=='object')return;if(Array.isArray(v.enum)&&v.enum.includes('describe')&&v.enum.includes('plan-native'))v.enum.push('lint');for(const x of Object.values(v))operations(x);}
operations(successor);
for (const [p,v] of [['contracts/target-protocol.schema.v0.6.4.json',wire],['contracts/adapter-manifest.schema.v0.6.4.json',successor]]) {
  const bytes=JSON.stringify(v,null,2)+'\n';if(process.argv.includes('--check')) {if(readFileSync(p,'utf8')!==bytes)throw new Error('successor drift '+p);}else writeFileSync(p,bytes);
}
