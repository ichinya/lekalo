# Issue #37 — independent review (Cline)

Reviewer: Cline. Review-only: no code under review was modified. Scope:
`git diff origin/ichinya/M7...HEAD` (base `9510dd07`, head `0abbec6d`,
13 commits). Authority read first: `docs/m7/issue-37-research.md`,
`docs/m7/issue-37-implementation.md`. Diff stat: 16 files, +4199/-0,
zero Rust/crate changes.

## Verdict: ISSUES

1 blocker, 3 major, 3 minor. The design, boundaries, privacy discipline,
and contract discipline are genuinely strong and mostly match the
documentation. The blockers are **gate/evidence integrity**, not design:
a committed test gate fails deterministically, the committed benchmark
script cannot produce the committed benchmark evidence, and the working
tree holds a large uncommitted rewrite plus a stash. The branch is not
mergeable as-is.

## Findings

### 1. BLOCKER — committed integration gate fails deterministically

`scripts/test-ai-workspace-hook.mjs:191-195` (Phase B) asserts that
`node scripts/ai-workspace-hook.mjs --base HEAD --manifest <fixture>`
returns `state: "planned"`. It cannot, at any commit on this branch.

Root cause: `--base HEAD` with no `--head` makes
`scripts/ai-workspace-hook.mjs:699` resolve head to the same commit
(`resolveRevision("HEAD")`), so base === head. `buildArtifacts`
(`scripts/ai-workspace-hook.mjs:381-397`) compares identical revisions,
produces zero artifacts, and the hook correctly takes the no-change exit
at `scripts/ai-workspace-hook.mjs:711-714` → `state: "no-change"`.

Verified (pinned Ajv 8.17.1 provisioned as CI does):

```
$ NODE_PATH=... node scripts/test-ai-workspace-hook.mjs
{"ok":false,"gate":"ai-workspace-hook",
 "reason":"Expected values to be strictly equal: + 'no-change' - 'planned'"}
```

The same invocation directly confirms it:

```
$ node scripts/ai-workspace-hook.mjs --base HEAD --manifest tests/fixtures/ai-workspace/routing-manifest.json
{"ok":true,"state":"no-change","reason":"no-approved-artifact-changed",
 "base":"0abbec6d…","head":"0abbec6d…"}
```

Impact: `docs/m7/issue-37-implementation.md:101-102` claims
"`node scripts/test-ai-workspace-hook.mjs` — pass, both tiers", and
`:80-81` claims "Executed end to end on this machine against the pinned
binary: all phases green." That claim is not reproducible from the
committed tree. Phases A–C (the always-on, dependency-free tier) cannot
be green as committed. The gate is **not** wired into
`.github/workflows/ci.yml` (only `test-ai-workspace-contracts.mjs` is,
at `ci.yml:80`), which is why CI stayed green while this broke.

Note the hook itself is behaving correctly here — the bug is in the
gate's fixture choice, not the production logic. A fix is small (use a
base that actually contains an approved-path change, e.g. the pinned
`4a084aab~1..4a084aab` already referenced in the docs), but it must be
committed and the doc claims corrected.

### 2. BLOCKER — committed benchmark script cannot produce the committed evidence

`docs/m7/issue-37-benchmark.json:14-51` reports
`coldReindexMs`, `warmSyncMs`, `retrievalBaselineMs`,
`retrievalCodegraphMs`, `coverageBaseline`, `coverageCodegraph`. The
committed script does emit those keys, but the **retrieval timings are
measured once per task, not `reps` times**.

In `scripts/benchmark-ai-workspace-context.mjs` at HEAD (lines 303-322)
the retrieval loop is `for (const task of TASKS)` — 3 tasks — pushing one
baseline and one codegraph sample per task. `REPS` (line 49, default 5)
is applied only to the cold (line 262) and warm (line 296) loops. So the
committed script yields **3** retrieval samples while
`docs/m7/issue-37-benchmark.json:13` asserts `"repetitions": 5`, and
`docs/m7/issue-37-benchmark.md:36` heads the results table "5 repetitions
per timing case" and `:42-43` reports retrieval medians/ranges.

Contrast the uncommitted working-tree version, whose own header
(`scripts/benchmark-ai-workspace-context.mjs:20-27`) explicitly says
"five repetitions of **EVERY** timing case (**including retrieval**)" and
which adds `samples:` counters to the retrieval aggregates (`:513-514`).
That correction exists only in the uncommitted tree.

Consequence: AC6's published medians/ranges are not reproducible from
the committed script, and the doc overstates the retrieval sample count.
Reproducing `benchmark.md`'s claim requires the uncommitted code.

### 3. BLOCKER — large uncommitted rewrite + stash left in the worktree

`git status --porcelain` → `MM scripts/benchmark-ai-workspace-context.mjs`
(231 insertions, 154 deletions staged *and* further unstaged changes),
plus `git stash list` shows a retained
`stash@{0}: WIP on ichinya/m7-issue-37: cf5b8154`.

