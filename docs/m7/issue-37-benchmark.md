# Issue #37 — CodeGraph context benchmark (AC6)

Status: **run and reported** (fix round 1: the retrieval measurement is
now a real repeated per-task comparison per the research protocol; the
first-round "one sample per task" measurement was superseded). This
document is the acceptance evidence for acceptance criterion 6
("Rust CodeGraph context benchmark run on core changes") per the
benchmark protocol in
[`docs/m7/issue-37-research.md`](issue-37-research.md). Reproduce with:

```sh
node scripts/benchmark-ai-workspace-context.mjs \
  --upstream <pinned ai-workspace binary> --reps 5
```

## Pinned inputs

| Input | Value |
| --- | --- |
| Upstream revision | `lee-to/ai-workspace@8fdf818fee757d24e723d657fc5d38614995e557` (package 1.5.0, debug build) |
| Producer under test | lekalo core change `3d7cfcfb87576147ec0b43dcc2380ddfeb677193` against first parent `1d1f14a79e99fba5c02a9ad75a1b9be6875cea27` (the #121 custody re-verification + recovery quarantine fix) |
| Shared scope | exactly the three changed files at the head revision (`crates/lekalo-core/src/run_history/{store,tests,mod}.rs`), scratch copies in the OS temp directory, dedicated benchmark database, group-scoped server with the widening flags forced off |
| Scope digest | `sha256:79f5c64acc0844efbf39f33d0cc84b9f087500b90c37a03d6c160be5c79e0b32` (per-file sha256 chain over the exact shared bytes) |
| Machine | one Windows development machine; timings are local medians, not controlled-environment SIs |

Machine-readable result: [`issue-37-benchmark.json`](issue-37-benchmark.json)
(schema `lekalo/ai-workspace-context-benchmark/v0.6.3`). Raw per-run rows
(with absolute paths) are private by design; the benchmark probes the
**complete printed document** for leaks before printing it.

## Navigation tasks (predeclared)

| Task | Expected evidence | Baseline covered | CodeGraph covered |
| --- | --- | --- | --- |
| custody-read-verification | `store.rs` + `check_frozen_refs` | yes | yes |
| recovery-quarantine | `store.rs` + `quarantined_runs` | yes | **no** (search hit the file; the `quarantined` FTS query did not surface the symbol within the top results) |
| quarantine-regression-test | `tests.rs` + the regression test name | **no** (the bounded baseline's 64 KiB read budget was consumed by the larger `store.rs` before `tests.rs` was visited) | yes |

Coverage is **measured, not asserted**: a miss is recorded truthfully
and fails nothing. The two misses above are themselves findings: the
baseline's byte budget is a real retrieval constraint, and upstream's
FTS ranking is not symbol-exact. Neither strategy covered all three
tasks.

## Measured results (5 repetitions of EVERY timing case)

| Case | Median | Range | Samples |
| --- | --- | --- | --- |
| Cold reindex (fresh DB, 3-file scope) | 174 ms | 155–198 ms | 5 |
| Unchanged warm sync | 56 ms | 55–61 ms | 5 |
| Changed-file sync (first sample sees the edit) | 60 ms | 53–234 ms | 5 |
| Retrieval, bounded local grep baseline | 1 ms | 1–2 ms | 15 (3 tasks × 5 reps) |
| Retrieval, `workspace_context` + `codegraph_search` + `codegraph_context` | 73 ms | 62–97 ms | 15 (3 tasks × 5 reps) |
| Returned `codegraph_context` size | 10 283 chars | 9 664–10 616 | 15 |
| Token estimate | **unknown** | — | — (upstream exposes no token count at the pin) |
| Unresolved references (`codegraph_status`) | 953 | — | — |

A second complete run reproduced the result (cold 187 ms, warm 55 ms,
baseline 1 ms, CodeGraph 77 ms, identical staleness/deletion/revocation
outcomes). A run on this machine is dominated by process-spawn cost
(each MCP call spawns a fresh stdio server), which is why the baseline
in-process grep is faster on a three-file scope; these numbers
characterize the tool, they do not establish a general speedup claim —
the research protocol explicitly forbids preclaiming one.

## Staleness, change, deletion, revocation (hard gates)

- **Edit-after-sync staleness is real and surfaced.** Upstream
  `codegraph_status` carries no staleness field at the pin (file/node/
  edge counts and `last_indexed_at` only), so staleness is proven
  behaviorally: a new unique symbol appended to a shared file after
  sync is invisible to `codegraph_search` until the next
  `codegraph sync`, then visible after it
  (`stalenessAfterEditSurfaced: true`,
  `stalenessResolvesAfterSync: true`; both **asserted** — the benchmark
  fails if either stops holding). Consequence recorded for
  integrators: a sync timestamp alone never proves source freshness —
  public CodeGraph evidence must carry the provenance wrapper from
  [`docs/integrations/ai-workspace.md`](../integrations/ai-workspace.md).
- **Changed-file sync measured** over exactly the convergence sync
  (first sample 234 ms, subsequent unchanged-path samples 53–60 ms —
  the range honestly spans both shapes).
- **Deletion is handled.** After removing one scope file and its
  share, a sync drops it from the graph (`deletedFileHandled: true`);
  the file was then restored and re-shared.
- **Revoked scope is refused, not stale-served.** After removing all
  scope shares, both `codegraph_search` (over the formerly indexed
  files) and `workspace_read` (by path) refuse:
  `revokedScopeDenied` both true (**asserted**).
- **No snippet leakage in the published aggregate.** Live source
  snippets are excluded from the printed aggregate by construction and
  probe-checked; a private sentinel file inside the core project never
  appears in any result, and the probe symbol never reaches stdout.

## Findings and limitations (explicit)

1. **Navigation-only evidence.** Upstream's parser is the conservative
   regex MVP (`rust-regex-mvp` / `rust-regex-mvp-resolver` provenance
   strings at the pin). Its results are never semantic edges, never
   impact proof, and never a substitute for `lekalo inspect`/`impact`
   (research B6). The benchmark used it strictly as a navigation index.
2. **No revision/hash provenance upstream.** Search results carry file
   path, line spans, signatures — no Git revision, no file hash, no
   parser version, no confidence envelope. The benchmark pins these at
   the wrapper level; upstream enrichment remains a documented gap.
3. **Neither strategy solved all three tasks.** Baseline missed the
   regression-test task under its read budget; CodeGraph missed the
   quarantine symbol under FTS ranking. "Which tool is better" is
   task-dependent; the benchmark records both misses instead of tuning
   either side until it wins.
4. **Process-spawn-dominated timings.** Each MCP call in this protocol
   spawns a fresh group-scoped server (the honest CLI-driven shape);
   long-lived MCP sessions would amortize that cost. Timing
   comparisons against an in-process baseline therefore measure the
   integration shape, not the indexer's asymptotics.
5. **One machine, one OS, debug build.** Numbers are reproducible on
   the same machine (second run within noise) but are not
   cross-environment claims. CI does not run the benchmark: it needs
   the pinned upstream binary built locally.
6. **Small scope.** Three files (the actual change surface). Indexing
   cost grows with shared scope; the benchmark intentionally measures
   the integration-relevant scope, not a full-repository index.
7. **Token cost remains unknown.** Upstream exposes no token estimate
   at the pin; the aggregate records `tokenEstimate: "unknown"` rather
   than a fabricated conversion.
8. **The upstream `codegraph_status` staleness gap** is recorded as an
   upstream gap in the implementation report; the benchmark's
   behavioral proof is the lekalo-side mitigation, not a fix.
