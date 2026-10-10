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

// Negative: the faulty candidate really fails its oracle. The good
// apply already mutated the tree, so the faulty run first plans fresh
// against the current bytes (the pre-execution staleness barrier
// refuses a plan whose pins no longer hold — exactly the X3 class),
// then verifies the faulty patch on top of the applied state.
const faultyRefactor = run([
  "--no-cache", "--json", "adoption", "refactor",
  "--use-case", "complete_case", "--target", "node-typescript", "--profile", "default",
  "--verification", spec, "--candidate", faulty, "--project", ".",
]);
if (faultyRefactor.status !== 0) fail("faulty-refactor-exit", faultyRefactor.stderr);
const faultyPlanPath = save("faulty-plan.json", faultyRefactor);
const faultyVerify = run([
  "--json", "adoption", "verify",
  "--plan", faultyPlanPath, "--verification", spec, "--candidate", faulty, "--project", ".",
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
  "--plan", faultyPlanPath, "--verification", faultyExecutionPath, "--candidate", faulty,
  "--confirm", parse(faultyRefactor).plan, "--project", ".",
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

// ---------------------------------------------------------------------------
// R2 adversarial probes: forged executions with recomputed custody (X1),
// consumed-input drift (X3), unenforceable confinement (X2), wrong-version
// exit classes (X9), receipt obstruction with journal-driven recovery
// (X5), whole-rollback snapshot preflight (X6), and the bounded temp
// hygiene (F7).
// ---------------------------------------------------------------------------
const canonicalJson = (value) => {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
};
const custodyOf = (document) =>
  `sha256:${createHash("sha256").update(`lekalo.adoption-verification-run.v0.8.0\n${canonicalJson(document)}`).digest("hex")}`;
const patchDigestOf = (document) =>
  `sha256:${createHash("sha256").update(`lekalo.adoption-patch.v0.8.0\n${canonicalJson(document)}`).digest("hex")}`;

// A fresh plan and a genuine preserved execution for the probes.
const probeRefactor = run([
  "--no-cache", "--json", "adoption", "refactor",
  "--use-case", "complete_case", "--target", "node-typescript", "--profile", "default",
  "--verification", spec, "--candidate", candidate, "--project", ".",
]);
if (probeRefactor.status !== 0) fail("probe-refactor-exit", probeRefactor.stderr);
const probePlan = parse(probeRefactor);
const probePlanPath = save("probe-plan.json", probeRefactor);
const probeVerify = run([
  "--json", "adoption", "verify", "--plan", probePlanPath, "--verification", spec,
  "--candidate", candidate, "--project", ".",
]);
if (probeVerify.status !== 0) fail("probe-verify-exit", probeVerify.stderr);
if (parse(probeVerify).conclusion !== "preserved") fail("probe-verify", parse(probeVerify).conclusion);
const probeExecutionPath = save("probe-execution.json", probeVerify);

// The forgery recipe is public: strip custody, mutate, re-sign. X1
// demands the classes that MUST refuse actually refuse.
const forge = (mutate) => {
  const doc = JSON.parse(readFileSync(probeExecutionPath, "utf8"));
  delete doc.custody;
  mutate(doc);
  doc.custody = { run: custodyOf(doc), measured_ms: 1 };
  return doc;
};
const forgedApply = (name, doc) => {
  const path = join(sandboxBase, name);
  writeFileSync(path, JSON.stringify(doc, null, 2));
  const result = run([
    "--json", "adoption", "apply", "--plan", probePlanPath, "--verification", path,
    "--candidate", candidate, "--confirm", probePlan.plan, "--project", ".",
  ]);
  return { result, json: parse(result) };
};
// Empty measurement sets never pass an exact 1:1 coverage demand.
const emptyForged = forgedApply("forged-empty.json", forge((doc) => {
  doc.baseline = { checks: [] };
  doc.candidate = { checks: [] };
}));
if (emptyForged.result.status !== 1 || !emptyForged.json.diagnostics.some((item) => item.code === "LEK-ADOPT-007")) {
  fail("forged-empty", { status: emptyForged.result.status, diagnostics: emptyForged.json.diagnostics });
}
if (Buffer.compare(originalBytes, readFileSync(join(project, "src/tasks.mjs"))) !== 0) fail("forged-empty-wrote");
// An asserted oracle beside a failed oracle measurement is re-derived
// and refused.
const oracleId = "complete-task.behavior-oracle";
const oracleInvented = forgedApply("forged-oracle.json", forge((doc) => {
  for (const side of [doc.baseline, doc.candidate]) {
    for (const check of side.checks) {
      if (check.id === oracleId) check.status = "failed";
    }
  }
  doc.oracle = { status: "passed", detail: "invented" };
}));
if (oracleInvented.result.status !== 1 || !oracleInvented.json.diagnostics.some((item) => item.code === "LEK-ADOPT-007")) {
  fail("forged-oracle", { status: oracleInvented.result.status, diagnostics: oracleInvented.json.diagnostics });
}
// A substituted check set refuses even when fully self-consistent: the
// declared obligations are the plan's preservation image, never the
// document's own (X1).
const substituted = forgedApply("forged-substituted.json", forge((doc) => {
  doc.checks = [{ id: "forged.obligation", kind: "semantic", expect_exit: 0, expect_stdout: [] }];
  doc.baseline = { checks: [{ id: "forged.obligation", status: "passed", exit: 0, detail: "ok" }] };
  doc.candidate = { checks: [{ id: "forged.obligation", status: "passed", exit: 0, detail: "ok" }] };
  doc.oracle = { status: "passed", detail: "forged" };
}));
if (substituted.result.status !== 1 || !substituted.json.diagnostics.some((item) => item.id === "adoption.input-invalid" && item.data && item.data.reason === "checks-mismatch")) {
  fail("forged-substituted", { status: substituted.result.status, diagnostics: substituted.json.diagnostics });
}
// The documented residual: a hand-forgery that recomputes every public
// member over a self-consistent envelope is a consistency pass, not an
// authenticity proof — apply completes, and the exact rollback owns the
// restoration. The tutorial and the module docs state this honestly.
const consistentForgery = forgedApply("forged-consistent.json", forge((doc) => {
  for (const side of [doc.baseline, doc.candidate]) {
    for (const check of side.checks) check.detail = "forged-but-consistent";
  }
}));
if (consistentForgery.result.status !== 0 || parse(consistentForgery.result).outcome !== "completed") {
  fail("forged-consistent", { status: consistentForgery.result.status });
}
const consistentReceipt = parse(consistentForgery.result);
const consistentReceiptPath = save("forged-consistent-receipt.json", consistentForgery.result);
const forgedRollback = run([
  "--json", "adoption", "rollback", "--receipt", consistentReceiptPath,
  "--confirm", consistentReceipt.receipt, "--project", ".",
]);
if (forgedRollback.status !== 0) fail("forged-rollback-exit", forgedRollback.stderr);
if (Buffer.compare(originalBytes, readFileSync(join(project, "src/tasks.mjs"))) !== 0) fail("forged-rollback-bytes");

// X3: a swapped runner or oracle after the run refuses at admission —
// the consumed argv bytes are re-derived from the live tree.
const oracleBytes = readFileSync(join(project, "tools/oracle.mjs"));
writeFileSync(join(project, "tools/oracle.mjs"), "// trivialized oracle\n");
const oracleDrift = forgedApply("probe-execution-reused.json", JSON.parse(readFileSync(probeExecutionPath, "utf8")));
if (oracleDrift.result.status !== 1 || !oracleDrift.json.diagnostics.some((item) => item.id === "adoption.input-invalid" && item.data && item.data.reason === "consumed-drift")) {
  fail("oracle-drift", { status: oracleDrift.result.status, diagnostics: oracleDrift.json.diagnostics });
}
writeFileSync(join(project, "tools/oracle.mjs"), oracleBytes);
const runnerBytes = readFileSync(join(project, "tools/verify.mjs"));
writeFileSync(join(project, "tools/verify.mjs"), `${runnerBytes.toString("utf8")}\n// drifted runner\n`);
const runnerDrift = forgedApply("probe-execution-reused.json", JSON.parse(readFileSync(probeExecutionPath, "utf8")));
if (runnerDrift.result.status !== 1 || !runnerDrift.json.diagnostics.some((item) => item.id === "adoption.input-invalid" && item.data && item.data.reason === "consumed-drift")) {
  fail("runner-drift", { status: runnerDrift.result.status, diagnostics: runnerDrift.json.diagnostics });
}
writeFileSync(join(project, "tools/verify.mjs"), runnerBytes);

// X2: a requested no-network confinement is refused as unenforceable —
// never silently dropped while the run proceeds unconfined.
const nnSpecValue = JSON.parse(readFileSync(spec, "utf8"));
nnSpecValue.confinement.push("no-network");
const nnSpecPath = join(sandboxBase, "nn-spec.json");
writeFileSync(nnSpecPath, JSON.stringify(nnSpecValue, null, 2));
const nnRefactor = run([
  "--no-cache", "--json", "adoption", "refactor",
  "--use-case", "complete_case", "--target", "node-typescript", "--profile", "default",
  "--verification", nnSpecPath, "--project", ".",
]);
if (nnRefactor.status !== 0) fail("nn-refactor-exit", nnRefactor.stderr);
const nnPlanPath = save("nn-plan.json", nnRefactor);
const nnVerify = run([
  "--json", "adoption", "verify", "--plan", nnPlanPath, "--verification", nnSpecPath,
  "--project", ".",
]);
if (nnVerify.status !== 3 || !parse(nnVerify).diagnostics.some((item) => item.code === "LEK-ADOPT-003")) {
  fail("nn-verify", { status: nnVerify.status, diagnostics: parse(nnVerify).diagnostics });
}

// X9: a wrong-version plan keeps its exit-5 class through the apply,
// verify, and recovery consumers; a wrong-version patch too.
const bumpVersion = (bytes, version) => {
  const doc = JSON.parse(bytes.toString("utf8"));
  doc.schema_version = version;
  return Buffer.from(JSON.stringify(doc, null, 2));
};
const bumpedPlanPath = join(sandboxBase, "bumped-plan.json");
writeFileSync(bumpedPlanPath, bumpVersion(readFileSync(probePlanPath), "lekalo/adoption-refactor-plan/v9.9.9"));
const bumpedExecutionPath = join(sandboxBase, "bumped-execution.json");
writeFileSync(bumpedExecutionPath, bumpVersion(readFileSync(probeExecutionPath), "lekalo/adoption-verification/v9.9.9"));
const bumpedPatchPath = join(sandboxBase, "bumped-patch.json");
writeFileSync(bumpedPatchPath, bumpVersion(readFileSync(candidate), "lekalo/adoption-patch/v9.9.9"));
for (const [name, args, expectedCode] of [
  ["apply-plan", ["--json", "adoption", "apply", "--plan", bumpedPlanPath, "--verification", probeExecutionPath, "--candidate", candidate, "--confirm", probePlan.plan, "--project", "."], "LEK-ADOPT-002"],
  ["verify-plan", ["--json", "adoption", "verify", "--plan", bumpedPlanPath, "--verification", spec, "--candidate", candidate, "--project", "."], "LEK-ADOPT-002"],
  ["recovery-plan", ["--json", "adoption", "recovery", "--plan", bumpedPlanPath, "--project", "."], "LEK-ADOPT-002"],
  ["apply-verification", ["--json", "adoption", "apply", "--plan", probePlanPath, "--verification", bumpedExecutionPath, "--candidate", candidate, "--confirm", probePlan.plan, "--project", "."], "LEK-ADOPT-002"],
  ["apply-patch", ["--json", "adoption", "apply", "--plan", probePlanPath, "--verification", probeExecutionPath, "--candidate", bumpedPatchPath, "--confirm", probePlan.plan, "--project", "."], "LEK-ADOPT-002"],
]) {
  const result = run(args);
  if (result.status !== 5 || !parse(result).diagnostics.some((item) => item.code === expectedCode)) {
    fail(`x9-${name}`, { status: result.status, diagnostics: parse(result).diagnostics });
  }
}

// X5: an obstructed receipt destination strands nothing — the refusal
// is the typed recovery contract and the journal-driven restore
// completes the transaction without the receipt.
const probePatched = JSON.parse(readFileSync(candidate, "utf8")).files[0].content;
const receiptHead = probePlan.plan.slice(7, 39);
const receiptTail = probePlan.plan.slice(39, 71);
const receiptObstruction = join(project, ".lekalo/import/adoption/receipts", receiptHead, `${receiptTail}.json`);
// The rolled-back probe receipt occupies the path; the rollback already
// consumed it, so the file gives way to the obstruction directory.
rmSync(receiptObstruction, { force: true });
mkdirSync(receiptObstruction, { recursive: true });
const obstructApply = run([
  "--json", "adoption", "apply", "--plan", probePlanPath, "--verification", probeExecutionPath,
  "--candidate", candidate, "--confirm", probePlan.plan, "--project", ".",
]);
if (obstructApply.status !== 1 || !parse(obstructApply).diagnostics.some((item) => item.code === "LEK-ADOPT-008")) {
  fail("obstruct-apply", { status: obstructApply.status, diagnostics: parse(obstructApply).diagnostics });
}
if (readFileSync(join(project, "src/tasks.mjs"), "utf8") !== probePatched) fail("obstruct-apply-bytes");
const obstructView = run(["--json", "adoption", "recovery", "--digest", probePlan.plan, "--project", "."]);
if (obstructView.status !== 0) fail("obstruct-view-exit", obstructView.stderr);
const obstructJson = parse(obstructView);
if (obstructJson.intact !== true || !obstructJson.steps.some((step) => step.state === "written")) {
  fail("obstruct-view", obstructJson);
}
const obstructRestore = run(["--json", "adoption", "recovery", "--digest", probePlan.plan, "--restore", "--project", "."]);
if (obstructRestore.status !== 0 || parse(obstructRestore).outcome !== "restored") {
  fail("obstruct-restore", { status: obstructRestore.status, detail: obstructRestore.stdout || obstructRestore.stderr });
}
if (Buffer.compare(originalBytes, readFileSync(join(project, "src/tasks.mjs"))) !== 0) fail("obstruct-restore-bytes");
const receiptsHome = join(project, ".lekalo/import/adoption/receipts");
for (const head of readdirSync(receiptsHome)) {
  for (const name of readdirSync(join(receiptsHome, head))) {
    if (name.endsWith(".tmp")) fail("orphan-tmp", name);
  }
}
rmSync(receiptObstruction, { recursive: true, force: true });

// X6: a corrupt later snapshot refuses the ENTIRE rollback — the first
// file is never restored ahead of the failure.
const limitsOriginal = readFileSync(join(project, "src/limits.mjs"), "utf8");
const limitsPatched = `${limitsOriginal}\n// reviewed: no behavior change\n`;
const twoPatch = {
  schema_version: "lekalo/adoption-patch/v0.8.0",
  identity: "dev.lekalo.adoption-patch@0.8.0",
  patch: "",
  origin: { kind: "supplied-reviewed", identity: "issue-136-two-file-probe" },
  scope: { symbol: "taskboard.complete_task", target: "node-typescript", profile: "default" },
  files: [
    {
      path: "src/limits.mjs",
      action: "replace",
      before: sha256(Buffer.from(limitsOriginal, "utf8")),
      after: sha256(Buffer.from(limitsPatched, "utf8")),
      content: limitsPatched,
    },
    JSON.parse(readFileSync(candidate, "utf8")).files[0],
  ],
};
const { patch: _twoPatchDigest, ...twoPatchRest } = twoPatch;
twoPatch.patch = patchDigestOf(twoPatchRest);
const twoPatchPath = join(sandboxBase, "two-patch.json");
writeFileSync(twoPatchPath, JSON.stringify(twoPatch, null, 2));
const twoRefactor = run([
  "--no-cache", "--json", "adoption", "refactor",
  "--use-case", "complete_case", "--target", "node-typescript", "--profile", "default",
  "--verification", spec, "--candidate", twoPatchPath, "--project", ".",
]);
if (twoRefactor.status !== 0) fail("two-refactor-exit", twoRefactor.stderr);
const twoPlan = parse(twoRefactor);
const twoPlanPath = save("two-plan.json", twoRefactor);
const twoVerify = run([
  "--json", "adoption", "verify", "--plan", twoPlanPath, "--verification", spec,
  "--candidate", twoPatchPath, "--project", ".",
]);
if (twoVerify.status !== 0 || parse(twoVerify).conclusion !== "preserved") fail("two-verify", twoVerify.stderr);
const twoApply = run([
  "--json", "adoption", "apply", "--plan", twoPlanPath, "--verification", save("two-execution.json", twoVerify),
  "--candidate", twoPatchPath, "--confirm", twoPlan.plan, "--project", ".",
]);
if (twoApply.status !== 0) fail("two-apply-exit", twoApply.stderr);
if (readFileSync(join(project, "src/limits.mjs"), "utf8") !== limitsPatched) fail("two-apply-limits");
// Corrupt the second-processed write's snapshot (the writes restore in
// patch order: limits first, tasks second).
const tasksSnapshotName = `${createHash("sha256").update("src/tasks.mjs").digest("hex").slice(0, 32)}.bin`;
const tasksSnapshot = join(
  project, ".lekalo/import/adoption/recovery",
  twoPlan.plan.slice(7, 39), twoPlan.plan.slice(39, 71), "snapshots", tasksSnapshotName,
);
const snapshotBytes = readFileSync(tasksSnapshot);
writeFileSync(tasksSnapshot, "corrupted-snapshot");
const twoReceipt = parse(twoApply);
const twoReceiptPath = save("two-receipt.json", twoApply);
const corruptRollback = run([
  "--json", "adoption", "rollback", "--receipt", twoReceiptPath,
  "--confirm", twoReceipt.receipt, "--project", ".",
]);
if (corruptRollback.status !== 1 || !parse(corruptRollback).diagnostics.some((item) => item.code === "LEK-ADOPT-008")) {
  fail("corrupt-rollback", { status: corruptRollback.status, diagnostics: parse(corruptRollback).diagnostics });
}
if (readFileSync(join(project, "src/limits.mjs"), "utf8") !== limitsPatched) fail("corrupt-rollback-partial-limits");
if (readFileSync(join(project, "src/tasks.mjs"), "utf8") !== probePatched) fail("corrupt-rollback-partial-tasks");
// An intact snapshot set restores both files exactly.
writeFileSync(tasksSnapshot, snapshotBytes);
const cleanRollback = run([
  "--json", "adoption", "rollback", "--receipt", twoReceiptPath,
  "--confirm", twoReceipt.receipt, "--project", ".",
]);
if (cleanRollback.status !== 0) fail("clean-rollback-exit", cleanRollback.stderr);
if (readFileSync(join(project, "src/limits.mjs"), "utf8") !== limitsOriginal) fail("clean-rollback-limits");
if (Buffer.compare(originalBytes, readFileSync(join(project, "src/tasks.mjs"))) !== 0) fail("clean-rollback-tasks");

// F7: disposable-copy preparation never leaks into the host temp area —
// not on an over-limit refusal and not as empty parents on success.
const tempScratch = join(sandboxBase, "temp-scratch");
mkdirSync(tempScratch, { recursive: true });
const tempEnv = { ...process.env, TEMP: tempScratch, TMP: tempScratch, TMPDIR: tempScratch };
const bigPath = join(project, "src/big-generated.mjs");
writeFileSync(bigPath, "// oversized generated blob\n".repeat(2_600_000));
const overVerify = spawnSync(BINARY, [
  "--json", "adoption", "verify", "--plan", probePlanPath, "--verification", spec,
  "--candidate", candidate, "--project", ".",
], { cwd: project, encoding: "utf8", maxBuffer: 64 * 1024 * 1024, env: tempEnv });
if (overVerify.status !== 1 || !JSON.parse(overVerify.stderr).diagnostics.some((item) => item.id === "adoption.input-invalid" && item.data && item.data.reason === "over-limit")) {
  fail("over-limit", { status: overVerify.status, detail: (overVerify.stderr || "").slice(0, 300) });
}
if (readdirSync(tempScratch).length !== 0) fail("temp-leak-on-failure", readdirSync(tempScratch));
rmSync(bigPath, { force: true });
const okVerify = spawnSync(BINARY, [
  "--json", "adoption", "verify", "--plan", probePlanPath, "--verification", spec,
  "--candidate", candidate, "--project", ".",
], { cwd: project, encoding: "utf8", maxBuffer: 64 * 1024 * 1024, env: tempEnv });
if (okVerify.status !== 0) fail("temp-ok-verify", (okVerify.stderr || "").slice(0, 300));
if (readdirSync(tempScratch).length !== 0) fail("temp-litter-on-success", readdirSync(tempScratch));

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
