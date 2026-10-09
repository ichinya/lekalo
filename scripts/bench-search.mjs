#!/usr/bin/env node
// Issue #93 benchmark gate (AC7): a reproducible comparison of the
// lekalo semantic search and a plain-text `rg` scan over identical
// frozen local fixture copies, answering the same task-oriented
// consumer questions scored against independently authored
// expectations.
//
// Every task declares, before either tool runs: the relevant answer
// keys (recall), the exact selection verdict, the top-ranked key where
// one must lead, the precision the page must meet, the returned-row cap
// for negatives, and the rg answer files its plain-text pattern must
// surface (no-hit negatives expect zero hits). Both legs are scored
// against those declarations: recall, precision, selection correctness,
// top-1 correctness, and rg answer-file recall. A task is fullyCorrect
// only when every declared expectation holds — recall-only success is
// never labeled correct, and no-hit negatives must be valid `none`.
//
// The benchmark separates three cost classes and never conflates them:
// cold index build, warm semantic query, and incremental update (noop
// republish and post-drift), plus the rg line-scan and the file opens
// its line hits force. Byte volumes ride every leg. Latency
// distributions are reported, never a single favorable value. No
// universal speedup or quality claim is implied: the deliverable is the
// reproducible comparison plus the correctness gates.
//
// Requires the cargo-built binary; `rg` is resolved from PATH (the gate
// fails closed when either is missing).

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  cpSync,
  writeFileSync,
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

const tasksPath = resolve(root, "tests/fixtures/search/benchmark/tasks.json");
const fixtureRoot = resolve(root, "tests/fixtures/search/planner");
for (const [name, path] of [
  ["binary-missing", binary],
  ["fixture-missing", tasksPath],
  ["fixture-missing", fixtureRoot],
]) {
  let ok = false;
  try {
    ok = statSync(path).isFile() || statSync(path).isDirectory();
  } catch {
    ok = false;
  }
  if (!ok) fail(name, `run: cargo build --workspace --locked (${path})`);
}
const rg = spawnSync("rg", ["--version"], { encoding: "utf8" });
if (rg.status !== 0) fail("rg-missing", "ripgrep must be on PATH for the comparison leg");

const tasks = JSON.parse(readFileSync(tasksPath, "utf8"));
if (tasks.benchmarkId !== "dev.lekalo.search-benchmark") fail("benchmark-identity", tasks.benchmarkId);
if (!Array.isArray(tasks.tasks) || tasks.tasks.length === 0) fail("tasks-empty");
const sha256 = (path) =>
  `sha256:${createHash("sha256").update(readFileSync(path)).digest("hex")}`;

// Identical frozen fixture copies for both tools.
const project = realpathSync.native(mkdtempSync(join(tmpdir(), "lekalo-search-bench-")));
cpSync(fixtureRoot, project, { recursive: true });
rmSync(join(project, ".lekalo"), { recursive: true, force: true });

// Privacy canaries and cache custody live in the CLI gate; the
// benchmark uses the same empty offline environment.
const offlineEnv = { PATH: process.env.PATH ?? "" };
if (process.platform === "win32") offlineEnv.SystemRoot = process.env.SystemRoot ?? "";

