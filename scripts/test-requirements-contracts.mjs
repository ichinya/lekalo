#!/usr/bin/env node
// Issue #36 release gate: the requirements attachment wire schema, the
// requirements report wire schema, the committed planner attachment, the
// derived report golden, and the neutral trace-manifest projection
// golden, validated with the same pinned third-party Draft 2020-12
// implementation as the other contract gates. Exact Ajv 8.17.1 is
// provisioned outside this checkout (CI does the same on Node 18 and 24)
// and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.
//
// The gate independently proves the canonical form of both goldens:
// re-serializing the parsed document with byte-sorted object keys must
// reproduce the committed bytes exactly (the Rust canonical writer is
// verified against the same rule in
// crates/lekalo-core/tests/requirements.rs), and both digest sidecars
// are recomputed over the exact golden payloads. A raw-text scan
// additionally rejects duplicate JSON keys, which a parsed-value
// representation cannot see. Vectors whose rejection is semantic only —
// the closed wire cannot express them — live in the `diff` directory and
// must satisfy Ajv; the typed normalizer rejects them (proven by the
// Rust suite).

import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
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
      gate: "requirements",
      error: `Ajv 8.17.1 is not reachable through NODE_PATH / LEKALO_AJV_NODE_PATH: ${error}`,
    })}\n`,
  );
  process.exit(1);
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(
    `${JSON.stringify({
      ok: false,
      gate: "requirements",
      error: `exact Ajv 8.17.1 is required, found ${ajvVersion}`,
    })}\n`,
  );
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const failures = [];
const fail = (caseName, detail) => failures.push({ case: caseName, detail });
function failEarly(reason, detail) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, gate: "requirements", reason, detail })}\n`,
  );
  process.exit(1);
}
const fixtureDir = "tests/fixtures/requirements/planner";
const invalidDir = "tests/fixtures/requirements/invalid";
const semanticDir = "tests/fixtures/requirements/diff";
const goldenDir = "tests/fixtures/requirements/golden";

const ajv = new Ajv2020({ strict: true, allErrors: true });
const attachmentSchema = JSON.parse(
  readFileSync(resolve(root, "contracts/requirements.schema.v1.0.0.json"), "utf8"),
);
const reportSchema = JSON.parse(
  readFileSync(
    resolve(root, "contracts/requirements-report.schema.v1.0.0.json"),
    "utf8",
  ),
);
const traceSchema = JSON.parse(
  readFileSync(resolve(root, "contracts/trace-manifest.schema.v1.0.0.json"), "utf8"),
);
let validateAttachment;
let validateReport;
let validateTrace;
try {
  validateAttachment = ajv.compile(attachmentSchema);
  validateReport = ajv.compile(reportSchema);
  validateTrace = ajv.compile(traceSchema);
} catch (error) {
  fail("schema:strict-compile", String(error));
}

// Wire violations are caught by the schemas; namespace, containment,
// duplicate-reference, and custody violations are caught by the typed
// normalizer (crates/lekalo-core). Semantic-only vectors may satisfy the
// schema; every other invalid vector must fail Ajv itself.
const SEMANTIC_ONLY = new Set([
  "bad-symbol.json",
  "provider-root-traversal.json",
]);

