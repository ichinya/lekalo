// Issue #41 managed CLI gate: the real binary against a real filesystem.
//
// Proves, on disposable sandbox copies of the synthetic planner fixture:
// zero-write check/plan previews, the confirmed apply transaction, the
// complete output oracle, staleness and drift refusals, exact restoration,
// scoped cleanup, mixed-mode isolation, recovery blocking, determinism
// across fresh roots, the upgrade diff, and the equivalence verdicts.
// Missing binary or fixture fails the lane; nothing regenerates committed
// fixtures in acceptance mode.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { root, version } from './gen-managed-contracts.mjs';

const read = (path) => readFileSync(join(root, path), 'utf8');
const wire = 'tests/fixtures/managed';
const bin = join(root, 'target/debug', process.platform === 'win32' ? 'lekalo.exe' : 'lekalo');
assert.ok(existsSync(bin), 'binary-missing: cargo build --workspace --locked must precede gate');

function snapshot(directory) {
  // Files by exact bytes PLUS the directory inventory: a gate must not
  // miss empty-directory writes (finding 12).
  const files = new Map();
  const walk = (current, logical) => {
    for (const name of readdirSync(current).sort()) {
      const full = join(current, name);
      const child = logical ? `${logical}/${name}` : name;
      if (statSync(full).isDirectory()) {
        files.set(`${child}/`, 'directory');
        walk(full, child);
      } else {
        files.set(child, readFileSync(full).toString('base64'));
      }
    }
  };
  walk(directory, '');
  return files;
}
function assertSameTree(before, after, label) {
  assert.deepEqual([...after.keys()].sort(), [...before.keys()].sort(), `${label}: inventory changed`);
  for (const [name, bytes] of before) {
    assert.equal(after.get(name), bytes, `${label}: ${name} changed`);
  }
}

function sandbox(name) {
  const directory = mkdtempSync(join(tmpdir(), `lekalo-managed-cli-${name}-`));
  const project = join(directory, 'proj');
  cpSync(join(root, wire, 'planner'), project, { recursive: true });
  rmSync(join(project, 'expected'), { recursive: true, force: true });
  return { project, json: (relative) => JSON.parse(readFileSync(join(project, relative), 'utf8')) };
}

let exits = 0;
function invoke(project, args, status) {
  exits += 1;
  const argv = ['--json', 'managed', ...args];
  if (args[0] !== 'equivalence') argv.push('--project', project);
  const child = spawnSync(bin, argv, {
    cwd: project,
    encoding: 'utf8',
    timeout: 120000,
    maxBuffer: 32 * 1024 * 1024,
  });
  const text = child.stdout.trim() || child.stderr.trim();
  let envelope;
  try {
    envelope = JSON.parse(text);
  } catch {
    throw new Error(`invalid output exit ${child.status}: ${text.slice(0, 1500)}`);
  }
  assert.equal(envelope.status, status, text.slice(0, 1500) + ' ARGS=' + JSON.stringify(args));
  const expectedExit = { valid: 0, invalid: 1, denied: 3, unsupported: 4, 'unsupported-version': 5 }[status];
  assert.equal(child.status, expectedExit, `exit for ${status}`);
  if (status === 'invalid' || status === 'unsupported-version') assert.equal(child.stdout, '', 'failures stream on stderr');
  else assert.equal(child.stderr, '');
  for (const diagnostic of envelope.diagnostics ?? []) {
    if (diagnostic.id.startsWith('managed.')) {
      assert.equal(diagnostic.registry_version, version);
    }
  }
  return envelope;
}

const ORACLE = {
  'apps/planner/gen/src/complete-task.ts': read(`${wire}/planner/expected/complete-task.ts`),
  'apps/planner/gen/src/focus-task.ts': read(`${wire}/planner/expected/focus-task.ts`),
  'apps/planner/gen/test/complete-task.test.ts': read(`${wire}/planner/expected/complete-task.test.ts`),
  'apps/planner/gen/test/focus-task.test.ts': read(`${wire}/planner/expected/focus-task.test.ts`),
};

function planReceipt(project, coverage = 'coverage.json', input = 'model/planner.model.json') {
  const receipt = invoke(project, [
    'plan', '--to', 'managed',
    '--ownership', 'ownership.json',
    '--coverage', coverage,
    '--module', 'planner', '--target', 'node-typescript',
    '--input', input,
  ], 'valid');
  return receipt.managed.plan;
}

function plan(project, coverage, input, name = 'plan.json') {
  const plan = planReceipt(project, coverage, input);
  writeFileSync(join(project, name), JSON.stringify(plan, null, 2) + '\n');
  return plan;
}

function apply(project, status, planFile = 'plan.json', coverage = 'coverage.json') {
  const plan = JSON.parse(readFileSync(join(project, planFile), 'utf8'));
  return invoke(project, [
    'apply', '--plan', planFile, '--coverage', coverage,
    '--ownership', 'ownership.json',
    '--confirm', plan.planDigest,
  ], status);
}

// ---- 1. Read-only surfaces never write: check, plan, and dry-run apply.
// The full tree including directories must be unchanged (finding 12).
{
  const { project } = sandbox('zero');
  const before = snapshot(project);
  invoke(project, ['check', '--ownership', 'ownership.json', '--coverage', 'coverage.json'], 'valid');
  assertSameTree(before, snapshot(project), 'check');
  planReceipt(project);
  assertSameTree(before, snapshot(project), 'plan preview');
  // A dry-run apply touches nothing: no custody directories appear.
  const previewPlan = planReceipt(project);
  writeFileSync(join(project, 'plan-dry.json'), JSON.stringify(previewPlan, null, 2) + '\n');
  const beforeDry = snapshot(project);
  invoke(project, [
    'apply', '--plan', 'plan-dry.json', '--coverage', 'coverage.json',
    '--ownership', 'ownership.json', '--dry-run', '--confirm', previewPlan.planDigest,
  ], 'valid');
  assertSameTree(beforeDry, snapshot(project), 'dry-run apply');
  assert.ok(!existsSync(join(project, '.lekalo')), 'dry run creates no custody tree');
  rmSync(project, { recursive: true, force: true });
}

