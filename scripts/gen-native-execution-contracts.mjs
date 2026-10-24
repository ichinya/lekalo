#!/usr/bin/env node
// Explicit authoring recipe; --check is the only CI mode.
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync} from 'node:fs';
const version = '0.9.1';
const object = properties => ({type:'object', additionalProperties:false, required:Object.keys(properties), properties});
const digest = {type:'string', pattern:'^sha256:[0-9a-f]{64}$'};
const token = {type:'string', pattern:'^[a-z0-9][a-z0-9._-]{0,127}$'};
const release = {type:'string', pattern:'^[0-9]+\\.[0-9]+\\.[0-9]+$', maxLength:32};
const array = (items, maxItems, minItems=1) => ({type:'array', minItems, maxItems, uniqueItems:true, items});
function embed(schema, source, prefix) {
  const rewrite = value => Array.isArray(value) ? value.map(rewrite) : value && typeof value === 'object'
    ? Object.fromEntries(Object.entries(value).map(([key,item])=>[key,rewrite(item)]))
    : typeof value === 'string' && value.startsWith('#/$defs/') ? value.replace('#/$defs/', `#/$defs/${prefix}_`) : value;
  schema.$defs ??= {};
  for (const [key,value] of Object.entries(source.$defs)) schema.$defs[`${prefix}_${key}`] = rewrite(value);
  schema.$defs[`${prefix}_Document`] = rewrite(Object.fromEntries(Object.entries(source).filter(([key])=>!['$schema','$id','$defs'].includes(key))));
  return {$ref:`#/$defs/${prefix}_Document`};
}
function schema(family, properties) {
  return {$schema:'https://json-schema.org/draft/2020-12/schema', $id:`https://dev.lekalo/native-${family}.schema.v${version}.json`,
    title:`Lekalo native ${family} ${version}`, ...object({schema_version:{const:`lekalo/native-${family}/v${version}`}, ...properties})};
}
const policy = schema('execution-policy', {allow_execution:{type:'boolean'}, platforms:{const:['windows']}, runtime_profile:{const:'windows-node-lpac-job-v1'}, planning_policy:{}});
policy.properties.planning_policy = embed(policy, JSON.parse(readFileSync('contracts/native-gate-policy.schema.v0.4.0.json','utf8')), 'Policy');
const catalog = schema('tool-catalog', {tools:array(object({id:token, program:{type:'string', minLength:1, maxLength:4096}, artifact_digest:digest, program_digest:digest, version:release}),32)});
const plan = schema('execution-plan', {kind:{const:'native-execution-plan'}, plan_digest:digest, native_plan:{},
  custody:object(Object.fromEntries(['policy_digest','profile_digest','catalog_digest','adapter_manifest_digest','adapter_package_digest','adapter_entry_digest','source_manifest_digest'].map(key=>[key,digest]))),
  inputs:array(object({path:{$ref:'#/$defs/Plan_logicalPath'},digest}),4096),
  tools:array(object({id:token,artifact_digest:digest,program_digest:digest,version:release}),32)});
plan.properties.native_plan = embed(plan, JSON.parse(readFileSync('contracts/native-gate-plan.schema.v0.4.0.json','utf8')), 'Plan');
const stream={oneOf:[object({state:{const:'known'},text:{type:'string',maxLength:65536},truncated:{type:'boolean'}}),object({state:{enum:['unknown','withheld']}})]};
const output=schema('command-output',{stdout:stream,stderr:stream});
for (const [family, document] of Object.entries({'execution-policy':policy, 'tool-catalog':catalog, 'execution-plan':plan, 'command-output':output})) {
  const path = `contracts/native-${family}.schema.v${version}.json`, bytes = JSON.stringify(document,null,2)+'\n';
  if (process.argv.includes('--check')) assert.equal(readFileSync(path,'utf8'),bytes,path+' drift');
  else writeFileSync(path,bytes);
}
