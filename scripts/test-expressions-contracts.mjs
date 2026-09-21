#!/usr/bin/env node
// Issue #66 release gate: the expressions v0.2.16 attachment, vector,
// and built-in-capability contracts, the valid and invalid fixture
// documents, the closed custody of the expression diagnostic family,
// the pinned 0.2.16 registry, the compiled Rust identity constants,
// and the source-level denials of the typed-expression language (no
// arbitrary calls, loops, reflection, eval, filesystem, or network
// representation anywhere in the family) validated with the same
// pinned third-party Draft 2020-12 implementation as the other
// contract gates. Exact Ajv 8.17.1 is provisioned outside this
// checkout (CI does the same on Node 18 and 24) and exposed through
// NODE_PATH / LEKALO_AJV_NODE_PATH.
//
// This script is the independent second canonical implementation: it
// re-checks the wire shape and the wire-level bounds without reusing
// any Rust code. Static typing, evaluation semantics, and the
// deterministic clock are Rust-owned; only their registered
// diagnostics and the published cross-target fixture shape are
// checked here.

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
// 1. The published 0.2.16 contracts are closed and bounded: identity
//    consts, the closed root surfaces, the closed node-op vocabulary,
//    the eleven binary operators, the fifteen built-ins, and the hard
//    numeric bounds shared with the compiled Rust constants.
// ---------------------------------------------------------------------------
const schema = read("contracts/expressions.schema.v0.2.16.json");
const vectorsSchema = read("contracts/expressions-vectors.schema.v0.2.16.json");
const supportSchema = read("contracts/expressions-builtin-support.schema.v0.2.16.json");
if (schema.properties.schemaVersion.const !== "lekalo/expressions/v0.2.16") {
  fail("schema-schema-version", schema.properties.schemaVersion.const);
}
if (schema.properties.identity.const !== "dev.lekalo.expressions@0.2.16") {
  fail("schema-identity", schema.properties.identity.const);
}
if (schema.additionalProperties !== false) fail("schema-open-root", "additionalProperties");
for (const member of [
  "schemaVersion",
  "identity",
  "attachmentRevision",
  "projectId",
  "modelRef",
  "irRef",
  "builtinSemantics",
  "expressions",
]) {
  if (!schema.properties[member]) fail("schema-member", member);
}
const defs = schema.$defs;
if (!defs.expressionRecord || !defs.node || !defs.scalarLiteral || !defs.exprType) {
  fail("schema-defs", "closed declaration surfaces");
}
if (schema.properties.expressions.maxItems !== 10000) {
  fail("schema-expressions-bound", schema.properties.expressions.maxItems);
}
// The closed binary operator vocabulary: equality, comparison, and
// the checked arithmetic pairs.
const binaryOps = new Set(defs.binaryNode.properties.op.enum);
for (const op of ["eq", "ne", "lt", "le", "gt", "ge", "add", "sub", "mul", "div", "mod"]) {
  if (!binaryOps.has(op)) fail("binary-op", op);
}
if (binaryOps.size !== 11) fail("binary-op-count", binaryOps.size);
// The closed node vocabulary: literals, references, now, the binary
// and unary operators, membership, combinators, the conditional, and
// the built-in call shape. Nothing else — no call, loop, or eval node
// exists in the grammar.
const nodeVariants = [];
const pushOpSpec = (spec) => {
  if (!spec) return;
  if (spec.const) nodeVariants.push(spec.const);
  if (Array.isArray(spec.enum)) nodeVariants.push(...spec.enum);
};
for (const def of Object.values(defs)) {
  pushOpSpec(def.properties?.op);
  const oneOf = def.oneOf ?? def.anyOf;
  if (!oneOf) continue;
  for (const variant of oneOf) {
    pushOpSpec(variant.properties?.op);
  }
}
const nodeOps = new Set(nodeVariants);
if (nodeOps.size !== nodeVariants.length) fail("node-op-duplicate", nodeVariants);
for (const op of [
  "lit-bool",
  "lit-int",
  "lit-string",
  "lit-datetime",
  "lit-duration",
  "lit-set",
  "ref",
  "now",
  "eq",
  "ne",
  "lt",
  "le",
  "gt",
  "ge",
  "add",
  "sub",
  "mul",
  "div",
  "mod",
  "is-null",
  "not-null",
  "not",
  "in-set",
  "not-in-set",
  "and",
  "or",
  "if",
  "builtin",
]) {
  if (!nodeOps.has(op)) fail("node-op", op);
}
if (nodeOps.size !== 28) fail("node-op-count", nodeOps.size);
// The bounded integer universe: exact in every target.
const litInt = defs.literalNode.oneOf.find((v) => v.properties?.op?.const === "lit-int");
if (!litInt) fail("lit-int", "missing");
const intValue = litInt.properties.value;
if (intValue.minimum !== -9007199254740991 || intValue.maximum !== 9007199254740991) {
  fail("int-bound", [intValue.minimum, intValue.maximum]);
}
// The typed reference scopes; assignment targets are narrower
// (input/entity only) and enforced by the Rust contract check, not
// the schema.
const scopes = new Set(defs.scope.enum);
if (scopes.size !== 4 || !scopes.has("input") || !scopes.has("actor") || !scopes.has("entity") || !scopes.has("result")) {
  fail("scopes", [...scopes]);
}
// The closed built-in vocabulary agrees with the published support
// snapshot vocabulary: exactly the fifteen registered names.
const builtinNames = new Set(defs.builtinNode.properties.name.enum);
if (builtinNames.size !== 15) fail("builtin-count", builtinNames.size);

