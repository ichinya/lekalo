#!/usr/bin/env node
// Issue #24 release gate: the transaction-concurrency wire schema, the
// committed valid goldens, and the adversarial vectors, validated with
// the same pinned third-party Draft 2020-12 implementation as the other
// contract gates. Exact Ajv 8.17.1 is provisioned outside this checkout
// (CI does the same on Node 18 and 24) and exposed through NODE_PATH /
// LEKALO_AJV_NODE_PATH.
//
// The gate independently proves the canonical form of every valid
// golden: re-serializing the parsed document with byte-sorted object
// keys must reproduce the committed bytes exactly (the Rust canonical
// writer is verified against the same rule in
// crates/lekalo-core/tests/transaction_concurrency.rs). A raw-text scan
// additionally rejects duplicate JSON keys, which a parsed-value
// representation cannot see. Diff vectors live outside the valid
// directory on purpose: the permutation vector commits non-canonical
// array order, so only its schema shape is checked here while its
// canonical collapse is proven by the Rust suite.

import { createRequire } from "node:module";
import { readdirSync, readFileSync } from "node:fs";
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
const validDir = "tests/fixtures/transaction-concurrency/valid";
const invalidDir = "tests/fixtures/transaction-concurrency/invalid";
const diffDir = "tests/fixtures/transaction-concurrency/diff";

const ajv = new Ajv2020({ strict: true, allErrors: true });
const schema = JSON.parse(
  readFileSync(
    resolve(root, "contracts/transaction-concurrency.schema.v1.0.0.json"),
    "utf8",
  ),
);
let validateAttachment;
try {
  validateAttachment = ajv.compile(schema);
} catch (error) {
  fail("schema:strict-compile", String(error));
}

// Wire violations are caught by the schema; semantic violations (group
// coverage, compensation, contradictions, schedules) are caught by the
// typed normalizer (crates/lekalo-core). Semantic-only vectors may
// satisfy the schema; every other invalid vector must fail Ajv itself.
const SEMANTIC_ONLY_DETAILS = new Set([
  "required-without-group",
  "external-effect-in-group",
  "duplicate-effect-ref",
  "foreign-effect-ref",
  "duplicate-group-id",
  "missing-compensation",
  "duplicate-lock-order-key",
  "key-ref-mismatch",
  "safety-condition-mismatch",
  "missing-required-capability",
  "unresolved-requirement-ref",
  "unresolved-invariant-ref",
  "cyclic-schedule",
  "unknown-participant",
  "outcome-mismatch",
  "unscheduled-invocation",
  "effect-kind",
]);

// --- raw duplicate-key scan (a parsed value cannot see duplicates) ---
function duplicateKeys(text) {
  const duplicates = new Set();
  const objectKeys = [];
  let index = 0;
  while (index < text.length) {
    const character = text[index];
    if (character === '"') {
      let end = index + 1;
      while (end < text.length) {
        if (text[end] === "\\") {
          end += 2;
          continue;
        }
        if (text[end] === '"') break;
        end += 1;
      }
      const literal = text.slice(index, end + 1);
      let probe = end + 1;
      while (probe < text.length && /\s/.test(text[probe])) probe += 1;
      const isKey = text[probe] === ":";
      if (isKey) {
        const scope = objectKeys[objectKeys.length - 1] ?? new Set();
        if (scope.has(literal)) duplicates.add(literal);
        scope.add(literal);
      }
      index = end + 1;
      continue;
    }
    if (character === "{") {
      objectKeys.push(new Set());
      index += 1;
      continue;
    }
    if (character === "[") {
      objectKeys.push(null);
      index += 1;
      continue;
    }
    if (character === "}" || character === "]") {
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
  if (!validateAttachment(value)) {
    fail(caseName, `schema rejected: ${ajv.errorsText(validateAttachment.errors)}`);
    continue;
  }
  const reserialized = canonicalJson(value);
  if (reserialized !== payload) {
    fail(caseName, "committed bytes are not the canonical form");
    continue;
  }
  goldenCount += 1;
}

if (goldenCount === 0) failEarly("no-goldens", "the valid fixture directory is empty");

// diff vectors: shape only; permutation deliberately keeps non-canonical
// array order (its canonical collapse is proven by the Rust suite).
let diffCount = 0;
for (const name of readdirSync(resolve(root, diffDir)).sort()) {
  if (!name.endsWith(".json")) continue;
  const text = readFileSync(resolve(root, diffDir, name), "utf8");
  const payload = text.endsWith("\n") ? text.slice(0, -1) : text;
  const caseName = `diff:${name}`;
  let value;
  try {
    value = JSON.parse(payload);
  } catch (error) {
    fail(caseName, `unparseable: ${error}`);
    continue;
  }
  if (!validateAttachment(value)) {
    fail(caseName, `schema rejected: ${ajv.errorsText(validateAttachment.errors)}`);
    continue;
  }
  diffCount += 1;
}
if (diffCount === 0) failEarly("no-diff-vectors", "the diff fixture directory is empty");

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
    fail(caseName, `unparseable: ${error}`);
    continue;
  }
  const rejected = !validateAttachment(value);
  if (rejected) {
    // The schema must reject for a wire reason, not accidentally.
    const errors = ajv.errorsText(validateAttachment.errors);
    if (errors.includes("must be equal to constant") && expect.rule !== "transaction.input-invalid") {
      fail(caseName, `unexpected constant mismatch: ${errors}`);
      continue;
    }
  } else if (!SEMANTIC_ONLY_DETAILS.has(expect.detail)) {
    fail(
      caseName,
      `expected a schema rejection for non-semantic detail ${expect.detail}`,
      // The Ajv gate cannot run the Rust normalizer; a schema-legal
      // vector with a non-semantic detail means the fixture taxonomy is
      // wrong.
    );
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
    ok: true,
    gate: "transaction-concurrency",
    ajv: ajvVersion,
    goldens: goldenCount,
    diffVectors: diffCount,
    invalidVectors: invalidCount,
  })}\n`,
);
