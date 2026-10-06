#!/usr/bin/env node
// Issue #113: the module API boundary family gate. Closed schema checks
// for the seven new 0.8.0 families plus the union registry successor, real
// CLI grammar/status/exit probes, independent boundary oracles, exact
// consumer diff partitions, public-only context bounds, lock denial, and
// determinism. Read-only against the checkout; every mutation is a
// temporary probe under the OS temp dir.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, existsSync, mkdtempSync, rmSync, readdirSync, statSync } from 'node:fs';
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(import.meta.url);
const fail = (reason, detail) => { process.stderr.write(JSON.stringify({ ok: false, gate: 'module-api-contracts', reason, detail }) + '\n'); process.exit(1); };
let Ajv;
try {
  Ajv = require('ajv/dist/2020').default;
  if (require('ajv/package.json').version !== '8.17.1') fail('ajv-version', 'expected 8.17.1');
} catch { fail('ajv-unavailable', 'set NODE_PATH=$LEKALO_AJV_NODE_PATH'); }
const binary = process.env.LEKALO_MODULE_API_BIN ?? join(root, 'target/debug', process.platform === 'win32' ? 'lekalo.exe' : 'lekalo');
if (!existsSync(binary)) fail('binary-missing', 'cargo build --workspace --locked');
const read = (p) => JSON.parse(readFileSync(join(root, p), 'utf8'));
const ajv = new Ajv({ strict: true, allErrors: true });
const compile = (family) => ajv.compile(read(`contracts/${family}.schema.v0.8.0.json`));
const families = [
  'module-api',
  'module-public-contract',
  'module-api-report',
  'module-api-consumers',
  'module-api-diff',
  'module-api-lock',
  'module-consumer-context',
];
const validators = new Map(families.map((family) => [family, compile(family)]));
const validates = (family, value) => {
  const check = validators.get(family);
  assert.ok(check(value), `${family}: ${JSON.stringify(check.errors?.slice(0, 3))}`);
};
const sha = (bytes) => 'sha256:' + createHash('sha256').update(bytes).digest('hex');
const scratch = mkdtempSync(join(tmpdir(), 'lekalo-module-api-gate-'));
const temp = (name, value) => {
  const p = join(scratch, name);
  writeFileSync(p, typeof value === 'string' ? value : JSON.stringify(value, null, 2));
  return p;
};
const fixture = 'tests/fixtures/module-api';
const reference = `${fixture}/reference/tasks`;
const checked = [];
function run(args, { exit = 0, cwd = root } = {}) {
  const p = spawnSync(binary, ['--json', ...args], { cwd, encoding: 'utf8', timeout: 120000, maxBuffer: 64 * 1024 * 1024 });
  assert.equal(p.status, exit, JSON.stringify({ args, status: p.status, error: p.error?.code, stdout: p.stdout?.slice(0, 400), stderr: p.stderr?.slice(0, 700) }));
  const stream = exit === 1 || exit === 5 ? p.stderr : p.stdout;
  const document = stream.trim() ? JSON.parse(stream) : {};
  return { document, stdout: p.stdout, stderr: p.stderr };
}
const codes = (document) => (document.diagnostics ?? []).map((d) => d.code);
let PAYLOAD_CALLS = 0;
const payload = (document, name) => document[name] ?? document.payload?.[name];