This is exactly the code that fixes finding 2 (REPS on retrieval, the
deleted-file phase, complete-printed-document leak probes, the
`private-out-directory` redaction at `:502`). It is also the code that
matches `docs/m7/issue-37-benchmark.md`'s claims. So the branch's
committed state and its own evidence disagree, and the reconciling work
is neither committed nor documented. The stash additionally risks losing
work silently. Must be either committed (with evidence regenerated) or
discarded deliberately.

### 4. MAJOR — two prior reviews are committed without disposition

`docs/m7/issue-37-review-devin.md` and `docs/m7/issue-37-review-codex.md`
are in the diff. The devin review's verdict line is preserved
(`issue-37-review-devin.md:21-22`, and `:33-35` records that binary-tier
phases D–H were skipped). Round-1 fixes landed in `ca0106c9` and
`0abbec6d` (commit message: "review round 1", explicitly crediting
"devin M4", "devin M1 / codex F1"). But neither document records which
findings are now closed, which remain open, or why. A reader cannot tell
whether the open items behind `issue-37-implementation.md:161-168`'s
verifiability split were ever adjudicated. Recommend a disposition table.

### 5. MAJOR — AC1/AC3 acceptance is documentation-only by construction

AC1 (recommended group/setup with role aliases) and AC3 (shared schemas
reachable by an agent in a consumer repo) rest on
`docs/integrations/ai-workspace.md:78-131` plus binary-tier gate phases.
Those phases are correctly skipped in CI
(`scripts/test-ai-workspace-hook.mjs:10-14`), and per
`issue-37-implementation.md:153-154` they were "executed … against the
pinned upstream binary". Given finding 1, that execution claim is not
independently reproducible from the tree, so AC1/AC3 currently rest on
the prose plus an unreproducible gate. The research itself is honest
that upstream has no role-alias registry
(`issue-37-research.md:17`, `issue-37-implementation.md:126-127`), so
the alias convention is legitimately a documented convention — but the
*verification* leg is weaker than the checklist implies.

### 6. MAJOR — benchmark privacy probes cover only the aggregate member at HEAD

At HEAD, `scripts/benchmark-ai-workspace-context.mjs:421-424` probes
`JSON.stringify(aggregate)` only. The printed document additionally
includes `rawRowsPrivate: OUT_DIR ? OUT_DIR : "(discarded)"`
(`:435`), which echoes a **host absolute path** whenever `--out` is
passed. The probe set is checked against the aggregate, not the printed
document, so this path is not covered — the documented reproduction
command (`issue-37-benchmark.md:8-11`) uses `--reps 5` without `--out`,
which masks it. The sentinel probe (`:424-425`) has the same scope
limitation. The uncommitted rewrite fixes exactly this (`:500-505`,
probing the whole `printed` object and replacing the path with the
literal `"private-out-directory"`). Given AC8 and the branch's own
"no absolute paths" claim at `docs/integrations/ai-workspace.md:54-57`,
this is a real defect in the committed state, not merely stylistic.

### 7. MINOR — the uncommitted benchmark script contradicts the frozen evidence doc

`docs/m7/issue-37-benchmark.json` is versioned
`lekalo/ai-workspace-context-benchmark/v0.6.3` and lists exactly four
limitations (`:58-63`). The uncommitted script adds a fifth,
`"process-spawn-dominated"`, plus new aggregate members
(`returnedContextBytes`, `tokenEstimate`, `unresolvedRefCount`,
`deletedFileHandled`, `expectedSymbolHits`). Adopting it requires
regenerating both evidence files under a reviewed version bump, not a
silent script edit.

### 8. MINOR — diagnostics-registry convention not applicable, worth stating explicitly

I searched for `LEK-*` codes across all 3953 files: zero hits, so this
branch introduces none, which is correct — the hook is a Node script with
no Rust runtime path (`issue-37-implementation.md:19-20`; zero diff under
`crates/`, `Cargo.toml`). Per `docs/diagnostics.md:6-9` the registry is a
`lekalo/diagnostic/v0.2.16` wire item for `DomainResult` emissions; an
opt-in external tool correctly stays out. Recorded as a verified
negative rather than a gap.

### 9. MINOR — no in-repo gate for the closed hook state vocabulary

The hook's documented closed state set is
`planned|delivered|unknown-delivery|refused|disabled|unavailable`
(`issue-37-implementation.md:65-66`). The code additionally emits
`no-change` (`:712`) and records `reconciled` (`:842`) and `sending`
(`:884`) outbox states. Those are defensible (outbox sub-states and an
honest no-op), but the documented vocabulary is incomplete and no gate
asserts the set is closed, so drift is invisible. Recommend listing
`no-change` in the docs and adding a vocabulary assertion.

## Verified-clean checks (no findings)

- **Contract versioning.** `node scripts/check-contract-versions.mjs --base
  origin/ichinya/M7` → `{"ok":true,"product":"0.6.3","contractArtifacts":96}`.
  `contracts/ai-workspace-event.schema.v0.6.3.json` matches the product
  version; the change rule is enforced, so the contract is correctly
  versioned, not unversioned.
