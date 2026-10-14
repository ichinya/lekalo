// Issue #41: managed ownership and deterministic implementation generation.
//
// Explicit authoring; running without arguments (re)writes the committed
// contract artifacts, `--check` is read-only and used by the live gate.
// The managed lane is additive: seven new closed families at the product
// version plus the diagnostic-registry union successor; every frozen
// predecessor keeps its exact bytes.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

export const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export const version = '0.8.0';
export const ordered = v => Array.isArray(v) ? v.map(ordered) : v && typeof v === 'object' ? Object.fromEntries(Object.keys(v).sort().map(k => [k, ordered(v[k])])) : v;
export const canonical = v => JSON.stringify(ordered(v));
export const digest = v => 'sha256:' + createHash('sha256').update(v).digest('hex');
export const hash = v => digest(canonical(v));
export const header = family => ({ schemaVersion: `lekalo/${family}/v${version}`, identity: `dev.lekalo.${family}@${version}` });
const pretty = v => JSON.stringify(v, null, 2) + '\n';
const read = path => JSON.parse(readFileSync(resolve(root, path), 'utf8'));

const token = { type: 'string', pattern: '^[A-Za-z0-9_.:/@-]+$', minLength: 1, maxLength: 256 };
// A project-relative coordinate: no leading separator, no `..`, no `\`,
// no drive or device spelling; every segment is a bounded safe token.
const relPath = { type: 'string', pattern: '^[A-Za-z0-9][A-Za-z0-9_.@-]*(/[A-Za-z0-9][A-Za-z0-9_.@-]*)*$', minLength: 1, maxLength: 512 };
const pin = { type: 'string', pattern: '^sha256:[a-f0-9]{64}$' };
const prose = { type: 'string', minLength: 1, maxLength: 2000, pattern: '^[^\\u0000-\\u001f\\u007f]+$' };
const en = (...values) => ({ enum: values });
const ref = name => ({ $ref: `#/$defs/${name}` });
const obj = properties => ({ type: 'object', properties, required: Object.keys(properties), additionalProperties: false });
const objOpt = (properties, optional) => ({ type: 'object', properties, required: Object.keys(properties).filter(k => !optional.includes(k)), additionalProperties: false });
const arr = (items, maxItems = 256, minItems = 0) => ({ type: 'array', items, maxItems, minItems });
// A closed optional reference: `{ "state": "known", "value": ... }` or
// `{ "state": "absent" }`; unknown states are refused.
const state = (value, states = ['absent']) => ({ oneOf: [obj({ state: { const: 'known' }, value }), ...states.map(s => obj({ state: { const: s } }))] });
const scalar = { anyOf: [{ type: 'string' }, { type: 'number' }, { type: 'boolean' }, { type: 'null' }] };
const knownPin = state(pin);
const knownDigest = state(obj({ generationDigest: pin }));
// The reserved derived authority pins the committed closure carries: the
// canonical Model/IR/lock custody evidence (finding 8) is not a project
// file, so the record closure grammar admits exactly these coordinates.
const authorityPinPath = { type: 'string', enum: ['.lekalo/managed-authority/model', '.lekalo/managed-authority/ir', '.lekalo/managed-authority/lock'] };
// The governed public manifest coordinate a transaction journal moves.
const manifestPath = { type: 'string', const: '.lekalo/generated/manifests/ownership.json' };
// A committed closure path: a project-relative coordinate or a derived
// authority pin.
const closurePath = { anyOf: [relPath, authorityPinPath] };
// A pending/conflict path: a project-relative coordinate or the governed
// manifest.
const transactionPath = { anyOf: [relPath, manifestPath] };
const knownConflict = state(obj({ path: transactionPath, reason: token }));

const requirementKinds = ['assertion', 'command', 'effect', 'entity', 'event', 'expression', 'invariant', 'job', 'policy', 'port', 'query', 'scenario', 'transaction'];
const supportLevels = ['full', 'partial', 'unsupported', 'unknown'];

