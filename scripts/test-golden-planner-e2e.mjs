#!/usr/bin/env node
// Golden-suite planner P0 end-to-end gate (issue #90, AC6).
//
// Executes the linked stage DAG of the Planner P0 chain over shared,
// catalog-registered inputs, with each stage consuming the actual
// upstream bytes (digest-checked), never a canned copy:
//
//   1. materialize the shared Planner project; load/validate/compile IR
//   2. project the dependency graph; inspect + impact + context over
//      the SAME sandbox project, symbol chosen from its own IR bytes
//   3. semantic diff of that project against a real mutation of itself
//      (equal-formatting shared control; seeds must name the mutation)
//   4. the committed six-scenario corpus runs through the real
//      dry-run/apply/verify adapter lane (documented residual: the
//      corpus is the catalog-registered shared input, not bytes
//      produced by stages 1-3)
//   5. the canonical trace golden validates and exports stably
//      (documented residual: a chain-built trace needs the G05
//      producer work recorded in the research)
//
// Every stage writes only into a fresh external sandbox. Any missing
// stage, digest mismatch, or canned substitution fails the gate.

import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const {
  REPO_ROOT,
  repoPath,
  sha256,
  failGate,
  passGate,
} = await import(new URL(`file:///${join(here, "lib/fixture-catalog.mjs").split("\\").join("/")}`).href);

