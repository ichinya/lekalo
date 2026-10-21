import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {mkdtempSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import test from 'node:test';

const sdk=new URL('../src/index.mjs',import.meta.url).href;

test('a missing conformance executable rejects without terminating the caller',()=>{
 const script=`
  import assert from 'node:assert/strict';
  import {runConformance} from ${JSON.stringify(sdk)};
  await assert.rejects(runConformance({executable:'lekalo-pr160-missing-executable',adapterArgs:[]}),{code:'ENOENT'});
  process.stdout.write('launch error handled');
 `;
 const result=spawnSync(process.execPath,['--input-type=module','-e',script],{encoding:'utf8',timeout:10000});
 assert.ifError(result.error);
 assert.equal(result.status,0,result.stderr);
 assert.equal(result.stdout,'launch error handled');
 assert.equal(result.stderr,'');
});

for(const exitCode of [0,7]) {
 test(`conformance preserves stdout, stderr, argv and exit ${exitCode}`,()=>{
  const scratch=mkdtempSync(join(tmpdir(),'lekalo-conformance-client-'));
  try {
   // Node executes this fake CLI as `node adapter test ...` on every OS.
   writeFileSync(join(scratch,'adapter'),`process.stdout.write(JSON.stringify(process.argv.slice(2)));process.stderr.write('fixture stderr');process.exitCode=${exitCode};`);
   const script=`
    import {runConformance} from ${JSON.stringify(sdk)};
    const result=await runConformance({executable:process.execPath,profile:'strict',report:'json',repeats:3,timeoutMs:1000,adapterArgs:['--','adapter with spaces']});
    process.stdout.write(JSON.stringify(result));
   `;
   const result=spawnSync(process.execPath,['--input-type=module','-e',script],{cwd:scratch,encoding:'utf8',timeout:10000});
   assert.ifError(result.error);
   assert.equal(result.status,0,result.stderr);
   const response=JSON.parse(result.stdout);
   assert.equal(response.code,exitCode);
   assert.equal(response.stderr,'fixture stderr');
   assert.deepEqual(JSON.parse(response.stdout),['test','--profile','strict','--report','json','--repeats','3','--timeout-ms','1000','--','adapter with spaces']);
  } finally {
   rmSync(scratch,{recursive:true,force:true});
  }
 });
}
