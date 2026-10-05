# Issue #75 fix round 1 — finding dispositions

Fixes the findings from all three independent reviews:
[issue-75-review-devin.md](issue-75-review-devin.md) (2 blockers + 8 majors
+ 14 minors), [issue-75-review-codex.md](issue-75-review-codex.md)
(1 blocker + 17 majors + 2 minors), and
[issue-75-review-cline.md](issue-75-review-cline.md) (1 blocker + 6 majors
+ 4 minors). The three reviews overlap heavily; dispositions below are
grouped by finding, with every reviewer id listed. Implementation
commits: `fix(core,cli): closure transitive arithmetic…` (blockers +
M1–M3, codex 1–12) and `fix(contracts,scripts,docs): provenance +
comparison payloads…` (M4–M8, codex 13–19, cline 4–7), plus the minor
commit trail. Nothing was weakened: every gate, threshold, and schema
constraint is at least as strict as before this round.

## Blockers

| Finding | Disposition | Evidence |
| --- | --- | --- |
| **B1** (devin) / **1** (codex) / **1** (cline): CI clippy gate fails — `clippy::ok_expect` in `compare.rs:318`; the implementation ran clippy without `--all-targets`. | **fixed** | `.ok().expect(...)` → `.expect(...)`; the exact CI command `cargo clippy --workspace --all-targets --locked -- -D warnings` now passes (output below). |
| **B2** (devin) / **2** (codex) / **2** (cline): `transitiveDependencies` drops direct-only deps; golden showed 3/7/6 (3+6≠7). | **fixed** | Root-loop targets now insert into `transitive` too (`closure.rs`); the golden regenerates as 3/9/6 (3+6=9). New test `dependency_counts_on_the_fixture_graph` asserts direct ⊆ transitive on the real graph plus the diamond arithmetic vector; cline's "the only closure test hand-constructs a struct" no longer holds. |

## Majors — devin

| Finding | Disposition | Evidence |
| --- | --- | --- |
| **M1** Breakdown bills whole modules per row (7×314=2198 vs 355), `shared` always 0. / codex **3**, cline **3**. | **fixed** | Per-dependency attribution: each required fact bills its owning dependency (effect-edge facts bill the declaring operation); cost of subject-owned facts lands on the canonically first owner as `sharedRequiredTokens`. Golden reconciles: Σ(exclusive)+Σ(shared) = 355 = required. `tests::over_budget_symbol_yields_explainable_breakdown…` + Node gate ledger check. |
| **M2** `duplicateSupportingTokens` structurally always 0. / codex **10**. | **fixed** | `supporting_requests` now walks the closure counting one request per incoming edge occurrence (bounded), so shared supporting nodes record multiplicity > 1; `supporting_diamond_dedup_records_duplicates` covers the diamond. |
| **M3** minimum-safe floors instead of ceil. / codex **7** (first half). | **fixed** | Checked ceiling `ceil(required × margin / denominator) + framing`; the M3 vector (101, 11/10) = 112 and the codex vector (355, 2/3, +100) = 337 are pinned in tests. |
| **M4** `--baseline` input not schema-validated; fabricated zeros. / codex **13**. | **fixed** | Replaced the hand-rolled parser with `context_budget::baseline::parse`: strict typed serde (`deny_unknown_fields` at every level), exact state shapes (known-requires-value, state-only forbids value), identity/metric-version/estimator-pin checks, ledger-sum and summary-count arithmetic. Malformed baselines are `invalid` (verified: `{ broken` → exit 1; unknown-with-value → exit 1). |
| **M5** Comparison computed then discarded; no deltas in output. / codex **14**, cline **5**. | **fixed** | The envelope carries `contextBudgetComparison` (validated by the new published `context-budget-comparison.schema.v0.6.3`): signed absolute + rational relative deltas per metric. Probe: a description-only growth yields `minimumRequiredSemanticTokens 355→377, delta +22` with no contract change to frozen `semantic-diff.v0.2.16`. |
| **M6** New Node gate not wired into CI. / codex **18**, cline **6**. | **fixed** | `scripts/test-context-budget-contracts.mjs` added to the CI Ajv matrix; the gate now **requires** the built binary (no silent skip) and asserts live canonical bytes equal the committed golden. (The research's second gate `test-context-budget-cli.mjs` is covered by the Rust CLI suite `context_budget.rs`, which runs in CI's cargo test job.) |
| **M7** No input provenance in the report. / codex **16**, cline AC7 gap. | **fixed** | New `provenance` block (schema-required): `modelVersion`, `irDigest`, `graphIdentity`, `effectIdentity`, `policy` pin (digest when selected), `baseline` evidence pin. Reports bind to what they measured; a mandatory-policy pass is distinguishable from an advisory run. |
| **M8** AC6 only symbol-vs-own-module. / codex **17**, cline **4**. | **fixed** | Paired `tests/fixtures/context-budget/integration` fixture (five modules; cross-module hops = 2; 5 modules in the closure) plus real comparison tests on both sides: `planner_reference_is_narrower_than_integration_workload` (core) and `integration_workload_is_broader_than_planner_reference` (CLI) — integration is strictly wider on cost, module count, and hops under identical pinned profile digests. |

