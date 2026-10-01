#!/usr/bin/env node
// Issue #75 release gate: the context-budget report schema, the live
// CLI report projection, the closed state-wrapper arithmetic, and the
// privacy/determinism invariants, validated with the same pinned
// third-party Draft 2020-12 implementation as the other contract gates.
// Exact Ajv 8.17.1 is provisioned outside this checkout (CI does the
// same on Node 18 and 24) and exposed through NODE_PATH.
//
// The live CLI probe needs the built binary; it is skipped with an
// explicit flag when the binary is absent (mirroring the other gates'
// reportLiveSkipped behavior), but the schema and fixture checks below
// always run.

import { createRequire } from "node:module";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  ({ default: Ajv2020 } = require("ajv/dist/2020"));
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "ajv-unavailable",
    detail: error?.code ?? error?.message,
  }, null, 2)}\n`);
  process.exit(1);
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "ajv-version",
    detail: `expected 8.17.1, found ${ajvVersion}`,
  }, null, 2)}\n`);
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));

const reportSchema = read("contracts/context-budget-report.schema.v0.6.3.json");
const registry = read("contracts/diagnostic-registry.v0.6.3.json");
const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateReport = ajv.compile(reportSchema);

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

// 1. The registered context.* family is exactly the eight LEK-CONTEXT
//    rules with their pinned codes, categories, severities, and closed
//    status allowlists (positive vectors).
const expectedRules = [
  ["context.input-invalid", "LEK-CONTEXT-001", "infrastructure", "error", ["invalid"]],
  ["context.profile-unsupported", "LEK-CONTEXT-002", "compatibility", "error", ["unsupported-version"]],
  ["context.closure-incomplete", "LEK-CONTEXT-003", "semantic", "warning", ["valid"]],
  ["context.artifact-evidence-incomplete", "LEK-CONTEXT-004", "semantic", "warning", ["valid"]],
  ["context.baseline-incomparable", "LEK-CONTEXT-005", "compatibility", "warning", ["valid"]],
  ["context.budget-exceeded", "LEK-CONTEXT-006", "semantic", "warning", ["valid"]],
  ["context.baseline-regression", "LEK-CONTEXT-007", "semantic", "warning", ["valid"]],
  ["context.policy-denied", "LEK-CONTEXT-008", "security", "error", ["denied"]],
];
const registryEntries = new Map(registry.entries.map((entry) => [entry.id, entry]));
for (const [id, code, category, severity, statuses] of expectedRules) {
  const entry = registryEntries.get(id);
  if (!entry) fail("context-rule-missing", id);
  if (entry.code !== code) fail("context-code-drift", { id, code: entry.code });
  if (entry.lifecycle !== "active") fail("context-rule-lifecycle", id);
  if (entry.category !== category) fail("context-category-drift", id);
  if (entry.default_severity !== severity) fail("context-severity-drift", id);
  if (JSON.stringify(entry.allowed_statuses) !== JSON.stringify(statuses)) {
    fail("context-status-drift", id);
  }
}
// Negative vector: a rule under a status its entry does not allow.
const budgetEntry = registryEntries.get("context.budget-exceeded");
if (budgetEntry.allowed_statuses.includes("invalid")) {
  fail("context-status-negative", "budget-exceeded must not allow invalid");
}
const policyEntry = registryEntries.get("context.policy-denied");
if (policyEntry.allowed_statuses.includes("valid")) {
  fail("context-status-negative", "policy-denied must not allow valid");
}

// 2. The Rust core embeds this registry generation and the report
//    identity constants agree with the published contract.
const registrySource = readFileSync(
  resolve(root, "crates/lekalo-core/src/diagnostics/registry.rs"), "utf8");
if (!registrySource.includes("diagnostic-registry.v0.6.3.json")) {
  fail("rust-embeds-predecessor", "diagnostic-registry.v0.6.3.json");
}
const versionSource = readFileSync(
  resolve(root, "crates/lekalo-core/src/diagnostics/version.rs"), "utf8");
if (!versionSource.includes('REGISTRY_VERSION: &str = "0.6.3"')) {
  fail("rust-registry-version", "0.6.3");
}
const budgetVersionSource = readFileSync(
  resolve(root, "crates/lekalo-core/src/context_budget/version.rs"), "utf8");
for (const constant of [
  'FAMILY: &str = "dev.lekalo.context-budget-report"',
  'VERSION: &str = "0.6.3"',
  'IDENTITY: &str = "dev.lekalo.context-budget-report@0.6.3"',
  'SCHEMA_VERSION: &str = "lekalo/context-budget-report/v0.6.3"',
  'METRIC_VERSION: &str = "context-budget-semantics/1"',
]) {
  if (!budgetVersionSource.includes(constant)) fail("rust-constant", constant);
}

