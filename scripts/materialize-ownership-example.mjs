#!/usr/bin/env node
// Explicit no-overwrite tutorial export. Never changes a consumer checkout.
import assert from 'node:assert/strict';
import {existsSync,mkdirSync,writeFileSync} from 'node:fs';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {materializeOwnershipExample,qualifyOwnershipCoverage} from './lib/ownership-example.mjs';
const args=process.argv.slice(2);
assert(args.length===2 && args[0]==='--output','usage: --output NEW_DIRECTORY');
const output=resolve(args[1]);
assert(!existsSync(output),'output already exists');
mkdirSync(dirname(output),{recursive:true});
mkdirSync(output); // Exclusive creation also refuses a dangling-link collision.
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const bin=process.env.LEKALO_BIN ?? join(root,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
assert(existsSync(bin),'build the matching checkout CLI');
const {request}=materializeOwnershipExample(output);
qualifyOwnershipCoverage(output,bin);
const write=(name,value)=>writeFileSync(join(output,name),JSON.stringify(value,null,2)+'\n');
write('to-contracted.json',request);
write('to-managed.json',{...request,to:'managed',declaration:null,coverage:'coverage.json'});
write('to-maintained.json',{...request,to:'contracted',declaration:null});
console.log(JSON.stringify({ok:true,output,scenarios:request.runner.expected.length,sourceRewritten:false}));
