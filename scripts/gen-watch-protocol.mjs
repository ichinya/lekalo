#!/usr/bin/env node
// Additive 0.8.0 selection seam; never rewrite frozen predecessor schemas.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
const wire=JSON.parse(readFileSync('contracts/target-protocol.schema.v0.6.4.json','utf8'));
wire.$id='https://dev.lekalo/target-protocol.schema.v0.8.0.json';
wire.description+=' Issue #95: explicitly declared read-only watch selection partitions. Full captured IR is resolution context only; module work validates/scans only the named partition (including removed symbols), global work only mandatory cross-module obligations. Every selected response echoes the SHA-256 of the canonical selection bytes. Legacy calls have no selection or echo.';
wire.$defs.protocolVersion={const:'0.8.0'};
const sha={type:'string',pattern:'^sha256:[0-9a-f]{64}$'};
const closed=(properties,required=Object.keys(properties))=>({type:'object',additionalProperties:false,properties,required});
const token={type:'string',minLength:1,maxLength:256,pattern:'^[a-z][a-z0-9_-]*(\\.[a-z][a-z0-9_-]*)*$'};
wire.$defs.WatchSelection={
 ...closed({source_revision:sha,scope:{enum:['module','global']},module:token,symbols:{type:'array',maxItems:4096,uniqueItems:true,items:token},changed_paths:{type:'array',maxItems:4096,uniqueItems:true,items:{$ref:'#/$defs/logicalPath'}}},['source_revision','scope','symbols','changed_paths']),
 allOf:[{if:{type:'object',properties:{scope:{const:'module'}},required:['scope']},then:{type:'object',properties:{module:token},required:['module']},else:{type:'object',properties:{symbols:{type:'array',maxItems:0}},not:{type:'object',properties:{module:{}},required:['module']}}}]
};
wire.$defs.capabilities.properties.watch=closed({selection:{const:true}});
wire.$defs.requestEnvelope.properties.watch_request={$ref:'#/$defs/WatchSelection'};
wire.$defs.requestEnvelope.allOf.push({if:{type:'object',properties:{watch_request:{}},required:['watch_request']},then:{type:'object',properties:{operation:{enum:['scan','validate']},target:token},required:['target']}});
wire.$defs.evidence.properties.watch_digest=sha;
const path='contracts/target-protocol.schema.v0.8.0.json';
const bytes=JSON.stringify(wire,null,2)+'\n';
if(process.argv.includes('--check'))assert.equal(readFileSync(path,'utf8'),bytes,'watch protocol drift');
else writeFileSync(path,bytes);