// 3. The live CLI projection satisfies its own closed schema, and the
//    adversarial negatives refuse. Skipped explicitly when the debug
//    binary has not been built.
const binary = resolve(root, "target/debug/lekalo.exe");
const binaryPosix = resolve(root, "target/debug/lekalo");
const lekalo = existsSync(binary) ? binary : existsSync(binaryPosix) ? binaryPosix : null;
let liveChecked = false;
if (lekalo) {
  const project = resolve(root, "tests/fixtures/context-budget/planner");
  const projectArg = project;
  const run = (args) =>
    JSON.parse(execFileSync(lekalo, ["--json", ...args], { encoding: "utf8", cwd: project }));
  const envelope = run(["context-budget", "--symbol", "planner.focus_task", "--budget", "200"]);
  if (envelope.status !== "valid") fail("live-status", envelope.status);
  const report = envelope.contextBudget;
  if (!validateReport(report)) fail("report-schema", validateReport.errors);
  liveChecked = true;

  const subject = report.subjects[0];
  if (subject.assessment !== "over-budget") fail("live-assessment", subject.assessment);
  const required = subject.metrics.minimumRequiredSemanticTokens.value;
  if (subject.overByTokens.value !== required - 200) fail("over-by", subject.overByTokens.value);
  const ledger = subject.requiredFacts.reduce((total, fact) => total + fact.tokens, 0);
  if (ledger !== required) fail("ledger-reconciles", { ledger, required });
  if (!envelope.diagnostics.some((item) => item.id === "context.budget-exceeded")) {
    fail("live-warning", "context.budget-exceeded missing");
  }
  if (!report.complete) fail("live-complete", "planner fixture must complete");

  // The module scope reconciles its union against the per-subject sums.
  const moduleEnvelope = run([
    "context-budget", "--module", "planner", "--budget", "12000",
    "--simulate-capsule", "--suggest",
  ]);
  const moduleReport = moduleEnvelope.contextBudget;
  if (!validateReport(moduleReport)) fail("module-schema", validateReport.errors);
  const perSubject = moduleReport.subjects.reduce(
    (total, item) =>
      total + (item.metrics.minimumRequiredSemanticTokens.value ?? 0), 0);
  const union = moduleReport.summary.unionRequiredTokens.value;
  if (union > perSubject) fail("union-reconciles", { union, perSubject });

  // The adversarial negatives refuse closed.
  const corrupted = JSON.parse(JSON.stringify(report));
  corrupted.subjects[0].metrics.directDependencies = { state: "unknown", value: 3 };
  if (validateReport(corrupted)) fail("negative-value-under-state", "accepted");
  const extra = JSON.parse(JSON.stringify(report));
  extra.surprise = true;
  if (validateReport(extra)) fail("negative-unknown-field", "accepted");
  const negative = JSON.parse(JSON.stringify(report));
  negative.summary.subjects = -1;
  if (validateReport(negative)) fail("negative-count", "accepted");

  // Determinism: two runs are byte-identical.
  const again = execFileSync(
    lekalo,
    ["--json", "context-budget", "--symbol", "planner.focus_task", "--budget", "200"],
    { encoding: "utf8", cwd: project });
  const first = execFileSync(
    lekalo,
    ["--json", "context-budget", "--symbol", "planner.focus_task", "--budget", "200"],
    { encoding: "utf8", cwd: project });
  if (again !== first) fail("determinism", "repeated runs differ");
  for (const forbidden of ["timestamp", "C:/", "C:\\\\", "target/debug"]) {
    if (again.includes(forbidden)) fail("privacy", forbidden);
  }
}

// 4. The canonical fixture golden: the pinned over-budget report bytes.
const fixturePath = "tests/fixtures/context-budget/golden/planner.over-budget.json";
const fixtureBytes = readFileSync(resolve(root, fixturePath), "utf8");
const fixture = JSON.parse(fixtureBytes);
if (fixture.schemaVersion !== "lekalo/context-budget-report/v0.6.3") {
  fail("golden-schema", fixture.schemaVersion);
}
if (!validateReport(fixture)) fail("golden-schema", validateReport.errors);
const goldenSubject = fixture.subjects[0];
if (goldenSubject.assessment !== "over-budget") fail("golden-assessment", goldenSubject.assessment);
const goldenRequired = goldenSubject.metrics.minimumRequiredSemanticTokens.value;
const goldenLedger = goldenSubject.requiredFacts.reduce((total, fact) => total + fact.tokens, 0);
if (goldenLedger !== goldenRequired) fail("golden-ledger", { ledger: goldenLedger, required: goldenRequired });
if (goldenSubject.overByTokens.value !== goldenRequired - 200) {
  fail("golden-over-by", goldenSubject.overByTokens.value);
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  ajv: ajvVersion,
  registryEntries: registry.entries.length,
  contextRules: expectedRules.length,
  liveChecked,
  golden: fixturePath,
}, null, 2)}\n`);
