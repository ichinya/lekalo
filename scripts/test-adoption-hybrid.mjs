#!/usr/bin/env node
// Issue #136 hybrid gate (AC5, AC6, and the AC7 workflow replay): the
// real binary walks one synthetic brownfield project from a second
// canonical feature through the verified plan to the mixed-mode join.
// Covered here and nowhere else: the create/apply/rollback class over a
// created canonical document and a created source file (F1), the
// binding write entry restored exactly by rollback (F2), the delete
// write class end to end (F3), a non-evidence mutation applying (F4),
// the --project selection on the wrapper commands (F5), the drained
// verification pipes (F6), the recovery surface (F10), execution
// custody refusing a forged/replayed receipt (X1/X2), the live
// conformance freshness of the coverage join (X11), and the scrubbed
// child environment (X2). Runs after `cargo build --workspace
// --locked` in the same job; expected outcomes are independent
// assertions, never regenerated goldens.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};
const BINARY = join(root, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
if (!existsSync(BINARY)) fail("binary-missing", "cargo build --workspace --locked first");

const FIXTURE = "tests/fixtures/adoption/app";
const S = "tests/fixtures/adoption";
const sandboxBase = join(root, "target", "adoption-hybrid-sandbox");
rmSync(sandboxBase, { recursive: true, force: true });
mkdirSync(sandboxBase, { recursive: true });
const project = join(sandboxBase, "proj");
cpSync(join(root, FIXTURE), project, { recursive: true });
const scanner = join(root, S, "scanner/adoption-scanner.mjs");
const spec = join(root, S, "verification/verification-spec.json");

// The canonical JSON + digest recipes (independently implemented, the
// same closed grammar the core enforces).
const canonical = (value) => {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
};
const digest = (family, value) =>
  "sha256:" + createHash("sha256").update(`lekalo.${family}.v0.8.0\n${canonical(value)}`).digest("hex");
const sha256 = (bytes) => `sha256:${createHash("sha256").update(bytes).digest("hex")}`;

const run = (args, env) =>
  spawnSync(BINARY, args, { cwd: project, encoding: "utf8", maxBuffer: 64 * 1024 * 1024, env });
const runFrom = (cwd, args) =>
  spawnSync(BINARY, args, { cwd, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
const parse = (output) => {
  if (!output) fail("parse-undefined-output");
  const text = output.status === 1 || output.status === 5 ? output.stderr : output.stdout;
  if (text === undefined || text === null) fail("parse-undefined-text", { status: output.status, signal: output.signal, error: String(output.error) });
  return JSON.parse(text);
};
const save = (name, output) => {
  const path = join(sandboxBase, name);
  writeFileSync(path, output.stdout || output.stderr);
  return path;
};

// A reviewed candidate patch document for one authored change set.
const patchDocument = (symbol, files) => {
  const doc = {
    schema_version: "lekalo/adoption-patch/v0.8.0",
    identity: "dev.lekalo.adoption-patch@0.8.0",
    patch: "",
    origin: { kind: "supplied-reviewed", identity: "issue-136-hybrid-reviewed-patch" },
    scope: { symbol, target: "node-typescript", profile: "default" },
    files: files.map((file) => ({
      ...file,
      before: file.before ?? null,
      after: file.after ?? null,
      content: file.content ?? null,
    })),
  };
  const { patch, ...rest } = doc;
  return { ...doc, patch: digest("adoption-patch", rest) };
};

// ---------------------------------------------------------------------------
// Bootstrap: scan, module analysis, and the second canonical feature.
// ---------------------------------------------------------------------------
if (run(["--json", "scan", "--target", "node-typescript", "--profile", "default", "--project", ".", "node", scanner]).status !== 0) {
  fail("scan");
}
const analysisPath = save("analysis-module.json", run([
  "--no-cache", "--json", "adoption", "analyze",
  "--module", "taskboard", "--target", "node-typescript", "--profile", "default", "--project", ".",
]));

// The new hybrid feature: the audit_event canonical definition gains its
// maintained implementation through a reviewed plan (AC7 step 3).
const featureContent = `// Synthetic new hybrid feature for the issue #136 hybrid gate:
// the maintained implementation of the audit_event event.
export function completeAudit(task, actor) {
  if (typeof actor !== "string" || actor.length === 0) {
    throw new TypeError("invalid-actor");
  }
  return { id: task.id, state: task.state, actor, kind: "task.audited" };
}
`;
const featurePatch = patchDocument("taskboard.audit_event", [
  { path: "src/audit.mjs", action: "create", after: sha256(Buffer.from(featureContent)), content: featureContent },
]);
const featurePatchPath = join(sandboxBase, "feature-patch.json");
writeFileSync(featurePatchPath, JSON.stringify(featurePatch, null, 2));
const featureRun = run([
  "--no-cache", "--json", "adoption", "refactor",
  "--module", "taskboard", "--target", "node-typescript", "--profile", "default",
  "--verification", spec, "--candidate", featurePatchPath, "--project", ".",
]);
if (featureRun.status !== 0) fail("feature-refactor-exit", featureRun.stderr);
const featurePlan = parse(featureRun);
if (JSON.stringify(featurePlan.mutation_scope) !== JSON.stringify(["src/audit.mjs"])) {
  fail("feature-mutation-scope", featurePlan.mutation_scope);
}
const featurePlanPath = save("feature-plan.json", featureRun);
const featureVerify = run([
  "--json", "adoption", "verify", "--plan", featurePlanPath, "--verification", spec,
  "--candidate", featurePatchPath, "--project", ".",
]);
if (featureVerify.status !== 0) fail("feature-verify-exit", featureVerify.stderr);
const featureExecution = parse(featureVerify);
if (featureExecution.conclusion !== "preserved") fail("feature-verify", featureExecution.conclusion);
if (!featureExecution.custody || !featureExecution.custody.run) fail("feature-custody", featureExecution.custody);
const featureExecutionPath = save("feature-execution.json", featureVerify);const featureApply = run([
  "--json", "adoption", "apply", "--plan", featurePlanPath, "--verification", spec,
  "--candidate", featurePatchPath, "--confirm", featurePlan.plan, "--project", ".",
]);
if (featureApply.status !== 0) fail("feature-apply-exit", featureApply.stderr);
const featureReceipt = parse(featureApply);
if (featureReceipt.outcome !== "completed") fail("feature-apply", featureReceipt);
if (readFileSync(join(project, "src/audit.mjs"), "utf8") !== featureContent) fail("feature-bytes");

// ---------------------------------------------------------------------------
// X1/X2: a forged execution without custody never applies, and a genuine
// execution for one candidate never stands in for another.
// ---------------------------------------------------------------------------
const forged = { ...JSON.parse(readFileSync(featureExecutionPath, "utf8")) };
delete forged.custody;
const forgedPath = join(sandboxBase, "forged-execution.json");
writeFileSync(forgedPath, JSON.stringify(forged, null, 2));
const forgedApply = run([
  "--json", "adoption", "apply", "--plan", featurePlanPath, "--verification", forgedPath,
  "--candidate", featurePatchPath, "--confirm", featurePlan.plan, "--project", ".",
]);
if (forgedApply.status !== 1) fail("forged-apply-exit", forgedApply.status);
if (!parse(forgedApply).diagnostics.some((item) => item.id === "adoption.input-invalid")) {
  fail("forged-apply-diagnostic", parse(forgedApply).diagnostics);
}
if (readFileSync(join(project, "src/audit.mjs"), "utf8") !== featureContent) fail("forged-apply-wrote");
const replayedApply = run([
  "--json", "adoption", "apply", "--plan", featurePlanPath, "--verification", spec,
  "--candidate", featurePatchPath, "--confirm", featurePlan.plan, "--project", ".",
]);
// The replay refuses because the plan's pinned pre-state no longer holds
// (the apply already happened): a denied plan-stale, never a second
// completion over consumed evidence.
if (replayedApply.status !== 3) fail("replayed-apply-exit", replayedApply.status);
if (!parse(replayedApply).diagnostics.some((item) => item.code === "LEK-ADOPT-004")) {
  fail("replayed-apply-diagnostic", parse(replayedApply).diagnostics);
}

// Rollback of the created source file removes it again (F1 create class).
const featureReceiptPath = save("feature-receipt.json", featureApply);
const featureRollback = run([
  "--json", "adoption", "rollback", "--receipt", featureReceiptPath,
  "--confirm", featureReceipt.receipt, "--project", ".",
]);
if (featureRollback.status !== 0) fail("feature-rollback-exit", featureRollback.stderr);
if (existsSync(join(project, "src/audit.mjs"))) fail("feature-rollback-leftover", "created file not removed");
// Restore the exact planned feature bytes for the rest of the walk: the
// hybrid join and the delete stage operate on the applied feature.

// ---------------------------------------------------------------------------
// F6: a check emitting far more than the 64 KiB OS pipe buffer on both
// streams measures as passed within the deadline — never a false timeout.
// ---------------------------------------------------------------------------
const floodRunner = [
  "// The bounded flood runner: executes the pinned native test and the",
  "// behavior oracle, then floods both pipes past the 64 KiB OS buffer.",
  "// A non-draining runner wedges here and reports a false timeout.",
  'import { spawnSync } from "node:child_process";',
  'import process from "node:process";',
  "",
  "let failed = false;",
  "",
  'const testRun = spawnSync(process.execPath, ["--test", "test/tasks.test.mjs"], { encoding: "utf8" });',
  'if (testRun.status !== 0 || !testRun.stdout.includes("TASK-TEST-OK")) {',
  "  failed = true;",
  '  process.stdout.write("native-test-failed" + String.fromCharCode(10));',
  "} else {",
  '  process.stdout.write("TASK-TEST-OK" + String.fromCharCode(10));',
  "}",
  "",
  'const oracleRun = spawnSync(process.execPath, ["tools/oracle.mjs"], { encoding: "utf8" });',
  'if (oracleRun.status !== 0 || !oracleRun.stdout.includes("ORACLE-OK")) {',
  "  failed = true;",
  '  process.stdout.write("behavior-oracle-failed" + String.fromCharCode(10));',
  "} else {",
  '  process.stdout.write("ORACLE-OK" + String.fromCharCode(10));',
  "}",
  "",
  'const chunk = "x".repeat(4096) + String.fromCharCode(10);',
  "for (let i = 0; i < 40; i++) { process.stdout.write(chunk); process.stderr.write(chunk); }",
  "process.exit(failed ? 1 : 0);",
  "",
].join("\n");
writeFileSync(join(project, "tools/flood-verify.mjs"), floodRunner);
const floodSpec = {
  schema_version: "lekalo/adoption-verification/v0.8.0",
  identity: "dev.lekalo.adoption-verification@0.8.0",
  variant: "specification",
  verification: "flood-preservation",
  spec: null,
  plan: null,
  tool: { program: "node", args: ["tools/flood-verify.mjs"], digest: null },
  baseline_source: null,
  custody: null,
  checks: [
    { id: "tasks.native-test", kind: "native-test", expect_exit: 0, expect_stdout: ["TASK-TEST-OK"] },
    { id: "complete-task.behavior-oracle", kind: "behavior-oracle", expect_exit: 0, expect_stdout: ["ORACLE-OK"] },
  ],
  confinement: ["disposable-copy", "bounded-output"],
};
const floodSpecPath = join(sandboxBase, "flood-spec.json");
writeFileSync(floodSpecPath, JSON.stringify(floodSpec, null, 2));
// The delete candidate removes a scanned observed file whose symbol the
// fixture's scanner reports (src/audit.mjs was created above and scanned
// in; the second scan below records it as taskboard.complete_audit).
const auditLegacyPath = join(project, "src/audit.mjs");
const deletePatchPath = join(sandboxBase, "delete-patch.json");
writeFileSync(auditLegacyPath, featureContent);
// The delete candidate removes the created feature file again: the CAS
// pre-state is the exact current bytes of the file, and the patch
// digest is computed over the complete final document.
const deletePatch = patchDocument("taskboard.complete_audit", [
  { path: "src/audit.mjs", action: "delete", before: sha256(readFileSync(auditLegacyPath)), after: null, content: null },
]);
writeFileSync(deletePatchPath, JSON.stringify(deletePatch, null, 2));
const floodPlanRun = run([
  "--no-cache", "--json", "adoption", "refactor",
  "--module", "taskboard", "--target", "node-typescript", "--profile", "default",
  "--verification", floodSpecPath, "--candidate", deletePatchPath, "--project", ".",
]);
if (floodPlanRun.status !== 0) fail("flood-refactor-exit", floodPlanRun.stderr);
const floodPlanPath = save("flood-plan.json", floodPlanRun);
const floodStarted = Date.now();
const floodVerify = run([
  "--json", "adoption", "verify", "--plan", floodPlanPath, "--verification", floodSpecPath,
  "--candidate", deletePatchPath, "--project", ".",
]);
const floodSeconds = (Date.now() - floodStarted) / 1000;
if (floodVerify.status !== 0) fail("flood-verify-exit", floodVerify.stderr);
const floodExecution = parse(floodVerify);
if (floodExecution.conclusion !== "preserved") fail("flood-conclusion", floodExecution.conclusion);
if (floodSeconds >= 55) fail("flood-false-timeout", `${floodSeconds}s for a drained run`);

// X2 (environment): a host-only canary never reaches a verification child.
const envSpec = {
  ...floodSpec,
  verification: "env-preservation",
  tool: { program: "node", args: ["tools/env-probe.mjs"], digest: null },
  checks: [{ id: "env", kind: "semantic", expect_exit: 0, expect_stdout: ["LEAK=-"] }],
};
const envSpecPath = join(sandboxBase, "env-spec.json");
writeFileSync(envSpecPath, JSON.stringify(envSpec, null, 2));
writeFileSync(
  join(project, "tools/env-probe.mjs"),
  [
    '// Prints whether the host-only canary variable leaked into the',
    '// verification child: the cleared environment must keep it out.',
    'import process from "node:process";',
    'process.stdout.write("LEAK=" + (process.env.LEKALO_ENV_CANARY ?? "-") + String.fromCharCode(10));',
    "",
  ].join("\n"),
);
const envPlanRun = run([
  "--no-cache", "--json", "adoption", "refactor",
  "--module", "taskboard", "--target", "node-typescript", "--profile", "default",
  "--verification", envSpecPath, "--candidate", deletePatchPath, "--project", ".",
]);
if (envPlanRun.status !== 0) fail("env-refactor-exit", envPlanRun.stderr);
const envPlanPath = save("env-plan.json", envPlanRun);
const childEnv = { ...process.env, LEKALO_ENV_CANARY: "hybrid-canary-secret" };
const envVerify = spawnSync(
  BINARY,
  ["--json", "adoption", "verify", "--plan", envPlanPath, "--verification", envSpecPath, "--candidate", deletePatchPath, "--project", "."],
  { cwd: project, encoding: "utf8", maxBuffer: 64 * 1024 * 1024, env: childEnv },
);
if (envVerify.status !== 0) fail("env-verify-exit", envVerify.stderr);
const envExecution = JSON.parse(envVerify.stdout);
if (envExecution.conclusion !== "failed" && envExecution.conclusion !== "unavailable") {
  // A leaked canary makes the check fail (LEAK=<secret>), not pass.
  const envCheck = envExecution.baseline.checks[0];
  if (envCheck.status !== "failed") fail("env-leaked", envCheck);
}
if (!envExecution.baseline.checks.concat(envExecution.candidate.checks).every((check) => check.status === "passed")) {
  fail("env-check-failed", envExecution.baseline.checks.concat(envExecution.candidate.checks));
}

// ---------------------------------------------------------------------------
// F3: the delete write class end to end — CAS against the exact prior
// bytes, then the file is gone and the gates stay green.
// ---------------------------------------------------------------------------
const deletePlanRun = run([
  "--no-cache", "--json", "adoption", "refactor",
  "--module", "taskboard", "--target", "node-typescript", "--profile", "default",
  "--verification", spec, "--candidate", deletePatchPath, "--project", ".",
]);
if (deletePlanRun.status !== 0) fail("delete-refactor-exit", deletePlanRun.stderr);
const deletePlan = parse(deletePlanRun);
const deletePlanPath = save("delete-plan.json", deletePlanRun);
const deleteVerify = run([
  "--json", "adoption", "verify", "--plan", deletePlanPath, "--verification", spec,
  "--candidate", deletePatchPath, "--project", ".",
]);
if (deleteVerify.status !== 0) fail("delete-verify-exit", deleteVerify.stderr);
if (parse(deleteVerify).conclusion !== "preserved") fail("delete-verify", parse(deleteVerify).conclusion);
const deleteExecutionPath = save("delete-execution.json", deleteVerify);
const deleteApply = run([
  "--json", "adoption", "apply", "--plan", deletePlanPath, "--verification", spec,
  "--candidate", deletePatchPath, "--confirm", deletePlan.plan, "--project", ".",
]);
if (deleteApply.status !== 0) fail("delete-apply-exit", deleteApply.stderr);
const deleteReceipt = parse(deleteApply);
if (deleteReceipt.outcome !== "completed") fail("delete-apply", deleteReceipt);
if (existsSync(join(project, "src/audit.mjs"))) fail("delete-kept-file", "src/audit.mjs survived apply");
const deleteReceiptPath = save("delete-receipt.json", deleteApply);
const deleteRollback = run([
  "--json", "adoption", "rollback", "--receipt", deleteReceiptPath,
  "--confirm", deleteReceipt.receipt, "--project", ".",
]);
if (deleteRollback.status !== 0) fail("delete-rollback-exit", deleteRollback.stderr);
if (Buffer.compare(readFileSync(auditLegacyPath), Buffer.from(featureContent, "utf8")) !== 0) {
  fail("delete-rollback-bytes");
}
if (!existsSync(auditLegacyPath)) fail("delete-rollback-missing");

// ---------------------------------------------------------------------------
// F2: the confirmed binding survives neither the confirmation rollback
// nor a partial failure — the index is a first-class write entry.
// ---------------------------------------------------------------------------
const proposeRun = run(["--json", "bindings", "propose", "--project", "."]);
if (proposeRun.status !== 0) fail("propose-exit", proposeRun.stderr);
const proposal = parse(proposeRun).proposals.find((row) => row.symbol === "taskboard.complete_task");
if (!proposal) fail("proposal-missing", parse(proposeRun).proposals);
const candidate = proposal.candidates[0].native;
const bindAnalysis = save("analysis-bind.json", run([
  "--no-cache", "--json", "adoption", "analyze",
  "--module", "taskboard", "--target", "node-typescript", "--profile", "default", "--project", ".",
]));
const bindPreview = run([
  "--no-cache", "--json", "adoption", "confirm", "--analysis", bindAnalysis,
  "--definition", "taskboard.complete_task", "--binding", proposal.proposal,
  "--candidate", candidate, "--dry-run", "--project", ".",
]);
if (bindPreview.status !== 0) fail("bind-preview-exit", bindPreview.stderr);
const bindPlan = parse(bindPreview);
if (!bindPlan.selections.binding) fail("bind-plan-selection", bindPlan.selections);
const bindPlanPath = save("bind-plan.json", bindPreview);
const bindApply = run([
  "--json", "adoption", "confirm", "--plan", bindPlanPath, "--confirm", bindPlan.plan, "--project", ".",
]);
if (bindApply.status !== 0) fail("bind-apply-exit", bindApply.stderr);
const bindReceipt = parse(bindApply);
const writePaths = bindReceipt.writes.map((write) => write.action);
if (!writePaths.includes("binding")) fail("bind-write-entry", bindReceipt.writes);
const bindReceiptPath = save("bind-receipt.json", bindApply);
const bindRollback = run([
  "--json", "adoption", "rollback", "--receipt", bindReceiptPath,
  "--confirm", bindReceipt.receipt, "--project", ".",
]);
if (bindRollback.status !== 0) fail("bind-rollback-exit", bindRollback.stderr);
const listAfter = parse(run(["--json", "bindings", "list", "--project", "."]));
const restored = listAfter.bindings.find((row) => row.semantic === "taskboard.complete_task");
if (!restored || restored.binding === "confirmed") fail("bind-rollback-state", restored);
if (existsSync(join(project, "lekalo/modules/taskboard/commands.yaml"))) {
  fail("bind-rollback-leftover", "created canonical document not removed");
}

// ---------------------------------------------------------------------------
// F4: a patch mutating a scanned file outside the use-case evidence set
// still plans and applies — no structural plan-stale refusal.
// ---------------------------------------------------------------------------
const unusedContent = `// Synthetic out-of-evidence module the hybrid gate deletes (F4).
export const UNUSED_MARKER = true;
`;
writeFileSync(join(project, "src/unused.mjs"), unusedContent);
if (run(["--json", "scan", "--target", "node-typescript", "--profile", "default", "--project", ".", "node", scanner]).status !== 0) {
  fail("rescan");
}
const unusedPatch = patchDocument("taskboard.complete_task", [
  { path: "src/unused.mjs", action: "delete", before: sha256(Buffer.from(unusedContent)), after: null, content: null },
]);
const unusedPatchPath = join(sandboxBase, "unused-patch.json");
writeFileSync(unusedPatchPath, JSON.stringify(unusedPatch, null, 2));
const unusedPlanRun = run([
  "--no-cache", "--json", "adoption", "refactor",
  "--use-case", "complete_case", "--target", "node-typescript", "--profile", "default",
  "--verification", spec, "--candidate", unusedPatchPath, "--project", ".",
]);
if (unusedPlanRun.status !== 0) fail("unused-refactor-exit", unusedPlanRun.stderr);
const unusedPlan = parse(unusedPlanRun);
const unusedPlanPath = save("unused-plan.json", unusedPlanRun);
const unusedVerify = run([
  "--json", "adoption", "verify", "--plan", unusedPlanPath, "--verification", spec,
  "--candidate", unusedPatchPath, "--project", ".",
]);
if (unusedVerify.status !== 0) fail("unused-verify-exit", unusedVerify.stderr);
const unusedExecutionPath = save("unused-execution.json", unusedVerify);
const unusedApply = run([
  "--json", "adoption", "apply", "--plan", unusedPlanPath, "--verification", spec,
  "--candidate", unusedPatchPath, "--confirm", unusedPlan.plan, "--project", ".",
]);
if (unusedApply.status !== 0) fail("unused-apply-exit", unusedApply.stderr);
if (parse(unusedApply).outcome !== "completed") fail("unused-apply", parse(unusedApply));
if (existsSync(join(project, "src/unused.mjs"))) fail("unused-delete-kept");

// ---------------------------------------------------------------------------
// AC5: the three-mode fixture through the conformed registry — the
// adapted legacy command and the new contracted symbol coexist beside
// the untouched rows, and the executed provenance join carries exactly
// the receipts that really wrote a row's paths.
// ---------------------------------------------------------------------------
// A fresh confirmable analysis: the F4 rescan moved the observed index.
const conformAnalysis = save("analysis-conform.json", run([
  "--no-cache", "--json", "adoption", "analyze",
  "--module", "taskboard", "--target", "node-typescript", "--profile", "default", "--project", ".",
]));
const conformPreview = run([
  "--no-cache", "--json", "adoption", "confirm", "--analysis", conformAnalysis,
  "--definition", "taskboard.complete_task", "--dry-run", "--project", ".",
]);
if (conformPreview.status !== 0) fail("conform-preview-exit", conformPreview.stderr);
const conformPlan = parse(conformPreview);
const conformPlanPath = save("conform-plan.json", conformPreview);
const conformApply = run([
  "--json", "adoption", "confirm", "--plan", conformPlanPath, "--confirm", conformPlan.plan, "--project", ".",
]);
if (conformApply.status !== 0) fail("conform-apply-exit", conformApply.stderr);

// The adapted legacy command: the shipped declaration names the exact
// current bytes of src/tasks.mjs, so check passes now and fails after
// any drift.
cpSync(join(root, S, "contracts/declaration.json"), join(project, "declaration-legacy.json"));
const legacyUpdate = run(["--json", "contract", "update", "--declaration", "declaration-legacy.json", "--project", "."]);
if (legacyUpdate.status !== 0) fail("legacy-update-exit", legacyUpdate.stderr);
if (run(["--json", "contract", "attach", "taskboard.complete_task", "--native-test", "test/tasks.test.mjs#completeTask", "--project", "."]).status !== 0) {
  fail("legacy-attach");
}
if (run(["--json", "contract", "check", "--project", "."]).status !== 0) fail("legacy-check");

// The new contracted symbol: authored canonically, never observed by
// the scanner, declared over the exact created bytes.
const featureDecl = {
  schemaVersion: "lekalo/contracted-declaration/v0.4.0",
  adapter: { id: "lekalo-target-node-typescript", version: "0.2.16", digest: null },
  project: "taskboard",
  revision: "sha256:1361a56d4a5b8e7c1c9f0d2a3b4c5d6e7f8091a2b3c4d5e6f70819a2b3c4d5e6",
  symbols: [
    {
      id: "taskboard.audit_event",
      kind: "scalar",
      source: { path: "src/audit.mjs", line: 3 },
      fingerprint: sha256(readFileSync(join(project, "src/audit.mjs"))),
      signature: null,
      effects: [],
    },
  ],
  artifacts: [],
};
writeFileSync(join(project, "declaration-feature.json"), JSON.stringify(featureDecl, null, 2));
const featureUpdate = run(["--json", "contract", "update", "--declaration", "declaration-feature.json", "--project", "."]);
if (featureUpdate.status !== 0) fail("feature-update-exit", featureUpdate.stderr);
if (run(["--json", "contract", "attach", "taskboard.audit_event", "--native-test", "test/audit.test.mjs#completeAudit", "--project", "."]).status !== 0) {
  fail("feature-attach");
}
if (run(["--json", "contract", "check", "--project", "."]).status !== 0) fail("feature-check");

// The join proves the mixed-mode project: nonzero contracted-legacy AND
// contracted-new, untouched rows still present, and the audit row
// carries exactly the executed receipts whose writes touched its path.
const conformedCoverage = run(["--no-cache", "--json", "adoption", "coverage", "--module", "taskboard", "--project", "."]);
if (conformedCoverage.status !== 0) fail("conformed-coverage-exit", conformedCoverage.stderr);
const conformedDoc = parse(conformedCoverage);
const conformedCounts = conformedDoc.counts;
if (conformedCounts.contracted_legacy < 1 || conformedCounts.contracted_new < 1) {
  fail("three-modes", conformedCounts);
}
if (conformedCounts.observed + conformedCounts.unbound < 1) fail("untouched-rows", conformedCounts);
const auditRow = conformedDoc.rows.find((row) => row.symbol === "taskboard.audit_event");
if (!auditRow || auditRow.effective !== "contracted-new") {
  fail("contracted-new-row", auditRow);
}
const legacyRow = conformedDoc.rows.find((row) => row.symbol === "taskboard.complete_task");
if (!legacyRow || legacyRow.effective !== "contracted-legacy" || legacyRow.verification) {
  // No completed receipt has ever written src/tasks.mjs in this walk
  // (the feature/delete applies were rolled back): an executed
  // provenance grant here would be fabricated.
  fail("legacy-row-shape", legacyRow);
}

// ---------------------------------------------------------------------------
// F5: the wrapper commands honor the exact --project selection from an
// unrelated working directory.
// ----------------------------------------------------------------------------
const coverageOutside = runFrom(sandboxBase, [
  "--no-cache", "--json", "adoption", "coverage", "--module", "taskboard", "--project", "proj",
]);
if (coverageOutside.status !== 0) fail("coverage-project-exit", coverageOutside.stderr);
const coveragePath = save("coverage.json", coverageOutside);
const statusOutside = runFrom(sandboxBase, ["--json", "status", "--adoption", "coverage.json", "--project", "proj"]);
if (statusOutside.status !== 0) fail("status-project-exit", statusOutside.stderr);
const statusReport = parse(statusOutside);
if (!Array.isArray(statusReport.panels) || statusReport.panels.length === 0) fail("status-panels", statusReport);

// ---------------------------------------------------------------------------
// X11: the conformance freshness of the join — fresh now, qualified
// after drift, never stale-as-fresh.
// ---------------------------------------------------------------------------
const freshPanels = statusReport.panels.filter((panel) => panel.state === "current");
if (freshPanels.length === 0 || freshPanels.some((panel) => panel.freshness !== "fresh")) {
  fail("freshness-fresh", statusReport.panels);
}
// R8: freshness binds to content, not identity reuse. The new
// contracted source (src/audit.mjs) was never scanned into the
// observed index — only the conformed registry names it. Editing it
// makes the conformance gate refuse, and the SAVED coverage attachment
// must qualify its rows instead of presenting them fresh.
const auditBytes = readFileSync(join(project, "src/audit.mjs"));
writeFileSync(join(project, "src/audit.mjs"), `${auditBytes.toString("utf8")}\n// drifted new-contracted source\n`);
if (run(["--json", "contract", "check", "--project", "."]).status === 0) {
  fail("drifted-new-source-check-passed", "contract check must refuse over drifted conformed bytes");
}
const driftedNewStatus = runFrom(sandboxBase, ["--json", "status", "--adoption", "coverage.json", "--project", "proj"]);
if (driftedNewStatus.status !== 0) fail("drifted-new-status-exit", driftedNewStatus.stderr);
const driftedNewPanels = parse(driftedNewStatus).panels;
if (driftedNewPanels.some((panel) => panel.state === "current" && panel.freshness === "fresh")) {
  fail("drifted-new-stale-as-fresh", driftedNewPanels);
}
const driftedNewImpact = runFrom(sandboxBase, ["--json", "impact", "taskboard.audit_event", "--adoption", "coverage.json", "--project", "proj"]);
if (driftedNewImpact.status !== 0) fail("drifted-new-impact-exit", driftedNewImpact.stderr);
const driftedNewBoundary = parse(driftedNewImpact).boundaries.find((row) => row.symbol === "taskboard.audit_event");
if (!driftedNewBoundary || driftedNewBoundary.freshness === "fresh") {
  fail("drifted-new-impact-fresh", driftedNewBoundary);
}
// Restoring the exact bytes restores the fresh verdict — the drift was
// content, not identity.
writeFileSync(join(project, "src/audit.mjs"), auditBytes);
const restoredNewStatus = runFrom(sandboxBase, ["--json", "status", "--adoption", "coverage.json", "--project", "proj"]);
if (restoredNewStatus.status !== 0) fail("restored-new-status-exit", restoredNewStatus.stderr);
const restoredNewPanels = parse(restoredNewStatus).panels.filter((panel) => panel.state === "current");
if (restoredNewPanels.length === 0 || restoredNewPanels.some((panel) => panel.freshness !== "fresh")) {
  fail("restored-new-fresh", parse(restoredNewStatus).panels);
}

const coverage = JSON.parse(readFileSync(coveragePath, "utf8"));
if (!coverage.pins || !coverage.revision) fail("coverage-pins", { pins: !!coverage.pins, revision: coverage.revision });
writeFileSync(join(project, "src/limits.mjs"), `${readFileSync(join(project, "src/limits.mjs"), "utf8")}\n// hybrid drift\n`);
const staleRun = runFrom(sandboxBase, ["--json", "status", "--adoption", "coverage.json", "--project", "proj"]);
if (staleRun.status !== 0) fail("status-stale-exit", staleRun.stderr);
const staleReport = parse(staleRun);
if (staleReport.panels.some((panel) => panel.state === "current" && panel.freshness === "fresh")) {
  fail("freshness-stale-as-fresh", staleReport.panels);
}
if (staleReport.panels.some((panel) => panel.state === "current" && panel.freshness !== "unqualified")) {
  fail("freshness-unqualified", staleReport.panels);
}
writeFileSync(join(project, "src/limits.mjs"), readFileSync(join(root, FIXTURE, "src/limits.mjs")));

// ---------------------------------------------------------------------------
// X11 (live conformance + executed provenance): one verified apply of
// the reviewed tasks refactor both drifts the contracted legacy source
// and writes the receipt the coverage join must carry. The conformance
// gate refuses, a freshly regenerated attachment downgrades the row to
// conflicting with the typed gap, the status panel is stale, the trace
// join carries the real receipt digest — and the exact rollback
// restores the conformant state byte-for-byte.
// ---------------------------------------------------------------------------
const tasksBeforeDrift = readFileSync(join(project, "src/tasks.mjs"), "utf8");
const driftAnalysis = save("analysis-drift.json", run([
  "--no-cache", "--json", "adoption", "analyze",
  "--use-case", "complete_case", "--target", "node-typescript", "--profile", "default", "--project", ".",
]));
if (!existsSync(driftAnalysis)) fail("drift-analysis");
const driftCandidatePath = join(root, S, "refactor/candidate-patch.json");
const driftPlanRun = run([
  "--no-cache", "--json", "adoption", "refactor",
  "--use-case", "complete_case", "--target", "node-typescript", "--profile", "default",
  "--verification", spec, "--candidate", driftCandidatePath, "--project", ".",
]);
if (driftPlanRun.status !== 0) fail("drift-refactor-exit", driftPlanRun.stderr);
const driftPlan = parse(driftPlanRun);
const driftPlanPath = save("drift-plan.json", driftPlanRun);
const driftVerify = run([
  "--json", "adoption", "verify", "--plan", driftPlanPath, "--verification", spec,
  "--candidate", driftCandidatePath, "--project", ".",
]);
if (driftVerify.status !== 0) fail("drift-verify-exit", driftVerify.stderr);
if (parse(driftVerify).conclusion !== "preserved") fail("drift-verify", parse(driftVerify).conclusion);
const driftExecutionPath = save("drift-execution.json", driftVerify);
const driftApply = run([
  "--json", "adoption", "apply", "--plan", driftPlanPath, "--verification", spec,
  "--candidate", driftCandidatePath, "--confirm", driftPlan.plan, "--project", ".",
]);
if (driftApply.status !== 0) fail("drift-apply-exit", driftApply.stderr);
if (parse(driftApply).outcome !== "completed") fail("drift-apply", parse(driftApply));
const driftReceipt = parse(driftApply);

// The conformance gate now refuses: the declared legacy fingerprint no
// longer matches the applied bytes.
if (run(["--json", "contract", "check", "--project", "."]).status === 0) fail("drift-check-passed");
const driftedCoverage = run(["--no-cache", "--json", "adoption", "coverage", "--module", "taskboard", "--project", "."]);
if (driftedCoverage.status !== 0) fail("drifted-coverage-exit", driftedCoverage.stderr);
const driftedDoc = parse(driftedCoverage);
const driftedRow = driftedDoc.rows.find((row) => row.symbol === "taskboard.complete_task");
if (!driftedRow || driftedRow.effective !== "conflicting") fail("drift-conflicting", driftedRow);
if (!driftedDoc.gaps.some((gap) => gap.subject === "taskboard.complete_task" && gap.detail === "conformance-fingerprint-drift")) {
  fail("drift-gap", driftedDoc.gaps);
}
// The executed provenance join credits exactly this receipt: its write
// touched the row's recorded source path.
if (!driftedRow.verification || !driftedRow.verification.split(",").includes(driftReceipt.receipt)) {
  fail("drift-provenance-join", { row: driftedRow, receipt: driftReceipt.receipt });
}
const driftedCoveragePath = save("coverage-drifted.json", driftedCoverage);
const driftedStatus = runFrom(sandboxBase, ["--json", "status", "--adoption", "coverage-drifted.json", "--project", "proj"]);
if (driftedStatus.status !== 0) fail("drifted-status-exit", driftedStatus.stderr);
const driftedPanel = parse(driftedStatus).panels.find((panel) => panel.symbol === "taskboard.complete_task");
if (!driftedPanel || driftedPanel.freshness !== "stale") fail("drift-stale", driftedPanel);
const traceRun = run([
  "--json", "trace", "query", join(root, "tests/fixtures/trace/partial.trace.json"), "gaps",
  "--adoption", driftedCoveragePath,
]);
if (traceRun.status !== 0) fail("trace-exit", traceRun.stderr);
const traceReport = parse(traceRun);
if (!Array.isArray(traceReport.verification) || !traceReport.verification.includes(driftReceipt.receipt)) {
  fail("trace-verification-join", { verification: traceReport.verification, receipt: driftReceipt.receipt });
}
if (!traceReport.verification.every((digest) => /^sha256:[0-9a-f]{64}$/.test(digest))) {
  fail("trace-verification-shape", traceReport.verification);
}
if (!traceReport.stale.some((gap) => gap.subject === "taskboard.complete_task")) {
  fail("trace-stale-row", traceReport.stale);
}

// F5: trace query --adoption resolves the adoption context from the
// --project selector exactly like status/impact — never cwd alone.
const traceOutside = runFrom(sandboxBase, [
  "--json", "trace", "query", join(root, "tests/fixtures/trace/partial.trace.json"), "gaps",
  "--adoption", driftedCoveragePath, "--project", "proj",
]);
if (traceOutside.status !== 0) fail("trace-project-exit", traceOutside.stderr);
if (!Array.isArray(parse(traceOutside).verification)) fail("trace-project-join", traceOutside.stdout);
// The exact rollback restores the conformant state: the declared
// fingerprint matches the restored bytes again.
const driftReceiptPath = save("drift-receipt.json", driftApply);
const driftRollback = run([
  "--json", "adoption", "rollback", "--receipt", driftReceiptPath,
  "--confirm", driftReceipt.receipt, "--project", ".",
]);
if (driftRollback.status !== 0) fail("drift-rollback-exit", driftRollback.stderr);
if (Buffer.compare(readFileSync(join(project, "src/tasks.mjs")), Buffer.from(tasksBeforeDrift, "utf8")) !== 0) {
  fail("drift-rollback-bytes");
}
if (run(["--json", "contract", "check", "--project", "."]).status !== 0) fail("restored-check");

// ---------------------------------------------------------------------------
// F10: the durable recovery surface consumes the journaled state.
// ----------------------------------------------------------------------------
const recovery = run([
  "--json", "adoption", "recovery", "--digest", deletePlan.plan, "--project", ".",
]);
if (recovery.status !== 0) fail("recovery-exit", recovery.stderr);
const recoveryView = parse(recovery);
if (recoveryView.plan !== deletePlan.plan || !Array.isArray(recoveryView.steps)) {
  fail("recovery-shape", recoveryView);
}
// A completed, un-rolled-back receipt owns the restoration: the
// executable recovery defers to the rollback surface instead of
// restoring consumed evidence a second time. (The deletePlan journal
// was rewritten by its rollback phase, so the unused-plan receipt is
// the completed one.)
const recoveryRestoreCompleted = run([
  "--json", "adoption", "recovery", "--digest", unusedPlan.plan, "--restore", "--project", ".",
]);
if (recoveryRestoreCompleted.status !== 3) fail("recovery-restore-completed-exit", recoveryRestoreCompleted.status);
if (!parse(recoveryRestoreCompleted).diagnostics.some((item) => item.code === "LEK-ADOPT-009")) {
  fail("recovery-restore-completed-diagnostic", parse(recoveryRestoreCompleted).diagnostics);
}

rmSync(sandboxBase, { recursive: true, force: true });
process.stdout.write(`${JSON.stringify({ ok: true, gate: "adoption-hybrid" })}\n`);