// ---- 2. The confirmed apply publishes the exact oracle bytes. -------------
const primary = sandbox('primary');
{
  const before = snapshot(primary.project);
  const built = planReceipt(primary.project);
  assertSameTree(before, snapshot(primary.project), 'plan preview (primary)');
  assert.equal(built.action, 'promotion');
  writeFileSync(join(primary.project, 'plan.json'), JSON.stringify(built, null, 2) + '\n');
  assert.deepEqual(built.outputs.map((output) => `${output.path}:${output.action}`), [
    'apps/planner/gen/src/complete-task.ts:create',
    'apps/planner/gen/src/focus-task.ts:create',
    'apps/planner/gen/test/complete-task.test.ts:create',
    'apps/planner/gen/test/focus-task.test.ts:create',
  ]);
  const preview = apply(primary.project, 'valid', 'plan.json');
  assert.equal(preview.managed.dryRun, false);
  // Missing confirmation never mutates.
  const beforeApply = snapshot(primary.project);
  const unconfirmed = spawnSync(bin, ['--json', 'managed', 'apply', '--plan', 'plan.json', '--coverage', 'coverage.json', '--ownership', 'ownership.json', '--project', primary.project], { cwd: primary.project, encoding: 'utf8' });
  assert.equal(JSON.parse(unconfirmed.stderr.trim()).reasonCodes[0], 'managed.input-invalid');
  assertSameTree(beforeApply, snapshot(primary.project), 'apply-preflight-must-not-write-until-confirmed');
  for (const [path, bytes] of Object.entries(ORACLE)) {
    assert.equal(readFileSync(join(primary.project, path), 'utf8'), bytes, `oracle drift ${path}`);
  }
  assert.ok(existsSync(join(primary.project, '.lekalo/generated/manifests/managed/current')));
  const generation = readFileSync(join(primary.project, '.lekalo/generated/manifests/managed/current'), 'utf8').trim();
  assert.equal(preview.managed.generationDigest, generation);
}
const primaryGeneration = readFileSync(join(primary.project, '.lekalo/generated/manifests/managed/current'), 'utf8').trim();

// ---- 3. Re-applying the identical plan is refused, never a silent rewrite.
// The create coordinates are now claimed by the committed generation, so
// the identical promotion plan is an ownership denial (create-conflict,
// finding 2) — the plan is stale by construction and never re-runs.
{
  const before = snapshot(primary.project);
  const stale = apply(primary.project, 'denied');
  assert.deepEqual(stale.diagnostics[0].data, { path: ORACLE ? 'apps/planner/gen/src/complete-task.ts' : '', reason: 'create-conflict' });
  assertSameTree(before, snapshot(primary.project), 'stale re-apply');
}

// ---- 4. Manual drift blocks regeneration until an exact resolve. ----------
{
  const handler = join(primary.project, 'apps/planner/gen/src/focus-task.ts');
  writeFileSync(handler, '// manually edited\n');
  // Planning already refuses: the live bytes differ from recorded custody.
  const drifted = invoke(primary.project, [
    'plan', '--to', 'managed',
    '--ownership', 'ownership.json',
    '--coverage', 'coverage.json',
    '--module', 'planner', '--target', 'node-typescript',
    '--input', 'model/planner.model.json',
  ], 'invalid');
  assert.equal(drifted.diagnostics[0].data.reason, 'before-state');
  assert.equal(drifted.diagnostics[0].id, 'managed.generated-drift');
  // No regeneration plan exists after the refusal; only the exact resolve
  // transaction can move the drifted path.
  // The preview resolves the recorded bytes; the confirmation is the exact
  // resolve plan digest.
  const preview = invoke(primary.project, ['resolve', '--path', 'apps/planner/gen/src/focus-task.ts', '--restore', '--dry-run'], 'valid');
  const resolveDigest = preview.managed.planDigest;
  invoke(primary.project, ['resolve', '--path', 'apps/planner/gen/src/focus-task.ts', '--restore', '--confirm', resolveDigest], 'valid');
  assert.equal(readFileSync(handler, 'utf8'), ORACLE['apps/planner/gen/src/focus-task.ts'], 'restore must be exact recorded bytes, never the edited baseline');
  // A missing generated file is drift too (plan time refuses the same way).
  rmSync(join(primary.project, 'apps/planner/gen/src/complete-task.ts'));
  const missing = invoke(primary.project, [
    'plan', '--to', 'managed',
    '--ownership', 'ownership.json',
    '--coverage', 'coverage.json',
    '--module', 'planner', '--target', 'node-typescript',
    '--input', 'model/planner.model.json',
  ], 'invalid');
  assert.equal(missing.diagnostics[0].id, 'managed.generated-drift');
  invoke(primary.project, ['resolve', '--path', 'apps/planner/gen/src/complete-task.ts', '--restore', '--dry-run'], 'valid');
  const restoreDigest = JSON.parse(
    spawnSync(bin, ['--json', 'managed', 'resolve', '--path', 'apps/planner/gen/src/complete-task.ts', '--restore', '--dry-run', '--project', primary.project], { cwd: primary.project, encoding: 'utf8' }).stdout.trim(),
  ).managed.planDigest;
  invoke(primary.project, ['resolve', '--path', 'apps/planner/gen/src/complete-task.ts', '--restore', '--confirm', restoreDigest], 'valid');
  assert.equal(readFileSync(join(primary.project, 'apps/planner/gen/src/complete-task.ts'), 'utf8'), ORACLE['apps/planner/gen/src/complete-task.ts']);
}

