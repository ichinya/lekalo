#!/usr/bin/env node
/**
 * Issue #37 CodeGraph context benchmark (research section "Rust
 * CodeGraph context benchmark protocol"): compare baseline bounded
 * file/grep retrieval against workspace_context + codegraph retrieval
 * over a pinned public core change, with cold/warm/changed/revoked
 * phases, five repetitions per timing case, and privacy-confined
 * aggregate output.
 *
 *   node scripts/benchmark-ai-workspace-context.mjs --upstream <bin>
 *     [--reps 5] [--out <dir>]
 *
 * Pinned inputs (overridable for reproduction):
 *   --upstream-commit 8fdf818fee757d24e723d657fc5d38614995e557
 *   --base-commit 1d1f14a79e99fba5c02a9ad75a1b9be6875cea27
 *   --head-commit 3d7cfcfb87576147ec0b43dcc2380ddfeb677193
 *
 * Everything runs on scratch copies in the OS temp directory: a
 * dedicated AI_WORKSPACE_DB, a synthetic lekalo-core project whose
 * shared scope is exactly the benchmark's run_history sources at the
 * head revision, and synthetic consumer projects. Raw per-run rows
 * (which contain absolute paths) stay in the private --out directory;
 * stdout prints only the closed public aggregate (counts, bytes,
 * milliseconds, digests — no absolute paths, no host names).
 *
 * Hard gates (fail the benchmark, never reported as success):
 *   - the changed/revoked phases must surface staleness or refusal,
 *     never a silent fresh-looking result;
 *   - aggregate privacy probes over the printed JSON;
 *   - declared expected files must be found through both strategies
 *     or the miss is recorded (coverage is measured, not assumed).
 */
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, dirname, relative } from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const argOf = (name, fallback = null) => {
  const index = process.argv.indexOf(name);
  if (index >= 0) return process.argv[index + 1];
  return fallback;
};
const UPSTREAM = argOf("--upstream", process.env.AI_WORKSPACE_BIN);
const REPS = Math.max(1, Number(argOf("--reps", "5")));
const OUT_DIR = argOf("--out") ? resolve(argOf("--out")) : null;
const UPSTREAM_COMMIT = argOf("--upstream-commit", "8fdf818fee757d24e723d657fc5d38614995e557");
const BASE_COMMIT = argOf("--base-commit", "1d1f14a79e99fba5c02a9ad75a1b9be6875cea27");
const HEAD_COMMIT = argOf("--head-commit", "3d7cfcfb87576147ec0b43dcc2380ddfeb677193");
const GROUP = "benchmark-dev";

const sha256Ref = (buffer) => `sha256:${createHash("sha256").update(buffer).digest("hex")}`;
const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, benchmark: "ai-workspace-context", reason, detail: String(detail ?? "").slice(0, 800) }, null, 2)}\n`);
  process.exit(1);
};

if (!UPSTREAM || !existsSync(UPSTREAM)) {
  fail("upstream-required", "pass --upstream <path to the pinned ai-workspace binary>");
}

// The pinned core change touches exactly these files (research
// benchmark step 1); navigation tasks and expected files derive from
// the reviewed diff of BASE_COMMIT..HEAD_COMMIT.
const CHANGE_FILES = [
  "crates/lekalo-core/src/run_history/store.rs",
  "crates/lekalo-core/src/run_history/tests.rs",
  "crates/lekalo-core/src/run_history/mod.rs",
];

// Predeclared navigation tasks (research step 2): question, expected
// files, and the expected symbol anchors from the reviewed change.
const TASKS = [
  {
    id: "custody-read-verification",
    question: "where does the history store re-verify record custody at read time",
    expectedFiles: ["crates/lekalo-core/src/run_history/store.rs"],
    grepNeedles: ["record-digest", "record_digest"],
    codegraphQueries: ["get", "check_frozen_refs"],
  },
  {
    id: "recovery-quarantine",
    question: "where does recovery quarantine records whose column and body ids diverge",
    expectedFiles: ["crates/lekalo-core/src/run_history/store.rs"],
    grepNeedles: ["quarantined", "embedded_run_id"],
    codegraphQueries: ["recover", "Store"],
  },
  {
    id: "quarantine-regression-test",
    question: "which test proves the column and body id divergence quarantine",
    expectedFiles: ["crates/lekalo-core/src/run_history/tests.rs"],
    grepNeedles: ["recovery_quarantines_records_whose_column_and_body_ids_diverge"],
    codegraphQueries: ["recovery_quarantines_records_whose_column_and_body_ids_diverge"],
  },
];

// ---------------------------------------------------------------------------

const cleanup = [];
const scratch = () => {
  const dir = mkdtempSync(join(tmpdir(), "lekalo-i37-bench-"));
  cleanup.push(dir);
  return dir;
};

const run = (command, argv, options = {}) => {
  const result = spawnSync(command, argv, {
    encoding: "utf8",
    timeout: options.timeoutMs ?? 120_000,
    cwd: options.cwd,
    input: options.input,
    env: options.env,
    windowsHide: true,
  });
  if (result.status !== 0 && process.env.BENCH_DEBUG) {
    process.stderr.write(`DBG ${command} ${argv.join(" ")} => ${result.status}: ${String(result.stderr ?? "").slice(0, 200)}
