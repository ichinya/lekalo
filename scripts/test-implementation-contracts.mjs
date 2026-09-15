#!/usr/bin/env node
// Issue #30 release gate: the implementation attachment schema and its
// fixture corpus, validated with the same pinned third-party Draft
// 2020-12 implementation as the other contract gates. Exact Ajv 8.17.1
// is provisioned outside this checkout (CI does the same on Node 18 and
// 24) and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.
//
// The semantic/ fixtures bind to the compiled IR of the accepted
// full-kinds fixture and are driven by the Rust integration suite
// (crates/lekalo-core/tests/implementation.rs); this gate checks that
// every semantic document is schema-valid and carries an expectation.

import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  const nodePath = process.env.LEKALO_AJV_NODE_PATH ?? "";
  if (nodePath !== "") {
    const req = createRequire(nodePath + "/");
    Ajv2020 = req("ajv/dist/2020.js").default;
    ajvVersion = req("ajv/package.json").version;
  } else {
    Ajv2020 = require("ajv/dist/2020").default;
    ajvVersion = require("ajv/package.json").version;
  }
} catch (error) {
  fail("ajv-missing", "pinned Ajv 8.17.1 must be provided through NODE_PATH/LEKALO_AJV_NODE_PATH");
}

if (ajvVersion !== "8.17.1") {
  fail("ajv-version", ajvVersion);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));

const schema = read("contracts/implementation.schema.v1.0.0.json");
const ajv = new Ajv2020({ strict: true, allErrors: true });
const validate = ajv.compile(schema);

const validDir = "tests/fixtures/implementation/valid";
const invalidDir = "tests/fixtures/implementation/invalid";
const semanticDir = "tests/fixtures/implementation/semantic";

const { readdirSync } = await import("node:fs");
const list = (dir) =>
  readdirSync(resolve(root, dir))
    .filter((name) => name.endsWith(".json"))
    .sort();

const failures = [];
for (const name of list(validDir)) {
  const document = read(`${validDir}/${name}`);
  if (!validate(document)) {
    failures.push({ fixture: `${validDir}/${name}`, errors: validate.errors });
  }
}
for (const name of list(invalidDir)) {
  const document = read(`${invalidDir}/${name}`);
  if (validate(document)) {
    failures.push({ fixture: `${invalidDir}/${name}`, error: "schema accepted the document" });
  }
}
let semanticChecked = 0;
for (const name of list(semanticDir)) {
  if (name.endsWith(".expect.json")) continue;
  const document = read(`${semanticDir}/${name}`);
  if (!validate(document)) {
    failures.push({ fixture: `${semanticDir}/${name}`, errors: validate.errors });
  }
  const expectation = read(`${semanticDir}/${name.replace(/\.json$/, ".expect.json")}`);
  if (
    (expectation.status !== "valid" && expectation.status !== "invalid") ||
    !Array.isArray(expectation.reasonCodes) ||
    (expectation.status === "invalid" && expectation.reasonCodes.length === 0)
  ) {
    failures.push({ fixture: `${semanticDir}/${name}`, error: "malformed expectation" });
  }
  semanticChecked += 1;
}

// The two embedded valid fixtures must stay canonical: hook contracts
// sorted by (symbol, contract), targets sorted by target, digests
// lowercase sha256 spellings.
for (const name of list(validDir)) {
  const document = read(`${validDir}/${name}`);
  const symbols = document.contracts.map((contract) => contract.symbol + "|" + contract.contract);
  if (JSON.stringify(symbols) !== JSON.stringify([...symbols].sort())) {
    failures.push({ fixture: `${validDir}/${name}`, error: "contracts not sorted" });
  }
  for (const contract of document.contracts) {
    const targets = contract.targets.map((binding) => binding.target);
    if (JSON.stringify(targets) !== JSON.stringify([...targets].sort())) {
      failures.push({ fixture: `${validDir}/${name}`, error: "targets not sorted" });
    }
  }
}

if (failures.length !== 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, failures }, null, 2)}\n`);
  process.exit(1);
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  ajv: ajvVersion,
  valid: list(validDir).length,
  invalid: list(invalidDir).length,
  semantic: semanticChecked,
}, null, 2)}\n`);
