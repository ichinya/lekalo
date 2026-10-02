# Issue #75 independent review: `lekalo context-budget`

Reviewer: Cline (independent; not the implementer). Reviewed the **pushed**
branch `ichinya/m7-issue-75` at `5808a3400d29df26d097d01087e2bc6fd86d11bd`
against base `origin/ichinya/M7 = 9510dd0767a56c0ab34b8d3c8ceb2a14db8de825`
(three-dot diff: 95 files, 8,362 insertions, 94 deletions), against
[issue-75-research.md](issue-75-research.md) (`feba13dc`, the acceptance
authority) and [issue-75-implementation.md](issue-75-implementation.md).
Two earlier independent reviews exist on the branch
([issue-75-review-codex.md](issue-75-review-codex.md) at `85a12c32`,
[issue-75-review-devin.md](issue-75-review-devin.md) at `5808a340`); I
re-derived every finding independently from the source and live runs rather
than adopting them.

Method: full read of `crates/lekalo-core/src/context_budget/**` and the CLI
wiring, the new schema/registry/gate/fixture family; an isolated
`git worktree` at `HEAD` so that HEAD could be compiled and tested without
the concurrently edited main worktree; live `target/debug/lekalo.exe` runs
against the planner fixture; and re-runs of the cheap Node gates with pinned
Ajv 8.17.1. This review changes only this document.

## Verdict: ISSUES

**1 blocker, 6 major, 4 minor.** The architecture is sound and the contract
hygiene is genuinely good: closed versioned schema, correct registry
successor, four-state metric leaves, advisory-by-default policy separation,
and byte-identical deterministic output were all confirmed. But the branch
does not pass its own CI Clippy gate, and the two headline metrics the issue
asks for are provably wrong on the shipped fixture: the dependency counts
are internally contradictory and the "explainable" breakdown over-bills by
6.2x. Two research-mandated deliverables (the paired `integration` fixture,
the CLI gate script) and the entire diff/QA half of AC5 were never built.

## Numbered findings

### Blockers

1. **blocker: the real CI Clippy gate fails on this branch.**
   Evidence: `crates/lekalo-core/src/context_budget/compare.rs:318-322`,
   `.github/workflows/ci.yml:119-127`.
   CI runs `cargo clippy --workspace --all-targets --locked -- -D warnings`.
   At `HEAD` the `planner()` test helper reads
   `normalize_model(...).ok().expect("fixture loads")`, which trips
   `clippy::ok_expect`. I verified the lint is warn-by-default and promoted to
   an error by `-D warnings` with a minimal probe crate, then reproduced the
   actual failure in a detached worktree at `HEAD`:
   `cargo clippy -p lekalo-core --all-targets --locked -- -D warnings` ends
   in `error: could not compile lekalo-core (lib test)`, verdict FAIL.
   The implementation report's gate (its Verification section) is
   `cargo clippy -p lekalo-core -p lekalo-cli -- -D warnings`, **without
   `--all-targets`**, which never lints test targets, so the documented
   verification could not have caught this. Fix: drop `.ok()` and run the
   actual CI command. This is a local reproduction of the CI command, not a
   claim about a hosted CI run.

### Major

