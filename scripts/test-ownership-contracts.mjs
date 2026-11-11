#!/usr/bin/env node
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFileSync} from 'node:fs';
import {ownershipSchemas} from './lib/ownership-contracts.mjs';
import {ACCEPTED_M8_REGISTRY_FAMILIES,assertRegistryUnion} from './lib/registry-union.mjs';
const require=createRequire(import.meta.url);
assert.equal(require('ajv/package.json').version,'8.20.0');
const Ajv=require('ajv/dist/2020').default;
const ajv=new Ajv({strict:true,allErrors:true});
const read=path=>JSON.parse(readFileSync(new URL('../'+path,import.meta.url)));
for(const [family,expected]of Object.entries(ownershipSchemas)) {
 const actual=read(`contracts/${family}.schema.v1.0.0.json`);
 assert.deepEqual(actual,expected,`contract drift: ${family}`);
 ajv.compile(actual);
}
const validate=ajv.getSchema(ownershipSchemas['ownership-transition-request'].$id);
const pin='sha256:'+'a'.repeat(64);
const request={schemaVersion:'lekalo/ownership-transition-request/v1.0.0',module:'planner',symbol:null,to:'contracted',
 inputs:['runner.mjs'],coverage:null,declaration:'declaration.json',unknownBehavior:[],runner:{program:'node',digest:pin,args:['runner.mjs'],
 expected:[{scenario:'planner.focus',output:{id:'t-1'},error:null,state:null,effects:[],authorization:null,transaction:null}]}};
assert(validate(request),JSON.stringify(validate.errors));
let refused=0;
for(const change of [
 x=>delete x.symbol,x=>x.extra=true,x=>x.runner.program='shell',x=>x.runner.digest='latest',
 x=>x.runner.expected=[],x=>x.inputs=Array(513).fill('runner.mjs'),x=>x.inputs=['../outside'],
 x=>x.inputs=['C:/outside'],x=>x.inputs=['.git/config'],x=>x.inputs=['.bb/notes'],
 x=>x.runner.expected[0].extra=true,x=>delete x.runner.expected[0].transaction,
 x=>x.schemaVersion='lekalo/ownership-transition-request/v2.0.0',
]) {
 const bad=structuredClone(request);change(bad);assert(!validate(bad),'invalid request admitted');refused++;
}
const predecessor=read('contracts/diagnostic-registry.v0.6.5.json');
const registry=read('contracts/diagnostic-registry.v0.8.0.json');
assertRegistryUnion(predecessor,registry,{families:ACCEPTED_M8_REGISTRY_FAMILIES,ownPrefix:'ownership.',ownCount:12});
assert.equal(new Set(registry.entries.filter(row=>row.id.startsWith('ownership.')).map(row=>row.code)).size,12);
console.log(JSON.stringify({ok:true,gate:'ownership-contracts',families:Object.keys(ownershipSchemas).length,negativeControls:refused,diagnostics:12}));
