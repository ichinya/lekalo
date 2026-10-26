#!/usr/bin/env node
// Additive public-native-inventory contracts; published predecessors stay frozen.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
const version='1.1.0';
assert.equal(readFileSync('Cargo.toml','utf8').match(/\[workspace.package\][\s\S]*?version = "([^"]+)"/)[1],version);
const ref=name=>({'$ref':'#/$defs/'+name});
const string=(max)=>({type:'string',minLength:1,maxLength:max,pattern:'^[^\\u0000-\\u001f\\u007f-\\u009f]+$'});
function native(pathSchema,shaSchema){
  return {type:'object',additionalProperties:false,required:['native','qualified_name','family','path','line','end_line','slot','declaration_only'],properties:{
    native:string(256),qualified_name:string(256),
    family:{type:'string',maxLength:128,pattern:'^(class|interface|enum|type-alias|function|method|variable|property|module|symbol)(\\+(class|interface|enum|type-alias|function|method|variable|property|module|symbol))*$'},
    path:pathSchema,line:{type:'integer',minimum:1,maximum:1000000},end_line:{type:'integer',minimum:1,maximum:1000000},
    slot:{enum:['static','instance']},declaration_only:{type:'boolean'},member_of:string(256),signature:shaSchema
  }};
}
function save(path,obj){const bytes=JSON.stringify(obj,null,2)+'\n';if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),bytes,path+' drift');else writeFileSync(path,bytes);}
function uncertainty(pathSchema,shaSchema){return {type:'object',additionalProperties:false,required:['path','kind','detail','source_digest'],properties:{path:pathSchema,kind:{const:'any-surface'},detail:string(64),line:{type:'integer',minimum:1,maximum:1000000},source_digest:shaSchema}};}
const wire=JSON.parse(readFileSync('contracts/target-protocol.schema.v0.9.1.json','utf8'));
wire.$id='https://dev.lekalo/target-protocol.schema.v1.1.0.json';
wire.description+=' Successor 1.1.0 carries a complete bounded native declaration subtree per scan entry, preserving lexical ownership, exact names, source positions and structural digests. Root/cycle/owner/span consistency is additionally validated by the runtime. Old contracts retain their original members and refusals.';
wire.$defs.protocolVersion={const:version};
const sha={type:'string',pattern:'^sha256:[a-f0-9]{64}$'};
wire.$defs.nativeSymbol=native(wire.$defs.scanEntry.properties.path,sha);
wire.$defs.nativeUncertainty=uncertainty(wire.$defs.scanEntry.properties.path,sha);
wire.$defs.evidence.properties.native_uncertainty={type:'array',maxItems:4096,items:ref('nativeUncertainty')};
wire.$defs.scanEntryEvidence.properties.native_symbols={type:'array',minItems:1,maxItems:256,items:ref('nativeSymbol')};
wire.$defs.scanEntryEvidence.properties.references.maxItems=256;
wire.$defs.responseEnvelope.allOf.push({
  if:{type:'object',properties:{operation:{const:'scan'},status:{const:'ok'}},required:['operation','status']},
  then:{type:'object',properties:{result:{type:'object',properties:{entries:{type:'array',items:{type:'object',properties:{evidence:{type:'object',required:['native_symbols'],properties:{native_symbols:{type:'array',minItems:1}}}}}}}}}},
  else:{type:'object',properties:{result:{type:'object',properties:{entries:{type:'array',items:{type:'object',properties:{evidence:{type:'object',not:{type:'object',required:['native_symbols'],properties:{native_symbols:{}}}}}}}}}}}
});
wire.$defs.responseEnvelope.allOf.push({
  if:{type:'object',properties:{operation:{const:'scan'},status:{const:'ok'}},required:['operation','status']},
  then:{type:'object',properties:{evidence:{type:'object',required:['native_uncertainty'],properties:{native_uncertainty:{type:'array'}}}}},
  else:{type:'object',properties:{evidence:{type:'object',not:{type:'object',required:['native_uncertainty'],properties:{native_uncertainty:{}}}}}}
});
save('contracts/target-protocol.schema.v1.1.0.json',wire);
for(const kind of ['scan','index']){
  const schema=JSON.parse(readFileSync('contracts/observed-'+kind+'.schema.v0.2.16.json','utf8'));
  schema.$id=schema.$id.replace('v0.2.16','v1.1.0');schema.description+=' The explicitly versioned native inventory and per-candidate evidence are source-bound scan facts; they do not infer business semantics.';
  if(kind==='scan')schema.properties.schemaVersion.const='lekalo/observed-scan/v1.1.0';
  else{schema.properties.schema_version.const='lekalo/observed-index/v1.1.0';schema.properties.identity.const='dev.lekalo.observed-index@1.1.0';}
  schema.$defs.nativeSymbol=native({type:'string',minLength:1,maxLength:512},ref('sha256'));
  schema.$defs.nativeUncertainty=uncertainty({type:'string',minLength:1,maxLength:512},ref('sha256'));
  schema.$defs.bindingEvidence={type:'object',additionalProperties:false,required:['symbol','native','evidence'],properties:{symbol:ref('semanticId'),native:string(256),evidence:ref('evidence')}};
  const members=kind==='scan'?['nativeSymbols','bindingEvidence']:['native_symbols','binding_evidence'];
  schema.properties[members[0]]={type:'array',maxItems:10000,items:ref('nativeSymbol')};
  schema.properties[members[1]]={type:'array',maxItems:10000,items:ref('bindingEvidence')};
  if(kind==='scan')schema.required.push(...members);
  schema.properties[kind==='scan'?'nativeUncertainty':'native_uncertainty']={type:'array',maxItems:4096,items:ref('nativeUncertainty')};
  if(kind==='scan')schema.required.push('nativeUncertainty');
  save('contracts/observed-'+kind+'.schema.v1.1.0.json',schema);
}
console.log(JSON.stringify({ok:true,version,contracts:3,check:process.argv.includes('--check')}));
