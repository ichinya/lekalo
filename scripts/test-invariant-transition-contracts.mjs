#!/usr/bin/env node
// Issue #63 release gate: the invariant-transition wire schema, the
// committed valid golden, and the adversarial vectors, validated with
// the same pinned third-party Draft 2020-12 implementation as the
// other contract gates. Exact Ajv 8.17.1 is provisioned outside this
// checkout (CI does the same on Node 18 and 24) and exposed through
// NODE_PATH / LEKALO_AJV_NODE_PATH.
//
// The gate independently proves the canonical form of the valid
// golden: re-serializing the parsed document with byte-sorted object
// keys must reproduce the committed bytes exactly (the Rust canonical
// writer is verified against the same rule in
// crates/lekalo-core/tests/invariant_transition.rs). A raw-text scan
// additionally rejects duplicate JSON keys, which a parsed-value
// representation cannot see. Diff vectors live outside the valid
// directory on purpose: the permutation vector commits non-canonical
// array order, so only its schema shape is checked here while its
// canonical collapse is proven by the Rust suite.

import { createRequire } from "node:module";
import { readFileSync, readdirSync } from "node:fs";
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
    `${JSON.stringify({
      ok: false,
      gate: "invariant-transition",
      error: `Ajv 8.17.1 is not reachable through NODE_PATH / LEKALO_AJV_NODE_PATH: ${error}`,
    })}\n`,
  );
  process.exit(1);
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(
    `${JSON.stringify({
      ok: false,
      gate: "invariant-transition",
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
    `${JSON.stringify({ ok: false, gate: "invariant-transition", reason, detail })}\n`,
  );
  process.exit(1);
}
const validDir = "tests/fixtures/invariant-transition/valid";
const invalidDir = "tests/fixtures/invariant-transition/invalid";
const diffDir = "tests/fixtures/invariant-transition/diff";

const ajv = new Ajv2020({ strict: true, allErrors: true });
const schema = JSON.parse(
  readFileSync(
    resolve(root, "contracts/invariant-transition.schema.v1.0.0.json"),
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
// (reference resolution, kind coherence, state-graph rules, mapping
// evidence coherence, predicate depth) are caught by the typed
// normalizer (crates/lekalo-core). Semantic-only vectors may satisfy
// the schema; every other invalid vector must fail Ajv itself.
const SEMANTIC_ONLY_DETAILS = new Set([
  "duplicate-state-space",
  "duplicate-state",
  "missing-initial-state",
  "unknown-state",
  "unknown-state-space",
  "duplicate-invariant",
  "field-value-shape",
  "cross-field-shape",
  "uniqueness-shape",
  "cardinality-shape",
  "temporal-shape",
  "aggregate-shape",
  "conditional-shape",
  "one-active-shape",
  "member-of-set-shape",
  "immutable-after-state-shape",
  "target-capability-shape",
  "cross-entity-field",
  "unknown-from-state",
  "unknown-to-state",
  "duplicate-assignment",
  "missing-error-ref",
  "duplicate-transition",
  "transition-from-terminal",
  "prior-read-after-write",
  "unknown-subject",
  "duplicate-mapping",
  "forbidden-cycle",
  "unreachable-state",
  "dead-nonterminal-state",
  "status-without-evidence",
  "gap-with-evidence",
  "stale-evidence",
  "predicate-depth",
]);

// --- raw duplicate-key scan (a parsed value cannot see duplicates) ---
//
// Character-level scan: a key is a quoted string followed (after
// optional whitespace) by a colon; key sets are scoped to the
// innermost open object, so sibling array elements never collide.
function duplicateKeys(text) {
  const duplicates = new Set();
  const stack = [new Set()];
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
      const content = text.slice(index + 1, end);
      let probe = end + 1;
      while (probe < text.length && /\s/.test(text[probe])) probe += 1;
      if (text[probe] === ":") {
        const keys = stack[stack.length - 1];
        if (keys.has(content)) duplicates.add(content);
        keys.add(content);
      }
      index = end + 1;
      continue;
    }
    if (character === "{") stack.push(new Set());
    else if (character === "}") stack.pop();
    index += 1;
  }
  return [...duplicates];
}

// --- canonical form: sorted object keys, compact separators ---
function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value && typeof value === "object") {
    const keys = Object.keys(value).sort((a, b) =>
      Buffer.compare(Buffer.from(a, "utf8"), Buffer.from(b, "utf8")),
    );
    return `{${keys
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
}

let goldenCount = 0;
for (const name of readdirSync(resolve(root, validDir)).sort()) {
  if (!name.endsWith(".json")) continue;
  const path = `${validDir}/${name}`;
  const text = readFileSync(resolve(root, validDir, name), "utf8");
  const dupe = duplicateKeys(text);
  if (dupe.length > 0) {
    fail(`${path}:duplicate-keys`, dupe.join(","));
    continue;
  }
  const value = JSON.parse(text);
  if (!validateAttachment(value)) {
    fail(path, JSON.stringify(validateAttachment.errors));
    continue;
  }
  const canonical = canonicalJson(value);
  if (canonical !== text) {
    fail(
      `${path}:canonical-form`,
      "committed bytes are not the canonical form",
    );
    continue;
  }
  goldenCount += 1;
}

if (goldenCount === 0) failEarly("no-goldens", "the valid fixture directory is empty");

// diff vectors: shape only; the permutation vector deliberately keeps
// non-canonical array order (its canonical collapse is proven by the
// Rust suite).
let diffCount = 0;
for (const name of readdirSync(resolve(root, diffDir)).sort()) {
  if (!name.endsWith(".json")) continue;
  const path = `${diffDir}/${name}`;
  const text = readFileSync(resolve(root, diffDir, name), "utf8");
  const value = JSON.parse(text);
  if (!validateAttachment(value)) {
    fail(path, JSON.stringify(validateAttachment.errors));
    continue;
  }
  diffCount += 1;
}
if (diffCount === 0) failEarly("no-diff-vectors", "the diff fixture directory is empty");

let invalidCount = 0;
for (const name of readdirSync(resolve(root, invalidDir)).sort()) {
  if (!name.endsWith(".json") || name.endsWith(".expect.json")) continue;
  const path = `${invalidDir}/${name}`;
  const text = readFileSync(resolve(root, invalidDir, name), "utf8");
  const dupe = duplicateKeys(text);
  const expectName = name.replace(/\.json$/, ".expect.json");
  let expect;
  try {
    expect = JSON.parse(readFileSync(resolve(root, invalidDir, expectName), "utf8"));
  } catch {
    fail(`${path}:missing-expectation`, expectName);
    continue;
  }
  const rule = expect.rule;
  const detail = expect.detail;
  if (rule === "raw-duplicate-key") {
    if (dupe.length === 0) fail(`${path}:expected-duplicate-key`, name);
    invalidCount += 1;
    continue;
  }
  if (dupe.length > 0) {
    fail(`${path}:duplicate-keys`, dupe.join(","));
    continue;
  }
  const value = JSON.parse(text);
  const semanticOnly = SEMANTIC_ONLY_DETAILS.has(detail);
  if (validateAttachment(value)) {
    if (!semanticOnly) {
      fail(`${path}:expected-schema-rejection`, `detail=${detail}`);
    }
    invalidCount += 1;
    continue;
  }
  invalidCount += 1;
}

if (invalidCount === 0) failEarly("no-invalid-vectors", "the invalid fixture directory is empty");

if (failures.length > 0) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, gate: "invariant-transition", failures })}\n`,
  );
  process.exit(1);
}
process.stdout.write(
  `${JSON.stringify({
    ok: true,
    gate: "invariant-transition",
    goldens: goldenCount,
    diffVectors: diffCount,
    invalidVectors: invalidCount,
    schema: "contracts/invariant-transition.schema.v1.0.0.json",
    ajv: ajvVersion,
  })}\n`,
);
