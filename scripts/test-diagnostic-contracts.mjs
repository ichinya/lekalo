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
const registry = read("contracts/diagnostic-registry.v1.11.0.json");

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

// 1.11.0 is additive to the exact frozen target-protocol predecessor.
// Normalize checkout line endings only; every entry and its semantics survive.
const predecessorText = readFileSync(resolve(root, "contracts/diagnostic-registry.v1.10.0.json"), "utf8").replace(/\r\n/g, "\n");
if (createHash("sha256").update(predecessorText).digest("hex") !== "e043f45e3f46e3de6170f06118b57fea78c3063ba7ee3646ebd8522cce20eebd") {
  fail("predecessor-custody");
}
const predecessor = JSON.parse(predecessorText);
const isAdditive = (candidate) => {
  const entries = new Map(candidate.entries.map((entry) => [entry.id, entry]));
  return predecessor.entries.every((entry) => isDeepStrictEqual(entries.get(entry.id), entry));
};
if (!isAdditive(registry)) fail("predecessor-entry-drift");
const additions = registry.entries.filter((entry) => !predecessor.entries.some((old) => old.id === entry.id));
if (additions.length !== 10 || additions.some((entry) => !entry.id.startsWith("requirements.") || !entry.code.startsWith("LEK-REQ-"))) {
  fail("requirement-additions", additions.map((entry) => entry.id));
}
// A missing target entry or changed classification must actually fail the gate.
const target = predecessor.entries.find((entry) => entry.code.startsWith("LEK-TGT-"));
if (!target) fail("missing-target-predecessor");
const missing = structuredClone(registry);
missing.entries = missing.entries.filter((entry) => entry.id !== target.id);
const changed = structuredClone(registry);
changed.entries.find((entry) => entry.id === target.id).allowed_statuses = ["valid"];
if (isAdditive(missing) || isAdditive(changed)) fail("additive-negative-control");

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