// --- raw duplicate-key scan (a parsed value cannot see duplicates) ---
function duplicateKeys(text) {
  const duplicates = new Set();
  const stack = [{ inObject: false }];
  const objectKeys = [];
  let index = 0;
  let current = [];
  const pattern = /"(?:\\.|[^"\\])*"/;
  while (index < text.length) {
    const rest = text.slice(index);
    const match = pattern.exec(rest);
    if (!match) break;
    const token = match[0];
    index += match.index + token.length;
    let lookahead = index;
    while (lookahead < text.length && /\s/.test(text[lookahead])) lookahead += 1;
    const isKey = text[lookahead] === ":";
    if (isKey && stack[stack.length - 1].inObject) {
      const key = JSON.parse(token);
      if (current.includes(key)) duplicates.add(key);
      current.push(key);
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
  const parsed = JSON.parse(documentText);
  const resorted = JSON.stringify(parsed);
  if (resorted !== documentText.replace(/\n$/, "")) {
    fail(`${name}:canonical-form`, "bytes are not sorted-key compact JSON");
  }
  return parsed;
}

function goldenDigest(name) {
  const text = readFileSync(resolve(root, goldenDir, name), "utf8");
  const payload = text.endsWith("\n") ? text.slice(0, -1) : text;
  const sidecar = readFileSync(
    resolve(root, goldenDir, `${name}.sha256`),
    "utf8",
  ).trim();
  const hex = createHash("sha256").update(payload, "utf8").digest("hex");
  if (sidecar !== `sha256:${hex}`) {
    fail(`${name}:digest`, `${sidecar} != sha256:${hex}`);
  }
  return { text, payload };
}

// 1. The committed attachment validates.
const attachmentText = readFileSync(
  resolve(root, fixtureDir, "requirements.attachment.json"),
  "utf8",
);
scanDuplicateKeys("attachment", attachmentText);
const attachment = JSON.parse(attachmentText);
if (!validateAttachment(attachment)) {
  fail("attachment:schema", JSON.stringify(validateAttachment.errors));
}

// 2. Every wire-invalid vector fails Ajv (semantic-only vectors pass).
for (const name of readdirSync(resolve(root, invalidDir)).sort()) {
  const text = readFileSync(resolve(root, invalidDir, name), "utf8");
  scanDuplicateKeys(`invalid/${name}`, text);
  const document = JSON.parse(text);
  if (validateAttachment(document)) {
    if (!SEMANTIC_ONLY.has(name)) {
      fail(`invalid/${name}:schema`, "expected Ajv rejection");
    }
  } else if (SEMANTIC_ONLY.has(name)) {
    fail(`invalid/${name}:schema`, "semantic-only vector must satisfy Ajv");
  }
}

// 3. Semantic-only vectors satisfy Ajv (the normalizer owns rejection).
for (const name of readdirSync(resolve(root, semanticDir)).sort()) {
  const text = readFileSync(resolve(root, semanticDir, name), "utf8");
  scanDuplicateKeys(`diff/${name}`, text);
  const document = JSON.parse(text);
  if (!validateAttachment(document)) {
    fail(`diff/${name}:schema`, "semantic-only vector must satisfy Ajv");
  }
}

// 4. The report golden validates and is canonical with a pinned digest.
const reportGolden = goldenDigest("planner.report.json");
scanDuplicateKeys("golden/report", reportGolden.text);
const reportDocument = canonicalForm(reportGolden.payload, "golden/report");
if (!validateReport(reportDocument)) {
  fail("golden/report:schema", JSON.stringify(validateReport.errors));
}

// 5. The trace projection golden validates against the accepted #22
// trace contract and is canonical with a pinned digest.
const traceGolden = goldenDigest("planner.trace.json");
scanDuplicateKeys("golden/trace", traceGolden.text);
const traceDocument = canonicalForm(traceGolden.payload, "golden/trace");
if (!validateTrace(traceDocument)) {
  fail("golden/trace:schema", JSON.stringify(validateTrace.errors));
}

if (
  reportDocument.modelRef.modelVersion !== attachment.modelRef.modelVersion ||
  reportDocument.modelRef.digest !== attachment.modelRef.digest
) {
  fail("golden:model-ref", "report model pin does not bind the attachment");
}
if (reportDocument.sourceRevision !== traceDocument.sourceRevision) {
  fail("golden:source-revision", "trace manifest revision drifts from report");
}

if (failures.length > 0) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, gate: "requirements", failures })}\n`,
  );
  process.exit(1);
}
process.stdout.write(
  `${JSON.stringify({
    ok: true,
    gate: "requirements",
    attachment: "valid",
    invalidVectors: readdirSync(resolve(root, invalidDir)).length,
    semanticVectors: readdirSync(resolve(root, semanticDir)).length,
    goldens: ["planner.report.json", "planner.trace.json"],
  })}\n`,
);
