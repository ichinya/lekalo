#!/usr/bin/env node
/**
 * Issue #29 contract gate: the composable target profile contract.
 *
 * Validates `contracts/target-profile.schema.v1.0.0.json` with the pinned
 * Ajv 8.17.1, proves the committed fixture set round-trips through the
 * closed schema — the issue's Node and Laravel profiles, a Go runtime
 * reusing the storage/transport/deployment components, a monorepo
 * document with inheritance and explicit overrides, decode-level
 * refusals, semantic refusals, and the resolved snapshot golden — and
 * checks the resolved golden's digests against the declared sha256
 * spelling.
 *
 * Structural acceptance only: the semantic vectors execute against the
 * production Rust decoder in `crates/lekalo-core/tests/target_profiles.rs`.
 * The gate is hermetic besides the pinned Ajv provided through
 * NODE_PATH/LEKALO_AJV_NODE_PATH.
 */
import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
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
    const nodePath = process.env.LEKALO_AJV_NODE_PATH ?? "";
    if (nodePath) {
      const req = createRequire(nodePath + "/");
      return req("ajv/package.json");
    }
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
  process.stderr.write(`${JSON.stringify({ ok: false, failures: [{ case: reason, detail }] }, null, 2)}\n`);
  process.exit(1);
}

function failAll(failures) {
  process.stderr.write(`${JSON.stringify({ ok: false, failures }, null, 2)}\n`);
  process.exit(1);
}

const read = (p) => readFileSync(new URL(p, import.meta.url), "utf8");

const schema = JSON.parse(read("../contracts/target-profile.schema.v1.0.0.json"));
const validate = ajv.compile(schema);

/** Raw lexer: reject duplicate JSON keys before any parse can hide them. */
function duplicateKeys(text) {
  const offenders = [];
  const stack = [{ keys: null }];
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

const ROOT = "../tests/fixtures/target-profile/";
const failures = [];
let accepted = 0;
let refused = 0;

for (const name of readdirSync(new URL(ROOT + "valid/", import.meta.url))) {
  if (!name.endsWith(".json")) continue;
  const text = read(`${ROOT}valid/${name}`);
  if (duplicateKeys(text).length > 0) {
    failures.push({ case: `valid/${name}`, detail: "duplicate decoded keys" });
    continue;
  }
  if (!validate(JSON.parse(text))) {
    failures.push({ case: `valid/${name}`, detail: JSON.stringify(validate.errors) });
  } else {
    accepted += 1;
  }
}

const resolved = JSON.parse(read(`${ROOT}valid/resolved/node-postgres-http.expect.json`));
if (!validate(resolved)) {
  failures.push({ case: "resolved-golden", detail: JSON.stringify(validate.errors) });
}
const digestSpelling = /^sha256:[0-9a-f]{64}$/;
for (const field of ["digest", "source_digest"]) {
  if (!digestSpelling.test(resolved[field])) {
    failures.push({ case: `resolved-golden:${field}`, detail: resolved[field] });
  }
}
const digestOf = (value) =>
  "sha256:" + createHash("sha256").update(JSON.stringify(value)).digest("hex");
if (!digestSpelling.test(digestOf(resolved))) {
  failures.push({ case: "digest-domain", detail: "the sha256 domain check is broken" });
}

for (const name of readdirSync(new URL(ROOT + "invalid/", import.meta.url))) {
  if (!name.endsWith(".json")) continue;
  const text = read(`${ROOT}invalid/${name}`);
  if (duplicateKeys(text).length > 0) {
    failures.push({ case: `invalid/${name}`, detail: "duplicate decoded keys" });
    continue;
  }
  let document;
  try {
    document = JSON.parse(text);
  } catch (error) {
    failures.push({ case: `invalid/${name}`, detail: `invalid JSON: ${error.message}` });
    continue;
  }
  if (validate(document)) {
    failures.push({ case: `invalid/${name}`, detail: "the frozen schema accepted a refusal vector" });
  } else {
    refused += 1;
  }
}

// Semantic vectors are decided by resolution, not by schema shape: the
// committed .expect.json rule must be a registered-looking rule id and
// the vector document itself must be schema-valid (they refuse on
// semantics, not on shape).
const RULE_ID = /^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+$/;
for (const name of readdirSync(new URL(ROOT + "semantic/", import.meta.url))) {
  if (!name.endsWith(".json") || name.endsWith(".expect.json")) continue;
  const text = read(`${ROOT}semantic/${name}`);
  if (duplicateKeys(text).length > 0) {
    failures.push({ case: `semantic/${name}`, detail: "duplicate decoded keys" });
    continue;
  }
  if (!validate(JSON.parse(text))) {
    failures.push({
      case: `semantic/${name}`,
      detail: "a semantic refusal vector must be shape-valid: " + JSON.stringify(validate.errors),
    });
    continue;
  }
  const expect = JSON.parse(read(`${ROOT}semantic/${name.replace(".json", ".expect.json")}`));
  if (!RULE_ID.test(expect.rule) || !expect.rule.startsWith("target-profile.")) {
    failures.push({ case: `semantic/${name}`, detail: `unexpected rule ${expect.rule}` });
  }
}

if (accepted === 0) failures.push({ case: "no-valid-goldens", detail: "the valid/ directory is empty" });
if (refused === 0) failures.push({ case: "no-invalid-vectors", detail: "the invalid/ directory is empty" });

if (failures.length > 0) {
  failAll(failures);
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  schema: "lekalo/target-profile/v1.0.0",
  validDocuments: accepted,
  refusedVectors: refused,
  resolvedGolden: resolved.id,
})}\n`);