2. **major: `transitiveDependencies` undercounts, because direct-only
   dependencies never enter `transitive`, violating the documented
   invariant.**
   Evidence: `crates/lekalo-core/src/context_budget/closure.rs:77-90`
   (root loop inserts only into `closure.direct`), `:100-113` (the BFS loop
   inserts only targets of non-root nodes), `closure.rs:24-26` (the doc
   comment claims "All distinct reachable dependency node ids outside the
   subject set, **including the direct ones**"),
   `crates/lekalo-core/src/context_budget/metrics.rs:5-9` (asserts the
   invariant that direct is a subset of transitive and indirect-only is
   transitive minus direct). The code contradicts its own documentation.
   The committed golden `tests/fixtures/context-budget/golden/planner.over-budget.json`
   bakes the contradiction in as an accepted artifact:
   `directDependencies=3`, `transitiveDependencies=7`,
   `indirectOnlyDependencies=6`, so 3 + 6 = 9 is not 7. A live re-run of the
   built binary reproduces exactly 3/7/6. The correct transitive count is 9.
   No test catches it: the only named closure test
   (`closure.rs::tests::direct_is_included_in_transitive`) hand-constructs a
   `DependencyClosure` struct and asserts on its own arithmetic without ever
   invoking `dependency_closure`. The research's named vector
   `dependency_counts_chain_diamond_cycle` was never implemented. Fix: seed
   `transitive` with all admitted direct targets, then assert the invariant
   against a real graph.

3. **major: the "explainable" breakdown bills whole modules per row and does
   not reconcile with the required total (AC2 core).**
   Evidence: `crates/lekalo-core/src/context_budget/mod.rs:269-277`
   (attribution accumulates every module's fact costs into one per-module
   bucket keyed by `fact.module`), `:280-301` (each breakdown row for a
   dependency in that module copies the module's whole aggregate as
   `exclusive_required_tokens`), `:302-306` (sorted by that duplicated
   figure), and the schema description in
   `contracts/context-budget-report.schema.v0.6.3.json:5`, which promises the
   report explains the over-budget amount.
   Measured on the shipped golden and reproduced live: the required total is
   **355** tokens, the breakdown has 7 rows that each bill **314**, summing to
   **2,198**, a 6.2x over-bill that cannot be used to decide what to reduce.
   Two required facts with no owning module (the synthesized effect facts
   `effect:create:...` and `effect:emit-event:...`) are dropped from the
   breakdown entirely by the `let Some(module_name) = module_of else
   { continue }` at `:272-274`. `sharedRequiredTokens` is emitted as `0` in
   every row, so the documented shared/exclusive split carries no
   information. No witness edge or fact-ownership mapping is recorded, so a
   reader cannot tell *why* a row costs what it costs.
   The gates miss this because they only reconcile the *ledger*
   (`scripts/test-context-budget-contracts.mjs:138-139` and `:195-196`
   compare `requiredFacts` token sums to the required metric; nothing sums the
   breakdown). Fix: attribute each required fact exactly once, model shared
   consumers and witness paths explicitly, and independently assert
   `sum(exclusive) + sum(shared) == minimumRequiredSemanticTokens`.

4. **major: the paired `integration` fixture the research mandates was never
   created, so AC6's comparison has no broader side.**
   Evidence: the research section "Planner versus integration fixture"
   requires paired new synthetic fixtures `context-budget/planner` **and**
   `context-budget/integration`, with an `integration.sync_external_objects`
   model whose supported references connect task, event, delivery,
   authorization and retry contracts across modules, run at identical budgets
   with published per-metric comparison. Only
   `tests/fixtures/context-budget/planner/` and `.../golden/` exist (the
   planner fixture also gained a `notify` module, but no
   `sync_external_objects` symbol). The two "AC6" tests are therefore
   symbol-vs-own-module within one module:
   `crates/lekalo-core/src/context_budget/tests.rs::planner_reference_is_narrower_than_module_closure`
   and `crates/lekalo-cli/tests/context_budget.rs::planner_reference_is_narrower_than_module`,
   and the CLI one reduces to `assert!(module_required >= symbol_required)`,
   a tautology that holds for any nested scope and would pass even if the two
   scopes were identical. The implementation report nonetheless marks AC6
   with named test evidence. This is a claimed-but-unverified acceptance
   criterion.

5. **major: the baseline comparison result is never emitted, so AC5 has no
   visible regression evidence.**
   Evidence: `crates/lekalo-core/src/context_budget/compare.rs:16,19-70`
   (`Comparison`, `SubjectComparison`, `MetricDelta` are `Serialize`
   derived), `crates/lekalo-cli/src/main.rs:5374-5397` (the comparison is
   computed and collapsed into a `BaselineVerdict` plus diagnostics; the
   `comparison` value itself is dropped), and
   `contracts/context-budget-report.schema.v0.6.3.json` (no `comparison` or
   `baseline` member exists anywhere in the closed schema).
   Live confirmation: running the built binary with `--baseline` against an
   identical prior report yields an envelope whose top-level keys are
   `status, contextBudget, diagnostics, reasonCodes`, and whose
   `contextBudget` keys are `complete, estimator, identity, metricVersion,
   profile, schemaVersion, scope, subjects, summary`, with no comparison
   payload and `reasonCodes: ["context.budget-exceeded"]`. A user cannot see
   the signed deltas the research requires ("emits signed delta and
   contributing changes"), so growth is invisible except as a code. The CLI
   test `baseline_comparison_records_verdicts` only asserts the *absence* of
   `context.baseline-incomparable` for the identical case and never asserts a
   positive regression or any delta value, so it cannot fail for a missing
   payload. The report's AC5 row claims this evidence; it does not support
   it. The diff/QA envelope attachment is additionally deferred to a successor
   that does not exist yet.

6. **major: the new release gate is not wired into CI.**
   Evidence: `scripts/test-context-budget-contracts.mjs` (new, 208 lines,
   described in the implementation report as the release gate) is referenced
   by **no** line in `.github/workflows/ci.yml`; searching the workflow for
   `budget` returns nothing, while the sibling gates
   `test-context-contracts.mjs` (`:50`), `test-diagnostic-contracts.mjs`
   (`:39`), `test-validation-contracts.mjs` (`:40`) and
   `test-classification-contracts.mjs` (`:74`) are all wired. The research
   also specifies a second CLI gate `scripts/test-context-budget-cli.mjs`,
   which does not exist. Consequently the only schema/provenance/determinism
   gate for the new family never runs in CI, and the fixture family is only
   covered indirectly. Noted for fairness: many optional scripts are also
   unwired in this repo, so this is a gap in the required verification path
   rather than a unique convention break; but for a family whose entire value
   is contract hygiene, shipping its gate unwired is a material gap.

7. **major: `detectedEffects` reports a fabricated `0` whenever any effect
   envelope exists.**
   Evidence: `crates/lekalo-core/src/context_budget/mod.rs:433-437`,
   `if effects.envelope_count() == 0 { Unknown } else { Known(0) }`. The
   `else` branch never consults the detected-effect set, so any project with
   at least one envelope reports exactly zero detected effects regardless of
   what was actually detected. This is precisely the "missing/detected
   evidence must not become an optimistic zero" failure the research's risk
   section forbids. The golden shows `detectedEffects` as unknown only because
   this fixture has no envelopes, so the dead branch is untested.

### Minor

8. **minor: documentation/code mismatch on the frozen `0.4.0` registry
   generation.**
   Evidence: `docs/m7/issue-75-implementation.md:34-38` claims "The frozen
   `0.4.0` generation stays byte-identical and accepted." The diff renames
   `contracts/diagnostic-registry.v0.4.0.json` to `...v0.6.3.json` (and
   likewise `validation-profile.default/strict`), and
   `contracts/diagnostic-registry.v0.4.0.json` and
   `contracts/validation-profile.default.v0.4.0.json` no longer exist. The
   successor approach itself is correct and consistent with
   `docs/versioning.md`'s no-old-data policy; only the doc statement is wrong
   and should be corrected.

9. **minor: research-mandated determinism and bound tests are absent.**
   Evidence: the research test plan names `permuted_inputs_same_bytes` and
   `bounded_cycles_and_work` for AC1; neither exists anywhere in
   `crates/lekalo-core/src/context_budget/**` or
   `crates/lekalo-cli/tests/context_budget.rs`. What ships instead is
   `same_pins_same_bytes`, `changed_profile_not_comparable`,
   `repeated_runs_are_byte_identical`. Determinism is verified in practice
   (I confirmed two consecutive runs are byte-identical, LF-only, canonical
   sorted-key JSON), but permutation invariance and bounded-cycle behavior
   are unverified.

10. **minor: the CLI test suite writes generated documents into the tracked
    fixture tree (root cause of the leftover-artifact task question).**
    Evidence: `crates/lekalo-cli/tests/context_budget.rs::fixture_path()`
    resolves `CARGO_MANIFEST_DIR/../../tests/fixtures/context-budget/planner`,
    the **committed** fixture directory rather than a temp copy, and
    `write_profiles_with_estimator` writes
    `context-budget-profiles-<stem>.json` into it, while
    `baseline_comparison_records_verdicts` writes
    `context-budget-baseline.json` there. Commit `78b4d881` ("keep generated
    profile documents out of the fixture tree") removed the generated
    documents from tracking but did **not** fix the writers, so every run
    re-dirties the tree. Fix: write to a temp copy of the fixture.

11. **minor: human-facing metric labels are looser than the contract.**
    Evidence: the golden reports `largestRequiredArtifact.artifactId` as
    `"entity:planner.task"` with `"bytes": null`, a semantic *fact* presented
    as an artifact, so M7's "largest artifact" is satisfied only nominally
    (the research itself notes a semantic fact is mislabeled as an artifact
    and the actual file maximum is absent). Likewise
    `generatedMaintainedRatio` and `duplicateSupportingTokens` are
    structurally present but permanently unknown or `0`, because no ownership
    evidence adapter is wired and supporting requests are deduplicated before
    counting, making the duplicate metric vacuous by construction. These are
    honest unknowns rather than false values, which is the right call, but
    three of the ten issue metrics are effectively inert.

## Acceptance-criteria checklist

| # | Criterion | Status | Evidence / gap |
| --- | --- | --- | --- |
| AC1 | Deterministic metric computation per model/profile version | **PARTIAL** | Determinism itself is real: two consecutive live runs byte-identical, LF-only, canonical sorted keys, no timestamp/uuid/host path; `same_pins_same_bytes`, `changed_profile_not_comparable`, `repeated_runs_are_byte_identical`, and the profile digest binds estimator + budget + limits. But finding #2 makes a headline metric wrong, `permuted_inputs_same_bytes` and `bounded_cycles_and_work` were never written (#9), and no model/IR/evidence provenance is pinned in the report, so "per model version" is not verifiable from the wire. |
| AC2 | Over-budget symbol yields explainable dependency breakdown | **NOT MET** | The mechanism exists (`LEK-CONTEXT-006`, exact `overBy = required - available`, boundary tests at exact and one-below budget), and the ledger reconciles. But the breakdown itself is unusable: 7 rows summing to 2,198 against a required 355 (#3), direct-only dependencies absent from the rows (#2), `sharedRequiredTokens` always 0, no witness paths. "Explainable" is the criterion's core word and it is not satisfied. |
| AC3 | Suggestions advisory unless profile/policy makes the gate mandatory | **MET (with gaps)** | Confirmed at the seam: default advisory, `--policy` denial exits 3 with the report mirrored in the denied envelope, profile-pin mismatch denies before evaluation, `context.policy-denied` registered with statuses `[denied]` and never `valid` (also asserted as a negative in the Node gate). Suggestions remain data-only with a fixed `advisory-only-extraction` blocker and `preservedFacts` always empty, so shallow but advisory. The research's named vectors `advisory_reports_exit_zero`, `unknown_required_evidence_denies`, `policy_cannot_be_weakened_by_override`, `suggestions_never_write` do not exist under those names, though the behaviour is covered by differently-named tests. |
| AC4 | Minimum semantic facts separated from optional source context | **MET** | Required vs supporting ledgers are separate and reconcile (`minimum_required_is_separate_from_supporting`), every required fact carries an inclusion reason and class, optional source stays unknown/unsupported absent the opt-in recipe, and the tiny-budget simulation exposes missing required ids rather than claiming a safe capsule. This is the best-evidenced criterion. |
| AC5 | Baseline regression visible in semantic diff/QA | **NOT MET** | The comparison engine is well built and unit-tested (comparable delta, configuration change, removed subject, shared-unknown, either-bound regression semantics, no fake zeros). But the computed comparison is discarded before output (#5): no schema member, no envelope field, verified live. The only CLI test asserts absence of a diagnostic. Diff-envelope integration is deferred to a non-existent successor. Regression is therefore not visible to any consumer. |
| AC6 | Planner reference module vs broader integration module comparison | **NOT MET** | Only symbol-vs-own-module within a single module; the research-mandated paired `integration` fixture was never built (#4), and the CLI assertion is a tautological `>=`. |
| AC7 | Output usable by AIFHub Framework Lift evaluation | **PARTIAL** | Genuinely strong on the contract side: closed `additionalProperties: false` schema, exact pins (metric version, estimator identity/version/digest, profile id/version/digest), four-state leaves with a value-under-state negative in both Rust and Ajv, registered diagnostic ids, registry successor consistent at 0.6.3 (457 entries checked), and privacy negatives (no `timestamp`, no `C:/`, no `target/debug`) in the gate. Gaps: no input/model provenance (AC1), no comparison payload (AC5), no history/run-observation adapter, and the schema gate that proves these properties is not run in CI (#6). |

**Issue metric coverage (M1-M10).** M1 direct/transitive deps: wrong (#2).
M2 closure tokens measured before selection: met. M3 required model files:
met from the source map. M4 cross-module hops: implemented, but always 0 on
the shipped fixture (single-module closure), so the relaxation is
unexercised. M5 unresolved/ambiguous edges: hardcoded `StateValue::Known(0)`
for both (`mod.rs:431`); defensible for the accepted graph (which refuses
unresolved endpoints) but the observed-evidence view the research asks for
does not exist. M6 effect/policy/scenario counts: policy and scenario counts
met; declared effects count operations rather than effects, and detected
effects is the fabricated `0` (#7). M7 largest artifact: nominally present
but is a semantic fact (#11). M8 duplicate context: vacuous by construction
(#11). M9 generated-vs-maintained ratio: permanently unknown, no ownership
adapter. M10 minimum safe context size: computed structurally with profile
margin/framing; empirically safe correctly stays unknown (calibration belongs
to #100, per the research).

## Task question: the untracked fixture file

`tests/fixtures/context-budget/planner/context-budget-profiles-0-2-16.json`
**must be removed, not committed.** It is generated test residue: every run
of `write_profiles_with_estimator` writes exactly that filename into the
committed fixture directory, and it is read only by its own invocation.
Commit `78b4d881` already removed it (and its sibling
`...-dev-lekalo-estimator-claude-3.json`) from tracking, and it is **not**
present in the current worktree: `git status` shows no untracked file there.
So the specific leftover named in my task is already resolved and needs no
commit. However the underlying defect is not fixed: the writers still target
the tracked fixture tree, so the file reappears on every test run (finding
#10). It should also not be blanket-gitignored as a workaround; the writer
should be redirected to a temp copy so the fixture tree stays clean and
`git status` stays meaningful after a test run.

## Verification actually performed

Passed locally against the reviewed source:

- `cargo test -p lekalo-core --lib context_budget --locked`: exit 0.
- `node scripts/check-contract-versions.mjs --base origin/ichinya/M7`:
  `{"ok":true,"product":"0.6.3","contractArtifacts":96}`.
- `node scripts/test-context-budget-contracts.mjs` with pinned Ajv 8.17.1:
  `{"ok":true,"registryEntries":457,"contextRules":8,"liveChecked":true}`.
  Note this gate is **green despite findings #2 and #3**, which is itself
  evidence that it does not validate metric arithmetic.
- `node scripts/test-fixture-provenance.mjs`:
  `{"ok":true,"families":64,"synthetic":64}`.
- `node scripts/test-diagnostic-contracts.mjs`: `{"ok":true,...}` with
  `registryEntries` 307. This gate pins its own historical expectation, so
  the 307-vs-457 split between the two gates is expected, not a defect.
- Determinism/privacy of the shipped golden and of two live runs: byte
  identical, LF-only, no trailing newline, canonical sorted keys, no
  timestamp, absolute path, or host path.

Failed:

- `cargo clippy -p lekalo-core --all-targets --locked -- -D warnings` (the
  shape of the CI command) in a detached worktree at `HEAD`: **FAIL** on
  `clippy::ok_expect` (finding #1).

Not verified: a full `cargo clippy --workspace --all-targets` cold run and a
full workspace `cargo test` were not completed within the review window (the
workspace build is long); no hosted CI run is claimed. Structural claims
about registry, provenance and schema were verified locally.

Practical limits on this review: because the main worktree was being edited
concurrently by another agent throughout (7 tracked files dirty, including a
half-applied `ClosureLimits` refactor that does not compile, plus new
untracked files), every compile/test/lint result above was deliberately taken
against `HEAD` in an isolated `git worktree` or against the previously built
binary. The concurrent in-progress edits are **not** part of the reviewed
state and were left untouched.

## Recommendation

Not merge-ready. Minimum before re-review:

1. Fix `clippy::ok_expect` and re-run the exact CI Clippy command (#1).
2. Make the closure satisfy direct-subset-transitive with `indirect_only` =
   transitive minus direct, and add the real-graph/diamond/cycle tests (#2).
3. Re-attribute the breakdown so `exclusive + shared` reconciles exactly to
   the required total, and assert that identity in both Rust and the Node
   gate (#3).
4. Build the paired `context-budget/integration` fixture and a comparison
   test that is not tautological (#4).
5. Emit the comparison (schema member plus envelope field) so AC5 regression
   is actually visible, and assert a positive regression case (#5).
6. Wire `scripts/test-context-budget-contracts.mjs` into CI and add the
   research's `test-context-budget-cli.mjs` (#6).
7. Replace the `detectedEffects` fabricated `0` with the real detected count
   or an explicit unknown (#7).
8. Redirect test fixture writers to a temp copy; correct the `0.4.0`
   documentation claim (#8, #10); add the missing permutation/bound tests
   (#9).