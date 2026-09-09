#!/usr/bin/env node
// Issue #11 release gate: the diagnostic wire schema, the registry schema,
// and the embedded registry instance, validated with the same pinned
// third-party Draft 2020-12 implementation as the other contract gates.
// Exact Ajv 8.17.1 is provisioned outside this checkout (CI does the same
// on Node 18 and 24) and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.

import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { isDeepStrictEqual } from "node:util";
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

const itemSchema = read("contracts/diagnostic.schema.v1.0.0.json");
const registrySchema = read("contracts/diagnostic-registry.schema.v1.0.0.json");
const registry = read("contracts/diagnostic-registry.v1.16.0.json");
// Predecessor custody: every accepted 1.14.0 rule must survive unchanged
// in the successor; the integrated chain is additive end to end and the
// observed family (issue #39) is purely additive on top of 1.14.0.
const predecessor = read("contracts/diagnostic-registry.v1.14.0.json");
const current = new Map(registry.entries.map((entry) => [entry.id, entry]));
for (const entry of predecessor.entries) {
  const successor = current.get(entry.id);
  if (!successor) fail("predecessor-rule-missing", entry.id);
  if (JSON.stringify(successor) !== JSON.stringify(entry)) {
    fail("predecessor-rule-changed", entry.id);
  }
}
for (const entry of registry.entries) {
  if (!entry.id.startsWith("observed.")) continue;
  if (!entry.code.startsWith("LEK-OBS-")) fail("observed-code-family", entry.id);
}

const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateItem = ajv.compile(itemSchema);
const validateRegistry = ajv.compile(registrySchema);

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

// 1. The embedded registry instance must satisfy its own schema.
if (!validateRegistry(registry)) {
  fail("registry-instance-invalid", validateRegistry.errors);
}

// Registry chain custody: 1.10.0 -> 1.11.0 (issue #36 requirements rules)
// -> 1.12.0 (issue #28 target.ir-unsupported) -> 1.13.0 (issue #38 init
// adoption rules) -> 1.14.0 (issue #29 target-profile.* rules; integrated
// additively over the accepted 1.13.0). Each successor is additive to its
// exact frozen predecessor: every predecessor entry and its semantics
// survive verbatim, and each step adds exactly its own family.
// Normalize checkout line endings only.
const registry110Text = readFileSync(resolve(root, "contracts/diagnostic-registry.v1.10.0.json"), "utf8").replace(/\r\n/g, "\n");
const registry111Text = readFileSync(resolve(root, "contracts/diagnostic-registry.v1.11.0.json"), "utf8").replace(/\r\n/g, "\n");
const registry112Text = readFileSync(resolve(root, "contracts/diagnostic-registry.v1.12.0.json"), "utf8").replace(/\r\n/g, "\n");
const registry113Text = readFileSync(resolve(root, "contracts/diagnostic-registry.v1.13.0.json"), "utf8").replace(/\r\n/g, "\n");
if (createHash("sha256").update(registry110Text).digest("hex") !== "e043f45e3f46e3de6170f06118b57fea78c3063ba7ee3646ebd8522cce20eebd") {
  fail("predecessor-custody");
}
if (createHash("sha256").update(registry111Text).digest("hex") !== "140d389824a72d4bf9a85b62a096b09052de8d40df4d9117bdf439d8f589598e") {
  fail("predecessor-custody");
}
if (createHash("sha256").update(registry112Text).digest("hex") !== "421d07a8838a0728a29cb96ff8c89d0e4da5a72d5609ac0ad1439d81da308a73") {
  fail("predecessor-custody");
}
if (createHash("sha256").update(registry113Text).digest("hex") !== "7f619034384ecf365c10097622d5d8eca5246b34be217e256ff6bdf319ca56a4") {
  fail("predecessor-custody");
}
const registry110 = JSON.parse(registry110Text);
const registry111 = JSON.parse(registry111Text);
const registry112 = JSON.parse(registry112Text);
const registry113 = JSON.parse(registry113Text);
const isAdditive = (candidate, predecessorRegistry) => {
  const entries = new Map(candidate.entries.map((entry) => [entry.id, entry]));
  return predecessorRegistry.entries.every((entry) => isDeepStrictEqual(entries.get(entry.id), entry));
};
// 1.11.0 added exactly the ten requirements.* rules over frozen 1.10.0.
if (!isAdditive(registry111, registry110)) fail("predecessor-entry-drift");
const requirementsAdditions = registry111.entries.filter((entry) => !registry110.entries.some((old) => old.id === entry.id));
if (requirementsAdditions.length !== 10 || requirementsAdditions.some((entry) => !entry.id.startsWith("requirements.") || !entry.code.startsWith("LEK-REQ-"))) {
  fail("requirement-additions", requirementsAdditions.map((entry) => entry.id));
}
// 1.12.0 added exactly the one target.* rule over 1.11.0.
if (!isAdditive(registry112, registry111)) fail("predecessor-entry-drift");
const targetAdditions = registry112.entries.filter((entry) => !registry111.entries.some((old) => old.id === entry.id));
if (targetAdditions.length !== 1 || !targetAdditions[0].id.startsWith("target.") || !targetAdditions[0].code.startsWith("LEK-TGT-")) {
  fail("target-additions", targetAdditions.map((entry) => entry.id));
}
// 1.13.0 added exactly the five init.adopt-* rules over 1.12.0.
if (!isAdditive(registry113, registry112)) fail("predecessor-entry-drift");
const initAdditions = registry113.entries.filter((entry) => !registry112.entries.some((old) => old.id === entry.id));
if (initAdditions.length !== 5 || initAdditions.some((entry) => !entry.id.startsWith("init.adopt-") || !entry.code.startsWith("LEK-INIT-"))) {
  fail("init-additions", initAdditions.map((entry) => entry.id));
}
// A missing predecessor entry or changed classification must actually fail the gate.
const target = registry111.entries.find((entry) => entry.code.startsWith("LEK-TGT-"));
const missing = structuredClone(registry);
missing.entries = missing.entries.filter((entry) => entry.id !== target.id);
const changed = structuredClone(registry);
changed.entries.find((entry) => entry.id === target.id).allowed_statuses = ["valid"];
if (isAdditive(missing, registry111) || isAdditive(changed, registry111)) fail("additive-negative-control");
const unsupported = registry112.entries.find((entry) => entry.id === "target.ir-unsupported");
const missing112 = structuredClone(registry);
missing112.entries = missing112.entries.filter((entry) => entry.id !== unsupported.id);
const changed112 = structuredClone(registry);
changed112.entries.find((entry) => entry.id === unsupported.id).allowed_statuses = ["valid"];
if (isAdditive(missing112, registry112) || isAdditive(changed112, registry112)) fail("additive-negative-control");
// 1.14.0 added exactly the six target-profile.* rules over frozen 1.13.0.
// The frozen accepted 1.14.0 instance is the reference for those checks;
// the current registry (1.16.0) additionally carries the observed family.
const registry114 = read("contracts/diagnostic-registry.v1.14.0.json");
if (!isAdditive(registry114, registry113)) fail("predecessor-entry-drift");
const profileAdditions = registry114.entries.filter((entry) => !registry113.entries.some((old) => old.id === entry.id));
if (
  profileAdditions.length !== 6 ||
  profileAdditions.some((entry) => !entry.id.startsWith("target-profile.") || !entry.code.startsWith("LEK-TPF-"))
) {
  fail("profile-additions", profileAdditions.map((entry) => entry.id));
}
// 1.16.0 (issue #39) added exactly the eleven observed.* rules over 1.14.0.
if (!isAdditive(registry, registry114)) fail("predecessor-entry-drift");
const observedAdditions = registry.entries.filter((entry) => !registry114.entries.some((old) => old.id === entry.id));
if (
  observedAdditions.length !== 11 ||
  observedAdditions.some((entry) => !entry.id.startsWith("observed.") || !entry.code.startsWith("LEK-OBS-"))
) {
  fail("observed-additions", observedAdditions.map((entry) => entry.id));
}
const profile = registry113.entries.find((entry) => entry.code.startsWith("LEK-TGT-"));
const missing113 = structuredClone(registry);
missing113.entries = missing113.entries.filter((entry) => entry.id !== profile.id);
const changed113 = structuredClone(registry);
changed113.entries.find((entry) => entry.id === profile.id).allowed_statuses = ["valid"];
if (isAdditive(missing113, registry113) || isAdditive(changed113, registry113)) fail("additive-negative-control");

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

