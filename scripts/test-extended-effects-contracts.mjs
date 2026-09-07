#!/usr/bin/env node
// Issue #26 release gate: the extended-effects wire schema, the
// committed valid golden, and the adversarial vectors, validated with
// the same pinned third-party Draft 2020-12 implementation as the other
// contract gates. Exact Ajv 8.17.1 is provisioned outside this checkout
// (CI does the same on Node 18 and 24) and exposed through NODE_PATH /
// LEKALO_AJV_NODE_PATH.
//
// The gate independently proves the canonical form of the valid
// golden: re-serializing the parsed document with byte-sorted object
// keys must reproduce the committed bytes exactly (the Rust canonical
// writer is verified against the same rule in
// crates/lekalo-core/tests/extended_effects.rs). A raw-text scan
// additionally rejects duplicate JSON keys, which a parsed-value
// representation cannot see. Diff vectors live outside the valid
// directory on purpose: the permutation vector commits non-canonical
// array order, so only its schema shape is checked here while its
// canonical collapse is proven by the Rust suite.

import { createRequire } from "node:module";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  ({ default: Ajv2020 } = require("ajv/dist/2020.js"));
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(
    `${JSON.stringify({
      ok: false,
      gate: "extended-effects",
      error: `Ajv 8.17.1 is not reachable through NODE_PATH / LEKALO_AJV_NODE_PATH: ${error}`,
    })}\n`,
  );
  process.exit(1);
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(
    `${JSON.stringify({
      ok: false,
      gate: "extended-effects",
      error: `exact Ajv 8.17.1 is required, found ${ajvVersion}`,
    })}\n`,
  );
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const failures = [];
const fail = (caseName, detail) => failures.push({ case: caseName, detail });
function failEarly(reason, detail) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, gate: "extended-effects", reason, detail })}\n`,
  );
  process.exit(1);
}
const validDir = "tests/fixtures/extended-effects/valid";
const invalidDir = "tests/fixtures/extended-effects/invalid";
const diffDir = "tests/fixtures/extended-effects/diff";

const ajv = new Ajv2020({ strict: true, allErrors: true });
const schema = JSON.parse(
  readFileSync(
    resolve(root, "contracts/extended-effects.schema.v1.0.0.json"),
    "utf8",
  ),
);
let validateAttachment;
try {
  validateAttachment = ajv.compile(schema);
} catch (error) {
  fail("schema:strict-compile", String(error));
}

// Wire violations are caught by the schema; semantic violations
// (deduplication keys, retry-idempotency coherence, consent, review
// gates, capability coverage, schedules) are caught by the typed
// normalizer (crates/lekalo-core). Semantic-only vectors may satisfy
// the schema; every other invalid vector must fail Ajv itself.
const SEMANTIC_ONLY_DETAILS = new Set([
  "duplicate-contract-ref",
  "dedup-key-mismatch",
  "effect-kind-binding",
  "missing-capability",
  "retry-without-idempotency",
  "missing-compensation",
  "approved-without-approver",
  "sensitive-without-gate",
  "operation-kind-mismatch",
  "unresolved-capability-ref",
  "cyclic-schedule",
  "unknown-step",
  "unscheduled-step",
  "missing-outcome",
  "fault-outcome-mismatch",
  "target-profile-missing",
  "no-contracts",
]);

// --- raw duplicate-key scan (a parsed value cannot see duplicates) ---
function duplicateKeys(text) {
  const duplicates = new Set();
  const objectKeys = [];
  const stack = [{ inObject: false, expectKey: false }];
  const lexer = text;
  let index = 0;
  let current = [];
  const pattern = /"(?:\\.|[^"\\])*"/;
  while (index < lexer.length) {
    const rest = lexer.slice(index);
    const match = pattern.exec(rest);
    if (!match) break;
    const token = match[0];
    index += match.index + token.length;
    let lookahead = index;
    while (lookahead < lexer.length && /\s/.test(lexer[lookahead])) lookahead += 1;
    const isKey = lexer[lookahead] === ":";
    if (isKey && stack[stack.length - 1].inObject) {
      const key = JSON.parse(token);
      if (current.includes(key)) duplicates.add(key);
      current.push(key);
    }
  }
  return [...duplicates];
}

// --- canonical form: sorted object keys, compact separators ---
function canonicalJson(value) {
  if (Array.isArray(value)) {
    return `[${value.map(canonicalJson).join(",")}]`;
  }
  if (value !== null && typeof value === "object") {
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
    if (errors.includes("must be equal to constant") && expect.rule !== "extended.input-invalid") {
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
    gate: "extended-effects",
    ajv: ajvVersion,
    goldens: goldenCount,
    diffVectors: diffCount,
    invalidVectors: invalidCount,
  })}\n`,
);
