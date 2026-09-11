#!/usr/bin/env node
// Issue #91 release gate: the generate/verify receipt wire schema, the
// pinned golden receipt fixtures, and the cross-language invariants the
// closed schema cannot express (canonical ordering, verdict derivation,
// counts consistency, required/optional distinction, plan/write rules),
// validated with the same pinned third-party Draft 2020-12 implementation
// as the other contract gates. Exact Ajv 8.17.1 is provisioned outside
// this checkout (CI does the same on Node 18 and 24) and exposed through
// NODE_PATH / LEKALO_AJV_NODE_PATH.

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

const schema = read("contracts/orchestration-report.schema.v1.0.0.json");

const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateWire = ajv.compile(schema);

const failures = [];
const fail = (name, reason) => failures.push({ name, reason });

const isSorted = (values) =>
  values.every((value, index) => index === 0 || value >= values[index - 1]);

// The closed invariants the JSON Schema cannot express. Returns the list
// of violations; an empty list means the document is fully accepted.
const invariants = (document) => {
  const violations = [];
  if (document.identity !== "dev.lekalo.orchestration-report@1.0.0") {
    violations.push("wrong contract identity");
  }
  if (document.operation === "generate") {
    const names = document.targets.map((target) => target.target);
    if (!isSorted(names)) violations.push("targets are not sorted by id");
    if (names.length !== new Set(names).size) violations.push("duplicate target ids");
    if (document.counts.targets !== document.targets.length) {
      violations.push("counts.targets disagrees with the target rows");
    }
    const planned = document.targets.filter((t) => t.state === "planned").length;
    const applied = document.targets.filter((t) => t.state === "applied").length;
    const failed = document.targets.filter((t) => t.state === "failed").length;
    if (
      document.counts.planned !== planned ||
      document.counts.applied !== applied ||
      document.counts.failed !== failed
    ) {
      violations.push("target counts disagree with the target states");
    }
    for (const target of document.targets) {
      const keys = target.writes.map((write) => `${write.path} ${write.action}`);
      if (!isSorted(keys)) {
        violations.push(`${target.target}: writes are not in canonical order`);
      }
      for (const write of target.writes) {
        if (write.action === "delete" && write.sha256 !== null) {
          violations.push(`${target.target}: a deletion carries a content digest`);
        }
        if (write.action !== "delete" && write.sha256 === null) {
          violations.push(`${target.target}: ${write.action} without a content digest`);
        }
      }
      if (target.state === "failed" && target.writes.length > 0) {
        violations.push(`${target.target}: a failed target lists writes`);
      }
      if (target.state !== "failed" && target.planId === null) {
        violations.push(`${target.target}: a planned/applied target without a plan id`);
      }
      if (target.state === "applied" && target.manifestDigest === null) {
        violations.push(`${target.target}: an applied target without a manifest digest`);
      }
      if (target.state !== "applied" && target.manifestDigest !== null) {
        violations.push(`${target.target}: a non-applied target carries a manifest digest`);
      }
    }
    if (document.verdict === "ready" && document.counts.failed > 0) {
      violations.push("a ready verdict with failed targets");
    }
  }
  if (document.operation === "verify") {
    const ids = document.components.map((component) => component.id);
    if (!isSorted(ids)) violations.push("components are not sorted by id");
    if (ids.length !== new Set(ids).size) violations.push("duplicate component ids");
    for (const component of document.components) {
      if (
        component.state !== "pass" &&
        component.state !== "absent" &&
        component.reasonCode === null
      ) {
        violations.push(`${component.id}: non-pass without a reason code`);
      }
      if (component.state === "pass" && component.reasonCode !== null) {
        violations.push(`${component.id}: pass carries a reason code`);
      }
      const permanent = ["scenarios.execution", "native.gates"];
      if (
        permanent.includes(component.id) &&
        (component.state !== "unsupported" || component.required)
      ) {
        violations.push(`${component.id}: the declared absence must stay optional`);
      }
      if (component.id === "model.validation" && component.severityCounts === null) {
        violations.push("model.validation without severity counts");
      }
      if (component.id === "artifact.drift" && component.verdictCounts === null) {
        violations.push("artifact.drift without drift counts");
      }
      if (component.id.startsWith("adapter.") && component.findings === null) {
        violations.push(`${component.id}: adapter components report findings`);
      }
    }
    if (!document.components.some((component) => component.id === "model.validation")) {
      violations.push("missing the required model.validation component");
    }
    if (!document.components.some((component) => component.id === "artifact.drift")) {
      violations.push("missing the required artifact.drift component");
    }
    // Verdict derivation (ADR-0038): any fail blocks; a required
    // non-pass component or any degraded findings degrade; declared
    // optional absences stay exit-neutral.
    const derive = () => {
      if (document.components.some((c) => c.state === "fail")) return "blocked";
      if (
        document.components.some(
          (c) =>
            c.state === "degraded" ||
            (c.required && (c.state === "unsupported" || c.state === "absent")),
        )
      ) {
        return "degraded";
      }
      return "ready";
    };
    if (derive() !== document.verdict) {
      violations.push(`verdict ${document.verdict} != derived ${derive()}`);
    }
  }
  return violations;
};

const goldenDir = "tests/fixtures/orchestration";
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

// The coverage the acceptance criteria pin: the dry-run receipt that
// lists every intended file action, the applied receipt with the updated
// manifest digest, and the verify receipt with isolated per-target and
// required/optional components.
for (const required of [
  "generate.dry-run.golden.json",
  "generate.apply.golden.json",
  "verify.full.golden.json",
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
  const violations =
    schemaValid && document.operation !== undefined ? invariants(document) : [];
  if (schemaValid && violations.length === 0) {
    fail(`invalid:${name}`, "the vector unexpectedly satisfies the closed wire");
  }
}

if (goldenCount < 3) fail("coverage", "the three golden receipts must be pinned");
if (invalidCount === 0) fail("coverage", "no invalid vectors were checked");

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, failures }, null, 2)}\n`);
  process.exit(1);
}
process.stdout.write(
  `${JSON.stringify({ ok: true, checked: "orchestration-contracts-v1", goldens: goldenCount, invalid: invalidCount }, null, 2)}\n`,
);
