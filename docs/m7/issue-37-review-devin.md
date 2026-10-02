# Issue #37 — independent implementation review (devin)

**Verdict: ISSUES** — 12 findings: 0 blocker, 2 major, 10 minor. The
implementation is substantively sound and mergeable in intent; the two
majors are a policy-reference digest domain mismatch between the hook
and the committed example, and an unprobed absolute path on the
benchmark's public stdout channel. Neither blocks the design; both
should be fixed before the emitted artifacts are relied on.

Reviewed: branch `ichinya/m7-issue-37`, 9 commits over diff base
`origin/ichinya/M7` = `9510dd0767a56c0ab34b8d3c8ceb2a14db8de825`.
Scope per diff: 14 files, 3331 insertions, zero deletions, zero Rust
changes — integration guide, closed event-envelope contract v0.6.3,
opt-in hook, two gates, benchmark script + evidence, fixture family,
one CI line, fixture-provenance declaration.

## Method

- Read `docs/m7/issue-37-research.md` and
  `docs/m7/issue-37-implementation.md` first (authority and claims),
  then the full diff hunk-by-hunk (`git diff origin/ichinya/M7...HEAD`).
- Re-ran the dependency-free gates on this machine:
  - `node scripts/check-contract-versions.mjs --base origin/ichinya/M7`
    → `{"ok":true,"contractArtifacts":96}` — new contract carries the
    product version `0.6.3` per the product-version rule.
  - `node scripts/test-fixture-provenance.mjs` → ok, 64 families.
  - `node scripts/check-structure.mjs`, `check-authority.mjs`,
    `check-privacy.mjs`, `check-model.mjs`, `test-contract-versions.mjs`
    → all pass; accepted matrix still admits no workspace change-event
    kind (consistent with the documented default refusal).
  - `node scripts/test-ai-workspace-contracts.mjs` (pinned Ajv 8.17.1
    provisioned outside the checkout, CI-style) → pass.
  - `node scripts/test-ai-workspace-hook.mjs` (no `AI_WORKSPACE_BIN`) →
    dependency-free phases A–C pass; binary phases skip with an
    explicit recorded reason. Binary-tier claims (phases D–H) are taken
    as reported — the pinned upstream binary was not rebuilt here.
- Exercised the hook live: `--base HEAD` → `disabled`; `--base 4a084aab~1
  --head 4a084aab --manifest <fixture>` → `planned` with
  `docs/target-protocol.md:modified` and affected roles
  `{brownfield,greenfield}-consumer` (matches gate expectations);
  byte-identical output on repeat (deterministic); `--base
  origin/ichinya/M7 --head HEAD` → `planned` with empty artifacts
  (see F6); unadmitted `--send` → `refused/policy-not-admitted`,
  exit 3, outbox recorded.
- Verified pin claims: `3d7cfcfb` changes exactly the three
  `run_history` files the benchmark names and its first parent is
  `1d1f14a7`; `4a084aab~1..4a084aab` touches only
  `docs/target-protocol.md` among approved paths.

## Acceptance-criteria checklist

| Criterion | Verdict | Evidence / caveats |
| --- | --- | --- |
| AC1 documented recommended group/setup with role aliases | Met | `docs/integrations/ai-workspace.md` §setup: `lekalo-dev` group, six role aliases, config-before-init, dedicated `AI_WORKSPACE_DB`; gate phase D exercises registration (reported executed). |
| AC2 protocol change → explainable affected-project event | Met | Hook builds closed envelope with typed origin chains; gate phase F delivers a real `service_changed` for `4a084aab` with readback-verified targets (reported); live plan-mode output confirmed here. |
| AC3 shared schemas reachable by consumer agent | Met | Gate phase E: group-scoped `workspace_read` returns exact committed bytes; single-project scope denied; wrong-group coverage via stranger project. |
| AC4 lekalo fully functional without AI Workspace | Met | Zero `crates/` diff (grep for `ai.?workspace`/`AI_WORKSPACE` in `crates/`: no matches); `disabled`/`unavailable` states verified live; nothing in Rust shells out or reads workspace state. |
| AC5 MCP scopes / sensitive-path policy never silently widened | Met | Both widening flags forced `=0` in every child env (send, readback, all gate MCP calls); hook *refuses* when it inherits `=1` (phase H); `project_tree`/`project_grep`/`project_file_write` confinement legs; no `include_hidden`/`include_sensitive` overrides anywhere. |
| AC6 CodeGraph context benchmark on core changes | Met | `scripts/benchmark-ai-workspace-context.mjs` + committed `issue-37-benchmark.{md,json}`: pinned inputs, scratch copies, 5 reps, cold/warm/changed/revoked phases, honest limitations, no preclaimed speedup. |
| AC7 no canonical-model duplication in workspace notes | Met (documented residual) | Approved paths exclude Model/IR; notes-pointer rule documented; the residual (audit of arbitrary future notes) is honestly scoped to authority/privacy owners rather than faked. |
| AC8 public events/evidence never reveal private identity | Met with one exception | Closed schema, member-level leak probes, hostile-stderr quarantine, sentinel coverage, real digests verified. Exception: F2 — `rawRowsPrivate` prints an absolute path on stdout when `--out` is passed, outside the probed `aggregate`. |

