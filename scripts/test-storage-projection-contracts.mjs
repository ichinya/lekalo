#!/usr/bin/env node
// Issue #65 release gate: the storage-projection wire schema, the
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
// crates/lekalo-core/tests/storage_projection.rs). A raw-text scan
// additionally rejects duplicate JSON keys, which a parsed-value
// representation cannot see. Diff vectors live outside the valid
// directory on purpose: the permutation vector commits non-canonical
// array order, so only its schema shape is checked here while its
// canonical collapse is proven by the Rust suite. The derived
// projection goldens are checked for canonical form so the committed
// renderings of the two namespaces cannot drift silently.

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

const failures = [];
const fail = (caseName, detail) => failures.push({ case: caseName, detail });
function failEarly(reason, detail) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`,
  );
  process.exit(1);
}
const validDir = "tests/fixtures/storage-projection/valid";
const invalidDir = "tests/fixtures/storage-projection/invalid";
const diffDir = "tests/fixtures/storage-projection/diff";
const derivedDir = "tests/fixtures/storage-projection/derived";

const ajv = new Ajv2020({ strict: true, allErrors: true });
const schema = JSON.parse(
  readFileSync(
    resolve(root, "contracts/storage-projection.schema.v1.0.0.json"),
    "utf8",
  ),
);
let validateAttachment;
try {
  validateAttachment = ajv.compile(schema);
} catch (error) {
  failEarly("schema:strict-compile", String(error));
}

// Wire violations are caught by the schema; semantic violations
// (reference resolution, aggregate coherence, kind/cardinality/
// delete-behavior coherence, coverage, projection-to-domain mapping)
// are caught by the typed normalizer (crates/lekalo-core). Semantic-
// only vectors may satisfy the schema; every other invalid vector must
// fail Ajv itself.
const SEMANTIC_ONLY_DETAILS = new Set([
  "storage-type",
  "type-params",
  "decimal-scale-above-precision",
  "duplicate-entity-key",
  "duplicate-field",
  "duplicate-namespace",
  "duplicate-relation-id",
  "foreign-key-forbidden",
  "aggregate-both",
  "aggregate-child-delete-behavior",
  "aggregate-child-ownership",
  "aggregate-owner-missing",
  "aggregate-owner-not-root",
  "column-collision",
  "coverage-missing",
  "detach-minimum",
  "duplicate-table-name",
  "external-aggregate",
  "external-mapped",
  "external-target-required",
  "external-delete-behavior",
  "entity-absent",
  "entity-unmapped",
  "foreign-key-required",
  "join-entity-unmapped",
  "join-kind",
  "join-materialization-missing",
  "join-unknown-relation",
  "migration-unknown-table",
  "min-above-max",
  "one-to-many-max",
  "one-to-one-max",
  "optional-reference-delete",
  "optional-reference-min",
  "polymorphic-kind",
  "polymorphic-materialization-missing",
  "polymorphic-owner-unmapped",
  "unknown-index-column",
  "unknown-primary-key",
]);

// --- raw duplicate-key scan (a parsed value cannot see duplicates) ---

// Character-level scan: a key is a quoted string followed (after
// optional whitespace) by a colon; key sets are scoped to the
// innermost open object, so sibling array elements never collide.
function duplicateKeys(text) {
  const stack = [new Set()];
  const keyPattern = /"((?:[^"\\]|\\.)*)"\s*:/g;
  for (let index = 0; index < text.length; index += 1) {
    const character = text[index];
    if (character === "{") {
      stack.push(new Set());
    } else if (character === "}") {
      if (stack.length > 1) stack.pop();
    } else if (character === '"') {
      keyPattern.lastIndex = index;
      const match = keyPattern.exec(text);
      if (match && match.index === index) {
        const keys = stack[stack.length - 1];
        if (keys.has(match[1])) return match[1];
        keys.add(match[1]);
        index = keyPattern.lastIndex - 1;
      }
    }
  }
  return null;
}

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

const read = (relative) => readFileSync(resolve(root, relative), "utf8");

let goldenCount = 0;
for (const name of readdirSync(resolve(root, validDir)).sort()) {
  const text = read(`${validDir}/${name}`);
  const duplicate = duplicateKeys(text);
  if (duplicate) {
    fail(`valid:${name}:duplicate-key`, duplicate);
    continue;
  }
  let document;
  try {
    document = JSON.parse(text);
  } catch (error) {
    fail(`valid:${name}:parse`, String(error));
    continue;
  }
  if (!validateAttachment(document)) {
    fail(`valid:${name}:schema`, validateAttachment.errors);
    continue;
  }
  const canonical = canonicalJson(document);
  if (canonical !== text.trim()) {
    fail(`valid:${name}:canonical`, "committed bytes deviate from canonical form");
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
  const document = JSON.parse(read(`${diffDir}/${name}`));
  if (!validateAttachment(document)) {
    fail(`diff:${name}:schema`, validateAttachment.errors);
    continue;
  }
  diffCount += 1;
}
if (diffCount === 0) failEarly("no-diff-vectors", "the diff fixture directory is empty");

// Derived projection goldens: canonical form of the two committed
// namespace renderings. Their schema belongs to the derivation, not to
// this attachment wire, so only the canonical-form rule is checked.
let derivedCount = 0;
for (const name of readdirSync(resolve(root, derivedDir)).sort()) {
  const text = read(`${derivedDir}/${name}`);
  let document;
  try {
    document = JSON.parse(text);
  } catch (error) {
    fail(`derived:${name}:parse`, String(error));
    continue;
  }
  if (canonicalJson(document) !== text.trim()) {
    fail(`derived:${name}:canonical`, "committed bytes deviate from canonical form");
    continue;
  }
  derivedCount += 1;
}
if (derivedCount !== 2) failEarly("no-derived-goldens", "both namespace goldens are required");

let invalidCount = 0;
for (const name of readdirSync(resolve(root, invalidDir)).sort()) {
  if (!name.endsWith(".json") || name.endsWith(".expect.json")) continue;
  const text = read(`${invalidDir}/${name}`);
  const duplicate = duplicateKeys(text);
  const expect = JSON.parse(read(`${invalidDir}/${name.replace(/\.json$/, ".expect.json")}`));
  if (expect.rule === "raw-duplicate-key") {
    if (!duplicate) fail(`invalid:${name}:expected-duplicate-key`, "the raw scan found nothing");
    invalidCount += 1;
    continue;
  }
  if (duplicate) {
    fail(`invalid:${name}:unexpected-duplicate-key`, duplicate);
    continue;
  }
  const document = JSON.parse(text);
  if (!validateAttachment(document)) {
    invalidCount += 1;
    continue;
  }
  if (!SEMANTIC_ONLY_DETAILS.has(expect.detail)) {
    fail(`invalid:${name}:schema-passed`, expect.detail);
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
    derivedGoldens: derivedCount,
    diffVectors: diffCount,
    invalidVectors: invalidCount,
    ok: true,
    validGoldens: goldenCount,
  })}\n`,
);
