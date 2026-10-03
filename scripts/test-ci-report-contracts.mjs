#!/usr/bin/env node
// Issue #103 release gate: the closed ci-report wire schema, the pinned
// golden report fixtures produced by the real binary, the deterministic
// JUnit/SARIF/Markdown projections, and the cross-language invariants
// the closed schema cannot express (verdict/exit coherence, sorted ids,
// diagnostic-index binding, count derivation), validated with the same
// pinned third-party Draft 2020-12 implementation as the other contract
// gates. Exact Ajv 8.17.1 is provisioned outside this checkout (CI does
// the same on Node 18 and 24) and exposed through NODE_PATH /
// LEKALO_AJV_NODE_PATH.

import { createRequire } from "node:module";
import { readFileSync, readdirSync } from "node:fs";
import { createHash } from "node:crypto";
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

import { existsSync } from "node:fs";

// The canonical byte form: recursive byte-sorted keys, compact, one
// trailing LF (must match the Rust `to_json_string` byte for byte).
const canonicalBytes = (document) => {
  const sorted = (value) => {
    if (Array.isArray(value)) return value.map(sorted);
    if (value && typeof value === "object") {
      const out = {};
      for (const key of Object.keys(value).sort()) out[key] = sorted(value[key]);
      return out;
    }
    return value;
  };
  return JSON.stringify(sorted(document)) + "\n";
};

const schema = read("contracts/ci-report.schema.v0.6.3.json");
const registry = read("contracts/diagnostic-registry.v0.6.3.json");
const registryIds = new Set(registry.entries.map((entry) => entry.id));

const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateWire = ajv.compile(schema);

const failures = [];
const fail = (name, reason) => failures.push({ name, reason });

// The fabricated pin the fix round removed from the code (review F3):
// `sha256:e3b0c442…b855` is exactly SHA-256("") — the digest of an
// empty inventory. A `known` pin carrying it is always a fabrication;
// an empty inventory must stay `unknown` with a closed reason.
const EMPTY_SHA256 = `sha256:${createHash("sha256").update("").digest("hex")}`;

const isSorted = (values) =>
  values.every((value, index) => index === 0 || value > values[index - 1]);

