# Issue #75 independent review: `lekalo context-budget`

Reviewer: Devin (independent; not the implementer). Reviewed the pushed
`ichinya/m7-issue-75` diff against base `9510dd07` (`origin/ichinya/M7`),
against [issue-75-research.md](issue-75-research.md) (`feba13dc`) and
[issue-75-implementation.md](issue-75-implementation.md). Method: full diff
read of `crates/lekalo-core/src/context_budget/**`, the CLI wiring in
`crates/lekalo-cli/src/main.rs`, the new schema/registry/gates, the fixture
family; plus live runs of `target/debug/lekalo.exe` on the planner fixture
and re-runs of the cheap gates (`check-contract-versions`,
`test-context-budget-contracts` with Ajv 8.17.1, `test-fixture-provenance`,
`test-validation-contracts`, `test-classification-contracts`,
`test-context-contracts`, `test-diagnostic-contracts`, `test-contract-versions`,
`test-run-history-contracts`, `check-structure`, `check-privacy`,
`check-authority`, `cargo fmt --check`, `cargo test` for both new suites,
`cargo clippy`).

## Verdict: ISSUES

Two blockers (one CI gate failure, one provably wrong primary metric) plus
eight majors. The architecture, contract hygiene, determinism, exit-class
and advisory/mandatory separation are otherwise sound, and the claimed
test/rustfmt gates are green — but the diff as pushed does not pass CI and
several shipped metric values are either wrong or vacuous.

## Findings

### Blockers

**B1. CI clippy gate fails on this branch.**
`cargo clippy --workspace --all-targets --locked -- -D warnings` (the exact
CI invocation in `.github/workflows/ci.yml`, `clippy` job) errors with
`clippy::ok_expect` at
`crates/lekalo-core/src/context_budget/compare.rs:318`
(`normalize_model(...).ok().expect("fixture loads")`). The implementation
report's verification ran `cargo clippy -p lekalo-core -p lekalo-cli`
*without* `--all-targets`, so test targets were never linted; under the
real gate the branch fails to compile lint-clean. Fix: `.expect(...)`
directly (drop `.ok()`).

