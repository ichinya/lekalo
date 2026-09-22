#!/usr/bin/env node
// Issue #117 release gate: the storage-engine-profile wire schema, the
// committed valid goldens, and the adversarial vectors, validated with
// the same pinned third-party Draft 2020-12 implementation as the
// other contract gates. Exact Ajv 8.17.1 is provisioned outside this
// checkout (CI does the same on Node 18 and 24) and exposed through
// NODE_PATH / LEKALO_AJV_NODE_PATH.
//
// The gate independently proves the canonical form of both valid
// goldens: re-serializing the parsed document with byte-sorted object
// keys must reproduce the committed bytes exactly (the Rust canonical
// writer is verified against the same rule in
// crates/lekalo-core/tests/storage_engine_profile.rs). Semantic
// violations (engine/variant coherence, collation/charset membership)
// are caught by the typed normalizer; every invalid vector must fail
// Ajv itself because the schema expresses the closed member sets, the
// exact-version grammar, and the partial-requires-bounds rule.

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
const validDir = "tests/fixtures/storage-engine-profile/valid";
const invalidDir = "tests/fixtures/storage-engine-profile/invalid";

const ajv = new Ajv2020({ strict: true, allErrors: true });
const schema = JSON.parse(
  readFileSync(
    resolve(root, "contracts/storage-engine-profile.schema.v0.4.0.json"),
    "utf8",
  ),
);
let validateProfile;
try {
  validateProfile = ajv.compile(schema);
} catch (error) {
  failEarly("schema:strict-compile", String(error));
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
  let document;
  try {
    document = JSON.parse(text);
  } catch (error) {
    fail(`valid:${name}:parse`, String(error));
    continue;
  }
  if (!validateProfile(document)) {
    fail(`valid:${name}:schema`, validateProfile.errors);
    continue;
  }
  const canonical = canonicalJson(document);
  if (canonical !== text.trim()) {
    fail(`valid:${name}:canonical`, "committed bytes deviate from canonical form");
    continue;
  }
  // The two goldens are separate engine profiles, never one family.
  if (document.engine.engine === "mysql-family") fail(`valid:${name}:engine`, "merged claim");
  goldenCount += 1;
}
if (goldenCount !== 2) failEarly("no-goldens", "both engine goldens are required");

// The semantic-only details the schema cannot express: the
// collation/charset membership and the engine/variant coherence are
// proven by the typed normalizer and the Rust suite.
const SEMANTIC_ONLY_DETAILS = new Set(["collation-charset-mismatch"]);
let invalidCount = 0;
for (const name of readdirSync(resolve(root, invalidDir)).sort()) {
  if (!name.endsWith(".json") || name.endsWith(".expect.json")) continue;
  const document = JSON.parse(read(`${invalidDir}/${name}`));
  const expect = JSON.parse(
    read(`${invalidDir}/${name.replace(/\.json$/, ".expect.json")}`),
  );
  if (validateProfile(document)) {
    if (SEMANTIC_ONLY_DETAILS.has(expect.detail)) {
      invalidCount += 1;
      continue;
    }
    fail(`invalid:${name}:schema-passed`, "the schema accepted a refusal vector");
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
    invalidVectors: invalidCount,
    ok: true,
    validGoldens: goldenCount,
  })}\n`,
);