// The closed invariants the JSON Schema cannot express. Returns the
// list of violations; an empty list means the document is fully
// accepted.
const invariants = (document) => {
  const violations = [];
  // Checks are sorted and unique by id.
  const checkIds = document.checks.map((check) => check.id);
  if (!isSorted(checkIds)) violations.push("checks are not sorted/unique by id");
  // Suites are sorted and unique by id; cases carry closed outcomes.
  const suiteIds = document.suites.map((suite) => suite.id);
  if (!isSorted(suiteIds)) violations.push("suites are not sorted/unique by id");
  // Exit/status coherence.
  const exits = { valid: 0, invalid: 1, denied: 3, unsupported: 4, unavailable: 4, "unsupported-version": 5 };
  for (const [label, outcome] of [
    ["commandResult", document.commandResult],
    ["evaluation", document.evaluation],
  ]) {
    if (exits[outcome.status] !== outcome.exitCode) {
      violations.push(`${label}.exitCode ${outcome.exitCode} disagrees with status ${outcome.status}`);
    }
  }
  // A blocked verdict demands terminal evidence (the same rule
  // `model.rs` validates): a fail/error effective outcome among the
  // checks and suite cases, or a failed command result (review R2-6:
  // the suite side reads `effectiveOutcome` — the single evaluated
  // decision — instead of re-deriving from source outcome + required).
  const hasBlocking =
    document.commandResult.exitCode !== 0 ||
    document.checks.some(
      (check) => check.effectiveOutcome === "fail" || check.effectiveOutcome === "error",
    ) ||
    document.suites.some((suite) =>
      suite.cases.some(
        (row) => row.effectiveOutcome === "fail" || row.effectiveOutcome === "error",
      ),
    );
  if ((document.evaluation.verdict === "blocked") !== hasBlocking) {
    violations.push(`verdict ${document.evaluation.verdict} disagrees with the blocking evidence`);
  }
  // A `known` working-set digest that is the digest of the empty
  // string is always fabricated (review F3).
  const workingSet = document.provenance?.git?.workingSetDigest;
  if (workingSet?.state === "known" && workingSet?.value === EMPTY_SHA256) {
    violations.push("workingSetDigest pins the fabricated empty-inventory digest");
  }
  // Diagnostic indexes: sorted, unique, and every referenced index is
  // declared and in bounds.
  const indexes = document.diagnosticIndexes;
  if (!isSorted(indexes)) violations.push("diagnosticIndexes are not sorted/unique");
  if (indexes.length !== new Set(indexes).size) violations.push("duplicate diagnostic indexes");
  const referenced = new Set(
    document.checks
      .flatMap((check) => check.diagnosticIndexes)
      .concat(document.suites.flatMap((suite) => suite.cases.flatMap((row) => row.diagnosticIndexes))),
  );
  // Node 18-compatible subset check (Set.prototype.isSubsetOf is
  // Node 20+ only; the contracts gate must run on both CI legs).
  const declaredSet = new Set(indexes);
  const isSubset = [...referenced].every((index) => declaredSet.has(index));
  if (referenced.size !== indexes.length || !isSubset) {
    violations.push("diagnosticIndexes disagree with the referenced set");
  }
  for (const index of indexes) {
    if (index >= document.diagnostics.length) violations.push("dangling diagnostic index");
  }
  // Every embedded diagnostic is a registered rule with the exact code.
  for (const diagnostic of document.diagnostics) {
    const entry = registryIds.has(diagnostic.id);
    if (!entry) violations.push(`unregistered rule ${diagnostic.id}`);
    const registered = registry.entries.find((candidate) => candidate.id === diagnostic.id);
    if (registered && registered.code !== diagnostic.code) {
      violations.push(`code drift for ${diagnostic.id}`);
    }
  }
  // No absolute or hostile path spellings anywhere in the document.
  const raw = JSON.stringify(document);
  for (const hostile of ["C:\\\\", "\\\\\\\\ workstation", "file://", "D:\\\\"]) {
    if (raw.includes(hostile.replace(/\\\\/g, "\\"))) {
      violations.push(`absolute/host path spelling present: ${hostile}`);
    }
  }
  return violations;
};

// 1. The golden fixtures the real binary produced.
const goldenDir = "tests/fixtures/ci-report";
let goldenCount = 0;
for (const name of readdirSync(resolve(root, goldenDir)).sort()) {
  if (!name.endsWith(".json") || !name.startsWith("valid.")) continue;
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
  // Canonical bytes: compact form with byte-sorted keys at every
  // level, exactly one trailing LF, and no CR anywhere.
  const compact = canonicalBytes(document);
  if (text !== compact) {
    fail(`golden:${name}`, "the committed golden is not the canonical byte-sorted form with one trailing LF");
  }
}

// Required coverage: all four commands pinned, both a ready and a
// blocked verdict.
for (const required of [
  "valid.validate.golden.json",
  "valid.verify.golden.json",
  "valid.generate-check.golden.json",
  "valid.readiness.golden.json",
]) {
  if (!readdirSync(resolve(root, goldenDir)).includes(required)) {
    fail("coverage", `${required} must be pinned`);
  }
}
const verdicts = new Set();
for (const name of readdirSync(resolve(root, goldenDir))) {
  if (!name.endsWith(".json") || !name.startsWith("valid.")) continue;
  try {
    verdicts.add(JSON.parse(readFileSync(resolve(root, goldenDir, name), "utf8")).evaluation.verdict);
  } catch {
    // counted above
  }
}
if (!verdicts.has("ready") || !verdicts.has("blocked")) {
  fail("coverage", `both a ready and a blocked golden are required (found: ${[...verdicts]})`);
}

// 2. The negative vectors: every one must be refused.
const invalidDir = join(goldenDir, "invalid");
let invalidCount = 0;
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
  const violations = schemaValid ? invariants(document) : [];
  if (schemaValid && violations.length === 0) {
    fail(`invalid:${name}`, "the vector unexpectedly satisfies the closed wire");
  }
}