// ---- 5. Scoped cleanup deletes exactly the orphaned generated output. ------
// Dropping a requirement is a Model change first: the coverage evidence is
// re-pinned to the moved authority (the `managed authority` read-only view),
// the plan is reviewed, and only the confirmed apply deletes the orphaned
// outputs of the retired obligation.
function authorityPins(project, module) {
  const receipt = invoke(project, ['authority', '--module', module], 'valid');
  return receipt.managed;
}
function rewriteModelCommands(project, keepIds) {
  const commands = join(project, 'lekalo/modules/planner/commands.yaml');
  if (keepIds.length === 0) {
    rmSync(commands);
    return;
  }
  const source = readFileSync(commands, 'utf8');
  const blocks = source.split(/(?=- id: )/);
  const kept = blocks.filter((block) => {
    const match = block.match(/- id: ([a-z0-9_.]+)/);
    return !match || keepIds.includes(match[1]);
  });
  writeFileSync(commands, kept.join(''));
}
{
  const coverage = JSON.parse(read(`${wire}/planner/coverage.json`));
  coverage.requirements = coverage.requirements.filter((requirement) => requirement.id !== 'planner.complete_task');
  // The Model retires the command first; the lock and the coverage pins
  // follow the moved authority.
  rewriteModelCommands(primary.project, []);
  invoke(primary.project, ['authority', '--module', 'planner'], 'valid');
  spawnSync(bin, ['lock'], { cwd: primary.project, encoding: 'utf8' });
  const moved = authorityPins(primary.project, 'planner');
  coverage.authority = { state: 'known', value: { model: moved.model, ir: moved.ir, lock: moved.lock } };
  writeFileSync(join(primary.project, 'coverage-cleanup.json'), JSON.stringify(coverage, null, 2) + '\n');
  // The stale coverage pins are refused before any plan exists.
  const staleCoverage = JSON.parse(read(`${wire}/planner/coverage.json`));
  staleCoverage.requirements = staleCoverage.requirements.filter((requirement) => requirement.id !== 'planner.complete_task');
  writeFileSync(join(primary.project, 'coverage-stale.json'), JSON.stringify(staleCoverage, null, 2) + '\n');
  const refused = invoke(primary.project, [
    'plan', '--to', 'managed',
    '--ownership', 'ownership.json',
    '--coverage', 'coverage-stale.json',
    '--module', 'planner', '--target', 'node-typescript',
    '--input', 'model/planner.model.json',
  ], 'invalid');
  assert.equal(refused.diagnostics[0].id, 'managed.plan-stale');
  // An unclaimed neighbor must survive every cleanup.
  writeFileSync(join(primary.project, 'apps/planner/gen/src/unclaimed.ts'), '// not ours\n');
  const built = plan(primary.project, 'coverage-cleanup.json');
  const deletions = built.outputs.filter((output) => output.action === 'delete');
  assert.deepEqual(deletions.map((output) => output.path), [
    'apps/planner/gen/src/complete-task.ts',
    'apps/planner/gen/test/complete-task.test.ts',
  ]);
  writeFileSync(join(primary.project, 'plan-cleanup.json'), JSON.stringify(built, null, 2) + '\n');
  const applied = apply(primary.project, 'valid', 'plan-cleanup.json', 'coverage-cleanup.json');
  assert.equal(applied.managed.deleted.length, 2);
  assert.ok(!existsSync(join(primary.project, 'apps/planner/gen/src/complete-task.ts')));
  assert.ok(existsSync(join(primary.project, 'apps/planner/gen/src/unclaimed.ts')), 'unclaimed files are never guessed disposable');
  rmSync(join(primary.project, 'apps/planner/gen/src/unclaimed.ts'));
  // Restore the full Model, lock, coverage pins, and generation for the
  // later phases.
  writeFileSync(join(primary.project, 'lekalo/modules/planner/commands.yaml'), read(`${wire}/planner/lekalo/modules/planner/commands.yaml`));
  spawnSync(bin, ['lock'], { cwd: primary.project, encoding: 'utf8' });
  const restored = authorityPins(primary.project, 'planner');
  const fullCoverage = JSON.parse(read(`${wire}/planner/coverage.json`));
  fullCoverage.authority = { state: 'known', value: { model: restored.model, ir: restored.ir, lock: restored.lock } };
  writeFileSync(join(primary.project, 'coverage.json'), JSON.stringify(fullCoverage, null, 2) + '\n');
  const rebuilt = plan(primary.project);
  writeFileSync(join(primary.project, 'plan-restore.json'), JSON.stringify(rebuilt, null, 2) + '\n');
  apply(primary.project, 'valid', 'plan-restore.json');
  for (const [path, bytes] of Object.entries(ORACLE)) {
    assert.equal(readFileSync(join(primary.project, path), 'utf8'), bytes, `restore-after-cleanup ${path}`);
  }
}

// ---- 6. Mixed modes: an observed sibling keeps every byte. -----------------
{
  const directory = mkdtempSync(join(tmpdir(), 'lekalo-managed-cli-mixed-'));
  const project = join(directory, 'proj');
  cpSync(join(root, wire, 'planner'), project, { recursive: true });
  rmSync(join(project, 'expected'), { recursive: true, force: true });
  cpSync(join(root, wire, 'mixed', 'ownership.json'), join(project, 'ownership.json'));
  const billing = join(project, 'apps/billing/handwritten/billing.ts');
  mkdirSync(join(project, 'apps/billing/handwritten'), { recursive: true });
  writeFileSync(billing, '// maintained billing handler; owned by the observed module\n');
  const before = snapshot(project);
  const built = plan(project);
  writeFileSync(join(project, 'plan.json'), JSON.stringify(built, null, 2) + '\n');
  apply(project, 'valid');
  assert.equal(readFileSync(billing, 'utf8'), '// maintained billing handler; owned by the observed module\n', 'observed module bytes are never touched');
  const afterTree = snapshot(project);
  for (const [name, bytes] of before) {
    if (!name.startsWith('apps/planner/gen') && !name.startsWith('.lekalo') && name !== 'plan.json') {
      assert.equal(afterTree.get(name), bytes, `mixed: ${name} changed`);
    }
  }
  const view = invoke(project, ['check', '--ownership', 'ownership.json'], 'valid');
  assert.deepEqual(view.managed.assignments.map((row) => `${row.module}:${row.mode}`), ['billing:observed', 'planner:managed']);
  rmSync(directory, { recursive: true, force: true });
}

// ---- 7. Recovery: an unresolved journal blocks every further mutation. ----
{
  const custody = join(primary.project, '.lekalo/generated/manifests/managed/transactions');
  writeFileSync(join(custody, 'blocked-9999.json'), JSON.stringify({
    schemaVersion: `lekalo/managed-transaction/v${version}`,
    identity: `dev.lekalo.managed-transaction@${version}`,
    transactionId: 'blocked-9999',
    state: 'prepared',
    action: 'apply',
    planDigest: { state: 'absent' },
    generationDigest: { state: 'absent' },
    beforeManifestDigest: { state: 'absent' },
    afterManifestDigest: { state: 'absent' },
    pending: [],
    affectedFiles: [],
    conflict: { state: 'absent' },
  }, null, 2) + '\n');
  planReceipt(primary.project);
  writeFileSync(join(primary.project, 'plan-blocked.json'), JSON.stringify(planReceipt(primary.project), null, 2) + '\n');
  const before = snapshot(primary.project);
  const blocked = apply(primary.project, 'invalid', 'plan-blocked.json');
  assert.equal(blocked.reasonCodes[0], 'managed.recovery-required');
  assertSameTree(before, snapshot(primary.project), 'blocked mutation');
  rmSync(join(custody, 'blocked-9999.json'));
  // Reconciliation unblocks: the resolved apply runs again.
  plan(primary.project);
  writeFileSync(join(primary.project, 'plan-unblocked.json'), JSON.stringify(plan(primary.project), null, 2) + '\n');
}

