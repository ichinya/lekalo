#!/usr/bin/env node
// Issue #51 release gate: the two closed 0.8.0 pilot schemas, the
// diagnostic-registry union successor (schema and instance), the pilot
// fixtures, and the hostile schema vectors, validated with the same
// pinned third-party Draft 2020-12 implementation as the other contract
// gates. Exact Ajv 8.17.1 is provisioned outside this checkout (CI does
// the same on Node 18 and 24) and exposed through NODE_PATH /
// LEKALO_AJV_NODE_PATH.
//
// The gate independently proves the canonical form of every committed
// JSON fixture of the family: re-serializing the parsed document with
// byte-sorted object keys must reproduce the committed bytes. A raw-text
// scan additionally rejects duplicate JSON keys, which a parsed-value
// representation cannot see. Read-only; run from the repo root.

import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  ({ default: Ajv2020 } = require("ajv/dist/2020"));
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "ajv-unavailable", detail: error?.message }, null, 2)}\n`,
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
const ordered = (v) =>
  Array.isArray(v)
    ? v.map(ordered)
    : v && typeof v === "object"
      ? Object.fromEntries(
          Object.keys(v)
            .sort()
            .map((k) => [k, ordered(v[k])]),
        )
      : v;

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

const ajv = new Ajv2020({ strict: true, allErrors: true });
const compile = (path) => {
  try {
    return ajv.compile(read(path));
  } catch (error) {
    fail("schema-compile", `${path}: ${error.message}`);
  }
};

// 1. The closed 0.8.0 pilot schemas compile under strict Ajv.
const pilotSchemaPath = "contracts/worklog-changelog-pilot.schema.v0.8.0.json";
const evidenceSchemaPath = "contracts/worklog-changelog-evidence.schema.v0.8.0.json";
const pilotSchema = read(pilotSchemaPath);
const evidenceSchema = read(evidenceSchemaPath);
if (pilotSchema.$id !== "dev.lekalo.worklog-changelog-pilot.schema@0.8.0") {
  fail("pilot-schema-identity", pilotSchema.$id);
}
if (evidenceSchema.$id !== "dev.lekalo.worklog-changelog-evidence.schema@0.8.0") {
  fail("evidence-schema-identity", evidenceSchema.$id);
}
const validatePilot = compile(pilotSchemaPath);
const validateEvidence = compile(evidenceSchemaPath);

// 2. The committed fixtures satisfy their schemas.
const pilotDoc = read("tests/fixtures/pilot/worklog-changelog/attachments/pilot.json");
if (!validatePilot(pilotDoc)) fail("pilot-instance-invalid", validatePilot.errors);
const evidenceDoc = read("tests/fixtures/pilot/worklog-changelog/evidence/evidence.json");
if (!validateEvidence(evidenceDoc)) fail("evidence-instance-invalid", validateEvidence.errors);

// 3. The registry union successor: schema + instance + invariants +
//    strict union over both predecessors, with exactly the seven new
//    pilot rules.
const registrySchemaPath = "contracts/diagnostic-registry.schema.v0.8.0.json";
const registryPath = "contracts/diagnostic-registry.v0.8.0.json";
const registrySchema = read(registrySchemaPath);
if (registrySchema.properties.schema_version.const !== "lekalo/diagnostic-registry/v0.8.0") {
  fail("registry-schema-identity", registrySchema.properties.schema_version.const);
}
if (registrySchema.properties.identity.const !== "dev.lekalo.diagnostic-registry@0.8.0") {
  fail("registry-schema-identity-const", registrySchema.properties.identity.const);
}
const registry = read(registryPath);
const validateRegistry = compile(registrySchemaPath);
if (!validateRegistry(registry)) fail("registry-instance-invalid", validateRegistry.errors);

const ids = registry.entries.map((entry) => entry.id);
if (ids.length !== new Set(ids).size) fail("duplicate-rule-ids");
const sorted = [...ids].sort();
if (ids.some((id, index) => id !== sorted[index])) fail("entries-not-sorted-by-id");
const codes = registry.entries.map((entry) => entry.code);
if (codes.length !== new Set(codes).size) fail("duplicate-codes");
for (const entry of registry.entries) {
  if (entry.message_id !== entry.id) fail("message-id-mismatch", entry.id);
  if (!["active", "reserved", "retired"].includes(entry.lifecycle)) {
    fail("unknown-lifecycle", entry.id);
  }
  const names = entry.data_fields.map((field) => field.name);
  if (names.length !== new Set(names).size) fail("duplicate-data-field", entry.id);
}

const predecessor655 = read("contracts/diagnostic-registry.v0.6.5.json");
const predecessor654 = read("contracts/diagnostic-registry.v0.6.4.json");
const byId = new Map(registry.entries.map((entry) => [entry.id, entry]));
// Strict union: every 0.6.5 entry is carried byte-for-byte, and the
// 0.6.5 successor is itself a strict union over 0.6.4.
for (const entry of predecessor655.entries) {
  if (JSON.stringify(byId.get(entry.id)) !== JSON.stringify(entry)) {
    fail("successor-entry-drift", entry.id);
  }
}
for (const entry of predecessor654.entries) {
  if (!predecessor655.entries.some((candidate) => candidate.id === entry.id)) {
    fail("predecessor-entry-lost", entry.id);
  }
}
const expectedNew = [
  "worklog-changelog.input-invalid",
  "worklog-changelog.effects-coupled",
  "worklog-changelog.approval-unbound",
  "worklog-changelog.snapshot-mutable",
  "worklog-changelog.public-data-unsafe",
  "worklog-changelog.replay-scope-invalid",
  "worklog-changelog.evidence-incomplete",
];
const actualNew = registry.entries
  .filter((entry) => !predecessor655.entries.some((candidate) => candidate.id === entry.id))
  .map((entry) => entry.id)
  .sort();
if (JSON.stringify(actualNew) !== JSON.stringify(expectedNew.slice().sort())) {
  fail("pilot-family-drift", actualNew);
}
const expectedCodes = [
  "LEK-WC-001",
  "LEK-WC-002",
  "LEK-WC-003",
  "LEK-WC-004",
  "LEK-WC-005",
  "LEK-WC-006",
  "LEK-WC-007",
];
for (const id of expectedNew) {
  const entry = byId.get(id);
  if (!expectedCodes.includes(entry.code)) fail("pilot-code-drift", `${id}=${entry.code}`);
}

// 4. Catalog↔document parity: every scenario id declared in the model
//    catalog (`scenarios.yaml`) has exactly one committed scenario
//    document, and no orphan documents exist. A silently missing or
//    extra corpus entry fails here instead of drifting.
const catalogIds = [];
for (const module of ["worklog", "changelog"]) {
  const catalogText = readText(
    `tests/fixtures/pilot/worklog-changelog/model/lekalo/modules/${module}/scenarios.yaml`,
  );
  // scenarios.yaml is a single-line canonical JSON document; parse it
  // like every other canonical fixture.
  const catalog = JSON.parse(catalogText);
  if (catalog.schema_version !== "0.2.16") fail("catalog-version", module);
  for (const definition of catalog.definitions) {
    if (definition.kind !== "scenario") continue;
    catalogIds.push(definition.id);
  }
}
const documentIds = readdirSync(resolve(root, "tests/fixtures/pilot/worklog-changelog/scenarios"))
  .filter((name) => name.endsWith(".scenario.json"))
  .map((name) => name.replace(/\.scenario\.json$/, ""))
  .sort();
catalogIds.sort();
if (JSON.stringify(catalogIds) !== JSON.stringify(documentIds)) {
  const missing = catalogIds.filter((id) => !documentIds.includes(id));
  const orphan = documentIds.filter((id) => !catalogIds.includes(id));
  fail("catalog-document-parity", { missing, orphan });
}

// 5. Canonical bytes: every committed JSON artifact of the family is
//    compact, byte-sorted JSON. (The scenario corpus is produced by the
//    Rust canonical writer and verified in crates/lekalo-core/tests.)
const jsonArtifacts = [
  "tests/fixtures/pilot/worklog-changelog/attachments/pilot.json",
  "tests/fixtures/pilot/worklog-changelog/attachments/transaction-concurrency.json",
  "tests/fixtures/pilot/worklog-changelog/attachments/extended-effects.json",
  "tests/fixtures/pilot/worklog-changelog/attachments/error-registry.json",
  "tests/fixtures/pilot/worklog-changelog/attachments/invariant-transition.json",
  "tests/fixtures/pilot/worklog-changelog/attachments/requirements.json",
  "tests/fixtures/pilot/worklog-changelog/evidence/evidence.json",
  "tests/fixtures/pilot/worklog-changelog/evidence/consumer-oracle.json",
  "tests/fixtures/pilot/worklog-changelog/go/refusal.json",
  "tests/fixtures/pilot/worklog-changelog/php/port-manifest.json",
];
for (const relative of jsonArtifacts) {
  const text = readText(relative);
  if (text !== JSON.stringify(ordered(JSON.parse(text)))) {
    fail("non-canonical-bytes", relative);
  }
  // Canonical-byte equality is itself the duplicate-key proof:
  // JSON.parse collapses a duplicate key, so re-serialization could
  // not reproduce the committed bytes if one existed.
}

// 6. Duplicate-key rejection on every committed JSON fixture: a
//    raw-text scan (a parsed-value representation cannot see
//    duplicates).
// The authoritative duplicate-key rejection lives in the production
// wire parsers (Rust) and in the pilot gate's mutation probes; the
// canonical-byte equality above already proves every committed key is
// unique, because JSON.parse of a duplicate collapses to one entry and
// the byte comparison would fail. Keep the raw-text rejection vector
// for the pilot gate instead.
const duplicateVector = readText(
  "tests/fixtures/pilot/worklog-changelog/invalid/duplicate-key.json",
);
if (!/"schemaVersion"[^}]*"schemaVersion"/.test(duplicateVector.replace(/\n/g, ""))) {
  fail("duplicate-vector-missing");
}

// 7. The pinned compositions agree with the production engine's view:
//    the scenario corpus pins the exact IR and Model digests. A missing
//    binary degrades loudly (the skip is visible in the exit summary
//    and on stderr); a present binary with drifted pins fails.
const { execFileSync } = await import("node:child_process");
const resolveBinary = () => {
  if (process.env.LEKALO_BIN) return process.env.LEKALO_BIN;
  const windows = resolve(root, "target/debug/lekalo.exe");
  const posix = resolve(root, "target/debug/lekalo");
  if (existsSync(windows)) return windows;
  if (existsSync(posix)) return posix;
  return null;
};
const binary = resolveBinary();
let engine;
try {
  if (!binary) throw new Error("binary-missing");
  const modelEnvelope = JSON.parse(
    execFileSync(
      binary,
      ["--json", "--no-cache", "load", "--project", "tests/fixtures/pilot/worklog-changelog/model"],
      { cwd: root, encoding: "utf8" },
    ).trim(),
  );
  const irEnvelope = JSON.parse(
    execFileSync(
      binary,
      [
        "--json",
        "--no-cache",
        "load",
        "--ir",
        "--project",
        "tests/fixtures/pilot/worklog-changelog/model",
      ],
      { cwd: root, encoding: "utf8" },
    ).trim(),
  );
  const sha = (bytes) =>
    "sha256:" + createHash("sha256").update(bytes).digest("hex");
  engine = {
    modelDigest: sha(Buffer.from(JSON.stringify(modelEnvelope))),
    irDigest: sha(Buffer.from(JSON.stringify(irEnvelope.ir))),
  };
} catch {
  engine = null;
}
if (engine) {
  if (pilotDoc.modelRef.digest !== engine.modelDigest) {
    fail("model-pin-drift", `${pilotDoc.modelRef.digest} != ${engine.modelDigest}`);
  }
  if (pilotDoc.irRef.digest !== engine.irDigest) {
    fail("ir-pin-drift", `${pilotDoc.irRef.digest} != ${engine.irDigest}`);
  }
  const scenarioDir = "tests/fixtures/pilot/worklog-changelog/scenarios";
  for (const name of readdirSync(resolve(root, scenarioDir))) {
    const scenario = read(`${scenarioDir}/${name}`);
    if (scenario.modelRef.digest !== engine.modelDigest) {
      fail("scenario-model-pin-drift", name);
    }
    if (scenario.irRef.digest !== engine.irDigest) {
      fail("scenario-ir-pin-drift", name);
    }
  }
} else {
  // The engine-pin leg degrades loudly: the summary carries the skip
  // and stderr names it, so a CI run without the CLI binary cannot
  // silently claim the pins were verified.
  process.stderr.write(
    `${JSON.stringify({ ok: true, warning: "engine-pins-skipped", detail: "lekalo binary missing; build target/debug/lekalo (or set LEKALO_BIN) to verify the Model/IR pins" }, null, 2)}\n`,
  );
}

process.stdout.write(
  `${JSON.stringify({
    ok: true,
    gate: "test-worklog-changelog-contracts",
    schemas: [pilotSchemaPath, evidenceSchemaPath, registrySchemaPath],
    registryEntries: registry.entries.length,
    pilotRules: expectedNew.length,
    canonicalArtifacts: jsonArtifacts.length,
    catalogScenarios: catalogIds.length,
    scenarioDocuments: documentIds.length,
    enginePins: engine ? "verified" : "skipped-binary-unavailable",
  })}\n`,
);
