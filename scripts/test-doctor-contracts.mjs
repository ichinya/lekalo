#!/usr/bin/env node
// Issue #92 release gate: the doctor/status/readiness wire schema, the
// pinned golden report fixtures, and the cross-language invariants the
// closed schema cannot express (check order, verdict consistency, recipe
// coverage, preserved registry ids), validated with the same pinned
// third-party Draft 2020-12 implementation as the other contract gates.
// Exact Ajv 8.17.1 is provisioned outside this checkout (CI does the same
// on Node 18 and 24) and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.

import { createRequire } from "node:module";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  Ajv2020 = require("ajv/dist/2020.js").default;
  ajvVersion = require("ajv/package.json").version;
} catch {
  try {
    const fallback = join(process.env.LEKALO_AJV_NODE_PATH ?? "", "ajv");
    Ajv2020 = require(join(fallback, "dist/2020.js")).default;
    ajvVersion = JSON.parse(readFileSync(join(fallback, "package.json"), "utf8")).version;
  } catch (error) {
    process.stderr.write(`ajv-8.17.1-unavailable: ${error?.message ?? error}\n`);
    process.exit(1);
  }
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(`ajv-version-mismatch: ${ajvVersion}\n`);
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));

const schema = read("contracts/doctor.schema.v1.0.0.json");
const registry = read("contracts/diagnostic-registry.v1.14.0.json");
const registryIds = new Set(registry.entries.map((entry) => entry.id));

const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateWire = ajv.compile(schema);

const failures = [];
const fail = (name, reason) => failures.push({ name, reason });

// The closed recipe catalog (mirrors docs/doctor.md; the Rust catalog is
// the authority, the gate pins the published surface).
const RECIPES = new Set([
  "fix-structure",
  "create-lock",
  "preview-lock-update",
  "validate",
  "migrate-model",
  "resolve-adapters",
  "regenerate",
  "clear-cache",
  "recover-migration",
  "install-git",
  "init-git",
  "grant-write",
  "trace-validate",
]);

// Verdict rule (ADR-0032): doctor blocks on any blocked/unknown check;
// readiness blocks on a required blocked/unknown; status blocks on any
// blocked check. Any non-ok check degrades.
const deriveVerdict = (document) => {
  let degraded = false;
  for (const check of document.checks) {
    const relevant =
      document.report === "status"
        ? check.state === "blocked"
        : document.report === "doctor"
          ? check.state === "blocked" || check.state === "unknown"
          : check.required && (check.state === "blocked" || check.state === "unknown");
    if (relevant) return "blocked";
    if (check.state !== "ok") degraded = true;
  }
  return degraded ? "degraded" : "ready";
};

// The closed invariants the JSON Schema cannot express. Returns the list
// of violations; an empty list means the document is fully accepted.
const invariants = (document) => {
  const violations = [];
  const ids = document.checks.map((check) => check.id);
  const sorted = [...ids].sort();
  if (ids.some((id, index) => id !== sorted[index])) {
    violations.push("checks are not sorted by id");
  }
  if (ids.length !== new Set(ids).size) violations.push("duplicate check ids");
  for (const check of document.checks) {
    if (check.state !== "ok" && !check.nextAction) {
      violations.push(`${check.id}: missing nextAction on ${check.state}`);
    }
    if (check.state === "ok" && check.nextAction) {
      violations.push(`${check.id}: ok check carries a nextAction`);
    }
    if (check.nextAction && !RECIPES.has(check.nextAction)) {
      violations.push(`${check.id}: unknown recipe ${check.nextAction}`);
    }
    for (const id of check.diagnostics ?? []) {
      if (!registryIds.has(id)) {
        violations.push(`${check.id}: preserved id ${id} is not a registered rule`);
      }
    }
  }
  if (document.report === "readiness" && !document.phase) {
    violations.push("readiness report without a phase");
  }
  if (document.report !== "readiness" && document.phase) {
    violations.push(`${document.report} report carries a phase`);
  }
  if (deriveVerdict(document) !== document.verdict) {
    violations.push(`verdict ${document.verdict} != derived ${deriveVerdict(document)}`);
  }
  return violations;
};

const goldenDir = "tests/fixtures/doctor";
const invalidDir = join(goldenDir, "invalid");
let goldenCount = 0;
let invalidCount = 0;

for (const name of readdirSync(resolve(root, goldenDir)).sort()) {
  if (!name.endsWith(".json")) continue;
  goldenCount += 1;
  const text = readFileSync(resolve(root, goldenDir, name), "utf8");
  let document;
  try {
    document = JSON.parse(text);
  } catch (error) {
    fail(`golden:${name}`, `invalid JSON: ${error.message}`);
    continue;
  }
  if (!validateWire(document)) {
    fail(`golden:${name}`, validateWire.errors);
    continue;
  }
  for (const violation of invariants(document)) {
    fail(`golden:${name}`, violation);
  }
}

// The coverage the acceptance criteria pin: a ready fresh fixture, a
// degraded-missing-optional-HLV fixture, a blocked missing-required
// fixture with next actions, the status panel, and the stale/invalid
// lock states the issue #92 correction pins.
for (const required of [
  "doctor.ready.golden.json",
  "doctor.degraded.golden.json",
  "readiness.blocked.golden.json",
  "status.golden.json",
  "status.stale.golden.json",
  "status.invalid.golden.json",
]) {
  if (!readdirSync(resolve(root, goldenDir)).includes(required)) {
    fail("coverage", `${required} must be pinned`);
  }
}

for (const name of readdirSync(resolve(root, invalidDir)).sort()) {
  invalidCount += 1;
  const text = readFileSync(resolve(root, invalidDir, name), "utf8");
  let document;
  try {
    document = JSON.parse(text);
  } catch (error) {
    fail(`invalid:${name}`, `fixture itself is not JSON: ${error.message}`);
    continue;
  }
  const schemaValid = validateWire(document);
  const violations = schemaValid && document.checks instanceof Array ? invariants(document) : [];
  if (schemaValid && violations.length === 0) {
    fail(`invalid:${name}`, "the vector unexpectedly satisfies the closed wire");
  }
}

if (goldenCount < 4) fail("coverage", "the four golden reports must be pinned");
if (invalidCount === 0) fail("coverage", "no invalid vectors were checked");

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, failures }, null, 2)}\n`);
  process.exit(1);
}
process.stdout.write(
  `${JSON.stringify({ ok: true, checked: "doctor-contracts-v1", goldens: goldenCount, invalid: invalidCount }, null, 2)}\n`,
);