const defs = {
  reference: obj({ id: token, version: token, digest: pin }),
  moduleRef: token,
  targetRef: token,
  extension: obj({ symbol: token, binding: token, contractDigest: pin }),
  extensionCheck: obj({ symbol: token, binding: token, contractDigest: pin, validated: { type: 'boolean' } }),
  targetAssignment: obj({ target: ref('targetRef'), profileRef: ref('reference'), implementationRef: ref('reference'), generatedRoot: relPath, extensions: arr(ref('extension'), 64) }),
  assignment: obj({ module: ref('moduleRef'), mode: en('observed', 'contracted', 'managed'), targets: arr(ref('targetAssignment'), 8, 1) }),
  requirement: objOpt({
    id: token,
    kind: en(...requirementKinds),
    support: en(...supportLevels),
    witness: knownPin,
    detail: prose,
    projection: arr(obj({ from: token, to: token }), 64),
    sample: state(obj({ input: { type: 'object', additionalProperties: scalar, maxProperties: 32 }, output: { type: 'object', additionalProperties: scalar, maxProperties: 32 } })),
  }, ['detail']),
  adapter: obj({ entry: relPath, packageDigest: pin, protocol: token }),
  outputAction: en('create', 'replace', 'delete', 'unchanged'),
  artifactRole: en('custody', 'implementation', 'manifest', 'map', 'test'),
  planOutput: obj({ path: relPath, action: ref('outputAction'), role: ref('artifactRole'), ownerModule: ref('moduleRef'), ownerTarget: ref('targetRef'), beforeDigest: knownPin, afterDigest: knownPin }),
  recordOutput: obj({ path: relPath, role: ref('artifactRole'), ownerModule: ref('moduleRef'), ownerTarget: ref('targetRef'), digest: pin, beforeRef: knownPin, afterRef: knownPin }),
  pendingOp: obj({ path: transactionPath, op: en('publish', 'delete', 'restore') }),
  restoreOp: obj({ path: relPath, digest: pin, ref: pin }),
  deleteOp: obj({ path: relPath, digest: pin }),
  scenarioVerdict: obj({ scenario: token, digest: pin, verdict: en('match', 'mismatch', 'unsupported') }),
  // One executed-outcome receipt (r4 finding 2): the digest of the
  // observed output of executing that scenario's retained executable
  // test. The verdict labels stay caller-authored; the receipts are what
  // unbound gate independently recomputes from retained coverage. Bound
  // receipts are explicit caller-supplied native-run attestations: the
  // offline verifier checks pins and consistency, not execution.
  scenarioExecution: obj({ scenario: token, observed: pin }),
};

const shape = (family, properties) => objOpt({ ...Object.fromEntries(Object.entries(header(family)).map(([k, v]) => [k, { const: v }])), ...properties }, family === 'managed-ownership' ? ['inputs'] : []);