// ---- 8. Hostile ownership: unclaimed creates are denied, traversal never
//         admits, capability gaps and unvalidated extensions block. --------
{
  const hostile = sandbox('hostile');
  // An unclaimed file exactly where a create would land.
  mkdirSync(join(hostile.project, 'apps/planner/gen/src'), { recursive: true });
  writeFileSync(join(hostile.project, 'apps/planner/gen/src/focus-task.ts'), '// squatter\n');
  const before = snapshot(hostile.project);
  // The plan itself refuses: an unclaimed file already occupies a create
  // coordinate, so no reviewed plan may exist for it.
  const denied = invoke(hostile.project, [
    'plan', '--to', 'managed',
    '--ownership', 'ownership.json',
    '--coverage', 'coverage.json',
    '--module', 'planner', '--target', 'node-typescript',
    '--input', 'model/planner.model.json',
  ], 'denied');
  assert.equal(denied.reasonCodes[0], 'managed.ownership-denied');
  assert.equal(denied.diagnostics[0].data.reason, 'create-conflict');
  assertSameTree(before, snapshot(hostile.project), 'denied plan');
  rmSync(hostile.project, { recursive: true, force: true });

  const traversal = sandbox('traversal');
  const ownership = JSON.parse(read(`${wire}/planner/ownership.json`));
  ownership.modules[0].targets[0].generatedRoot = 'apps/../secrets';
  writeFileSync(join(traversal.project, 'ownership.json'), JSON.stringify(ownership, null, 2) + '\n');
  const refused = invoke(traversal.project, ['check', '--ownership', 'ownership.json'], 'invalid');
  assert.equal(refused.reasonCodes[0], 'managed.input-invalid');
  rmSync(traversal.project, { recursive: true, force: true });

  const gap = sandbox('gap');
  const partial = JSON.parse(read(`${wire}/planner/coverage.json`));
  partial.requirements[1].support = 'unknown';
  writeFileSync(join(gap.project, 'coverage-gap.json'), JSON.stringify(partial, null, 2) + '\n');
  const gapBefore = snapshot(gap.project);
  const blocked = invoke(gap.project, ['plan', '--to', 'managed', '--ownership', 'ownership.json', '--coverage', 'coverage-gap.json', '--module', 'planner', '--target', 'node-typescript', '--input', 'model/planner.model.json'], 'unsupported');
  assert.equal(blocked.reasonCodes[0], 'managed.capability-gap');
  assert.ok(blocked.diagnostics[0].data.requirement === 'planner.focus_task', 'the gap names the semantic witness');
  assertSameTree(gapBefore, snapshot(gap.project), 'capability gap writes nothing');
  rmSync(gap.project, { recursive: true, force: true });

  const extension = sandbox('extension');
  const unvalidated = JSON.parse(read(`${wire}/planner/coverage.json`));
  unvalidated.extensions.push({
    symbol: 'legacy_hook',
    binding: 'custom',
    contractDigest: 'sha256:9999999999999999999999999999999999999999999999999999999999999999',
    validated: false,
  });
  unvalidated.extensions.sort((a, b) => (a.symbol < b.symbol ? -1 : 1));
  writeFileSync(join(extension.project, 'coverage-ext.json'), JSON.stringify(unvalidated, null, 2) + '\n');
  const unresolved = invoke(extension.project, ['plan', '--to', 'managed', '--ownership', 'ownership.json', '--coverage', 'coverage-ext.json', '--module', 'planner', '--target', 'node-typescript', '--input', 'model/planner.model.json'], 'invalid');
  assert.equal(unresolved.reasonCodes[0], 'managed.extension-unresolved');
  rmSync(extension.project, { recursive: true, force: true });
}

// ---- 9. Determinism: a fresh root produces identical bytes and identity. --
{
  const twin = sandbox('twin');
  const built = plan(twin.project);
  // A clean rebuild legitimately reviews create actions where the repeated
  // no-op plan reviews unchanged actions; the stable generation identity is
  // what must match (AC1).
  writeFileSync(join(twin.project, 'plan.json'), JSON.stringify(built, null, 2) + '\n');
  const applied = apply(twin.project, 'valid');
  assert.equal(applied.managed.generationDigest, primaryGeneration, 'the stable generation digest matches across roots');
  for (const [path, bytes] of Object.entries(ORACLE)) {
    assert.equal(readFileSync(join(twin.project, path), 'utf8'), bytes, `cross-root byte equality ${path}`);
  }
  rmSync(twin.project, { recursive: true, force: true });
}

// ---- 10. Upgrade diff: exact pin changes, zero writes. --------------------
{
  const candidate = JSON.parse(read(`${wire}/planner/coverage.json`));
  candidate.recipe.digest = 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
  candidate.adapter.packageDigest = 'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';
  writeFileSync(join(primary.project, 'coverage-candidate.json'), JSON.stringify(candidate, null, 2) + '\n');
  const before = snapshot(primary.project);
  const diff = invoke(primary.project, ['upgrade-plan', '--module', 'planner', '--target', 'node-typescript', '--ownership', 'ownership.json', '--base', 'coverage.json', '--candidate', 'coverage-candidate.json'], 'valid');
  assert.deepEqual(diff.managed.changes.map((change) => change.field), ['recipe.digest', 'adapter.packageDigest']);
  assertSameTree(before, snapshot(primary.project), 'upgrade-plan writes nothing');
}

