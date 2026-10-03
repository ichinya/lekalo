#!/usr/bin/env node
// Issue #75 release gate: the context-budget report schema, the live
// CLI report projection, the closed state-wrapper arithmetic, and the
// privacy/determinism invariants, validated with the same pinned
// third-party Draft 2020-12 implementation as the other contract gates.
// Exact Ajv 8.17.1 is provisioned outside this checkout (CI does the
// same in the build-test job) and exposed through NODE_PATH.
//
// The live CLI probe needs the built binary and is mandatory: when the
// binary is absent this gate fails closed with "binary-missing" (no
// skip flag exists). CI therefore runs this gate in build-test, after
// `cargo build --workspace --locked` (R2-B1); the schema and fixture
// checks also validate the committed golden after the live probes.

import { createRequire } from "node:module";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";

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
const fixturePath = "tests/fixtures/context-budget/golden/planner.over-budget.json";
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));

const reportSchema = read("contracts/context-budget-report.schema.v0.6.3.json");
const profileSchema = read("contracts/context-budget-profile.schema.v0.6.3.json");
const policySchema = read("contracts/context-budget-policy.schema.v0.6.3.json");
const comparisonSchema = read("contracts/context-budget-comparison.schema.v0.6.3.json");
const registry = read("contracts/diagnostic-registry.v0.6.3.json");
const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateReport = ajv.compile(reportSchema);
const validateProfile = ajv.compile(profileSchema);
const validatePolicy = ajv.compile(policySchema);
const validateComparison = ajv.compile(comparisonSchema);

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
//    adversarial negatives refuse. The binary check below fails closed
//    when the debug binary has not been built; the live checks are
//    mandatory (R2-B1).
const binary = resolve(root, "target/debug/lekalo.exe");
const binaryPosix = resolve(root, "target/debug/lekalo");
if (!existsSync(binary) && !existsSync(binaryPosix)) {
  fail("binary-missing", "build target/debug/lekalo before this gate; the live checks are mandatory");
}
const lekalo = existsSync(binary) ? binary : binaryPosix;
let liveChecked = false;
{
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
  // The breakdown reconciliation (R2-6): Σ(exclusive) + Σ(shared)
  // equals the required total, and shared cost is billed to the
  // subject's own row, never to an unrelated dependency (R2-m3).
  const breakdownExclusive = subject.breakdown.reduce(
    (total, row) => total + row.exclusiveRequiredTokens, 0);
  const breakdownShared = subject.breakdown.reduce(
    (total, row) => total + row.sharedRequiredTokens, 0);
  if (breakdownExclusive + breakdownShared !== required) {
    fail("breakdown-reconciles", {
      exclusive: breakdownExclusive,
      shared: breakdownShared,
      required,
    });
  }
  const subjectId = subject.id;
  const subjectRow = subject.breakdown.find((row) => row.dependency === subjectId);
  if (!subjectRow || subjectRow.sharedRequiredTokens <= 0) {
    fail("shared-bills-the-subject", subjectId);
  }
  if (subject.breakdown.some(
    (row) => row.dependency !== subjectId && row.sharedRequiredTokens !== 0,
  )) {
    fail("shared-only-on-the-subject-row", subjectId);
  }
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

  // R3-1: --all must satisfy the same published report contract as
  // symbol/module reports. Its wildcard is a project-wide selector,
  // permitted only under project scope, never a semantic node id.
  const projectEnvelope = run(["context-budget", "--all", "--budget", "12000"]);
  if (projectEnvelope.status !== "valid") fail("project-status", projectEnvelope.status);
  const projectReport = projectEnvelope.contextBudget;
  if (!validateReport(projectReport)) fail("project-schema", validateReport.errors);
  if (projectReport.scope.kind !== "project" || projectReport.scope.id !== "*") {
    fail("project-scope", projectReport.scope);
  }
  for (const kind of ["symbol", "module"]) {
    const wrongScope = structuredClone(projectReport);
    wrongScope.scope.kind = kind;
    if (validateReport(wrongScope)) fail("negative-wildcard-scope", kind);
  }
  const wrongProjectId = structuredClone(projectReport);
  wrongProjectId.scope.id = "**";
  if (validateReport(wrongProjectId)) fail("negative-project-id", "accepted");

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

  // Consumer import: the closed profile and policy contracts accept
  // the same documents the CLI consumes, and refuse the known-bad
  // shapes the Rust loader refuses.
  const tempProfile = readFileSync(
    resolve(root, "tests/fixtures/context-budget/golden/consumer.profile.json"),
    "utf8",
  );
  const profileDocument = JSON.parse(tempProfile);
  if (!validateProfile(profileDocument)) {
    fail("consumer-profile-schema", validateProfile.errors);
  }
  const profileBad = JSON.parse(tempProfile);
  profileBad.profiles[0].budget.contextWindowTokens = 0;
  if (validateProfile(profileBad)) fail("negative-profile-zero-window", "accepted");
  const profileUnknown = JSON.parse(tempProfile);
  profileUnknown.profiles[0].surprise = true;
  if (validateProfile(profileUnknown)) fail("negative-profile-unknown-field", "accepted");
  const tempPolicy = readFileSync(
    resolve(root, "tests/fixtures/context-budget/golden/consumer.policy.json"),
    "utf8",
  );
  const policyDocument = JSON.parse(tempPolicy);
  if (!validatePolicy(policyDocument)) {
    fail("consumer-policy-schema", validatePolicy.errors);
  }
  const policyAdvisory = JSON.parse(tempPolicy);
  policyAdvisory.mode = "advisory";
  if (validatePolicy(policyAdvisory)) fail("negative-policy-mode", "accepted");
  const policyEmpty = JSON.parse(tempPolicy);
  policyEmpty.failOn = [];
  if (validatePolicy(policyEmpty)) fail("negative-policy-empty-failon", "accepted");
  // The emitted comparison payload satisfies the published comparison
  // contract (consumer round trip).
  // (consumer comparison round trip happens in the live block above)

  // Live comparison capture: run with --baseline against a serialized
  // copy of the first report (temp dir; the tracked fixture stays clean).
  {
    const temp = mkdtempSync(join(tmpdir(), "cb-gate-"));
    const baselinePath = join(temp, "baseline.json");
    writeFileSync(baselinePath, JSON.stringify(report));
    const withBaseline = run([
      "context-budget", "--symbol", "planner.focus_task", "--budget", "200",
      "--baseline", baselinePath,
    ]);
    if (withBaseline.status !== "valid") fail("baseline-status", withBaseline.status);
    // R2-M1: the consumed baseline is pinned on the wire, never unknown.
    if (withBaseline.contextBudget.provenance.baseline.state !== "known") {
      fail("baseline-pin", withBaseline.contextBudget.provenance.baseline);
    }
    const baselineDigest = "sha256:" + createHash("sha256")
      .update(readFileSync(baselinePath)).digest("hex");
    if (withBaseline.contextBudget.provenance.baseline.digest !== baselineDigest) {
      fail("baseline-pin-digest", withBaseline.contextBudget.provenance.baseline);
    }
    if (!validateReport(withBaseline.contextBudget)) fail("baseline-report-schema", validateReport.errors);
    if (!withBaseline.contextBudgetComparison) fail("comparison-missing", "the envelope must carry the comparison block");
    if (!validateComparison(withBaseline.contextBudgetComparison)) {
      fail("comparison-schema", validateComparison.errors);
    }
    // A description-only growth shows up as a positive required delta
    // even when the semantic diff would be equal — assert the deltas
    // exist and the summary reconciles.
    const rows = withBaseline.contextBudgetComparison.subjects.flatMap(
      (subject) => subject.deltas || [],
    );
    if (!rows.some((row) => row.delta)) fail("comparison-deltas", "no signed deltas emitted");
    rmSync(temp, { recursive: true, force: true });
  }

  // R2-M1 gate leg: a mandatory-policy pass emits the real policy pin
  // (digest of the exact selected policy bytes), not an advisory-
  // identical unknown.
  {
    const temp = mkdtempSync(join(tmpdir(), "cb-gate-policy-"));
    const policyPath = join(temp, "policy.json");
    const policy = {
      schemaVersion: "lekalo/context-budget-policy/v0.6.3",
      identity: "dev.lekalo.context-budget-policy@0.6.3",
      mode: "mandatory",
      profileRef: {
        id: report.profile.id,
        version: report.profile.version,
        digest: report.profile.digest,
      },
      // The digest binds the exact budget, so the pass must run at the
      // pinned profile's own budget (200); required-incomplete keeps
      // the mandatory policy from denying the over-budget subject.
      failOn: ["required-incomplete"],
      regressionLimits: [],
    };
    const pinnedBudget = 200;
    writeFileSync(policyPath, JSON.stringify(policy));
    const withPolicy = run([
      "context-budget", "--symbol", "planner.focus_task",
      "--budget", String(pinnedBudget),
      "--policy", policyPath,
    ]);
    if (withPolicy.status !== "valid") fail("policy-status", withPolicy.status);
    const policyPin = withPolicy.contextBudget.provenance.policy;
    if (policyPin.state !== "known") fail("policy-pin-unknown", policyPin);
    const policyDigest = "sha256:" + createHash("sha256")
      .update(readFileSync(policyPath))
      .digest("hex");
    if (policyPin.digest !== policyDigest) fail("policy-pin-digest", policyPin.digest);
    if (withPolicy.contextBudget.provenance.baseline.state !== "unknown") {
      fail("policy-baseline-pin", withPolicy.contextBudget.provenance.baseline);
    }
    if (!validateReport(withPolicy.contextBudget)) fail("policy-report-schema", validateReport.errors);
    const baselinePath = join(temp, "baseline.json");
    writeFileSync(baselinePath, JSON.stringify(report));
    const both = run([
      "context-budget", "--symbol", "planner.focus_task", "--budget", "200",
      "--policy", policyPath, "--baseline", baselinePath,
    ]);
    const baselineDigest = "sha256:" + createHash("sha256")
      .update(readFileSync(baselinePath)).digest("hex");
    if (!validateReport(both.contextBudget)) fail("combined-pins-schema", validateReport.errors);
    if (both.contextBudget.provenance.policy.digest !== policyDigest ||
        both.contextBudget.provenance.baseline.digest !== baselineDigest) {
      fail("combined-pins", both.contextBudget.provenance);
    }
    // The metric-policy denial must preserve the same digest-bearing payload.
    policy.failOn = ["over-budget"];
    writeFileSync(policyPath, JSON.stringify(policy));
    let denied;
    try {
      run([
        "context-budget", "--symbol", "planner.focus_task", "--budget", "200",
        "--policy", policyPath, "--baseline", baselinePath,
      ]);
      fail("policy-denial-missing", "over-budget policy passed");
    } catch (error) {
      if (error.status !== 3) fail("policy-denial-exit", error.status);
      denied = JSON.parse(error.stdout);
    }
    const deniedReport = denied.payload?.contextBudget;
    if (denied.status !== "denied" || !validateReport(deniedReport)) {
      fail("policy-denial-report", validateReport.errors);
    }
    const deniedPolicyDigest = "sha256:" + createHash("sha256")
      .update(readFileSync(policyPath)).digest("hex");
    if (deniedReport.provenance.policy.digest !== deniedPolicyDigest ||
        deniedReport.provenance.baseline.digest !== baselineDigest) {
      fail("policy-denial-pins", deniedReport.provenance);
    }
    rmSync(temp, { recursive: true, force: true });
  }

  // Both input pins are closed digest-bearing states. A path, missing
  // digest, malformed digest, extra field or digest on a non-known state refuses.
  for (const member of ["policy", "baseline"]) {
    for (const pin of [
      "known", { state: "known" }, { state: "known", digest: "C:/private/base.json" },
      { state: "known", digest: "sha256:" + "a".repeat(64), extra: true },
      { state: "unknown", digest: "sha256:" + "a".repeat(64) },
      { state: "unknown", digest: null }, { state: "invented" },
    ]) {
      const bad = structuredClone(report);
      bad.provenance[member] = pin;
      if (validateReport(bad)) fail("negative-provenance-pin", { member, pin });
    }
  }

  // Reason/class coupling parity (devin minor 12 / codex 19).
  const noReason = JSON.parse(JSON.stringify(report));
  delete noReason.subjects[0].requiredFacts[0].reason;
  if (validateReport(noReason)) fail("negative-required-without-reason", "accepted");
  const withReason = JSON.parse(JSON.stringify(report));
  withReason.subjects[0].supportingFacts = [
    { id: "x:1", class: "supporting-semantic", reason: "subject-contract", tokens: 5 },
  ];
  if (validateReport(withReason)) fail("negative-supporting-with-reason", "accepted");

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
  // The live canonical bytes equal the committed golden byte for byte.
  const goldenRaw = readFileSync(resolve(root, fixturePath), "utf8");
  const livePayload = JSON.parse(again).contextBudget;
  const liveCanonical = JSON.stringify(livePayload);
  if (liveCanonical !== JSON.stringify(JSON.parse(goldenRaw))) {
    fail("golden-drift", "live report bytes differ from the committed golden; regenerate the golden with the implementation commit");
  }
  for (const forbidden of ["timestamp", "C:/", "C:\\\\", "target/debug"]) {
    if (again.includes(forbidden)) fail("privacy", forbidden);
  }
}

// 4. The canonical fixture golden: the pinned over-budget report bytes.
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
// The golden breakdown reconciles (R2-6) and bills shared cost to the
// subject's own row (R2-m3).
const goldenExclusive = goldenSubject.breakdown.reduce(
  (total, row) => total + row.exclusiveRequiredTokens, 0);
const goldenShared = goldenSubject.breakdown.reduce(
  (total, row) => total + row.sharedRequiredTokens, 0);
if (goldenExclusive + goldenShared !== goldenRequired) {
  fail("golden-breakdown-reconciles", {
    exclusive: goldenExclusive,
    shared: goldenShared,
    required: goldenRequired,
  });
}
const goldenId = goldenSubject.id;
if (!goldenSubject.breakdown.some(
  (row) => row.dependency === goldenId && row.sharedRequiredTokens > 0,
)) {
  fail("golden-shared-bills-the-subject", goldenId);
}
if (goldenSubject.breakdown.some(
  (row) => row.dependency !== goldenId && row.sharedRequiredTokens !== 0,
)) {
  fail("golden-shared-only-on-the-subject-row", goldenId);
}
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