export const shapes = {
  'managed-ownership': { project: token, modules: arr(ref('assignment'), 64, 1), inputs: { ...arr(relPath, 4095), uniqueItems: true } },
  'managed-coverage': { module: ref('moduleRef'), target: ref('targetRef'), recipe: ref('reference'), adapter: ref('adapter'), authority: state(obj({ model: pin, ir: pin, lock: pin })), requirements: arr(ref('requirement'), 4096), extensions: arr(ref('extensionCheck'), 64), complete: { type: 'boolean' } },
  'managed-generation-plan': {
    action: en('promotion', 'regeneration', 'upgrade'),
    modules: arr(ref('moduleRef'), 64, 1),
    targets: arr(ref('targetRef'), 8, 1),
    inputClosure: arr(obj({ path: relPath, digest: pin }), 4096),
    beforeManifest: knownDigest,
    coverageDigest: pin,
    extensions: arr(ref('extension'), 64),
    outputs: arr(ref('planOutput'), 8192, 1),
    planDigest: pin,
  },
  'managed-generation-record': {
    planDigest: pin,
    generationDigest: pin,
    previousGeneration: knownDigest,
    inputClosure: arr(obj({ path: closurePath, digest: pin }), 4096),
    outputs: arr(ref('recordOutput'), 8192, 1),
    ownershipDigest: pin,
    lockPin: knownPin,
    recordDigest: pin,
  },
  'managed-transaction': {
    transactionId: token,
    state: en('prepared', 'publishing', 'committed', 'restoring', 'recovered', 'abandoned'),
    action: en('apply', 'rollback', 'resolve'),
    planDigest: knownPin,
    generationDigest: knownDigest,
    beforeManifestDigest: knownPin,
    afterManifestDigest: knownPin,
    pending: arr(ref('pendingOp'), 8192),
    affectedFiles: arr(relPath, 8192),
    conflict: knownConflict,
    beforeDigest: knownPin,
  },
  'managed-rollback-plan': {
    currentGeneration: knownDigest,
    destinationGenerationDigest: pin,
    restore: arr(ref('restoreOp'), 8192),
    delete: arr(ref('deleteOp'), 8192),
    manifestDigest: pin,
    planDigest: pin,
  },
  'managed-equivalence': {
    module: ref('moduleRef'),
    target: ref('targetRef'),
    implementationDigest: pin,
    referenceDigest: knownPin,
    scenarios: arr(ref('scenarioVerdict'), 4096, 1),
    execution: arr(ref('scenarioExecution'), 4096, 1),
    gaps: arr(token, 4096),
    verdict: en('match', 'mismatch', 'incomplete'),
  },
};

const used = new Set();
const visit = value => {
  if (Array.isArray(value)) value.forEach(visit);
  else if (value && typeof value === 'object') {
    if (value.$ref) {
      const name = value.$ref.split('/').at(-1);
      if (!used.has(name)) {
        used.add(name);
        visit(defs[name]);
      }
    }
    Object.values(value).forEach(visit);
  }
};

