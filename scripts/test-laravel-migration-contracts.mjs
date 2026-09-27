#!/usr/bin/env node
// Issue #57 release gate: the Laravel-migration-input contract schema,
// the committed valid goldens (rendered by the core pipeline itself),
// and the adversarial schema vectors, validated with the same pinned
// third-party Draft 2020-12 implementation as the other contract gates
// (exact Ajv 8.17.1, provisioned outside this checkout and exposed
// through NODE_PATH / LEKALO_AJV_NODE_PATH).
//
// The gate independently proves the canonical form of the goldens:
// re-serializing the parsed document with byte-sorted object keys must
// reproduce the committed bytes exactly. The golden set also pins the
// effective gate semantics: the additive golden is ready, the
// destructive and backfill goldens are confirmed (never ready), and
// the rename golden carries its history digest.
import { createRequire } from "node:module";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  ({ default: Ajv2020 } = require("ajv/dist/2020"));
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  failEarly("ajv-unavailable", String(error));
}
if (ajvVersion !== "8.17.1") failEarly("ajv-version", ajvVersion);

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => readFileSync(resolve(root, relative), "utf8");
const readJson = (relative) => JSON.parse(read(relative));

const failures = [];
const fail = (caseName, detail) => failures.push({ case: caseName, detail });
function failEarly(reason, detail) {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
}

const ajv = new Ajv2020({ strict: true, allErrors: true });
const schema = readJson("contracts/laravel-migration-input.schema.v0.4.0.json");
const validate = ajv.compile(schema);

// --- canonical form: sorted object keys, compact separators ---
function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value && typeof value === "object") {
    const body = Object.keys(value)
      .sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)))
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`);
    return `{${body.join(",")}}`;
  }
  return JSON.stringify(value);
}

// The valid goldens: schema-valid, canonical, and gate-consistent.
const GOLDEN_EXPECTATIONS = {
  "additive.json": { status: "ready", gated: false, backfillGated: false, history: false },
  "destructive-confirmed.json": { status: "confirmed", gated: true, backfillGated: false, history: false },
  "backfill-confirmed.json": { status: "confirmed", gated: false, backfillGated: true, history: false },
  "rename-confirmed.json": { status: "confirmed", gated: true, backfillGated: false, history: true },
  "column-rename-confirmed.json": { status: "confirmed", gated: true, backfillGated: false, history: true },
};
let goldenCount = 0;
const goldensDir = "tests/fixtures/laravel-migrations/golden";
for (const name of readdirSync(resolve(root, goldensDir)).sort()) {
  if (!name.endsWith(".json")) continue;
  const text = read(`${goldensDir}/${name}`);
  let document;
  try {
    document = JSON.parse(text);
  } catch (error) {
    fail(`golden:${name}:json`, String(error));
    continue;
  }
  if (!validate(document)) {
    fail(`golden:${name}:schema`, validate.errors);
    continue;
  }
  if (canonicalJson(document) !== text.trim()) {
    fail(`golden:${name}:canonical`, "committed bytes deviate from canonical form");
    continue;
  }
  const expected = GOLDEN_EXPECTATIONS[name];
  if (!expected) {
    fail(`golden:${name}:expectation`, "every golden must declare its gate semantics");
    continue;
  }
  if (document.effectiveStatus !== expected.status) {
    fail(`golden:${name}:status`, `${document.effectiveStatus} != ${expected.status}`);
    continue;
  }
  if (document.gated !== expected.gated || document.backfillGated !== expected.backfillGated) {
    fail(`golden:${name}:gate-flags`, "the gate flags contradict the declared expectations");
    continue;
  }
  if (expected.history !== Boolean(document.historyDigest)) {
    fail(`golden:${name}:history`, "the history presence contradicts the declared expectations");
    continue;
  }
  if (document.effectiveStatus === "ready" && document.gated) {
    fail(`golden:${name}:ready-with-destructive`, "a ready plan never carries destructive steps");
    continue;
  }
  goldenCount += 1;
}
if (goldenCount !== Object.keys(GOLDEN_EXPECTATIONS).length) {
  failEarly("missing-goldens", `expected ${Object.keys(GOLDEN_EXPECTATIONS).length} goldens, validated ${goldenCount}`);
}

// The invalid vectors: schema-level rejections with their expect
// files. The expectation is load-bearing: every vector must declare a
// schema signal, and the actual Ajv rejection must carry it.
const EXPECTED_AJV_SIGNALS = {
  "unknown-field": (errors) =>
    errors.some((error) => error.keyword === "additionalProperties"),
  "unknown-operation-kind": (errors) =>
    errors.some((error) => error.keyword === "enum" && error.instancePath.includes("/kind")),
  "wrong-schema-version": (errors) =>
    errors.some(
      (error) =>
        (error.keyword === "const" || error.keyword === "enum") &&
        /\/schemaVersion$/.test(error.instancePath),
    ),
  "bad-plan-id-shape": (errors) =>
    errors.some((error) => error.keyword === "pattern" && error.instancePath.includes("/planId")),
};
let invalidCount = 0;
const invalidDir = "tests/fixtures/laravel-migrations/invalid";
for (const name of readdirSync(resolve(root, invalidDir)).sort()) {
  if (!name.endsWith(".json") || name.endsWith(".expect.json")) continue;
  let document;
  let expect;
  try {
    document = readJson(`${invalidDir}/${name}`);
    expect = readJson(`${invalidDir}/${name.replace(/\.json$/, ".expect.json")}`);
  } catch (error) {
    fail(`invalid:${name}:json`, String(error));
    continue;
  }
  if (expect.rule !== "schema") {
    fail(`invalid:${name}:rule`, `the gate only exercises schema rejections, got ${expect.rule}`);
    continue;
  }
  const signal = EXPECTED_AJV_SIGNALS[expect.detail];
  if (!signal) {
    fail(`invalid:${name}:expectation`, `no schema signal mapped for detail ${expect.detail}`);
    continue;
  }
  if (validate(document)) {
    fail(`invalid:${name}:schema-passed`, expect.detail);
    continue;
  }
  if (!signal(validate.errors ?? [])) {
    fail(`invalid:${name}:wrong-reason`, expect.detail);
    continue;
  }
  invalidCount += 1;
}
if (invalidCount === 0) failEarly("no-invalid-vectors", "the invalid fixture directory is empty");

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, failures }, null, 2)}\n`);
  process.exit(1);
}
process.stdout.write(
  `${JSON.stringify({
    ajv: ajvVersion,
    goldens: goldenCount,
    invalidVectors: invalidCount,
    ok: true,
  })}\n`,
);