**B2. `transitiveDependencies` undercounts: direct-only dependencies never
enter `transitive`.**
In `crates/lekalo-core/src/context_budget/closure.rs:78-90` the root loop
inserts targets into `closure.direct` but never into
`closure.transitive`; the BFS loop (lines 101-113) inserts only targets
of non-root nodes. Result: `transitive` = "reachable at depth >= 2, or
also reached indirectly", which violates the documented invariant
`direct ⊆ transitive` (metrics.rs:7, research M1: "transitive = all
distinct reachable dependency ids outside R, including direct"). The
shipped golden proves it internally inconsistent:
`tests/fixtures/context-budget/golden/planner.over-budget.json` reports
`directDependencies=3, transitiveDependencies=7, indirectOnlyDependencies=6`
— direct + indirect = 9 != 7 (live re-run reproduces the same numbers).
The correct transitive count is 9. The same wrong set drives the
per-dependency `breakdown` (mod.rs:280), so the two direct-only deps of
`planner.focus_task` (`effect:planner.create_task`,
`requirement:PLANNER-REQ-001`) never appear as breakdown rows — the AC2
"explainable dependency breakdown" omits the subject's own direct edges.
No test catches this: the research-planned `dependency_counts_chain_diamond_cycle`
vector was never implemented, and `closure::tests::direct_is_included_in_transitive`
tests a hand-constructed struct, not the walk. Fix: insert root-edge
targets into `transitive` too (then `indirect_only = transitive - direct`
keeps its current values).

### Majors

**M1. Breakdown token attribution is per-module, duplicated per row, and
does not reconcile.** mod.rs:269-301 builds `attribution` keyed by *module*
(sum of fact tokens per module), then assigns that module total as
`exclusiveRequiredTokens` of *every* dependency row in the module; the
`shared` slot is never written (`entry.0` only), so `sharedRequiredTokens`
is always 0. In the golden, all 7 rows read `exclusiveRequiredTokens=314`
(the whole planner-module fact sum); the row sum 2198 != required 355.
Per the spec, exclusive+shared attribution must reconcile to F; as shipped
the breakdown repeats module totals and does not explain per-dependency
cost. Fix: attribute fact tokens to their owning dependency (dedup shared
facts to the canonical first owner), or relabel the rows as module totals.

**M2. `duplicateSupportingTokens` is structurally always `known:0`.**
`supporting_requests` (mod.rs:563-572) increments each id exactly once
from disjoint sets (`direct` and `indirect_only`), so
`request_multiplicity` is always 1 and `requested - unique` is
identically 0 (metrics.rs:198-225). Moreover `supporting` itself only ever
collects MODULE/PROJECT direct-dep cards (facts.rs:456-495: every other
reachable kind was already taken into `required`), so the fixture reports
0 supporting facts at all. M8 is emitted as a measured zero without a
measurement — exactly the fake-zero shape the research forbids; either
implement bounded request provenance or emit `unknown`.

**M3. `minimum_safe_estimate` floors where the spec requires `ceil`.**
estimate.rs:48-50 uses truncating `checked_div` on
`required * marginNumerator / marginDenominator`; research line 319 and
the module doc both specify `ceil(...) + framingTokens`. With
margin 11/10, required=101 yields 111 instead of 112 — the M10
minimum-safe estimate is systematically underestimated for non-integral
margins (shipped profiles all use 1/1, so the bug only fires on custom
profiles).

**M4. `--baseline` input is not validated against the closed report
schema.** `parse_baseline` (crates/lekalo-cli/src/main.rs) is a
hand-rolled tolerant parser: it only checks the `schemaVersion` string,
then rebuilds `BudgetReport` with `unwrap_or` everywhere —
`{"state":"known"}` with no `value` becomes `Known(0)`, non-u64 values
become 0, unknown fields are ignored, and `identity`/`metricVersion` are
never read. The research requires "validates its closed schema and
digest". Consequence: a subtly malformed baseline yields fabricated-zero
deltas instead of an `invalid` refusal, and `compare()` cannot detect a
metric-version skew because the baseline's `metricVersion` is never
parsed (comparability only checks profile digest + estimator identity).

**M5. The comparison result is computed and then discarded — no deltas
ever leave the process.** In `run_context_budget` the `Comparison` value
feeds only the verdict -> diagnostic path; the `json` envelope
(`{"status":"valid","contextBudget":{...}}`) is built before the baseline
block and contains no `comparison` key. So `--baseline` output shows at
most a `context.baseline-regression`/`baseline-incomparable` warning
token — never which metric regressed or by how much. The research
proposed a `context-budget-comparison` contract family; none shipped, and
no deferral note covers the missing payload (the impl doc defers only the
semantic-diff envelope binding). AC5 evidence is reduced to a bare
warning row.

**M6. The new Node gate is not wired into CI.**
`scripts/test-context-budget-contracts.mjs` exists and passes locally
(liveChecked=true under a built binary), but `.github/workflows/ci.yml`
is untouched by this diff and never invokes the script — the contract
lists every gate explicitly (ci.yml:37-79). The schema<->wire
conformance, golden, and determinism checks therefore run only when a
human remembers to invoke them. Add it to the contract-gates step.

**M7. The report carries no input provenance.** The schema's report has
`identity`, `metricVersion`, `scope`, `profile`, `estimator`, `complete`,
`subjects`, `summary` — no model/IR/graph digests, no revision, no
evidence/scope-state, no `coverage` block (`limitsHit`, per-metric gap
reasons live only as subject-level vocabulary). The research's provenance
tuple ("digest binds all consumed evidence") and the AC7 pins (exact
model/report/profile/estimator, scope state for the #100 consumer) are
absent: two reports over different model revisions are indistinguishable,
and a baseline consumer cannot bind a report to what it measured.

**M8. AC6 is exercised only as symbol-vs-owning-module.** The research's
acceptance evidence called for the planner reference compared against a
*broader integration module* fixture pair
(`tests/fixtures/context-budget/integration`, designed wider). Only the
`planner` fixture exists; `planner_reference_is_narrower_than_module*`
assert `sum(module subjects) >= symbol`, which is trivially true because
the module contains the symbol. The intended narrow-vs-broad comparison
evidence is absent.

### Minors

1. `edgeOccurrences` is listed in `metrics::METRIC_KEYS` (metrics.rs:283)
   but is not in `SubjectMetrics`, never emitted by `metrics_json`, and
   absent from the schema — the spec'd M1 edge-occurrence count does not
   exist on the wire. `metrics::dependency_counts` and
   `metrics::semantic_counts` are dead code (mod.rs inlines its own
   counting).
2. `FactGap::ErrorContractsUnrepresentable` is pushed unconditionally
   (facts.rs:498) on every subject — the fixture has zero error contracts
   yet the golden reports the gap, so `gaps` misrepresents coverage.
3. `--source-context bogus` under `--budget` is silently accepted as
   `none`: the value is validated only inside the `--budget-profile`
   branch (main.rs, the `(None, Some(_))` arm); the `--budget` arm never
   checks it.
4. `LEK-CONTEXT-004` (`context.artifact-evidence-incomplete`) is
   registered but never emitted — `diagnostic::artifact_evidence_incomplete`
   is dead code, and `--source-context mapped-files` yields
   `optionalSourceTokens: unsupported` with no diagnostic row.
5. `--profiles`/`--policy`/`--baseline` are plain unbounded
   `std::fs::read`s (no size bound, absolute paths accepted) — the
   research asked for bounded project-relative reads.
6. `failOn:["baseline-regression"]` with no `--baseline` flag passes
   silently (`baseline_verdict=None` -> `evaluate` skips the arm); the
   report also records no policy pin, so a policy-passed run is
   indistinguishable from an advisory run.
7. `required_module_count` receives `""` when the subject has no module
   (`module.clone().unwrap_or_default()`, mod.rs:414) — a phantom empty
   module is counted for module-less subjects.
8. `maxCrossModuleHops` is gated on the *overall* `complete` flag
   (`hop_maximum`, mod.rs:400/514), so a fact-selection bound hit also
   hides an otherwise-complete hop metric.
9. `declaredEffects` counts *operations with edges*
   (`counted_operations.len()`, mod.rs:510), not effect facts — the
   golden shows `declaredEffects=1` beside two `operation-effect` ledger
   rows plus the declared effect contract.
10. `--all` subject selection (OPERATION|ENTITY|TYPE|EVENT|EFFECT,
    mod.rs:203-212) excludes policies/scenarios/endpoints/requirements
    that `--module` does include as subjects — inconsistent subject
    coverage between scopes.
11. `simulation.included_required_facts` counts facts that individually
    fit the budget, not a greedy selection; `missing_required_ids` lists
    all of F whenever it doesn't fit.
12. Schema looseness: `fact` does not require `reason` on
    `required-semantic` rows (the `if/then` constrains the enum but not
    presence), and supporting facts may carry a `reason` — weaker than
    the documented reason-class coupling.
13. Test residue: `write_profiles*` (crates/lekalo-cli/tests/context_budget.rs:427-451)
    writes `context-budget-profiles-*.json` into the tracked fixture dir
    and never removes them; nothing in `.gitignore` covers them. The two
    untracked files in this worktree are that residue — they must be
    **deleted, not committed**; the `78b4d881` cleanup removed the
    committed copies but not the cause, so residue regenerates on every
    test run (baseline/policy files are removed; profile files are not).
14. No user docs: `docs/cli.md` and the docs tree have no `context-budget`
    entry (only the m7 notes).

### Non-findings (verified clean)

- Determinism: two live runs are byte-identical; no timestamps, run ids,
  or host paths in the payload (also asserted in tests + Node gate).
- The four-state wrapper is exact in Rust (`deny_unknown_fields`,
  known-requires-value) and in the schema (`oneOf` coupling);
  `additionalProperties:false` throughout; count bounds are JS-safe.
- Registry successor `0.4.0 -> 0.6.3` is consistent: embedded bytes,
  `REGISTRY_VERSION`, the eight `context.*` entries with correct
  codes/statuses, validation profiles re-pinned, all affected gates
  re-pointed and passing; `check-contract-versions --base` green
  (96 artifacts).
- Exit classes verified live: advisory valid 0 (incl. over-budget),
  usage/invalid 1, denied 3 with the report embedded in the denied
  envelope, unsupported-version 5 for the unknown estimator profile.
- No new dependencies (`Cargo.lock` untouched); fmt clean; both new test
  suites pass (46 core + 12 CLI); the earlier lib-test cwd race is
  handled via the crate-local fixture copy.
- Legacy `lekalo context` untouched (`pub(crate)` widening of
  `canonical` only); scope of the diff stays within the issue.

## Acceptance-criteria checklist

| # | Criterion | Status | Evidence / gap |
| --- | --- | --- | --- |
| AC1 | Deterministic metric computation per model/profile version | PARTIAL | Byte-identical reruns verified; profile digest binds estimator+budget+limits. But B2 makes a headline metric wrong, and no model/IR/evidence provenance pins exist (M7), so "per model version" is unverifiable from the report. |
| AC2 | Over-budget symbol yields explainable dependency breakdown | PARTIAL | Breakdown + `overBy` + LEK-CONTEXT-006 verified live; but rows omit direct-only deps (B2) and per-row exclusive tokens are duplicated module totals that don't reconcile (M1). |
| AC3 | Suggestions advisory unless policy makes the gate mandatory | MET (with gaps) | Advisory default; `--policy` denies 3 with report embedded; pin mismatch denies pre-eval. Suggestions are data-only but shallow: `preservedFacts` always `[]`, fixed `advisory-only-extraction` blocker, no crossing-edge/SCC analysis (minor). |
| AC4 | Minimum semantic facts separated from optional source | MET | Required/supporting ledgers reconcile (ledger sum = required, tested); optional source is `unknown`/`unsupported` by recipe; tiny-budget simulation exposes missing required ids. |
| AC5 | Baseline regression visible in semantic diff/QA | PARTIAL | `compare.rs` engine + verdict diagnostics exist and are tested; but the comparison payload is never emitted (M5), baseline input isn't closed-schema validated (M4), and diff-envelope integration is deferred. |
| AC6 | Planner reference vs broader integration module | WEAK | Only symbol-vs-own-module; the specified broader integration fixture pair was never built (M8). |
| AC7 | Output usable by AIFHub Framework Lift eval | PARTIAL | Closed versioned schema + estimator/profile digests + registered diagnostics yes; input provenance, scope-state, harness round-trip, and history adapter absent (M7). |

Metric coverage (issue M1-M10): M1 miscounted (B2; edge occurrences
absent), M2/M3/M4/M7 measured, M5 canonical-zero only (observed domain
unmeasured — honestly the only domain available), M6 declared count
under-counts (minor 9) and detected is `unknown` or a hardcoded `0` when
envelopes exist (dead branch, mod.rs:433-437), M8 vacuous (M2), M9
`unknown` (no ownership evidence wired — consistent with state contract),
M10 computed but floor-vs-ceil deviates under non-1 margins (M3).

## Leftover-artifact resolution (task question)

`tests/fixtures/context-budget/planner/context-budget-profiles-0-2-16.json`
and `...-dev-lekalo-estimator-claude-3.json` are regenerated test residue
(`write_profiles*` writes them into the fixture dir per run). They must be
removed — not committed — per the `78b4d881` intent, and the tests should
write to a temp copy or the pattern should be gitignored (minor 13). I
deleted the two untracked files from this worktree; no tracked content
changed.