// 3. The captured CLI envelopes are valid wire documents: every diagnostic
// inside them must satisfy the item schema.
const fixturesDir = resolve(root, "tests/fixtures/diagnostics");
const { readdirSync } = await import("node:fs");
const envelopes = readdirSync(fixturesDir)
  .filter((name) => name.endsWith(".json"))
  .map((name) => ({
    name,
    document: read(`tests/fixtures/diagnostics/${name}`),
  }));
let itemChecks = 0;
for (const envelope of envelopes) {
  const diagnostics = envelope.document.diagnostics;
  if (!Array.isArray(diagnostics) || diagnostics.length === 0) {
    fail("envelope-without-diagnostics", envelope.name);
  }
  for (const diagnostic of diagnostics) {
    if (!validateItem(diagnostic)) {
      fail("diagnostic-item-invalid", { envelope: envelope.name, errors: validateItem.errors });
    }
    const registered = registry.entries.find((entry) => entry.id === diagnostic.id);
    if (!registered) fail("unregistered-id", { envelope: envelope.name, id: diagnostic.id });
    if (!registered.allowed_statuses.includes(envelope.document.status)) {
      fail("status-not-allowed", { envelope: envelope.name, id: diagnostic.id, status: envelope.document.status });
    }
    if (registered.code !== diagnostic.code) fail("code-drift", { envelope: envelope.name, id: diagnostic.id });
    if (registered.default_severity !== diagnostic.severity) fail("severity-drift", envelope.name);
    if (registered.category !== diagnostic.category) fail("category-drift", envelope.name);
    if (registered.default_message !== diagnostic.message) fail("message-drift", envelope.name);
    itemChecks += 1;
  }
  const reasonCodes = envelope.document.reasonCodes;
  const expected = [...new Set(envelope.document.diagnostics.map((d) => d.id))];
  if (JSON.stringify(reasonCodes) !== JSON.stringify(expected)) {
    fail("reason-codes-not-derived", envelope.name);
  }
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  ajv: ajvVersion,
  registryEntries: registry.entries.length,
  envelopes: envelopes.length,
  diagnosticItems: itemChecks,
}, null, 2)}\n`);
