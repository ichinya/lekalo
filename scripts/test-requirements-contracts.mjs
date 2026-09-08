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
// report keys use byte order, trace keys use the published fixed order;
// re-serializing the parsed document and canonical collections must
// reproduce the committed bytes exactly (the Rust canonical writer is
// verified against the same rule in
// crates/lekalo-core/tests/requirements.rs), and both digest sidecars
// are recomputed over the exact golden payloads. A raw-text scan
// additionally rejects duplicate JSON keys, which a parsed-value
// representation cannot see. Vectors whose rejection is semantic only вЂ”
// the closed wire cannot express them вЂ” live in the `diff` directory and
// must satisfy Ajv; the typed normalizer rejects them (proven by the
// Rust suite).

import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { canonicalTrace } from "./requirements-trace-canonical.mjs";

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
  // Write recursively rather than relying on JavaScript property insertion
  // order (integer-looking keys get reordered even after Object.fromEntries).
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
  const normalized = structuredClone(parsed);
  if (normalized?.schemaVersion === 'lekalo/requirements-report/v1.0.0') {
    const compare = keys => (a,b) => {
      for (const key of keys) {
        const order = Buffer.compare(Buffer.from(a[key]), Buffer.from(b[key]));
        if (order) return order;
      }
      return 0;
    };
    for (const [field, keys] of Object.entries({providers:['source'], requirements:['source','id'], references:['symbol','relation','source','requirement','revision'], coverageGaps:['source','id'], conflicts:['source','capability','subjectId','detail'], impact:['source','requirement','change']})) {
      normalized[field].sort(compare(keys));
      for (const row of normalized[field]) {
        row.symbols?.sort(); row.renameCandidates?.sort();
      }
    }
  }
  if (write(normalized) !== documentText.replace(/\n$/, "")) {
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

// Negative controls must exercise the same rejection paths used for goldens.
const duplicateControls = [
  '{"references":[],"references":[]}',
  '{"providers":[{"root":"a","root":"b"}]}',
  '{"modelRef":{"digest":"a","\u0064igest":"b"}}',
  '{"references":[],"\u0072eferences":[]}',
];
for (const [i, text] of duplicateControls.entries()) {
  const start = failures.length;
  scanDuplicateKeys(`control/${i}`, text);
  if (failures.length !== start + 1) fail(`control/${i}`, "duplicate-key rejection path did not fire");
  else failures.splice(start, 1);
}
for (const text of ['{"a":{"x":1},"b":{"x":2}}', JSON.stringify({a: 'braces { and key "x":', b: 2})]) {
  // Sibling object keys are independent, and string contents are opaque.
  if (duplicateKeys(text).length) fail("control/siblings", "false duplicate");
}
for (const [i, text] of ['{"z":1,"a":2}', '{"a":[{"z":1,"a":2}]}', '{"2":0,"10":0}'].entries()) {
  const start = failures.length;
  canonicalForm(text, `control/unsorted-${i}`);
  if (failures.length !== start + 1) fail(`control/unsorted-${i}`, "canonical rejection path did not fire");
  else failures.splice(start, 1);
}
canonicalForm('{"10":0,"2":0,"a":[{"a":2,"z":1}]}', "control/canonical");

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

// The same vectors exercise parse/from_value and real CLI loads in Rust.
const projectIdVectors = JSON.parse(readFileSync(resolve(root,
  "tests/fixtures/requirements/project-id-vectors.json"), "utf8"));
const modelSchema = JSON.parse(readFileSync(resolve(root,
  "contracts/model.schema.v1.0.0.json"), "utf8"));
const validateModelProjectId = ajv.compile({
  $defs: modelSchema.$defs, $ref: "#/$defs/projectId",
});
for (const {id, valid} of projectIdVectors) {
  const candidate = structuredClone(attachment); candidate.projectId = id;
  if (validateAttachment(candidate) !== valid) fail(`project-id/${JSON.stringify(id)}`, "attachment grammar differs");
  if (validateModelProjectId(id) !== valid) fail(`model-project-id/${JSON.stringify(id)}`, "Model grammar differs");
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
const traceDocument = JSON.parse(traceGolden.payload);
if (canonicalTrace(traceDocument) !== traceGolden.payload) {
  fail("golden/trace:canonical-form", "bytes violate the fixed trace contract order");
}
const traceReversed = Object.fromEntries(Object.entries(traceDocument).reverse());
if (JSON.stringify(traceReversed) === canonicalTrace(traceReversed)) {
  fail("control/trace-key-order", "reordered trace was accepted");
}
const traceNodesReversed = structuredClone(traceDocument);
traceNodesReversed.nodes.reverse();
if (JSON.stringify(traceNodesReversed) === canonicalTrace(traceNodesReversed)) {
  fail("control/trace-node-order", "reordered trace nodes were accepted");
}
if (!validateTrace(traceDocument)) {
  fail("golden/trace:schema", JSON.stringify(validateTrace.errors));
}

// Real CLI export with two valid 191-character semantic IDs differing only
// at the final character; the Rust CLI regression pins these exact bytes.
const maximalGolden = goldenDigest("maximal.trace.json");
scanDuplicateKeys("golden/maximal", maximalGolden.text);
const maximalTrace = JSON.parse(maximalGolden.payload);
if (!validateTrace(maximalTrace)) fail("golden/maximal:schema", JSON.stringify(validateTrace.errors));
if (canonicalTrace(maximalTrace) !== maximalGolden.payload) fail("golden/maximal:canonical-form", "trace is not canonical");
const maximalSymbols = maximalTrace.nodes.filter(n => n.semanticId?.length === 191);
if (maximalSymbols.length !== 2 || new Set(maximalSymbols.map(n => n.nodeId)).size !== 2) {
  fail("golden/maximal:identity", "maximum semantic IDs must have distinct local nodes");
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

for (const [name, change] of [
  ["root-length", r => { r.providers[0].root = "a".repeat(513); }],
  ["requirement-id-length", r => { r.requirements[0].id = `${"a".repeat(63)}.REQ-${"b".repeat(64)}`; }],
  ["capability-count", r => { r.providers[0].capabilityCount = 257; }],
  ["aggregate-rows", r => { r.requirements = Array(10001).fill(r.requirements[0]); }],
  ["conflict-privacy", r => { r.conflicts = [{source:"openspec",capability:"planner",title:"raw text",detail:"duplicate-title"}]; }],
]) {
  const invalid = structuredClone(reportDocument); change(invalid);
  if (validateReport(invalid)) fail(`control/schema-${name}`, "invalid report was accepted");
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
    projectIdVectors: projectIdVectors.length,
    goldens: ["planner.report.json", "planner.trace.json", "maximal.trace.json"],
  })}\n`,
);
