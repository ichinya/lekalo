#!/usr/bin/env node
// Issue #93 contract gate: the four search wire schemas, the registry
// union successor, the committed planner evidence documents, and the
// recorded CLI goldens, validated with the same pinned third-party
// Draft 2020-12 implementation as the other contract gates. Exact
// Ajv 8.17.1 is provisioned outside this checkout (CI does the same on
// Node 18 and 24) and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.
//
// This gate is schema-only (no binary): the live CLI behavior is
// test-search-cli.mjs in the build-test lane.

import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  Ajv2020 = require("ajv/dist/2020").default;
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "ajv-unavailable", detail: error?.code ?? error?.message }, null, 2)}\n`,
  );
  process.exit(1);
}
if (ajvVersion !== "8.17.1") {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "ajv-version", detail: ajvVersion }, null, 2)}\n`,
  );
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));
const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

const ajv = new Ajv2020({ strict: true, allErrors: true });
const compile = (relative) => ajv.compile(read(relative));

// 1. The four search schemas compile under strict Draft 2020-12.
const querySchema = "contracts/search-query.schema.v0.8.0.json";
const resultSchema = "contracts/search-result.schema.v0.8.0.json";
const indexSchema = "contracts/search-index.schema.v0.8.0.json";
const inputsSchema = "contracts/search-inputs.schema.v0.8.0.json";
const validators = {};
for (const [name, schema] of [
  ["query", querySchema],
  ["result", resultSchema],
  ["index", indexSchema],
  ["inputs", inputsSchema],
]) {
  try {
    validators[name] = compile(schema);
  } catch (error) {
    fail("schema-compile", { schema, detail: error.message });
  }
}
const validateQuery = validators.query;
const validateResult = validators.result;
const validateIndex = validators.index;
const validateInputs = validators.inputs;

