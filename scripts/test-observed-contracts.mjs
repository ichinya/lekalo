#!/usr/bin/env node
// Issue #39 release gate: the observed-scan and observed-index wire
// schemas, the pinned canonical golden index, and the compiled Rust
// identity constants, validated with the same pinned third-party Draft
// 2020-12 implementation as the other contract gates. Exact Ajv 8.17.1
// is provisioned outside this checkout (CI does the same on Node 18 and
// 24) and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.
//
// This script is the independent second canonical implementation: it
// re-checks the closed vocabularies, the canonical byte shape of the
// golden index (compact, fixed key order, sorted records), and the
// source-level custody invariants (the index home, the clean managed
// root, and the promotion document roots) without reusing any Rust code.

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

const scanSchema = read("contracts/observed-scan.schema.v1.0.0.json");
const indexSchema = read("contracts/observed-index.schema.v1.0.0.json");
const goldenFile = "tests/fixtures/observed/golden/task-domain.index.json";

const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateScan = ajv.compile(scanSchema);
const validateIndex = ajv.compile(indexSchema);

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

// ---------------------------------------------------------------------------
// 1. Every adapter scan fixture satisfies the scan schema.
// ---------------------------------------------------------------------------
const scanDir = "tests/fixtures/observed/task-domain/scans";
const scanFiles = readdirSync(resolve(root, scanDir)).filter((name) => name.endsWith(".json"));
if (scanFiles.length < 2) fail("scan-fixtures", "the fixture pair is missing");
for (const name of scanFiles) {
  const scan = read(`${scanDir}/${name}`);
  if (!validateScan(scan)) {
    fail("scan-invalid", { fixture: name, errors: validateScan.errors });
  }
  if (scan.schemaVersion !== "lekalo/observed-scan/v1.0.0") {
    fail("scan-identity", name);
  }
}
// 2. The golden index satisfies the index schema and the canonical byte
//    shape: compact JSON, fixed key order, and sorted record ids.
// ---------------------------------------------------------------------------
const goldenText = readText(goldenFile);
const golden = JSON.parse(goldenText);
if (JSON.stringify(golden) !== goldenText.trim()) {
  fail("golden-not-compact", "canonical bytes must equal their compact re-rendering");
}
if (!validateIndex(golden)) {
  fail("golden-index-invalid", validateIndex.errors);
}
const GOLDEN_KEYS = [
  "schema_version",
  "identity",
  "project",
  "mode",
  "adapter",
  "revision",
  "symbols",
  "endpoints",
  "schemas",
];
if (JSON.stringify(Object.keys(golden)) !== JSON.stringify(GOLDEN_KEYS)) {
  fail("golden-key-order", Object.keys(golden).join(","));
}
if (golden.schema_version !== "lekalo/observed-index/v1.0.0" || golden.identity !== "dev.lekalo.observed-index@1.0.0" || golden.mode !== "observed") {
  fail("golden-identity", golden.identity);
}
const symbolIds = golden.symbols.map((symbol) => symbol.id);
if (JSON.stringify(symbolIds) !== JSON.stringify([...symbolIds].sort())) {
  fail("golden-symbols-unsorted", symbolIds.join(","));
}
const endpointIds = golden.endpoints.map((endpoint) => endpoint.id);
if (JSON.stringify(endpointIds) !== JSON.stringify([...endpointIds].sort())) {
  fail("golden-endpoints-unsorted", endpointIds.join(","));
}
// Inferred and confirmed facts are distinguishable in the wire, and a
// promoted symbol carries its adoption receipt.
const statuses = new Set(golden.symbols.map((symbol) => symbol.status));
for (const required of ["inferred", "confirmed", "explicit"]) {
  if (!statuses.has(required)) fail("golden-status-missing", required);
}
const promoted = golden.symbols.filter((symbol) => symbol.promoted);
if (promoted.length === 0 || promoted.some((symbol) => !symbol.promotion)) {
  fail("golden-promotion-receipt", promoted.length);
}
for (const symbol of golden.symbols) {
  if (symbol.status === "inferred" && symbol.promoted) {
    fail("golden-inferred-promoted", symbol.id);
  }
  if (symbol.stable_key === null) fail("golden-stable-key", symbol.id);
  if (symbol.fingerprint === null) fail("golden-fingerprint", symbol.id);
}