`);
  }
  return {
    status: result.status,
    stdout: typeof result.stdout === "string" ? result.stdout : "",
    stderr: typeof result.stderr === "string" ? result.stderr : "",
    error: result.error,
  };
};

const upstreamEnv = (dbPath) => ({
  ...process.env,
  AI_WORKSPACE_DB: dbPath,
  AI_WORKSPACE_ALLOW_PROJECT_WIDE_TOOLS: "0",
  AI_WORKSPACE_ALLOW_PROJECT_FILE_WRITE: "0",
});

const upstreamRun = (dbPath, argv, options = {}) =>
  run(UPSTREAM, argv, { ...options, env: upstreamEnv(dbPath) });

const mcpCall = (dbPath, cwd, calls, options = {}) => {
  const requests = calls.map((call, index) => ({
    jsonrpc: "2.0",
    id: index + 1,
    method: "tools/call",
    params: { name: call.name, arguments: call.arguments ?? {} },
  }));
  const started = performance.now();
  const result = run(UPSTREAM, ["serve", "--group", options.group ?? GROUP], {
    cwd,
    input: `${requests.map((request) => JSON.stringify(request)).join("\n")}\n`,
    env: upstreamEnv(dbPath),
    timeoutMs: 60_000,
  });
  const elapsedMs = performance.now() - started;
  const responses = result.stdout
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => { try { return JSON.parse(line); } catch { return null; } })
    .filter(Boolean);
  const results = calls.map((call, index) => {
    const response = responses.find((message) => message.id === index + 1);
    const text = response && !response.error
      ? response.result?.content?.find((entry) => entry.type === "text")?.text ?? ""
      : "";
    let json = null;
    try { json = JSON.parse(text); } catch { json = null; }
    return { name: call.name, denied: Boolean(response?.error) || Boolean(response?.result?.isError), text, json };
  });
  return { results, elapsedMs };
};

// Copy a file set from the lekalo checkout at one revision into the
// scratch core project (scratch copy discipline: never share the real
// checkout; never share more than the declared scope).
const copyAtRevision = (targetRoot, revision, files) => {
  for (const relPath of files) {
    const result = run("git", ["show", `${revision}:${relPath}`], { cwd: REPO_ROOT });
    if (result.status !== 0) fail("git-show-failed", `${revision}:${relPath}`);
    const target = join(targetRoot, relPath);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, result.stdout);
  }
};

const median = (values) => {
  const sorted = [...values].sort((left, right) => left - right);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : Math.round((sorted[mid - 1] + sorted[mid]) / 2);
};
const range = (values) => [Math.min(...values), Math.max(...values)];

// Baseline strategy: bounded recursive grep over the declared scope.
const baselineSearch = (coreRoot, needle, { maxFiles = 10, maxBytes = 16 * 1024 } = {}) => {
  const started = performance.now();
  const hits = [];
  const walk = (dir) => {
    if (hits.length >= maxFiles) return;
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full);
      } else if (entry.name.endsWith(".rs")) {
        const content = readFileSync(full, "utf8");
        if (content.includes(needle)) {
          hits.push(relative(coreRoot, full).split("\\").join("/"));
          if (hits.length >= maxFiles) return;
        }
      }
    }
  };
  walk(join(coreRoot, "crates"));
  const bytes = hits.slice(0, maxFiles).reduce((total, file) => Math.min(maxBytes, statSync(join(coreRoot, file)).size), 0);
  return { hits, readBytes: bytes, elapsedMs: Math.round(performance.now() - started) };
};

try {
  const root = scratch();
  const dbPath = join(root, "benchmark.db");
  const coreRoot = join(root, "lekalo-core");
  const consumerRoot = join(root, "bench-consumer");
  const privateRoot = join(root, "private");

  // Scratch core project: the benchmark scope is exactly the three
  // changed run_history files at the pinned head revision.
  mkdirSync(coreRoot, { recursive: true });
  copyAtRevision(coreRoot, HEAD_COMMIT, CHANGE_FILES);
  // The scratch core config carries no share list: sharing here is
  // done explicitly below, and a duplicate share is a no-op (the
  // config-sync would re-add it on every init). Keep the config
  // share-free so cold-DB re-inits never collide with explicit shares.
  writeFileSync(join(coreRoot, ".ai-workspace.local.json"), `${JSON.stringify({
    ai_workspace_config_version: 1, name: "lekalo-core", slug: "lekalo-core", groups: [GROUP], share: [], notes: [],
  }, null, 2)}\n`);
  // A private sentinel that must never enter the index or results.
  mkdirSync(join(coreRoot, "private"), { recursive: true });
  writeFileSync(join(coreRoot, "private", "secret.rs"), "fn benchmark_private_sentinel() {}\n");

  mkdirSync(consumerRoot, { recursive: true });
  writeFileSync(join(consumerRoot, ".ai-workspace.local.json"), `${JSON.stringify({
    ai_workspace_config_version: 1, name: "bench-consumer", slug: "bench-consumer", groups: [GROUP], share: [], notes: [],
  }, null, 2)}\n`);

  for (const [dir, name] of [[coreRoot, "lekalo-core"], [consumerRoot, "bench-consumer"]]) {
    const result = upstreamRun(dbPath, ["--config", ".ai-workspace.local.json", "init", "--name", name, "--slug", name, "--group", GROUP], { cwd: dir });
    assert.equal(result.status, 0, `init failed for ${name}`);
  }

  // Scope: share the three files explicitly. Upstream auto-writes the
  // share list back into the local config; the config is reset to the
  // empty-share shape before every registration that must not
  // auto-share (see cold loop below).
  for (const relPath of CHANGE_FILES) {
    const result = upstreamRun(dbPath, ["--config", ".ai-workspace.local.json", "share", relPath, "--label", `benchmark ${relPath}`], { cwd: coreRoot });
    assert.equal(result.status, 0, `share failed: ${relPath}`);
  }

  // ---- cold reindex ----
  const coldTimings = [];
  let coldStats = null;
  for (let index = 0; index < REPS; index += 1) {
    // Cold = a fresh database per repetition. Upstream's `share`
    // auto-writes the share list into the local config, and `init`
    // syncs that list — so the config is reset to the empty-share
    // shape first and the shares are re-issued explicitly, keeping
    // every cold registration deterministic.
    const coldDb = join(root, `cold-${index}.db`);
    writeFileSync(join(coreRoot, ".ai-workspace.local.json"), `${JSON.stringify({
      ai_workspace_config_version: 1, name: "lekalo-core", slug: "lekalo-core", groups: [GROUP], share: [], notes: [],
    }, null, 2)}\n`);
    for (const [dir, name] of [[coreRoot, "lekalo-core"], [consumerRoot, "bench-consumer"]]) {
      const init = upstreamRun(coldDb, ["--config", ".ai-workspace.local.json", "init", "--name", name, "--slug", name, "--group", GROUP], { cwd: dir });
      assert.equal(init.status, 0);
    }
    for (const relPath of CHANGE_FILES) {
      const share = upstreamRun(coldDb, ["--config", ".ai-workspace.local.json", "share", relPath], { cwd: coreRoot });
      assert.equal(share.status, 0);
    }
    const started = performance.now();
    const reindex = upstreamRun(coldDb, ["codegraph", "reindex", "--project", "lekalo-core"], { cwd: coreRoot });
    coldTimings.push(Math.round(performance.now() - started));
    assert.equal(reindex.status, 0, `cold reindex failed: ${reindex.stderr.slice(0, 200)}`);
    if (index === REPS - 1) coldStats = { db: coldDb };
  }
  // Keep one cold DB for retrieval probes.
  const coldDbPath = join(root, `cold-${REPS - 1}.db`);

  // ---- warm sync + retrieval (shared DB) ----
  const warmSyncTimings = [];
  const warmRetrievalTimings = [];
  const retrieval = { baseline: [], codegraph: [] };
  const coverage = { baseline: [], codegraph: [] };
  const rawRows = [];

  for (let index = 0; index < REPS; index += 1) {
    const started = performance.now();
    const sync = upstreamRun(dbPath, ["codegraph", "sync", "--project", "lekalo-core"], { cwd: coreRoot });
    warmSyncTimings.push(Math.round(performance.now() - started));
    assert.equal(sync.status, 0, `warm sync failed: ${sync.stderr.slice(0, 200)}`);
  }

  // ---- per-task retrieval: baseline vs codegraph, warm ----
  for (const task of TASKS) {
    const baseStart = performance.now();
    const baseAggregate = task.grepNeedles.map((needle) => baselineSearch(coreRoot, needle));
    const baseMs = Math.round(performance.now() - baseStart);
    const baseHits = [...new Set(baseAggregate.flatMap((entry) => entry.hits))];
    const baseCovered = task.expectedFiles.every((expected) => baseHits.includes(expected));
    retrieval.baseline.push(baseMs);
    coverage.baseline.push(baseCovered);

    // codegraph retrieval: status + search + context through MCP.
    // Upstream codegraph tools require an explicit project selector;
    // the group scope must allow it (denial would fail the benchmark).
    const cgStart = performance.now();
    const { results } = mcpCall(dbPath, consumerRoot, [
      { name: "workspace_context" },
      ...task.codegraphQueries.map((query) => ({ name: "codegraph_search", arguments: { query, limit: 10, project: "lekalo-core" } })),
    ]);
    const cgMs = Math.round(performance.now() - cgStart);
    retrieval.codegraph.push(cgMs);
    const context = results[0];
    assert.equal(context.denied, false, "workspace_context denied in benchmark scope");
    for (const entry of results.slice(1)) {
      assert.equal(entry.denied, false, `codegraph_search denied: ${JSON.stringify(entry.text ?? "").slice(0, 120)}`);
    }
    const searchHits = results.slice(1).flatMap((entry) => (Array.isArray(entry.json) ? entry.json : (entry.json?.results ?? entry.json?.nodes ?? [])));
    const searchPaths = JSON.stringify(searchHits);
    const cgCovered = task.expectedFiles.every((expected) => searchPaths.includes(expected) || searchPaths.includes(expected.split("/").pop()));
    coverage.codegraph.push(cgCovered);
    const snippetLeak = /fn\s+\w+\s*\([^)]*\)\s*\{/.test(JSON.stringify(searchHits));
    rawRows.push({
      task: task.id, baseMs, baseHits, baseCovered, cgMs, cgCovered,
      codegraphSnippetLeakageSuspected: snippetLeak,
      searchResultCount: searchHits.length,
    });
  }

  // ---- changed-file phase: edit after sync must surface staleness ----
  // Upstream codegraph_status carries no staleness field at the pin
  // (file/node/edge counts + last_indexed_at only), so staleness is
  // proven behaviorally: a new unique symbol in the live file is
  // INVISIBLE to codegraph until the next sync while a live-file grep
  // finds it — that divergence is the recorded stale evidence.
  const changedPath = join(coreRoot, CHANGE_FILES[0]);
  const original = readFileSync(changedPath, "utf8");
  const probeSymbol = "fn staleprobesym() {}";
  writeFileSync(changedPath, `${original}\n${probeSymbol}\n`);
  const staleProbe = mcpCall(dbPath, consumerRoot, [
    { name: "codegraph_status", arguments: { project: "lekalo-core" } },
    { name: "codegraph_search", arguments: { query: "staleprobesym", limit: 5, project: "lekalo-core" } },
  ]);
  const staleAfterEdit = !JSON.stringify(staleProbe.results[1].json ?? staleProbe.results[1].text).includes("staleprobesym");
  const liveFileSeesIt = readFileSync(changedPath, "utf8").includes(probeSymbol);
  assert.ok(staleAfterEdit, "codegraph served the post-edit symbol without a sync (no staleness gap to surface)");
  assert.ok(liveFileSeesIt, "probe symbol missing from the live file");
  const sync2 = upstreamRun(dbPath, ["codegraph", "sync", "--project", "lekalo-core"], { cwd: coreRoot });
  assert.equal(sync2.status, 0);
  const freshProbe = mcpCall(dbPath, consumerRoot, [
    { name: "codegraph_search", arguments: { query: "staleprobesym", limit: 5, project: "lekalo-core" } },
  ]);
  const freshAfterSync = JSON.stringify(freshProbe.results[0].json ?? freshProbe.results[0].text).includes("staleprobesym");
  if (process.env.BENCH_DEBUG) {
    process.stderr.write(`${JSON.stringify({ freshProbe: freshProbe.results[0].text.slice(0, 300) })}\n`);
  }
  writeFileSync(changedPath, original); // restore; scope stays pinned
  const sync3 = upstreamRun(dbPath, ["codegraph", "sync", "--project", "lekalo-core"], { cwd: coreRoot });
  assert.equal(sync3.status, 0);

  // ---- revoked-share phase: unshare, then read must be refused ----
  const sharedItemsBefore = upstreamRun(dbPath, ["--config", ".ai-workspace.local.json", "status"], { cwd: coreRoot });
  assert.equal(sharedItemsBefore.status, 0);
  for (const relPath of CHANGE_FILES) {
    const rm = upstreamRun(dbPath, ["rm", relPath], { cwd: coreRoot });
    assert.equal(rm.status, 0, `unshare failed: ${relPath}: ${rm.stderr.slice(0, 200)}`);
  }
  const revoked = mcpCall(dbPath, consumerRoot, [
    { name: "codegraph_search", arguments: { query: "check_frozen_refs", limit: 5, project: "lekalo-core" } },
    { name: "workspace_read", arguments: { rel_path: CHANGE_FILES[0], project_id: 1 } },
  ]);
  const revokedSearch = revoked.results[0];
  const revokedSearchEmpty = revokedSearch.denied
    || (Array.isArray(revokedSearch.json) ? revokedSearch.json : []).length === 0
    || !JSON.stringify(revokedSearch.json ?? {}).includes("check_frozen_refs");
  const revokedRead = revoked.results[1];
  const revokedReadDenied = revokedRead.denied || revokedRead.text === ""
    || /denied|not shared|Access denied/i.test(revokedRead.text);
  assert.ok(revokedSearchEmpty, "revoked scope still returned codegraph results");
  assert.ok(revokedReadDenied, "revoked share still readable");

  // ---- aggregate (closed public shape; digests/counts/timings only) ----
  const scopeDigest = sha256Ref(CHANGE_FILES.map((file) => readFileSync(join(coreRoot, file))).map((buffer) => sha256Ref(buffer)).join("\n"));
  const aggregate = {
    schema: "lekalo/ai-workspace-context-benchmark/v0.6.3",
    upstreamCommit: UPSTREAM_COMMIT,
    upstreamPackage: "1.5.0",
    baseCommit: BASE_COMMIT,
    headCommit: HEAD_COMMIT,
    changeFiles: CHANGE_FILES,
    changeScopeDigest: scopeDigest,
    repetitions: REPS,
    coldReindexMs: { median: median(coldTimings), range: range(coldTimings) },
    warmSyncMs: { median: median(warmSyncTimings), range: range(warmSyncTimings) },
    retrievalBaselineMs: { median: median(retrieval.baseline), range: range(retrieval.baseline) },
    retrievalCodegraphMs: { median: median(retrieval.codegraph), range: range(retrieval.codegraph) },
    coverageBaseline: coverage.baseline,
    coverageCodegraph: coverage.codegraph,
    stalenessAfterEditSurfaced: staleAfterEdit,
    stalenessResolvesAfterSync: freshAfterSync,
    revokedScopeDenied: { codegraphSearch: revokedSearchEmpty, workspaceRead: revokedReadDenied },
    limitations: [
      "regex-parser-provenance", // rust-regex-mvp at the pin; navigation only
      "snippet-free-aggregate", // live snippets excluded from published evidence
      "one-machine-one-run", // timings are machine-local medians, not SIs
      "scratch-copies-only", // public benchmark scope; no private checkout
    ],
  };

  // Privacy probes over the aggregate before printing.
  const aggregateText = JSON.stringify(aggregate);
  for (const leak of [tmpdir().toLowerCase(), "users\\\\", "appdata", REPO_ROOT.toLowerCase()]) {
    assert.equal(aggregateText.toLowerCase().includes(leak.replace(/\\+/g, "\\\\")), false, `aggregate leak: ${leak}`);
  }
  assert.equal(aggregateText.includes("benchmark_private_sentinel"), false, "private sentinel reached the aggregate");

  // Raw rows (with local paths) stay private: out dir if given, else
  // dropped with the scratch tree.
  if (OUT_DIR) {
    mkdirSync(OUT_DIR, { recursive: true });
    writeFileSync(join(OUT_DIR, "raw-runs.json"), `${JSON.stringify({ aggregate, rawRows, coldTimings, warmSyncTimings, retrieval }, null, 2)}\n`);
  }

  for (const dir of cleanup) rmSync(dir, { recursive: true, force: true });
  process.stdout.write(`${JSON.stringify({ ok: true, benchmark: "ai-workspace-context", aggregate, rawRowsPrivate: OUT_DIR ? OUT_DIR : "(discarded)" }, null, 2)}\n`);
} catch (error) {
  process.stderr.write(`${JSON.stringify({ ok: false, benchmark: "ai-workspace-context", reason: String(error?.message ?? error).slice(0, 600) }, null, 2)}\n`);
  for (const dir of cleanup) rmSync(dir, { recursive: true, force: true });
  process.exit(1);
}
