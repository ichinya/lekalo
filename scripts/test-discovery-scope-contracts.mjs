#!/usr/bin/env node
// Independent schema admission and real CLI receipts for issue #149.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
import {readFileSync,writeFileSync,mkdirSync,mkdtempSync,realpathSync,rmSync,existsSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname,resolve,join,basename} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const require=createRequire((process.env.LEKALO_AJV_NODE_PATH??root)+'/');
assert.equal(require('ajv/package.json').version,'8.20.0');
const Ajv=require('ajv/dist/2020').default;
const ajv=new Ajv({strict:true,allErrors:true});
const schema=name=>JSON.parse(readFileSync(join(root,`contracts/${name}.schema.v0.9.0.json`),'utf8'));
const validateInput=ajv.compile(schema('discovery-scope'));
const validateReceipt=ajv.compile(schema('discovery-scope-receipt'));
const input={schemaVersion:'lekalo/discovery-scope/v0.9.0',identity:'dev.lekalo.discovery-scope@0.9.0',includes:['app','app'],excludes:['archives'],gitIgnore:'respect'};
assert(validateInput(input),JSON.stringify(validateInput.errors));
let controls=0;
for(const changed of [
 {...input,schemaVersion:'lekalo/discovery-scope/v1.0.0'},
 {...input,identity:'unknown'}, {...input,includes:[]}, {...input,excludes:[17]},
 {...input,gitIgnore:'off'}, {...input,unknown:true}, {...input,includes:['../escape']},
 {...input,includes:['/absolute']}, {...input,includes:['a\\b']},
]){assert.equal(validateInput(changed),false,JSON.stringify(changed));controls++;}
const binary=process.env.LEKALO_BIN??join(root,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
const tempRoot=realpathSync.native(tmpdir());
const work=realpathSync.native(mkdtempSync(join(tempRoot,'lekalo-discovery-scope-')));
const env={...process.env};delete env.LEKALO_PROJECT;
const command=args=>{
 const result=spawnSync(binary,['--json','init','--adopt','--project','.','--project-id','scoped',...args],{cwd:work,env,encoding:'utf8',timeout:60000,maxBuffer:8*1024*1024});
 assert.ifError(result.error);assert.equal(result.status,0,result.stdout+result.stderr);
 const receipt=JSON.parse(result.stdout);assert.equal(receipt.status,'valid');
 assert(validateReceipt(receipt.detection.scope),JSON.stringify(validateReceipt.errors));
 return receipt;
};
try{
 mkdirSync(join(work,'app'));mkdirSync(join(work,'archives'));
 const packageBytes=JSON.stringify({name:'synthetic-app',devDependencies:{typescript:'1'},scripts:{test:'DO-NOT-EXECUTE'}})+'\n';
 writeFileSync(join(work,'app/package.json'),packageBytes);
 writeFileSync(join(work,'archives/package.json'),'{"name":"EXCLUDED-CONTENT-SENTINEL"}');
 writeFileSync(join(work,'.gitignore'),'archives/\n');
 writeFileSync(join(work,'discovery.json'),JSON.stringify(input));
 const preview=command(['--discovery-scope','discovery.json','--dry-run']);
 assert.equal(preview.detection.scope.source,'file');
 assert.deepEqual(preview.detection.scope.includes,['app']);
 assert.equal(preview.detection.scope.complete,true);
 assert.deepEqual(preview.detection.manifests.map(v=>v.path),['app/package.json']);
 assert(!JSON.stringify(preview).includes('EXCLUDED-CONTENT-SENTINEL'));
 assert(!existsSync(join(work,'lekalo')));
 const builtin=command(['--dry-run']);assert.equal(builtin.detection.scope.source,'builtin');
 assert.equal(builtin.detection.scope.input,null);
 const applied=command(['--discovery-scope','discovery.json']);
 const replay=command(['--discovery-scope','discovery.json']);
 assert.equal(applied.changed,true);assert.equal(replay.changed,false);
 assert.equal(preview.detection.scope.digest,applied.detection.scope.digest);
 assert.equal(applied.detection.scope.digest,replay.detection.scope.digest);
 assert.equal(readFileSync(join(work,'app/package.json'),'utf8'),packageBytes);
 assert.equal(readFileSync(join(work,'.gitignore'),'utf8'),'archives/\n');
 const child=structuredClone(preview.detection.scope);child.unknown=true;
 assert.equal(validateReceipt(child),false);controls++;
 console.log(JSON.stringify({ok:true,schemas:2,negativeControls:controls,liveReceipts:4,sourcePreserved:true}));
}finally{
 assert.equal(dirname(work),tempRoot);assert(basename(work).startsWith('lekalo-discovery-scope-'));
 assert.equal(realpathSync.native(work),work);
 rmSync(work,{recursive:true,force:true});
}