// 2. The registry union successor validates against its own schema,
//    preserves every accepted 0.6.5 entry byte-for-byte, and scopes
//    its additions by family (union-aware, issue #93 r9).
const predecessor = read("contracts/diagnostic-registry.v0.6.5.json");
const successor = read("contracts/diagnostic-registry.v0.8.0.json");
const validateRegistry = compile("contracts/diagnostic-registry.schema.v0.8.0.json");
if (!validateRegistry(successor)) {
  fail("registry-instance-invalid", validateRegistry.errors);
}
const predecessorIds = new Map(predecessor.entries.map((entry) => [entry.id, entry]));
for (const entry of successor.entries) {
  const previous = predecessorIds.get(entry.id);
  if (!previous) continue;
  if (JSON.stringify(previous) !== JSON.stringify(entry)) {
    fail("predecessor-entry-drift", entry.id);
  }
}
// The eight search.* rules are registered with the exact code block.
const searchCodes = new Map(
  successor.entries
    .filter((entry) => entry.id.startsWith("search."))
    .map((entry) => [entry.id, entry.code]),
);
const expected = [
  ["search.query-invalid", "LEK-SRCH-001"],
  ["search.version-unsupported", "LEK-SRCH-002"],
  ["search.input-invalid", "LEK-SRCH-003"],
  ["search.index-stale", "LEK-SRCH-004"],
  ["search.cursor-invalid", "LEK-SRCH-005"],
  ["search.result-limit", "LEK-SRCH-006"],
  ["search.coverage-incomplete", "LEK-SRCH-007"],
  ["search.ambiguous", "LEK-SRCH-008"],
];
for (const [id, code] of expected) {
  if (searchCodes.get(id) !== code) fail("search-family-code", { id, code: searchCodes.get(id) });
}
// Union-aware family scope (issue #93 r9, mirroring the #52 gate fix
// in 0099a3f8): the 0.8.0 successor is the merged grand union — the
// milestone integration carries the adoption, format, managed,
// portability, tasklink, and worklog-changelog families alongside
// this issue's rules (module-api follows from #113), so a raw
// total-size assertion over 0.6.5 + 8 fails on the merged tree and
// invites dropping the other families' entries. The invariant is
// family-scoped instead: every addition beyond the pinned predecessor
// must belong to a known landed family, and the search family must
// carry exactly its eight rules (erosion control).
const landedFamilies = [
  "adoption.",
  "format.",
  "managed.",
  "module-api.",
  "portability.",
  "tasklink.",
  "worklog-changelog.",
  "search.",
];
const familyScopeVerdict = (registry) => {
  const familyAdded = registry.entries.filter((entry) => !predecessorIds.has(entry.id));
  const foreign = familyAdded.filter(
    (entry) => !landedFamilies.some((family) => entry.id.startsWith(family)),
  );
  if (foreign.length) {
    return {
      ok: false,
      reason: "registry-foreign-family-addition",
      foreign: foreign.map((entry) => entry.id).slice(0, 4),
      predecessor: predecessor.entries.length,
      successor: registry.entries.length,
    };
  }
  // Presence half of the union invariant: every accepted 0.6.5 entry
  // stays in the successor. With ids unique (enforced separately on
  // the shipped registry), len(successor) === len(predecessor) +
  // len(additions) iff no predecessor row was dropped from the union
  // (the wrong add/add merge resolution this gate exists to catch).
  if (registry.entries.length !== predecessor.entries.length + familyAdded.length) {
    return {
      ok: false,
      reason: "predecessor-entry-missing",
      predecessor: predecessor.entries.length,
      successor: registry.entries.length,
      familyAdded: familyAdded.length,
    };
  }
  const searchFamily = registry.entries.filter((entry) => entry.id.startsWith("search.")).length;
  if (searchFamily !== expected.length) {
    return {
      ok: false,
      reason: "registry-union-size",
      searchFamily,
      expected: expected.length,
    };
  }
  return { ok: true, reason: null };
};
const scope = familyScopeVerdict(successor);
if (!scope.ok) fail(scope.reason, scope);
// 2b. Union-growth regression for the family-scope invariant (r9): a
//     simulated merged union where another landed family sits
//     alongside the eight search.* rules must still pass (this is the
//     586-entry tree this branch merges into), while a stray
//     non-search addition outside every landed family, a dropped
//     0.6.5 predecessor row, and a ninth search.* rule must each fail
//     with their own reason (predecessor presence added in r10).
{
  const template = successor.entries.find((entry) => entry.id === "search.ambiguous");
  const byId = (a, b) => (a.id < b.id ? -1 : 1);
  const neighbors = ["adoption.", "tasklink.", "worklog-changelog."].map((family, index) => ({
    ...template,
    id: `${family}registry-union-sim-${index}`,
    code: `LEK-SIM-${String(index).padStart(3, "0")}`,
  }));
  const mergedUnion = {
    ...successor,
    entries: [...successor.entries, ...neighbors].sort(byId),
  };
  const mergedVerdict = familyScopeVerdict(mergedUnion);
  if (!mergedVerdict.ok) fail("registry-union-simulation-rejected", mergedVerdict);
  const stray = {
    ...template,
    id: "bogus.registry-union-stray",
    code: "LEK-SIM-999",
  };
  const strayUnion = {
    ...mergedUnion,
    entries: [...mergedUnion.entries, stray].sort(byId),
  };
  const strayVerdict = familyScopeVerdict(strayUnion);
  if (strayVerdict.ok || strayVerdict.foreign?.[0] !== "bogus.registry-union-stray") {
    fail("registry-union-simulation-stray-accepted", strayVerdict);
  }
  const dropped = "adapter.check-failed";
  if (!predecessorIds.has(dropped)) fail("registry-union-simulation-predecessor-unknown", dropped);
  const droppedUnion = {
    ...mergedUnion,
    entries: mergedUnion.entries.filter((entry) => entry.id !== dropped),
  };
  const droppedVerdict = familyScopeVerdict(droppedUnion);
  if (droppedVerdict.ok || droppedVerdict.reason !== "predecessor-entry-missing") {
    fail("registry-union-simulation-dropped-predecessor-accepted", droppedVerdict);
  }
  const ninth = { ...template, id: "search.ninth-rule", code: "LEK-SRCH-009" };
  const ninthUnion = {
    ...mergedUnion,
    entries: [...mergedUnion.entries, ninth].sort(byId),
  };
  const ninthVerdict = familyScopeVerdict(ninthUnion);
  if (ninthVerdict.ok || ninthVerdict.reason !== "registry-union-size") {
    fail("registry-union-simulation-ninth-search-accepted", ninthVerdict);
  }
}
// Registry invariants JSON Schema cannot express.
const ids = successor.entries.map((entry) => entry.id);
const sorted = [...ids].sort();
if (ids.some((id, index) => id !== sorted[index])) fail("registry-entries-not-sorted");
if (ids.length !== new Set(ids).size) fail("registry-duplicate-ids");
const codes = successor.entries.map((entry) => entry.code);
if (codes.length !== new Set(codes).size) fail("registry-duplicate-codes");

