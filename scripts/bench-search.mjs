#!/usr/bin/env node
// Issue #93 benchmark gate (AC7): a reproducible comparison of the
// lekalo semantic search and a plain-text `rg` scan over identical
// frozen local fixture copies, answering the same task-oriented
// consumer questions scored against independently authored
// expectations.
//
// Every task declares, before either tool runs: the relevant answer
// keys (recall), the exact selection verdict, the top-ranked key where
// one must lead, the minimum ANSWER precision the page must meet
// (declared on every task — a broad lexical question names its
// deliberately tolerated irrelevant tail in `toleratedIrrelevant` so a
// passing task never hides a low-precision page behind an undeclared
// omission), the returned-row cap for negatives, and the rg answer
// files its plain-text pattern must surface together with the minimum
// rg answer-file precision (no-hit negatives expect zero hits). Both
// legs are scored against those declarations: recall, precision,
// selection correctness, top-1 correctness, rg answer-file recall AND
// rg answer-file precision. A task is fullyCorrect only when every
// declared expectation holds — recall-only success is never labeled
// correct, and no-hit negatives must be valid `none`.
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
  mkdirSync,
  readFileSync,
  readdirSync,
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
const rgLocation = spawnSync(process.platform === "win32" ? "where" : "which", ["rg"], { encoding: "utf8" });
if (rgLocation.status !== 0) fail("rg-path-unresolved", rgLocation.stderr);
const rgBinary = rgLocation.stdout.trim().split(/\r?\n/)[0];

const tasks = JSON.parse(readFileSync(tasksPath, "utf8"));
if (tasks.benchmarkId !== "dev.lekalo.search-benchmark") fail("benchmark-identity", tasks.benchmarkId);
if (!Array.isArray(tasks.tasks) || tasks.tasks.length === 0) fail("tasks-empty");
// Predeclared comparison hygiene: every task carries an explicit
// minimum answer precision and names its tolerated irrelevant class —
// an undeclared omission can never pass as correct.
for (const task of tasks.tasks) {
  if (typeof task.expectedPrecision !== "number") {
    fail("task-precision-undeclared", { id: task.id });
  }
  if (typeof task.toleratedIrrelevant !== "string") {
    fail("task-tolerance-unnamed", { id: task.id });
  }
}
const sha256 = (path) =>
  `sha256:${createHash("sha256").update(readFileSync(path)).digest("hex")}`;
const binaryDigest = sha256(binary);
const rgBinaryDigest = sha256(rgBinary);
// The full fixture tree digest: every committed fixture file (sorted
// project-relative POSIX paths, each pinned by its own digest) — not
// just one manifest — plus the exact lekalo binary bytes.
const treeDigest = (dir, prefix = "") => {
  const entries = readdirSync(dir, { withFileTypes: true })
    .map((entry) => ({ name: entry.name, dir: entry.isDirectory() }))
    .sort((a, b) => a.name.localeCompare(b.name));
  const hasher = createHash("sha256");
  for (const entry of entries) {
    const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.dir) {
      hasher.update(relative);
      hasher.update(treeDigest(join(dir, entry.name), relative));
    } else {
      hasher.update(relative);
      hasher.update(sha256(join(dir, entry.name)));
    }
  }
  return hasher.digest("hex");
};

// Identical frozen fixture copies for both tools.
const project = realpathSync.native(mkdtempSync(join(tmpdir(), "lekalo-search-bench-")));
cpSync(fixtureRoot, project, { recursive: true });
rmSync(join(project, ".lekalo"), { recursive: true, force: true });

// Privacy canaries and cache custody live in the CLI gate; the
// benchmark uses the same empty offline environment.
const offlineEnv = { PATH: process.env.PATH ?? "" };
if (process.platform === "win32") offlineEnv.SystemRoot = process.env.SystemRoot ?? "";

const runLekalo = (args, cwd = project) => {
  const started = process.hrtime.bigint();
  const result = spawnSync(binary, args, { cwd, encoding: "utf8", env: offlineEnv });
  const millis = Number(process.hrtime.bigint() - started) / 1e6;
  const stdout = result.stdout ?? "";
  return { code: result.status ?? 1, stdout, stderr: result.stderr ?? "", millis };
};

