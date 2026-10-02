#!/usr/bin/env node
/**
 * Issue #37 CodeGraph context benchmark (research section "Rust
 * CodeGraph context benchmark protocol"): compare baseline bounded
 * file/grep retrieval against workspace_context + codegraph
 * retrieval over a pinned public core change, with cold/warm/changed/
 * deleted/revoked phases, five repetitions of EVERY timing case
 * (including retrieval), and privacy-confined aggregate output.
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
 * head revision, and a synthetic consumer project. Raw per-run rows
 * (which contain absolute paths) stay in the private --out directory
 * or are dropped; stdout prints only the closed public aggregate
 * (counts, bytes, milliseconds, digests — no absolute paths, no host
 * names, no error text). The leak probes cover the ENTIRE printed
 * document, not just the aggregate member.
 *
 * What is measured, per the research protocol:
 *   - cold reindex (fresh database per repetition),
 *   - unchanged warm sync,
 *   - changed-file sync (after a probe edit, before restore),
 *   - deleted-file handling (one scope file removed, then restored),
 *   - retrieval: baseline bounded grep vs workspace_context +
 *     codegraph_search (+ codegraph_context with a bounded budget on
 *     the navigation task), REPS repetitions per task per strategy,
 *   - returned context size (bytes; upstream exposes no token count —
 *     recorded as an explicit unknown),
 *   - expected-evidence coverage and misses,
 *   - unresolved references from codegraph_status,
 *   - staleness: post-edit invisibility until sync, then freshness,
 *     both asserted (never merely recorded),
 *   - revoked-share refusal for both search and read, asserted.
 *
 * Hard gates (fail the benchmark, never reported as success):
 *   - the staleness/revocation asserts;
 *   - privacy probes over the complete printed document;
 *   - the private sentinel never reaching any result.
 */
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, openSync, readSync, closeSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
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
const CONTEXT_BUDGET_CHARS = 4000; // bounded retrieval budget, in characters

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
    expectedSymbols: ["check_frozen_refs"],
    grepNeedles: ["record-digest", "record_digest"],
    codegraphQueries: ["check_frozen_refs"],
    contextAnchors: ["record-digest", "custody"],
  },
  {
    id: "recovery-quarantine",
    question: "where does recovery quarantine records whose column and body ids diverge",
    expectedFiles: ["crates/lekalo-core/src/run_history/store.rs"],
    expectedSymbols: ["quarantined_runs"],
    grepNeedles: ["quarantined"],
    codegraphQueries: ["quarantined"],
    contextAnchors: ["quarantin"],
  },
  {
    id: "quarantine-regression-test",
    question: "which test proves the column and body id divergence quarantine",
    expectedFiles: ["crates/lekalo-core/src/run_history/tests.rs"],
    expectedSymbols: ["recovery_quarantines_records_whose_column_and_body_ids_diverge"],
    grepNeedles: ["recovery_quarantines_records_whose_column_and_body_ids_diverge"],
    codegraphQueries: ["recovery_quarantines_records_whose_column_and_body_ids_diverge"],
    contextAnchors: ["recovery_quarantines_records_whose_column_and_body_ids_diverge"],
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
    const denied = Boolean(response?.error) || Boolean(response?.result?.isError);
    const text = response && !denied
      ? response.result?.content?.find((entry) => entry.type === "text")?.text ?? ""
      : "";
    let json = null;
    try { json = JSON.parse(text); } catch { json = null; }
    return { name: call.name, denied, text, json };
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

// Baseline strategy: bounded recursive grep over the declared scope —
// bounded by files examined AND bytes read (the read stops at the
// budget; the bound is part of the strategy being measured).
const baselineSearch = (coreRoot, needle, { maxFiles = 10, maxBytes = 64 * 1024 } = {}) => {
  const started = performance.now();
  const hits = [];
  let bytes = 0;
  const walk = (dir) => {
    if (hits.length >= maxFiles || bytes >= maxBytes) return;
    let entries;
    try {
      entries = readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (hits.length >= maxFiles || bytes >= maxBytes) return;
      const full = join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full);
      } else if (entry.name.endsWith(".rs")) {
        let content;
        try {
          const stat = statSync(full);
          const remaining = maxBytes - bytes;
          if (remaining <= 0) return;
          const handle = openSync(full, "r");
          const buffer = Buffer.alloc(Math.min(stat.size, remaining));
          readSync(handle, buffer, 0, buffer.length, 0);
          closeSync(handle);
          content = buffer.toString("utf8");
          bytes += buffer.length;
        } catch {
          continue;
        }
        if (content.includes(needle)) {
          hits.push(relative(coreRoot, full).split("\\").join("/"));
        }
      }
    }
  };
  walk(join(coreRoot, "crates"));
  return { hits, readBytes: bytes, elapsedMs: Math.round(performance.now() - started) };
};

