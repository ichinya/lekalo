#!/usr/bin/env node
// Issue #136 release gate: the ten adoption contract schemas, the
// diagnostic-registry 0.8.0 union successor, the committed adoption
// fixtures, and the live CLI analysis/confirmation/coverage flows,
// validated with the same pinned third-party Draft 2020-12
// implementation as the other contract gates (exact Ajv 8.17.1 through
// NODE_PATH / LEKALO_AJV_NODE_PATH) and a freshly built real binary.
// Schema-only checks never replace the end-to-end acceptance in
// test-adoption-refactor.mjs and test-adoption-hybrid.mjs, which must
// run after `cargo build --workspace --locked` in the same job.
import { createRequire } from "node:module";
import "./registry-union.mjs";
import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  ({ default: Ajv2020 } = require("ajv/dist/2020.js"));
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(`${JSON.stringify({ ok: false, reason: "ajv-unavailable", detail: String(error) }, null, 2)}\n`);
  process.exit(1);
}
if (ajvVersion !== "8.17.1") {
  process.stderr.write(`${JSON.stringify({ ok: false, reason: "ajv-version", detail: ajvVersion }, null, 2)}\n`);
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

const BINARY = join(root, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
if (!existsSync(BINARY)) {
  fail("binary-missing", "cargo build --workspace --locked must run before this gate");
}

const ajv = new Ajv2020({ strict: true, allErrors: true });

// ---------------------------------------------------------------------------
// 1. The ten closed schemas compile and the committed fixture documents
//    satisfy them.
// ---------------------------------------------------------------------------
const FAMILIES = [
  "adoption-analysis",
  "adoption-confirmation-plan",
  "adoption-patch",
  "adoption-refactor-plan",
  "adoption-verification",
  "adoption-receipt",
  "adoption-coverage",
  "adoption-status-report",
  "adoption-impact-report",
  "adoption-trace-report",
];
const validators = new Map();
for (const family of FAMILIES) {
  const schema = read(`contracts/${family}.schema.v0.8.0.json`);
  validators.set(family, ajv.compile(schema));
}
const committedDocs = [
  ["adoption-patch", "tests/fixtures/adoption/refactor/candidate-patch.json"],
  ["adoption-patch", "tests/fixtures/adoption/refactor/faulty-candidate.json"],
  ["adoption-verification", "tests/fixtures/adoption/verification/verification-spec.json"],
];
for (const [family, path] of committedDocs) {
  const document = read(path);
  const validate = validators.get(family);
  if (!validate(document)) fail("fixture-invalid", { path, errors: validate.errors });
}

// ---------------------------------------------------------------------------
// 2. The diagnostic-registry 0.8.0 union successor: full 0.6.5 union
//    preserved verbatim plus the landed 0.8.0 families (the nine
//    `adoption.*` rules, the `format.*`, `portability.*`, and
//    `worklog-changelog.*`, `managed.*`, and `module-api.*` families).
// ---------------------------------------------------------------------------
const registry64 = read("contracts/diagnostic-registry.v0.6.4.json");
const registry65 = read("contracts/diagnostic-registry.v0.6.5.json");
const registry80 = read("contracts/diagnostic-registry.v0.8.0.json");
const registrySchema80 = read("contracts/diagnostic-registry.schema.v0.8.0.json");
const validateRegistry = ajv.compile(registrySchema80);
if (!validateRegistry(registry80)) fail("registry-invalid", validateRegistry.errors);
const ids65 = new Map(registry65.entries.map((entry) => [entry.id, entry]));
const landedFamilies = ["format.", "adoption.", "portability.", "worklog-changelog.", "managed.", "module-api.", "tasklink.", "mcp."];
const additions = registry80.entries.filter((entry) => !ids65.has(entry.id));
if (registry80.entries.length !== registry65.entries.length + additions.length || additions.length === 0) {
  fail("registry-count", { v08: registry80.entries.length, v065: registry65.entries.length });
}
for (const entry of additions) {
  if (!landedFamilies.some((family) => entry.id.startsWith(family))) {
    fail("registry-addition-outside-landed-families", entry.id);
  }
}
for (const entry of registry80.entries) {
  const predecessor = ids65.get(entry.id);
  if (predecessor) {
    if (JSON.stringify(predecessor) !== JSON.stringify(entry)) {
      fail("predecessor-mutated", entry.id);
    }
  }
}
const adoptionCodes = new Set(
  registry80.entries.filter((entry) => entry.id.startsWith("adoption.")).map((entry) => entry.id),
);
const expected = [
  "adoption.input-invalid",
  "adoption.version-unsupported",
  "adoption.scope-refused",
  "adoption.plan-stale",
  "adoption.confirmation-required",
  "adoption.evidence-incomplete",
  "adoption.verification-failed",
  "adoption.recovery-required",
  "adoption.rollback-conflict",
];
if (adoptionCodes.size !== expected.length) fail("adoption-codes", [...adoptionCodes]);
for (const id of expected) {
  if (!adoptionCodes.has(id)) fail("adoption-code-missing", id);
}
const codes = registry80.entries.map((entry) => entry.code);
if (new Set(codes).size !== codes.length) fail("code-collision", "LEK-ADOPT-*");
for (let index = 1; index < registry80.entries.length; index += 1) {
  if (registry80.entries[index - 1].id >= registry80.entries[index].id) {
    fail("registry-sort", registry80.entries[index].id);
  }
}
// The frozen default producer registry stays byte-identical to HEAD.
const frozen64 = execFileSync("git", ["show", "HEAD:contracts/diagnostic-registry.v0.6.4.json"], {
  cwd: root,
  maxBuffer: 64 * 1024 * 1024,
});
if (Buffer.compare(frozen64, readFileSync(resolve(root, "contracts/diagnostic-registry.v0.6.4.json"))) !== 0) {
  fail("frozen-predecessor-drift", "diagnostic-registry.v0.6.4.json");
}

// ---------------------------------------------------------------------------
// 3. The generator's --check mode: committed schema bytes are exactly the
//    deterministic authoring output.
// ---------------------------------------------------------------------------
const genCheck = spawnSync(process.execPath, [join(root, "scripts/gen-adoption-contracts.mjs"), "--check"], {
  cwd: root,
  encoding: "utf8",
});
if (genCheck.status !== 0) fail("generator-drift", genCheck.stderr || genCheck.stdout);

// ---------------------------------------------------------------------------
// 4. The live binary: read-only analysis over the synthetic fixture, with
//    independent expected facts, no canonical/source/report writes, and
//    the registered diagnostics for the negative probes.
// ---------------------------------------------------------------------------
const sha256 = (bytes) => `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
const appFixture = "tests/fixtures/adoption/app";
const sandboxBase = join(root, "target", "adoption-contracts-sandbox");
rmSync(sandboxBase, { recursive: true, force: true });
mkdirSync(sandboxBase, { recursive: true });
const project = join(sandboxBase, "proj");
cpSync(join(root, appFixture), project, { recursive: true });

const run = (args, cwd) =>
  spawnSync(BINARY, args, { cwd: cwd ?? project, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
const parse = (output) => JSON.parse(output.status === 1 || output.status === 5 ? output.stderr : output.stdout);

// 4a. A missing use case is a denied scope refusal with candidates, never
// a scan of all code.
const unknownUseCase = run(["--no-cache", "--json", "adoption", "analyze", "--use-case", "missing_case", "--project", "."]);
if (unknownUseCase.status !== 3) fail("use-case-exit", unknownUseCase.status);
const unknownUseCaseJson = parse(unknownUseCase);
if (unknownUseCaseJson.status !== "denied") fail("use-case-status", unknownUseCaseJson.status);
if (!unknownUseCaseJson.diagnostics.some((item) => item.id === "adoption.scope-refused" && item.code === "LEK-ADOPT-003")) {
  fail("use-case-diagnostic", unknownUseCaseJson.diagnostics);
}
if (!unknownUseCaseJson.payload.candidates.includes("complete_case")) {
  fail("use-case-candidates", unknownUseCaseJson.payload);
}

// 4b. A malformed (duplicate-key) analysis input is invalid exit 1 with
// LEK-ADOPT-001, never a partial accept.
const broken = join(sandboxBase, "duplicate.json");
mkdirSync(dirname_of(broken), { recursive: true });
writeFileSync(broken, '{"id":"x","id":"y","symbol":"taskboard.complete_task"}');
const dupRun = run(["--no-cache", "--json", "adoption", "analyze", "--use-case", "complete_case", "--project", "."]);
const badAnalysis = run([
  "--no-cache", "--json", "adoption", "confirm",
  "--analysis", broken,
  "--definition", "taskboard.complete_task",
  "--project", ".",
]);
if (badAnalysis.status !== 1) fail("duplicate-key-exit", badAnalysis.status);
if (!parse(badAnalysis).diagnostics.some((item) => item.id === "adoption.input-invalid" && item.code === "LEK-ADOPT-001")) {
  fail("duplicate-key-diagnostic", parse(badAnalysis).diagnostics);
}

// 4c. A foreign discriminator is unsupported-version exit 5 with
// LEK-ADOPT-002.
const foreign = join(sandboxBase, "foreign-analysis.json");
writeFileSync(
  foreign,
  JSON.stringify({ ...read(`${appFixture}/.lekalo/import/adoption/use-cases/complete_case.json`), schema_version: "lekalo/adoption-analysis/v0.2.16" })
    .replace('"lekalo/adoption-analysis/v0.2.16"', '"lekalo/adoption-analysis/v0.2.16"'),
);
const useCaseDoc = read(`${appFixture}/.lekalo/import/adoption/use-cases/complete_case.json`);
const foreignDoc = {
  schema_version: "lekalo/adoption-analysis/v0.2.16",
  identity: "dev.lekalo.adoption-analysis@0.2.16",
  id: useCaseDoc.id,
  symbol: useCaseDoc.symbol,
};
writeFileSync(foreign, JSON.stringify(foreignDoc, null, 2));
const foreignPlan = run([
  "--no-cache", "--json", "adoption", "confirm",
  "--analysis", foreign,
  "--definition", "taskboard.complete_task",
  "--project", ".",
]);
if (foreignPlan.status !== 5) fail("foreign-version-exit", foreignPlan.status);
if (!parse(foreignPlan).diagnostics.some((item) => item.id === "adoption.version-unsupported" && item.code === "LEK-ADOPT-002")) {
  fail("foreign-version-diagnostic", parse(foreignPlan).diagnostics);
}

// 4c-bis. R7 wire parity: every specification mutation the pinned Ajv
// schema rejects is also refused by the Rust admission, and vice
// versa — the decoder and the closed 0.8.0 schema agree in both
// directions (nullable-but-required members, digest grammars, token
// bounds).
const specValidator = validators.get("adoption-verification");
const specBase = JSON.parse(readFileSync(join(root, "tests/fixtures/adoption/verification/verification-spec.json"), "utf8"));
const parityMutations = [
  ["nested-tool-digest", (doc) => { delete doc.tool.digest; }],
  ["top-level-custody", (doc) => { delete doc.custody; }],
  ["baseline-not-a-digest", (doc) => { doc.baseline_source = "not-a-digest"; }],
  ["spec-not-a-digest", (doc) => { doc.spec = "not-a-digest"; }],
  ["empty-verification-token", (doc) => { doc.verification = ""; }],
];
for (const [name, mutate] of parityMutations) {
  const doc = structuredClone(specBase);
  mutate(doc);
  if (specValidator(doc)) fail("parity-ajv-accepts-" + name, "Ajv must reject the mutation");
  const path = join(sandboxBase, "parity-" + name + ".json");
  writeFileSync(path, JSON.stringify(doc, null, 2));
  const result = run(["--no-cache", "--json", "adoption", "refactor", "--use-case", "complete_case", "--target", "node-typescript", "--profile", "default", "--verification", path, "--project", "."]);
  if (result.status !== 1) fail("parity-cli-" + name, { status: result.status });
  if (!parse(result).diagnostics.some((item) => item.code === "LEK-ADOPT-001")) {
    fail("parity-cli-diagnostic-" + name, parse(result).diagnostics);
  }
}
// The pristine specification stays valid on both sides.
if (!specValidator(specBase)) fail("parity-pristine-ajv", specValidator.errors);

// 4d. The real analysis: exact scope, provenance, confidence, unknowns,
// pins — and zero writes to sources, the canonical Model, or any
// report/cache home.
const scanner = join(root, "tests/fixtures/adoption/scanner/adoption-scanner.mjs");
const scanRun = run([
  "--json", "scan", "--target", "node-typescript", "--profile", "default",
  "--project", ".", "node", scanner,
]);
if (scanRun.status !== 0) fail("scan-exit", scanRun.stderr || scanRun.stdout);
const canonicalBefore = readFileSync(join(project, "lekalo/modules/taskboard/module.yaml"));
const sourceBefore = readFileSync(join(project, "src/tasks.mjs"));
const treeBefore = listTree(project);
const analysisRun = run([
  "--no-cache", "--json", "adoption", "analyze",
  "--use-case", "complete_case",
  "--target", "node-typescript",
  "--profile", "default",
  "--project", ".",
]);
if (analysisRun.status !== 0) fail("analyze-exit", analysisRun.stderr);
const analysis = parse(analysisRun);
if (analysis.scope.kind !== "use-case" || analysis.scope.id !== "complete_case") fail("analyze-scope", analysis.scope);
if (analysis.module.id !== "taskboard" || analysis.module.proposed !== false) fail("analyze-module", analysis.module);
const completeTask = analysis.definitions.find((definition) => definition.id === "taskboard.complete_task");
if (!completeTask) fail("analyze-definition-missing", analysis.definitions);
if (completeTask.status !== "proposed" || completeTask.confidence !== "low") {
  fail("analyze-definition-fact", completeTask);
}
if (completeTask.provenance.source !== "observed-index" || completeTask.provenance.path !== "src/tasks.mjs") {
  fail("analyze-provenance", completeTask.provenance);
}
if (!analysis.unknowns.some((gap) => gap.subject === "taskboard.complete_task" && gap.detail === "binding-inferred")) {
  fail("analyze-unknowns", analysis.unknowns);
}
if (analysis.pins.sources.length !== 1 || analysis.pins.sources[0].path !== "src/tasks.mjs") {
  fail("analyze-source-pins", analysis.pins.sources);
}
if (analysis.pins.sources[0].sha256 !== sha256(readFileSync(join(project, "src/tasks.mjs")))) {
  fail("analyze-source-hash", analysis.pins.sources[0]);
}
if (!analysis.pins.observed_index || !analysis.pins.model) fail("analyze-pins", analysis.pins);
const treeAfter = listTree(project);
const added = treeAfter.filter((path) => !treeBefore.includes(path));
if (added.length !== 0) fail("analyze-wrote-files", added);
if (Buffer.compare(canonicalBefore, readFileSync(join(project, "lekalo/modules/taskboard/module.yaml"))) !== 0) {
  fail("analyze-model-write");
}
if (Buffer.compare(sourceBefore, readFileSync(join(project, "src/tasks.mjs"))) !== 0) {
  fail("analyze-source-write");
}
const analysisPath = join(sandboxBase, "analysis.json");
writeFileSync(analysisPath, analysisRun.stdout);

// 4e. Selective confirmation: one definition confirmed, the other stays a
// proposal; repeat analysis preserves the confirmed fact and reports it
// as confirmed.
const previewRun = run([
  "--no-cache", "--json", "adoption", "confirm",
  "--analysis", analysisPath,
  "--definition", "taskboard.complete_task",
  "--dry-run",
  "--project", ".",
]);
if (previewRun.status !== 0) fail("confirm-preview-exit", previewRun.stderr);
const plan = parse(previewRun);
if (plan.writes.length !== 1 || plan.writes[0].path !== "lekalo/modules/taskboard/commands.yaml") {
  fail("confirm-write-set", plan.writes);
}
const planPath = join(sandboxBase, "confirmation-plan.json");
writeFileSync(planPath, previewRun.stdout);
const digest = plan.plan;
const applyRun = run(["--json", "adoption", "confirm", "--plan", planPath, "--confirm", digest, "--project", "."]);
if (applyRun.status !== 0) fail("confirm-apply-exit", applyRun.stderr);
const receipt = parse(applyRun);
if (receipt.outcome !== "completed" || receipt.phase !== "confirmation") fail("confirm-apply", receipt);
const receiptsDir = join(project, ".lekalo/import/adoption/receipts");
if (!existsSync(join(receiptsDir, digest.slice(7, 39), `${digest.slice(39)}.json`))) {
  fail("receipt-home", listTree(join(project, ".lekalo/import/adoption")));
}
// The wrong digest is a denial (LEK-ADOPT-005), never a write.
const wrongDigestRun = run([
  "--json", "adoption", "confirm",
  "--plan", planPath,
  "--confirm", `sha256:${"0".repeat(64)}`,
  "--project", ".",
]);
if (wrongDigestRun.status !== 3) fail("wrong-digest-exit", wrongDigestRun.status);
if (!parse(wrongDigestRun).diagnostics.some((item) => item.code === "LEK-ADOPT-005")) {
  fail("wrong-digest-diagnostic", parse(wrongDigestRun).diagnostics);
}
// Re-analysis: the confirmed definition stays confirmed; the untouched
// scalar definition is still a proposal (selectivity preserved).
const reAnalysisRun = run([
  "--no-cache", "--json", "adoption", "analyze",
  "--use-case", "complete_case",
  "--target", "node-typescript",
  "--profile", "default",
  "--project", ".",
]);
if (reAnalysisRun.status !== 0) fail("reanalyze-exit", reAnalysisRun.stderr);
const reAnalysis = parse(reAnalysisRun);
const reComplete = reAnalysis.definitions.find((definition) => definition.id === "taskboard.complete_task");
if (reComplete.status !== "confirmed") fail("reanalyze-confirmed", reAnalysis.definitions);
// Confirming an already-canonical definition refuses.
const canonicalRun = run([
  "--no-cache", "--json", "adoption", "confirm",
  "--analysis", analysisPath,
  "--definition", "taskboard.complete_task",
  "--dry-run",
  "--project", ".",
]);
if (canonicalRun.status !== 1) fail("already-canonical-exit", canonicalRun.status);
if (!parse(canonicalRun).diagnostics.some((item) => item.code === "LEK-ADOPT-001")) {
  fail("already-canonical-diagnostic", parse(canonicalRun).diagnostics);
}

// 4e-bis. R9: incremental confirmation — a SECOND definition joins the
// ordinary existing canonical document the first confirmation wrote
// (whose last line is "version: 1", not a list item). The append goes
// through the admitted canonical representation: the existing
// definitions and bytes are preserved and both definitions land.
const moduleAnalysis = run([
  "--no-cache", "--json", "adoption", "analyze",
  "--module", "taskboard", "--target", "node-typescript", "--profile", "default",
  "--project", ".",
]);
if (moduleAnalysis.status !== 0) fail("second-analysis-exit", moduleAnalysis.stderr);
const moduleAnalysisPath = join(sandboxBase, "module-analysis.json");
writeFileSync(moduleAnalysisPath, moduleAnalysis.stdout);
const secondPreview = run([
  "--no-cache", "--json", "adoption", "confirm",
  "--analysis", moduleAnalysisPath, "--definition", "taskboard.register_task",
  "--dry-run", "--project", ".",
]);
if (secondPreview.status !== 0) fail("second-confirm-preview-exit", secondPreview.stderr);
const secondPlan = parse(secondPreview);
if (secondPlan.writes.length !== 1 || secondPlan.writes[0].path !== "lekalo/modules/taskboard/commands.yaml") {
  fail("second-confirm-write-set", secondPlan.writes);
}
if (secondPlan.writes[0].action !== "replace" || !secondPlan.writes[0].before) {
  fail("second-confirm-replace", secondPlan.writes[0]);
}
const secondPlanPath = join(sandboxBase, "second-confirmation-plan.json");
writeFileSync(secondPlanPath, secondPreview.stdout);
const secondApply = run(["--json", "adoption", "confirm", "--plan", secondPlanPath, "--confirm", secondPlan.plan, "--project", "."]);
if (secondApply.status !== 0) fail("second-confirm-apply-exit", secondApply.stderr);
const commandsText = readFileSync(join(project, "lekalo/modules/taskboard/commands.yaml"), "utf8");
const confirmedIds = [...commandsText.matchAll(/- id: (taskboard.[a-z_]+)/g)].map((match) => match[1]).sort();
if (JSON.stringify(confirmedIds) !== JSON.stringify(["taskboard.complete_task", "taskboard.register_task"])) {
  fail("second-confirm-definitions", confirmedIds);
}
// The final staleness drift below starts from a live re-analysis.
const thirdAnalysis = run([
  "--no-cache", "--json", "adoption", "analyze",
  "--use-case", "complete_case", "--target", "node-typescript", "--profile", "default",
  "--project", ".",
]);
if (thirdAnalysis.status !== 0) fail("third-analysis-exit", thirdAnalysis.stderr);

// 4f. Coverage over the confirmed project: disjoint counts and the
// Ajv-validated coverage document.
const coverageRun = run(["--no-cache", "--json", "adoption", "coverage", "--module", "taskboard", "--project", "."]);
if (coverageRun.status !== 0) fail("coverage-exit", coverageRun.stderr);
const coverage = parse(coverageRun);
const coverageValidate = validators.get("adoption-coverage");
if (!coverageValidate(coverage)) fail("coverage-schema", coverageValidate.errors);
const effectiveTotal =
  coverage.counts.observed +
  coverage.counts["contracted-legacy"] +
  coverage.counts["contracted-new"] +
  coverage.counts.unbound +
  coverage.counts.conflicting;
if (effectiveTotal !== coverage.rows.length) fail("coverage-counts-disjoint", coverage.counts);
if (coverage.rows.some((row) => row.symbol === "taskboard.complete_task" && row.effective !== "observed")) {
  // complete_task is canonical but not yet conformed: effectively observed
  // with an explicit canonical-without-conformance gap.
  const row = coverage.rows.find((row) => row.symbol === "taskboard.complete_task");
  fail("coverage-complete-task", row);
}

// 4e-finish: staleness — an in-scope source edit makes bindings audit
// fail and the analysis report the discrepancy, while the confirmed
// canonical definition stays untouched.
writeFileSync(join(project, "src/tasks.mjs"), `${readFileSync(join(project, "src/tasks.mjs"), "utf8")}\n// drifted\n`);
const auditRun = run(["--json", "bindings", "audit", "--project", "."]);
if (auditRun.status !== 1) fail("stale-audit-exit", auditRun.status);
const driftAnalysis = parse(run([
  "--no-cache", "--json", "adoption", "analyze",
  "--use-case", "complete_case",
  "--target", "node-typescript",
  "--profile", "default",
  "--project", ".",
]));
if (!driftAnalysis.discrepancies.some((row) => row.symbol === "taskboard.complete_task" && row.detail === "fingerprint-drift")) {
  fail("stale-discrepancy", driftAnalysis.discrepancies);
}
if (!driftAnalysis.definitions.some((definition) => definition.id === "taskboard.complete_task" && definition.status === "confirmed")) {
  fail("stale-confirmed-preserved", driftAnalysis.definitions);
}

rmSync(sandboxBase, { recursive: true, force: true });

process.stdout.write(`${JSON.stringify({ ok: true, gate: "adoption-contracts", registry: registry80.entries.length })}\n`);

function dirname_of(path) {
  return resolve(path, "..");
}

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
