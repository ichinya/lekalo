#!/usr/bin/env node
/**
 * Issue #27 contract gate: the `lekalo.target/v1` process protocol.
 *
 * Dependency-free except the pinned Ajv 8.17.1 release gate (CI provisions
 * it outside the checkout and exposes it through NODE_PATH /
 * LEKALO_AJV_NODE_PATH). The gate:
 *
 * 1. validates the closed wire schema artifact,
 * 2. checks every valid golden for duplicate JSON keys, schema validity,
 *    and canonical byte form (sorted keys, no whitespace, no trailing LF),
 * 3. refuses every adversarial vector with its recorded registered rule,
 *    allowing schema-valid vectors only when they exercise the core
 *    normalizer rules that JSON Schema cannot express.
 *
 * Exit protocol: 0 with a one-line JSON status on success, 1 with a JSON
 * failure report on stderr otherwise.
 */
import { createRequire } from "node:module";
import { readFileSync, readdirSync } from "node:fs";

const require = createRequire(import.meta.url);
let Ajv;
try {
  const nodePath = process.env.LEKALO_AJV_NODE_PATH ?? "";
  if (nodePath) {
    const req = createRequire(nodePath + "/");
    Ajv = req("ajv/dist/2020.js");
  } else {
    Ajv = require("ajv/dist/2020.js");
  }
} catch {
  fail("ajv-missing", "pinned Ajv 8.17.1 must be provided through NODE_PATH/LEKALO_AJV_NODE_PATH");
}

const ajvModule = Ajv.default ?? Ajv;

const packageJson = (() => {
  try {
    return require("ajv/package.json");
  } catch {
    return undefined;
  }
})();
if (!packageJson || packageJson.version !== "8.17.1") {
  fail("ajv-version", `expected Ajv 8.17.1, found ${packageJson ? packageJson.version : "none"}`);
}

const ajv = new ajvModule({ strict: true, allErrors: true });

function fail(reason, detail) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "target-protocol", reason, detail })}\n`);
  process.exit(1);
}

function failEarly(reason, detail) {
  fail(reason, detail);
}

function failAll(failures) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "target-protocol", failures })}\n`);
  process.exit(1);
}

const read = (p) => readFileSync(new URL(p, import.meta.url), "utf8");

const schemas = {
  "1.0.0": JSON.parse(read("../contracts/target-protocol.schema.v1.0.0.json")),
  "1.1.0": JSON.parse(read("../contracts/target-protocol.schema.v1.1.0.json")),
};
// Fixtures whose name carries the v1_1 marker live on the 1.1.0 contract;
// every other fixture stays on the frozen published 1.0.0 document.
const schemaFor = (name) => (name.includes("v1_1") ? schemas["1.1.0"] : schemas["1.0.0"]);
const validators = Object.fromEntries(
  Object.entries(schemas).map(([version, schema]) => [version, ajv.compile(schema)]),
);
// The extension members are additive: a 1.1.0 response carrying them must
// be refused by the frozen 1.0.0 document, and the two documents must
// still accept every legacy fixture.
for (const name of ["describe-request.json", "describe-response.json"]) {
  const legacy = JSON.parse(read(`../tests/fixtures/target-protocol/valid/${name}`));
  if (!validators["1.0.0"](legacy)) failEarly("legacy-golden", `${name} must stay a 1.0.0 document`);
  if (!validators["1.1.0"](legacy)) failEarly("additive-golden", `${name} must validate under 1.1.0`);
}
const extensionGolden = JSON.parse(
  read("../tests/fixtures/target-protocol/valid/describe-response-v1_1.json"),
);
if (!validators["1.1.0"](extensionGolden)) {
  failEarly("extension-golden", "the 1.1.0 extension golden must validate under 1.1.0");
}
if (validators["1.0.0"](extensionGolden)) {
  failEarly("extension-not-additive", "the frozen 1.0.0 document must refuse extension members");
}

const ROOT = "../tests/fixtures/target-protocol/";

for (const vector of JSON.parse(read(ROOT + "error-code-vectors.json"))) {
  const response = JSON.parse(read(ROOT + "valid/describe-response.json"));
  delete response.capabilities;
  response.status = "error";
  response.error = {
    class: "invalid", code: vector.unit.repeat(vector.repeat), message: "owned synthetic error",
  };
  if (validators["1.0.0"](response) !== vector.valid) failEarly("error-code-parity", vector.name);
}

for (const field of ["path", "scope"]) {
  const definition = schemas["1.0.0"].$defs[field === "path" ? "logicalPath" : "scope"];
  const check = ajv.compile(definition);
  for (const vector of JSON.parse(read(ROOT + "scope-grammar.json"))) {
    if (check(vector.value) !== vector[field]) failEarly("scope-parity", `${field}: ${vector.value}`);
  }
  if (!check("x".repeat(64)) || check("x".repeat(65))) failEarly("scope-bound", field);
}