// 1. Union registry successor: exact identity, sorted-unique ids, the
// fourteen new rules, and the full 0.6.5 union preserved byte-for-byte.
{
  const registry = read('contracts/diagnostic-registry.v0.8.0.json');
  assert.equal(registry.schema_version, 'lekalo/diagnostic-registry/v0.8.0');
  assert.equal(registry.identity, 'dev.lekalo.diagnostic-registry@0.8.0');
  assert.equal(registry.registry_version, '0.8.0');
  assert.equal(registry.closed, true);
  const ids = registry.entries.map((e) => e.id);
  for (let i = 1; i < ids.length; i++) assert.ok(ids[i - 1] < ids[i], 'registry ids sorted-unique');
  const moduleRules = registry.entries.filter((e) => e.id.startsWith('module-api.'));
  assert.equal(moduleRules.length, 14);
  const expectedCodes = new Set(Array.from({ length: 14 }, (_, i) => `LEK-MODULE-${String(i + 1).padStart(3, '0')}`));
  assert.equal(moduleRules.filter((e) => expectedCodes.has(e.code)).length, 14, 'exact LEK-MODULE-001..014 allocation');
  const byId = Object.fromEntries(moduleRules.map((e) => [e.id, e.code]));
  assert.equal(byId['module-api.input-invalid'], 'LEK-MODULE-001');
  assert.equal(byId['module-api.version-unsupported'], 'LEK-MODULE-002');
  assert.equal(byId['module-api.symbol-not-exported'], 'LEK-MODULE-003');
  assert.equal(byId['module-api.import-undeclared'], 'LEK-MODULE-004');
  assert.equal(byId['module-api.public-private-exposure'], 'LEK-MODULE-005');
  assert.equal(byId['module-api.public-target-exposure'], 'LEK-MODULE-006');
  assert.equal(byId['module-api.cross-module-write-forbidden'], 'LEK-MODULE-007');
  assert.equal(byId['module-api.dependency-cycle'], 'LEK-MODULE-008');
  assert.equal(byId['module-api.consumer-incompatible'], 'LEK-MODULE-009');
  assert.equal(byId['module-api.version-policy-violation'], 'LEK-MODULE-010');
  assert.equal(byId['module-api.deprecation-window-violation'], 'LEK-MODULE-011');
  assert.equal(byId['module-api.evidence-incomplete'], 'LEK-MODULE-012');
  assert.equal(byId['module-api.lock-stale'], 'LEK-MODULE-013');
  assert.equal(byId['module-api.policy-denied'], 'LEK-MODULE-014');
  const predecessor = read('contracts/diagnostic-registry.v0.6.5.json');
  const successorIds = new Set(ids);
  for (const entry of predecessor.entries) {
    const kept = registry.entries.find((e) => e.id === entry.id);
    assert.deepEqual(kept, entry, `union entry preserved: ${entry.id}`);
    assert.ok(successorIds.has(entry.id));
  }
  checked.push('union-registry-successor');
}
// 2. Boundary profiles: 0.8.0 pins, module rules enabled, default/strict
// differ only in the portability override.
{
  for (const profile of ['default', 'strict']) {
    const document = read(`contracts/validation-profile.${profile}.v0.8.0.json`);
    assert.equal(document.schema_version, 'lekalo/validation-profile/v0.8.0');
    assert.equal(document.identity, 'dev.lekalo.validation-profile@0.8.0');
    assert.equal(document.version, '0.8.0');
    assert.equal(document.diagnostic_registry_version, '0.8.0');
    const ids = document.rules.map((r) => r.id);
    for (let i = 1; i < ids.length; i++) assert.ok(ids[i - 1] < ids[i], 'profile rules sorted-unique');
    for (const rule of registryRules()) assert.ok(ids.includes(rule), `profile lists ${rule}`);
  }
  function registryRules() {
    return read('contracts/diagnostic-registry.v0.8.0.json').entries.map((e) => e.id).filter((id) => id.startsWith('module-api.'));
  }
  const def = read('contracts/validation-profile.default.v0.8.0.json');
  const strict = read('contracts/validation-profile.strict.v0.8.0.json');
  assert.equal(def.rules.length, strict.rules.length);
  const differences = def.rules.filter((rule, i) => JSON.stringify(rule) !== JSON.stringify(strict.rules[i]));
  assert.deepEqual(differences, [{ id: 'semantic.portable-target-reference', enabled: true, severity_override: 'info' }]);
  checked.push('boundary-profiles');
}
// 3. Golden schemas: every committed document of the family validates.
{
  validates('module-api', read(`${reference}/module-api.json`));
  for (const side of ['before', 'after']) validates('module-api', read(`${fixture}/diff/${side}/module-api.json`));
  validates('module-api-report', read(`${fixture}/valid/golden-report.json`));
  validates('module-public-contract', read(`${fixture}/standalone/golden-bundle.json`));
  for (const side of ['before', 'after']) validates('module-public-contract', read(`${fixture}/diff/golden-bundle-${side}.json`));
  validates('module-api-consumers', read(`${fixture}/standalone/golden-consumers.json`) ?? read(`${fixture}/diff/golden-consumers-before.json`));
  validates('module-api-lock', read(`${fixture}/lock/golden-lock.json`));
  validates('module-api-diff', read(`${fixture}/diff/golden-diff.json`).moduleApiDiff);
  validates('module-consumer-context', read(`${fixture}/standalone/golden-context.json`));
  validates('module-consumer-context', read(`${fixture}/standalone/golden-context-tiny.json`));
  checked.push('closed-schemas');
}
// 4. Reference check: advisory pass, deterministic bytes, digest pins.
{
  const args = ['module', 'api', 'check', '--project', reference, '--api', `${reference}/module-api.json`];
  const first = run(args);
  const second = run(args);
  assert.equal(first.stdout, second.stdout, 'check is byte-deterministic');
  const report = payload(first.document, 'moduleApiReport');
  assert.deepEqual(report, read(`${fixture}/valid/golden-report.json`));
  assert.equal(report.assessment, 'pass');
  assert.equal(report.complete, true);
  assert.equal(report.pins.registryVersion, '0.8.0');
  assert.match(report.pins.irDigest, /^sha256:[0-9a-f]{64}$/);
  checked.push('reference-check-deterministic');
}
// 5. Visibility: project-visible-but-unexported is invalid under strict,
// advisory default keeps exit 0 with the full evidence, and the local
// private helper never fires.
{
  const visibility = `${fixture}/visibility`;
  const strict = run(['module', 'api', 'check', '--project', visibility, '--api', `${visibility}/module-api.json`, '--strict'], { exit: 1 });
  assert.ok(codes(strict.document).includes('LEK-MODULE-003'), 'symbol-not-exported strict');
  assert.ok(codes(strict.document).includes('LEK-MODULE-005'), 'transitive closure exposure strict');
  assert.equal(strict.stderr !== '', true);
  const advisory = run(['module', 'api', 'check', '--project', visibility, '--api', `${visibility}/module-api.json`]);
  assert.equal(advisory.document.status, 'valid');
  const report = payload(advisory.document, 'moduleApiReport');
  assert.equal(report.assessment, 'fail');
  assert.ok(report.findings.some((f) => f.code === 'LEK-MODULE-003' && f.export === 'tasks.task_state'));
  assert.ok(report.findings.some((f) => f.code === 'LEK-MODULE-005' && f.path.join('>').includes('tasks.task_view')));
  assert.ok(!report.findings.some((f) => (f.subject ?? '').includes('planner.internal_note')), 'local private helper is legal');
  checked.push('visibility-strict-and-advisory');
}
// 6. Writes: the cross-owner write is invalid under strict; the same
// project with a narrow shared-aggregate grant passes; an entity-wide or
// delete write can never bypass a field-only grant.
{
  const writes = `${fixture}/writes`;
  const strict = run(['module', 'api', 'check', '--project', writes, '--api', `${writes}/module-api.json`, '--strict'], { exit: 1 });
  assert.ok(codes(strict.document).includes('LEK-MODULE-007'));
  const advisory = run(['module', 'api', 'check', '--project', writes, '--api', `${writes}/module-api.json`]);
  const finding = payload(advisory.document, 'moduleApiReport')?.findings?.find((f) => f.code === 'LEK-MODULE-007');
  assert.equal(finding.producer, 'tasks');
  assert.equal(finding.consumer, 'planner');
  const grant = JSON.parse(JSON.stringify(read(`${writes}/module-api.json`)));
  grant.modules.find((m) => m.moduleId === 'tasks').grants = [{
    grant: 'planner-narrow-update',
    consumer: 'planner',
    aggregate: 'tasks.task',
    operations: ['update'],
    fields: ['*'],
    rationale: 'planner owns the scheduling state of the shared aggregate',
  }];
  const narrow = run(['module', 'api', 'check', '--project', writes, '--api', temp('writes-narrow.json', grant), '--strict']);
  assert.equal(payload(narrow.document, 'moduleApiReport').assessment, 'pass', 'narrow grant admits the exact write');
  const fieldGrant = JSON.parse(JSON.stringify(grant));
  fieldGrant.modules.find((m) => m.moduleId === 'tasks').grants[0].fields = ['title'];
  const bypass = run(['module', 'api', 'check', '--project', writes, '--api', temp('writes-field.json', fieldGrant), '--strict'], { exit: 1 });
  assert.ok(codes(bypass.document).includes('LEK-MODULE-007'), 'field-only grant never covers an entity-wide write');
  checked.push('writes-grants');
}
// 7. Portability: target-specific exposure is invalid under strict and
// the uncertified bundle is refused even in advisory mode.
{
  const portability = `${fixture}/portability`;
  const strict = run(['module', 'api', 'check', '--project', portability, '--api', `${portability}/module-api.json`, '--strict'], { exit: 1 });
  assert.ok(codes(strict.document).includes('LEK-MODULE-006'));
  const denied = run(['module', 'api', 'export', 'tasks', '--project', portability, '--api', `${portability}/module-api.json`], { exit: 1 });
  assert.ok(codes(denied.document).includes('LEK-MODULE-006'), 'no certified bundle from an illegal closure');
  checked.push('portability-leak-refused');
}
// 8. Cycles: the fatal declared import cycle keeps the existing graph
// rule and an ordered path; the check command fails before any graph.
{
  const cycles = `${fixture}/cycles`;
  const failed = run(['validate', '--project', cycles], { exit: 1 });
  assert.ok(codes(failed.document).includes('LEK-LOAD-008'), 'fatal import cycle keeps LEK-LOAD-008');
  checked.push('fatal-cycle-existing-rule');
}
// 9. Export: certified bundle, stable digest, closed export kinds, and
// the public DTO contains no internal aggregate shape.
{
  const exported = run(['module', 'api', 'export', 'tasks', '--project', reference, '--api', `${reference}/module-api.json`]);
  const bundle = payload(exported.document, 'modulePublicContract');
  const again = run(['module', 'api', 'export', 'tasks', '--project', reference, '--api', `${reference}/module-api.json`]);
  assert.equal(exported.stdout, again.stdout, 'export is byte-deterministic');
  assert.equal(bundle.digest, read(`${fixture}/standalone/golden-bundle.json`).digest);
  assert.equal(bundle.apiVersion, '1.2.0');
  assert.equal(bundle.exports.length, 9);
  assert.ok(bundle.exports.every((e) => !['tasks.task'].includes(e.export)), 'internal entity shape never exported');
  assert.ok(!JSON.stringify(bundle).includes('internal_note'), 'private fields never leak');
  // The digest covers canonical public semantics: reformatting the
  // project documents cannot change it (source-order/layout freedom).
  const reformatted = JSON.parse(JSON.stringify(read(`${reference}/module-api.json`)));
  reformatted.modules = reformatted.modules.slice().reverse();
  const same = run(['module', 'api', 'export', 'tasks', '--project', reference, '--api', temp('attachment-reordered.json', reformatted)]);
  assert.equal(payload(same.document, 'modulePublicContract').digest, bundle.digest, 'attachment row order is not semantics');
  checked.push('certified-export');
}
// 10. Consumers: declared and resolved uses, policy retained, zero-use
// rows impossible to lose, immutable snapshot digest.
{
  const consumerRun = run(['module', 'api', 'consumers', 'tasks', '--project', reference, '--api', `${reference}/module-api.json`]);
  if (!consumerRun.document.moduleApiConsumers) {
    console.error('DEBUG', JSON.stringify({
      stdout: consumerRun.stdout?.length,
      stderr: consumerRun.stderr?.slice(0, 400),
      keys: Object.keys(consumerRun.document),
    }));
  }
  const snapshot = consumerRun.document.moduleApiConsumers;
  const planner = snapshot.consumers.find((c) => c.consumer === 'planner');
  assert.ok(planner?.declared);
  assert.equal(planner.policy.type, 'version-range');
  assert.ok(planner.uses.some((u) => u.origin === 'declared' && u.export === 'tasks.task_view'));
  assert.ok(planner.uses.some((u) => u.origin === 'resolved'), 'resolved uses are joined');
  assert.equal(snapshot.complete, false, 'external callers stay explicitly unknown');
  checked.push('consumer-snapshot');
}
// 11. Lock: candidate on stdout only; fresh check passes; stale denies
// with LEK-MODULE-013 + LEK-MODULE-014 and full evidence; misuse is
// usage-invalid; nothing is ever written.
{
  const lock = `${fixture}/lock`;
  const before = readdirSync(join(root, lock)).sort().join(',');
  const candidate = run(['module', 'api', 'lock', '--project', lock, '--api', `${lock}/module-api.json`, '--module', 'tasks', '--consumer', 'planner']);
  const candidateDoc = payload(candidate.document, 'moduleApiLock');
  assert.deepEqual(candidateDoc, read(`${lock}/golden-lock.json`));
  assert.equal(readdirSync(join(root, lock)).sort().join(','), before, 'lock never writes a file');
  run(['module', 'api', 'lock', '--project', lock, '--api', `${lock}/module-api.json`, '--module', 'tasks', '--consumer', 'planner', '--check', '--lock', `${lock}/golden-lock.json`]);
  const stale = JSON.parse(JSON.stringify(read(`${lock}/module-api.json`)));
  stale.modules.find((m) => m.moduleId === 'tasks').apiVersion = '1.3.0';
  const stalePath = temp('lock-stale-attachment.json', stale);
  const denied = run(['module', 'api', 'lock', '--project', lock, '--api', stalePath, '--module', 'tasks', '--consumer', 'planner', '--check', '--lock', `${lock}/golden-lock.json`], { exit: 3 });
  assert.ok(codes(denied.document).includes('LEK-MODULE-013'), 'stale lock keeps its finding');
  assert.ok(codes(denied.document).includes('LEK-MODULE-014'), 'stale lock denies policy');
  assert.ok(payload(denied.document, 'moduleApiLock'), 'denial retains the evidence report');
  run(['module', 'api', 'lock', '--project', lock, '--api', `${lock}/module-api.json`, '--module', 'tasks', '--consumer', 'planner', '--lock', `${lock}/golden-lock.json`], { exit: 1 });
  checked.push('lock-denial');
}
// 12. Diff: exact consumers and breaking changes; unaffected controls;
// strict policy denial; hostile digest-tampered bundles refused.
{
  const diff = `${fixture}/diff`;
  const args = ['module', 'api', 'diff', `${diff}/golden-bundle-before.json`, `${diff}/golden-bundle-after.json`, '--consumers-before', `${diff}/golden-consumers-before.json`, '--consumers-after', `${diff}/golden-consumers-after.json`];
  const report = payload(run(args).document, 'moduleApiDiff');
  assert.deepEqual(report, read(`${diff}/golden-diff.json`).moduleApiDiff);
  assert.equal(report.verdict, 'policy-denied');
  const removed = report.changes.find((c) => c.reason === 'field-removed');
  assert.equal(removed.export, 'tasks.task_view');
  assert.equal(removed.member, 'state');
  assert.equal(removed.class, 'source-breaking');
  const planner = report.consumers.find((c) => c.consumer === 'planner');
  assert.equal(planner.status, 'affected');
  assert.ok(planner.changes.includes('tasks.task_view'));
  const provider = report.consumers.find((c) => c.consumer === 'provider');
  assert.equal(provider.status, 'unaffected', 'the unchanged-event consumer is a control');
  assert.equal(report.coverage.known, 1);
  assert.deepEqual(report.coverage.unknownReasons, ['external-callers-unknown']);
  assert.equal(report.versionPolicy.decision, 'violation');
  assert.equal(report.versionPolicy.expected, 'major');
  const strict = run([...args, '--strict'], { exit: 3 });
  assert.ok(codes(strict.document).includes('LEK-MODULE-014'));
  // Digest-valid but tampered semantics: flip a field name and re-pin a
  // stale digest — refused, never trusted because of its hash spelling.
  const tampered = JSON.parse(JSON.stringify(read(`${diff}/golden-bundle-after.json`)));
  tampered.exports.find((e) => e.export === 'tasks.get_task').reads = [];
  run(['module', 'api', 'diff', `${diff}/golden-bundle-before.json`, temp('bundle-tampered.json', tampered)], { exit: 1 });
  checked.push('diff-exact-partition');
}
// 13. Context: public-only capsule without the provider checkout, pinned
// estimator, fit boundary, and bounded inputs.
{
  const standalone = `${fixture}/standalone`;
  const bundlePath = join(root, standalone, 'golden-bundle.json');
  const isolated = mkdtempSync(join(tmpdir(), 'lekalo-module-api-no-provider-'));
  const isolatedBundle = join(isolated, 'golden-bundle.json');
  writeFileSync(isolatedBundle, readFileSync(bundlePath));
  const args = ['module', 'api', 'context', isolatedBundle, '--consumer', 'planner', '--uses', 'tasks.task_view,tasks.get_task', '--budget', '4000'];
  const capsule = payload(run(args, { cwd: isolated }).document, 'moduleConsumerContext');
  assert.deepEqual(capsule, read(`${standalone}/golden-context.json`));
  assert.equal(capsule.fits, true);
  assert.deepEqual(capsule.estimator, { id: 'module-context-estimator', recipe: 'public-facts-chars4/1', version: '1' });
  assert.ok(capsule.exports.some((e) => e.export === 'tasks.task_view'), 'required public facts retained');
  const tiny = payload(run([...args.slice(0, -1), '16'], { cwd: isolated }).document, 'moduleConsumerContext');
  assert.equal(tiny.fits, false, 'an over-budget capsule never claims fit');
  assert.deepEqual(tiny.gaps, ['budget-exceeded']);
  run(['module', 'api', 'context', isolatedBundle, '--consumer', 'planner', '--uses', 'tasks.task_view,tasks.task_view', '--budget', '4000'], { exit: 1 });
  run(['module', 'api', 'context', isolatedBundle, '--consumer', 'planner', '--uses', 'tasks.task_view', '--budget', '0'], { exit: 1 });
  run(['module', 'api', 'context', isolatedBundle, '--consumer', 'planner', '--uses', 'tasks.task', '--budget', '4000'], { exit: 1 });
  // Missing required public dependency materialization is an explicit gap.
  const dep = run(['module', 'api', 'context', isolatedBundle, '--consumer', 'planner', '--uses', 'core.uuid', '--budget', '4000'], { exit: 1 });
  assert.ok(codes(dep.document).includes('LEK-MODULE-001'));
  rmSync(isolated, { recursive: true, force: true });
  checked.push('public-only-context');
}
// 14. Mapping conformance: the same portable digest and typed operation
// set back the in-process consumer and the declared service port.
{
  const bundle = read(`${fixture}/standalone/golden-bundle.json`);
  const capsule = read(`${fixture}/standalone/golden-context.json`);
  assert.equal(bundle.digest, capsule.producerDigest, 'context capsule binds the same digest');
  // The lock family pins its own producer revision: the current export of
  // the lock project must equal the committed lock digest exactly.
  const lock = `${fixture}/lock`;
  const current = run(['module', 'api', 'export', 'tasks', '--project', lock, '--api', `${lock}/module-api.json`]).document.modulePublicContract;
  assert.equal(current.digest, read(`${lock}/golden-lock.json`).digest, 'lock pins the current portable digest');
  const port = bundle.ports.find((p) => p.port === 'tasks.api');
  assert.ok(port, 'service port descriptor present');
  const exportIds = new Set(bundle.exports.map((e) => e.export));
  assert.ok(port.operations.every((operation) => exportIds.has(operation)), 'port references exact typed operations');
  assert.ok(bundle.capabilities.some((c) => c.capability === 'http' && c.obligation === 'required'), 'capability obligation retained');
  checked.push('mapping-same-digest');
}
// 15. Hostile inputs and version refusal.
{
  const attachment = readFileSync(join(root, reference, 'module-api.json'), 'utf8');
  const duplicated = attachment.replace('"schemaVersion":', '"schemaVersion": "lekalo/module-api/v0.8.0", "identity": "dev.lekalo.module-api@0.8.0", "schemaVersion":');
  run(['module', 'api', 'check', '--project', reference, '--api', temp('attachment-duplicate.json', duplicated)], { exit: 1 });
  const future = JSON.parse(attachment.replace('v0.8.0', 'v9.9.9'));
  const refused = run(['module', 'api', 'check', '--project', reference, '--api', temp('attachment-future.json', future)], { exit: 5 });
  assert.ok(codes(refused.document).includes('LEK-MODULE-002'), 'unknown family version is unsupported-version');
  checked.push('duplicate-keys-and-versions');
}
// 16. Validate seam: accepted base behavior with the boundary gate; the
// strict boundary violation invalidates (exit 1) even over a base strict
// denial; the stale checked lock denies; the plain validate run is
// untouched.
{
  run(['validate', '--project', reference]);
  const seam = run(['validate', '--project', reference, '--module-api', `${reference}/module-api.json`]);
  assert.equal(seam.document.status, 'valid');
  const visibility = `${fixture}/visibility`;
  const strict = run(['validate', '--project', visibility, '--strict', '--module-api', `${visibility}/module-api.json`], { exit: 1 });
  assert.ok(codes(strict.document).includes('LEK-MODULE-003'), 'strict seam boundary invalid');
  const lock = `${fixture}/lock`;
  const stale = JSON.parse(JSON.stringify(read(`${lock}/module-api.json`)));
  stale.modules.find((m) => m.moduleId === 'tasks').apiVersion = '1.3.0';
  const denied = run(['validate', '--project', lock, '--module-api', temp('seam-stale-attachment.json', stale), '--module-api-lock', `${lock}/golden-lock.json`], { exit: 3 });
  assert.ok(codes(denied.document).includes('LEK-MODULE-013'));
  checked.push('validate-seam');
}
// 17. Read-only producer: no checkout mutations and no runtime cache.
{
  const inventory = (dir) => readdirSync(join(root, dir), { recursive: true }).sort().join(',');
  const projects = [reference, `${fixture}/visibility`, `${fixture}/writes`, `${fixture}/lock`, `${fixture}/diff/before`];
  const before = projects.map(inventory);
  run(['module', 'api', 'check', '--project', reference, '--api', `${reference}/module-api.json`, '--no-cache']);
  run(['module', 'api', 'export', 'tasks', '--project', reference, '--api', `${reference}/module-api.json`, '--no-cache']);
  const after = projects.map(inventory);
  assert.deepEqual(after, before, 'no file, lock, cache, or baseline writes');
  checked.push('read-only-producer');
}

process.stdout.write(JSON.stringify({ ok: true, gate: 'module-api-contracts', ajv: '8.17.1', schemas: families.length, checks: checked.length, checked }) + '\n');