## Majors — codex (beyond the overlaps above)

| Finding | Disposition | Evidence |
| --- | --- | --- |
| **4** Incoming applicability expands peer covered subjects into F. | **fixed** | Policies/scenarios reached as applicability enter F but are marked no-expand: their other covered subjects and forward edges never join the closure (`facts.rs`). |
| **5** Advertised limits don't bound computation; unbounded `fs::read`. | **fixed** | `ClosureLimits::effective` (owner-capped) threads `maxNodes`/`maxEdges`/`maxFacts` through the closure, hop, and fact walks; a profile with bounds of 1 flips completeness. `MAX_REPORT_BYTES` enforced in `to_canonical_json`; `MAX_COMPARISON_ROWS` bounds the request walk; caller inputs use `read_bounded` (`MAX_INPUT_BYTES`, oversize → invalid). |
| **6** Profile admission/recipe mismatch; unbounded version strings; bogus `--source-context` accepted. | **fixed** | Profile ids/versions validated against a bounded grammar; unknown `selection.version` refuses `unsupported-version`; selection is bound into the effective digest; `--source-context` validates once for both budget handles (bogus → usage error). |
| **7** (second half) Assessment ignores the profile cost; gate passes its own overhead. | **fixed** | Assessment and `overBy` compare the effective `minimumSafeContextEstimate` against the available budget, so framing/margin cannot slip past the gate. |
| **8** `error-contracts-unrepresentable` unconditional; partial totals look exact. | **fixed** | The gap now fires only when the bound error registry actually carries a binding for a selected operation — and then reduces `complete` (minimum-safe stays unknown, per research). The fixture no longer reports the gap (`gaps: ["detected-effects-absent"]` only). |
| **9** Declared effects count operations, not effect facts. | **fixed** | Only operations with ≥1 admitted effect edge are counted; distinct effect facts. `notify.purge_cache_cmd` (no effects) no longer inflates the count. |
| **11** Semantic fact labeled as largest artifact. / cline **11**. | **fixed** | The semantic maximum moved to its own wire field `largestRequiredSemanticFact` (role pinned `model`); `largestRequiredArtifact` is honestly `unknown` without artifact evidence. |
| **12** Simulation reports an impossible included required set. | **fixed** | One deterministic greedy selection in canonical order; `includedRequiredFacts` and `missingRequiredIds` derive from it; `requiredFits` additionally requires completeness. |
| **15** Mandatory regression gate succeeds without a baseline. | **fixed** | `evaluate` returns `baseline-required` when the arm is selected with no baseline; pinned in `regression_gate_requires_the_baseline`. |

## Majors — cline (beyond the overlaps above)

| Finding | Disposition | Evidence |
| --- | --- | --- |
| **7** Fabricated `detectedEffects: 0` when any envelope exists. | **fixed** | The metric now reads the attached evidence: `Known(effects.detected().len())` when envelopes exist, `Unknown` otherwise. |

## Minors

