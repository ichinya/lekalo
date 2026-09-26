#!/usr/bin/env node
// Issue #85 release gate: the NFR attachment wire schema, the NFR
// evidence wire schema, the NFR report wire schema, the committed
// planner attachment, both evidence sets, and the derived report
// golden, validated with the same pinned third-party Draft 2020-12
// implementation as the other contract gates. Exact Ajv 8.17.1 is
// provisioned outside this checkout (CI does the same on Node 18 and
// 24) and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.
//
// The gate independently proves the canonical form of the golden
// report (byte-sorted keys, sorted collections) and recomputes its
// digest sidecar. A raw-text scan rejects duplicate JSON keys, which
// a parsed-value representation cannot see. Vectors whose rejection
// is semantic only — the closed wire cannot express them — live in
// the `diff` directory and must satisfy Ajv; the typed normalizer
// rejects them (proven by the Rust suite).

import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  ({ default: Ajv2020 } = require("ajv/dist/2020.js"));
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(
    `${JSON.stringify({
      ok: false,
      gate: "nfr",
      error: `Ajv 8.17.1 is not reachable through NODE_PATH / LEKALO_AJV_NODE_PATH: ${error}`,
    })}\n`,
  );
  process.exit(1);
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(
    `${JSON.stringify({
      ok: false,
      gate: "nfr",
      error: `exact Ajv 8.17.1 is required, found ${ajvVersion}`,
    })}\n`,
  );
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));

const failures = [];
const fail = (caseName, detail) => failures.push({ case: caseName, detail });
const fixtureDir = "tests/fixtures/nfr/planner";
const invalidDir = "tests/fixtures/nfr/invalid";
const semanticDir = "tests/fixtures/nfr/diff";
const goldenDir = "tests/fixtures/nfr/golden";

const ajv = new Ajv2020({ strict: true, allErrors: true });
const attachmentSchema = read("contracts/nfr.schema.v0.4.0.json");
const evidenceSchema = read("contracts/nfr-evidence.schema.v0.4.0.json");
const reportSchema = read("contracts/nfr-report.schema.v0.4.0.json");
let validateAttachment;
let validateEvidence;
let validateReport;
try {
  validateAttachment = ajv.compile(attachmentSchema);
  validateEvidence = ajv.compile(evidenceSchema);
  validateReport = ajv.compile(reportSchema);
} catch (error) {
  fail("schema:strict-compile", String(error));
}

// Wire violations are caught by the schemas; duplicate constraint ids,
// scope resolution, evidence coherence, and custody violations are
// caught by the typed normalizer (crates/lekalo-core). Semantic-only
// vectors may satisfy the schema; every other invalid vector must fail
// Ajv itself. Evidence vectors carry the `e-` prefix.
const SEMANTIC_ONLY = new Set([
  "duplicate-constraint.json",
  "endpoint-scope-on-command.json",
  "unknown-constraint-evidence.json",
  "unit-mismatch-evidence.json",
  "duplicate-result.json",
]);

// --- raw duplicate-key scan (a parsed value cannot see duplicates) ---
function duplicateKeys(text) {
  JSON.parse(text); // Reject malformed syntax independently of this key scan.
  const duplicates = new Set();
  const stack = [];
  const tokens = /"(?:\\.|[^"\\])*"|[{}\[\]:,]|[^\s{}\[\]:,"]+/g;
  for (const [token] of text.matchAll(tokens)) {
    const frame = stack.at(-1);
    if (token === "{") stack.push({ keys: new Set(), key: true });
    else if (token === "[") stack.push({ key: false });
    else if (token === "}" || token === "]") stack.pop();
    else if (token === "," && frame?.keys) frame.key = true;
    else if (token === ":" && frame?.keys) frame.key = false;
    else if (token.startsWith('"') && frame?.keys && frame.key) {
      const key = JSON.parse(token);
      if (frame.keys.has(key)) duplicates.add(key);
      frame.keys.add(key);
    }
  }
  return [...duplicates];
}

function scanDuplicateKeys(name, text) {
  const duplicates = duplicateKeys(text);
  if (duplicates.length > 0) {
    fail(`${name}:duplicate-json-key`, duplicates.join(","));
  }
}

// --- canonical form: sorted object keys, compact separators ---
function canonicalForm(documentText, name) {
  function write(value) {
    if (Array.isArray(value)) return `[${value.map(write).join(",")}]`;
    if (value !== null && typeof value === "object") {
      return `{${Object.keys(value)
        .sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)))
        .map(key => `${JSON.stringify(key)}:${write(value[key])}`).join(",")}}`;
    }
    return JSON.stringify(value);
  }
  const parsed = JSON.parse(documentText);
  if (write(parsed) !== documentText.replace(/\n$/, "")) {
    fail(`${name}:canonical-form`, "bytes are not sorted-key compact JSON");
  }
  return parsed;
}

