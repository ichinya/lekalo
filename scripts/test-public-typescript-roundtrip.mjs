#!/usr/bin/env node
// #167 regression: real public CLI + unchanged manifested bundle, no inference.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {cpSync,existsSync,mkdtempSync,readFileSync,readdirSync,rmSync,writeFileSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {tmpdir} from 'node:os';
import {basename,dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
const repo=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const binary=process.env.LEKALO_BIN??join(repo,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
const adapter=join(repo,'adapters/node-typescript/adapter.mjs');
const fixtures=join(repo,'tests/fixtures/public-typescript-scan/planner');
const require=createRequire((process.env.LEKALO_AJV_NODE_PATH??repo)+'/');
assert.equal(require('ajv/package.json').version,'8.20.0');
const Ajv=require('ajv/dist/2020').default;
const schema=version=>new Ajv({strict:true,allErrors:true}).compile(JSON.parse(readFileSync(join(repo,'contracts/observed-index.schema.v'+version+'.json'),'utf8')));
const nativeIndex=schema('1.1.0'),oldIndex=schema('0.2.16');
function checkIndex(index){assert.ok(nativeIndex(index),JSON.stringify(nativeIndex.errors));assert.equal(oldIndex(index),false,'new fields require their exact successor contract');}
const root=mkdtempSync(join(tmpdir(),'lekalo-native-roundtrip-')),receipts=[];
assert.ok(existsSync(binary),'build the checkout CLI first');
assert.ok(Number(process.versions.node.split('.')[0])>=24,'native planner tests require Node 24');
const hash=b=>'sha256:'+createHash('sha256').update(b).digest('hex');
function inventory(dir,prefix=''){return readdirSync(join(dir,prefix),{withFileTypes:true}).flatMap(e=>{const p=prefix?prefix+'/'+e.name:e.name;return e.isDirectory()?inventory(dir,p):[[p,hash(readFileSync(join(dir,p)))]];});}
function command(program,args,expected=0){const r=spawnSync(program,args,{cwd:root,encoding:'utf8',timeout:120000,maxBuffer:16e6,windowsHide:true});assert.equal(r.status,expected,args.slice(0,6).join(' ')+'\n'+r.stdout+'\n'+r.stderr);return r;}
function cli(args,expected=0){const r=command(binary,['--json','--no-cache',...args],expected);const doc=JSON.parse(r.stdout.trim()||r.stderr.trim());receipts.push({args,exit:r.status,result:doc});return doc;}
const profile={id:'standalone',mode:'observed',target:'node-typescript',readRoots:[{kind:'file',path:'package.json'},{kind:'file',path:'tsconfig.json'},{kind:'tree',path:'src'}],exclusions:[],provenance:{origin:'declared',revision:'native-roundtrip-v1',disposition:'public-fixture'}};
const scan=(expected=0)=>cli(['scan','--target','node-typescript','--profile','standalone','--project','.',process.execPath,adapter,'--lekalo-project-profile-json',JSON.stringify(profile)],expected);
const indexPath=join(root,'.lekalo/import/observed/index.json');
function confirmedPlan(){const plan=cli(['bindings','confirm','--batch','--preview','--project','.']);assert.ok(plan.entries.length);return plan;}
function denied(args,id){const before=readFileSync(indexPath);const r=cli(args,1);assert.ok(r.diagnostics.some(d=>d.id===id),JSON.stringify(r));assert.deepEqual(readFileSync(indexPath),before);return r;}
try {
  cpSync(join(fixtures,'baseline'),root,{recursive:true});
  const before=inventory(root);
  command(process.execPath,['--test','test/public.test.mjs']);
  cli(['init','--adopt','--project-id','native_planner','--project','.']);
  cli(['module','new','synthetic_priority_planner','--project','.']);
  cli(['validate','--strict','--project','.']);
  const first=scan();assert.equal(first.symbols,14);
  const initial=JSON.parse(readFileSync(indexPath,'utf8'));
  checkIndex(initial);
  assert.equal(initial.schema_version,'lekalo/observed-index/v1.1.0');
  assert.equal(initial.native_symbols.length,38);
  assert.equal(initial.native_symbols.filter(s=>s.member_of).length,24);
  assert.ok(initial.native_uncertainty.length>0,'unknown type evidence must remain explicit');
  assert.ok(initial.native_uncertainty.every(u=>u.kind==='any-surface'));
  for(const mutate of [v=>{v.native_symbols[0].signature=null;},v=>{v.native_symbols[0].invented=true;},v=>{v.native_uncertainty[0].source_digest='invalid';}]){
    const malformed=structuredClone(initial);mutate(malformed);assert.equal(nativeIndex(malformed),false);
  }
  const taskId='synthetic_priority_planner.Task';
  const proposals=cli(['bindings','propose','--project','.']);
  const oldTaskProposal=proposals.proposals.find(p=>p.symbol===taskId).proposal;
  assert.equal(proposals.proposals.length,14);
  const plan=confirmedPlan();
  cli(['bindings','confirm','--batch','--confirm',plan.plan,'--project','.']);
  cli(['bindings','audit','--project','.']);
  const card=cli(['observe','inspect',taskId,'--project','.']);
  assert.ok(card.native_symbols.some(s=>s.qualified_name==='Task.id'));
  assert.ok(!card.native_symbols.some(s=>s.qualified_name==='Task.priority'));
  const memory=cli(['observe','inspect','synthetic_priority_planner.MemoryStore','--project','.']);
  assert.ok(memory.native_symbols.some(s=>s.qualified_name==='MemoryStore.get.t'));
  assert.ok(memory.native_uncertainty.length);
  assert.equal(memory.completeness,'incomplete');
  const create=cli(['observe','inspect','synthetic_priority_planner.createTask','--project','.']);
  assert.ok(create.native_symbols.some(s=>s.qualified_name==='createTask.position'));
  // Read-only diagnostics and unchanged scans never mutate an accepted index.
  const stable=readFileSync(indexPath);const repeated=scan();assert.equal(repeated.revision,first.revision);assert.deepEqual(readFileSync(indexPath),stable);
  for(const [p,digest]of before)assert.equal(hash(readFileSync(join(root,p))),digest,'baseline changed by adoption '+p);
  // Real priority implementation from the original public synthetic pilot.
  cpSync(join(fixtures,'priority-src'),join(root,'src'),{recursive:true});
  command(process.execPath,['--test','test/public.test.mjs']);
  if(existsSync(join(fixtures,'oracle.mjs')))command(process.execPath,[join(fixtures,'oracle.mjs'),root]);
  denied(['bindings','audit','--project','.'],'observed.stale-binding');
  const changed=scan();
  assert.ok(changed.staled.includes(taskId),'interface field changes require reconfirmation');
  const pending=cli(['observe','inspect',taskId,'--project','.']);
  assert.equal(pending.state,'stale');
  assert.equal(pending.fingerprint,card.fingerprint,'scan may not silently replace confirmed shape evidence');
  assert.ok(pending.native_symbols.some(s=>s.qualified_name==='Task.priority'),'fresh native declarations remain visible during review');
  assert.notEqual(pending.evidence.signature,pending.native_symbols.find(s=>s.qualified_name==='Task').signature);
  const fresh=cli(['bindings','propose','--project','.']).proposals.find(p=>p.symbol===taskId);
  assert.ok(fresh,'changed confirmed binding must have a fresh review proposal');
  assert.equal(fresh.source,'user-confirmed');
  denied(['bindings','confirm',oldTaskProposal,'--project','.'],'bindings.proposal-unknown');
  const taskPath=join(root,'src/domain/task.ts'),newBytes=readFileSync(taskPath);
  writeFileSync(taskPath,Buffer.concat([newBytes,Buffer.from('\n')]));
  denied(['bindings','confirm',fresh.proposal,'--project','.'],'bindings.plan-mismatch');
  writeFileSync(taskPath,newBytes);
  const review=confirmedPlan();
  cli(['bindings','confirm','--batch','--confirm',review.plan,'--project','.']);
  const current=cli(['bindings','audit','--project','.']);
  assert.equal(current.unknown,0);assert.equal(current.current,changed.symbols);
  const accepted=cli(['observe','inspect',taskId,'--project','.']);
  assert.equal(accepted.state,'current');assert.notEqual(accepted.fingerprint,card.fingerprint);
  assert.equal(accepted.evidence.signature,accepted.native_symbols.find(s=>s.qualified_name==='Task').signature);
  const after=readFileSync(indexPath),again=scan();assert.equal(again.revision,changed.revision);assert.deepEqual(readFileSync(indexPath),after,'reconfirmed shape must not become stale again');
  checkIndex(JSON.parse(after));
  // An incomplete future scan never overwrites the last accepted inventory.
  writeFileSync(taskPath,Buffer.concat([newBytes,Buffer.from('\nimport {missing} from "./outside.ts";\n')]));
  denied(['scan','--target','node-typescript','--profile','standalone','--project','.',process.execPath,adapter,'--lekalo-project-profile-json',JSON.stringify(profile)],'target.operation-failed');
  writeFileSync(taskPath,newBytes);
  cli(['bindings','audit','--project','.']);
  if(process.env.LEKALO_ROUNDTRIP_RECEIPT)writeFileSync(process.env.LEKALO_ROUNDTRIP_RECEIPT,JSON.stringify({gate:'public-typescript-roundtrip',receipts},null,2)+'\n');
  console.log(JSON.stringify({ok:true,gate:'public-typescript-roundtrip',baselineRoots:14,baselineNative:38,baselineNested:24,changedRoots:changed.symbols,confirmed:current.current,sourcePreserved:true,publicCli:true,inference:false}));
} finally {
  assert.ok(basename(root).startsWith('lekalo-native-roundtrip-'));assert.equal(dirname(root),resolve(tmpdir()));
  rmSync(root,{recursive:true,force:true});
}
