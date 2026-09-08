#!/usr/bin/env node
// Issue #11 release gate: the diagnostic wire schema, the registry schema,
// and the embedded registry instance, validated with the same pinned
// third-party Draft 2020-12 implementation as the other contract gates.
// Exact Ajv 8.17.1 is provisioned outside this checkout (CI does the same
// on Node 18 and 24) and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.

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
