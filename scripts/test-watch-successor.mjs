#!/usr/bin/env node
// Exact successor payload golden plus a real negotiated target cycle.
// Refusal goldens execute through the production Rust client as well as
// test-watch-contracts: schema acceptance alone cannot prove input custody.
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {cpSync,mkdtempSync,readFileSync,readdirSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {selectiveTargetProbe} from './lib/watch-target-probe.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const binary=process.env.LEKALO_BIN??join(root,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
const env={...process.env};delete env.LEKALO_PROJECT;
const project=mkdtempSync(join(tmpdir(),'lekalo-watch-successor-'));
function inventory(dir,prefix=''){
 return readdirSync(dir,{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name)).flatMap(e=>{
  assert(e.isFile()||e.isDirectory());const path=prefix+e.name;
  return e.isDirectory()?inventory(join(dir,e.name),path+'/'):[[path,readFileSync(join(dir,e.name)).toString('hex')]];
 });
}
try{
 cpSync(join(root,'tests/fixtures/watch/project-successor'),project,{recursive:true});
 const before=inventory(project);
 const run=spawnSync(binary,['watch','--backend','poll','--validate','--impact','--json-lines','--stop-after-runs','1'],{cwd:project,env,encoding:'utf8',timeout:30000,maxBuffer:16*1024*1024});
 assert.ifError(run.error);assert.equal(run.status,0,run.stdout+run.stderr);
 const result=run.stdout.trim().split(/\r?\n/).map(JSON.parse).find(e=>e.event==='run-completed').result;
 const golden=JSON.parse(readFileSync(join(root,'tests/fixtures/watch/golden/successor-result.json'),'utf8'));
 assert.deepEqual(result,golden,'complete successor payload, digests and version pins are deterministic');
 assert.deepEqual(inventory(project),before,'watch cannot write the successor source or derived state');
 const boundary=JSON.parse(readFileSync(join(root,'tests/fixtures/watch/golden/platform-qualification.json'),'utf8'));
 assert.equal(boundary.deliveryEvidence.macos,'native-runtime-unavailable-no-macos-host');
 assert.equal(boundary.crossCompilationIsRuntimeEvidence,false);
 assert.equal(boundary.windowsTestsAreMacosEvidence,false);
 assert.deepEqual(boundary.requiredCiPlatforms,['ubuntu-latest','windows-latest','macos-latest']);
 const docs=readFileSync(join(root,'docs/target-protocol.md'),'utf8');
 assert(docs.includes('platform-qualification.json'),'runtime evidence boundary is owned and documented');
 console.log(JSON.stringify({ok:true,platform:process.platform,successorGolden:true,selectiveTarget:await selectiveTargetProbe({root,binary,env,fixture:'project-successor'}),macosRuntime:'not-claimed'}));
}finally{rmSync(project,{recursive:true,force:true});}
