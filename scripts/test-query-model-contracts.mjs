#!/usr/bin/env node
// Issue #64 release gate: the query-model v1.0.0 attachment contract,
// the valid and invalid fixture documents, the closed diagnostic
// custody of the query family, the pinned 1.20.0 predecessor, the
// compiled Rust identity constants, and the source-level invariants of
// the declarative query model (read-only kind enforcement, the
// deterministic identity tie-breaker, the strict tenant gate, the
// single closed pagination contract, and the bounded filter grammar)
// validated with the same pinned third-party Draft 2020-12
// implementation as the other contract gates. Exact Ajv 8.17.1 is
// provisioned outside this checkout (CI does the same on Node 18 and
// 24) and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.
//
// This script is the independent second canonical implementation: it
// re-checks the wire shape and the wire-level bounds without reusing
// any Rust code. Semantic refusals (unknown fields, unresolved
// references, tenant omissions) are Rust-owned and only their
// registered diagnostics are checked here.

import { readdirSync, readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { createRequire } from "node:module";
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

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

// ---------------------------------------------------------------------------
// 1. The published 1.0.0 contract is closed and bounded: identity consts,
//    the single pagination contract shared by every target projection, the
//    closed filter operator vocabulary, and the closed cardinality,
//    consistency, direction, and strategy vocabularies.
// ---------------------------------------------------------------------------
const schema = read("contracts/query-model.schema.v1.0.0.json");
if (schema.properties.schemaVersion.const !== "lekalo/query-model/v1.0.0") {
  fail("schema-schema-version", schema.properties.schemaVersion.const);
}
if (schema.properties.identity.const !== "dev.lekalo.query-model@1.0.0") {
  fail("schema-identity", schema.properties.identity.const);
}
if (schema.additionalProperties !== false) fail("schema-open-root", "additionalProperties");
for (const member of ["tenancy", "queries"]) {
  if (!schema.properties[member]) fail("schema-member", member);
}
const defs = schema.$defs;
if (!defs.queryDecl || !defs.filterExpr || !defs.pagination || !defs.tenancyDecl) {
  fail("schema-defs", "closed declaration surfaces");
}
// The one pagination contract: limit/offset/cursor in a single closed shape.
const paginationOps = new Set(defs.pagination.properties.strategy.enum);
if (
  !paginationOps.has("offset") ||
  !paginationOps.has("cursor") ||
  paginationOps.size !== 2
) {
  fail("pagination-strategies", [...paginationOps]);
}
if (defs.pagination.properties.limit.maximum !== 10000) {
  fail("pagination-limit-bound", defs.pagination.properties.limit.maximum);
}
// The closed filter operator vocabulary.
const ops = new Set(defs.filterLeaf.properties.op.enum);
for (const op of ["eq", "ne", "lt", "le", "gt", "ge", "in", "not-in", "is-null", "is-not-null"]) {
  if (!ops.has(op)) fail("filter-op", op);
}
if (ops.size !== 10) fail("filter-op-count", ops.size);
// The closed result-cardinality and consistency vocabularies.
const cards = new Set(defs.queryDecl.properties.cardinality.enum);
if (cards.size !== 5 || !cards.has("one") || !cards.has("optional") || !cards.has("list") || !cards.has("page") || !cards.has("stream")) {
  fail("cardinality-vocabulary", [...cards]);
}
const consistency = new Set(defs.queryDecl.properties.consistency.enum);
if (consistency.size !== 3 || !consistency.has("strong") || !consistency.has("bounded") || !consistency.has("stale-ok")) {
  fail("consistency-vocabulary", [...consistency]);
}
// The bounded logical grammar: and/or/not with fanout 16.
if (defs.filterOperands.maxItems !== 16) fail("filter-fanout", defs.filterOperands.maxItems);
const logicalKeys = 0;
if (!defs.filterExpr.oneOf || defs.filterExpr.oneOf.length !== 4) {
  fail("filter-union", "leaf + and + or + not");
}
if (logicalKeys !== 0) fail("unreachable", "logicalKeys is a compile-time guard");

// ---------------------------------------------------------------------------
// 2. Fixtures: every valid document validates against the schema; the
//    schema-detected invalid document fails it; the semantic refusals are
//    wire-legal and stay with the Rust validator by design.
// ---------------------------------------------------------------------------
const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateAttachment = ajv.compile(schema);
for (const name of readdirSync(resolve(root, "tests/fixtures/query-model/valid"))) {
  const document = read(`tests/fixtures/query-model/valid/${name}`);
  if (!validateAttachment(document)) {
    fail(`valid-fixture-${name}`, validateAttachment.errors);
  }
}
const wireInvalid = read("tests/fixtures/query-model/invalid/wrong-schema-version.json");
if (validateAttachment(wireInvalid)) {
  fail("wire-invalid-accepted", "wrong-schema-version.json must fail the schema");
}
// The semantic refusals must be schema-legal: they are caught by the
// model-bound Rust resolution, not by JSON Schema.
for (const name of [
  "tenant-filter-missing.json",
  "write-source.json",
  "write-query-symbol.json",
  "sort-nondeterministic.json",
  "filter-unknown-member.json",
  "filter-type-mismatch.json",
  "selection-visibility.json",
  "source-not-read.json",
  "include-segment-type.json",
]) {
  const document = read(`tests/fixtures/query-model/invalid/${name}`);
  if (!validateAttachment(document)) {
    fail(`semantic-fixture-not-wire-legal-${name}`, validateAttachment.errors);
  }
}

// ---------------------------------------------------------------------------
// 3. Diagnostic registry custody: the query family is exactly the ten
//    LEK-QRY rules, 1.21.0 is additive over the pinned frozen 1.20.0, and
//    the pinned predecessor bytes are unchanged.
// ---------------------------------------------------------------------------
const registry = read("contracts/diagnostic-registry.v1.21.0.json");
const predecessorText = readFileSync(
  resolve(root, "contracts/diagnostic-registry.v1.20.0.json"),
  "utf8",
).replace(/\r\n/g, "\n");
if (
  createHash("sha256").update(predecessorText).digest("hex") !==
  "3b1edf6f953ea3126d2c6d93a720fa0b2a3c3dbefac98eb5be52a857b86fa429"
) {
  fail("predecessor-custody", "1.20.0 bytes changed");
}
const predecessor = JSON.parse(predecessorText);
if (registry.registry_version !== "1.21.0") fail("registry-version", registry.registry_version);
if (registry.identity !== "dev.lekalo.diagnostic-registry@1.21.0") {
  fail("registry-identity", registry.identity);
}
const current = new Map(registry.entries.map((entry) => [entry.id, entry]));
const { isDeepStrictEqual } = await import("node:util");
for (const entry of predecessor.entries) {
  const successor = current.get(entry.id);
  if (!successor) fail("predecessor-rule-missing", entry.id);
  if (!isDeepStrictEqual(successor, entry)) fail("predecessor-rule-changed", entry.id);
}
const additions = registry.entries.filter(
  (entry) => !predecessor.entries.some((old) => old.id === entry.id),
);
const expectedQueryRules = [
  "query.input-invalid",
  "query.contract-invalid",
  "query.source-invalid",
  "query.filter-invalid",
  "query.sort-invalid",
  "query.pagination-invalid",
  "query.tenant-filter-missing",
  "query.visibility-boundary",
  "query.reference-invalid",
  "query.export-limit",
];
if (additions.length !== expectedQueryRules.length) {
  fail("query-additions-count", additions.map((entry) => entry.id));
}
for (const id of expectedQueryRules) {
  const entry = current.get(id);
  if (!entry) fail("query-rule-missing", id);
  if (!entry.code.startsWith("LEK-QRY-")) fail("query-code-family", id);
  if (entry.category !== "semantic") fail("query-category", id);
  if (!entry.allowed_statuses.includes("invalid")) fail("query-status", id);
}
if (registry.entries.length !== predecessor.entries.length + expectedQueryRules.length) {
  fail("registry-entry-count", registry.entries.length);
}

// ---------------------------------------------------------------------------
// 4. The compiled Rust identity constants and hard bounds.
// ---------------------------------------------------------------------------
const versionSource = readText("crates/lekalo-core/src/query_model/version.rs");
for (const constant of [
  'FAMILY: &str = "dev.lekalo.query-model"',
  'VERSION: &str = "1.0.0"',
  'IDENTITY: &str = "dev.lekalo.query-model@1.0.0"',
  'SCHEMA_VERSION: &str = "lekalo/query-model/v1.0.0"',
  'IR_IDENTITY: &str = "dev.lekalo.ir@0.1.0"',
  'MAX_TENANCY: usize = 256',
  'MAX_QUERIES: usize = 10_000',
  'MAX_PARAMETERS: usize = 64',
  'MAX_FILTER_DEPTH: usize = 8',
  'MAX_FILTER_OPERANDS: usize = 16',
  'MAX_FILTER_LEAVES: usize = 64',
  'MAX_SET_ITEMS: usize = 64',
  'MAX_SORT_KEYS: usize = 16',
  'MAX_SELECTION: usize = 256',
  'MAX_INCLUDES: usize = 16',
  'MAX_INCLUDE_PATH: usize = 4',
  'MAX_POLICY_REFS: usize = 32',
  'MAX_SCENARIO_REFS: usize = 32',
  'MAX_LIMIT: i64 = 10_000',
  'MAX_OFFSET: i64 = 1_000_000',
  'MAX_COST_ROWS: i64 = 1_000_000',
  'MAX_STALENESS_SECONDS: i64 = 2_592_000',
  'MAX_REASON_BYTES: usize = 256',
]) {
  if (!versionSource.includes(constant)) fail("rust-constant", constant);
}
// The wire bounds must agree with the published schema bounds.
if (defs.queryDecl.maxItems !== undefined) fail("query-decl-max", "queries carry the bound");
if (schema.properties.queries.maxItems !== 10000) fail("schema-queries-bound", schema.properties.queries.maxItems);
if (schema.properties.tenancy.maxItems !== 256) fail("schema-tenancy-bound", schema.properties.tenancy.maxItems);
if (defs.queryDecl.properties.parameters.maxItems !== 64) fail("schema-parameters-bound", defs.queryDecl.properties.parameters.maxItems);
if (defs.queryDecl.properties.sort.maxItems !== 16) fail("schema-sort-bound", defs.queryDecl.properties.sort.maxItems);
if (defs.queryDecl.properties.selection.maxItems !== 256) fail("schema-selection-bound", defs.queryDecl.properties.selection.maxItems);
if (defs.queryDecl.properties.includes.maxItems !== 16) fail("schema-includes-bound", defs.queryDecl.properties.includes.maxItems);
if (defs.queryDecl.properties.policies.maxItems !== 32) fail("schema-policies-bound", defs.queryDecl.properties.policies.maxItems);
if (defs.queryDecl.properties.scenarios.maxItems !== 32) fail("schema-scenarios-bound", defs.queryDecl.properties.scenarios.maxItems);
if (defs.include.properties.path.maxItems !== 4) fail("schema-include-path-bound", defs.include.properties.path.maxItems);
if (defs.costHint.properties.maxRows.maximum !== 1000000) fail("schema-cost-bound", defs.costHint.properties.maxRows.maximum);
if (defs.queryDecl.properties.maxStaleness.maximum !== 2592000) fail("schema-staleness-bound", defs.queryDecl.properties.maxStaleness.maximum);
if (defs.literalNode.oneOf.length !== 10) fail("literal-union", defs.literalNode.oneOf.length);
const setNode = defs.literalNode.oneOf.find(
  (variant) => variant.properties?.kind?.const === "set",
);
if (setNode.properties.value.maxItems !== 64) fail("schema-set-bound", setNode.properties.value.maxItems);

// ---------------------------------------------------------------------------
// 5. Source-level invariants of the declarative query model.
// ---------------------------------------------------------------------------
const resolveSource = readText("crates/lekalo-core/src/query_model/resolve.rs");
const validateSource = readText("crates/lekalo-core/src/query_model/validate.rs");
const wireSource = readText("crates/lekalo-core/src/query_model/wire.rs");
const planSource = readText("crates/lekalo-core/src/query_model/plan.rs");
const filterSource = readText("crates/lekalo-core/src/query_model/filter.rs");
// Read-only enforcement: write surfaces are refused by kind in the query
// and source slots.
for (const token of ['"write-surface"', '"write-source"', '"query-symbol-kind"']) {
  if (!resolveSource.includes(token)) fail("read-only-gate", token);
}
// Deterministic order: the identity tie-breaker and the page/stream order
// requirement.
if (!resolveSource.includes('"identity-tiebreaker-missing"')) {
  fail("identity-tiebreaker", "sort must end in the entity identity");
}
if (!validateSource.includes('"total-order-required"')) {
  fail("total-order-required", "page/stream must declare a total order");
}
if (!validateSource.includes('"page-requires-pagination"')) {
  fail("page-pagination", "page cardinality requires the pagination block");
}
// The strict tenant gate: source and include terminals are covered.
if (!resolveSource.includes('"source-tenant-filter-missing"')) {
  fail("tenant-source-gate", "strict profile requires the tenant filter");
}
if (!resolveSource.includes('"include-tenant-unconstrained"')) {
  fail("tenant-include-gate", "strict profile guards tenant-scoped includes");
}
// Custody: project id, model version, and the exact model digest.
for (const token of ['"project-id"', '"model-version"', '"model-digest"']) {
  if (!resolveSource.includes(token)) fail("custody-gate", token);
}
// The filter bomb defense: depth is enforced while decoding.
if (!wireSource.includes("MAX_FILTER_DEPTH")) fail("filter-depth-gate", "incremental depth bound");
if (!wireSource.includes("unknown-field")) fail("unknown-field-gate", "closed wire surface");
// The plan is the single neutral mapping contract; foreign queries
// produce no managed steps.
if (!planSource.includes('foreign: true')) fail("foreign-plan", "foreign queries stay unmanaged");
if (!filterSource.includes("within_bounds")) fail("filter-bounds", "typed-level bound check");

process.stdout.write(
  `${JSON.stringify(
    {
      ok: true,
      ajv: ajvVersion,
      schema: "lekalo/query-model/v1.0.0",
      registryEntries: registry.entries.length,
      queryRules: expectedQueryRules.length,
      validFixtures: readdirSync(resolve(root, "tests/fixtures/query-model/valid")).length,
    },
    null,
    2,
  )}\n`,
);