const runRg = (pattern, cwd = project) => {
  const started = process.hrtime.bigint();
  const result = spawnSync(
    rgBinary,
    ["-n", "--fixed-strings", "--", pattern, "."],
    { cwd, encoding: "utf8", env: offlineEnv },
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

// Real observed-consumer pilot: adopt the Taskhub workspace and ingest
// its pinned scanner-produced document, not a family renamed over Planner.
const consumerRoot = resolve(root, "tests/fixtures/pilot/taskhub");
const scanPath = resolve(root, "tests/fixtures/pilot/taskhub-scans/taskhub.scan.v0_2_16.json");
const consumerProject = realpathSync.native(mkdtempSync(join(tmpdir(), "lekalo-search-consumer-")));
cpSync(consumerRoot, consumerProject, { recursive: true });
const consumerStage = (args) => {
  const run = runLekalo(["--json", ...args, "--project", "."], consumerProject);
  if (run.code !== 0) fail("consumer-setup", { args, stdout: run.stdout, stderr: run.stderr });
};
consumerStage(["init", "--adopt", "--project-id", "taskhub"]);
for (const module of ["taskhub_tasks", "taskhub_events", "taskhub_integrations", "taskhub_sync", "taskhub_api"]) {
  consumerStage(["module", "new", module]);
}
consumerStage(["observe", "update", "--scan", scanPath]);
mkdirSync(join(consumerProject, "evidence"));
cpSync(join(consumerProject, ".lekalo/import/observed/index.json"), join(consumerProject, "evidence/observed.index.json"));
writeFileSync(join(consumerProject, "evidence/search-inputs.json"), JSON.stringify({
  schemaVersion: "lekalo/search-inputs/v0.8.0",
  identity: "dev.lekalo.search-inputs@0.8.0",
  projectId: "taskhub",
  inputs: [{ kind: "observed-index", path: "evidence/observed.index.json", identity: "dev.lekalo.observed-index@0.2.16" }],
}));
consumerStage(["search-index", "build", "--inputs", "evidence/search-inputs.json"]);

const trials = 5;
const rows = [];
let correct = 0;
let honest = 0;
for (const task of tasks.tasks) {
  const taskProject = task.fixture === "taskhub" ? consumerProject : project;
  // Registered serving (no --inputs), scored across every task and trial.
  // Explicit re-admission is a separately timed comparison below.
  const explicitArgs = ["--json", task.command];
  if (task.command === "search") explicitArgs.push(task.query);
  for (const flag of task.flags ?? []) explicitArgs.push(...flag.split(" "));
  explicitArgs.push("--project", ".", "--inputs", "evidence/search-inputs.json");
  const explicitRun = runLekalo(explicitArgs, taskProject);
  if (explicitRun.code !== 0) fail("explicit-query", explicitRun.stderr || explicitRun.stdout);
  const explicitKeys = JSON.parse(explicitRun.stdout).items.map((item) => item.card.key);
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
    args.push("--project", ".");
    const run = runLekalo(args, taskProject);
    if (run.code !== 0) fail("registered-query", run.stderr || run.stdout);
    if (JSON.stringify(JSON.parse(run.stdout).items.map((item) => item.card.key)) !== JSON.stringify(explicitKeys)) {
      fail("registered-explicit-parity", task.id);
    }
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
  // declared answer files on BOTH recall and answer-file precision —
  // merely hitting every answer file is not a correct answer when the
  // same pattern drags in unrelated files a consumer must open. A
  // no-hit negative is correct only with zero hits; every other task
  // must surface every declared answer file at the declared precision.
  const rgRun = runRg(task.rgPattern, taskProject);
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
  // Answer-file precision: the share of opened files that are declared
  // answer files. The rg consumer's real cost is the irrelevant files
  // the line hits force open (explicitly modeled here, not measured by
  // the tool: rg opens exactly the files it reports).
  const rgFilePrecision =
    rgRun.filesOpened === 0 ? 1 : rgRelevantHit.length / rgRun.filesOpened;
  const rgPrecisionCorrect =
    task.rgNoHitNegative || task.rgExpectedFilePrecision == null
      ? true
      : rgFilePrecision >= task.rgExpectedFilePrecision;
  const rgCorrect =
    !rgError && rgPrecisionCorrect &&
    (task.rgNoHitNegative
      ? rgRun.lines === 0
      : rgRecall === 1 || (task.rgRelevantFiles ?? []).length === 0);

  rows.push({
    id: task.id,
    family: task.family ?? "general",
    fixture: task.fixture ?? "planner",
    negative: task.negative === true,
    question: task.question,
    toleratedIrrelevant: task.toleratedIrrelevant ?? null,
    lekalo: {
      servingPath: "registered: no --inputs",
      explicitAdmissionMillis: explicitRun.millis,
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
      // Consistently LABELED, not silently asserted: the query path
      // opens no target source BY DESIGN (the core never reads target
      // files; the only reads are the canonical IR and the admitted
      // producer files). The metric states its measurement model
      // instead of presenting an instrumented count.
      targetSourceFilesOpened: 0,
      targetSourceFilesOpenedModel:
        "design-asserted: lekalo-core search opens no target source (audited, no target-read APIs)",
    },
    rg: {
      pattern: task.rgPattern,
      exitCode: rgRun.code,
      recallOverDeclaredAnswerFiles: rgRecall,
      answerFilePrecision: rgFilePrecision,
      expectedFilePrecision: task.rgExpectedFilePrecision ?? null,
      answerFilesHit: rgRelevantHit,
      lineHits: rgRun.lines,
      // Explicitly modeled file-open work: rg opens exactly the files
      // its line hits name; a human consumer then reads each hit line's
      // context to select an answer.
      filesOpened: rgRun.filesOpened,
      filesOpenedModel: "modeled: rg opens exactly the files it reports line hits for",
      stdoutBytes: Buffer.byteLength(rgRun.stdout, "utf8"),
      approxTokens: Math.ceil(Buffer.byteLength(rgRun.stdout, "utf8") / 4),
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
    // The full fixture tree and both tool binaries are pinned: the
    // comparison is reproducible against exact fixture and tool bytes.
    fixtureTreeDigest: `sha256:${treeDigest(fixtureRoot)}`,
    fixtureManifestDigest: sha256(join(fixtureRoot, "lekalo/project.yaml")),
    taskManifestDigest: sha256(tasksPath),
    binary: process.env.LEKALO_BIN ?? "target/debug/lekalo",
    binaryDigest,
    rgBinary,
    rgDigest: rgBinaryDigest,
    consumerFixture: "tests/fixtures/pilot/taskhub",
    consumerFixtureTreeDigest: `sha256:${treeDigest(consumerRoot)}`,
    consumerScanDigest: sha256(scanPath),
    trials,
    note: "Timings are local-run distributions, never universal claims; rg file opens are explicitly modeled (see filesOpenedModel).",
  },
  costClasses: {
    coldIndexBuildMillis: Math.round(coldIndexBuildMillis * 1000) / 1000,
    coldRebuildMillis: Math.round(coldRebuildMillis * 1000) / 1000,
    warmQueryMillis: "registered queries without --inputs: per-task distributions below",
    explicitAdmissionQueryMillis: "per-task measurements below",
    incrementalUpdateNoopMillis: Math.round(incrementalUpdateNoopMillis * 1000) / 1000,
    incrementalUpdateDriftMillis: Math.round(incrementalUpdateDriftMillis * 1000) / 1000,
  },
  tasks: rows,
  summary: {
    taskCount: tasks.tasks.length,
    fullyCorrect: correct,
    ambiguityHonest: honest,
    rgCorrect: rows.filter((row) => row.rgCorrect).length,
    // The consumer-answer comparison in one line: both tools' precision
    // over their own declared answer spaces, task for task.
    lekaloPrecision: rows.map((row) => row.lekalo.precision),
    rgAnswerFilePrecision: rows.map((row) => row.rg.answerFilePrecision),
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
rmSync(consumerProject, { recursive: true, force: true });
