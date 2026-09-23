#!/usr/bin/env node
// Issue #87 release gate: the classification and dataflow diagnostic
// families in the additive diagnostic-registry v0.4.0 successor.
//
// The successor registry is created here as the shared M4 artifact: the
// base entries are the exact 0.3.2 set (additive chain), issue #85 adds
// the thirteen LEK-NFR rules, and issue #87 adds the twelve LEK-CLS and
// nine LEK-DFL rules validated below. Validated with the same pinned
// third-party Draft 2020-12 implementation as the other contract gates.
// Exact Ajv 8.17.1 is provisioned outside this checkout (CI does the
// same on Node 18 and 24) and exposed through NODE_PATH /
// LEKALO_AJV_NODE_PATH.

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

const registrySchema = read("contracts/diagnostic-registry.schema.v0.4.0.json");
const registry = read("contracts/diagnostic-registry.v0.4.0.json");
const predecessor = read("contracts/diagnostic-registry.v0.3.2.json");
const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateRegistry = ajv.compile(registrySchema);

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

// 1. The registry instance must satisfy its own schema.
if (!validateRegistry(registry)) {
  fail("registry-instance-invalid", validateRegistry.errors);
}

// 2. Registry invariants that JSON Schema cannot express.
const ids = registry.entries.map((entry) => entry.id);
const sorted = [...ids].sort();
if (ids.length !== new Set(ids).size) fail("duplicate-rule-ids");
if (ids.some((id, index) => id !== sorted[index])) fail("entries-not-sorted-by-id");
const codes = registry.entries.map((entry) => entry.code);
if (codes.length !== new Set(codes).size) fail("duplicate-codes");
for (const entry of registry.entries) {
  if (entry.message_id !== entry.id) fail("message-id-mismatch", entry.id);
  if (!["active", "reserved", "retired"].includes(entry.lifecycle)) {
    fail("unknown-lifecycle", entry.id);
  }
  if (entry.lifecycle === "retired" && !entry.replacement) {
    fail("retired-without-replacement", entry.id);
  }
  const names = entry.data_fields.map((field) => field.name);
  if (names.length !== new Set(names).size) fail("duplicate-data-field", entry.id);
}

// 3. The chain is additive end to end: every predecessor rule is
//    preserved byte-semantically (id, code, severity, statuses,
//    category, message, fields, lifecycle).
const predEntries = new Map(predecessor.entries.map((entry) => [entry.id, entry]));
if (predecessor.registry_version !== "0.3.2") fail("predecessor-version", predecessor.registry_version);
const projected = (entry) => JSON.stringify([
  entry.code, entry.category, entry.default_severity, entry.allowed_statuses,
  entry.default_message, entry.data_fields, entry.lifecycle,
]);
for (const [id, before] of predEntries) {
  const after = registry.entries.find((entry) => entry.id === id);
  if (!after) fail("predecessor-rule-dropped", id);
  if (projected(before) !== projected(after)) fail("predecessor-rule-drift", id);
}
if (registry.entries.length !== predEntries.size + 13 + 21 + 3 + 1) {
  // The trailing increments: +3 is the r3 F-5 custody increment
  // (dedicated ids for the project/model/IR pin refusals,
  // LEK-CLS-013..015); +1 is the r4 F-5 malformed-review-ref id
  // (LEK-CLS-016).
  fail("entry-count", registry.entries.length);
}

// 4. The classification family is exactly the sixteen LEK-CLS rules,
//    each active, error-severity, semantic-category, with closed
//    status sets: declaration failures invalidate; flow failures may
//    also deny. The three custody ids (review r3, F-5) give the
//    project/model/IR pin refusals their own greppable rules instead
//    of the generic unknown-kind id; the malformed-review-ref id
//    (r4 F-5) does the same for grant `approvedBy` shape failures.
const expectedClassificationRules = [
  ["classification.unknown-kind", "LEK-CLS-001", ["invalid"]],
  ["classification.unknown-subject", "LEK-CLS-002", ["invalid"]],
  ["classification.duplicate-subject", "LEK-CLS-003", ["invalid"]],
  ["classification.conflicting-kind", "LEK-CLS-004", ["invalid"]],
  ["classification.invalid-declassification", "LEK-CLS-005", ["invalid"]],
  ["classification.missing-approval", "LEK-CLS-006", ["invalid"]],
  ["classification.expired-declassification", "LEK-CLS-007", ["invalid", "denied"]],
  ["classification.self-approved", "LEK-CLS-008", ["invalid"]],
  ["classification.policy-missing", "LEK-CLS-009", ["invalid"]],
  ["classification.kind-rule-missing", "LEK-CLS-010", ["invalid"]],
  ["classification.sink-ceiling-exceeded", "LEK-CLS-011", ["invalid", "denied"]],
  ["classification.unclassified-sensitive-sink", "LEK-CLS-012", ["invalid", "denied"]],
  ["classification.custody-project", "LEK-CLS-013", ["invalid"]],
  ["classification.custody-model", "LEK-CLS-014", ["invalid"]],
  ["classification.custody-ir", "LEK-CLS-015", ["invalid"]],
  ["classification.malformed-review-ref", "LEK-CLS-016", ["invalid"]],
];
const registryEntries = new Map(registry.entries.map((entry) => [entry.id, entry]));
for (const [id, code, statuses] of expectedClassificationRules) {
  const entry = registryEntries.get(id);
  if (!entry) fail("classification-rule-missing", id);
  if (entry.code !== code) fail("classification-code-drift", { id, code: entry.code });
  if (entry.lifecycle !== "active") fail("classification-rule-lifecycle", id);
  if (entry.default_severity !== "error") fail("classification-rule-severity", id);
  if (entry.category !== "semantic") fail("classification-rule-category", id);
  if (JSON.stringify(entry.allowed_statuses) !== JSON.stringify(statuses)) fail("classification-status-drift", id);
}