const runLekalo = (args) => {
  const started = process.hrtime.bigint();
  const result = spawnSync(binary, args, { cwd: project, encoding: "utf8", env: offlineEnv });
  const millis = Number(process.hrtime.bigint() - started) / 1e6;
  const stdout = result.stdout ?? "";
  return { code: result.status ?? 1, stdout, stderr: result.stderr ?? "", millis };
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
  const lines = stdout.split("\n").filter(Boolean);
  // rg prefixes hit paths with `./` for a `.` root and emits native
  // separators on Windows; normalize to the fixture's POSIX spelling.
  const fileOf = (line) =>
    line
      .slice(0, line.indexOf(":"))
      .split("\\")
      .join("/")
      .replace(/^\.\//, "");
  return {
    code: result.status,
    stdout,
    millis,
    // The file opens the line hits force (accounted as work, not free).
    filesOpened: new Set(lines.map((line) => fileOf(line))).size,
    filesWithHits: [...new Set(lines.map((line) => fileOf(line)))],
    lines: lines.length,
  };
};

// Cold build cost (lekalo only; rg has no build phase).
const build = runLekalo([
  "search-index", "build", "--project", ".", "--inputs", "evidence/search-inputs.json",
]);
if (build.code !== 0) fail("build-failed", build.stderr);
const coldIndexBuildMillis = build.millis;

// Incremental update costs: a no-op republish (same admitted bytes),
// then a real post-drift update.
const updateNoopStart = process.hrtime.bigint();
const updateNoop = runLekalo([
  "--json",
  "search-index", "update", "--project", ".", "--inputs", "evidence/search-inputs.json",
]);
const incrementalUpdateNoopMillis = Number(process.hrtime.bigint() - updateNoopStart) / 1e6;
if (updateNoop.code !== 0 || !updateNoop.stdout.includes('"reused":true')) {
  fail("incremental-noop-failed", updateNoop.stderr || updateNoop.stdout);
}
// Drift one admitted producer byte, then update: the changed partition
// triggers a real republish over the existing store.
const driftedPath = join(project, "evidence", "effects.detected.json");
const originalEffects = readFileSync(driftedPath, "utf8");
writeFileSync(driftedPath, originalEffects + " ");
const updateDriftStart = process.hrtime.bigint();
const updateDrift = runLekalo([
  "--json",
  "search-index", "update", "--project", ".", "--inputs", "evidence/search-inputs.json",
]);
const incrementalUpdateDriftMillis = Number(process.hrtime.bigint() - updateDriftStart) / 1e6;
if (updateDrift.code !== 0) fail("incremental-drift-failed", updateDrift.stderr);
if (updateDrift.stdout.includes('"reused":true')) {
  fail("incremental-drift-not-detected", updateDrift.stdout.slice(0, 200));
}
// Restore the exact committed fixture bytes and republish cleanly for
// the scored tasks.
writeFileSync(driftedPath, originalEffects);
rmSync(join(project, ".lekalo"), { recursive: true, force: true });
const rebuildStart = process.hrtime.bigint();
const rebuild = runLekalo([
  "search-index", "build", "--project", ".", "--inputs", "evidence/search-inputs.json",
]);
const coldRebuildMillis = Number(process.hrtime.bigint() - rebuildStart) / 1e6;
if (rebuild.code !== 0) fail("rebuild-failed", rebuild.stderr);

const trials = 5;
const rows = [];
let correct = 0;
let honest = 0;
for (const task of tasks.tasks) {
  // lekalo leg: warm queries, repeated for a distribution.
  const lekaloRuns = [];
  let lekaloKeys = [];
  let selection = "";
  let returned = 0;
  let stdoutBytes = 0;
  let reasonCodes = [];
  for (let attempt = 0; attempt < trials; attempt += 1) {
    const args = ["--json"];
    args.push(task.command);
    if (task.command === "search") args.push(task.query);
    for (const flag of task.flags ?? []) args.push(...flag.split(" "));
    args.push("--project", ".", "--inputs", "evidence/search-inputs.json");
    const run = runLekalo(args);
    lekaloRuns.push(run.millis);
    if (attempt === 0) {
      stdoutBytes = Buffer.byteLength(run.stdout, "utf8");
      if (run.code === 0) {
        try {
          const doc = JSON.parse(run.stdout);
          selection = doc.selection ?? "";
          lekaloKeys = (doc.items ?? []).map((item) => item.card.key);
          returned = lekaloKeys.length;
          reasonCodes = doc.reasonCodes ?? [];
        } catch {
          fail("lekalo-json", run.stdout.slice(0, 200));
        }
      }
    }
  }
  lekaloRuns.sort((a, b) => a - b);
  const expected = new Set(task.relevant);
  const hits = lekaloKeys.filter((key) => expected.has(key));
  const recall = expected.size === 0 ? (returned === 0 ? 1 : 0) : hits.length / expected.size;
  const precision = returned === 0 ? 1 : hits.length / returned;
  const selectionCorrect = selection === task.expectedSelection;
  const topCorrect =
    task.expectedTop == null || lekaloKeys[0] === task.expectedTop;
  const precisionCorrect =
    task.expectedPrecision == null || precision >= task.expectedPrecision;
  const returnedCorrect =
    task.expectedReturned == null || returned <= task.expectedReturned;
  const coverageWarningCorrect =
    task.expectCoverageWarning == null ||
    reasonCodes.includes("search.coverage-incomplete");
  // Ambiguity honesty: a task marked ambiguous:true must be reported
  // ambiguous; any other selection is a silent-choice failure.
  const ambiguityHonest = task.ambiguous
    ? selection === "ambiguous"
    : selection !== "ambiguous" || task.expectedSelection === "ambiguous";
  const lekaloCorrect =
    recall === 1 &&
    selectionCorrect &&
    topCorrect &&
    precisionCorrect &&
    returnedCorrect &&
    coverageWarningCorrect;
  if (ambiguityHonest) honest += 1;
  if (lekaloCorrect && ambiguityHonest) correct += 1;

  // rg leg: the predeclared fair plain-text query, scored against the
  // declared answer files. A no-hit negative is correct only with zero
  // hits; every other task must surface every declared answer file.
  const rgRun = runRg(task.rgPattern);
  const rgError = rgRun.code !== 0 && rgRun.code !== 1;
  const rgHitFiles = new Set(rgRun.filesWithHits);
  const rgRelevantHit = (task.rgRelevantFiles ?? []).filter((file) => rgHitFiles.has(file));
  const rgRecall =
    (task.rgRelevantFiles ?? []).length === 0
      ? task.rgNoHitNegative
        ? rgRun.lines === 0
          ? 1
          : 0
        : 1
      : rgRelevantHit.length / task.rgRelevantFiles.length;
  const rgCorrect =
    !rgError &&
    (task.rgNoHitNegative
      ? rgRun.lines === 0
      : rgRecall === 1 || (task.rgRelevantFiles ?? []).length === 0);

  rows.push({
    id: task.id,
    family: task.family ?? "general",
    negative: task.negative === true,
    question: task.question,
    lekalo: {
      command: task.command,
      selection,
      expectedSelection: task.expectedSelection,
      selectionCorrect,
      topCorrect,
      precisionCorrect,
      coverageWarningCorrect,
      recall,
      precision,
      latencyMillis: {
        min: lekaloRuns[0],
        median: lekaloRuns[Math.floor(trials / 2)],
        max: lekaloRuns[trials - 1],
      },
      resultBytes: stdoutBytes,
      approxTokens: Math.ceil(stdoutBytes / 4),
      // The query path never opens target source (contract): the only
      // reads are the canonical IR and the admitted producer files.
      targetSourceFilesOpened: 0,
    },
    rg: {
      pattern: task.rgPattern,
      exitCode: rgRun.code,
      recallOverDeclaredAnswerFiles: rgRecall,
      answerFilesHit: rgRelevantHit,
      lineHits: rgRun.lines,
      filesOpened: rgRun.filesOpened,
      stdoutBytes: Buffer.byteLength(rgRun.stdout, "utf8"),
      latencyMillis: Math.round(rgRun.millis * 1000) / 1000,
    },
    lekaloCorrect,
    rgCorrect,
    ambiguityHonest,
    failures: [
      ...(recall === 1 ? [] : ["recall"]),
      ...(selectionCorrect ? [] : ["selection"]),
      ...(topCorrect ? [] : ["top"]),
      ...(precisionCorrect ? [] : ["precision"]),
      ...(returnedCorrect ? [] : ["returned"]),
      ...(coverageWarningCorrect ? [] : ["coverage-warning"]),
      ...(rgCorrect ? [] : ["rg"]),
    ],
  });
}

const report = {
  benchmarkId: "dev.lekalo.search-benchmark",
  recipe: "semantic-search/1",
  environment: {
    platform: process.platform,
    node: process.version,
    rg: rg.stdout.split("\n")[0],
    fixture: tasks.fixture ?? "tests/fixtures/search/planner",
    fixtureManifestDigest: sha256(join(fixtureRoot, "lekalo/project.yaml")),
    taskManifestDigest: sha256(tasksPath),
    binary: process.env.LEKALO_BIN ?? "target/debug/lekalo",
    trials,
    note: "Timings are local-run distributions, never universal claims.",
  },
  costClasses: {
    coldIndexBuildMillis: Math.round(coldIndexBuildMillis * 1000) / 1000,
    coldRebuildMillis: Math.round(coldRebuildMillis * 1000) / 1000,
    warmQueryMillis: "per-task distributions below",
    incrementalUpdateNoopMillis: Math.round(incrementalUpdateNoopMillis * 1000) / 1000,
    incrementalUpdateDriftMillis: Math.round(incrementalUpdateDriftMillis * 1000) / 1000,
  },
  tasks: rows,
  summary: {
    taskCount: tasks.tasks.length,
    fullyCorrect: correct,
    ambiguityHonest: honest,
    rgCorrect: rows.filter((row) => row.rgCorrect).length,
  },
};

if (correct !== tasks.tasks.length || honest !== tasks.tasks.length) {
  fail("benchmark-correctness", {
    fullyCorrect: correct,
    ambiguityHonest: honest,
    failing: rows.filter((row) => !row.lekaloCorrect || !row.ambiguityHonest).map((row) => ({
      id: row.id,
      failures: row.failures,
      selection: row.lekalo.selection,
    })),
  });
}
if (rows.some((row) => !row.rgCorrect)) {
  fail("rg-leg-correctness", {
    failing: rows.filter((row) => !row.rgCorrect).map((row) => ({
      id: row.id,
      recallOverDeclaredAnswerFiles: row.rg.recallOverDeclaredAnswerFiles,
    })),
  });
}
process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
rmSync(project, { recursive: true, force: true });
