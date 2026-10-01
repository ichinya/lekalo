# Issue #37 — CodeGraph context benchmark (AC6)

Status: **run and reported**. This document is the acceptance evidence
for acceptance criterion 6 ("Rust CodeGraph context benchmark run on
core changes") per the benchmark protocol in
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
(with absolute paths) are private by design; the benchmark prints only
the closed aggregate and runs leak probes over it before printing.

## Navigation tasks (predeclared)

| Task | Expected file found via | Baseline | CodeGraph |
| --- | --- | --- | --- |
| custody-read-verification (where does the store re-verify record custody at read time) | `store.rs` | hit | hit |
| recovery-quarantine (where recovery quarantines divergent column/body ids) | `store.rs` | hit | hit |
| quarantine-regression-test (which test proves the divergence quarantine) | `tests.rs` | hit | hit |

## Measured results (5 repetitions per timing case)

| Case | Median | Range |
| --- | --- | --- |
| Cold reindex (fresh DB, 3-file scope) | 182 ms | 171–209 ms |
| Warm sync (unchanged scope) | 46 ms | 45–176 ms |
| Retrieval, bounded local grep baseline | 2 ms | 1–3 ms |
| Retrieval, `workspace_context` + `codegraph_search` | 43 ms | 42–45 ms |
| Coverage of expected files, baseline | 3/3 | — |
| Coverage of expected files, CodeGraph | 3/3 | — |

A second complete run reproduced the result (cold median 183 ms,
CodeGraph retrieval median 49 ms, identical coverage and staleness
outcomes). A run on this machine is dominated by process-spawn cost
(each MCP call spawns a fresh stdio server), which is why the baseline
in-process grep is faster on a three-file scope; these numbers
characterize the tool, they do not establish a general speedup claim —
the research protocol explicitly forbids preclaiming one.

## Staleness and revocation (hard gates)

- **Edit-after-sync staleness is real and surfaced.** Upstream
  `codegraph_status` carries no staleness field at the pin (file/node/
  edge counts and `last_indexed_at` only), so staleness was proven
  behaviorally: a new unique symbol appended to a shared file after
  sync is invisible to `codegraph_search` until the next
  `codegraph sync`, then visible after it
  (`stalenessAfterEditSurfaced: true`,
  `stalenessResolvesAfterSync: true`). Consequence recorded for
  integrators: a sync timestamp alone never proves source freshness —
  public CodeGraph evidence must carry the provenance wrapper from
  [`docs/integrations/ai-workspace.md`](../integrations/ai-workspace.md).
- **Revoked scope is refused, not stale-served.** After removing the
  shares, both `codegraph_search` (over the formerly indexed files)
  and `workspace_read` (by path) refuse: `revokedScopeDenied` both
  true. The upstream visibility filter rechecks current share state on
  every call, including stale indexed rows.
- **No snippet leakage in the published aggregate.** Live source
  snippets (which `codegraph_context`/`codegraph_search` results
  contain) are excluded from the printed aggregate by construction and
  probe-checked; a private sentinel file inside the core project never
  appears in any result.

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
3. **Process-spawn-dominated timings.** Each MCP call in this protocol
   spawns a fresh group-scoped server (the honest CLI-driven shape);
   long-lived MCP sessions would amortize that cost. Timing
   comparisons against an in-process baseline therefore measure the
   integration shape, not the indexer's asymptotics.
4. **One machine, one OS, debug build.** Numbers are reproducible on
   the same machine (second run within noise) but are not
   cross-environment claims. CI does not run the benchmark: it needs
   the pinned upstream binary built locally.
5. **Small scope.** Three files (the actual change surface). Indexing
   cost grows with shared scope; the benchmark intentionally measures
   the integration-relevant scope, not a full-repository index.
6. **The upstream `codegraph_status` staleness gap** (finding in the
   staleness section) is recorded as an upstream gap in the
   implementation report; the benchmark's behavioral probe is the
   lekalo-side mitigation, not a fix.