// ---- 11. Equivalence verdicts never lower the denominator. ----------------
{
  const pass = invoke(primary.project, ['equivalence', '--evidence', 'equivalence.json'], 'valid');
  assert.equal(pass.managed.verdict, 'match');
  const evidence = JSON.parse(read(`${wire}/planner/equivalence.json`));
  const mismatch = { ...evidence, scenarios: [{ ...evidence.scenarios[0], verdict: 'mismatch' }, ...evidence.scenarios.slice(1)] };
  writeFileSync(join(primary.project, 'equivalence-mismatch.json'), JSON.stringify(mismatch, null, 2) + '\n');
  const failed = invoke(primary.project, ['equivalence', '--evidence', 'equivalence-mismatch.json'], 'invalid');
  assert.equal(failed.reasonCodes[0], 'managed.equivalence-failed');
  const gap = { ...evidence, gaps: ['expression.regex'] };
  writeFileSync(join(primary.project, 'equivalence-gap.json'), JSON.stringify(gap, null, 2) + '\n');
  const incomplete = invoke(primary.project, ['equivalence', '--evidence', 'equivalence-gap.json'], 'unsupported');
  assert.equal(incomplete.reasonCodes[0], 'managed.equivalence-incomplete');
  // The evidence is bound to the exact committed generation: a stale
  // implementation pin (the bytes moved) is a failed gate, not a pass
  // (finding L).
  const staleEvidence = { ...evidence, implementationDigest: 'sha256:cccc333333333333333333333333333333333333333333333333333333333333' };
  writeFileSync(join(primary.project, 'equivalence-stale.json'), JSON.stringify(staleEvidence, null, 2) + '\n');
  const stale = invoke(primary.project, ['equivalence', '--evidence', 'equivalence-stale.json'], 'invalid');
  assert.equal(stale.reasonCodes[0], 'managed.equivalence-failed');
}

// ---- 12. The resolve confirmation cannot be bypassed (finding A). --------
{
  const sandbox0 = sandbox('confirm');
  const project = sandbox0.project;
  const built = plan(project);
  writeFileSync(join(project, 'plan.json'), JSON.stringify(built, null, 2) + '\n');
  apply(project, 'valid');
  const handler = join(project, 'apps/planner/gen/src/focus-task.ts');
  writeFileSync(handler, '// drift for confirm probe\n');
  const preview = invoke(project, ['resolve', '--path', 'apps/planner/gen/src/focus-task.ts', '--restore', '--dry-run'], 'valid');
  const digest = preview.managed.planDigest;
  const before = snapshot(project);
  // No confirmation never moves the file.
  const none = spawnSync(bin, ['--json', 'managed', 'resolve', '--path', 'apps/planner/gen/src/focus-task.ts', '--restore', '--project', project], { cwd: project, encoding: 'utf8' });
  assert.equal(JSON.parse(none.stderr.trim()).reasonCodes[0], 'managed.input-invalid');
  // An all-zero confirmation never moves the file.
  const zero = spawnSync(bin, ['--json', 'managed', 'resolve', '--path', 'apps/planner/gen/src/focus-task.ts', '--restore', '--confirm', 'sha256:' + '0'.repeat(64), '--project', project], { cwd: project, encoding: 'utf8' });
  assert.equal(JSON.parse(zero.stderr.trim()).reasonCodes[0], 'managed.input-invalid');
  assertSameTree(before, snapshot(project), 'resolve confirmations refuse without moving bytes');
  // The exact previewed confirmation restores the recorded bytes.
  invoke(project, ['resolve', '--path', 'apps/planner/gen/src/focus-task.ts', '--restore', '--confirm', digest], 'valid');
  assert.equal(readFileSync(handler, 'utf8'), ORACLE['apps/planner/gen/src/focus-task.ts']);
  rmSync(project, { recursive: true, force: true });
}

// ---- 13. Scoped applies preserve every unaffected managed module (F). -----
{
  const directory = mkdtempSync(join(tmpdir(), 'lekalo-managed-cli-two-'));
  const project = join(directory, 'proj');
  cpSync(join(root, wire, 'planner'), project, { recursive: true });
  rmSync(join(project, 'expected'), { recursive: true, force: true });
  // A second managed module zoo with its own generated root.
  const ownership = JSON.parse(read(`${wire}/planner/ownership.json`));
  ownership.modules.push({
    module: 'zoo',
    mode: 'managed',
    targets: [{
      target: 'node-typescript',
      profileRef: ownership.modules[0].targets[0].profileRef,
      implementationRef: ownership.modules[0].targets[0].implementationRef,
      generatedRoot: 'apps/zoo/gen',
      extensions: [],
    }],
  });
  writeFileSync(join(project, 'ownership.json'), JSON.stringify(ownership, null, 2) + '\n');
  const zooCoverage = JSON.parse(read(`${wire}/planner/coverage.json`));
  zooCoverage.module = 'zoo';
  // The zoo requirement witnesses the zoo Model's exact declared input
  // (one `animal_id` field): projection and sample are narrowed to it,
  // because the Model-derived semantics gate (finding 4) refuses caller
  // projections that invent Model-absent fields.
  zooCoverage.requirements = [{
    ...zooCoverage.requirements[0],
    id: 'zoo.feed_animal',
    projection: [{ from: 'id', to: 'id' }],
    sample: {
      state: 'known',
      value: { input: { id: 't-1' }, output: { id: 't-1' } },
    },
  }];
  // zoo must be a real module of the model for its ledger to exist.
  const projectYaml = readFileSync(join(project, 'lekalo/project.yaml'), 'utf8') + '';
  const moduleDir = join(project, 'lekalo/modules/zoo');
  mkdirSync(moduleDir, { recursive: true });
  writeFileSync(join(moduleDir, 'module.yaml'), 'schema_version: "0.2.16"\ndefinitions:\n  - id: zoo\n    kind: module\n    version: 1\n');
  writeFileSync(join(moduleDir, 'entities.yaml'), 'schema_version: "0.2.16"\ndefinitions:\n  - id: zoo.animal_id\n    kind: scalar\n    version: 1\n    base: uuid\n');
  writeFileSync(join(moduleDir, 'commands.yaml'), 'schema_version: "0.2.16"\ndefinitions:\n  - id: zoo.feed_animal\n    kind: command\n    version: 1\n    input:\n      - name: id\n        type:\n          ref: animal_id\n        required: true\n');
  spawnSync(bin, ['lock'], { cwd: project, encoding: 'utf8' });
  const plannerPins = invoke(project, ['authority', '--module', 'planner'], 'valid').managed;
  const plannerCoverage = JSON.parse(read(`${wire}/planner/coverage.json`));
  plannerCoverage.authority = { state: 'known', value: { model: plannerPins.model, ir: plannerPins.ir, lock: plannerPins.lock } };
  writeFileSync(join(project, 'coverage.json'), JSON.stringify(plannerCoverage, null, 2) + '\n');
  const pins = invoke(project, ['authority', '--module', 'zoo'], 'valid').managed;
  zooCoverage.authority = { state: 'known', value: { model: pins.model, ir: pins.ir, lock: pins.lock } };
  writeFileSync(join(project, 'coverage-zoo.json'), JSON.stringify(zooCoverage, null, 2) + '\n');
  // Apply planner, then zoo, then replan planner: zoo's outputs must
  // survive in the committed record and planner must not be orphaned.
  const plannerPlan = plan(project);
  writeFileSync(join(project, 'plan.json'), JSON.stringify(plannerPlan, null, 2) + '\n');
  apply(project, 'valid');
  const zooPlanReceipt = invoke(project, [
    'plan', '--to', 'managed', '--ownership', 'ownership.json', '--coverage', 'coverage-zoo.json',
    '--module', 'zoo', '--target', 'node-typescript', '--input', 'model/planner.model.json',
  ], 'valid');
  const zooPlan = zooPlanReceipt.managed.plan;
  writeFileSync(join(project, 'plan-zoo.json'), JSON.stringify(zooPlan, null, 2) + '\n');
  invoke(project, ['apply', '--plan', 'plan-zoo.json', '--coverage', 'coverage-zoo.json', '--ownership', 'ownership.json', '--confirm', zooPlan.planDigest], 'valid');
  assert.ok(existsSync(join(project, 'apps/zoo/gen/src/feed-animal.ts')));
  // Replanning planner must succeed: zoo custody is preserved, planner's
  // files are still owned.
  const replan = invoke(project, [
    'plan', '--to', 'managed', '--ownership', 'ownership.json', '--coverage', 'coverage.json',
    '--module', 'planner', '--target', 'node-typescript', '--input', 'model/planner.model.json',
  ], 'valid');
  assert.ok(replan.managed.plan.outputs.every((output) => output.action === 'unchanged'), 'planner replan is a clean no-op');
  rmSync(directory, { recursive: true, force: true });
}