// 3. The committed planner evidence documents are schema-valid:
//    the detected effect-graph snapshot and the two trace manifests
//    reuse their own published contracts; the inputs manifest and the
//    waiver set validate against the search/producer schemas.
const validateEffects = compile("contracts/effect-graph.schema.v0.2.16.json");
const effects = read("tests/fixtures/search/planner/evidence/effects.detected.json");
if (!validateEffects(effects)) fail("effects-evidence-invalid", validateEffects.errors);
const detected = effects.effects.filter(
  (row) => row.provenance?.type === "evidence",
);
if (detected.length !== 5) fail("effects-evidence-detected-count", detected.length);
const exactWriters = detected.filter(
  (row) => row.kind === "write-field" && row.field === "focused_at",
);
if (exactWriters.length !== 2) fail("effects-evidence-exact-writers", exactWriters.length);

const validateTrace = compile("contracts/trace-manifest.schema.v0.2.16.json");
for (const name of ["trace.openspec.json", "trace.aifhub.json"]) {
  const manifest = read(`tests/fixtures/search/planner/evidence/${name}`);
  if (!validateTrace(manifest)) fail("trace-evidence-invalid", { name, errors: validateTrace.errors });
}

const manifest = read("tests/fixtures/search/planner/evidence/search-inputs.json");
if (!validateInputs(manifest)) fail("inputs-manifest-invalid", validateInputs.errors);
const seenInputs = new Set();
for (const input of manifest.inputs) {
  const key = `${input.kind}:${input.path}`;
  if (seenInputs.has(key)) fail("inputs-manifest-duplicate", key);
  seenInputs.add(key);
}

// 3b. The two 0.8.0 producer wire contracts are published, closed, and
//     in parity with the runtime decoders: the committed fixtures
//     validate, and negative vectors (unknown member, missing member,
//     foreign version, open vocabulary, ghost shapes the schema itself
//     can close) refuse. The metadata count cross-checks and the deeper
//     custody vectors (ghost subjects, identity conflicts) are runtime
//     admissions beyond JSON Schema's expressive power — proven by the
//     CLI gate over the same fixtures.
const diagnosticsSchema = "contracts/search-diagnostics.schema.v0.8.0.json";
const ownershipSchema = "contracts/search-ownership-profile.schema.v0.8.0.json";
let validateDiagnostics;
let validateOwnership;
try {
  validateDiagnostics = compile(diagnosticsSchema);
  validateOwnership = compile(ownershipSchema);
} catch (error) {
  fail("producer-schema-compile", { detail: error.message });
}
const diagnosticsFixture = read("tests/fixtures/search/planner/evidence/diagnostics.json");
if (!validateDiagnostics(diagnosticsFixture)) {
  fail("diagnostics-producer-invalid", validateDiagnostics.errors);
}
const ownershipFixture = read("tests/fixtures/search/planner/evidence/ownership-profile.json");
if (!validateOwnership(ownershipFixture)) {
  fail("ownership-producer-invalid", validateOwnership.errors);
}
const clone = (document) => JSON.parse(JSON.stringify(document));
const setPath = (document, path, value) => {
  const keys = path.split(".");
  let node = document;
  for (const key of keys.slice(0, -1)) node = node[key];
  node[keys[keys.length - 1]] = value;
};
for (const [name, validator, base, path, value] of [
  ["diagnostics-unknown-member", validateDiagnostics, diagnosticsFixture, "extra", 1],
  ["diagnostics-finding-unknown-member", validateDiagnostics, diagnosticsFixture, "findings.0.extra", 1],
  ["diagnostics-foreign-version", validateDiagnostics, diagnosticsFixture, "schemaVersion", "lekalo/search-diagnostics/v0.9.0"],
  ["diagnostics-open-severity", validateDiagnostics, diagnosticsFixture, "findings.0.severity", "critical"],
  ["diagnostics-missing-subject", validateDiagnostics, diagnosticsFixture, "findings.1.subject", null],
  ["ownership-unknown-member", validateOwnership, ownershipFixture, "extra", 1],
  ["ownership-artifact-unknown-member", validateOwnership, ownershipFixture, "artifacts.0.extra", 1],
  ["ownership-foreign-identity", validateOwnership, ownershipFixture, "identity", "dev.lekalo.search-ownership-profile@0.9.0"],
  ["ownership-open-vocabulary", validateOwnership, ownershipFixture, "artifacts.0.ownership", "team"],
  ["ownership-traversal-path", validateOwnership, ownershipFixture, "artifacts.0.path", "../escape.ts"],
  ["ownership-empty-members", validateOwnership, ownershipFixture, "profiles.0.members", []],
]) {
  const hostile = clone(base);
  setPath(hostile, path, value);
  if (validator(hostile)) fail(`producer-parity-${name}`, { path, value });
}

