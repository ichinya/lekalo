#!/usr/bin/env node
// Real CLI + confined native execution. Infrastructure refusal is a failure,
// never evidence of behavioral equivalence or a completed ownership handoff.
import assert from 'node:assert/strict';
import {materializeOwnershipExample,qualifyOwnershipCoverage} from './lib/ownership-example.mjs';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const bin = process.env.LEKALO_BIN ?? join(root, 'target/debug', process.platform === 'win32' ? 'lekalo.exe' : 'lekalo');
assert(existsSync(bin), 'build the matching checkout CLI');
const home = mkdtempSync(join(tmpdir(), 'lekalo-ownership-111-'));
const sha = bytes => 'sha256:' + createHash('sha256').update(bytes).digest('hex');
const canonical = value => Array.isArray(value) ? value.map(canonical) : value && typeof value==='object'
  ? Object.fromEntries(Object.keys(value).sort().map(key=>[key,canonical(value[key])])) : value;
let calls = 0;
function invoke(stage, args, accepted = true, reason = null) {
  calls++;
  const result = spawnSync(bin, ['--no-cache', '--json', ...args, '--project', '.'], {
    cwd: stage, encoding: 'utf8', timeout: 180_000, windowsHide: true,
    env: { ...process.env, LEKALO_PROJECT: '' }, maxBuffer: 16 * 1024 * 1024,
  });
  if (accepted) assert.equal(result.status, 0, result.stderr || result.stdout || String(result.error));
  else assert.notEqual(result.status, 0, 'unsafe operation unexpectedly succeeded');
  const text = (result.stdout || result.stderr).trim();
  const doc = JSON.parse(text);
  if(reason) assert(doc.reasonCodes?.includes(reason),JSON.stringify(doc));
  return doc.ownership ?? doc.payload?.ownership ?? doc;
}
function json(stage, path, value) {
  mkdirSync(dirname(join(stage, path)), { recursive: true });
  writeFileSync(join(stage, path), JSON.stringify(value, null, 2) + '\n');
}
function fixture(name) { return materializeOwnershipExample(join(home,name)); }

function prepare(stage, request) {
  json(stage, 'transition-request.json', request);
  const plan = invoke(stage, ['ownership', 'plan', '--request', 'transition-request.json']);
  json(stage, 'transition-plan.json', plan);
  return plan;
}
function commit(stage, plan) { return invoke(stage, ['ownership', 'apply', '--plan', 'transition-plan.json', '--confirm', plan.planDigest]); }
function qualify(stage) { qualifyOwnershipCoverage(stage,bin); }