// Negative controls must exercise the same rejection paths used for the golden.
const duplicateControls = [
  '{"constraints":[],"constraints":[]}',
  '{"results":[{"constraintId":"a"}],"results":[]}',
  '{"openQuestions":[{"questionId":"a","questionId":"b"}]}',
];
for (const [i, text] of duplicateControls.entries()) {
  const start = failures.length;
  scanDuplicateKeys(`control/duplicate-${i}`, text);
  if (failures.length !== start + 1) fail(`control/duplicate-${i}`, "duplicate-key rejection path did not fire");
  else failures.splice(start, 1);
}
for (const [i, text] of ['{"z":1,"a":2}', '{"2":0,"10":0}'].entries()) {
  const start = failures.length;
  canonicalForm(text, `control/unsorted-${i}`);
  if (failures.length !== start + 1) fail(`control/unsorted-${i}`, "canonical rejection path did not fire");
  else failures.splice(start, 1);
}
canonicalForm('{"10":0,"2":0,"a":[{"a":2,"z":1}]}', "control/canonical");

// 1. The committed attachment validates.
const attachmentText = readFileSync(resolve(root, fixtureDir, "nfr.attachment.json"), "utf8");
scanDuplicateKeys("attachment", attachmentText);
const attachment = JSON.parse(attachmentText);
if (!validateAttachment(attachment)) {
  fail("attachment:schema", JSON.stringify(validateAttachment.errors));
}

// 2. Both committed evidence sets validate.
const evidenceNames = ["nfr-evidence.staging-eu.json", "nfr-evidence.staging-us.json"];
for (const name of evidenceNames) {
  const text = readFileSync(resolve(root, fixtureDir, name), "utf8");
  scanDuplicateKeys(`evidence/${name}`, text);
  const document = JSON.parse(text);
  if (!validateEvidence(document)) {
    fail(`evidence/${name}:schema`, JSON.stringify(validateEvidence.errors));
  }
}

// 3. Every wire-invalid vector fails Ajv (semantic-only vectors pass).
for (const name of readdirSync(resolve(root, invalidDir)).sort()) {
  const text = readFileSync(resolve(root, invalidDir, name), "utf8");
  scanDuplicateKeys(`invalid/${name}`, text);
  const document = JSON.parse(text);
  const validate = name.startsWith("e-") ? validateEvidence : validateAttachment;
  if (validate(document)) {
    fail(`invalid/${name}:schema`, "expected Ajv rejection");
  }
}

// 4. Semantic-only vectors satisfy Ajv (the normalizer owns rejection).
for (const name of readdirSync(resolve(root, semanticDir)).sort()) {
  const text = readFileSync(resolve(root, semanticDir, name), "utf8");
  scanDuplicateKeys(`diff/${name}`, text);
  const document = JSON.parse(text);
  const validate = name.startsWith("unknown-constraint-evidence")
    || name.startsWith("unit-mismatch-evidence")
    || name.startsWith("duplicate-result")
    ? validateEvidence
    : validateAttachment;
  if (!validate(document)) {
    fail(`diff/${name}:schema`, JSON.stringify(validate.errors));
  }
}

// 5. The report golden validates, is canonical, and matches its
// digest sidecar.
const goldenText = readFileSync(resolve(root, goldenDir, "planner.report.json"), "utf8");
scanDuplicateKeys("golden/report", goldenText);
const goldenDocument = canonicalForm(goldenText, "golden/report");
if (!validateReport(goldenDocument)) {
  fail("golden/report:schema", JSON.stringify(validateReport.errors));
}
const sidecar = readFileSync(
  resolve(root, goldenDir, "planner.report.json.sha256"),
  "utf8",
).trim();
const hex = createHash("sha256").update(goldenText, "utf8").digest("hex");
if (sidecar !== `sha256:${hex}`) {
  fail("golden/report:digest", `${sidecar} != sha256:${hex}`);
}

// 6. The golden binds the attachment: the Model pin and the project
// match, and both dimension sections are disjoint.
if (
  goldenDocument.modelRef.modelVersion !== attachment.modelRef.modelVersion ||
  goldenDocument.modelRef.digest !== attachment.modelRef.digest
) {
  fail("golden:model-ref", "report model pin does not bind the attachment");
}
if (goldenDocument.projectId !== attachment.projectId) {
  fail("golden:project", "report project drifts from the attachment");
}
const runtimeIds = goldenDocument.runtime.constraints.map(row => row.constraintId);
const aiIds = goldenDocument.aiBudget.constraints.map(row => row.constraintId);
for (const id of aiIds) {
  if (runtimeIds.includes(id)) fail("golden:dimension-disjoint", id);
}

if (failures.length > 0) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, gate: "nfr", failures })}\n`,
  );
  process.exit(1);
}
process.stdout.write(
  `${JSON.stringify({
    ok: true,
    gate: "nfr",
    attachment: "valid",
    evidenceSets: evidenceNames.length,
    invalidVectors: readdirSync(resolve(root, invalidDir)).length,
    semanticVectors: readdirSync(resolve(root, semanticDir)).length,
    golden: "planner.report.json",
  })}\n`,
);
