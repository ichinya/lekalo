#!/usr/bin/env node
// Issue #20 release gate: the cache wire schema, the pinned golden record
// fixtures, and the cross-language canonical digest vectors, validated with
// the same pinned third-party Draft 2020-12 implementation as the other
// contract gates. Exact Ajv 8.17.1 is provisioned outside this checkout
// (CI does the same on Node 18 and 24) and exposed through NODE_PATH /
// LEKALO_AJV_NODE_PATH.

import { createRequire } from "node:module";
import { readdirSync, readFileSync } from "node:fs";
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
  console.error(`ajv is not provisioned: ${error.message}`);
  process.exit(1);
}
if (ajvVersion !== "8.17.1") {
  console.error(`pinned Ajv 8.17.1 is required, found ${ajvVersion}`);
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));

const schema = read("contracts/cache.schema.v1.0.0.json");
const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateWire = ajv.compile(schema);

const fail = (reason, detail) => {
  console.error(`FAIL ${reason}${detail ? `: ${detail}` : ""}`);
  process.exit(1);
};

// Canonical bytes: compact UTF-8 JSON with byte-sorted object keys, the
// exact spelling the Rust canonicalizer emits (serde_json map order).
const canonical = (value) => {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value !== null && typeof value === "object") {
    const keys = Object.keys(value).sort();
    return `{${keys.map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
};
const sha256 = (text) =>
  `sha256:${createHash("sha256").update(text, "utf8").digest("hex")}`;

const RECORD_KINDS = [
  "source",
  "parsed-fragment",
  "ir-fragment",
  "graph-fragment",
  "effect-fragment",
  "adapter-capability",
  "adapter-result",
  "artifact-manifest-key",
  "context-key",
];
const HEALTH_STATES = [
  "ok",
  "empty",
  "missing",
  "disabled",
  "corrupt",
  "quarantined",
  "locked",
  "unreadable",
];

let recordCount = 0;
let invalidCount = 0;
let healthCount = 0;

// 1. Every valid golden record validates against the closed schema, its
//    pinned digests match an independent canonical re-derivation, and no
//    forbidden free-form data (source text, hosts, timestamps) appears.
for (const name of readdirSync(resolve(root, "tests/fixtures/cache/records")).sort()) {
  const document = read(`tests/fixtures/cache/records/${name}`);
  if (!validateWire(document)) {
    fail(`valid record fixture rejected: ${name}`, JSON.stringify(validateWire.errors));
  }
  const record = document;
  if (record.keyDigest !== sha256(canonical(record.key))) {
    fail(`key digest pin does not match the canonical re-derivation: ${name}`);
  }
  if (record.payloadDigest !== sha256(canonical(record.payload.value))) {
    fail(`payload digest pin does not match the canonical re-derivation: ${name}`);
  }
  if (!RECORD_KINDS.includes(record.recordKind)) fail(`unknown kind: ${name}`);
  const wire = JSON.stringify(document);
  for (const forbidden of ["C:\\", "/home/", "file://", "createdAt", "mtime", "timestamp"]) {
    if (wire.includes(forbidden)) fail(`forbidden wire content (${forbidden}): ${name}`);
  }
  recordCount += 1;
}

// 2. Every invalid vector is rejected by the closed schema.
for (const name of readdirSync(resolve(root, "tests/fixtures/cache/invalid")).sort()) {
  const document = read(`tests/fixtures/cache/invalid/${name}`);
  if (validateWire(document)) {
    fail(`invalid fixture was accepted: ${name}`);
  }
  invalidCount += 1;
}

// 3. The health fixture is a valid closed projection.
for (const name of readdirSync(resolve(root, "tests/fixtures/cache/health")).sort()) {
  const document = read(`tests/fixtures/cache/health/${name}`);
  if (!validateWire(document)) {
    fail(`health fixture rejected: ${name}`, JSON.stringify(validateWire.errors));
  }
  if (!HEALTH_STATES.includes(document.state)) fail(`unknown health state: ${name}`);
  healthCount += 1;
}

if (recordCount < 5) fail("the five v1 record kinds must be pinned");
if (invalidCount === 0) fail("no invalid vectors were checked");
if (healthCount === 0) fail("no health vector was checked");

process.stdout.write(`${JSON.stringify({
  gate: "lekalo-cache-contracts",
  schema: "lekalo/cache/v1.0.0",
  ajv: ajvVersion,
  validRecords: recordCount,
  rejectedVectors: invalidCount,
  healthProjections: healthCount,
}, null, 2)}\n`);