// 3. The Markdown projection pins: bounded, escaped, table-headed.
const summaryPath = resolve(root, goldenDir, "valid.summary.golden.md");
try {
  const summary = readFileSync(summaryPath, "utf8");
  if (!summary.startsWith("## Lekalo ")) fail("markdown:summary", "missing the command heading");
  if (!summary.includes("| pin | value |")) fail("markdown:summary", "missing the revisions table");
  if (!summary.endsWith("\n") || summary.endsWith("\n\n")) {
    fail("markdown:summary", "the document is exactly one trailing LF");
  }
  if (/<script|javascript:/i.test(summary)) {
    fail("markdown:summary", "raw HTML/link syntax leaked");
  }
} catch {
  fail("markdown:summary", "the golden summary must be pinned");
}

// 4. The SARIF projection pins: SARIF 2.1.0 shape with the Lekalo
// properties, parsed and re-deterministic.
const sarifPath = resolve(root, goldenDir, "valid.sarif.golden.sarif");
try {
  const sarif = JSON.parse(readFileSync(sarifPath, "utf8"));
  if (sarif.version !== "2.1.0") fail("sarif:golden", "not SARIF 2.1.0");
  const run = sarif.runs?.[0];
  if (!run) fail("sarif:golden", "missing the run");
  if (run?.tool?.driver?.name !== "Lekalo") fail("sarif:golden", "missing the Lekalo driver");
  if (run?.columnKind !== "unicodeCodePoints") fail("sarif:golden", "wrong columnKind");
  if (!run?.properties?.lekaloReportDigest?.startsWith("sha256:")) {
    fail("sarif:golden", "missing the report digest binding");
  }
  // The digest must bind a committed report, not an arbitrary string
  // (review F4): the sha256 of one of the pinned JSON goldens, with
  // status/exit/verdict agreeing with that bound document.
  const jsonGoldenDigests = new Map();
  for (const name of readdirSync(resolve(root, goldenDir)).sort()) {
    if (!name.endsWith(".json") || !name.startsWith("valid.")) continue;
    const bytes = readFileSync(resolve(root, goldenDir, name));
    jsonGoldenDigests.set(`sha256:${createHash("sha256").update(bytes).digest("hex")}`, name);
  }
  const boundName = jsonGoldenDigests.get(run?.properties?.lekaloReportDigest);
  if (!boundName) {
    fail(
      "sarif:binding",
      `lekaloReportDigest ${run?.properties?.lekaloReportDigest} matches no committed JSON golden`,
    );
  } else {
    const bound = JSON.parse(readFileSync(resolve(root, goldenDir, boundName), "utf8"));
    const properties = run.properties;
    if (properties.lekaloStatus !== bound.evaluation.status) {
      fail("sarif:binding", `lekaloStatus disagrees with ${boundName}`);
    }
    if (properties.lekaloExitCode !== bound.evaluation.exitCode) {
      fail("sarif:binding", `lekaloExitCode disagrees with ${boundName}`);
    }
    if (properties.lekaloVerdict !== bound.evaluation.verdict) {
      fail("sarif:binding", `lekaloVerdict disagrees with ${boundName}`);
    }
  }
  // The pinned official OASIS SARIF 2.1.0 schema (review F5): the
  // golden must satisfy the real specification, not just our fields.
  const sarifSchema = read("scripts/lib/sarif-schema-2.1.0.json");
  const { pathToFileURL } = await import("node:url");
  const resolveDep = (name) =>
    pathToFileURL(require.resolve(name, { paths: [process.env.LEKALO_AJV_NODE_PATH ?? ""] })).href;
  const { default: AjvDraft04 } = await import(resolveDep("ajv-draft-04"));
  const { default: addFormats } = await import(resolveDep("ajv-formats"));
  const draft04 = new AjvDraft04({ allErrors: true });
  addFormats(draft04);
  const validateSarif = draft04.compile(sarifSchema);
  // The golden must exercise the inline-diagnostic path (AC1): at
  // least one derived rule and one result with a safe location.
  if ((run.tool.driver.rules ?? []).length === 0) {
    fail("sarif:inline", "the golden carries no derived rules");
  }
  if ((run.results ?? []).length === 0) {
    fail("sarif:inline", "the golden carries no results");
  }
  const withLocation = (run.results ?? []).filter(
    (r) => r.locations?.[0]?.physicalLocation?.artifactLocation?.uri,
  );
  if (withLocation.length === 0) {
    fail("sarif:inline", "no result carries a repository-relative location");
  }
  if (!validateSarif(sarif)) {
    fail("sarif:schema", validateSarif.errors);
  }
  const ids = (run?.tool?.driver?.rules ?? []).map((rule) => rule.id);
  if (JSON.stringify(ids) !== JSON.stringify([...ids].sort())) {
    fail("sarif:golden", "rules are not sorted by id");
  }
} catch (error) {
  fail("sarif:golden", error.message);
}

