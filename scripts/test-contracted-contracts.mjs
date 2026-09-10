#!/usr/bin/env node
// Issue #40 release gate: the contracted-declaration wire schema, the
// committed declaration fixtures (conforming plus drift vectors), and
// the source-level custody invariants of the contracted seam, validated
// with the same pinned third-party Draft 2020-12 implementation as the
// other contract gates. Exact Ajv 8.17.1 is provisioned outside this
// checkout (CI does the same on Node 18 and 24) and exposed through
// NODE_PATH / LEKALO_AJV_NODE_PATH.
//
// This script is the independent second canonical implementation: it
// re-checks the closed vocabularies, the ownership boundaries (the
// registry home, the generated support home, and the refusal of
// canonical-model paths), and the diagnostic family identity without
// reusing any Rust code.

import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
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

const schema = read("contracts/contracted-declaration.schema.v1.0.0.json");

const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateDeclaration = ajv.compile(schema);

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

const DECLARATION_DIR = "tests/fixtures/contracted/planner-slice/declarations";

// ---------------------------------------------------------------------------
// 1. The conforming declaration fixture satisfies the schema.
// ---------------------------------------------------------------------------
const initial = read(`${DECLARATION_DIR}/initial.json`);
if (!validateDeclaration(initial)) {
  fail("declaration-invalid", validateDeclaration.errors);
}
if (initial.schemaVersion !== "lekalo/contracted-declaration/v1.0.0") {
  fail("declaration-identity", initial.schemaVersion);
}
if (initial.adapter.id !== "lekalo-target-node-typescript") {
  fail("declaration-adapter", initial.adapter.id);
}
const ids = initial.symbols.map((symbol) => symbol.id);
if (new Set(ids).size !== ids.length) fail("declaration-duplicate-id", ids.join(","));
const operations = initial.symbols.filter(
  (symbol) => symbol.kind === "command" || symbol.kind === "query",
);
for (const operation of operations) {
  if (!operation.signature) fail("operation-signature-missing", operation.id);
}
// Command claims must declare their effects; queries carry reads.
for (const symbol of initial.symbols) {
  if (symbol.kind === "command" && !symbol.effects) {
    fail("command-effects-missing", symbol.id);
  }
}
// Support artifacts are owned by declared symbols and confined to the
// generated home.
const declared = new Set(ids);
for (const artifact of initial.artifacts ?? []) {
  if (!declared.has(artifact.symbol)) fail("artifact-owner-unknown", artifact.symbol);
  if (!artifact.path.startsWith(".lekalo/generated/")) {
    fail("artifact-path-outside-generated-home", artifact.path);
  }
}

// ---------------------------------------------------------------------------
// 2. The drift fixtures differ from the conforming declaration in
// exactly their recorded dimension: a fixture must never pin a wrong
// claim against a conforming baseline.
// ---------------------------------------------------------------------------
const driftSignature = read(`${DECLARATION_DIR}/drift-signature.json`);
if (!validateDeclaration(driftSignature)) {
  fail("drift-signature-invalid", validateDeclaration.errors);
}
const initialInputs = JSON.stringify(initial.symbols[0].signature.inputs);
const driftInputs = JSON.stringify(driftSignature.symbols[0].signature.inputs);
if (initialInputs === driftInputs) fail("drift-signature-not-drifting", initialInputs);

const driftEffects = read(`${DECLARATION_DIR}/drift-effects.json`);
if (!validateDeclaration(driftEffects)) {
  fail("drift-effects-invalid", validateDeclaration.errors);
}
const initialEffects = JSON.stringify(initial.symbols[0].effects);
const driftedEffects = JSON.stringify(driftEffects.symbols[0].effects);
if (initialEffects === driftedEffects) fail("drift-effects-not-drifting", initialEffects);