export function artifacts() {
  const files = new Map();
  const predecessor = read('contracts/diagnostic-registry.v0.6.5.json');
  assert.ok(predecessor.entries.length >= 500, 'predecessor registry shrank');
  // Issue #41 managed family: twelve distinct admission/transaction/coverage
  // decisions. Allowed statuses follow the research table; every entry keeps
  // the closed data-field grammar (module/target/requirement/path/pin/reason).
  const managedFields = [
    { name: 'module', type: 'token' },
    { name: 'target', type: 'token' },
    { name: 'requirement', type: 'token' },
    { name: 'path', type: 'token' },
    { name: 'pin', type: 'token' },
    { name: 'reason', type: 'token' },
  ];
  const managed = [
    ['input-invalid', 'invalid', 'The managed ownership, plan, history, or confirmation input is invalid.'],
    ['version-unsupported', 'unsupported-version', 'The managed contract, recipe, or protocol version is not supported.'],
    ['capability-gap', 'unsupported', 'A required generation or assertion semantic lacks qualified full support.'],
    ['generated-drift', 'invalid', 'Recorded generated bytes were manually changed or are missing at apply preflight.'],
    ['extension-unresolved', 'invalid', 'A required custom or foreign binding selection is missing, ambiguous, stale, or unvalidated.'],
    ['plan-stale', 'invalid', 'A pinned input, lock, adapter, binding, or before-state differs from the reviewed plan.'],
    ['artifact-coverage-incomplete', 'invalid', 'The output inventory is not a complete, owned, mapped manifest of the declared generated roots.'],
    ['equivalence-failed', 'invalid', 'Qualified supported behavior differs on an independently checked assertion.'],
    ['equivalence-incomplete', 'unsupported', 'Required oracle, scenario, or native equivalence evidence is missing, unsupported, or stale.'],
    ['rollback-refused', 'invalid', 'The rollback destination is missing, forged, or incompatible with the current Model or generation.'],
    ['recovery-required', 'invalid', 'An unresolved managed transaction must be reconciled before any further mutation.'],
    ['ownership-denied', 'denied', 'The requested write is outside exact recorded generated ownership or protected bookkeeping.'],
  ];
  const entries = managed.map(([suffix, status, message], i) => ({
    id: `managed.${suffix}`,
    code: `LEK-MGD-${String(i + 1).padStart(3, '0')}`,
    category: 'infrastructure',
    default_severity: 'error',
    allowed_statuses: [status],
    message_id: `managed.${suffix}`,
    default_message: message,
    location_requirement: 'none',
    data_fields: managedFields,
    allowed_fix_ids: [],
    lifecycle: 'active',
  }));
  // The 0.8.0 registry is the merged milestone union: this generator owns
  // the managed family and carries every other merged family (format,
  // adoption, portability, worklog-changelog, change) verbatim from the
  // committed successor, so the artifact stays one sorted closed union.
  const committed = read(`contracts/diagnostic-registry.v${version}.json`);
  const predecessorIds = new Set(predecessor.entries.map(entry => entry.id));
  const carried = committed.entries.filter(
    entry => !predecessorIds.has(entry.id) && !entry.id.startsWith('managed.'),
  );
  const registry = {
    ...predecessor,
    schema_version: `lekalo/diagnostic-registry/v${version}`,
    identity: `dev.lekalo.diagnostic-registry@${version}`,
    registry_version: version,
    entries: [...predecessor.entries, ...carried, ...entries].sort((a, b) => (a.id < b.id ? -1 : 1)),
  };
  const registrySchema = JSON.parse(JSON.stringify(read('contracts/diagnostic-registry.schema.v0.6.4.json')).replaceAll('0.6.4', version));
  registrySchema.description = 'Merged additive union successor: the 522 predecessor entries are preserved verbatim, and this successor tightens the data-fields `name` grammar to `^[A-Za-z][A-Za-z0-9]{0,31}$` (all existing data names conform; no `maxLength` member is carried — the pattern bounds the length). Issue #94 adds the closed format.* family (LEK-FORMAT-001..009 with LEK-FORMAT-010..012 in later successors), issue #136 adds nine adoption.* rules (LEK-ADOPT-001..009) owning service-specific admission, consent, revision, verification, and recovery failures, issue #19 adds nine portability.* diagnostics (LEK-PORT-001..009), issue #51 adds seven worklog-changelog.* diagnostics (LEK-WC-001..007), issue #41 adds the managed.* infrastructure family, and issue #73 adds twelve change.* rules (LEK-CHANGE-001..012) owning the semantic Change API refusals; the merged registry carries all six families (582 entries). Wire diagnostics remain v0.2.16; predecessor producer registries (0.6.4 default, 0.6.5 architecture union) keep their exact pins.';
  files.set(`contracts/diagnostic-registry.v${version}.json`, pretty(registry));
  files.set(`contracts/diagnostic-registry.schema.v${version}.json`, pretty(registrySchema));
  for (const [family, properties] of Object.entries(shapes)) {
    used.clear();
    const body = shape(family, properties);
    visit(body);
    files.set(`contracts/${family}.schema.v${version}.json`, pretty({
      $schema: 'https://json-schema.org/draft/2020-12/schema',
      $id: `dev.lekalo.${family}-schema@${version}`,
      title: `Lekalo ${family} ${version}`,
      ...body,
      $defs: Object.fromEntries([...used].sort().map(k => [k, defs[k]])),
    }));
  }
  return files;
}

export function generate(check = false) {
  const files = artifacts();
  if (check) {
    for (const [path, bytes] of files) {
      let current;
      try {
        current = readFileSync(resolve(root, path), 'utf8');
      } catch {
        throw new Error(`missing contract artifact: ${path}`);
      }
      assert.equal(current, bytes, `contract drift: ${path}`);
    }
    return files.size;
  }
  for (const [path, bytes] of files) writeFileSync(resolve(root, path), bytes);
  return files.size;
}

if (import.meta.url === `file:///${process.argv[1].split('\\').join('/')}` || process.argv[1] && import.meta.url === new URL(`file:///${resolve(process.argv[1]).split('\\').join('/')}`).href) {
  const check = process.argv.includes('--check');
  const count = generate(check);
  console.log(JSON.stringify({ ok: true, check, artifacts: count, version }));
}