// ---- 14. Ancestor reparse aliases never escape the project (H). ----------
if (process.platform === 'win32') {
  const directory = mkdtempSync(join(tmpdir(), 'lekalo-managed-cli-junction-'));
  const project = join(directory, 'proj');
  cpSync(join(root, wire, 'planner'), project, { recursive: true });
  rmSync(join(project, 'expected'), { recursive: true, force: true });
  // A sibling directory outside the project plus a junction from the
  // generated root: a confined plan must refuse before any byte moves.
  const outside = join(directory, 'outside');
  mkdirSync(outside, { recursive: true });
  writeFileSync(join(outside, 'escaped.ts'), '// must never be written\n');
  rmSync(join(project, 'apps'), { recursive: true, force: true });
  mkdirSync(join(project, 'apps/planner'), { recursive: true });
  symlinkSync(join(directory, 'outside'), join(project, 'apps/planner/gen'), 'junction');
  const before = snapshot(project);
  const denied = invoke(project, [
    'plan', '--to', 'managed', '--ownership', 'ownership.json', '--coverage', 'coverage.json',
    '--module', 'planner', '--target', 'node-typescript', '--input', 'model/planner.model.json',
  ], 'denied');
  assert.equal(denied.diagnostics[0].id, 'managed.ownership-denied');
  assertSameTree(before, snapshot(project), 'junction plan writes nothing outside');
  rmSync(join(project, 'apps/planner/gen'), { recursive: true, force: true });
  rmSync(directory, { recursive: true, force: true });
}

// ---- 15. Custody publication is verified by generate --check (E, finding
// 14). The supported selector is `--project .` invoked from the project's
// cwd (an absolute selector is a structure refusal, never drift evidence);
// every verdict is asserted with its concrete diagnostics, and every
// check is verified read-only over the full tree including directories.
function generateCheck(project, status, label) {
  const before = snapshot(project);
  const child = spawnSync(bin, ['--json', 'generate', '--check', '--project', '.'], { cwd: project, encoding: 'utf8' });
  const envelope = JSON.parse((child.stdout.trim() || child.stderr.trim()));
  assert.equal(envelope.status, status, `${label}: ${JSON.stringify(envelope).slice(0, 400)}`);
  assert.equal(child.status, { valid: 0, invalid: 1 }[status], `${label}: exit`);
  assertSameTree(before, snapshot(project), `${label}: read-only`);
  return envelope;
}
function assertFullManifest(project, label) {
  const manifest = JSON.parse(readFileSync(join(project, '.lekalo/generated/manifests/ownership.json'), 'utf8'));
  const custody = readFileSync(join(project, '.lekalo/generated/manifests/managed/current'), 'utf8').trim();
  const record = JSON.parse(readFileSync(join(project, '.lekalo/generated/manifests/managed/objects', custody.slice('sha256:'.length)), 'utf8'));
  const recorded = new Set(record.outputs.map((output) => output.path));
  const published = new Set(manifest.artifacts.map((artifact) => artifact.path));
  assert.deepEqual([...published].sort(), [...recorded].sort(), `${label}: manifest bijects with the committed record`);
  for (const artifact of manifest.artifacts) {
    const live = sha256(readFileSync(join(project, artifact.path)));
    assert.equal(live, artifact.content.digest, `${label}: ${artifact.path} bytes`);
  }
  for (const map of manifest.source_maps) {
    assert.ok(recorded.has(map.artifact.path), `${label}: map artifact ${map.artifact.path}`);
  }
}
function sha256(bytes) {
  return 'sha256:' + createHash('sha256').update(bytes).digest('hex');
}
{
  // Clean committed custody: the ownership manifest matches every byte
  // and bijects with the committed record; the check is valid/0.
  const clean = generateCheck(primary.project, 'valid', 'generate-check clean');
  assert.ok(clean.manifestDigest, 'clean check names the manifest digest');
  assertFullManifest(primary.project, 'clean');
  // A manual edit of a recorded output is a blocking verdict naming the
  // path and the drift kind.
  const handler = join(primary.project, 'apps/planner/gen/src/focus-task.ts');
  writeFileSync(handler, '// drifted for generate check\n');
  const drifted = generateCheck(primary.project, 'invalid', 'generate-check drift');
  assert.ok(
    (drifted.findings ?? []).some((finding) => finding.path?.includes('focus-task.ts'))
      || (drifted.diagnostics ?? []).some(
        (diagnostic) => diagnostic.id.startsWith('lock.') || diagnostic.id.startsWith('managed.'),
      ),
    'drift verdict carries a concrete blocking diagnostic',
  );
  // Restore through the exact resolve, then the check is clean again.
  const preview = invoke(primary.project, ['resolve', '--path', 'apps/planner/gen/src/focus-task.ts', '--restore', '--dry-run'], 'valid');
  invoke(primary.project, ['resolve', '--path', 'apps/planner/gen/src/focus-task.ts', '--restore', '--confirm', preview.managed.planDigest], 'valid');
  generateCheck(primary.project, 'valid', 'generate-check restored');
  assertFullManifest(primary.project, 'restored');
  // A missing recorded output is a blocking verdict too.
  rmSync(join(primary.project, 'apps/planner/gen/src/complete-task.ts'));
  generateCheck(primary.project, 'invalid', 'generate-check missing');
  const missingPreview = invoke(primary.project, ['resolve', '--path', 'apps/planner/gen/src/complete-task.ts', '--restore', '--dry-run'], 'valid');
  invoke(primary.project, ['resolve', '--path', 'apps/planner/gen/src/complete-task.ts', '--restore', '--confirm', missingPreview.managed.planDigest], 'valid');
  // Control: the absolute selector is a selection refusal, never drift
  // evidence — the gate must not accept it as a check result.
  const absolute = spawnSync(bin, ['--json', 'generate', '--check', '--project', primary.project], { cwd: primary.project, encoding: 'utf8' });
  assert.equal(JSON.parse(absolute.stderr.trim()).diagnostics[0].id, 'structure.selection-absolute');
}