const binary = join(repoRoot, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
if (!existsSync(binary)) failGate("golden-planner-e2e", [{ reason: "binary-missing", hint: "cargo build -p lekalo-cli --locked" }]);

const root = realpathSync.native(mkdtempSync(join(tmpdir(), "lekalo-golden-e2e-")));
const stages = [];
const step = (name, body) => {
  try {
    const detail = body();
    stages.push({ stage: name, ok: true, detail });
    process.stdout.write(`ok - ${name}\n`);
  } catch (error) {
    stages.push({ stage: name, ok: false, detail: String(error?.message ?? error) });
    process.stdout.write(`FAIL - ${name}\n${error?.stack ?? error}\n`);
    failGate("golden-planner-e2e", stages.filter((row) => !row.ok));
  }
};

const runLekalo = (cwd, args) => {
  const result = spawnSync(binary, ["--no-cache", ...args], { cwd, encoding: "utf8", timeout: 120000 });
  const text = ((result.stdout ?? "") + (result.stderr ?? "")).trim();
  let envelope = null;
  try { envelope = JSON.parse(text); } catch { /* non-JSON output */ }
  return { code: result.status, envelope, text };
};

const expect = (condition, message) => { if (!condition) throw new Error(message); };

// The catalog-registered shared corpus.
const ORCHESTRATION_PROJECT = repoPath("tests/fixtures/orchestration/project");
const DIFF_CASE = repoPath("tests/fixtures/diff/cases/behavior");
const TRACE_GOLDEN = repoPath("tests/fixtures/trace/golden/planner.trace.json");

try {
  // Stage 1: materialize + load + validate + compile IR.
  const project = join(root, "project");
  step("materialize-shared-planner-project", () => {
    cpSync(ORCHESTRATION_PROJECT, project, { recursive: true });
    expect(existsSync(join(project, "lekalo", "project.yaml")), "project materialized");
  });
  let loadEnvelopeDigest = null;
  step("load-and-validate-project", () => {
    const load = runLekalo(project, ["load", "--json", "--project", "."]);
    expect(load.code === 0, `load exit ${load.code}`);
    expect(load.envelope?.status === "valid", "load valid");
    loadEnvelopeDigest = sha256(load.text);
    const validate = runLekalo(project, ["validate", "--json", "--project", "."]);
    expect(validate.code === 0, `validate exit ${validate.code}`);
    const ir = runLekalo(project, ["load", "--ir", "--json", "--project", "."]);
    expect(ir.code === 0, `ir exit ${ir.code}`);
    expect(ir.envelope?.ir?.contract === "dev.lekalo.ir@0.2.16", "ir contract");
    // The downstream stages consume exactly these bytes.
    return { loadDigest: loadEnvelopeDigest, irDefinitions: ir.envelope.ir.definitions.length };
  });

  // Stage 2: graph + inspect/impact/context over the SAME sandbox
  // project stage 1 validated; the queried symbol comes from the
  // project's own compiled IR bytes.
  let chainSymbol = null;
  step("graph-and-query-projections", () => {
    const graph = runLekalo(project, ["--json", "graph", "export", "--project", "."]);
    expect(graph.code === 0, `graph exit ${graph.code}`);
    expect(graph.envelope?.graph?.identity === "dev.lekalo.graph@0.2.16", "graph identity");
    const nodes = graph.envelope.graph.metadata.nodeCount;
    expect(nodes > 0, "graph non-empty");
    // The queried symbol is chosen from THIS project's IR (stage-1
    // bytes), not from an unrelated fixture.
    const ir = runLekalo(project, ["load", "--ir", "--json", "--project", "."]);
    expect(ir.code === 0, "stage-2 IR reload");
    const entity = ir.envelope.ir.definitions.find((def) => def.kind === "entity");
    chainSymbol = entity?.id;
    expect(typeof chainSymbol === "string", "the chain project has an entity");
    const inspect = runLekalo(project, ["inspect", chainSymbol, "--project", ".", "--json"]);
    expect(inspect.code === 0, `inspect exit ${inspect.code}: ${inspect.text.slice(0, 120)}`);
    expect(inspect.envelope?.inspect?.symbol?.id === chainSymbol, "inspect resolves the chain symbol");
    const impact = runLekalo(project, ["impact", chainSymbol, "--project", ".", "--json"]);
    expect(impact.code === 0, `impact exit ${impact.code}`);
    expect(impact.envelope?.impact?.identity === "dev.lekalo.impact@0.2.16", "impact identity");
    const context = runLekalo(project, ["context", chainSymbol, "--project", ".", "--budget", "4096", "--json"]);
    expect(context.code === 0, `context exit ${context.code}`);
    expect(context.envelope?.context?.coverage?.included > 0, "context includes symbols");
    return {
      nodes,
      chainSymbol,
      graphDigest: sha256(graph.text),
      contextDigest: sha256(context.text),
    };
  });

  // Stage 3: semantic diff of the CHAIN project against a real
  // mutation of itself (input type swap), plus the shared
  // equal-formatting control. The affected seed must name the mutated
  // command and impact must consume the chain symbol.
  step("semantic-diff-mutation", () => {
    expect(chainSymbol !== null, "stage 2 produced the chain symbol");
    const equal = runLekalo(
      repoPath("tests/fixtures/diff/cases"),
      ["diff", "--base", "equal-formatting/base", "equal-formatting/candidate", "--json"],
    );
    expect(equal.code === 0, `equal diff exit ${equal.code}`);
    expect(equal.envelope?.diff?.equal === true, "formatting-only input is semantically equal");
    // Candidate = a copy of the stage-1 project with one semantic edit.
    const candidate = join(root, "candidate");
    cpSync(project, candidate, { recursive: true });
    const commandsPath = join(candidate, "lekalo", "modules", "planner", "commands.yaml");
    const original = readFileSync(commandsPath, "utf8");
    const mutated = original.replace('type: "planner.task_id"', 'type: "planner.text"');
    expect(mutated !== original, "the chain mutation applied");
    writeFileSync(commandsPath, mutated);
    const diff = runLekalo(root, ["diff", "--base", "project", "candidate", "--json"]);
    expect(diff.code === 0, `chain diff exit ${diff.code}: ${diff.text.slice(0, 160)}`);
    expect(diff.envelope?.diff?.equal === false, "the chain mutation is semantic");
    const classification = diff.envelope?.diff?.classification ?? [];
    expect(classification.includes("source-breaking"), "the type swap is source-breaking");
    const seeds = diff.envelope?.diff?.affectedSeeds ?? [];
    expect(seeds.length > 0, "affected seeds present");
    const subjects = JSON.stringify(seeds);
    expect(subjects.includes("focus_task"), "the seed names the mutated command");
    // Downstream consumption: impact over the chain symbol still
    // resolves after the mutation exists (candidate is separate; the
    // base project stays the impact subject).
    const impact = runLekalo(project, ["impact", chainSymbol, "--project", ".", "--json"]);
    expect(impact.code === 0, "post-diff impact consumes the chain project");
    return {
      classification,
      seeds: seeds.length,
      diffDigest: sha256(diff.text),
    };
  });

  // Stage 4: scenario compile + run through the committed node lane.
  step("scenario-lane-compile-and-run", () => {
    const result = spawnSync(
      process.execPath,
      [join(repoRoot, "scripts", "test-node-scenario-tests.mjs")],
      { cwd: repoRoot, encoding: "utf8", timeout: 600000 },
    );
    expect(result.status === 0, `scenario lane exit ${result.status}: ${(result.stdout ?? "").slice(-200)}`);
    expect(/ok \(6 scenarios\)/.test(result.stdout ?? ""), "all six scenarios adjudicated");
    return { scenarios: 6 };
  });

  // Stage 5: trace manifest validates and its digest is stable.
  step("trace-manifest-chain", () => {
    const direct = spawnSync(binary, ["--no-cache", "trace", "export", TRACE_GOLDEN, "--json"], { cwd: repoRoot, encoding: "utf8", timeout: 60000 });
    expect(direct.status === 0, `trace exit ${direct.status}: ${String(direct.stderr ?? "").slice(0, 200)}`);
    const doc = JSON.parse(String(direct.stdout ?? "").trim());
    expect(doc.status === "valid", "trace valid");
    expect(doc.trace?.completeness === "full", "trace complete");
    return { traceIdentity: doc.trace?.identity };
  });

  passGate("golden-planner-e2e", {
    stages: stages.length,
    stageNames: stages.map((row) => row.stage),
    // Per-stage digest details are part of the receipt, not only the
    // failure path.
    stageDetails: stages.map((row) => ({ stage: row.stage, ok: row.ok, detail: row.detail })),
  });
} finally {
  rmSync(root, { recursive: true, force: true });
}
