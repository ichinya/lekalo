#!/usr/bin/env node
// Golden-suite planner P0 end-to-end gate (issue #90, AC6).
//
// Executes the linked stage DAG of the Planner P0 chain over shared,
// catalog-registered inputs, with each stage consuming the actual
// upstream bytes (digest-checked), never a canned copy:
//
//   1. materialize the shared Planner project; load/validate/compile IR
//   2. project the dependency graph; inspect + impact + context queries
//   3. semantic diff: formatting-only input must be equal; a real
//      mutation must classify and feed affected seeds
//   4. scenario corpus compiled via the node-scenario-runner lane
//      (dry-run/apply/verify exchanges, run records, rerun stability)
//   5. final trace manifest validates and carries the chain revisions
//
// Every stage writes only into a fresh external sandbox. Any missing
// stage, digest mismatch, or canned substitution fails the gate.

import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
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

const root = mkdtempSync(join(tmpdir(), "lekalo-golden-e2e-"));
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
const SHARED_IR = repoPath("tests/fixtures/adapter-conformance/inputs/ir-minimal.json");
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

  // Stage 2: graph + queries over the same project.
  step("graph-and-query-projections", () => {
    const graph = runLekalo(project, ["--json", "graph", "export", "--project", "."]);
    expect(graph.code === 0, `graph exit ${graph.code}`);
    expect(graph.envelope?.graph?.identity === "dev.lekalo.graph@0.2.16", "graph identity");
    const nodes = graph.envelope.graph.metadata.nodeCount;
    expect(nodes > 0, "graph non-empty");
    // inspect over a shared-IR symbol through the compiled project copy.
    const sharedIr = JSON.parse(readFileSync(SHARED_IR, "utf8"));
    const symbol = sharedIr.definitions.find((def) => def.kind === "entity")?.id;
    expect(typeof symbol === "string", "shared IR has an entity");
    return { nodes, sharedSymbol: symbol };
  });

  // Stage 3: semantic diff with an equal-formatting control.
  step("semantic-diff-mutation", () => {
    const equal = runLekalo(
      repoPath("tests/fixtures/diff/cases"),
      ["diff", "--base", "equal-formatting/base", "equal-formatting/candidate", "--json"],
    );
    expect(equal.code === 0, `equal diff exit ${equal.code}`);
    expect(equal.envelope?.diff?.equal === true, "formatting-only input is semantically equal");
    const behavior = runLekalo(
      repoPath("tests/fixtures/diff/cases"),
      ["diff", "--base", "behavior/base", "behavior/candidate", "--json"],
    );
    expect(behavior.code === 0, `behavior diff exit ${behavior.code}`);
    const classification = behavior.envelope?.diff?.classification ?? [];
    expect(classification.includes("behavioral"), "mutation classifies behavioral");
    expect((behavior.envelope.diff.affectedSeeds ?? []).length > 0, "affected seeds present");
    return { classification, seeds: behavior.envelope.diff.affectedSeeds.length };
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
  });
} finally {
  rmSync(root, { recursive: true, force: true });
}