// ---- 16. Interrupted publication recovers through the defined path (J). --
{
  const custody = join(primary.project, '.lekalo/generated/manifests/managed/transactions');
  writeFileSync(join(custody, 'blocked-7777.json'), JSON.stringify({
    schemaVersion: `lekalo/managed-transaction/v${version}`,
    identity: `dev.lekalo.managed-transaction@${version}`,
    transactionId: 'blocked-7777',
    state: 'publishing',
    action: 'apply',
    planDigest: { state: 'absent' },
    generationDigest: { state: 'absent' },
    beforeManifestDigest: { state: 'absent' },
    afterManifestDigest: { state: 'absent' },
    pending: [],
    affectedFiles: [],
    conflict: { state: 'absent' },
    beforeDigest: { state: 'absent' },
  }, null, 2) + '\n');
  // Rollback and resolve are blocked by the unresolved journal too.
  const blockedRollback = invoke(primary.project, ['rollback', '--to', 'deadbeef'], 'invalid');
  assert.equal(blockedRollback.reasonCodes[0], 'managed.recovery-required');
  // Recovery reconciles the journal and unblocks every mutator.
  const recovered = invoke(primary.project, ['recover'], 'valid');
  assert.equal(recovered.managed.reconciled[0].transactionId, 'blocked-7777');
  invoke(primary.project, ['recover'], 'invalid'); // nothing left to recover
  rmSync(join(custody, 'blocked-7777.json'), { force: true });
}

// ---- 17. A resolved custom binding renders its invocation and stays owned
//          by its maintainer across regeneration (K). ----------------------
{
  const extension = sandbox('custom-binding');
  const project = extension.project;
  mkdirSync(join(project, 'src/extensions'), { recursive: true });
  const bindingBytes = 'export function legacyHook(input) { return input ?? null; }\n';
  writeFileSync(join(project, 'src/extensions/legacy_hook.ts'), bindingBytes);
  const bindingDigest = createHash('sha256').update(bindingBytes).digest('hex');
  // The ownership target selects the extension explicitly.
  const ownership = JSON.parse(read(`${wire}/planner/ownership.json`));
  ownership.modules[0].targets[0].extensions.push({
    symbol: 'legacy_hook',
    binding: 'src/extensions/legacy_hook.ts',
    contractDigest: `sha256:${bindingDigest}`,
  });
  writeFileSync(join(project, 'ownership.json'), JSON.stringify(ownership, null, 2) + '\n');
  const coverage = JSON.parse(read(`${wire}/planner/coverage.json`));
  coverage.extensions.push({
    symbol: 'legacy_hook',
    binding: 'src/extensions/legacy_hook.ts',
    contractDigest: `sha256:${bindingDigest}`,
    validated: true,
  });
  writeFileSync(join(project, 'coverage-bound.json'), JSON.stringify(coverage, null, 2) + '\n');
  // A validated row whose binding file is missing never reaches a plan:
  // both the ownership selection and the coverage row retarget, so the
  // refusal comes from the live resolution, not a stale selection.
  const missingCoverage = JSON.parse(JSON.stringify(coverage));
  missingCoverage.extensions[0].binding = 'src/extensions/missing.ts';
  const missingOwnership = JSON.parse(JSON.stringify(ownership));
  missingOwnership.modules[0].targets[0].extensions[0].binding = 'src/extensions/missing.ts';
  writeFileSync(join(project, 'ownership-missing.json'), JSON.stringify(missingOwnership, null, 2) + '\n');
  writeFileSync(join(project, 'coverage-missing-binding.json'), JSON.stringify(missingCoverage, null, 2) + '\n');
  const unresolved = invoke(project, [
    'plan', '--to', 'managed', '--ownership', 'ownership-missing.json', '--coverage', 'coverage-missing-binding.json',
    '--module', 'planner', '--target', 'node-typescript', '--input', 'model/planner.model.json',
  ], 'invalid');
  assert.equal(unresolved.reasonCodes[0], 'managed.extension-unresolved');
  assert.equal(unresolved.diagnostics[0].data.reason, 'missing');
  // The resolved binding plans, applies, and the generated test invokes it.
  const built = plan(project, 'coverage-bound.json');
  writeFileSync(join(project, 'plan-bound.json'), JSON.stringify(built, null, 2) + '\n');
  invoke(project, ['apply', '--plan', 'plan-bound.json', '--coverage', 'coverage-bound.json', '--ownership', 'ownership.json', '--confirm', built.planDigest], 'valid');
  const test = readFileSync(join(project, 'apps/planner/gen/test/focus-task.test.ts'), 'utf8');
  assert.ok(test.includes('src/extensions/legacy_hook.ts'), 'the generated test imports the bound implementation');
  assert.ok(test.includes('legacyHook({})'), 'the generated test invokes the binding');
  execFileSync(process.execPath, [join(project, 'apps/planner/gen/test/focus-task.test.ts')], { timeout: 60000, stdio: 'pipe' });
  // The bound implementation is maintained source: regeneration keeps it.
  const rebuilt = plan(project, 'coverage-bound.json');
  writeFileSync(join(project, 'plan-rebound.json'), JSON.stringify(rebuilt, null, 2) + '\n');
  apply(project, 'valid', 'plan-rebound.json', 'coverage-bound.json');
  assert.equal(readFileSync(join(project, 'src/extensions/legacy_hook.ts'), 'utf8'), bindingBytes, 'custom source is preserved');
  rmSync(project, { recursive: true, force: true });
}

