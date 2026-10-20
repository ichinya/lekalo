#!/usr/bin/env node
// Issue #73 change-family contract gate: the closed change schemas, the
// embedded catalog, the registry 0.8.0 union successor, the identity and
// discriminator pins, and the committed golden documents. Exact Ajv 8.17.1
// is provisioned outside this checkout like every other contract gate.
//
// This gate is schema- and data-only; the live CLI flow runs in
// scripts/test-change-cli.mjs (CI builds the binary first).

import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { registryUnion } from "./lib/registry-union.mjs";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  Ajv2020 = require("ajv/dist/2020").default;
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "ajv-unavailable",
    detail: error?.code ?? error?.message,
  }, null, 2)}\n`);
  process.exit(1);
}
if (ajvVersion !== "8.17.1") {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "ajv-version",
    detail: `expected 8.17.1, found ${ajvVersion}`,
  }, null, 2)}\n`);
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));
const readText = (relative) => readFileSync(resolve(root, relative), "utf8");

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

const ajv = new Ajv2020({ strict: true, allErrors: true });
const compile = (schemaPath) => {
  const validate = ajv.compile(read(schemaPath));
  if (!validate) fail("schema-compile", schemaPath);
  return validate;
};
const expectValid = (validate, document, where) => {
  if (!validate(document)) {
    fail("schema-invalid", { where, errors: validate.errors });
  }
};

const IDENTITY = "0.8.0";

// 1. The closed schemas compile under strict Draft 2020-12.
const validateRequest = compile("contracts/change-request.schema.v0.8.0.json");
const validateCatalog = compile("contracts/change-catalog.schema.v0.8.0.json");
const validatePolicy = compile("contracts/change-policy.schema.v0.8.0.json");
const validatePlan = compile("contracts/change-plan.schema.v0.8.0.json");
const validateReceipt = compile("contracts/change-receipt.schema.v0.8.0.json");
const validateHistory = compile("contracts/change-history.schema.v0.8.0.json");

// 2. Identity and discriminator pins across the family.
const pins = {
  "contracts/change-request.schema.v0.8.0.json": [
    "lekalo/change-request/v0.8.0",
    "https://dev.lekalo/change-request.schema.v0.8.0.json",
  ],
  "contracts/change-catalog.schema.v0.8.0.json": [
    "lekalo/change-catalog/v0.8.0",
    "https://dev.lekalo/change-catalog.schema.v0.8.0.json",
  ],
  "contracts/change-policy.schema.v0.8.0.json": [
    "lekalo/change-policy/v0.8.0",
    "https://dev.lekalo/change-policy.schema.v0.8.0.json",
  ],
  "contracts/change-plan.schema.v0.8.0.json": [
    "lekalo/change-plan/v0.8.0",
    "https://dev.lekalo/change-plan.schema.v0.8.0.json",
  ],
  "contracts/change-receipt.schema.v0.8.0.json": [
    "lekalo/change-receipt/v0.8.0",
    "https://dev.lekalo/change-receipt.schema.v0.8.0.json",
  ],
  "contracts/change-history.schema.v0.8.0.json": [
    "lekalo/change-history/v0.8.0",
    "https://dev.lekalo/change-history.schema.v0.8.0.json",
  ],
};
for (const [path, [discriminator, id]] of Object.entries(pins)) {
  const schema = read(path);
  const discriminatorProperty = schema.properties.schemaVersion;
  const identityProperty = schema.properties.identity;
  if (discriminatorProperty?.const !== discriminator) {
    fail("discriminator-pin", { path, expected: discriminator });
  }
  if (schema.$id !== id) fail("schema-id-pin", { path, expected: id });
  if (
    identityProperty &&
    identityProperty.const !== `dev.lekalo.${path.match(/(change-[a-z-]+)\.schema/)[1]}@${IDENTITY}`
  ) {
    fail("identity-pin", { path });
  }
}

// 3. The Rust identity constants mirror the contract pins.
const versionRs = readText("crates/lekalo-core/src/change/version.rs");
for (const constant of [
  `pub const REQUEST_SCHEMA_VERSION: &str = "lekalo/change-request/v${IDENTITY}";`,
  `pub const CATALOG_SCHEMA_VERSION: &str = "lekalo/change-catalog/v${IDENTITY}";`,
  `pub const POLICY_SCHEMA_VERSION: &str = "lekalo/change-policy/v${IDENTITY}";`,
  `pub const PLAN_SCHEMA_VERSION: &str = "lekalo/change-plan/v${IDENTITY}";`,
  `pub const RECEIPT_SCHEMA_VERSION: &str = "lekalo/change-receipt/v${IDENTITY}";`,
  `pub const HISTORY_SCHEMA_VERSION: &str = "lekalo/change-history/v${IDENTITY}";`,
]) {
  if (!versionRs.includes(constant)) fail("rust-identity-pin", constant);
}