try {
  const {stage, request} = fixture('complete');
  const maintained = readFileSync(join(stage, 'src/complete_task.mjs'));
  let plan = prepare(stage, request);
  assert(!existsSync(join(stage, 'lekalo/ownership-state.json')), 'preview writes nothing');
  assert.equal(invoke(stage, ['ownership', 'verify', '--plan', 'transition-plan.json']).verdict, 'match');
  const contracted = commit(stage, plan);
  assert.equal(contracted.state, 'committed');
  assert(readFileSync(join(stage, 'src/complete_task.mjs')).equals(maintained));
  assert.equal(commit(stage, plan).state, 'replayed');
  const status = invoke(stage,['ownership','status']);
  assert.equal(status.evidenceBasis,'declared-ownership');
  assert.equal(status.assignments[0].mode,'contracted');
  const trace = invoke(stage,['ownership','trace','--receipt',contracted.planDigest]);
  assert.equal(trace.authority,'derived');
  assert.equal(trace.current,true);
  assert(trace.verification.observations.every(row=>!Object.hasOwn(row,'output') && row.expectedDigest===row.candidateDigest));
  assert.equal(JSON.parse(readFileSync(join(stage, 'lekalo/ownership.json'))).modules.find(row=>row.module==='legacy').mode, 'observed');
  qualify(stage);
  const managedRequest = {...request, to: 'managed', declaration: null, coverage: 'coverage.json'};
  const coverageBytes = readFileSync(join(stage,'coverage.json'));
  const unsupported = JSON.parse(coverageBytes);
  unsupported.requirements[0].support='unsupported';
  json(stage,'coverage.json',unsupported);
  json(stage,'transition-request.json',managedRequest);
  invoke(stage,['ownership','plan','--request','transition-request.json'],false,'managed.capability-gap');
  writeFileSync(join(stage,'coverage.json'),coverageBytes);
  plan = prepare(stage, managedRequest);
  assert(plan.claims.length > 0);
  const managed = commit(stage, plan);
  assert.equal(managed.to, 'managed');
  assert(readFileSync(join(stage, 'src/complete_task.mjs')).equals(maintained), 'no early source deletion');
  const generated = readFileSync(join(stage, plan.candidate[0].path));
  const downgrade = prepare(stage, {...request, to: 'contracted', declaration: null});
  commit(stage, downgrade);
  assert(readFileSync(join(stage, plan.candidate[0].path)).equals(generated), 'materialized maintained bytes survive');
  const state = JSON.parse(readFileSync(join(stage, 'lekalo/ownership-state.json')));
  assert(state.assignments[0].artifacts.every(row => row.lifecycle === 'scaffolded'));
  const manifest = JSON.parse(readFileSync(join(stage,'.lekalo/generated/manifests/ownership.json')));
  assert(manifest.artifacts.every(row=>row.lifecycle==='scaffolded' && row.regeneration_policy==='once'));
  const cleanup=invoke(stage,['generate','--clean','--dry-run']);
  assert.equal(cleanup.count,0,'scaffolded native bytes never become generic clean candidates');
  assert(readFileSync(join(stage,plan.candidate[0].path)).equals(generated));
  assert.equal(invoke(stage,['ownership','trace','--receipt',contracted.planDigest]).current,false,'old evidence is historical');
  invoke(stage, ['ownership', 'rollback', '--receipt', downgrade.planDigest, '--confirm', downgrade.planDigest]);
  assert.equal(JSON.parse(readFileSync(join(stage, 'lekalo/ownership.json'))).modules.find(row=>row.module==='planner').mode, 'managed');
  const repeated=prepare(stage,{...request,to:'contracted',declaration:null});
  assert.equal(repeated.planDigest,downgrade.planDigest,'exact rollback permits the same prepared handoff');
  const reapplied=commit(stage,repeated);
  assert(reapplied.previousReceipt,'reapply retains the prior receipt in its audit chain');
  const receiptTrace=invoke(stage,['ownership','trace','--receipt',repeated.planDigest]);
  const priorReceipt=JSON.parse(readFileSync(join(stage,'.lekalo/generated/manifests/ownership-transitions/objects',receiptTrace.previousReceipt.slice(7))));
  assert.equal(priorReceipt.state,'rolled-back');assert(priorReceipt.previousReceipt);

  const stale = fixture('stale');
  const stalePlan = prepare(stale.stage, stale.request);
  writeFileSync(join(stale.stage, 'src/complete_task.mjs'), 'export function completeTask() { throw Error("mutant"); }\n');
  invoke(stale.stage, ['ownership', 'apply', '--plan', 'transition-plan.json', '--confirm', stalePlan.planDigest], false,'ownership.plan-stale');
  assert(!existsSync(join(stale.stage, 'lekalo/ownership-state.json')));

  const partial = fixture('partial');
  partial.request.symbol = 'planner.complete_task';
  partial.request.runner.expected = partial.request.runner.expected.filter(row => row.scenario === partial.request.symbol);
  const declaration = JSON.parse(readFileSync(join(partial.stage, 'declaration.json')));
  declaration.symbols = declaration.symbols.filter(row => row.id === partial.request.symbol);
  json(partial.stage, 'declaration.json', declaration);
  const untouched = readFileSync(join(partial.stage, 'src/focus_task.mjs'));
  commit(partial.stage, prepare(partial.stage, partial.request));
  assert(readFileSync(join(partial.stage, 'src/focus_task.mjs')).equals(untouched));
  assert.equal(JSON.parse(readFileSync(join(partial.stage, 'lekalo/ownership.json'))).modules.find(row=>row.module==='planner').mode, 'observed');
  qualify(partial.stage);
  commit(partial.stage,prepare(partial.stage,{...partial.request,to:'managed',declaration:null,coverage:'coverage.json'}));
  const unselectedPath=plan.candidate.find(row=>row.symbol==='planner.focus_task').path;
  assert(!existsSync(join(partial.stage,unselectedPath)),'no implicit symbol widening');
  assert(readFileSync(join(partial.stage,'src/focus_task.mjs')).equals(untouched));
  json(partial.stage,'transition-request.json',{...partial.request,symbol:null});
  invoke(partial.stage,['ownership','plan','--request','transition-request.json'],false,'ownership.scope-refused');

  for(const [label,path] of [['model','lekalo/modules/planner/commands.yaml'],['lock','lekalo.lock'],['scenarios','scenarios.json']]) {
    const probe=fixture(`stale-${label}`);const prepared=prepare(probe.stage,probe.request);
    writeFileSync(join(probe.stage,path),Buffer.concat([readFileSync(join(probe.stage,path)),Buffer.from('\n')]));
    invoke(probe.stage,['ownership','apply','--plan','transition-plan.json','--confirm',prepared.planDigest],false,'ownership.plan-stale');
    assert(!existsSync(join(probe.stage,'lekalo/ownership-state.json')));
  }

  const unknown=fixture('unknown-behavior');unknown.request.unknownBehavior=['planner.foreign-port'];
  json(unknown.stage,'transition-request.json',unknown.request);
  invoke(unknown.stage,['ownership','plan','--request','transition-request.json'],false,'ownership.scope-refused');
  assert(!existsSync(join(unknown.stage,'lekalo/ownership-state.json')));

  const adapter=fixture('stale-adapter');commit(adapter.stage,prepare(adapter.stage,adapter.request));qualify(adapter.stage);
  const adapterPlan=prepare(adapter.stage,{...adapter.request,to:'managed',declaration:null,coverage:'coverage.json'});
  writeFileSync(join(adapter.stage,'adapters/managed.mjs'),readFileSync(join(adapter.stage,'adapters/managed.mjs'),'utf8')+'\n');
  invoke(adapter.stage,['ownership','apply','--plan','transition-plan.json','--confirm',adapterPlan.planDigest],false,'ownership.plan-stale');

  const mutant=fixture('mutant');
  writeFileSync(join(mutant.stage,'src/complete_task.mjs'),'export function completeTask(input) { return {id:input.id,done:false}; }\n');
  const mutantDeclaration=JSON.parse(readFileSync(join(mutant.stage,'declaration.json')));
  mutantDeclaration.symbols[0].fingerprint=sha(readFileSync(join(mutant.stage,'src/complete_task.mjs')));
  json(mutant.stage,'declaration.json',mutantDeclaration);
  const mutantPlan=prepare(mutant.stage,mutant.request);
  const mismatch=invoke(mutant.stage,['ownership','verify','--plan','transition-plan.json'],false,'ownership.verification-failed');
  assert.equal(mismatch.verdict,'mismatch');assert.equal(mismatch.counterexample,'planner.complete_task');
  invoke(mutant.stage,['ownership','apply','--plan','transition-plan.json','--confirm',mutantPlan.planDigest],false,'ownership.verification-failed');
  assert(!existsSync(join(mutant.stage,'lekalo/ownership-state.json')));

  const forged=fixture('forged-plan');const forgedPlan=prepare(forged.stage,forged.request);
  forgedPlan.edits.push({path:'src/injected.mjs',before:null,beforeContent:null,after:sha(Buffer.from('injected')),content:'injected'});
  const unsigned=structuredClone(forgedPlan);delete unsigned.planDigest;
  forgedPlan.planDigest=sha(Buffer.from(JSON.stringify(canonical(unsigned))));
  json(forged.stage,'transition-plan.json',forgedPlan);
  invoke(forged.stage,['ownership','apply','--plan','transition-plan.json','--confirm',forgedPlan.planDigest],false,'ownership.plan-stale');
  assert(!existsSync(join(forged.stage,'src/injected.mjs')));

  const runtime=fixture('runtime-pin');runtime.request.runner.digest='sha256:'+'a'.repeat(64);
  prepare(runtime.stage,runtime.request);
  invoke(runtime.stage,['ownership','verify','--plan','transition-plan.json'],false,'ownership.plan-stale');

  const shared=fixture('shared-source');shared.request.symbol='planner.complete_task';
  shared.request.runner.expected=shared.request.runner.expected.filter(row=>row.scenario===shared.request.symbol);
  const sharedDeclaration=JSON.parse(readFileSync(join(shared.stage,'declaration.json')));
  sharedDeclaration.symbols[1].source=sharedDeclaration.symbols[0].source;
  sharedDeclaration.symbols[1].fingerprint=sharedDeclaration.symbols[0].fingerprint;
  json(shared.stage,'declaration.json',sharedDeclaration);json(shared.stage,'transition-request.json',shared.request);
  invoke(shared.stage,['ownership','plan','--request','transition-request.json'],false,'ownership.scope-refused');

  const immutable=fixture('immutable');
  const runnerPath=join(immutable.stage,'runner.mjs');
  writeFileSync(runnerPath,readFileSync(runnerPath,'utf8').replace('const observed = [];',`writeFileSync('src/complete_task.mjs','export function completeTask(input) { return {done:input.done,id:input.id}; }');\nconst observed = [];`));
  const immutablePlan=prepare(immutable.stage,immutable.request);
  invoke(immutable.stage,['ownership','apply','--plan','transition-plan.json','--confirm',immutablePlan.planDigest],false,'ownership.execution-unavailable');
  assert(!existsSync(join(immutable.stage,'lekalo/ownership-state.json')));

  const conflict=fixture('rollback-conflict');const conflictPlan=prepare(conflict.stage,conflict.request);
  commit(conflict.stage,conflictPlan);
  writeFileSync(join(conflict.stage,'src/complete_task.mjs'),'user edit\n');
  invoke(conflict.stage,['ownership','apply','--plan','transition-plan.json','--confirm',conflictPlan.planDigest],false,'ownership.ownership-conflict');
  assert.equal(invoke(conflict.stage,['ownership','trace','--receipt',conflictPlan.planDigest]).current,false);
  invoke(conflict.stage,['ownership','rollback','--receipt',conflictPlan.planDigest,'--confirm',conflictPlan.planDigest],false,'ownership.rollback-conflict');
  assert.equal(readFileSync(join(conflict.stage,'src/complete_task.mjs'),'utf8'),'user edit\n');
  assert(!existsSync(join(conflict.stage,'.lekalo/generated/manifests/ownership-transitions/pending.json')),'predictable rollback conflict leaves no pending journal');
  assert.equal(JSON.parse(readFileSync(join(conflict.stage,'lekalo/ownership-state.json'))).pending,null);

  const imported=fixture('legacy-import');
  const legacyPolicy=JSON.parse(readFileSync(join(imported.stage,'lekalo/ownership.json')));
  legacyPolicy.modules.find(row=>row.module==='planner').mode='managed';
  json(imported.stage,'lekalo/ownership.json',legacyPolicy);qualify(imported.stage);
  const legacyPlan=invoke(imported.stage,['managed','plan','--ownership','lekalo/ownership.json','--coverage','coverage.json',
    '--module','planner','--target','node-typescript','--to','managed']).managed.plan;
  json(imported.stage,'legacy-plan.json',legacyPlan);
  invoke(imported.stage,['managed','apply','--ownership','lekalo/ownership.json','--coverage','coverage.json',
    '--plan','legacy-plan.json','--confirm',legacyPlan.planDigest]);
  const importedDeclaration=JSON.parse(readFileSync(join(imported.stage,'declaration.json')));
  for(const symbol of importedDeclaration.symbols){
    const stem=symbol.id.split('.').at(-1).replaceAll('_','-');
    const output=legacyPlan.outputs.find(row=>row.role==='implementation' && row.path.endsWith(`/${stem}.ts`));
    assert(output,'legacy implementation output');symbol.source={path:output.path,line:1};
    symbol.fingerprint=sha(readFileSync(join(imported.stage,output.path)));
  }
  json(imported.stage,'declaration.json',importedDeclaration);
  const legacyBefore=readFileSync(join(imported.stage,'.lekalo/generated/manifests/ownership.json'));
  const importedPlan=prepare(imported.stage,{...imported.request,to:'contracted',coverage:'coverage.json'});
  commit(imported.stage,importedPlan);
  assert.equal(invoke(imported.stage,['generate','--clean','--dry-run']).count,0);
  invoke(imported.stage,['ownership','rollback','--receipt',importedPlan.planDigest,'--confirm',importedPlan.planDigest]);
  assert(!existsSync(join(imported.stage,'lekalo/ownership-state.json')));
  assert(readFileSync(join(imported.stage,'.lekalo/generated/manifests/ownership.json')).equals(legacyBefore));
  invoke(imported.stage,['managed','check','--ownership','lekalo/ownership.json','--coverage','coverage.json']);

  console.log(JSON.stringify({ok:true, gate:'ownership-transitions-native', calls, fullLifecycle:true, partialSymbol:true,
    staleRefused:['source','model','lock','scenarios','adapter'],unsupportedRefused:true,unknownBehaviorRefused:true,realMutantRefused:true,immutableSources:true,rollbackConflict:true,neutralTrace:true,
    reapplyAfterRollback:true,receiptLineage:true,legacyManagedImport:true}));
} finally { rmSync(home, {recursive:true,force:true}); }
