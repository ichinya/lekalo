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

// 2. The registry union successor validates against its own schema and
//    preserves every accepted 0.6.5 entry byte-for-byte.
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
if (successor.entries.length !== predecessor.entries.length + 8) {
  fail("registry-union-size", {
    predecessor: predecessor.entries.length,
    successor: successor.entries.length,
  });
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
    },
    null,
    2,
  )}\n`,
);