// 5. The JUnit projection pins: the golden is XML-well-formed
// (balanced tags, no raw control characters), counts derive from the
// emitted children, and the document is deterministic.
const junitPath = resolve(root, goldenDir, "valid.suite.golden.junit.xml");
if (existsSync(junitPath)) {
  const junit = readFileSync(junitPath, "utf8");
  const balanced = (tag) =>
    (junit.match(new RegExp(`<${tag}[ >]`, "g")) || []).length ===
    (junit.match(new RegExp(`</${tag}>`, "g")) || []).length;
  for (const tag of ["testsuites", "testsuite", "testcase"]) {
    if (!balanced(tag)) fail(`junit:${tag}`, "unbalanced tags");
  }
  if (!junit.startsWith('<?xml version="1.0" encoding="UTF-8"?>')) {
    fail("junit:prolog", "missing the XML prolog");
  }
  if (/[\u0000-\u0008\u000B\u000C\u000E-\u001F]/.test(junit)) {
    fail("junit:control-chars", "raw control characters in the document");
  }
  const suites = [...junit.matchAll(/<testsuite ([^>]*)>([\s\S]*?)<\/testsuite>/g)].map(
    (m) => ({ head: m[1], body: m[2] }),
  );
  const attr = (text, name) =>
    Number((text.match(new RegExp(`${name}="(\\d+)"`)) || [])[1] ?? "-1");
  const totals = { tests: 0, failures: 0, errors: 0, skipped: 0 };
  for (const { head, body } of suites) {
    const counts = {
      tests: attr(head, "tests"),
      failures: attr(head, "failures"),
      errors: attr(head, "errors"),
      skipped: attr(head, "skipped"),
    };
    for (const [name, value] of Object.entries(counts)) {
      if (value < 0) fail("junit:counts", `missing count attribute ${name}`);
    }
    // Counts must derive from the emitted children, not merely exist
    // (review F7): a tampered attribute over a different child set
    // fails the gate.
    const emitted = {
      tests: (body.match(/<testcase[ >]/g) || []).length,
      failures: (body.match(/<failure[ />]/g) || []).length,
      errors: (body.match(/<error[ />]/g) || []).length,
      skipped: (body.match(/<skipped[ />]/g) || []).length,
    };
    for (const name of Object.keys(counts)) {
      if (counts[name] !== emitted[name]) {
        fail(
          "junit:counts",
          `${name}="${counts[name]}" disagrees with ${emitted[name]} emitted children`,
        );
      }
    }
    for (const name of Object.keys(totals)) totals[name] += Math.max(counts[name], 0);
  }
  const aggregate = junit.match(/<testsuites ([^>]*)>/);
  if (!aggregate) {
    fail("junit:aggregate", "missing the testsuites element");
  } else {
    for (const name of Object.keys(totals)) {
      const declared = attr(aggregate[1], name);
      if (declared !== totals[name]) {
        fail(
          "junit:aggregate",
          `${name}="${declared}" disagrees with the suite sum ${totals[name]}`,
        );
      }
    }
  }
} else {
  fail("junit:golden", "the JUnit suite golden must be pinned");
}
if (goldenCount < 4) fail("coverage", "the four command goldens must be pinned");
if (invalidCount < 3) fail("coverage", "the adversarial vectors must be pinned");

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, failures }, null, 2)}\n`);
  process.exit(1);
}
process.stdout.write(
  `${JSON.stringify({
    ok: true,
    checked: "ci-report-contracts-v1",
    ajv: ajvVersion,
    goldens: goldenCount,
    invalid: invalidCount,
    verdicts: [...verdicts].sort(),
  }, null, 2)}\n`,
);