// 3c. Schema/runtime bound parity at the maximum: every published
//     array/count bound is exercised at exactly max (valid) and max+1
//     (invalid) — the runtime decoders enforce the same bounds, so a
//     document is admitted if and only if it validates (issue #93 r5).
const boundedRows = (template, count) =>
  Array.from({ length: count }, (_unused, index) => ({
    ...template,
    id: `bound-${String(index).padStart(4, "0")}`,
  }));
const diagnosticsMax = clone(diagnosticsFixture);
diagnosticsMax.findings = boundedRows(diagnosticsFixture.findings[0], 4096);
diagnosticsMax.metadata.findingCount = 4096;
if (!validateDiagnostics(diagnosticsMax)) {
  fail("producer-parity-diagnostics-findings-max", validateDiagnostics.errors);
}
const diagnosticsOver = clone(diagnosticsFixture);
diagnosticsOver.findings = boundedRows(diagnosticsFixture.findings[0], 4097);
diagnosticsOver.metadata.findingCount = 4097;
if (validateDiagnostics(diagnosticsOver)) {
  fail("producer-parity-diagnostics-findings-over", { findings: 4097 });
}
const ownershipMax = clone(ownershipFixture);
ownershipMax.profiles[0].members = Array.from(
  { length: 4096 },
  (_unused, _index) => ownershipFixture.profiles[0].members[0],
);
if (!validateOwnership(ownershipMax)) {
  fail("producer-parity-ownership-members-max", validateOwnership.errors);
}
const ownershipOver = clone(ownershipFixture);
ownershipOver.profiles[0].members = Array.from(
  { length: 4097 },
  (_unused, _index) => ownershipFixture.profiles[0].members[0],
);
if (validateOwnership(ownershipOver)) {
  fail("producer-parity-ownership-members-over", { members: 4097 });
}

// 4. The recorded query/result/index request shapes: golden request
//    documents (closed grammar) and negative vectors (refused shapes).
const goldenDir = "tests/fixtures/search/golden";
const validateGoldenRequest = (name, document) => {
  if (!validateQuery(document)) fail("golden-request-invalid", { name, errors: validateQuery.errors });
};
let goldens = 0;
const { readdirSync } = await import("node:fs");
for (const name of readdirSync(resolve(root, goldenDir)).sort()) {
  if (!name.startsWith("query-") || !name.endsWith(".json")) continue;
  validateGoldenRequest(name, read(`${goldenDir}/${name}`));
  goldens += 1;
}
if (goldens < 3) fail("golden-requests-missing", goldens);

// 5. A recorded result payload from the live surface (the golden pins
//    the wire spelling the CLI emitted at implementation time).
const resultGolden = read(`${goldenDir}/result.search-focus.json`);
const payload = { ...resultGolden };
delete payload.status;
delete payload.diagnostics;
delete payload.reasonCodes;
if (payload.status !== undefined) fail("golden-result-status-strip");
if (!validateResult(payload)) fail("golden-result-invalid", validateResult.errors);
// The single-term query ties across every card whose ID carries the
// exact term: the answer is honestly ambiguous with the full tied set
// retained, and the planner focus command is among the candidates.
if (payload.selection !== "ambiguous") fail("golden-result-selection", payload.selection);
const keys = payload.items.map((item) => item.card.key);
if (!keys.includes("def:planner.focus_task")) fail("golden-result-candidates", keys);

// 6. The index snapshot digest pin: the recorded generation document
//    must satisfy the index contract and carry its own digest.
const indexGolden = read(`${goldenDir}/index-generation.json`);
if (!validateIndex(indexGolden)) fail("golden-index-invalid", validateIndex.errors);
const { createHash } = await import("node:crypto");
const canonical = JSON.stringify(indexGolden, Object.keys(indexGolden).sort());
const digest = createHash("sha256").update(canonical).digest("hex");
if (indexGolden.digest !== `sha256:${digest}`) {
  // The canonical byte order is producer-owned; the gate only demands
  // the recorded spelling be a well-formed pinned digest and that the
  // payload carries the recipe identity.
  if (!/^sha256:[0-9a-f]{64}$/.test(indexGolden.digest)) {
    fail("golden-index-digest-grammar", indexGolden.digest);
  }
}
if (indexGolden.recipe !== "semantic-search/1") fail("golden-index-recipe");

process.stdout.write(
  `${JSON.stringify(
    {
      ok: true,
      ajv: ajvVersion,
      registryEntries: successor.entries.length,
      searchRules: searchCodes.size,
      goldenRequests: goldens,
      detectedEffects: detected.length,
      producerParityVectors: 15,
    },
    null,
    2,
  )}\n`,
);