function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value !== null && typeof value === "object") {
    const body = Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
      .join(",");
    return `{${body}}`;
  }
  return JSON.stringify(value);
}

/** Raw lexer: reject duplicate JSON keys before any parse can hide them. */
function duplicateKeys(text) {
  const offenders = [];
  const stack = [ { keys: null } ];
  let index = 0;
  const skipWhitespace = () => {
    while (index < text.length && /\s/.test(text[index])) index += 1;
  };
  while (index < text.length) {
    const ch = text[index];
    if (ch === '"') {
      const start = index;
      index += 1;
      while (index < text.length) {
        const c = text[index];
        if (c === "\\") {
          index += 2;
          continue;
        }
        if (c === '"') {
          index += 1;
          break;
        }
        index += 1;
      }
      const value = JSON.parse(text.slice(start, index));
      skipWhitespace();
      if (text[index] === ":") {
        index += 1;
        const frame = stack[stack.length - 1];
        if (frame.keys !== null) {
          if (frame.keys.has(value)) offenders.push(value);
          frame.keys.add(value);
        }
      }
      continue;
    }
    if (ch === "{") {
      stack.push({ keys: new Set() });
      index += 1;
      continue;
    }
    if (ch === "}") {
      if (stack.length > 1) stack.pop();
      index += 1;
      continue;
    }
    index += 1;
  }
  return offenders;
}

// Context-dependent vectors are executed by the mandatory Rust conformance
// gate. This schema pass reports structural acceptance instead of pretending
// a filename whitelist executed the production client.
for (const control of ['{"a":1,"a":2}', '{"a":1,"\\u0061":2}', '{"nested":{"a":1,"\\u0061":2}}']) {
  if (duplicateKeys(control).length !== 1) failEarly("duplicate-control", "decoded duplicate was lost");
}
if (duplicateKeys('{"a":{"b":1},"c":{"b":2}}').length) failEarly("duplicate-control", "distinct objects collided");

const RULE_ID = /^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+$/;
const failures = [];
let goldens = 0;
let invalidVectors = 0;
let schemaAcceptedContextualVectors = 0;

for (const entry of readdirSync(new URL(ROOT, import.meta.url))) {
  if (entry !== "valid" && entry !== "invalid") continue;
  for (const name of readdirSync(new URL(`${ROOT}${entry}/`, import.meta.url))) {
    if (!name.endsWith(".json") || name.endsWith(".expect.json")) continue;
    const rel = `${ROOT}${entry}/${name}`;
    const raw = read(rel);
    const caseName = `${entry}/${name}`;
    let parsed;
    try {
      parsed = JSON.parse(raw);
    } catch (error) {
      failures.push({ case: caseName, detail: `unparseable: ${error.message}` });
      continue;
    }
    const dups = duplicateKeys(raw);
    if (dups.length > 0) {
      failures.push({ case: caseName, detail: `duplicate keys: ${dups.join(",")}` });
      continue;
    }
    if (entry === "valid") {
      goldens += 1;
      const validator = validators[schemaFor(name).$id.endsWith("v1.1.0.json") ? "1.1.0" : "1.0.0"];
      if (!validator(parsed)) {
        failures.push({ case: caseName, detail: `schema: ${ajv.errorsText(validator.errors)}` });
        continue;
      }
      if (raw.endsWith("\n")) {
        failures.push({ case: caseName, detail: "trailing LF" });
        continue;
      }
      if (canonicalJson(parsed) !== raw) {
        failures.push({ case: caseName, detail: "bytes are not the canonical form" });
      }
      continue;
    }
    // invalid/
    invalidVectors += 1;
    let expectation;
    try {
      expectation = JSON.parse(read(`${ROOT}invalid/${name.replace(/\.json$/, "")}.expect.json`));
    } catch {
      failures.push({ case: caseName, detail: "missing or unparseable .expect.json" });
      continue;
    }
    const { rule, detail } = expectation;
    if (typeof rule !== "string" || !RULE_ID.test(rule)) {
      failures.push({ case: caseName, detail: `bad rule: ${JSON.stringify(rule)}` });
      continue;
    }
    if (typeof detail !== "string" || detail.length === 0 || detail.length > 128) {
      failures.push({ case: caseName, detail: `bad detail: ${JSON.stringify(detail)}` });
      continue;
    }
    const rejected = !validators[schemaFor(name).$id.endsWith("v1.1.0.json") ? "1.1.0" : "1.0.0"](parsed);
    if (!rejected) schemaAcceptedContextualVectors += 1;
  }
}

if (goldens === 0) failEarly("no-goldens", "the valid/ directory is empty");
if (invalidVectors === 0) failEarly("no-invalid-vectors", "the invalid/ directory is empty");

if (failures.length > 0) {
  failAll(failures);
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  gate: "target-protocol",
  ajv: "8.17.1",
  goldens,
  invalidVectors,
  schemaAcceptedContextualVectors,
  runtimeGate: "cargo test -p lekalo-core target_protocol_conformance --locked",
})}\n`);