// ---------------------------------------------------------------------------
// 3. Refusal vectors: hostile or out-of-custody documents fail the
// schema exactly like the Rust wire parser.
// ---------------------------------------------------------------------------
const refusal = (name, mutate, expectedKeyword) => {
  const document = JSON.parse(JSON.stringify(initial));
  mutate(document);
  if (validateDeclaration(document) === false) {
    const keywords = (validateDeclaration.errors ?? []).map((error) => error.keyword);
    if (expectedKeyword && !keywords.includes(expectedKeyword)) {
      fail(`refusal-${name}`, { expectedKeyword, keywords });
    }
    return;
  }
  fail(`refusal-accepted-${name}`, name);
};
refusal("unknown-top-key", (document) => {
  document.extra = 1;
}, "additionalProperties");
refusal("bad-schema-version", (document) => {
  document.schemaVersion = "lekalo/contracted-declaration/v9.9.9";
}, "const");
refusal("unbound-fingerprint", (document) => {
  document.symbols[0].fingerprint = "sha256:UPPER";
}, "pattern");
refusal("canonical-model-path", (document) => {
  document.symbols[0].source.path = "lekalo/modules/planner/commands.yaml";
}, "pattern");
refusal("support-outside-generated-home", (document) => {
  document.artifacts[0].path = "src/generated/planner.json";
}, "pattern");
refusal("unknown-effect-kind", (document) => {
  document.symbols[0].effects[0].kind = "drop-database";
}, "enum");

// ---------------------------------------------------------------------------
// 4. Source-level custody invariants: the persisted registry home, the
// support-path grammar, the mode identity, and the diagnostic family
// are compiled into the Rust source; this gate re-checks them without
// reusing Rust code.
// ---------------------------------------------------------------------------
const registryRust = readText("crates/lekalo-core/src/contracted/mod.rs");
if (!registryRust.includes("lekalo/import/contracted")) {
  fail("custody-registry-home", "the registry home must stay inside the import authority home");
}
const versionRust = readText("crates/lekalo-core/src/contracted/version.rs");
if (!versionRust.includes('MODE: &str = "contracted"')) {
  fail("custody-mode", "the contracted mode identity moved");
}
if (!versionRust.includes('SCHEMA_VERSION: &str = "lekalo/conformed-binding-registry/v1.0.0"')) {
  fail("custody-registry-schema", "the registry discriminator moved");
}
if (!versionRust.includes(".lekalo/generated/")) {
  fail("custody-support-home", "the support-path grammar must confine to the generated home");
}
const checkRust = readText("crates/lekalo-core/src/contracted/check.rs");
for (const detail of [
  '"fingerprint-mismatch"',
  '"source-missing"',
  '"signature"',
  '"effects"',
  '"unimplemented"',
  '"content-stale"',
  '"artifact-missing"',
]) {
  if (!checkRust.includes(detail)) fail("custody-drift-detail", detail);
}
const diagnosticRust = readText("crates/lekalo-core/src/contracted/diagnostic.rs");
for (const rule of [
  '"contracted.declaration-invalid"',
  '"contracted.declaration-limit"',
  '"contracted.unknown-module"',
  '"contracted.unknown-symbol"',
  '"contracted.registry-io"',
  '"contracted.binding-drift"',
  '"contracted.stale-artifact"',
  '"contracted.coverage-missing"',
]) {
  if (!diagnosticRust.includes(rule)) fail("custody-rule", rule);
}
// The registry family rides the reserved 1.19.0 successor with full
// predecessor custody; the frozen 1.16.0 bytes must never move.
const registry116Text = readText(
  "contracts/diagnostic-registry.v1.16.0.json",
).replace(/\r\n/g, "\n");
if (
  createHash("sha256").update(registry116Text).digest("hex") !==
  "02035b9c01451fd7e130dab7251b9b5213237fa433e2744795ae9bfbb222b485"
) {
  fail("custody-predecessor-registry", "diagnostic-registry.v1.16.0.json");
}
const registry119 = read("contracts/diagnostic-registry.v1.19.0.json");
const registry116 = JSON.parse(registry116Text);
const current = new Map(registry119.entries.map((entry) => [entry.id, entry]));
for (const entry of registry116.entries) {
  const successor = current.get(entry.id);
  if (!successor) fail("predecessor-rule-missing", entry.id);
  if (JSON.stringify(successor) !== JSON.stringify(entry)) {
    fail("predecessor-rule-changed", entry.id);
  }
}
const additions = registry119.entries.filter(
  (entry) => !registry116.entries.some((old) => old.id === entry.id),
);
if (
  additions.length !== 8 ||
  additions.some(
    (entry) => !entry.id.startsWith("contracted.") || !entry.code.startsWith("LEK-CNT-"),
  )
) {
  fail("contracted-additions", additions.map((entry) => entry.id));
}

process.stdout.write(
  `${JSON.stringify(
    {
      ok: true,
      schema: "lekalo/contracted-declaration/v1.0.0",
      declarationSymbols: ids.length,
      refusalVectors: 6,
      registryEntries: registry119.entries.length,
    },
    null,
    2,
  )}\n`,
);