Opt-in is real: no config → `disabled`; no `--send` → `planned` (writes
nothing); send without `--allow-unadmitted-send` →
`refused/policy-not-admitted` while the accepted authority matrix admits
no such kind — verified live, matching the honest upstream-gap list
rather than a faked capability. Upstream gaps are documented plainly
(non-transactional non-idempotent create, direct-links-only impact,
CodeGraph staleness) with mechanism-level detail.

## Findings

### Major

1. **`privacyPolicyRef.digest` uses the wrong digest domain — emitted
   envelopes can never match the repo's accepted policy reference.**
   `scripts/ai-workspace-hook.mjs:251-258` digests the *raw bytes* of
   `contracts/privacy-policy.v0.3.2.json` (`sha256:1fb90479…`, the repo's
   `policyFileDigest` domain). But the repo convention for
   `policyRef.digest` is the canonical policy identity —
   `sha256:5a80966f…`: embedded in the policy itself
   (`contracts/privacy-policy.v0.3.2.json:1144-1147`, projection
   `complete-policy-with-policyRef.digest-omitted`), enforced by
   `check-privacy.mjs:41,196-198,286-287` (`custody.policy-identity-mismatch`),
   and carried by `privacy-policy.v0.3.2.manifest.json`. The committed
   example correctly uses `5a80966f…`
   (`tests/fixtures/ai-workspace/event-envelope.example.json:124-127`),
   so the example and the hook disagree on the same field — and any
   verifier binding the envelope to the *accepted* policy reference
   fails. (`authorityRef` coincidentally matches because the repo
   defines that ref over raw bytes.) Fix: emit the canonical policy
   identity digest, or declare a distinct digest domain explicitly and
   fix the example to match the hook. Schema text ("sha256 over its
   declared domain") declares no domain for these refs.

2. **Benchmark stdout prints an absolute path on the public channel
   when `--out` is used, outside the leak probes.**
   `scripts/benchmark-ai-workspace-context.mjs:435` prints
   `rawRowsPrivate: OUT_DIR` where `OUT_DIR` is a resolved absolute
   path; the leak probes at `:421-425` cover `JSON.stringify(aggregate)`
   only, not this sibling field. This contradicts
   `docs/m7/issue-37-benchmark.md:24-26` ("stdout prints only the closed
   public aggregate … no absolute paths") and the guide's channel rule
   (`docs/integrations/ai-workspace.md:55`). Narrow trigger (opt-in
   `--out`), but the stated invariant is unconditional; fix by printing
   a marker/basename or extending the probe to the full printed result.

### Minor

3. **Explanation detail typo emits "modifiedd"/"addedd"/"deletedd".**
   `scripts/ai-workspace-hook.mjs:231` interpolates
   `${entry.change}d` over enum values already past tense
   (`added|modified|deleted`); live output verified:
   `"detail": "docs/target-protocol.md modifiedd"`. Public-facing text
   in every `public-artifact-changed` step. (Also confirms the fixture
   example's `"modified"` strings are not hook output — see F4.)

4. **Committed envelope example diverges from actual hook output.**
   `tests/fixtures/ai-workspace/event-envelope.example.json` is
   schema-valid and declared synthetic, but: its base/head
   (`1d1f14a7→3d7cfcfb`) is a real commit pair over which *no* approved
   path changed (those commits touch only `run_history`; the hook emits
   `artifacts:[]` for it — verified live); `aifhub-extension` is shown
   affected via `docs/target-protocol.md` although its route watches
   `["schema"]` only (`routing-manifest.json:29-36`, and
   `docs/target-protocol.md` is protocol-family per
   `ai-workspace-hook.mjs:213-214`); role order, step counts, and detail
   strings differ from hook output. Fine as a synthetic schema example,
   but bound to real revisions it reads as captured output it could
   never be. Recommend regenerating it from an actual hook `planned`
   run over a synthetic fixture diff.

5. **Hook gate is not wired into CI, despite its own header.**
   `scripts/test-ai-workspace-hook.mjs:10-14` states "CI runs the
   dependency-free phases only," but `.github/workflows/ci.yml` invokes
   only `test-ai-workspace-contracts.mjs` (line 80). Phases A–C are
   dependency-free and would run in the existing Contracts job (Ajv is
   already provisioned there) — missed regression coverage plus an
   inaccurate comment.

6. **A diff with zero approved-path changes still produces a sendable
   event.** Verified live: `--base origin/ichinya/M7 --head HEAD` →
   `planned`, `eventKind:"schema-change"`, `artifacts:[]`. With
   `--send --allow-unadmitted-send` this creates a real upstream
   `service_changed` event titled "Lekalo target contract change" that
   upstream will fan out to all linked consumers for a no-op diff.
   Research T3 lists the "unrelated-file" case; it is untested and
   unhandled. Add an empty-artifact refusal or explicit `no-change`
   closed state.

7. **Availability probe treats a nonzero `--version` exit as
   available.** `scripts/ai-workspace-hook.mjs:558-564` maps only spawn
   `error`/`throw` to `unavailable`; a binary that runs but exits
   nonzero proceeds to send (fails later as `unknown-delivery`).
   Harmless end state, but contradicts the comment at `:556-557`
   ("a binary that cannot even be probed cannot be sent through").

8. **`--send` without `--manifest` silently defaults to the committed
   test fixture.** `scripts/ai-workspace-hook.mjs:153` resolves a
   missing `--manifest` to `tests/fixtures/ai-workspace/routing-manifest.json`.
   The fixture is the documented template ("a deployment copies this"),
   so a real deployment should be required to pass an explicit reviewed
   manifest path — silently routing sends through fixture routes is a
   foot-gun. Related validation gaps: `routes` is unbounded (schema caps
   `manifest.routes` at 64 — `contracts/ai-workspace-event.schema.v0.6.3.json:81-86`),
   `watches` values are unchecked (an unknown family silently matches
   nothing — `:215-218`), and an unknown `route.origin` silently
   coerces to `declared-service-link` (`:236`).

9. **Boundary table names tests that do not exist.**
   `docs/integrations/ai-workspace.md:363-366` claims B2 is proven by a
   "hook gate digest-mismatch test" (the gate verifies byte *equality*
   of a shared read; there is no tampered-bytes negative leg) and B1 by
   a "deletion test" (no workspace-DB deletion leg exists in phases
   A–H). The underlying properties are covered differently (zero Rust
   diff; exact-byte read), but the named proofs are overstated.

10. **`.ai-workspace.local.json` is not ignored in this repository.**
    The guide mandates the operator-local config be untracked/ignored
    in every participating checkout (`docs/integrations/ai-workspace.md:115-119,135-147`),
    but `.gitignore` (6 entries) lacks it — the lekalo checkout itself
    would show it as untracked and `git add -A` would stage
    installation-local state. One-line fix: `/.ai-workspace.local.json`.

11. **`unknown-delivery` results omit the closed reason code.**
    `scripts/ai-workspace-hook.mjs:581,592,600,625` emit
    `{state:"unknown-delivery", eventKey}` with no `reason`, while
    `refused`/`unavailable` emit `reason` and the outbox records the
    closed code. Print the (already-closed) reason for diagnosability.

12. **Nits.** Dead `privateRoot` variable
    (`scripts/benchmark-ai-workspace-context.mjs:223`); stray
    single-space indentation at
    `contracts/ai-workspace-event.schema.v0.6.3.json:87`.

## Non-findings (verified sound)

- Contract versioning: new schema is closed at every level
  (`additionalProperties:false`), product-versioned `0.6.3` per the
  documented rule, `check-contract-versions` covers it (96 artifacts).
- Event key: sha256 over recursive lexicographic canonical form bound
  to the schema version; independently re-derived by the contracts gate
  and proven deterministic under key permutation; hook↔gate
  canonicalization implementations agree (verified: identical planned
  output across runs).
- Outbox discipline: key recorded before invocation, verified-delivery
  repeats are no-ops, refusal/unavailable recorded — all asserted by
  phase B and re-verified live.
- No shell invocation for upstream/git (argv arrays; `.cmd`/`.bat`
  Windows shell opt-in only); upstream stdout/stderr never re-enter
  results (poison probe verified).
- All committed artifacts are deterministic and identity-free: no
  timestamps, UUIDs, or host paths in `issue-37-benchmark.json`,
  fixtures, or schema; LF-only enforced via `.gitattributes` (zero CR
  in the diff); fixture family declared `synthetic` with provenance
  note.
- Exit-code/result conventions match the repo's script style
  (`{ok, state, reason}` JSON on stdout/stderr; 0/2/3/4 documented).
- No weakened gates, no scope creep, no leftover artifacts
  (`git status` clean; all generated state lives under OS temp).

## Residual review risks

- Binary-tier gate phases (D–H) and the benchmark run are taken as
  reported in `docs/m7/issue-37-implementation.md`; the pinned upstream
  binary was not rebuilt in this review. The dependency-free phases do
  fail closed and skip explicitly, so nothing is silently green.
- `cargo test` (1765 pass claim) not re-run — there are zero Rust
  changes, so risk is minimal.