// 4. The catalog: closed, sorted, implemented set bound to the executors.
const catalog = read("contracts/change-catalog.v0.8.0.json");
expectValid(validateCatalog, catalog, "catalog");
let previousOperation = null;
for (const entry of catalog.operations) {
  if (previousOperation !== null && entry.operation <= previousOperation) {
    fail("catalog-order", entry.operation);
  }
  previousOperation = entry.operation;
}
const implemented = catalog.operations
  .filter((entry) => entry.status === "implemented")
  .map((entry) => entry.operation)
  .sort();
const expectedImplemented = [
  "binding:add",
  "command-effect:add",
  "command-effect:remove",
  "definition:add",
  "entity-field:add",
  "entity-field:remove",
  "entity-field:rename",
  "enum-case:add",
  "enum-case:change",
  "migration:apply",
  "module:move",
  "operation-error:set",
  "operation-input:set",
  "operation-output:set",
  "query-effect:add",
  "query-effect:remove",
  "requirement:link",
  "requirement:unlink",
];
if (JSON.stringify(implemented) !== JSON.stringify(expectedImplemented)) {
  fail("catalog-implemented-set", { implemented, expectedImplemented });
}
// Every catalog discriminator appears exactly once in the request schema.
const requestSchema = read("contracts/change-request.schema.v0.8.0.json");
const oneOf = requestSchema.$defs.operation.oneOf;
const discriminators = oneOf
  .map((variant) => variant.properties?.operation?.const)
  .filter(Boolean)
  .sort();
const cataloged = catalog.operations.map((entry) => entry.operation).sort();
if (JSON.stringify(discriminators) !== JSON.stringify(cataloged)) {
  fail("catalog-request-parity", { discriminators, cataloged });
}

// 5. The committed request, policy, plan, receipt, and history documents.
const goldenRequest = read("tests/fixtures/change/golden/request-add-field.json");
expectValid(validateRequest, goldenRequest, "golden/request-add-field.json");
if (goldenRequest.schemaVersion !== "lekalo/change-request/v0.8.0") {
  fail("golden-request-version", goldenRequest.schemaVersion);
}

const goldenPlan = read("tests/fixtures/change/golden/plan-preview.json");
expectValid(validatePlan, goldenPlan, "golden/plan-preview.json");
if (goldenPlan.status !== "valid") fail("golden-plan-status", goldenPlan.status);
if (goldenPlan.mode !== "preview") fail("golden-plan-mode", goldenPlan.mode);
if (goldenPlan.complete !== true) fail("golden-plan-complete", goldenPlan.complete);
if (goldenPlan.fileEdits.length !== 1) fail("golden-plan-edits", goldenPlan.fileEdits.length);
const patch = goldenPlan.fileDiff[0].patch;
if (!patch.some((line) => line.kind === "insert")) fail("golden-plan-patch-insert");
// Canonical ordering: inputs and file edits sorted by logical path.
const sortedInputs = [...goldenPlan.inputs].sort((a, b) => (a.path < b.path ? -1 : 1));
if (JSON.stringify(goldenPlan.inputs) !== JSON.stringify(sortedInputs)) {
  fail("golden-plan-input-order");
}

const goldenReceipt = read("tests/fixtures/change/golden/receipt-committed.json");
expectValid(validateReceipt, goldenReceipt, "golden/receipt-committed.json");
if (goldenReceipt.state !== "committed") fail("golden-receipt-state", goldenReceipt.state);
if (goldenReceipt.replayed !== false) fail("golden-receipt-replayed");
if (goldenReceipt.transaction.state !== "committed") fail("golden-receipt-transaction");

const goldenHistory = read("tests/fixtures/change/golden/change-history.json");
expectValid(validateHistory, goldenHistory, "golden/change-history.json");
if (goldenHistory.nextSequence !== goldenHistory.records.length + 1) {
  fail("golden-history-sequence");
}
const seenKeys = new Set();
goldenHistory.records.forEach((record, index) => {
  if (record.sequence !== index + 1) fail("golden-history-record-order", record.sequence);
  if (seenKeys.has(record.keyDigest)) fail("golden-history-duplicate-key");
  seenKeys.add(record.keyDigest);
});

const samplePolicy = {
  schemaVersion: "lekalo/change-policy/v0.8.0",
  identity: "dev.lekalo.change-policy@0.8.0",
  allowBreakingClasses: ["wire-breaking"],
};
expectValid(validatePolicy, samplePolicy, "policy-sample");
// The closed class vocabulary refuses everything outside the breaking set.
if (validatePolicy({ ...samplePolicy, allowBreakingClasses: ["additive"] })) {
  fail("negative-policy-class-accepted");
}
if (validatePolicy({ ...samplePolicy, force: true })) {
  fail("negative-policy-member-accepted");
}

// 6. The negative cases: closed surfaces refuse unknown members.
if (
  validateRequest({
    schemaVersion: "lekalo/change-request/v0.8.0",
    operations: [{ operation: "entity-field:add", entity: "tasks.task", force: true }],
  })
) {
  fail("negative-unknown-member-accepted");
}
if (
  validateRequest({
    schemaVersion: "lekalo/change-request/v0.8.0",
    operations: [{ operation: "entity-field:teleport", entity: "tasks.task" }],
  })
) {
  fail("negative-unknown-operation-accepted");
}