| Finding | Disposition | Evidence |
| --- | --- | --- |
| devin **1** `edgeOccurrences` declared but never emitted; dead `metrics::dependency_counts`/`semantic_counts`. | **fixed** | `edgeOccurrences` is computed (distinct admitted edges over the closure), serialized, schema-required, on the baseline wire, and in the comparison metric keys. Dead helpers removed. |
| devin **2** `ErrorContractsUnrepresentable` pushed unconditionally. | **fixed** | See codex 8 — applicability-gated, reduces completeness. |
| devin **3** bogus `--source-context` under `--budget` accepted. | **fixed** | Validated once before both budget handles. |
| devin **4** `LEK-CONTEXT-004` never emitted. | **fixed** | `artifact_evidence_incomplete` warning emitted for `--source-context mapped-files` (capability honestly unsupported this generation). |
| devin **5** unbounded profile/policy/baseline reads. | **fixed** | `read_bounded` with `MAX_INPUT_BYTES` (16 MiB); oversize → `invalid`. (Absolute-path denial remains the selection policy's job, unchanged.) |
| devin **6** regression gate without baseline passes; no policy pin recorded. | **fixed** | `baseline-required` denial + `provenance.policy` digest pin. |
| devin **7** phantom empty module in `requiredModules`. | **fixed** | Subject modules derive from the actual module (no `unwrap_or_default()` phantom) plus owning modules of required facts. |
| devin **8** `maxCrossModuleHops` gated on the overall complete flag. | **fixed** | Gated on the hop walk's own `complete`. |
| devin **9** declaredEffects semantics. | **fixed** | See codex 9. |
| devin **10** `--all` excludes kinds `--module` includes. | **fixed** | `--all` now selects every definition kind `--module` does (operations, entities, types, events, effects, policies, scenarios, endpoints, target-bindings). |
| devin **11** simulation semantics. | **fixed** | See codex 12. |
| devin **12** / codex **19** schema reason/class coupling. | **fixed** | `required-semantic` requires a reason; `supporting-semantic`/`optional-source` forbid one; both negatives asserted in Ajv and added as gate parity vectors. |
| devin **13** / codex **20** / cline **10** test residue writers. | **fixed** | Writer tests copy the fixture into an isolated temp dir (`fixture_copy`) and never touch the tracked tree; `git status` stays clean after a run. The named untracked residue file is deleted and cannot regenerate into the checkout. |
| devin **14** no user docs. | **fixed** | `docs/context-budget.md` added (command, flags, exits, report reading, profile/policy formats, boundaries). |
| codex **13** (doc note) implementation doc wrongly claimed frozen 0.4.0 files remain. | **fixed** | Corrected to the no-old-data rename statement in `issue-75-implementation.md`. |
| cline **9** research-named determinism/bound tests absent. | **fixed** | `permuted_inputs_same_bytes` (six-run byte identity + canonical key order) and `bounded_cycles_and_work` (one-node bound flips completeness; default bounds complete) implemented. |
| cline **11** inert metrics M7/M8/M9. | **fixed (M7/M8) / honest-unknown retained (M9)** | M7 split (see codex 11), M8 real requests (see codex 10); M9 stays `unknown` because no ownership evidence adapter exists — emitting a number without evidence is exactly the fabricated-value shape the research forbids. The ownership adapter remains a research-scoped successor dependency. |

## Rebuttals (no code change)

| Finding | Disposition | Reason |
| --- | --- | --- |
| codex **18** sub-point / cline **6**: a second CLI gate `scripts/test-context-budget-cli.mjs` must exist. | **rebutted (form), delivered (substance)** | The research proposed that script before the Rust CLI suite existed; the CLI acceptance vectors now live in `crates/lekalo-cli/tests/context_budget.rs` (13 tests, run by CI's cargo test job), which is the stronger gate — real process exits, envelopes, and exit codes rather than a Node re-implementation. |
| cline **AC3 note**: research-named test names (`advisory_reports_exit_zero`, `policy_cannot_be_weakened_by_override`, `suggestions_never_write`) don't exist verbatim. | **rebutted** | The behaviours are pinned under different names: `over_budget_is_advisory_valid_exit_zero_with_explainable_breakdown`, `policy_pin_mismatch_denies` (override cannot weaken), and the temp-copy isolation (writers never touch the checkout). Names are not contract. |
| cline **M5 sub-point**: diff/QA "envelope attachment" must ship now. | **partially rebutted** | The QA-visible half shipped (comparison payload + policy thresholds + gate). The `semantic-diff` envelope binding touches the frozen `semantic-diff.v0.2.16` producer path and stays with the diff-family successor per the implementation doc's deferral section; this round adds the missing comparison evidence the review could not see. |

## Verification outputs (real runs, this worktree)

```
$ cargo fmt --check
(exit 0)

$ cargo clippy --workspace --all-targets --locked -- -D warnings
Finished `dev` profile [unoptimized + debugbuild] target(s) in …s
(exit 0; no errors)

$ cargo test -p lekalo-core
core ok: 1518 failed: 0

$ cargo test -p lekalo-cli
cli ok: 313 failed: 0

$ NODE_PATH=… node scripts/test-context-budget-contracts.mjs
{"ok":true,"ajv":"8.17.1","registryEntries":457,"contextRules":8,"liveChecked":true,
 "golden":"tests/fixtures/context-budget/golden/planner.over-budget.json"}

$ node scripts/check-contract-versions.mjs --base HEAD
{"ok":true,"product":"0.6.3","contractArtifacts":99,"base":"HEAD"}

$ node scripts/test-fixture-provenance.mjs
{"ok":true,"families":64,"synthetic":64,"evidenceBacked":0}

$ node scripts/check-structure.mjs / check-privacy.mjs / check-authority.mjs
all exit 0

$ node scripts/test-{context,validation,classification,diagnostic,run-history,
  expressions,semantic-diff}-contracts.mjs
all exit 0

$ git status --short
(clean; the residue files are deleted and the writers are isolated)
```

## Live behavior probes (after the fixes)

- Closure: planner.focus_task reports direct 3 / transitive 9 /
  indirect-only 6 (3+6=9).
- Breakdown: Σ(exclusive)+Σ(shared) = 355 = required total.
- Gaps: `["detected-effects-absent"]` only (no unrepresentable-error
  false positive; the embedded registry binds the planner operation, so
  the check is real).
- Baseline: identical rerun → comparable, no warnings, deltas emitted;
  description-only growth → delta +22 visible; malformed baseline →
  invalid; policy without baseline → denied `baseline-required`.
- AC6: integration.sync_external_objects = required 418, 5 modules,
  hops 2 vs planner.focus_task = required 355, 1 module, hops 0, under
  identical profile digests.