// ---------------------------------------------------------------------------
// 3. The compiled Rust identity constants agree with the published
//    schemas.
// ---------------------------------------------------------------------------
const versionSource = readText("crates/lekalo-core/src/observed/version.rs");
for (const constant of [
  'SCHEMA_VERSION: &str = "lekalo/observed-index/v1.0.0"',
  'SCAN_SCHEMA_VERSION: &str = "lekalo/observed-scan/v1.0.0"',
  'INDEX_IDENTITY: &str = "dev.lekalo.observed-index@1.0.0"',
  'MODE: &str = "observed"',
  'VERSION: &str = "1.0.0"',
]) {
  if (!versionSource.includes(constant)) fail("rust-constant", constant);
}
const schemaIds = [scanSchema.$id, indexSchema.$id];
if (schemaIds.some((id) => !id.startsWith("https://dev.lekalo/"))) fail("schema-id", schemaIds.join(","));

// ---------------------------------------------------------------------------
// 4. Source-level custody invariants.
// ---------------------------------------------------------------------------
const storeSource = readText("crates/lekalo-core/src/observed/store.rs");
const indexService = readText("crates/lekalo-core/src/observed/index.rs");
if (!indexService.includes('".lekalo/import/observed"') || !indexService.includes('"index.json"')) {
  fail("store-home", "the index persists under .lekalo/import/observed/index.json");
}
if (!storeSource.includes("rename") || !storeSource.includes(".tmp")) {
  fail("store-atomic", "the index write must be an in-place atomic rename");
}
if (!storeSource.includes("REPARSE_POINT") && !storeSource.includes("is_symlink")) {
  fail("store-link-rejection", "the store must reject links and reparse points");
}
const cleanSource = readText("crates/lekalo-core/src/artifacts/clean.rs");
if (!cleanSource.includes(".lekalo/generated")) {
  fail("clean-managed-root", "clean must scan only the generated managed root");
}
const promoteSource = readText("crates/lekalo-core/src/observed/promote.rs");
if (!promoteSource.includes("lekalo/modules/")) {
  fail("promotion-document-root", "promotion writes only canonical model documents");
}
if (!promoteSource.includes("PromotionSelection::Module")) {
  fail("promotion-module-scope", "module-scoped promotion must exist");
}
// The generated-artifact manifest cannot claim observed files: its path
// grammar is rooted at the managed root.
const artifactTypes = readText("crates/lekalo-core/src/artifacts/types.rs");
if (!artifactTypes.includes('strip_prefix(".lekalo/generated/")')) {
  fail("manifest-path-confinement", "manifest entries are rooted at .lekalo/generated/");
}

// ---------------------------------------------------------------------------
// 5. The diagnostic registry carries the observed family.
// ---------------------------------------------------------------------------
const registry = read("contracts/diagnostic-registry.v1.10.0.json");
const observedRules = registry.entries.filter((entry) => entry.id.startsWith("observed."));
if (registry.registry_version !== "1.10.0") fail("registry-version", registry.registry_version);
if (observedRules.length !== 11) fail("observed-rule-count", observedRules.length);
for (const entry of observedRules) {
  if (!entry.code.startsWith("LEK-OBS-")) fail("observed-code", entry.id);
}

process.stdout.write(
  `${JSON.stringify(
    {
      ok: true,
      ajv: ajvVersion,
      scanFixtures: scanFiles.length,
      goldenSymbols: golden.symbols.length,
      observedRules: observedRules.length,
    },
    null,
    2,
  )}\n`,
);
