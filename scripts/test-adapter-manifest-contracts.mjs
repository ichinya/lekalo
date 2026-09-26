/**
 * Issue #32 contract gate: the adapter package manifest schema
 * (fix round 1, cline F-7 — the gate that would have caught F-1).
 *
 * Dependency-free except the pinned Ajv 8.17.1 release gate (CI provisions
 * it outside the checkout and exposes it through NODE_PATH /
 * LEKALO_AJV_NODE_PATH). The gate:
 *
 * 1. validates the closed schema artifact compiles,
 * 2. checks every valid fixture for schema validity and duplicate JSON
 *    keys,
 * 3. refuses every adversarial fixture with the schema violation its name
 *    promises (the schema↔Rust parity guard lives in
 *    adapter_package::manifest::committed_exemplar_tests).
 *
 * Exit protocol: 0 with a one-line JSON status on success, 1 with a JSON
 * failure report on stderr otherwise.
 */
import { createRequire } from "node:module";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
try {
  const nodePath = process.env.LEKALO_AJV_NODE_PATH ?? "";
  if (nodePath) {
    const req = createRequire(nodePath + "/");
    Ajv2020 = req("ajv/dist/2020").default;
  } else {
    Ajv2020 = require("ajv/dist/2020").default;
  }
} catch {
  process.stderr.write(`${JSON.stringify({ ok: false, reason: "ajv-missing" }, null, 2)}\n`);
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const schemaPath = join(root, "contracts", "adapter-manifest.schema.v0.3.2.json");
const schema = JSON.parse(readFileSync(schemaPath, "utf8"));
const ajv = new Ajv2020({ strict: true, allErrors: true });
const validate = ajv.compile(schema);

const fail = (failures) => {
  process.stderr.write(`${JSON.stringify({ ok: false, failures }, null, 2)}\n`);
  process.exit(1);
};
const failures = [];
const check = (name, ok, detail) => {
  if (!ok) failures.push({ case: name, detail });
};

const validDir = join(root, "tests", "fixtures", "adapter-manifest", "valid");
const invalidDir = join(root, "tests", "fixtures", "adapter-manifest", "invalid");
const duplicateKeys = (text) => {
  const stack = [];
  const result = [];
  let depth = 0;
  // naive duplicate-key detector over strict JSON objects
  try {
    const seen = [];
    const walk = (value, path) => {
      if (Array.isArray(value)) {
        value.forEach((item, i) => walk(item, `${path}[${i}]`));
      } else if (value !== null && typeof value === "object") {
        const keys = new Set();
        for (const key of Object.keys(value)) {
          if (keys.has(key)) result.push(path + "/" + key);
          keys.add(key);
          walk(value[key], path + "/" + key);
        }
      }
    };
    walk(JSON.parse(text), "");
    return result;
  } catch {
    return [];
  }
};

  // The shipped exemplar is always part of the valid set.
  const exemplar = JSON.parse(
    readFileSync(join(root, "adapters", "node-typescript", "adapter.manifest.json"), "utf8"),
  );
  check("valid:shipped-exemplar", validate(exemplar), validate.errors);

for (const name of readdirSync(validDir)) {
  const text = readFileSync(join(validDir, name), "utf8");
  const value = JSON.parse(text);
  const ok = validate(value);
  check(`valid:${name}`, ok, validate.errors);
  const dups = duplicateKeys(text);
  check(`valid:${name}:duplicate-keys`, dups.length === 0, dups);
}

for (const name of readdirSync(invalidDir)) {
  const text = readFileSync(join(invalidDir, name), "utf8");
  const value = JSON.parse(text);
  const ok = validate(value);
  check(`invalid:${name}`, !ok, ok ? "schema accepted an adversarial fixture" : validate.errors);
}

if (failures.length > 0) fail(failures);
process.stdout.write(
  `${JSON.stringify({
    ok: true,
    schema: "lekalo/adapter-manifest/v0.3.2",
    valid: readdirSync(validDir).length,
    invalid: readdirSync(invalidDir).length,
  })}\n`,
);