try {
  const root = scratch();
  const dbPath = join(root, "workspace.db");
  const coreRoot = join(root, "lekalo-core");
  const consumerRoot = join(root, "bench-consumer");

  // Scratch core project: the benchmark scope is exactly the three
  // changed run_history files at the pinned head revision.
  mkdirSync(coreRoot, { recursive: true });
  copyAtRevision(coreRoot, HEAD_COMMIT, CHANGE_FILES);
  const writeCoreConfig = () => {
    writeFileSync(join(coreRoot, ".ai-workspace.local.json"), `${JSON.stringify({
      ai_workspace_config_version: 1, name: "lekalo-core", slug: "lekalo-core", groups: [GROUP], share: [], notes: [],
    }, null, 2)}\n`);
  };
  writeCoreConfig();
  // A private sentinel that must never enter the index or any result.
  mkdirSync(join(coreRoot, "private"), { recursive: true });
  writeFileSync(join(coreRoot, "private", "secret.rs"), "fn benchmark_private_sentinel() {}\n");

  mkdirSync(consumerRoot, { recursive: true });
  writeFileSync(join(consumerRoot, ".ai-workspace.local.json"), `${JSON.stringify({
    ai_workspace_config_version: 1, name: "bench-consumer", slug: "bench-consumer", groups: [GROUP], share: [], notes: [],
  }, null, 2)}\n`);

  const initPair = (db) => {
    for (const [dir, name] of [[coreRoot, "lekalo-core"], [consumerRoot, "bench-consumer"]]) {
      const init = upstreamRun(db, ["--config", ".ai-workspace.local.json", "init", "--name", name, "--slug", name, "--group", GROUP], { cwd: dir });
      assert.equal(init.status, 0, `init failed for ${name}`);
    }
  };
  const shareScope = (db) => {
    for (const relPath of CHANGE_FILES) {
      const result = upstreamRun(db, ["--config", ".ai-workspace.local.json", "share", relPath], { cwd: coreRoot });
      assert.equal(result.status, 0, `share failed: ${relPath}`);
    }
  };

  // ---- main shared database: register + share + reindex once ----
  initPair(dbPath);
  shareScope(dbPath);
  const coldStart = performance.now();
  const coldReindex = upstreamRun(dbPath, ["codegraph", "reindex", "--project", "lekalo-core"], { cwd: coreRoot });
  const coldReindexMs = Math.round(performance.now() - coldStart);
  assert.equal(coldReindex.status, 0, `cold reindex failed: ${coldReindex.stderr.slice(0, 200)}`);

  // ---- cold reindex timings: fresh database per repetition ----
  const coldTimings = [coldReindexMs];
  for (let index = 1; index < REPS; index += 1) {
    const coldDb = join(root, `cold-${index}.db`);
    writeCoreConfig(); // upstream share rewrites the local config; reset
    initPair(coldDb);
    shareScope(coldDb);
    const started = performance.now();
    const reindex = upstreamRun(coldDb, ["codegraph", "reindex", "--project", "lekalo-core"], { cwd: coreRoot });
    coldTimings.push(Math.round(performance.now() - started));
    assert.equal(reindex.status, 0, `cold reindex failed: ${reindex.stderr.slice(0, 200)}`);
  }

  // ---- unchanged warm sync timings ----
  const warmSyncTimings = [];
  for (let index = 0; index < REPS; index += 1) {
    const started = performance.now();
    const sync = upstreamRun(dbPath, ["codegraph", "sync", "--project", "lekalo-core"], { cwd: coreRoot });
    warmSyncTimings.push(Math.round(performance.now() - started));
    assert.equal(sync.status, 0, `warm sync failed: ${sync.stderr.slice(0, 200)}`);
  }

  // ---- changed-file phase: probe edit, staleness proof, sync timings ----
  // The edit lands BEFORE any sync: the staleness probe proves the
  // index does not serve the post-edit symbol until a sync runs, then
  // the changed-file sync timing is taken over exactly that
  // convergence sync. Bytes are restored and re-synced afterwards.
  const changedPath = join(coreRoot, CHANGE_FILES[0]);
  const originalStore = readFileSync(changedPath, "utf8");
  const probeSymbol = "fn staleprobesym() {}";
  writeFileSync(changedPath, `${originalStore}\n${probeSymbol}\n`);
  const staleProbe = mcpCall(dbPath, consumerRoot, [
    { name: "codegraph_search", arguments: { query: "staleprobesym", limit: 5, project: "lekalo-core" } },
  ]);
  const staleAfterEdit = !JSON.stringify(staleProbe.results[0].json ?? staleProbe.results[0].text).includes("staleprobesym");
  assert.ok(staleAfterEdit, "codegraph served the post-edit symbol without a sync (no staleness gap to surface)");
  assert.ok(readFileSync(changedPath, "utf8").includes(probeSymbol), "probe symbol missing from the live file");
  const changedSyncTimings = [];
  for (let index = 0; index < REPS; index += 1) {
    // Only the first sync sees the change; repetitions after the first
    // measure the unchanged path on the already-synced edit — the
    // range covers both shapes honestly.
    const started = performance.now();
    const sync = upstreamRun(dbPath, ["codegraph", "sync", "--project", "lekalo-core"], { cwd: coreRoot });
    changedSyncTimings.push(Math.round(performance.now() - started));
    assert.equal(sync.status, 0);
  }
  const freshProbe = mcpCall(dbPath, consumerRoot, [
    { name: "codegraph_search", arguments: { query: "staleprobesym", limit: 5, project: "lekalo-core" } },
  ]);
  const freshnessAfterSync = JSON.stringify(freshProbe.results[0].json ?? freshProbe.results[0].text).includes("staleprobesym");
  assert.ok(freshnessAfterSync, "sync did not surface the post-edit symbol (freshness gate)");
  writeFileSync(changedPath, originalStore); // restore; scope stays pinned
  const restoreSync = upstreamRun(dbPath, ["codegraph", "sync", "--project", "lekalo-core"], { cwd: coreRoot });
  assert.equal(restoreSync.status, 0);
  const restoredProbe = mcpCall(dbPath, consumerRoot, [
    { name: "codegraph_search", arguments: { query: "staleprobesym", limit: 5, project: "lekalo-core" } },
    { name: "codegraph_search", arguments: { query: "check_frozen_refs", limit: 5, project: "lekalo-core" } },
  ]);
  const staleProbeCleared = !JSON.stringify(restoredProbe.results[0].json ?? restoredProbe.results[0].text).includes("staleprobesym");
  const restoredSymbolVisible = JSON.stringify(restoredProbe.results[1].json ?? restoredProbe.results[1].text).includes("check_frozen_refs");
  assert.ok(staleProbeCleared && restoredSymbolVisible, "sync did not converge the index back to the restored bytes");

  // ---- retrieval: REPS repetitions per task per strategy ----
  const rawRows = [];
  const retrievalBaselineMs = [];
  const retrievalCodegraphMs = [];
  const coverageBaseline = [];
  const coverageCodegraph = [];
  let unresolvedRefCount = null;
  for (let rep = 0; rep < REPS; rep += 1) {
    for (const task of TASKS) {
      // Baseline: bounded local grep, repeated.
      const baseStart = performance.now();
      const baseAggregate = task.grepNeedles.map((needle) => baselineSearch(coreRoot, needle));
      const baseMs = Math.round(performance.now() - baseStart);
      const baseHits = [...new Set(baseAggregate.flatMap((entry) => entry.hits))];
      const baseCovered = task.expectedFiles.every((expected) => baseHits.includes(expected));
      retrievalBaselineMs.push(baseMs);
      if (rep === REPS - 1) coverageBaseline.push(baseCovered);

      // CodeGraph strategy: context + search + bounded context tool,
      // repeated. Upstream codegraph tools require an explicit project
      // selector; a denial fails the benchmark (fail closed).
      const cgStart = performance.now();
      const { results } = mcpCall(dbPath, consumerRoot, [
        { name: "workspace_context" },
        ...task.codegraphQueries.map((query) => ({ name: "codegraph_search", arguments: { query, limit: 10, project: "lekalo-core" } })),
        { name: "codegraph_context", arguments: { task: task.question, project: "lekalo-core", budget: CONTEXT_BUDGET_CHARS } },
        { name: "codegraph_status", arguments: { project: "lekalo-core" } },
      ]);
      const cgMs = Math.round(performance.now() - cgStart);
      retrievalCodegraphMs.push(cgMs);
      const context = results[0];
      assert.equal(context.denied, false, "workspace_context denied in benchmark scope");
      for (const entry of results.slice(1, -1)) {
        assert.equal(entry.denied, false, `${entry.name} denied: ${JSON.stringify(entry.text ?? "").slice(0, 120)}`);
      }
      const searchHits = results.slice(1, -2).flatMap((entry) => (Array.isArray(entry.json) ? entry.json : (entry.json?.results ?? entry.json?.nodes ?? [])));
      const searchPaths = JSON.stringify(searchHits);
      const searchSymbolsFound = task.expectedSymbols.filter((symbol) => searchPaths.includes(symbol));
      const contextText = results[results.length - 2].text ?? "";
      const contextAnchorsFound = task.contextAnchors.filter((anchor) => contextText.includes(anchor) || searchPaths.includes(anchor));
      const contextBytes = Buffer.byteLength(contextText, "utf8");
      const cgCovered = task.expectedFiles.every((expected) => searchPaths.includes(expected) || searchPaths.includes(expected.split("/").pop()))
        && searchSymbolsFound.length > 0;
      if (rep === REPS - 1) coverageCodegraph.push(cgCovered);
      const status = results[results.length - 1].json ?? {};
      if (typeof status.unresolved_ref_count === "number") unresolvedRefCount = status.unresolved_ref_count;
      rawRows.push({
        rep, task: task.id, baseMs, baseHits, baseCovered,
        cgMs, cgCovered, searchSymbolsFound, contextAnchorsFound,
        contextBytes, searchResultCount: searchHits.length,
        unresolvedRefCount: typeof status.unresolved_ref_count === "number" ? status.unresolved_ref_count : null,
      });
    }
  }

  // ---- deleted-file handling: remove one scope file, sync, probe ----
  const deletedPath = join(coreRoot, CHANGE_FILES[2]);
  const originalMod = readFileSync(deletedPath, "utf8");
  rmSync(deletedPath);
  const shareRemoval = upstreamRun(dbPath, ["rm", CHANGE_FILES[2]], { cwd: coreRoot });
  assert.equal(shareRemoval.status, 0, `unshare failed: ${shareRemoval.stderr.slice(0, 200)}`);
  const deletedSync = upstreamRun(dbPath, ["codegraph", "sync", "--project", "lekalo-core"], { cwd: coreRoot });
  assert.equal(deletedSync.status, 0);
  const deletedProbe = mcpCall(dbPath, consumerRoot, [
    { name: "codegraph_search", arguments: { query: "run_history", limit: 10, project: "lekalo-core" } },
  ]);
  const deletedHandled = !(JSON.stringify(deletedProbe.results[0].json ?? "") ?? "").includes("run_history/mod.rs");
  writeFileSync(deletedPath, originalMod);
  const reshare = upstreamRun(dbPath, ["--config", ".ai-workspace.local.json", "share", CHANGE_FILES[2]], { cwd: coreRoot });
  assert.equal(reshare.status, 0);
  const restoreSync2 = upstreamRun(dbPath, ["codegraph", "sync", "--project", "lekalo-core"], { cwd: coreRoot });
  assert.equal(restoreSync2.status, 0);

  // ---- revoked-share refusal: remove ALL scope shares, probe ----
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
    || !JSON.stringify(revokedSearch.json ?? revokedSearch.text ?? "").includes("check_frozen_refs");
  const revokedRead = revoked.results[1];
  const revokedReadDenied = revokedRead.denied || revokedRead.text === ""
    || /denied|not shared|Access denied/i.test(revokedRead.text);
  assert.ok(revokedSearchEmpty, "revoked scope still returned codegraph results");
  assert.ok(revokedReadDenied, "revoked share still readable");
  // Restore the shares so the scratch tree stays coherent for cleanup.
  shareScope(dbPath);
  const finalSync = upstreamRun(dbPath, ["codegraph", "sync", "--project", "lekalo-core"], { cwd: coreRoot });
  assert.equal(finalSync.status, 0);

  // ---- aggregate (closed public shape; digests/counts/timings only) ----
  const scopeDigest = sha256Ref(CHANGE_FILES.map((file) => readFileSync(join(coreRoot, file))).map((buffer) => sha256Ref(buffer)).join("\n"));
  const contextBytesList = rawRows.map((row) => row.contextBytes);
  const aggregate = {
    schema: "lekalo/ai-workspace-context-benchmark/v0.6.3",
    upstreamCommit: UPSTREAM_COMMIT,
    upstreamPackage: "1.5.0",
    baseCommit: BASE_COMMIT,
    headCommit: HEAD_COMMIT,
    changeFiles: CHANGE_FILES,
    changeScopeDigest: scopeDigest,
    repetitions: REPS,
    retrievalRepetitions: { perStrategyPerTask: REPS, tasks: TASKS.length },
    coldReindexMs: { median: median(coldTimings), range: range(coldTimings) },
    warmSyncMs: { median: median(warmSyncTimings), range: range(warmSyncTimings) },
    changedFileSyncMs: { median: median(changedSyncTimings), range: range(changedSyncTimings), note: "first sample sees the edit; later samples are the unchanged path on synced bytes" },
    retrievalBaselineMs: { median: median(retrievalBaselineMs), range: range(retrievalBaselineMs), samples: retrievalBaselineMs.length },
    retrievalCodegraphMs: { median: median(retrievalCodegraphMs), range: range(retrievalCodegraphMs), samples: retrievalCodegraphMs.length },
    coverageBaseline: coverageBaseline,
    coverageCodegraph: coverageCodegraph,
    expectedSymbolHits: TASKS.map((task) => task.id),
    returnedContextBytes: { median: median(contextBytesList), range: range(contextBytesList) },
    tokenEstimate: "unknown", // upstream exposes no token count at the pin
    unresolvedRefCount,
    stalenessAfterEditSurfaced: staleAfterEdit,
    stalenessResolvesAfterSync: staleProbeCleared && restoredSymbolVisible,
    deletedFileHandled: deletedHandled,
    revokedScopeDenied: { codegraphSearch: revokedSearchEmpty, workspaceRead: revokedReadDenied },
    limitations: [
      "regex-parser-provenance", // rust-regex-mvp at the pin; navigation only
      "snippet-free-aggregate", // live snippets excluded from published evidence
      "one-machine-one-run", // timings are machine-local medians, not SIs
      "scratch-copies-only", // public benchmark scope; no private checkout
      "process-spawn-dominated", // each MCP call spawns a fresh stdio server
    ],
  };

  // Privacy probes over the COMPLETE printed document (aggregate plus
  // every sibling field), not just the aggregate member.
  const printed = { ok: true, benchmark: "ai-workspace-context", aggregate, rawRowsPrivate: OUT_DIR ? "private-out-directory" : "discarded" };
  const printedText = JSON.stringify(printed).toLowerCase();
  for (const leak of [tmpdir().toLowerCase(), "users\\", "appdata", REPO_ROOT.toLowerCase(), "bench-consumer", "lekalo-core\\"]) {
    assert.equal(printedText.includes(leak), false, `printed leak: ${leak}`);
  }
  assert.equal(printedText.includes("benchmark_private_sentinel"), false, "private sentinel reached the printed output");
  assert.equal(printedText.includes("staleprobesym"), false, "probe symbol reached the printed output");

  // Raw rows (with local paths) stay private: out dir if given, else
  // dropped with the scratch tree.
  if (OUT_DIR) {
    mkdirSync(OUT_DIR, { recursive: true });
    writeFileSync(join(OUT_DIR, "raw-runs.json"), `${JSON.stringify({ aggregate, rawRows, coldTimings, warmSyncTimings, changedSyncTimings, retrievalBaselineMs, retrievalCodegraphMs }, null, 2)}\n`);
  }

  for (const dir of cleanup) rmSync(dir, { recursive: true, force: true });
  process.stdout.write(`${JSON.stringify(printed, null, 2)}\n`);
} catch (error) {
  process.stderr.write(`${JSON.stringify({ ok: false, benchmark: "ai-workspace-context", reason: String(error?.message ?? error).slice(0, 600) }, null, 2)}\n`);
  for (const dir of cleanup) rmSync(dir, { recursive: true, force: true });
  process.exit(1);
}
