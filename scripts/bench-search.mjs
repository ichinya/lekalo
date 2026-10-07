#!/usr/bin/env node
// Issue #93 benchmark gate (AC7): a reproducible comparison of the
// lekalo semantic search and a plain-text `rg` scan over identical
// frozen local fixture copies, answering the same task-oriented
// consumer questions with independently authored answer sets.
//
// The benchmark separates three cost classes and never conflates them:
// cold index build, warm semantic query, and the rg line-scan plus the
// file opens its line hits force. Recall/precision/ambiguity honesty
// are computed against answer sets authored before either tool runs;
// latency distributions are reported, never a single favorable value.
// No universal speedup or quality claim is implied: the deliverable is
// the reproducible comparison plus the correctness gates.
//
// Requires the cargo-built binary; `rg` is resolved from PATH (the gate
// fails closed when either is missing).

import { spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  statSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const binary =
  process.env.LEKALO_BIN ??
  join(root, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

for (const [name, path] of [
  ["binary-missing", binary],
  ["fixture-missing", resolve(root, "tests/fixtures/search/benchmark/tasks.json")],
]) {
  let ok = false;
  try {
    ok = statSync(path).isFile();
  } catch {
    ok = false;
  }
  if (!ok) fail(name, `run: cargo build --workspace --locked (${path})`);
}
const rg = spawnSync("rg", ["--version"], { encoding: "utf8" });
if (rg.status !== 0) fail("rg-missing", "ripgrep must be on PATH for the comparison leg");

const tasks = JSON.parse(readFileSync(resolve(root, "tests/fixtures/search/benchmark/tasks.json"), "utf8"));
if (tasks.benchmarkId !== "dev.lekalo.search-benchmark") fail("benchmark-identity", tasks.benchmarkId);
if (!Array.isArray(tasks.tasks) || tasks.tasks.length === 0) fail("tasks-empty");

// Identical frozen fixture copies for both tools.
const project = realpathSync.native(mkdtempSync(join(tmpdir(), "lekalo-search-bench-")));
cpSync(resolve(root, "tests/fixtures/search/planner"), project, { recursive: true });
rmSync(join(project, ".lekalo"), { recursive: true, force: true });

// Privacy canaries and cache custody live in the CLI gate; the
// benchmark uses the same empty offline environment.
const offlineEnv = { PATH: process.env.PATH ?? "" };
if (process.platform === "win32") offlineEnv.SystemRoot = process.env.SystemRoot ?? "";

const runLekalo = (args) => {
  const started = process.hrtime.bigint();
  const result = spawnSync(binary, args, { cwd: project, encoding: "utf8", env: offlineEnv });
  const millis = Number(process.hrtime.bigint() - started) / 1e6;
  return { code: result.status ?? 1, stdout: result.stdout ?? "", stderr: result.stderr ?? "", millis };
};

const runRg = (pattern) => {
  const started = process.hrtime.bigint();
  const result = spawnSync(
    "rg",
    ["-n", "--fixed-strings", "--", pattern, "."],
    { cwd: project, encoding: "utf8", env: offlineEnv },
  );
  const millis = Number(process.hrtime.bigint() - started) / 1e6;
  const stdout = result.stdout ?? "";
  return {
    code: result.status,
    stdout,
    millis,
    filesOpened: new Set(
      stdout
        .split("\n")
        .filter(Boolean)
        .map((line) => line.slice(0, line.indexOf(":"))),
    ).size,
  };
};

// Cold build cost (lekalo only; rg has no build phase).
const buildStarted = process.hrtime.bigint();
const build = runLekalo([
  "search-index", "build", "--project", ".", "--inputs", "evidence/search-inputs.json",
]);
const buildMillis = Number(process.hrtime.bigint() - buildStarted) / 1e6;
if (build.code !== 0) fail("build-failed", build.stderr);

const trials = 5;
const rows = [];
let correct = 0;
let honest = 0;
for (const task of tasks.tasks) {
  // lekalo leg: warm queries, repeated for a distribution.
  const lekaloRuns = [];
  let lekaloKeys = [];
  let selection = "";
  for (let attempt = 0; attempt < trials; attempt += 1) {
    const args = ["--json"];
    args.push(task.command);
    if (task.command === "search") args.push(task.query);
    for (const flag of task.flags ?? []) args.push(...flag.split(" "));
    args.push("--project", ".", "--inputs", "evidence/search-inputs.json");
    const run = runLekalo(args);
    lekaloRuns.push(run.millis);
    if (run.code === 0 && attempt === 0) {
      const doc = JSON.parse(run.stdout);
      selection = doc.selection ?? "";
      lekaloKeys = (doc.items ?? []).map((item) => item.card.key);
    }
  }
  lekaloRuns.sort((a, b) => a - b);
  const expected = new Set(task.relevant);
  const hits = lekaloKeys.filter((key) => expected.has(key));
  const recall = expected.size === 0 ? 1 : hits.length / expected.size;
  const precision = lekaloKeys.length === 0 ? 1 : hits.length / lekaloKeys.length;
  // Ambiguity honesty: a task marked ambiguous:true must be reported
  // ambiguous; any other selection is a silent-choice failure.
  const ambiguityHonest = task.ambiguous ? selection === "ambiguous" : selection !== "ambiguous";
  if (ambiguityHonest) honest += 1;
  if (recall === 1 && ambiguityHonest) correct += 1;

  // rg leg: the predeclared fair plain-text query, then the file opens
  // its line hits force (accounted as work, not free).
  const rgRun = runRg(task.rgPattern);
  const rgAnswerable = rgRun.code === 0 || rgRun.code === 1;

  rows.push({
    id: task.id,
    question: task.question,
    lekalo: {
      command: task.command,
      selection,
      recall,
      precision,
      latencyMillis: {
        min: lekaloRuns[0],
        median: lekaloRuns[Math.floor(trials / 2)],
        max: lekaloRuns[trials - 1],
      },
    },
    rg: {
      pattern: task.rgPattern,
      exitCode: rgRun.code,
      answerableWithoutFileOpens: rgAnswerable,
      lineHits: rgRun.stdout.split("\n").filter(Boolean).length,
      filesOpened: rgRun.filesOpened,
      latencyMillis: Math.round(rgRun.millis * 1000) / 1000,
    },
    ambiguityHonest,
  });
}

const report = {
  benchmarkId: "dev.lekalo.search-benchmark",
  recipe: "semantic-search/1",
  environment: {
    platform: process.platform,
    node: process.version,
    rg: rg.stdout.split("\n")[0],
    fixture: "tests/fixtures/search/planner",
    trials,
    note: "Timings are local-run distributions, never universal claims.",
  },
  costClasses: {
    coldIndexBuildMillis: Math.round(buildMillis * 1000) / 1000,
    warmQueryMillis: "per-task distributions below",
    incrementalUpdateMillis: null,
  },
  tasks: rows,
  summary: {
    taskCount: tasks.tasks.length,
    fullyCorrect: correct,
    ambiguityHonest: honest,
  },
};

if (correct !== tasks.tasks.length || honest !== tasks.tasks.length) {
  fail("benchmark-correctness", { fullyCorrect: correct, ambiguityHonest: honest });
}
process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
rmSync(project, { recursive: true, force: true });