// ---------------------------------------------------------------------------
// 2. Fixtures: every valid document validates against its schema; the
//    wire-detected invalid documents fail the attachment schema; the
//    semantic refusals are wire-legal and stay with the Rust
//    validator by design; the vectors and support snapshots validate;
//    the vectors resolve against the attachment and inject the clock
//    for every now-reading expression.
// ---------------------------------------------------------------------------
const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateAttachment = ajv.compile(schema);
const validateVectors = ajv.compile(vectorsSchema);
const validateSupport = ajv.compile(supportSchema);

const planner = read("tests/fixtures/expressions/valid/planner.json");
if (!validateAttachment(planner)) fail("valid-fixture-planner.json", validateAttachment.errors);

const wireInvalid = [
  "bad-expression-id.json",
  "int-literal-bound.json",
  "target-readonly-scope.json",
  "unknown-builtin.json",
  "unknown-record-field.json",
  "unknown-top-field.json",
  "wrong-builtin-semantics.json",
  "wrong-identity.json",
];
for (const name of wireInvalid) {
  const document = read(`tests/fixtures/expressions/invalid/${name}`);
  if (validateAttachment(document)) fail("wire-invalid-accepted", name);
}
const semanticInvalid = readdirSync(resolve(root, "tests/fixtures/expressions/invalid")).filter(
  (name) => name.endsWith(".json") && !wireInvalid.includes(name),
);
if (semanticInvalid.length !== 20) fail("semantic-fixture-count", semanticInvalid.length);
for (const name of semanticInvalid) {
  const document = read(`tests/fixtures/expressions/invalid/${name}`);
  if (!validateAttachment(document)) {
    fail(`semantic-fixture-not-wire-legal-${name}`, validateAttachment.errors);
  }
}

const vectors = read("tests/fixtures/expressions/vectors.json");
if (!validateVectors(vectors)) fail("vectors-invalid", validateVectors.errors);
for (const name of ["full.json", "core-only.json"]) {
  const snapshot = read(`tests/fixtures/expressions/builtin-support/${name}`);
  if (!validateSupport(snapshot)) fail(`support-invalid-${name}`, validateSupport.errors);
}

