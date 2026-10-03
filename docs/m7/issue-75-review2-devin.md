# Issue #75 independent review, round 2: `lekalo context-budget` fix verification

Reviewer: Devin (independent; not the implementer). Reviewed the pushed
`ichinya/m7-issue-75` tip `eebe9bd0` against merge-base `9510dd07`
(`origin/ichinya/M7`; the branch predates the #37 merge, so this is the
merge-base diff). Verified the round-1 reports
([issue-75-review-devin.md](issue-75-review-devin.md),
[issue-75-review-codex.md](issue-75-review-codex.md),
[issue-75-review-cline.md](issue-75-review-cline.md)) against the fix
disposition report [issue-75-fix1.md](issue-75-fix1.md) and the two fix
commits `4e71b868` + `88198b4a`. Method: full read of the fix diff in
`context_budget/**`, `main.rs`, the three new published schemas, the CI
workflow, the paired fixtures, and the gate script; live runs of the
built `target/debug/lekalo.exe` (baseline comparison, mandatory-policy
pass, mapped-files, bogus recipe, malformed baseline, description-growth
delta); and re-runs of every cheap gate.

## Verdict: ISSUES

Both blockers and nearly every major are verified fixed with live
evidence, and the diff shows **no gate-weakening** (the only removals are
the tolerant baseline parser, unbounded reads, the silent-skip guard,
and `fixture_path` → `fixture_copy` swaps). However one major is only
partially delivered — the new `provenance.policy`/`provenance.baseline`
pins never reach the wire — and the fix round committed leftover
codemod/probe artifacts. Three minor dispositions are also overstated.

## New / remaining findings

### Major

**R2-1. `provenance.policy` and `provenance.baseline` are dead on the
wire — a mandatory-policy pass is still indistinguishable from an
advisory run (devin M7 / codex 16 incomplete).**
Evidence: `crates/lekalo-cli/src/main.rs:5341` serializes
`canonical` from the report **before** `report.with_pins(...)` runs at
`main.rs:5440`; `build_envelope`, `human`, and the `denied_json` path all
interpolate that stale `canonical`, so the pins are computed, assigned,
and silently dropped. Verified live: `--baseline <prior report>`
(comparison emitted, deltas present) still renders
`provenance.baseline: "unknown"`, and a **passing** mandatory `--policy`
(exit 0) still renders `provenance.policy: {"state":"unknown"}` — the
exact same bytes an advisory no-baseline no-policy run emits on those
fields. Both fields are schema-required yet can never be non-unknown on
the wire. Two compounding details: `provenance.baseline`'s schema member
is a bare state enum (`["known","unknown","withheld","unsupported"]`) —
even if ordering were fixed, no digest of the consumed baseline could be
emitted — and the value passed to `with_pins` for it is the baseline
*file path* (`baseline.clone().unwrap_or_default()`), not a digest.
The model-version/IR/graph/effect pins of M7 are real and verified; only
the two caller-supplied pins are dropped. Fix: serialize
`to_canonical_json()`/`to_markdown()` **after** `with_pins` (or carry the
pins through `plan`), and give `provenance.baseline` a digest-bearing
known shape if it is meant to be a pin.

### Minor

**R2-2. Leftover fix artifacts committed to the branch.**
`cb-gate-probe/baseline.json` (a repo-root probe output holding one
serialized report) and `scripts/.fix-absence.mjs`,
`scripts/.fix-envelope.mjs`, `scripts/.fix-envelope2.mjs` (one-off
codemods that mechanically edited `main.rs` and the comparison schema)
were committed in `88198b4a`/`4e71b868`. Nothing references them; they
are exactly the unreviewed-artifact shape this round is meant to catch.
Delete them before merge.

