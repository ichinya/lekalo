#!/usr/bin/env node
// Explicit authoring recipe for issue #95's registry-dependent fixtures.
// Never run in CI: tests independently reproduce these bytes with the CLI.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {cpSync,mkdirSync,mkdtempSync,readFileSync,rmSync,writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const bin=process.env.LEKALO_BIN??join(root,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
const env={...process.env};delete env.LEKALO_PROJECT;
const run=(cwd,args)=>execFileSync(bin,args,{cwd,env,encoding:'utf8',timeout:120000,maxBuffer:16*1024*1024});
const json=(cwd,args)=>JSON.parse(run(cwd,['--json','--no-cache',...args]));
const hash=bytes=>'sha256:'+createHash('sha256').update(bytes).digest('hex');
const stable=value=>Array.isArray(value)?value.map(stable):value&&typeof value==='object'?Object.fromEntries(Object.keys(value).sort().map(k=>[k,stable(value[k])])):value;
const work=mkdtempSync(join(tmpdir(),'lekalo-watch-registry-'));
const output=[];
const save=(path,bytes)=>{writeFileSync(join(root,path),bytes);output.push(path);};
const fixture=(name,path)=>{const dir=join(work,name);cpSync(join(root,path),dir,{recursive:true});return dir;};
assert.equal(json(root,['--version']).version,'0.8.0','build the current CLI first');
try {
 const lockRoot=fixture('lock','tests/fixtures/lockfile/project');
 json(lockRoot,['lock']);
 const bytes=readFileSync(join(lockRoot,'lekalo.lock'));
 const lockDigest=hash(bytes.subarray(0,-1));
 save('tests/fixtures/lockfile/valid/contract-only.lock.json',bytes);
 save('tests/fixtures/lockfile/valid/contract-only.expect.json',JSON.stringify({lockDigest,payloadSha256:lockDigest.slice(7)})+'\n');
 const orch=fixture('orchestration','tests/fixtures/orchestration/project');
 const adapter=['--','node','adapters/node-typescript/node-adapter.mjs'];
 json(orch,['lock',...adapter]);
 for(const [name,args] of [
  ['generate.dry-run',['generate','--target','node-typescript','--dry-run']],
  ['generate.apply',['generate','--target','node-typescript']],
  ['verify.full',['verify','--target','node-typescript']],
 ]) {
  const receipt=json(orch,[...args,...adapter]);assert.equal(receipt.verdict,'ready');
  for(const target of receipt.targets??[])if(Object.hasOwn(target,'planId'))target.planId='plan-normalized';
  save(`tests/fixtures/orchestration/${name}.golden.json`,JSON.stringify(receipt,null,2)+'\n');
 }
 for(const [name,args] of [['verify',['verify','--locked']],['generate-check',['generate','--check']]]) {
  const project=fixture(name,'tests/fixtures/loader/valid-direct-visibility');
  execFileSync('git',['init','-q'],{cwd:project,env});
  execFileSync('git',['-c','user.name=Fixture author','-c','user.email=fixture@example.invalid','commit','--allow-empty','-qm','fixture'],{cwd:project,env});
  json(project,['lock']);mkdirSync(join(project,'out'));
  json(project,[...args,'--project','.','--report-file','out/report.json']);
  const path=`tests/fixtures/ci-report/valid.${name}.golden.json`;
  const prior=JSON.parse(readFileSync(join(root,path),'utf8'));
  const live=JSON.parse(readFileSync(join(project,'out/report.json'),'utf8'));
  for(const pin of ['commit','dirty'])live.provenance.git[pin]=prior.provenance.git[pin];
  const report=JSON.stringify(stable(live))+'\n';save(path,report);
  if(name==='verify') {
   const mdPath='tests/fixtures/ci-report/valid.verify.golden.md';
   const md=readFileSync(join(root,mdPath),'utf8').replace(/\| lock \| `[^`]+` \|/,`| lock | \`${live.provenance.lock.digest.value}\` |`).replace(/report digest: `[^`]+`/,`report digest: \`${hash(report)}\``);
   save(mdPath,md);
  }
 }
 // Keep the derived SARIF receipt bound to its committed blocked report.
 const sarifPath='tests/fixtures/ci-report/valid.sarif.golden.sarif';
 const blocked=readFileSync(join(root,'tests/fixtures/ci-report/valid.validate-blocked.golden.json'));
 save(sarifPath,readFileSync(join(root,sarifPath),'utf8').replace(/"lekaloReportDigest":"sha256:[0-9a-f]{64}"/,`"lekaloReportDigest":"${hash(blocked)}"`));
 // Reproduce the provider test's custom-artifact manifest with fresh
 // canonical Model/IR and lock pins; the manifest digest changes too.
 const provider=fixture('provider','tests/fixtures/artifacts/project');
 json(provider,['lock']);
 const model=json(provider,['load']),ir=json(provider,['load','--ir']);
 const providerLock=readFileSync(join(provider,'lekalo.lock'));
 const content='export function focusHook() {\n  return planner.focusTask();\n}\n';
 const manifest={schema_version:'lekalo/artifact-manifest/v0.2.16',identity:'dev.lekalo.artifact-manifest@0.2.16',project_ref:'planner',
  lock_ref:{schema_version:'lekalo/lock/v0.3.2',digest:hash(providerLock.subarray(0,-1))},
  inputs:{model:{version:model.modelVersion,digest:hash(JSON.stringify(model))},ir:{version:ir.modelVersion,digest:hash(JSON.stringify(ir.ir))}},
  artifacts:[{semantic_owner:'planner.focus_task',path:'apps/api/src/planner/focus-hook.ts',artifact_kind:'source',lifecycle:'custom',content:{algorithm:'sha256',digest:hash(content),canonicalization:'exact-file-bytes'},input_refs:['planner.focus_task'],regeneration_policy:'manual-only'}],source_maps:[]};
 manifest.manifest_digest=hash(JSON.stringify(stable(manifest)));
 mkdirSync(join(provider,'.lekalo/generated/manifests'),{recursive:true});
 writeFileSync(join(provider,'.lekalo/generated/manifests/ownership.json'),JSON.stringify(stable(manifest)));
 mkdirSync(join(provider,'apps/api/src/planner'),{recursive:true});
 writeFileSync(join(provider,'apps/api/src/planner/focus-hook.ts'),"export function focusHook() {\n  return 'drifted';\n}\n");
 const receipt=json(provider,['generate','--check']);assert.equal(receipt.verdict,'reported');
 save('tests/fixtures/provider/drift-reported.golden.json',JSON.stringify(receipt,null,2)+'\n');
 console.log(JSON.stringify({ok:true,lockDigest,files:output}));
} finally {rmSync(work,{recursive:true,force:true});}