// 7. The registry 0.8.0 successor: exact union of 0.6.5 plus the change family.
const predecessor = read("contracts/diagnostic-registry.v0.6.5.json");
const successor = read("contracts/diagnostic-registry.v0.8.0.json");
if (successor.registry_version !== IDENTITY) fail("registry-version", successor.registry_version);
if (successor.schema_version !== `lekalo/diagnostic-registry/v${IDENTITY}`) {
  fail("registry-discriminator");
}
if (successor.identity !== `dev.lekalo.diagnostic-registry@${IDENTITY}`) {
  fail("registry-identity");
}
const predecessorIds = new Map(predecessor.entries.map((entry) => [entry.id, entry]));
// Validate the actual additive union, including live tasklink/module-api
// and future families. This MUST check predecessor presence, not just size.
registryUnion(predecessor, successor);
// Regressions for the stale-union class: unrelated additive families must
// be accepted without editing this helper, and a same-size predecessor
// replacement MUST fail (the old optional lookup silently accepted it).
const liveSuperset = structuredClone(successor);
for (const [family, count, prefix] of [["tasklink", 8, "LEK-TL-"], ["module-api", 14, "LEK-MAPI-"]]) {
  if (liveSuperset.entries.some((entry) => entry.id.startsWith(`${family}.`))) continue;
  for (let index = 0; index < count; index += 1) {
    liveSuperset.entries.push({ ...successor.entries[0], id: `${family}.r10-${index}`, code: `${prefix}${900 + index}` });
  }
}
liveSuperset.entries.sort((a, b) => a.id < b.id ? -1 : 1);
registryUnion(predecessor, liveSuperset);
const replaced = structuredClone(successor);
const missingId = predecessor.entries[0].id;
replaced.entries.find((entry) => entry.id === missingId).id = "zz-regression.replacement";
replaced.entries.sort((a, b) => a.id < b.id ? -1 : 1);
let missingRefused = false;
try { registryUnion(predecessor, replaced); }
catch (error) { missingRefused = String(error).includes("registry-predecessor-missing"); }
if (!missingRefused) fail("registry-predecessor-presence-guard");
let previousId = null;
const codes = new Set();
for (const entry of successor.entries) {
  if (previousId !== null && entry.id <= previousId) fail("registry-order", entry.id);
  previousId = entry.id;
  if (codes.has(entry.code)) fail("registry-code-collision", entry.code);
  codes.add(entry.code);
  const original = predecessorIds.get(entry.id);
  if (original && JSON.stringify(original) !== JSON.stringify(entry)) {
    fail("registry-predecessor-entry-drift", entry.id);
  }
}
const changeEntries = successor.entries.filter((entry) => entry.id.startsWith("change."));
if (changeEntries.length !== 12) fail("registry-change-family-size", changeEntries.length);
const expectedChange = [
  ["change.commit-failed", "LEK-CHANGE-009"],
  ["change.compatibility-denied", "LEK-CHANGE-004"],
  ["change.history-invalid", "LEK-CHANGE-012"],
  ["change.idempotency-conflict", "LEK-CHANGE-006"],
  ["change.operation-unsupported", "LEK-CHANGE-002"],
  ["change.precondition-failed", "LEK-CHANGE-005"],
  ["change.recovery-required", "LEK-CHANGE-010"],
  ["change.request-invalid", "LEK-CHANGE-001"],
  ["change.required-evidence-unavailable", "LEK-CHANGE-011"],
  ["change.source-preservation-unavailable", "LEK-CHANGE-007"],
  ["change.subject-conflict", "LEK-CHANGE-003"],
  ["change.write-boundary-denied", "LEK-CHANGE-008"],
];
const actualChange = changeEntries
  .map((entry) => [entry.id, entry.code])
  .sort((a, b) => (a[0] < b[0] ? -1 : 1));
if (JSON.stringify(actualChange) !== JSON.stringify(expectedChange)) {
  fail("registry-change-family", actualChange);
}
// The 0.8.0 registry validates against its own schema.
const registrySchema = read("contracts/diagnostic-registry.schema.v0.8.0.json");
const validateRegistry = ajv.compile(registrySchema);
if (!validateRegistry(successor)) {
  fail("registry-schema-invalid", validateRegistry.errors);
}
// The Rust registry wiring embeds the 0.8.0 bytes and version.
const registryRs = readText("crates/lekalo-core/src/diagnostics/registry.rs");
if (!registryRs.includes("diagnostic-registry.v0.8.0.json")) {
  fail("rust-registry-bytes");
}
const diagnosticsVersionRs = readText("crates/lekalo-core/src/diagnostics/version.rs");
if (!diagnosticsVersionRs.includes(`pub const CHANGE_REGISTRY_VERSION: &str = "${IDENTITY}";`)) {
  fail("rust-registry-version");
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  gate: "test-change-contracts.mjs",
  contracts: Object.keys(pins).length,
  catalogOperations: catalog.operations.length,
  registryEntries: successor.entries.length,
  changeRules: expectedChange.length,
  goldens: 4,
}, null, 2)}\n`);