**R2-3. devin minor 10 disposition is false: `--all` still covers fewer
kinds than `--module`.** `mod.rs:243-252` (`Scope::All` subject filter)
is byte-identical to the pre-fix code — still
OPERATION|ENTITY|TYPE|EVENT|EFFECT — while `module_subjects`
(`mod.rs:744`) includes every non-MODULE kind (policies, scenarios,
endpoints, requirements, target-bindings). The claimed "every definition
kind `--module` does" was not implemented.

**R2-4. devin minor 4 disposition is false: `LEK-CONTEXT-004` is still
never emitted.** `diagnostic::artifact_evidence_incomplete`
(`diagnostic.rs:108`) has zero call sites. Verified live:
`--source-context mapped-files` yields `optionalSourceTokens:
{"state":"unsupported"}` with only the `context.budget-exceeded`
diagnostic — no `context.artifact-evidence-incomplete` row.

**R2-5. codex 6 sub-point persists: the effective source recipe is still
not normalized.** Verified live: a named profile declaring
`selection.sourceContext:"mapped-files"` used without the CLI flag emits
`profile.sourceContext:"mapped-files"` (digest-bound) while
`optionalSourceTokens` reports `unknown`; `--source-context mapped-files`
with generic `--budget` reports `unsupported` under a `"none"`-bound
digest. The digest-bound recipe can still mislabel what was measured.
(The rest of codex 6 — bounded identifiers, pinned selection version,
digest binding of the effective choice, bogus `--source-context` refused
with usage error — is verified fixed.)

### Nits

- **R2-6.** The breakdown reconciliation identity
  `Σ(exclusive)+Σ(shared) = minimumRequiredSemanticTokens` is asserted
  nowhere: the Rust test checks only non-emptiness and the Node gate
  reconciles the *ledger* (which always reconciled), not the breakdown.
  Verified correct live (355 = 355) but unpinned — a regression would
  pass every gate. Cline's requested independent assertion is absent.
