#!/usr/bin/env node
// Issue #156: a successor for preprovisioned Node tool entry/asset custody.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
const object=properties=>({type:'object',additionalProperties:false,required:Object.keys(properties),properties});
const digest={type:'string',pattern:'^sha256:[0-9a-f]{64}$'};
const release={type:'string',pattern:'^[0-9]+\\.[0-9]+\\.[0-9]+$',maxLength:32};
const token={type:'string',pattern:'^[a-z0-9][a-z0-9._-]{0,127}$'};
const path={type:'string',minLength:1,maxLength:512,pattern:'^[A-Za-z0-9@_.+-]+(?:/[A-Za-z0-9@_.+-]+)*$'};
const array=(items,maxItems,minItems=1)=>({type:'array',items,minItems,maxItems,uniqueItems:true});
const schema=(family,properties)=>({$schema:'https://json-schema.org/draft/2020-12/schema',$id:`https://dev.lekalo/native-${family}.schema.v1.0.0.json`,title:`Lekalo native ${family} 1.0.0`,...object({schema_version:{const:`lekalo/native-${family}/v1.0.0`},...properties})});
const asset=object({path,digest,bytes:{type:'integer',minimum:0,maximum:32*1024*1024}});
const catalog=schema('tool-catalog',{
  runtime:object({program:{type:'string',minLength:1,maxLength:4096},program_digest:digest,version:release}),
  assets:object({root:{type:'string',minLength:1,maxLength:4096},manifest_digest:digest,files:array(asset,4096)}),
  tools:array(object({id:token,entry:path,entry_digest:digest,artifact_digest:digest,version:release}),32),
});
const plan=JSON.parse(readFileSync('contracts/native-execution-plan.schema.v0.9.1.json','utf8'));
plan.$id='https://dev.lekalo/native-execution-plan.schema.v1.0.0.json';plan.title='Lekalo native execution plan 1.0.0';
plan.properties.schema_version.const='lekalo/native-execution-plan/v1.0.0';
plan.required.push('assets');
plan.properties.assets=object({manifest_digest:digest,file_count:{type:'integer',minimum:1,maximum:4096},total_bytes:{type:'integer',minimum:1,maximum:128*1024*1024},runtime_version:release});
Object.assign(plan.properties.tools.items.properties,{entry:path,entry_digest:digest});plan.properties.tools.items.required.push('entry','entry_digest');
function embed(target,source,prefix){
  const rewrite=value=>Array.isArray(value)?value.map(rewrite):value&&typeof value==='object'?Object.fromEntries(Object.entries(value).map(([k,v])=>[k,rewrite(v)])):typeof value==='string'&&value.startsWith('#/$defs/')?value.replace('#/$defs/',`#/$defs/${prefix}_`):value;
  target.$defs??={};for(const [key,value]of Object.entries(source.$defs??{}))target.$defs[`${prefix}_${key}`]=rewrite(value);
  target.$defs[`${prefix}_Document`]=rewrite(Object.fromEntries(Object.entries(source).filter(([key])=>!['$schema','$id','$defs'].includes(key))));
  return {$ref:`#/$defs/${prefix}_Document`};
}
const runTools=JSON.parse(readFileSync('contracts/native-gate-run.schema.v0.4.0.json','utf8'));
runTools.$id='https://dev.lekalo/native-gate-run.schema.v1.0.0.json';runTools.title='Lekalo native run with explicit tool output paths 1.0.0';
for(const name of ['runRequest','runResult'])runTools.$defs[name].properties.schema_version.const='lekalo/native-gate-run/v1.0.0';
runTools.description='Terminal receipt for declared tools with separately bounded relative output paths; predecessor source/cwd and command semantics remain unchanged.';
delete runTools.oneOf;runTools.$ref='#/$defs/runResult';
runTools.$defs.toolOutputPath=path;
runTools.$defs.commandResult.properties.cwd={oneOf:[{const:'.'},{$ref:'#/$defs/logicalPath'}]};
for(const name of ['created','modified'])runTools.$defs.mutationSummary.properties[name].items.properties.path={$ref:'#/$defs/toolOutputPath'};
for(const name of ['deleted','unexpected'])runTools.$defs.mutationSummary.properties[name].items={$ref:'#/$defs/toolOutputPath'};
const report=schema('execution-report',{kind:{const:'native-execution-report'},run:{},outputs:array(object({digest,output:{}}),128,0)});
report.properties.run={oneOf:[embed(report,JSON.parse(readFileSync('contracts/native-gate-run.schema.v0.4.0.json','utf8')),'Run'),embed(report,runTools,'RunTools')]};
report.properties.outputs.items.properties.output=embed(report,JSON.parse(readFileSync('contracts/native-command-output.schema.v0.9.1.json','utf8')),'Output');
const verify=schema('verify-report',{kind:{const:'native-verify-report'},status:{enum:['passed','failed','security','infrastructure','unsupported','not-run','denied']},verdict:{enum:['ready','blocked','degraded']},semantic:{type:'object'},semantic_after:{oneOf:[object({state:{const:'not-run'}}),object({state:{const:'checked'},report:{type:'object'}})]},native:{},exit_code:{type:'integer',enum:[0,1,3,4,5]},reason_codes:array({enum:['semantic-after-refused','semantic-custody-changed']},2,0)});
verify.properties.native={oneOf:[embed(verify,report,'Execution'),object({state:{const:'not-run'},reason:{type:'string',maxLength:128}}),object({state:{const:'error'},execution:{const:'unverified'},diagnostic:{type:'object'}})]};
for(const [family,value]of Object.entries({'tool-catalog':catalog,'execution-plan':plan,'gate-run':runTools,'execution-report':report,'verify-report':verify})){
  const path=`contracts/native-${family}.schema.v1.0.0.json`,bytes=JSON.stringify(value,null,2)+'\n';
  if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),bytes,path+' drift');else writeFileSync(path,bytes);
}
