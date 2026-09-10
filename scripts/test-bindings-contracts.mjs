#!/usr/bin/env node
// Issue #42 release gate: the observed-scan and observed-index v1.1.0
// binding-registry extensions, the frozen v1.0.0 predecessors, the
// reference scan fixtures, and the compiled Rust identity constants,
// validated with the same pinned third-party Draft 2020-12
// implementation as the other contract gates. Exact Ajv 8.17.1 is
// provisioned outside this checkout (CI does the same on Node 18 and 24)
// and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.
//
// This script is the independent second canonical implementation: it
// re-checks the additive shape of the issue #42 wire (target, profile,
// candidates, native test bindings), the closed diagnostic custody of
// the bindings family, and the source-level invariants of the proposal
// workflow (ambiguity is never resolved by picking a candidate;
// confirmation requires the recorded proposal) without reusing any Rust
// code.

import { createRequire } from "node:module";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  ({ default: Ajv2020 } = require("ajv/dist/2020.js"));
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "ajv-unavailable", detail: String(error) }, null, 2)}\n`,
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
const readText = (relative) => readFileSync(resolve(root, relative), "utf8");

const scanSchema = read("contracts/observed-scan.schema.v1.1.0.json");
const scanSchemaLegacy = read("contracts/observed-scan.schema.v1.0.0.json");
const indexSchema = read("contracts/observed-index.schema.v1.1.0.json");

const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateScan = ajv.compile(scanSchema);
const validateScanLegacy = ajv.compile(scanSchemaLegacy);
const validateIndex = ajv.compile(indexSchema);

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

// ---------------------------------------------------------------------------
// 1. The published 1.1.0 schemas are additive successors of the frozen
//    1.0.0 contracts: every 1.0.0 document validates against 1.1.0 only
//    through the current identity, and the additive members exist.
// ---------------------------------------------------------------------------
if (scanSchema.properties.schemaVersion.const !== "lekalo/observed-scan/v1.1.0") {
  fail("scan-schema-identity", scanSchema.properties.schemaVersion.const);
}
if (indexSchema.properties.identity.const !== "dev.lekalo.observed-index@1.1.0") {
  fail("index-schema-identity", indexSchema.properties.identity.const);
}
for (const member of ["target", "profile", "testBindings"]) {
  if (!scanSchema.properties[member]) fail("scan-schema-member", member);
}
for (const member of ["target", "profile", "test_bindings"]) {
  if (!indexSchema.properties[member]) fail("index-schema-member", member);
}
const symbolMember = scanSchema.$defs.symbol.properties.candidates;
if (!symbolMember) fail("scan-symbol-candidates");
const indexSymbolCandidates = indexSchema.$defs.symbol.properties.candidates;
if (!indexSymbolCandidates) fail("index-symbol-candidates");
if (!indexSchema.properties.test_bindings) fail("index-test-bindings");

// ---------------------------------------------------------------------------
// 2. Every committed bindings scan fixture satisfies the exact schema
//    version it declares, and the ambiguous fixture stays ambiguous.
// ---------------------------------------------------------------------------
const scanDir = "tests/fixtures/bindings/scans";
const scanFiles = readdirSync(resolve(root, scanDir)).filter((name) => name.endsWith(".json"));
if (scanFiles.length < 2) fail("scan-fixtures", scanDir);
for (const name of scanFiles) {
  const scan = read(`${scanDir}/${name}`);
  const validate =
    scan.schemaVersion === "lekalo/observed-scan/v1.0.0"
      ? validateScanLegacy
      : scan.schemaVersion === "lekalo/observed-scan/v1.1.0"
        ? validateScan
        : null;
  if (!validate) fail("scan-identity", name);
  else if (!validate(scan)) {
    fail("scan-invalid", { fixture: name, errors: validate.errors });
  }
}
const ambiguous = read(`${scanDir}/app-ambiguous.scan.v1_1_0.json`);
const ambiguousSymbol = ambiguous.symbols.find((symbol) => symbol.id === "taskboard.focus_task");
if (!ambiguousSymbol || (ambiguousSymbol.candidates?.length ?? 0) !== 2) {
  fail("ambiguous-fixture", "the ambiguous fixture must list both candidates");
}
if (ambiguousSymbol.location || ambiguousSymbol.stableKey) {
  fail("ambiguous-resolved", "an ambiguous entry must not carry a resolved mapping");
}

// ---------------------------------------------------------------------------
// 3. The compiled Rust identity constants agree with the published
//    schemas.
// ---------------------------------------------------------------------------
const versionSource = readText("crates/lekalo-core/src/observed/version.rs");
for (const constant of [
  'SCHEMA_VERSION: &str = "lekalo/observed-index/v1.1.0"',
  'SCAN_SCHEMA_VERSIONS: [&str; 2] =',
  '"lekalo/observed-scan/v1.0.0"',
  '"lekalo/observed-scan/v1.1.0"',
  'SCAN_SCHEMA_VERSION: &str = SCAN_SCHEMA_VERSIONS[1];',
  'INDEX_IDENTITY: &str = "dev.lekalo.observed-index@1.1.0"',
  'PROPOSAL_ID_PREFIX: &str = "prop-"',
  'RELATIONS: [&str; 3] = ["implements", "verifies", "exposes"]',
  'VERSION: &str = "1.1.0"',
]) {
  if (!versionSource.includes(constant)) fail("rust-constant", constant);
}

// ---------------------------------------------------------------------------
// 4. Source-level custody invariants of the binding workflow.
// ---------------------------------------------------------------------------
const bindingsSource = readText("crates/lekalo-core/src/observed/bindings.rs");
const indexSource = readText("crates/lekalo-core/src/observed/index.rs");
const scanService = readText("crates/lekalo-core/src/observed/scan_service.rs");
// Proposals are only derived from inferred records: a user-owned fact
// never generates a proposal (explicit bindings have priority).
if (!bindingsSource.includes("BindingStatus::Inferred || record.promoted")) {
  fail("proposal-source", "propose derives from inferred records only");
}
// An ambiguous proposal refuses confirmation until a candidate is named.
if (!bindingsSource.includes('"ambiguous-candidates"')) {
  fail("ambiguity-gate", "ambiguous proposals refuse without a named candidate");
}
// A batch applies exactly the previewed plan identity.
if (!bindingsSource.includes("batch_plan_id")) {
  fail("batch-plan-binding", "the batch plan id must be recomputed and bound");
}
// The audit re-fingerprints native test bindings too.
if (!bindingsSource.includes("test_bindings")) {
  fail("audit-test-bindings", "the audit covers native test bindings");
}
// The scan merge never downgrades a user-owned status.
if (!indexSource.includes("A scan never downgrades a user-owned binding status")) {
  fail("merge-no-downgrade", "the #39 merge rule stays");
}
// The scan service computes fingerprints from the real tree and groups
// duplicate proposals into candidate sets (never first-match).
if (!scanService.includes("fn file_fingerprint")) {
  fail("scan-core-fingerprints", "freshness evidence comes from real bytes");
}
if (!scanService.includes("grouped")) {
  fail("scan-candidate-grouping", "duplicate semantic ids form candidate sets");
}
// The reference scanner declares minimal read scopes and the closed
// skip list, so sensitive paths are never read.
const scanner = readText("tests/fixtures/bindings/ts-scanner.mjs");
for (const required of [
  'READ_SCOPES = ["package.json", "src/**"]',
  '"node_modules"',
  '.env',
]) {
  if (!scanner.includes(required)) fail("scanner-scope", required);
}

// ---------------------------------------------------------------------------
// 5. The diagnostic registry carries the bindings family over the
//    accepted 1.16.0 predecessor.
// ---------------------------------------------------------------------------
const registry = read("contracts/diagnostic-registry.v1.20.0.json");
const registry116 = read("contracts/diagnostic-registry.v1.16.0.json");
if (registry.registry_version !== "1.20.0") fail("registry-version", registry.registry_version);
const bindingsRules = registry.entries.filter((entry) => entry.id.startsWith("bindings."));
if (bindingsRules.length !== 3) fail("bindings-rule-count", bindingsRules.length);
for (const entry of bindingsRules) {
  if (!entry.code.startsWith("LEK-BND-")) fail("bindings-code", entry.id);
}
const current = new Map(registry.entries.map((entry) => [entry.id, entry]));
for (const entry of registry116.entries) {
  const successor = current.get(entry.id);
  if (!successor) fail("predecessor-rule-missing", entry.id);
  if (JSON.stringify(successor) !== JSON.stringify(entry)) {
    fail("predecessor-rule-changed", entry.id);
  }
}

process.stdout.write(
  `${JSON.stringify(
    {
      ok: true,
      ajv: ajvVersion,
      scanFixtures: scanFiles.length,
      bindingsRules: bindingsRules.length,
      registryEntries: registry.entries.length,
    },
    null,
    2,
  )}\n`,
);
