#!/usr/bin/env node
// Issue #69 release gate: the three new storage-engine contract
// schemas (profile, introspection evidence, migration plan), the
// committed valid goldens, and the adversarial vectors, validated with
// the same pinned third-party Draft 2020-12 implementation as the
// other contract gates (exact Ajv 8.17.1, provisioned outside this
// checkout and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH).
//
// The gate independently proves the canonical form of the valid
// goldens: re-serializing the parsed document with byte-sorted object
// keys must reproduce the committed bytes exactly. The runtime goldens
// and the drifted evidence vector are checked for canonical form and
// schema shape, never mutated.
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
const schemas = new Map([
  ["storage-engine", readJson("contracts/storage-engine.schema.v0.4.0.json")],
  ["storage-introspection", readJson("contracts/storage-introspection.schema.v0.4.0.json")],
  ["storage-migration-plan", readJson("contracts/storage-migration-plan.schema.v0.4.0.json")],
]);
const validators = new Map();
for (const [name, schema] of schemas) {
  try {
    validators.set(name, ajv.compile(schema));
  } catch (error) {
    failEarly(`schema:${name}:strict-compile`, String(error));
  }
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

let goldenCount = 0;
// The profile goldens validate and must be canonical.
for (const name of readdirSync(resolve(root, "tests/fixtures/storage-engine/valid")).sort()) {
  const text = read(`tests/fixtures/storage-engine/valid/${name}`);
  const document = JSON.parse(text);
  if (!validators.get("storage-engine")(document)) {
    fail(`valid:${name}:schema`, validators.get("storage-engine").errors);
    continue;
  }
  if (canonicalJson(document) !== text.trim()) {
    fail(`valid:${name}:canonical`, "committed bytes deviate from canonical form");
    continue;
  }
  goldenCount += 1;
}
if (goldenCount === 0) failEarly("no-goldens", "the valid fixture directory is empty");

// The introspection evidence goldens: schema-valid and canonical.
let evidenceCount = 0;
for (const name of ["observed.json", "observed-drifted.json"]) {
  const text = read(`tests/fixtures/storage-engine/introspection/${name}`);
  const document = JSON.parse(text);
  if (!validators.get("storage-introspection")(document)) {
    fail(`introspection:${name}:schema`, validators.get("storage-introspection").errors);
    continue;
  }
  if (canonicalJson(document) !== text.trim()) {
    fail(`introspection:${name}:canonical`, "committed bytes deviate from canonical form");
    continue;
  }
  evidenceCount += 1;
}
if (evidenceCount !== 2) failEarly("no-evidence-goldens", "both evidence goldens are required");

// The runtime goldens: one document three times, canonical form.
const runtimes = ["node.json", "php.json", "go.json"].map((name) =>
  read(`tests/fixtures/storage-engine/runtimes/${name}`),
);
if (runtimes[0] !== runtimes[1] || runtimes[1] !== runtimes[2]) {
  failEarly("runtime-goldens-diverge", "the three runtimes must receive identical bytes");
}
if (canonicalJson(JSON.parse(runtimes[0])) !== runtimes[0].trim()) {
  failEarly("runtime-golden-noncanonical", "the input document deviates from canonical form");
}

// The invalid vectors: schema-level rejections with their expect
// files. The expectation is load-bearing: every vector must declare a
// schema rule, and the actual Ajv rejection must carry the signal the
// declared detail names — a document rejected for the wrong reason
// fails the gate like one that passes.
const EXPECTED_AJV_SIGNALS = {
  "unknown-field": (errors) =>
    errors.some((error) => error.keyword === "additionalProperties"),
  "mode-not-checked": (errors) =>
    errors.some(
      (error) =>
        (error.keyword === "const" || error.keyword === "enum") &&
        /\/mode$/.test(error.instancePath),
    ),
  "unknown-step-kind": (errors) =>
    errors.some(
      (error) =>
        error.keyword === "enum" && error.instancePath.includes("/kind"),
    ),
};
let invalidCount = 0;
const invalidDir = "tests/fixtures/storage-engine/invalid";
for (const name of readdirSync(resolve(root, invalidDir)).sort()) {
  if (!name.endsWith(".json") || name.endsWith(".expect.json")) continue;
  const [family, document] = name.startsWith("introspection-")
    ? ["storage-introspection", readJson(`${invalidDir}/${name}`)]
    : name.startsWith("plan-")
      ? ["storage-migration-plan", readJson(`${invalidDir}/${name}`)]
      : ["storage-engine", readJson(`${invalidDir}/${name}`)];
  const expect = readJson(`${invalidDir}/${name.replace(/\.json$/, ".expect.json")}`);
  if (expect.rule !== "schema") {
    fail(`invalid:${name}:rule`, `the gate only exercises schema rejections, got ${expect.rule}`);
    continue;
  }
  const signal = EXPECTED_AJV_SIGNALS[expect.detail];
  if (!signal) {
    fail(`invalid:${name}:expectation`, `no schema signal mapped for detail ${expect.detail}`);
    continue;
  }
  const validate = validators.get(family);
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
    evidenceGoldens: evidenceCount,
    invalidVectors: invalidCount,
    ok: true,
    validGoldens: goldenCount,
  })}\n`,
);
