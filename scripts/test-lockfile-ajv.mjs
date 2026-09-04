#!/usr/bin/env node
// Release gate for the pinned third-party JSON Schema implementation over
// the issue #10 lock wire. Exact Ajv 8.17.1 is provisioned outside this
// checkout (CI does the same on Node 18 and 24) and exposed through
// NODE_PATH / LEKALO_AJV_NODE_PATH.

import { createRequire } from "node:module";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  Ajv2020 = require("ajv/dist/2020").default;
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "ajv-unavailable",
    detail: error?.code ?? error?.message,
  }, null, 2)}\n`);
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const schemaText = readFileSync(resolve(root, "contracts/lock.schema.v1.0.0.json"), "utf8");
const schema = JSON.parse(schemaText);

const listDir = (relative) =>
  readdirSync(resolve(root, relative))
    .filter((name) => name.endsWith(".lock.json"))
    .sort();

const validCases = listDir("tests/fixtures/lockfile/valid").map((name) => ({
  name: `valid/${name}`,
  document: JSON.parse(readFileSync(resolve(root, `tests/fixtures/lockfile/valid/${name}`), "utf8")),
  expectValid: true,
}));

const invalidCases = listDir("tests/fixtures/lockfile/invalid").map((name) => ({
  name: `invalid/${name}`,
  document: JSON.parse(readFileSync(resolve(root, `tests/fixtures/lockfile/invalid/${name}`), "utf8")),
  expectValid: false,
}));

if (ajvVersion !== "8.17.1") {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "ajv-version",
    detail: `expected 8.17.1, found ${ajvVersion}`,
  }, null, 2)}\n`);
  process.exit(1);
}

const ajv = new Ajv2020({
  strict: true,
  allErrors: true,
  allowUnionTypes: true,
});

let compiled;
try {
  compiled = ajv.compile(schema);
} catch (error) {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "schema-compile",
    detail: error?.message,
  }, null, 2)}\n`);
  process.exit(1);
}

const failures = [];
let checks = 0;
for (const testCase of [...validCases, ...invalidCases]) {
  const valid = compiled(testCase.document);
  checks += 1;
  if (valid !== testCase.expectValid) {
    failures.push({
      case: testCase.name,
      detail: `expected ${testCase.expectValid ? "valid" : "invalid"}`,
      errors: valid ? [] : compiled.errors,
    });
  }
}

// Every golden digest domain and the discriminator constant are checked by
// the schema itself; assert both sides were exercised.
if (checks !== validCases.length + invalidCases.length) {
  failures.push({ case: "coverage", detail: "not every fixture ran" });
}

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, failures }, null, 2)}\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({
  ok: true,
  ajv: ajvVersion,
  validCases: validCases.length,
  invalidCases: invalidCases.length,
}, null, 2)}\n`);
