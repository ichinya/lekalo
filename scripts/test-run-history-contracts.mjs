#!/usr/bin/env node
// Issue #121 release gate: the four closed run-history wire schemas,
// the committed valid goldens, and the adversarial vectors, validated
// with the same pinned third-party Draft 2020-12 implementation as the
// other contract gates. Exact Ajv 8.17.1 is provisioned outside this
// checkout (CI does the same) and exposed through NODE_PATH /
// LEKALO_AJV_NODE_PATH.
//
// The gate independently proves the canonical form of every golden:
// re-serializing the parsed document with byte-sorted object keys and
// one trailing LF must reproduce the committed bytes exactly (the Rust
// canonical writer is verified against the same rule in
// crates/lekalo-core/src/run_history/tests.rs). A raw-text scan
// additionally rejects duplicate JSON keys, which a parsed-value
// representation cannot see.

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
const schemaDir = resolve(root, "contracts");
const fixtureDir = resolve(root, "tests/fixtures/run-history");

const fail = (caseName, detail) => {
  process.stderr.write(
    `${JSON.stringify({ ok: false, case: caseName, detail }, null, 2)}\n`,
  );
  process.exit(1);
};

// --- schemas compile under strict Ajv ------------------------------------
const schemas = new Map();
for (const name of [
  "run-record.schema.v0.4.0.json",
  "run-assertions.schema.v0.4.0.json",
  "run-history-store.schema.v0.4.0.json",
  "run-observation.schema.v0.4.0.json",
]) {
  const schema = JSON.parse(readFileSync(join(schemaDir, name), "utf8"));
  const ajv = new Ajv2020({ strict: true, allErrors: true });
  try {
    schemas.set(name, ajv.compile(schema));
  } catch (error) {
    fail(`schema:${name}`, String(error));
  }
}

// The schema discriminator selects the validating schema.
const DISCRIMINATORS = [
  ["lekalo/run-record/v0.4.0", "run-record.schema.v0.4.0.json"],
  ["lekalo/run-assertions/v0.4.0", "run-assertions.schema.v0.4.0.json"],
  ["lekalo/run-history-store/v0.4.0", "run-history-store.schema.v0.4.0.json"],
  ["lekalo/run-observation/v0.4.0", "run-observation.schema.v0.4.0.json"],
];

// --- raw duplicate-key scan (a parsed value cannot see duplicates) -------
function duplicateKeys(text) {
  const duplicates = [];
  const stack = [];
  let index = 0;
  const inString = () => stack[stack.length - 1] === "string";
  while (index < text.length) {
    const character = text[index];
    if (inString()) {
      if (character === "\\") {
        index += 2;
        continue;
      }
      if (character === '"') stack.pop();
      index += 1;
      continue;
    }
    if (character === '"') {
      stack.push("string");
      index += 1;
      continue;
    }
    if (character === "{") {
      stack.push({ keys: new Map() });
      index += 1;
      continue;
    }
    if (character === "}") {
      const frame = stack.pop();
      if (frame && typeof frame === "object") {
        for (const [key, count] of frame.keys) {
          if (count > 1) duplicates.push(key);
        }
      }
      index += 1;
      continue;
    }
    if (character === ":") {
      // The most recent string on the stack top frame is a key.
      const frame = stack[stack.length - 1];
      if (frame && typeof frame === "object") {
        const text2 = text.slice(0, index);
        const match = text2.match(/"((?:[^"\\]|\\.)*)"\s*$/);
        if (match) {
          frame.keys.set(match[1], (frame.keys.get(match[1]) ?? 0) + 1);
        }
      }
      index += 1;
      continue;
    }
    index += 1;
  }
  return duplicates;
}

// Canonical form: parse, byte-sort keys recursively, compact, trailing LF.
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") {
    const keys = Object.keys(value).sort((a, b) => {
      const left = Array.from(a);
      const right = Array.from(b);
      for (let index = 0; index < Math.min(left.length, right.length); index++) {
        const delta = left[index].codePointAt(0) - right[index].codePointAt(0);
        if (delta !== 0) return delta;
      }
      return left.length - right.length;
    });
    return `{${keys.map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

// --- valid goldens --------------------------------------------------------
let validCount = 0;
for (const name of readdirSync(join(fixtureDir, "valid")).sort()) {
  const path = join(fixtureDir, "valid", name);
  const bytes = readFileSync(path, "utf8");
  if (duplicateKeys(bytes).length > 0) {
    fail(`valid:${name}`, "duplicate JSON keys");
  }
  if (!bytes.endsWith("\n") || bytes.endsWith("\n\n")) {
    fail(`valid:${name}`, "exactly one trailing LF expected");
  }
  const value = JSON.parse(bytes);
  if (canonical(value) + "\n" !== bytes) {
    fail(`valid:${name}`, "bytes are not the canonical form");
  }
  const discriminator = value.schema_version;
  const schemaName = DISCRIMINATORS.find(([key]) => key === discriminator)?.[1];
  if (!schemaName) {
    fail(`valid:${name}`, `unknown schema_version ${discriminator}`);
  }
  const validate = schemas.get(schemaName);
  if (!validate(value)) {
    fail(`valid:${name}`, JSON.stringify(validate.errors, null, 1));
  }
  validCount += 1;
}

// --- invalid vectors ------------------------------------------------------
let invalidCount = 0;
for (const name of readdirSync(join(fixtureDir, "invalid")).sort()) {
  const path = join(fixtureDir, "invalid", name);
  const bytes = readFileSync(path, "utf8");
  if (name === "duplicate-key.json") {
    if (duplicateKeys(bytes).length === 0) {
      fail(`invalid:${name}`, "the duplicate key was not detected");
    }
    invalidCount += 1;
    continue;
  }
  const value = JSON.parse(bytes);
  const discriminator = value.schema_version;
  const schemaName = DISCRIMINATORS.find(([key]) => key === discriminator)?.[1];
  if (!schemaName) {
    fail(`invalid:${name}`, `unknown schema_version ${discriminator}`);
  }
  const validate = schemas.get(schemaName);
  if (validate(value)) {
    fail(`invalid:${name}`, "the vector must fail its schema");
  }
  invalidCount += 1;
}

// --- the closed custody labels inside every valid record ------------------
for (const name of ["greenfield.json", "brownfield.json"]) {
  const record = JSON.parse(readFileSync(join(fixtureDir, "valid", name), "utf8"));
  if (record.privacy.exportDisposition !== "local-private") {
    fail(`custody:${name}`, "exportDisposition must be local-private");
  }
  if (record.privacy.exportEligibility !== "ineligible") {
    fail(`custody:${name}`, "exportEligibility must be ineligible");
  }
  if (record.privacy.policyRef.version !== "0.3.2") {
    fail(`custody:${name}`, "records must reference the accepted #120 policy version");
  }
}

process.stdout.write(
  `${JSON.stringify({
    ok: true,
    schemas: schemas.size,
    valid: validCount,
    invalid: invalidCount,
  })}\n`,
);
