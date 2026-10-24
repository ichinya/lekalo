#!/usr/bin/env node
// Explicit authoring only; CI uses --check.
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync} from 'node:fs';
import {resolve, dirname, join} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const version='0.9.0';
const string={type:'string',minLength:1,maxLength:4096};
const segment='(?:[A-Za-z0-9]|[A-Za-z0-9.][A-Za-z0-9._-]{0,62}[A-Za-z0-9_-])';
const rootPath={type:'string',minLength:1,maxLength:1024,pattern:`^(?:\\.|${segment}(?:/${segment})*)$`,description:'Literal repository-relative path, or dot; hidden names and actual source capitalization are admitted. Shared escape/device rules and case-accurate physical no-follow checks additionally apply.'};
const digest={type:'string',pattern:'^sha256:[0-9a-f]{64}$'};
const pin={type:'object',additionalProperties:false,required:['path','sha256'],properties:{path:string,sha256:digest}};
const scope={
  type:'object',additionalProperties:false,
  required:['schemaVersion','identity','includes','excludes','gitIgnore'],
  properties:{schemaVersion:{const:`lekalo/discovery-scope/v${version}`},identity:{const:`dev.lekalo.discovery-scope@${version}`},includes:{type:'array',minItems:1,maxItems:32,items:rootPath},excludes:{type:'array',maxItems:64,items:rootPath},gitIgnore:{const:'respect'}},
};
const reasons=['protected','excluded','outside-includes','gitignored','physical-entry-refused','non-portable-directory','depth-limit','entry-limit','directory-unreadable','manifest-read-or-parse-refused'];
const limits=['skip-receipt-limit','physical-entry-refused','non-portable-directory','depth-limit','entry-limit','directory-unreadable','manifest-read-or-parse-refused','collection-limit'];
const receipt={
  type:'object',additionalProperties:false,
  required:['schemaVersion','identity','digest','source','input','includes','excludes','gitIgnore','ignoreFiles','complete','limits','skipped','skippedTruncated'],
  properties:{
    schemaVersion:{const:`lekalo/discovery-scope-receipt/v${version}`},identity:{const:`dev.lekalo.discovery-scope-receipt@${version}`},digest,
    source:{enum:['builtin','file']},input:{anyOf:[{type:'null'},pin]},
    includes:{type:'array',minItems:1,maxItems:32,uniqueItems:true,items:rootPath},excludes:{type:'array',maxItems:64,uniqueItems:true,items:rootPath},gitIgnore:{const:'respect'},
    ignoreFiles:{type:'array',maxItems:128,items:pin},complete:{type:'boolean'},limits:{type:'array',uniqueItems:true,items:{enum:limits}},
    skipped:{type:'array',maxItems:128,items:{type:'object',additionalProperties:false,required:['path','reason'],properties:{path:string,reason:{enum:reasons}}}},skippedTruncated:{type:'boolean'},
  },
  allOf:[{if:{properties:{source:{const:'builtin'}},required:['source']},then:{properties:{input:{type:'null'}}},else:{properties:{input:pin}}}],
};
for(const [family,body]of [['discovery-scope',scope],['discovery-scope-receipt',receipt]]){
 const value={$schema:'https://json-schema.org/draft/2020-12/schema',$id:`https://lekalo.dev/schemas/${family}/v${version}`,title:`${family} v${version}`,...body};
 const file=join(root,`contracts/${family}.schema.v${version}.json`),bytes=JSON.stringify(value,null,2)+'\n';
 if(process.argv.includes('--check'))assert.equal(readFileSync(file,'utf8'),bytes,`generated drift: ${family}`);else writeFileSync(file,bytes);
}
console.log(JSON.stringify({ok:true,version,mode:process.argv.includes('--check')?'check':'write'}));
