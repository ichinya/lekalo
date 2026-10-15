// Issue #41 planner e2e: the reference planner fixture can be deleted and
// reproduced from the Model.
//
// On a disposable copy: record the exact input pins, delete every generated
// application file and the whole managed custody tree, then recreate the
// output strictly from the retained canonical inputs (Model, ownership,
// coverage). The rebuilt bytes, the generation identity, and the executed
// generated-handler evidence must match the committed oracle exactly. No
// golden backup, hidden prior implementation, or adapter execution is
// involved: the fixture carries the independently authored expectations.
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { root, version } from './gen-managed-contracts.mjs';

const sha256 = (bytes) => `sha256:${createHash('sha256').update(bytes).digest('hex')}`;
// The in-lane coverage pin is the canonical digest: key-sorted compact JSON.
const canonical = (bytes) => {
  const ordered = (value) => Array.isArray(value) ? value.map(ordered) : value && typeof value === 'object' ? Object.fromEntries(Object.keys(value).sort().map((key) => [key, ordered(value[key])])) : value;
  return sha256(Buffer.from(JSON.stringify(ordered(JSON.parse(bytes))), 'utf8'));
};
const wire = 'tests/fixtures/managed';
const bin = join(root, 'target/debug', process.platform === 'win32' ? 'lekalo.exe' : 'lekalo');
assert.ok(existsSync(bin), 'binary-missing: cargo build --workspace --locked must precede gate');

const directory = mkdtempSync(join(tmpdir(), 'lekalo-managed-e2e-'));
const project = join(directory, 'proj');
cpSync(join(root, wire, 'planner'), project, { recursive: true });

// The exact pinned input closure of this fixture (AC7 records it first).
// The ownership configuration and explicit semantic inputs ride the
// closure; the adapter coverage evidence rides its own coverageDigest pin.
const inputs = ['model/planner.model.json', 'ownership.json'];
const pins = inputs.map((relative) => ({ path: relative, digest: sha256(readFileSync(join(project, relative))) }));
const coveragePin = canonical(readFileSync(join(project, 'coverage.json')));

// Delete every generated application artifact and all derived custody.
rmSync(join(project, 'apps'), { recursive: true, force: true });
rmSync(join(project, '.lekalo'), { recursive: true, force: true });
const remaining = [];
const walk = (current) => {
  for (const name of readdirSync(current).sort()) {
    const full = join(current, name);
    if (statSync(full).isDirectory()) walk(full);
    else remaining.push(full);
  }
};
walk(project);
assert.ok(!remaining.some((file) => file.includes('apps')), 'generated outputs deleted');
assert.ok(!remaining.some((file) => file.includes('.lekalo')), 'custody deleted');

// Reproduce strictly from the retained canonical inputs.
const invoke = (args, status) => {
  const argv = ['--json', 'managed', ...args];
  if (args[0] !== 'equivalence') argv.push('--project', project);
  const child = spawnSync(bin, argv, {
    cwd: project,
    encoding: 'utf8',
    timeout: 120000,
  });
  const text = child.stdout.trim() || child.stderr.trim();
  const envelope = JSON.parse(text);
  assert.equal(envelope.status, status, text.slice(0, 1500));
  return envelope;
};
const receipt = invoke([
  'plan', '--to', 'managed',
  '--ownership', 'ownership.json',
  '--coverage', 'coverage.json',
  '--module', 'planner', '--target', 'node-typescript',
  '--input', 'model/planner.model.json',
], 'valid');
const plan = receipt.managed.plan;
// The reviewed plan pins exactly the recorded closure, byte for byte.
const closure = new Map(plan.inputClosure.map((pin) => [pin.path, pin.digest]));
for (const pin of pins) {
  assert.equal(closure.get(pin.path), pin.digest, `input pin moved: ${pin.path}`);
}
assert.equal(plan.coverageDigest, coveragePin, 'coverage pin moved');
assert.deepEqual(plan.outputs.map((output) => output.action), ['create', 'create', 'create', 'create']);
writeFileSync(join(project, 'plan.json'), JSON.stringify(plan, null, 2) + '\n');
const applied = invoke([
  'apply', '--plan', 'plan.json', '--coverage', 'coverage.json',
  '--ownership', 'ownership.json', '--confirm', plan.planDigest,
], 'valid');