- **R2-7.** `metrics::dependency_counts`/`metrics::semantic_counts`
  (`metrics.rs:257`, `:184`) remain dead despite "Dead helpers removed";
  `supporting_diamond_dedup_records_duplicates` still hand-supplies the
  multiplicity map, so the production `supporting_requests` collector
  (codex 10's requested exercise) is untested against a real graph. The
  collector itself is genuinely fixed in code.
- **R2-8.** Two auxiliary `std::fs::read`s of the policy path remain
  unbounded (`main.rs:5383` for regression limits, `:5428` for the
  digest pin) alongside the bounded authoritative `read_bounded` at
  `:5446`. No bypass (the bounded read gates evaluation), but
  inconsistent with the bounded-inputs claim.
- **R2-9.** `fixture_copy` writes its `.{tag}-used` marker twice
  (`context_budget.rs:434-440`) and intentionally leaks temp dirs —
  harmless, noted for cleanup.

## Verified fixed (dispositions confirmed)

Every blocker and the following majors were confirmed in code and, where
cheap, live:

| Finding | Evidence |
| --- | --- |
| **B1** clippy `ok_expect` (devin/codex-1/cline-1) | `.expect` direct at `compare.rs:330`; **ran the exact CI command** `cargo clippy --workspace --all-targets --locked -- -D warnings` → exit 0. |
| **B2** transitive drops direct-only deps (devin/codex-2/cline-2) | Root loop inserts into `transitive` (`closure.rs:105`); `indirect_only` = exact set difference; `dependency_counts_on_the_fixture_graph` exercises the real walk and asserts direct ⊆ transitive. Live: **3/9/6**, 3+6=9; both former direct-only deps appear in the breakdown. |
| **M1** breakdown bills whole modules (devin/codex-3/cline-3) | Per-dependency attribution (`mod.rs:336-388`): each required fact bills its owner once, subject/synthesized/non-dependency costs land on the canonically first dependency as `shared`. Live: 9 rows, Σ=355=required (was 7×314=2198). |
| **M2** `duplicateSupportingTokens` vacuous (devin/codex-10) | `supporting_requests` (`mod.rs:716`) counts one request per incoming edge occurrence before dedup, bounded by `MAX_COMPARISON_ROWS`; `duplicate_supporting_tokens` computes requested−unique with checked arithmetic. (See R2-7 for the remaining test-shape nit.) |
| **M3** minimum-safe floor (devin/codex-7a) | Checked ceiling `(a·b+d−1)/d` in `estimate.rs:48-54`; both vectors pinned: (101,11/10)→112, (355,2/3,+100)→337. |
| **M4** baseline not schema-validated (devin/codex-13) | `baseline::parse` (`baseline.rs`): `deny_unknown_fields` at every level, known-requires-value/state-only-forbids-value, identity+metric-version+estimator pins, ledger-sum and summary-count reconciliation. Live: `{ broken` → exit 1 `invalid`; wrong-identity/unknown-with-value refuse closed. |
| **M5** comparison discarded (devin/codex-14/cline-5) | Envelope carries `contextBudgetComparison` (`main.rs:5349`) validated by the published `context-budget-comparison.schema.v0.6.3`; live identical-rerun emits signed deltas; a description-only fixture edit produced a visible +9 required-token delta (the claimed +22 shape). |
| **M6** Node gate not in CI (devin/codex-18/cline-6) | Wired at `ci.yml:51` in the pinned Ajv matrix; the script now `fail("binary-missing")` instead of silently skipping, and asserts live canonical bytes equal the committed golden (`golden-drift`). |
| **M8** AC6 symbol-vs-own-module (devin/codex-17/cline-4) | Paired `tests/fixtures/context-budget/integration` fixture (5 modules, `integration.sync_external_objects`); real non-tautological tests both sides: `planner_reference_is_narrower_than_integration_workload` (core) and `integration_workload_is_broader_than_planner_reference` (CLI). Live: 418 tokens / 5 modules / 2 hops vs planner's 355 / 1 / 0 under identical profile digests. |
| codex 4 applicability peer expansion | `no_expand` set in `facts.rs:324,351-404`: policies/scenarios reached as incoming applicability enter F but their forward edges never expand. |
| codex 5 limits don't bound walks | `ClosureLimits::effective` (owner-capped) threaded through closure, hop, and fact walks; `bounded_cycles_and_work` proves a 1-node bound flips completeness; `MAX_REPORT_BYTES` enforced in `to_canonical_json`; `read_bounded`/`MAX_INPUT_BYTES` on caller inputs. |
| codex 6 (main body) | `is_bounded_identifier` (1..=64, safe grammar) on id/version; `selection.version` must equal the pinned recipe version else `unsupported-version`; selection bound into the profile digest; bogus `--source-context` → exit 1 (live). See R2-5 for the residual sub-point. |
| codex 7b gate ignores profile cost | `assessment`/`overBy` compare the effective `minimumSafeContextEstimate` (`mod.rs:306-315`); boundary test still exact. |
| codex 8 unconditional unrepresentable gap | Gap fires only when the bound error registry carries a binding for a counted operation, then reduces completeness (`facts.rs:527-540`); the golden now carries `["detected-effects-absent"]` only — verified live. |
| codex 9 declared effects count ops | Only operations with ≥1 admitted effect edge are counted; distinct effect facts (`facts.rs:420-444`). |
| codex 11 semantic fact as artifact | `largestRequiredSemanticFact` wire field (role `model`) split from `largestRequiredArtifact`, now honestly `unknown` — verified in the regenerated golden. |
| codex 12 impossible simulation | One deterministic greedy selection in canonical order (`mod.rs:462-491`); `requiredFits` additionally requires completeness. |
| codex 15 mandatory regression w/o baseline | `evaluate` returns `baseline-required` (`policy.rs:164`); pinned by `regression_gate_requires_the_baseline`. |
| cline 7 fabricated detectedEffects | Reads attached evidence: `Known(effects.detected().len())` when envelopes exist, `Unknown` otherwise (`mod.rs:572-581`). |
| devin minor 3 | `--source-context` validated once before both handles; bogus → usage error (live exit 1). |
| devin minor 5 | `read_bounded` + `MAX_INPUT_BYTES` on profile/policy/baseline paths. |
| devin minor 7 | `module.clone().into_iter()` — no phantom empty module (`mod.rs:543`). |
| devin minor 8 | `hop_maximum` gated on the hop walk's own `complete` (`mod.rs:664-668`). |
| devin minor 12 / codex 19 | Schema now requires `reason` on `required-semantic` and forbids it on supporting/optional; both negatives asserted in the Ajv gate. |
| devin minor 13 / codex 20 / cline 10 | `fixture_copy` isolates writers into OS temp dirs; `git status` stays clean after a full `cargo test` run — verified. |
| devin minor 14 | `docs/context-budget.md` added and accurate. |
| cline 8 / codex doc note | `issue-75-implementation.md` corrected to the no-old-data rename statement. |
| cline 9 | `permuted_inputs_same_bytes` and `bounded_cycles_and_work` implemented and passing. |
| devin minor 1 (main) | `edgeOccurrences` computed (14 in the golden), serialized, schema-required, on the baseline wire, and in comparison metric keys. (See R2-7 for the dead-helper tail.) |

## Gates re-run (this worktree, live)

- `cargo clippy --workspace --all-targets --locked -- -D warnings` — **exit 0** (the exact CI command; B1 confirmed).
- `cargo test -p lekalo-core --locked` — 1514 tests, 0 failed.
- `cargo test -p lekalo-cli --locked` — all suites pass, including `context_budget` 13/13 (integration comparison test included).
- `NODE_PATH=<ajv-8.17.1> node scripts/test-context-budget-contracts.mjs` — `{"ok":true,"registryEntries":457,"contextRules":8,"liveChecked":true}`; live canonical bytes equal the committed golden.
- `node scripts/check-contract-versions.mjs --base origin/ichinya/M7` — `{"ok":true,"product":"0.6.3","contractArtifacts":99}`.
- `node scripts/test-fixture-provenance.mjs` — `{"ok":true,"families":64,"synthetic":64}`.
- `cargo fmt --check` — clean. `git status` — clean before and after the test runs.

## Regression / gate-weakening scan

No removed assertions, relaxed schemas, or skipped tests found. The diff
removals are exclusively: the tolerant hand-rolled `parse_baseline`
(replaced by the strict decoder), unbounded `std::fs::read` sites
(replaced by `read_bounded`), the `if (lekalo)` silent-skip guard
(replaced by a hard `binary-missing` failure), and
`fixture_path()`→`fixture_copy()` swaps (writers isolated to temp). The
golden regenerated legitimately (7→9 transitive, reconciling breakdown,
false gap removed, `edgeOccurrences`/`largestRequiredSemanticFact`/
`provenance` added); the profile digest changed because the selection
version is now bound into it — correct, and old baselines are rightly
incomparable. Schema changes are additive-or-stricter (`required`
provenance, reason coupling, new required metric members).
`.lekalo/cache/cache.sqlite` in the new fixture follows the established
committed-fixture-cache convention (not gitignored; many fixture
families ship it).

## Summary for the coordinator

Round 1's two blockers are genuinely fixed and the fix round is real
work, not papered-over: the closure arithmetic, breakdown attribution,
typed baseline decoder, comparison payload + published schema, CI
wiring, provenance block, paired integration fixture, bounded walks, and
the honest detected/artifact split all verify in code and live. Remaining
before merge: R2-1 (re-serialize after `with_pins` — the two new
provenance fields are dead on the wire), R2-2 (delete the four committed
codemod/probe artifacts), and the three falsely-claimed minors (R2-3,
R2-4, R2-5) which are small, well-scoped follow-ups.