// 4b. The report schema's gate-reason enum is exactly the GateReason
//     vocabulary the analyzer can produce (r4 F-4: the removed
//     `low-confidence`/`inputs-incomplete` arms stay out — the enum is
//     not a superset implying coverage that does not exist).
const reportSchema = read("contracts/data-flow-report.schema.v0.4.0.json");
const gateReasonEnum =
  reportSchema.$defs?.gate?.properties?.reason?.enum;
const expectedGateReasons = [
  "not-required",
  "destination-declared",
  "approval-present",
  "missing-destination",
  "missing-approval",
  "destination-forbidden",
  "unknown-flow",
  "sink-ceiling-exceeded",
  "unclassified-subject",
];
if (!gateReasonEnum) fail("report-schema-gate-enum-missing", "gate.properties.reason.enum");
if (JSON.stringify(gateReasonEnum) !== JSON.stringify(expectedGateReasons)) {
  fail("report-schema-gate-enum-drift", gateReasonEnum);
}

// 5. The dataflow family is exactly the nine LEK-DFL rules; the
//    observed-incompleteness signal is an error-severity finding whose
//    project-wide denial rides `inputsComplete: false` (review r2,
//    R2-4: producer and registry agree on error, and the rule can
//    never appear in a valid report).
const expectedDataflowRules = [
  ["dataflow.exposed-private-field", "LEK-DFL-001", ["invalid", "denied"]],
  ["dataflow.tenant-crossing", "LEK-DFL-002", ["invalid", "denied"]],
  ["dataflow.unknown-flow", "LEK-DFL-003", ["invalid", "denied"]],
  ["dataflow.low-confidence-sensitive", "LEK-DFL-004", ["invalid", "denied"]],
  ["dataflow.missing-destination", "LEK-DFL-005", ["invalid", "denied"]],
  ["dataflow.missing-approval", "LEK-DFL-006", ["invalid", "denied"]],
  ["dataflow.destination-forbidden", "LEK-DFL-007", ["invalid", "denied"]],
  ["dataflow.adapter-metadata-loss", "LEK-DFL-008", ["invalid", "denied"]],
  ["dataflow.observed-incomplete", "LEK-DFL-009", ["invalid", "denied"]],
];
for (const [id, code, statuses, severity = "error"] of expectedDataflowRules) {
  const entry = registryEntries.get(id);
  if (!entry) fail("dataflow-rule-missing", id);
  if (entry.code !== code) fail("dataflow-code-drift", { id, code: entry.code });
  if (entry.lifecycle !== "active") fail("dataflow-rule-lifecycle", id);
  if (entry.default_severity !== severity) fail("dataflow-rule-severity", id);
  if (entry.category !== "semantic") fail("dataflow-rule-category", id);
  if (JSON.stringify(entry.allowed_statuses) !== JSON.stringify(statuses)) fail("dataflow-status-drift", id);
}

// 6. The NFR family added by issue #85 on the same successor is intact.
const expectedNfrCodes = 13;
const nfrRules = registry.entries.filter((entry) => entry.code.startsWith("LEK-NFR-"));
if (nfrRules.length !== expectedNfrCodes) fail("nfr-family-count", nfrRules.length);

// 7. The embedded registry the Rust binary compiles is this successor,
//    and the Rust identity constants agree.
const registrySource = readText("crates/lekalo-core/src/diagnostics/registry.rs");
if (!registrySource.includes("diagnostic-registry.v0.4.0.json")) {
  fail("rust-embeds-predecessor", "diagnostic-registry.v0.4.0.json");
}
const diagnosticsVersionSource = readText("crates/lekalo-core/src/diagnostics/version.rs");
if (!diagnosticsVersionSource.includes('REGISTRY_VERSION: &str = "0.4.0"')) {
  fail("rust-registry-version", "0.4.0");
}
if (!diagnosticsVersionSource.includes('REGISTRY_IDENTITY: &str = "dev.lekalo.diagnostic-registry@0.4.0"')) {
  fail("rust-registry-identity", "0.4.0");
}
if (!diagnosticsVersionSource.includes('REGISTRY_SCHEMA_VERSION: &str = "lekalo/diagnostic-registry/v0.4.0"')) {
  fail("rust-registry-schema-version", "0.4.0");
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  ajv: ajvVersion,
  registryEntries: registry.entries.length,
  predecessorEntries: predEntries.size,
  classificationRules: expectedClassificationRules.length,
  dataflowRules: expectedDataflowRules.length,
}, null, 2)}\n`);