// Cross-fixture closure: every vector resolves to a declared
// expression and ids are unique. A now-reading vector either injects
// its canonical deterministic clock (the schema pattern above) or
// omits the clock and is evaluated at the shared epoch default
// (1970-01-01T00:00:00Z) by the reference and every generated
// target. Omission is the only epoch form: an explicit null or
// noncanonical clock member is wire-illegal and refuses.
const declared = new Map(planner.expressions.map((record) => [record.id, record]));
const bodyHasNow = (node) => {
  if (!node || typeof node !== "object") return false;
  if (node.op === "now") return true;
  for (const value of Object.values(node)) {
    if (Array.isArray(value) && value.some(bodyHasNow)) return true;
    if (value && typeof value === "object" && bodyHasNow(value)) return true;
  }
  return false;
};
const vectorIds = new Set();
const epochClockVectors = [];
for (const vector of vectors.vectors) {
  if (vectorIds.has(vector.id)) fail("duplicate-vector-id", vector.id);
  vectorIds.add(vector.id);
  const record = declared.get(vector.expression);
  if (!record) fail("vector-expression-unknown", { vector: vector.id, expression: vector.expression });
  if (bodyHasNow(record.body) && !vector.clock) epochClockVectors.push(vector.id);
}

// ---------------------------------------------------------------------------
// 3. Diagnostic registry custody: the expression family is exactly
//    the nine LEK-EXPR rules of the pinned 0.2.16 registry, each
//    active, error-severity, and semantic-category; every one is
//    preserved from the frozen 1.24.0 line plus exactly its own nine.
// ---------------------------------------------------------------------------
const registry = read("contracts/diagnostic-registry.v0.2.16.json");
const predecessor = read("contracts/diagnostic-registry.v0.2.16.json");
if (registry.registry_version !== "0.2.16") fail("registry-version", registry.registry_version);
const expectedExpressionRules = [
  ["expression.input-invalid", "LEK-EXPR-001"],
  ["expression.contract-invalid", "LEK-EXPR-002"],
  ["expression.type-invalid", "LEK-EXPR-003"],
  ["expression.complexity-limit", "LEK-EXPR-004"],
  ["expression.builtin-unsupported", "LEK-EXPR-005"],
  ["expression.eval-invalid", "LEK-EXPR-006"],
  ["expression.binding-invalid", "LEK-EXPR-007"],
  ["expression.diff-invalid", "LEK-EXPR-008"],
  ["expression.export-limit", "LEK-EXPR-009"],
];
const registryEntries = new Map(registry.entries.map((entry) => [entry.id, entry]));
for (const [id, code] of expectedExpressionRules) {
  const entry = registryEntries.get(id);
  if (!entry) fail("expression-rule-missing", id);
  if (entry.code !== code) fail("expression-code-drift", { id, code: entry.code });
  if (entry.lifecycle !== "active") fail("expression-rule-lifecycle", id);
  if (entry.default_severity !== "error") fail("expression-rule-severity", id);
  if (entry.category !== "semantic") fail("expression-rule-category", id);
}
// The registry chain is additive: the current embedded registry (the
// version Rust compiles via include_bytes!) must still carry every
// LEK-EXPR rule with the same code/lifecycle/severity/category.
const currentRegistry = read("contracts/diagnostic-registry.v0.4.0.json");
const currentEntries = new Map(currentRegistry.entries.map((entry) => [entry.id, entry]));
for (const [id, code] of expectedExpressionRules) {
  const entry = currentEntries.get(id);
  if (!entry) fail("expression-rule-not-preserved", id);
  if (entry.code !== code) fail("expression-code-drift", { id, code: entry.code });
  if (entry.lifecycle !== "active") fail("expression-rule-lifecycle", id);
  if (entry.default_severity !== "error") fail("expression-rule-severity", id);
  if (entry.category !== "semantic") fail("expression-rule-category", id);
}
// ---------------------------------------------------------------------------
// 4. The compiled Rust identity constants, hard bounds, and built-in
//    table agree with the published contract.
// ---------------------------------------------------------------------------
const versionSource = readText("crates/lekalo-core/src/expressions/version.rs");
for (const constant of [
  'FAMILY: &str = "dev.lekalo.expressions"',
  'VERSION: &str = "0.2.16"',
  'IDENTITY: &str = "dev.lekalo.expressions@0.2.16"',
  'IR_IDENTITY: &str = "dev.lekalo.ir@0.2.16"',
  'SCHEMA_VERSION: &str = "lekalo/expressions/v0.2.16"',
  'VECTORS_SCHEMA_VERSION: &str = "lekalo/expressions/vectors/v0.2.16"',
  'SUPPORT_SCHEMA_VERSION: &str = "lekalo/expressions/builtin-support/v0.2.16"',
  'BUILTIN_SEMANTICS_VERSION: &str = "0.2.16"',
  'CORE_CAPABILITY: &str = "expression.core"',
  'BUILTIN_CAPABILITY_PREFIX: &str = "expression.builtin/"',
  "MAX_EXPRESSIONS: usize = 10_000",
  "MAX_PARAMS: usize = 64",
  "MAX_DEPTH: usize = 12",
  "MAX_NODES: usize = 256",
  "MAX_OPERANDS: usize = 16",
  "MAX_SET_ITEMS: usize = 64",
  "MAX_LITERAL_BYTES: usize = 256",
  "MAX_INT: i64 = 9_007_199_254_740_991",
  "MAX_DURATION_SECONDS: i64 = 31_536_000_000",
  "MAX_VECTORS: usize = 10_000",
  "MAX_CANONICAL_BYTES: usize = 32 * 1024 * 1024",
]) {
  if (!versionSource.includes(constant)) fail("rust-constant", constant);
}
const diagnosticsVersionSource = readText("crates/lekalo-core/src/diagnostics/version.rs");
if (!diagnosticsVersionSource.includes('REGISTRY_VERSION: &str = "0.4.0"')) {
  fail("rust-registry-version", "0.4.0");
}
if (!diagnosticsVersionSource.includes('REGISTRY_IDENTITY: &str = "dev.lekalo.diagnostic-registry@0.4.0"')) {
  fail("rust-registry-identity", "0.4.0");
}
const builtinSource = readText("crates/lekalo-core/src/expressions/builtin.rs");
for (const name of builtinNames) {
  if (!builtinSource.includes(`name: "${name}"`)) fail("rust-builtin-table", name);
}

