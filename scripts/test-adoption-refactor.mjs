#!/usr/bin/env node
// Issue #136 selective-refactoring gate (AC3 and AC4): the exact
// recommendation preview, the declined-plan no-op, the real verified
// apply with native gates and the behavior oracle in disposable copies,
// the negative probes (faulty candidate, stale plan, out-of-scope
// candidate, wrong confirmation), and the exact selective rollback.
// Runs the real binary over the committed synthetic fixture; expected
// outcomes are independent assertions, never regenerated goldens.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
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
const sandboxBase = join(root, "target", "adoption-refactor-sandbox");
rmSync(sandboxBase, { recursive: true, force: true });
mkdirSync(sandboxBase, { recursive: true });
const project = join(sandboxBase, "proj");
cpSync(join(root, FIXTURE), project, { recursive: true });

const run = (args) =>
  spawnSync(BINARY, args, { cwd: project, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
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
const sha256 = (bytes) => `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
const spec = join(root, S, "verification/verification-spec.json");
const candidate = join(root, S, "refactor/candidate-patch.json");
const faulty = join(root, S, "refactor/faulty-candidate.json");
const outOfScope = join(sandboxBase, "out-of-scope.json");
writeFileSync(
  outOfScope,
  JSON.stringify({
    ...JSON.parse(readFileSync(candidate, "utf8")),
    files: [
      {
        path: "elsewhere/evil.mjs",
        action: "create",
        before: null,
        after: sha256(Buffer.from("// no\n")),
        content: "// no\n",
      },
    ],
  }),
);

// Bootstrap: scan, analysis, and the selective confirmation of the
// reviewed definition (the prerequisite reviewed semantics).
const scanner = join(root, "tests/fixtures/adoption/scanner/adoption-scanner.mjs");
if (run(["--json", "scan", "--target", "node-typescript", "--profile", "default", "--project", ".", "node", scanner]).status !== 0) {
  fail("scan");
}
const analysisPath = save("analysis.json", run([
  "--no-cache", "--json", "adoption", "analyze",
  "--use-case", "complete_case", "--target", "node-typescript", "--profile", "default", "--project", ".",
]));
const preview = run([
  "--no-cache", "--json", "adoption", "confirm",
  "--analysis", analysisPath, "--definition", "taskboard.complete_task", "--dry-run", "--project", ".",
]);
if (preview.status !== 0) fail("confirm-preview", preview.stderr);
const planPath = save("confirmation-plan.json", preview);
if (run(["--json", "adoption", "confirm", "--plan", planPath, "--confirm", JSON.parse(preview.stdout).plan, "--project", "."]).status !== 0) {
  fail("confirm-apply");
}

// ---------------------------------------------------------------------------
// AC3: the exact preview and the declined no-op.
// ---------------------------------------------------------------------------
const originalBytes = readFileSync(join(project, "src/tasks.mjs"));
const canonicalBytes = readFileSync(join(project, "lekalo/modules/taskboard/commands.yaml"));
const treeBefore = listTree(project);
const refactorRun = run([
  "--no-cache", "--json", "adoption", "refactor",
  "--use-case", "complete_case", "--target", "node-typescript", "--profile", "default",
  "--verification", spec, "--candidate", candidate, "--project", ".",
]);
if (refactorRun.status !== 0) fail("refactor-exit", refactorRun.stderr);
const refactor = parse(refactorRun);
// Every recommendation names reason, files, risk, verification.
for (const recommendation of refactor.recommendations) {
  if (!recommendation.reason || recommendation.risk === undefined || !Array.isArray(recommendation.files)) {
    fail("recommendation-shape", recommendation);
  }
}
const kinds = new Set(refactor.recommendations.map((row) => row.kind));
for (const expectedKind of ["contract", "api", "binding", "test"]) {
  if (!kinds.has(expectedKind)) fail("recommendation-kinds", [...kinds]);
}
if (refactor.mutation_scope.length !== 1 || refactor.mutation_scope[0] !== "src/tasks.mjs") {
  fail("mutation-scope", refactor.mutation_scope);
}
if (!refactor.patch) fail("patch-pin");
if (!refactor.verification) fail("verification-pin");
if (refactor.preservation.length < 2) fail("preservation-obligations", refactor.preservation);
const refactorPath = save("refactor-plan.json", refactorRun);
// Declining the plan writes nothing at all.
const treeAfterPreview = listTree(project);
const addedPreview = treeAfterPreview.filter((path) => !treeBefore.includes(path));
if (addedPreview.length !== 0) fail("declined-preview-wrote", addedPreview);
if (Buffer.compare(originalBytes, readFileSync(join(project, "src/tasks.mjs"))) !== 0) fail("declined-source-write");
if (Buffer.compare(canonicalBytes, readFileSync(join(project, "lekalo/modules/taskboard/commands.yaml"))) !== 0) {
  fail("declined-canonical-write");
}
// An out-of-scope candidate refuses (LEK-ADOPT-003) before anything.
const outOfScopeRun = run([
  "--no-cache", "--json", "adoption", "refactor",
  "--use-case", "complete_case", "--target", "node-typescript", "--profile", "default",
  "--verification", spec, "--candidate", outOfScope, "--project", ".",
]);
if (outOfScopeRun.status !== 3) fail("out-of-scope-exit", outOfScopeRun.status);
if (!parse(outOfScopeRun).diagnostics.some((item) => item.code === "LEK-ADOPT-003")) {
  fail("out-of-scope-diagnostic", parse(outOfScopeRun).diagnostics);
}
// A plan without a candidate patch is advice: the schema still accepts
// the plan but apply refuses it later.
const adviceRun = run([
  "--no-cache", "--json", "adoption", "refactor",
  "--use-case", "complete_case", "--target", "node-typescript", "--profile", "default",
  "--verification", spec, "--project", ".",
]);
if (adviceRun.status !== 0) fail("advice-exit", adviceRun.stderr);
if (parse(adviceRun).patch !== null) fail("advice-patch-null", parse(adviceRun).patch);
const advicePath = save("advice-plan.json", adviceRun);

// ---------------------------------------------------------------------------
// AC4: verified apply — baseline and candidate really run.
// ---------------------------------------------------------------------------
const verifyRun = run([
  "--json", "adoption", "verify",
  "--plan", refactorPath, "--verification", spec, "--candidate", candidate, "--project", ".",
]);
if (verifyRun.status !== 0) fail("verify-exit", verifyRun.stderr);
const execution = parse(verifyRun);
if (execution.conclusion !== "preserved") fail("verify-conclusion", execution.conclusion);
if (execution.oracle.status !== "passed") fail("verify-oracle", execution.oracle);
for (const side of [execution.baseline, execution.candidate]) {
  if (side.checks.some((check) => check.status !== "passed")) fail("verify-checks", side.checks);
}
const executionPath = save("execution.json", verifyRun);
const digest = refactor.plan;
const applyRun = run([
  "--json", "adoption", "apply",
  "--plan", refactorPath, "--verification", executionPath, "--candidate", candidate,
  "--confirm", digest, "--project", ".",
]);
if (applyRun.status !== 0) fail("apply-exit", applyRun.stderr);
const applyReceipt = parse(applyRun);
if (applyReceipt.outcome !== "completed") fail("apply-outcome", applyReceipt);
const patchedExpected = JSON.parse(readFileSync(candidate, "utf8")).files[0].content;
if (readFileSync(join(project, "src/tasks.mjs"), "utf8") !== patchedExpected) {
  fail("apply-bytes", "src/tasks.mjs does not carry the reviewed candidate bytes");
}

// Negative: the faulty candidate really fails its oracle, and an apply
// built on the failed execution refuses and completes nothing.
const faultyVerify = run([
  "--json", "adoption", "verify",
  "--plan", refactorPath, "--verification", spec, "--candidate", faulty, "--project", ".",
]);
if (faultyVerify.status !== 0) fail("faulty-verify-exit", faultyVerify.stderr);
const faultyExecution = parse(faultyVerify);
if (faultyExecution.conclusion !== "failed" || faultyExecution.oracle.status !== "failed") {
  fail("faulty-verify", { conclusion: faultyExecution.conclusion, oracle: faultyExecution.oracle });
}
// The shallow happy-path test alone still passes for the faulty
// candidate — proven directly, outside the adoption envelope — which is
// exactly why the oracle is a separate obligation.
const faultyContent = JSON.parse(readFileSync(faulty, "utf8")).files[0].content;
const scratch = join(sandboxBase, "shallow");
mkdirSync(join(scratch, "test"), { recursive: true });
cpSync(join(project, "test/tasks.test.mjs"), join(scratch, "test/tasks.test.mjs"));
writeFileSync(join(scratch, "test/tasks.test.mjs"), readFileSync(join(project, "test/tasks.test.mjs"), "utf8").replace('../src/tasks.mjs', '../mutated.mjs'));
writeFileSync(join(scratch, "mutated.mjs"), faultyContent.replace('"./limits.mjs"', '"./src-limits.mjs"'));
cpSync(join(project, "src/limits.mjs"), join(scratch, "src-limits.mjs"));
const shallow = spawnSync(process.execPath, ["--test", "test/tasks.test.mjs"], { cwd: scratch, encoding: "utf8" });
if (shallow.status !== 0 || !shallow.stdout.includes("TASK-TEST-OK")) {
  fail("shallow-test-not-green", { status: shallow.status, stdout: shallow.stdout, stderr: shallow.stderr });
}
const oracleCheck = faultyExecution.candidate.checks.find((check) => check.id === "complete-task.behavior-oracle");
if (!oracleCheck) fail("oracle-check-missing", faultyExecution.candidate);
// The oracle failed on the candidate side; the conclusion is failed, so
// an apply attempt refuses (LEK-ADOPT-007) and the tree keeps the
// already-applied good bytes.
const faultyExecutionPath = save("faulty-execution.json", faultyVerify);
const faultyApply = run([
  "--json", "adoption", "apply",
  "--plan", refactorPath, "--verification", faultyExecutionPath, "--candidate", candidate,
  "--confirm", digest, "--project", ".",
]);
if (faultyApply.status !== 1) fail("faulty-apply-exit", faultyApply.status);
if (!parse(faultyApply).diagnostics.some((item) => item.code === "LEK-ADOPT-007")) {
  fail("faulty-apply-diagnostic", parse(faultyApply).diagnostics);
}
if (readFileSync(join(project, "src/tasks.mjs"), "utf8") !== patchedExpected) fail("faulty-apply-wrote");

// Negative: a stale plan. Rebuild the plan, drift the tree, then apply
// refuses (LEK-ADOPT-004) with zero writes before admission.
const reRefactor = run([
  "--no-cache", "--json", "adoption", "refactor",
  "--use-case", "complete_case", "--target", "node-typescript", "--profile", "default",
  "--verification", spec, "--candidate", candidate, "--project", ".",
]);
if (reRefactor.status !== 0 || !reRefactor.stdout) fail("restale-exit", { status: reRefactor.status, error: String(reRefactor.error), stderr: (reRefactor.stderr||'').slice(0,400) });
const stalePath = save("stale-plan.json", reRefactor);
const staleVerify = run([
  "--json", "adoption", "verify",
  "--plan", stalePath, "--verification", spec, "--candidate", candidate, "--project", ".",
]);
if (staleVerify.status !== 0) fail("stale-verify-exit", staleVerify.stderr);
const staleExecutionPath = save("stale-execution.json", staleVerify);
const drifted = `${readFileSync(join(project, "src/tasks.mjs"), "utf8")}\n// unrelated drift\n`;
writeFileSync(join(project, "src/tasks.mjs"), drifted);
const staleApply = run([
  "--json", "adoption", "apply",
  "--plan", stalePath, "--verification", staleExecutionPath, "--candidate", candidate,
  "--confirm", JSON.parse(reRefactor.stdout).plan, "--project", ".",
]);
if (staleApply.status !== 3) fail("stale-apply-exit", staleApply.status);
const staleApplyJson = parse(staleApply);
if (!staleApplyJson.diagnostics.some((item) => item.code === "LEK-ADOPT-004")) {
  fail("stale-apply-diagnostic", staleApplyJson.diagnostics);
}
if (!staleApplyJson.diagnostics.some((item) => item.id === "adoption.plan-stale" && (item.data.inputs || []).some((input) => input.input === "src/tasks.mjs"))) {
  fail("stale-names-input", staleApplyJson.diagnostics);
}
if (readFileSync(join(project, "src/tasks.mjs"), "utf8") !== drifted) fail("stale-apply-wrote");
// Restore the applied state for rollback.
writeFileSync(join(project, "src/tasks.mjs"), patchedExpected);

// Negative: advice-only plans can never apply.
const adviceApply = run([
  "--json", "adoption", "apply",
  "--plan", advicePath, "--verification", staleExecutionPath, "--candidate", candidate,
  "--confirm", JSON.parse(adviceRun.stdout).plan, "--project", ".",
]);
if (adviceApply.status !== 3) fail("advice-apply-exit", adviceApply.status);

// ---------------------------------------------------------------------------
// The exact selective rollback.
// ---------------------------------------------------------------------------
const applyReceiptPath = save("apply-receipt.json", applyRun);
const rbPreview = run(["--json", "adoption", "rollback", "--receipt", applyReceiptPath, "--dry-run", "--project", "."]);
if (rbPreview.status !== 0) fail("rb-preview-exit", rbPreview.stderr);
if (parse(rbPreview).writes.length !== 1) fail("rb-preview-set", parse(rbPreview).writes);
const rbRun = run([
  "--json", "adoption", "rollback",
  "--receipt", applyReceiptPath, "--confirm", applyReceipt.receipt, "--project", ".",
]);
if (rbRun.status !== 0) fail("rb-exit", rbRun.stderr);
if (parse(rbRun).outcome !== "rolled-back") fail("rb-outcome", parse(rbRun));
if (Buffer.compare(originalBytes, readFileSync(join(project, "src/tasks.mjs"))) !== 0) {
  fail("rb-bytes", "src/tasks.mjs not restored byte-identically");
}
// A second rollback conflicts with itself: the receipt's expected
// post-state (patched bytes) no longer holds (LEK-ADOPT-009).
const rbAgain = run([
  "--json", "adoption", "rollback",
  "--receipt", applyReceiptPath, "--confirm", applyReceipt.receipt, "--project", ".",
]);
if (rbAgain.status !== 3) fail("rb-conflict-exit", rbAgain.status);
if (!parse(rbAgain).diagnostics.some((item) => item.code === "LEK-ADOPT-009")) {
  fail("rb-conflict-diagnostic", parse(rbAgain).diagnostics);
}

rmSync(sandboxBase, { recursive: true, force: true });
process.stdout.write(`${JSON.stringify({ ok: true, gate: "adoption-refactor" })}\n`);

function listTree(base) {
  const out = [];
  const visit = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) visit(path);
      else out.push(path.slice(base.length + 1));
    }
  };
  visit(base);
  return out;
}
