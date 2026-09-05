#!/usr/bin/env node
// Issue #23 release gate: the Scenario IR wire schema, the committed
// valid goldens, and the adversarial vectors, validated with the same
// pinned third-party Draft 2020-12 implementation as the other contract
// gates. Exact Ajv 8.17.1 is provisioned outside this checkout (CI does
// the same on Node 18 and 24) and exposed through NODE_PATH /
// LEKALO_AJV_NODE_PATH.
//
// The gate independently proves the canonical form of every golden:
// re-serializing the parsed document with byte-sorted object keys must
// reproduce the committed bytes exactly (the Rust canonical writer is
// verified against the same rule in crates/lekalo-core/tests/scenario.rs).
// A raw-text scan additionally rejects duplicate JSON keys, which a
// parsed-value representation cannot see.

import { createRequire } from "node:module";
import { readdirSync, readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  ({ default: Ajv2020 } = require("ajv/dist/2020"));
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(
    `ajv provisioning failed: ${error && error.message ? error.message : error}\n`,
  );
  process.exit(1);
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(
    `ajv ${ajvVersion} is not the pinned 8.17.1 contract gate\n`,
  );
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const failures = [];
const fail = (caseName, detail) => failures.push({ case: caseName, detail });
function failEarly(reason, detail) {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
}
const validDir = "tests/fixtures/scenario/valid";
const invalidDir = "tests/fixtures/scenario/invalid";

const ajv = new Ajv2020({ strict: true, allErrors: true });
const schema = JSON.parse(
  readFileSync(resolve(root, "contracts/scenario-ir.schema.v1.0.0.json"), "utf8"),
);
let validateScenario;
try {
  validateScenario = ajv.compile(schema);
} catch (error) {
  fail("schema:strict-compile", String(error));
}

// wire schema: scenario-local data-flow, data-flow and setup semantics
// validated by the typed normalizer (crates/lekalo-core). Every other
// invalid vector must fail the Ajv validation itself.
const SEMANTIC_ONLY_DETAILS = new Set([
  "duplicate-step-id",
  "replay-not-prior-action",
  "actor-undeclared",
  "forward-step-output",
  "given-dangling",
  "output-not-prior-action",
  "then-unreachable",
  "conflicting-setup",
  "idempotency-without-control",
  "undeclared-actor",
  "control-kind",
  "control-dangling",
  "replay-not-prior",
]);

// --- raw duplicate-key scan (a parsed value cannot see duplicates) ---
function duplicateKeys(text) {
  const duplicates = new Set();
  const objectKeys = [];
  let index = 0;
  const isValueContext = () =>
    objectKeys.length === 0 || objectKeys[objectKeys.length - 1] !== null;
  while (index < text.length) {
    const character = text[index];
    if (character === '\"') {
      let end = index + 1;
      while (end < text.length) {
        if (text[end] === '\\') {
          end += 2;
          continue;
        }
        if (text[end] === '\"') break;
        end += 1;
      }
      const literal = text.slice(index, end + 1);
      let probe = end + 1;
      while (probe < text.length && /\s/.test(text[probe])) probe += 1;
      const isKey = text[probe] === ':';
      if (isKey) {
        const scope = objectKeys[objectKeys.length - 1] ?? new Set();
        if (scope.has(literal)) duplicates.add(literal);
        scope.add(literal);
      }
      index = end + 1;
      continue;
    }
    if (character === '{') {
      objectKeys.push(new Set());
      index += 1;
      continue;
    }
    if (character === '[') {
      objectKeys.push(null);
      index += 1;
      continue;
    }
    if (character === '}' || character === ']') {
      objectKeys.pop();
      index += 1;
      continue;
    }
    index += 1;
  }
  return [...duplicates];
}

// --- canonical form: sorted object keys, compact separators ---
function canonicalJson(value) {
  if (Array.isArray(value)) {
    return `[${value.map(canonicalJson).join(",")}]`;
  }
  if (value && typeof value === "object") {
    const members = Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`);
    return `{${members.join(",")}}`;
  }
  return JSON.stringify(value);
}

let goldenCount = 0;
for (const name of readdirSync(resolve(root, validDir)).sort()) {
  if (!name.endsWith(".json")) continue;
  const text = readFileSync(resolve(root, validDir, name), "utf8");
  const payload = text.endsWith("\n") ? text.slice(0, -1) : text;
  const caseName = `valid:${name}`;
  let value;
  try {
    value = JSON.parse(payload);
  } catch (error) {
    fail(caseName, `unparseable: ${error}`);
    continue;
  }
  const duplicateScan = duplicateKeys(payload);
  if (duplicateScan.length > 0) {
    fail(caseName, `duplicate keys: ${duplicateScan.join(", ")}`);
  }
  if (!validateScenario(value)) {
    fail(caseName, `schema rejected: ${ajv.errorsText(validateScenario.errors)}`);
    continue;
  }
  const reserialized = canonicalJson(value);
  if (reserialized !== payload) {
    fail(caseName, "committed bytes are not the canonical form");
    continue;
  }
  const digest = createHash("sha256").update(payload, "utf8").digest("hex");
  if (!/^[0-9a-f]{64}$/.test(digest)) {
    fail(caseName, "digest shape");
  }
  goldenCount += 1;
}

if (goldenCount === 0) failEarly("no-goldens", "the valid fixture directory is empty");

let invalidCount = 0;
for (const name of readdirSync(resolve(root, invalidDir)).sort()) {
  if (!name.endsWith(".json") || name.endsWith(".expect.json")) continue;
  const base = name.slice(0, -".json".length);
  const text = readFileSync(resolve(root, invalidDir, name), "utf8");
  const payload = text.endsWith("\n") ? text.slice(0, -1) : text;
  const expect = JSON.parse(
    readFileSync(resolve(root, invalidDir, `${base}.expect.json`), "utf8"),
  );
  const caseName = `invalid:${base}`;
  if (typeof expect.detail !== "string" || typeof expect.rule !== "string") {
    fail(caseName, "expectation must record detail and rule");
    continue;
  }
  let value;
  try {
    value = JSON.parse(payload);
  } catch (error) {
    // Unparseable vectors are transport-domain; the typed normalizer
    // never sees them. Schema-level vectors must parse.
    if (!SEMANTIC_ONLY_DETAILS.has(expect.detail)) fail(caseName, `unparseable: ${error}`);
    invalidCount += 1;
    continue;
  }
  const valid = validateScenario(value);
  if (SEMANTIC_ONLY_DETAILS.has(expect.detail)) {
    if (!valid) {
      fail(
        caseName,
        `semantic vector must pass the wire schema: ${ajv.errorsText(validateScenario.errors)}`,
      );
    }
  } else if (valid) {
    fail(caseName, `schema must reject the vector (detail ${expect.detail})`);
  }
  invalidCount += 1;
}

if (invalidCount === 0) failEarly("no-adversarials", "the invalid fixture directory is empty");

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, failures }, null, 2)}\n`);
  process.exit(1);
}
process.stdout.write(
  `${JSON.stringify({ ok: true, checked: "scenario-contracts-v1", goldenCount, invalidCount }, null, 2)}\n`,
);