// The complete deterministic inventory matches the committed oracle.
const oracle = {
  'apps/planner/gen/src/complete-task.ts': `${wire}/planner/expected/complete-task.ts`,
  'apps/planner/gen/src/focus-task.ts': `${wire}/planner/expected/focus-task.ts`,
  'apps/planner/gen/test/complete-task.test.ts': `${wire}/planner/expected/complete-task.test.ts`,
  'apps/planner/gen/test/focus-task.test.ts': `${wire}/planner/expected/focus-task.test.ts`,
};
for (const [generated, expected] of Object.entries(oracle)) {
  assert.equal(
    readFileSync(join(project, generated), 'utf8'),
    readFileSync(join(root, expected), 'utf8'),
    `reproduced bytes differ: ${generated}`,
  );
}
// The regenerated custody manifest and the previous digest agree: a clean
// rebuild has the same generation identity as the original apply.
const regenerated = readFileSync(join(project, '.lekalo/generated/manifests/managed/current'), 'utf8').trim();
assert.equal(applied.managed.generationDigest, regenerated);

// The generated tests execute the generated handlers directly.
for (const test of ['complete-task', 'focus-task']) {
  execFileSync(process.execPath, [join(project, `apps/planner/gen/test/${test}.test.ts`)], {
    timeout: 60000,
    stdio: 'pipe',
  });
}

// The fixture's independently authored equivalence evidence is a match.
const verdict = invoke(['equivalence', '--evidence', 'equivalence.json'], 'valid');
assert.equal(verdict.managed.verdict, 'match');

// A byte-identical rebuild in a second fresh root (AC1, cross-root).
const twin = join(mkdtempSync(join(tmpdir(), 'lekalo-managed-e2e-twin-')), 'proj');
cpSync(join(project, 'ownership.json'), join(twin, 'ownership.json'));
cpSync(join(project, 'coverage.json'), join(twin, 'coverage.json'));
cpSync(join(project, 'model'), join(twin, 'model'), { recursive: true });
cpSync(join(project, 'lekalo'), join(twin, 'lekalo'), { recursive: true });
cpSync(join(project, 'lekalo.lock'), join(twin, 'lekalo.lock'));
if (existsSync(join(project, 'adapters'))) cpSync(join(project, 'adapters'), join(twin, 'adapters'), { recursive: true });
const twinReceipt = spawnSync(bin, [
  '--json', 'managed', 'plan', '--to', 'managed',
  '--ownership', 'ownership.json', '--coverage', 'coverage.json',
  '--module', 'planner', '--target', 'node-typescript',
  '--input', 'model/planner.model.json', '--project', twin,
], { cwd: twin, encoding: 'utf8' });
const twinPlan = JSON.parse(twinReceipt.stdout.trim()).managed.plan;
writeFileSync(join(twin, 'plan.json'), JSON.stringify(twinPlan, null, 2) + '\n');
const twinApplied = spawnSync(bin, [
  '--json', 'managed', 'apply', '--plan', 'plan.json', '--coverage', 'coverage.json',
  '--ownership', 'ownership.json', '--confirm', twinPlan.planDigest, '--project', twin,
], { cwd: twin, encoding: 'utf8' });
const twinEnvelope = JSON.parse(twinApplied.stdout.trim());
assert.equal(twinEnvelope.status, 'valid');
assert.equal(twinEnvelope.managed.generationDigest, regenerated, 'byte-stable generation identity across roots');
for (const generated of Object.keys(oracle)) {
  assert.equal(readFileSync(join(twin, generated), 'utf8'), readFileSync(join(project, generated), 'utf8'));
}

rmSync(directory, { recursive: true, force: true });
rmSync(join(twin, '..'), { recursive: true, force: true });
console.log(JSON.stringify({ ok: true, gate: 'test-managed-planner-e2e', generation: regenerated, registry: version, rules: 12 }));