- **Contracts gate.** `test-ai-workspace-contracts.mjs` passes under
  pinned Ajv 8.17.1. It independently re-derives the event key
  (`:100-103`), proves key-permutation determinism (`:113-117`), proves
  closure by mutation (`:121-126`), enforces the 64-artifact/64-role
  bounds (`:128-140`), enforces the role grammar against path-like
  aliases (`:144-146`), and leak-probes every member (`:151-154`).
  This is real verification, not decoration.
- **No weakened existing gates.** The only CI change is one added line
  (`ci.yml:80`); no existing step was removed or relaxed.
- **Determinism.** No `Date.now`, `new Date`, `toISOString`,
  `randomUUID`, or `Math.random` in the hook, benchmark, or contracts
  gate. Envelope members are pinned revisions, digests, and neutral
  aliases. `git ls-files --eol` confirms `i/lf w/lf` with
  `attr/text eol=lf` on all new JSON/JS — LF-only holds.
- **Existing repo gates still green.** `check-structure`,
  `check-authority`, `check-privacy`, `test-fixture-provenance` all exit 0.
- **Opt-in is real.** Two independent layers: the send path refuses when
  this process *inherited* either widening flag as `1`
  (`ai-workspace-hook.mjs:770-772`, refuse `widening-flags-enabled`)
  **and** every child is spawned with both forced to `0` (`:513-514`
  send, `:540-541` readback). Production sends are refused while the
  authority matrix admits no change-event kind (`:748-752`); the
  override is an explicit operator flag. No auto-enable path found.
- **No canonical-model duplication.** Share allowlist in
  `tests/fixtures/ai-workspace/routing-manifest.json:6-11` is four
  public contract/doc paths, no Model/IR; envelope artifacts are typed
  contract identities only.
- **Privacy probes are genuine.** The contracts gate walks every member
  against a 12-entry leak set; the hook gate's Phase C uses a hostile
  fake upstream whose stderr carries `C:\Users\alice\topsecret\…` and
  asserts non-re-entry (`test-ai-workspace-hook.mjs:234-238`). The
  fixture family is declared synthetic in
  `tests/fixtures/fixture-provenance.json`.
- **Upstream gaps honestly documented.** Eight gaps at
  `issue-37-implementation.md:124-146` match the research inventory, and
  both prior reviews are committed rather than suppressed.

## Acceptance-criteria checklist

| # | Criterion | Verdict | Basis |
|---|---|---|---|
| AC1 | Recommended workspace group/setup with role aliases | **Partial** | Documented thoroughly (`ai-workspace.md:78-131`); verification leg unreproducible (findings 1, 5) |
| AC2 | Protocol change → explainable affected-project event | **Partial** | Typed ordered explanation chains in schema (`schema:261-297`) and hook; exercised only by the broken gate (finding 1) |
| AC3 | Shared schemas reachable by agent from consumer repo | **Partial** | Binary-tier phase, skipped in CI by design; not reproducible (finding 5) |
| AC4 | Lekalo fully functional without AI Workspace | **Verified** | Zero Rust diff; hook has `disabled`/`unavailable` paths; no code path shells out to upstream |
| AC5 | MCP scopes / sensitive-path policy not widened silently | **Verified** | Forced off in every child (`:513-514`, `:540-541`) plus inherited-hostile refusal (`:770-772`); gate inventory covers it |
| AC6 | CodeGraph context benchmark run on core changes | **Fails as committed** | Evidence exists but the committed script under-samples retrieval (finding 2); reconciling code uncommitted (finding 3) |
| AC7 | No canonical-model duplication in workspace notes | **Documented boundary** | Allowlist excludes model/IR; residual honestly scoped to the authority owner (`implementation.md:158, 166-168`) — acceptable, not faked |
| AC8 | Public events/evidence never reveal private identity | **Partial** | Envelope and hook disciplined; benchmark stdout path leak at HEAD (finding 6) |

## Required before merge

1. Fix and commit `scripts/test-ai-workspace-hook.mjs` Phase B's base/head
   fixture (finding 1); correct the "all phases green" claims at
   `issue-37-implementation.md:80-81, 101-102`.
2. Commit or deliberately discard the benchmark rewrite **and** the
   retained stash (finding 3); if committed, regenerate
   `issue-37-benchmark.{md,json}` under a reviewed version bump
   (findings 2, 7).
3. Fix or accept-and-document the `rawRowsPrivate` absolute-path echo in
   the committed benchmark (finding 6).
4. Add a disposition section to the two committed prior reviews
   (finding 4).
5. Re-run both gates and paste real output; consider wiring the
   dependency-free hook gate into CI so this class of break cannot recur
   silently.

## Note on orchestration

My Dispatch `ctx_d0865cae76af` was fenced mid-review (`consumer_fenced`:
the Attempt was re-attached to another worker), so I stopped all
`orca orchestration` mutations and therefore did **not** send
`worker_done` and did **not** push. This document is committed locally on
`ichinya/m7-issue-37` for whoever owns the Dispatch to push.