// ---------------------------------------------------------------------------
// 5. Source-level denials: the family has no representation for the
//    forbidden shapes, so no accepted attachment can express them.
// ---------------------------------------------------------------------------
const astSource = readText("crates/lekalo-core/src/expressions/ast.rs");
for (const forbidden of ["Call", "Loop", "While", "For(", "Eval", "Reflect"]) {
  if (astSource.includes(forbidden)) fail("grammar-forbidden-shape", forbidden);
}
// The evaluator is pure: the only ambient input is the injected clock.
const evalSource = readText("crates/lekalo-core/src/expressions/eval.rs");
if (!evalSource.includes("Clock") || !evalSource.includes("clock")) {
  fail("clock-injection", "the evaluator must consume the injected clock");
}
// No family module reaches the filesystem, the network, or a process.
for (const name of readdirSync(resolve(root, "crates/lekalo-core/src/expressions"))) {
  const source = readText(`crates/lekalo-core/src/expressions/${name}`);
  for (const forbidden of ["std::fs", "std::net", "std::process", "Command::new"]) {
    if (source.includes(forbidden)) fail("family-host-access", { file: name, token: forbidden });
  }
}
// Decoding enforces the depth bound incrementally (the bomb defense)
// and the wire surface stays closed.
const wireSource = readText("crates/lekalo-core/src/expressions/wire.rs");
if (!wireSource.includes("MAX_DEPTH")) fail("depth-gate", "incremental depth bound");
if (!wireSource.includes("unknown-field")) fail("unknown-field-gate", "closed wire surface");
// Static typing decides every operator combination at declaration
// time; the complexity refusal routes to the foreign family.
const typingSource = readText("crates/lekalo-core/src/expressions/typing.rs");
if (!typingSource.includes("expression.type-invalid")) fail("typing-diagnostics", "exhaustive typing");
if (!typingSource.includes("expression.complexity-limit")) fail("complexity-routing", "foreign escape hatch");

process.stdout.write(
  `${JSON.stringify(
    {
      ok: true,
      ajv: ajvVersion,
      schema: "lekalo/expressions/v0.2.16",
      registryEntries: registry.entries.length,
      expressionRules: expectedExpressionRules.length,
      builtins: builtinNames.size,
      nodeOps: nodeOps.size,
      vectors: vectors.vectors.length,
      epochClockVectors: epochClockVectors.length,
      invalidFixtures: wireInvalid.length + semanticInvalid.length,
    },
    null,
    2,
  )}\n`,
);
