import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {root,jobs,field,environment,assertRuntimeGate} from './lib/ci-workflow.mjs';
const read=p=>readFileSync(join(root,p),'utf8').replaceAll('\r\n','\n');
const digest=s=>createHash('sha256').update(s).digest('hex');
const expr=s=>'${{ '+s+' }}';
const consumers=['cli-core','node-typescript','php-laravel','python','sdk','docs','mcp','native-execution'];
export function checkPlatform({caller=read('.github/workflows/ci.yml'),callee=read('.github/workflows/ci-platform.yml'),inventory=JSON.parse(read('.github/ci/platform-gates.json'))}={}){
 const parent=jobs(caller),graph=jobs(callee),call=parent.platform;
 assert.equal(call?.uses,'./.github/workflows/ci-platform.yml');
 assert.deepEqual(call.needs,[]);assert(!field(call.raw,'if'));assert(!field(call.raw,'secrets'));
 assert.match(call.raw,/fail-fast: false\n/);
 assert.match(call.raw,/os: \[ubuntu-26\.04, windows-latest, macos-latest\]/);
 assert.equal(field(call.raw,'os',6),expr('matrix.os'));
 assert(!callee.includes('\nconcurrency:'),'Callee must not cancel caller');
 assert.match(callee,/permissions:\n  contents: read\n/);
 const cargo=environment(callee.slice(0,callee.indexOf('\njobs:')),0);
 assert.deepEqual(cargo,{CARGO_PROFILE_DEV_DEBUG:'0',CARGO_PROFILE_TEST_DEBUG:'0',CARGO_PROFILE_DEV_DEBUG_ASSERTIONS:'true',CARGO_PROFILE_TEST_DEBUG_ASSERTIONS:'true',CARGO_PROFILE_DEV_OVERFLOW_CHECKS:'true',CARGO_PROFILE_TEST_OVERFLOW_CHECKS:'true'});
 assert.deepEqual(Object.keys(graph).sort(),['build-debug','build-release','rust','rust-process',...consumers].sort());
 for(const job of Object.values(graph)){
  assert.equal(job.runner,expr('inputs.os'),`${job.id}: OS isolation`);
  assert(!field(job.raw,'if')&&!field(job.raw,'continue-on-error'),`${job.id}: mandatory job`);
  assert.match(job.raw,/defaults:\n      run:\n        shell: bash/);
  assert.equal(job.steps[0].uses,'actions/checkout@v7');
  assert(job.steps.some(s=>s.uses==='actions/setup-node@v7'));
  assert(!job.steps.some(s=>s.raw.includes('continue-on-error')),`${job.id}: optional step`);
 }
 for(const profile of ['debug','release']){
  const p=graph['build-'+profile];assert.deepEqual(p.needs,[]);
  assert.equal(p.env.RUNTIME_PROFILE,profile);assert.equal(p.env.RUNTIME_IMAGE,expr('inputs.os'));
  for(const [key,value] of Object.entries({'artifact-id':expr('steps.upload.outputs.artifact-id'),'manifest-sha256':expr('steps.package.outputs.manifest-sha256'),'producer-attempt':expr('steps.package.outputs.producer-attempt')}))assert.equal(field(p.raw,key,6),value);
  const build=p.steps.findIndex(s=>s.run?.includes('cargo build'));
  assert.equal(p.steps[build].run,profile==='debug'?'cargo build -p lekalo-cli --bin lekalo --locked\ncargo build -p lekalo-core --example typescript-native-contract --locked':'cargo build --release -p lekalo-cli --bin lekalo --locked');
  const pack=p.steps.findIndex(s=>s.run===`node scripts/ci-runtime-artifacts.mjs prepare "$RUNNER_TEMP/runtime-${profile}"`);
  const upload=p.steps.findIndex(s=>s.uses==='actions/upload-artifact@v7');
  assert(build<pack&&pack<upload);assert.equal(field(p.steps[pack].raw,'id',8),'package');assert.equal(field(p.steps[upload].raw,'id',8),'upload');
  assert.equal(field(p.steps[upload].raw,'name',10),`runtime-${profile}-${expr('inputs.os')}-${expr('github.run_attempt')}`);
  assert.equal(field(p.steps[upload].raw,'retention-days',10),'7');
  assert.equal(field(p.steps[upload].raw,'compression-level',10),'6','Runtime binaries must be compressed');
  assert.equal(field(p.steps[upload].raw,'if-no-files-found',10),'error');
  assert(p.steps.every(s=>!s.condition));
 }
 for(const owner of ['rust','rust-process']){
  const job=graph[owner];
  assert.deepEqual(job.needs,[]);assert(!job.steps.some(s=>s.uses?.startsWith('actions/download-artifact')));
  assert(job.steps.some(s=>s.run==='cargo build --workspace --locked'));
  for(const tool of ['shivammathur/setup-php@v2','actions/setup-go@v7'])assert(job.steps.some(s=>s.uses===tool),`${owner} parity needs ${tool}`);
  const lane=owner==='rust'?'core':'process';
  const runs=job.steps.filter(s=>s.run?.includes('node scripts/ci-rust-tests.mjs'));
  assert.equal(runs.length,1,`${owner}: one Rust lane invocation`);
  assert.equal(runs[0].run,`node scripts/ci-rust-tests.mjs --lane=${lane}`);
  assert(!runs[0].condition,`${owner}: conditional Rust lane`);
 }
 assert(!graph['rust-process'].steps.some(s=>s.run?.includes('cargo test')), 'Qualification must stay in core Rust lane');
 for(const owner of consumers){
  const j=graph[owner],profile=owner==='native-execution'?'release':'debug';
  // Apply the same handoff guard as live contract gates, using a unique marker.
  const marker='ci-owner-probe.mjs';const probe={...j,steps:[...j.steps,{run:`node scripts/${marker}`,env:{}}]};
  assertRuntimeGate(marker,{profile,graph:{[owner]:probe}});
  const verify=j.steps.findIndex(s=>s.run?.includes('ci-runtime-artifacts.mjs verify'));
  assert.equal(j.steps[verify].run,`node scripts/ci-runtime-artifacts.mjs verify "$RUNNER_TEMP/runtime-${profile}"`);
  assert.equal(field(j.steps.find(s=>s.uses==='actions/download-artifact@v8').raw,'path',10),`${expr('runner.temp')}/runtime-${profile}`);
  for(const s of j.steps)if(s.run?.includes('cargo build')||s.run?.includes('cargo test'))assert(['sdk','python'].includes(owner)||s.condition?.includes('failure()'),`Hidden Cargo in ${owner}`);
  if(['sdk','python'].includes(owner))assert(j.steps.some(s=>s.uses==='Swatinem/rust-cache@v2'&&field(s.raw,'cache-targets',10)==='false'),'A cache must not overwrite verified host binaries');
 }
 const seen=new Set();assert.equal(inventory.gates.length,91);
 const commands=new Map();
 for(const [owner,indices] of Object.entries(inventory.prerequisites))for(const index of indices){
  const s=inventory.setup.find(s=>s.index===index);assert(s,`Unknown setup ${index}`);
  const expected=s.raw.replaceAll(expr('matrix.os'),expr('inputs.os'));
  const position=graph[owner].steps.findIndex(s=>s.raw===expected);
  assert(position>=0,`${owner}: missing or altered prerequisite ${s.name??s.uses}`);
  const gates=inventory.gates.filter(g=>g.owner===owner).map(g=>graph[owner].steps.findIndex(s=>s.name===g.name));
  if(owner==='rust-process')gates.push(graph[owner].steps.findIndex(s=>s.run==='node scripts/ci-rust-tests.mjs --lane=process'));
  assert(gates.length>0&&gates.every(p=>p>=0),`${owner}: missing execution step`);
  if(index===104)assert(position>Math.max(...gates),`${owner}: premature cleanup`);
  else assert(position<Math.min(...gates),`${owner}: prerequisite runs after a gate`);
 }
 const caches=Object.values(graph).flatMap(j=>j.steps.filter(s=>s.uses==='Swatinem/rust-cache@v2').map(s=>field(s.raw,'shared-key',10)));
 assert.equal(new Set(caches).size,caches.length,'Cargo cache keys overlap between lanes');
 for(const gate of inventory.gates){
  assert(!seen.has(gate.id),'Duplicate inventory entry');seen.add(gate.id);
  let expected=gate.raw.replaceAll(expr('matrix.os'),expr('inputs.os'));
  if(gate.id==='build-test-075')expected=expected.replace('          cargo build -p lekalo-core --example typescript-native-contract --locked\n','');
  if(gate.id==='native-execution-10')expected=expected.replace('          cargo build --release -p lekalo-cli --locked\n','');
  if(gate.id==='native-execution-13')expected=expected.replace('name: native-acceptance-'+expr('inputs.os'),'name: native-acceptance-'+expr('inputs.os')+'-'+expr('github.run_attempt'));
  // The original test set is now partitioned between two mandatory jobs;
  // selectGroups/checkCoverage prove the union, and both commands are pinned above.
  if(gate.id==='build-test-056')expected=expected.replace('run: node scripts/ci-rust-tests.mjs','run: node scripts/ci-rust-tests.mjs --lane=core');
  const matches=Object.values(graph).flatMap(job=>job.steps.filter(s=>s.name===gate.name).map(s=>({job,s})));
  assert.equal(matches.length,1,`${gate.id}: one owner`);assert.equal(matches[0].job.id,gate.owner,`${gate.id}: wrong owner`);
  assert.equal(matches[0].s.raw,expected,`${gate.id}: gate arguments/env/conditions/timeout changed`);
  if(matches[0].s.run){const command=matches[0].s.run;commands.set(command,(commands.get(command)??0)+1);}
  if(['build-test-015','build-test-028'].includes(gate.id)){
   const owner=matches[0].job,position=owner.steps.indexOf(matches[0].s),setup=owner.steps[position-1];
   assert.equal(setup.uses,'dtolnay/rust-toolchain@stable');assert.equal(setup.condition,gate.condition,'Diagnostic toolchain must be failure-only');
  }
 }
 for(const [command,count] of commands)assert.equal(Object.values(graph).flatMap(j=>j.steps).filter(s=>s.run===command).length,count,'Unexpected repeated gate command');
 for(const [id,hash] of Object.entries(inventory.unchangedJobs))assert.equal(digest(parent[id]?.raw??''),hash,`Unrelated job ${id} changed`);
 assert.deepEqual(Object.keys(parent).sort(),['platform',...Object.keys(inventory.unchangedJobs)].sort());
 assert.equal(digest(caller.slice(0,caller.indexOf('\njobs:'))),inventory.callerPreambleSha256,'Event/env/permission policy changed');
 assert.equal(digest(read('.github/workflows/release.yml')),inventory.releaseSha256,'Release policy changed');
 return {ok:true,gates:seen.size,platforms:3,jobsPerPlatform:Object.keys(graph).length};
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){try{console.log(JSON.stringify(checkPlatform()));}catch(e){console.error(e.message);process.exitCode=1;}}