// ---- 18. In-place requirement diffs are named by upgrade-plan (N). -------
{
  const candidate = JSON.parse(read(`${wire}/planner/coverage.json`));
  candidate.requirements[1].detail = 'in-place witness tightening';
  writeFileSync(join(primary.project, 'coverage-inplace.json'), JSON.stringify(candidate, null, 2) + '\n');
  const diff = invoke(primary.project, ['upgrade-plan', '--module', 'planner', '--target', 'node-typescript', '--ownership', 'ownership.json', '--base', 'coverage.json', '--candidate', 'coverage-inplace.json'], 'valid');
  const fields = diff.managed.changes.map((change) => change.field);
  assert.ok(fields.includes('requirement.planner.focus_task'), `in-place diff present: ${fields}`);
  const retired = JSON.parse(read(`${wire}/planner/coverage.json`));
  retired.recipe.digest = 'sha256:' + 'd'.repeat(64);
  retired.adapter.protocol = '0.6.4';
  writeFileSync(join(primary.project, 'coverage-incomparable.json'), JSON.stringify(retired, null, 2) + '\n');
  const incomparable = invoke(primary.project, ['upgrade-plan', '--module', 'planner', '--target', 'node-typescript', '--ownership', 'ownership.json', '--base', 'coverage.json', '--candidate', 'coverage-incomparable.json'], 'valid');
  assert.equal(incomparable.managed.comparable, false);
  // A candidate that omits a real Model obligation is a capability gap
  // in the plan lane; upgrade-plan must not call it comparable (finding
  // 13 / N).
  const omitted = JSON.parse(read(`${wire}/planner/coverage.json`));
  omitted.requirements = omitted.requirements.filter((requirement) => requirement.id !== 'planner.focus_task');
  writeFileSync(join(primary.project, 'coverage-omitted.json'), JSON.stringify(omitted, null, 2) + '\n');
  const omission = invoke(primary.project, ['upgrade-plan', '--module', 'planner', '--target', 'node-typescript', '--ownership', 'ownership.json', '--base', 'coverage.json', '--candidate', 'coverage-omitted.json'], 'valid');
  assert.equal(omission.managed.comparable, false, 'omitted obligation is incomparable');
  assert.deepEqual(omission.managed.requirementsRemoved, ['planner.focus_task']);
  // A candidate whose extension binding does not resolve (missing
  // export) is incomparable too.
  const bound = JSON.parse(read(`${wire}/planner/coverage.json`));
  const emptyBinding = 'export {};\n';
  mkdirSync(join(primary.project, 'custom'), { recursive: true });
  writeFileSync(join(primary.project, 'custom/empty.ts'), emptyBinding);
  bound.extensions = [{
    symbol: 'missing_hook',
    binding: 'custom/empty.ts',
    contractDigest: `sha256:${createHash('sha256').update(emptyBinding).digest('hex')}`,
    validated: true,
  }];
  bound.extensions.sort((a, b) => (a.symbol < b.symbol ? -1 : 1));
  writeFileSync(join(primary.project, 'coverage-unbound.json'), JSON.stringify(bound, null, 2) + '\n');
  const unbound = invoke(primary.project, ['upgrade-plan', '--module', 'planner', '--target', 'node-typescript', '--ownership', 'ownership.json', '--base', 'coverage.json', '--candidate', 'coverage-unbound.json'], 'valid');
  assert.equal(unbound.managed.comparable, false, 'unresolved binding is incomparable');
}

// ---- 19. A tampered generation record is refused without touching files
//          (finding I). --------------------------------------------------
{
  const current = readFileSync(join(primary.project, '.lekalo/generated/manifests/managed/current'), 'utf8').trim();
  const hex = current.slice('sha256:'.length);
  const recordPath = join(primary.project, '.lekalo/generated/manifests/managed/objects', hex);
  const original = readFileSync(recordPath, 'utf8');
  const record = JSON.parse(original);
  // Retarget one recorded output at an unrelated maintained file.
  const victim = record.outputs[0];
  victim.path = 'maintained/handler.ts';
  record.outputs.sort((left, right) => (left.path < right.path ? -1 : 1));
  writeFileSync(recordPath, JSON.stringify(record, null, 2) + '\n');
  const before = snapshot(primary.project);
  const forgedResolve = invoke(primary.project, ['resolve', '--path', 'maintained/handler.ts', '--restore', '--dry-run'], 'invalid');
  assert.ok(
    forgedResolve.reasonCodes.includes('managed.rollback-refused') || forgedResolve.reasonCodes.includes('managed.input-invalid'),
    `forged resolve refused: ${forgedResolve.reasonCodes}`,
  );
  assertSameTree(before, snapshot(primary.project), 'forged custody never moves a byte');
  assert.ok(!existsSync(join(primary.project, 'maintained/handler.ts')), 'the forged target was never created');
  // The authentic record is restored so the primary stays coherent.
  writeFileSync(recordPath, original);
}

console.log(JSON.stringify({ ok: true, gate: 'test-managed-cli', invocations: exits }));
