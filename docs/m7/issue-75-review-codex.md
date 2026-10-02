# Issue #75: independent Codex review

Verdict: **ISSUES**. Findings: **1 blocker, 17 major, 2 minor**.

Reviewed on 2026-10-02 at `85a12c322c0819979b8f18c35726dc12b8fe3a15`,
branch `ichinya/m7-issue-75`, against
`origin/ichinya/M7 = 9510dd0767a56c0ab34b8d3c8ceb2a14db8de825`.
The three-dot diff contains 94 files, 8,007 insertions and 94 deletions,
including research, implementation notes and the earlier review document.
This review changes only this document; no implementation corrections were made.

Authority: the live [issue #75](https://github.com/ichinya/lekalo/issues/75)
read through `gh issue view 75 --repo ichinya/lekalo`, followed by
`docs/m7/issue-75-research.md` and `docs/m7/issue-75-implementation.md`.
The research explicitly requires the diff attachment, paired integration fixture,
evidence adapters and consumer provenance; its final paragraph says partial graph
counts do not complete the issue. The implementation's deferral section does not
override that acceptance authority.

## Numbered findings

1. **blocker — The actual CI Clippy command fails.**
   Evidence: `crates/lekalo-core/src/context_budget/compare.rs:318`,
   `.github/workflows/ci.yml:125`.
   `cargo clippy --workspace --all-targets --locked -- -D warnings` exits 1
   with `clippy::ok_expect` on `normalize_model(...).ok().expect(...)`.
   The implementation report's narrower Clippy invocation does not lint these
   test targets. Call `expect` directly and rerun the real gate. This is a local
   reproduction of the CI command, not a claim about a hosted CI run.

2. **major — The dependency closure excludes direct-only dependencies.**
   Evidence: `crates/lekalo-core/src/context_budget/closure.rs:78`,
   `crates/lekalo-core/src/context_budget/closure.rs:103`,
   `crates/lekalo-core/src/context_budget/closure.rs:193`.
   Root edges populate `direct`, but only children of those targets populate
   `transitive`. Thus `notify.purge_cache_cmd` reports direct 1/transitive 0;
   `planner.focus_task` reports direct 3/transitive 7/indirect-only 6, violating
   the specified direct-subset and set-difference arithmetic. Its two direct-only
   targets also disappear from the breakdown. The named closure test constructs
   sets by hand and never calls the traversal. Seed transitive with all admitted
   direct targets, exclude roots consistently in cycles, and test actual graphs.

3. **major — The explainable breakdown repeatedly bills whole modules.**
   Evidence: `crates/lekalo-core/src/context_budget/mod.rs:269`,
   `crates/lekalo-core/src/context_budget/mod.rs:290`,
   `crates/lekalo-core/src/context_budget/tests.rs:154`,
   `scripts/test-context-budget-contracts.mjs:138`.
   Attribution aggregates every module's fact costs and copies that aggregate
   into every dependency row in the module. The shipped focus-task fixture has
   required total 355, but seven breakdown rows each bill 314, totaling 2,198;
   synthesized effect facts with no module are omitted. `sharedRequiredTokens`
   never receives an attribution, and no witness edge/fact ownership mapping is
   carried. Tests check a nonempty breakdown and the separate ledger sum, not
   breakdown reconciliation. Attribute each fact once, represent shared consumers
   and witness paths, and independently assert the additive total.

4. **major — Incoming applicability expands unrelated peer operations into F.**
   Evidence: `crates/lekalo-core/src/context_budget/facts.rs:344`,
   `crates/lekalo-core/src/context_budget/facts.rs:365`,
   `docs/m7/issue-75-research.md:215`.
   After discovering an incoming policy or scenario, the fixed point follows all
   of its outgoing applicability/coverage edges as referenced contracts. Adding
   `notify.purge_cache_cmd` to the existing policy's `applies_to` adds that
   unrelated operation to the required ledger of `planner.focus_task`, even
   though focus-task has no dependency on it. A shared policy/scenario can pull
   sibling subjects and then their closures into the supposed minimum.
   Distinguish semantic references needed to interpret applicability from the
   policy/scenario's list of other covered subjects.

5. **major — Advertised resource limits do not bound the computation.**
   Evidence: `crates/lekalo-core/src/context_budget/mod.rs:223`,
   `crates/lekalo-core/src/context_budget/mod.rs:239`,
   `crates/lekalo-core/src/context_budget/closure.rs:84`,
   `crates/lekalo-core/src/context_budget/facts.rs:335`,
   `crates/lekalo-core/src/context_budget/version.rs:48`,
   `crates/lekalo-cli/src/main.rs:5276`.
   Only profile `maxSubjects` is applied; walks use global constants rather than
   profile `maxNodes`, `maxEdges`, and `maxFacts`. A profile setting all three to
   1 returns 13 required facts and `complete=true`. The edge limit checks the
   number of distinct transitive nodes, not examined edge occurrences.
   `MAX_REPORT_BYTES` and `MAX_COMPARISON_ROWS` are declarations/tests without
   enforcement, while profile/baseline/policy reads use unbounded `fs::read`.
   Validate bounds against owner maxima, pass effective limits into the walks,
   count work/edges, and enforce input, comparison and rendered-output limits.

6. **major — Profile admission and source selection do not match their pins.**
   Evidence: `crates/lekalo-core/src/context_budget/profile.rs:190`,
   `crates/lekalo-core/src/context_budget/profile.rs:262`,
   `crates/lekalo-core/src/context_budget/profile.rs:304`,
   `crates/lekalo-cli/src/main.rs:5273`,
   `crates/lekalo-cli/src/main.rs:5316`,
   `crates/lekalo-core/src/context_budget/mod.rs:421`.
   `selection.version` is parsed but neither validated nor retained in the
   effective profile/digest; `future-selection/99` is accepted. A named
   `mapped-files` profile without the CLI opt-in reports source tokens unknown,
   while the CLI opt-in with a generic profile reports unsupported but retains
   recipe `none` and exactly the semantic-only profile digest. Generic
   `--source-context bogus` also exits 0. Profile version length/grammar are
   unchecked: a 1,000-character version yields exit-0 output that fails its own
   schema's 64-character bound. Normalize one effective recipe, reject unknown
   selection versions and invalid identifiers, and bind every effective choice.

7. **major — Minimum-safe arithmetic rounds down and the gate ignores it.**
   Evidence: `crates/lekalo-core/src/context_budget/estimate.rs:51`,
   `crates/lekalo-core/src/context_budget/mod.rs:253`,
   `crates/lekalo-core/src/context_budget/mod.rs:446`.
   The advertised `ceil(required * numerator / denominator) + framing` uses
   integer division without ceiling. With required 355, margin 2/3 and framing
   100, the output is 336 instead of 337. Assessment and over-by use the raw
   required total rather than the profile estimate: budget 356/framing 100
   reports `within-budget` despite minimum-safe estimate 455, so an over-budget
   mandatory policy can pass. Implement checked ceiling arithmetic and apply the
   declared profile cost consistently to assessment, over-by and policy.

8. **major — Missing required evidence and partial totals can still look exact.**
   Evidence: `crates/lekalo-core/src/context_budget/facts.rs:498`,
   `crates/lekalo-core/src/context_budget/mod.rs:175`,
   `crates/lekalo-core/src/context_budget/mod.rs:405`,
   `crates/lekalo-core/src/context_budget/mod.rs:419`,
   `crates/lekalo-core/src/context_budget/mod.rs:479`,
   `docs/m7/issue-75-research.md:228`.
   The collector always adds `error-contracts-unrepresentable` without reducing
   required completeness or determining applicability. The service accepts only
   Compilation and builds declared effects; there is no typed inventory of the
   applicable error/authorization/invariant/transaction/Scenario IR attachments
   or detected evidence. A bounded selection still writes known required/closure
   totals, a known minimum-safe estimate and a known union; no separate lower
   bounds exist. The normal fixture reports `complete=true` alongside its
   unrepresentable-error gap. Establish declared coverage explicitly, mark
   unsupported applicable required facts incomplete, and expose partial costs
   as lower bounds rather than exact minima. Optional source absence alone need
   not make the semantic minimum incomplete.

9. **major — Declared effects count operations rather than effect facts.**
   Evidence: `crates/lekalo-core/src/context_budget/mod.rs:493`,
   `crates/lekalo-core/src/context_budget/mod.rs:510`,
   `crates/lekalo-core/src/context_budget/facts.rs:403`.
   Every reached operation enters `counted_operations`, including operations
   without effects. Consequently `notify.purge_cache_cmd`, whose model declares
   no effects and whose effect ledger is empty, reports `declaredEffects=1`.
   Focus-task also reports 1 despite two distinct operation-effect rows. Count
   the unique admitted declared effect identities; retain detected effects as a
   separate evidence-backed count, with known zero for a confirmed empty set.

10. **major — Duplicate supporting context is provably always zero.**
    Evidence: `crates/lekalo-core/src/context_budget/mod.rs:563`,
    `crates/lekalo-core/src/context_budget/metrics.rs:199`,
    `docs/m7/issue-75-research.md:226`.
    `supporting_requests` visits two disjoint deduplicated sets, direct and
    indirect-only, once each. Every recorded multiplicity is therefore exactly
    1; subtracting unique cost always produces zero. Parent-edge/root provenance
    has already been discarded. The unit test supplies an invented multiplicity
    map and does not test the production collector. Preserve bounded requests
    before deduplication and exercise a real shared-supporting diamond.

11. **major — A semantic fact is reported as the largest required artifact.**
    Evidence: `crates/lekalo-core/src/context_budget/mod.rs:426`,
    `crates/lekalo-core/src/context_budget/mod.rs:541`,
    `crates/lekalo-core/src/context_budget/mod.rs:445`,
    `docs/m7/issue-75-research.md:225`.
    The implementation chooses the largest LedgerFact and labels it role
    `model`, with null bytes, without measuring a file/artifact. For the empty
    effect command it reports the operation fact at 28 tokens; no required-file
    size participates. The research expressly separates largest semantic fact
    from largest file. Source/target counts and ownership ratio are hardcoded
    unknown, and mapped-source measurement is unsupported because the artifact
    evidence adapter is absent. Keep the semantic maximum in a separate field,
    and implement evidence-backed artifact/file/ownership measurements with
    honest unknown states when the particular input lacks evidence.

12. **major — Capsule simulation reports an impossible included required set.**
    Evidence: `crates/lekalo-core/src/context_budget/mod.rs:363`,
    `crates/lekalo-core/src/context_budget/mod.rs:373`,
    `crates/lekalo-core/src/context_budget/tests.rs:211`.
    Included required facts are those individually cheaper than the entire
    budget, without a cumulative selection or join to included capsule facts;
    if F exceeds budget, every required id is simultaneously called missing.
    At budget 200, the fixture reports 9 included facts, 13 included required
    facts, and all 13 required ids missing. `requiredFits` also ignores required
    completeness. Produce one deterministic selection, derive inclusion/missing
    sets from it, and keep the legacy capsule's different fact set explicit.

13. **major — Baseline input bypasses the closed contract and fabricates zeros.**
    Evidence: `crates/lekalo-cli/src/main.rs:5484`,
    `crates/lekalo-cli/src/main.rs:5494`,
    `crates/lekalo-cli/src/main.rs:5501`,
    `crates/lekalo-core/src/context_budget/compare.rs:73`.
    Only the schema discriminator is checked; fields are recovered from generic
    JSON with defaults. Unknown fields, wrong identity, wrong metric version,
    wrong estimator digest, unknown-with-value, and known-without-value all
    reproduced exit 0 with no baseline warning. The last case becomes known
    zero. Estimator version/digest and other provenance are discarded, and
    `compare` trusts the supplied profile digest without these checks.
    Decode and validate the complete closed payload, state shapes, uniqueness,
    arithmetic and pins before comparing; malformed and incompatible input must
    retain their distinct failure/incomparability meanings.

14. **major — Baseline deltas never reach the output or semantic diff.**
    Evidence: `crates/lekalo-cli/src/main.rs:5347`,
    `crates/lekalo-cli/src/main.rs:5375`,
    `crates/lekalo-cli/src/main.rs:5417`,
    `crates/lekalo-cli/src/main.rs:2599`,
    `docs/m7/issue-75-research.md:201`.
    The report JSON and Markdown are constructed before comparison, and the
    Comparison is discarded after deriving a verdict. Without policy limits a
    comparable cost increase is entirely quiet. A real description-only change
    raises required tokens 355 to 756, but `--baseline` returns no delta block,
    warning or baseline provenance. `run_diff` has no opt-in attachment; the new
    Node gate does not exercise baseline/QA. Emit a versioned comparison and bind
    it to the unchanged semantic-diff payload as the research specifies. This
    requires no modification of the frozen semantic-diff contract itself.

15. **major — A mandatory regression gate succeeds without a baseline.**
    Evidence: `crates/lekalo-core/src/context_budget/policy.rs:157`,
    `crates/lekalo-cli/src/main.rs:5419`.
    A pinned mandatory policy with `failOn:["baseline-regression"]` and zero
    regression allowances exits 0 when `--baseline` is omitted: `None` skips
    that gate. The same policy with a valid baseline and actual growth denies
    correctly. Require the needed baseline or report a denial/incomplete gate
    instead of silently removing the mandatory check.

16. **major — The evaluation wire lacks input provenance and policy evidence.**
    Evidence: `contracts/context-budget-report.schema.v0.6.3.json:8`,
    `crates/lekalo-core/src/context_budget/mod.rs:962`,
    `docs/m7/issue-75-research.md:203`,
    `docs/m7/issue-75-research.md:68`.
    Output has metric/profile/estimator pins, but no model version or Model/IR/
    graph/effect digests, attachment inventory, contracted/observed scope state,
    measurement identity, or selected policy digest/verdict. A passing mandatory
    run is indistinguishable from an advisory run. This cannot implement the
    promised reproducibility/comparability contract for Framework Lift. Only the
    report JSON Schema ships; profile, policy and comparison have Rust literals/
    types but no published closed schemas or consumer-import test. Add the
    evidence pins and externally validatable contracts, keeping measured
    structural estimates separate from provider usage and empirical success.

17. **major — AC6 compares a symbol to its own containing module.**
    Evidence: `crates/lekalo-core/src/context_budget/tests.rs:233`,
    `crates/lekalo-cli/tests/context_budget.rs:360`,
    `docs/m7/issue-75-implementation.md:116`,
    `docs/m7/issue-75-research.md:347`.
    Both purported planner-versus-integration tests compare `planner.focus_task`
    to module `planner` in the same fixture. Their assertion is the expected
    subset relationship, not comparison with a broader integration module.
    There is no paired `context-budget/integration` fixture or comparison table
    for all ten metrics. Implement and document the two distinct workloads under
    identical pins; the current test names cannot demonstrate this criterion.

18. **major — The new contract release gate is not wired into CI.**
    Evidence: `.github/workflows/ci.yml:34`,
    `.github/workflows/ci.yml:77`,
    `scripts/test-context-budget-contracts.mjs:120`.
    No workflow invokes `test-context-budget-contracts.mjs`; the exhaustive schema
    command list includes legacy context but omits this new family. Its live
    section also silently skips when a debug binary is absent, and merely parsing
    the stored golden does not check live output equality with that golden.
    Wire it into the pinned Ajv matrix, build/select the binary explicitly,
    require live execution there, and compare live canonical bytes with the
    golden. Keep the existing gates intact.

19. **minor — The report schema does not enforce reason/class coupling.**
    Evidence: `contracts/context-budget-report.schema.v0.6.3.json:159`,
    `contracts/context-budget-report.schema.v0.6.3.json:184`.
    The required-semantic conditional restricts a reason only when it is present;
    it never requires it. Supporting facts can carry a required reason as well.
    Ajv accepted both a golden with its first required reason deleted and a
    supporting fact with `reason:"subject-contract"`. Require a reason exactly
    for required facts and add these schema/runtime parity vectors.

20. **minor — Deleted profile residue will be recreated by the CLI tests.**
    Evidence: `crates/lekalo-cli/tests/context_budget.rs:45`,
    `crates/lekalo-cli/tests/context_budget.rs:431`,
    `crates/lekalo-cli/tests/context_budget.rs:449`, cleanup commit `78b4d881`.
    `fixture_path` points to the real checkout, while `write_profiles*` claims it
    writes to a temporary project, writes into that checkout and never removes
    the profiles. The specifically named `context-budget-profiles-0-2-16.json`
    is generated test residue, **must not be committed**, and should be removed
    when present. It was already absent from the working tree and Git index at
    this review's starting HEAD because the cleanup deleted the tracked copies;
    there was nothing
    for this reviewer to delete. Fix the tests to use isolated temporary copies
    and cleanup so their ordinary execution cannot regenerate the leftover.

## Acceptance-criteria checklist

| Criterion | Code and tests inspected | Assessment |
| --- | --- | --- |
| AC1: deterministic computation per model/profile version | `plan`, profile digest, canonical projection; core `same_pins_same_bytes`, `changed_profile_not_comparable`; CLI `repeated_runs_are_byte_identical`; live Node gate | **PARTIAL.** Repetition is byte-identical/LF-only for the fixture, but effective recipe/version admission, required input pins and profile bounds are incomplete (#5, #6, #16). |
| AC2: over-budget symbol has explainable dependency breakdown | `closure.rs`, attribution and `metrics::assess`; core/CLI boundary and over-budget tests | **FAIL.** Advisory warning and exact generic-budget boundary work, but closure and additive breakdown are wrong (#2, #3); named profile cost also disagrees with assessment (#7). |
| AC3: suggestions advisory unless policy makes a gate mandatory | suggestion rows; policy `pins/evaluate`; existing `mandatory_policy_denies_and_passes_on_the_pin`, `policy_pin_mismatch_denies`; external same-pin denial/pass probes | **PARTIAL.** Advisory default and explicit denied/3 work. Regression checking can be omitted (#15), named cost can escape the gate (#7), and policy success is not recorded (#16). Suggestions are shallow advisory data, with empty preserved facts; no refactoring is executed. |
| AC4: minimum semantic facts separate from optional source | fact classes and required/supporting ledgers; `minimum_required_is_separate_from_supporting`, `tiny_budget_simulation_exposes_missing_required` | **PARTIAL.** Separate fields/classes exist and source is not silently included in F. Applicability, missing required evidence and actual simulation inclusion are incorrect (#4, #8, #12); mapped source is not implemented (#11). |
| AC5: baseline regression visible in semantic diff/QA | `compare.rs`, `parse_baseline`, `run_context_budget`, `run_diff`; comparison unit tests and `baseline_comparison_records_verdicts` | **FAIL.** Comparison arithmetic and policy denial exist, but input is permissive, deltas are discarded, semantic diff is unwired and required baseline omission passes (#13–#15). The CLI test only covers identical/configuration-change/malformed-JSON cases. |
| AC6: planner reference module compared with broader integration module | the two `planner_reference_is_narrower_than_module*` tests and fixture trees | **FAIL.** These compare a symbol with its own module; no distinct integration workload is measured (#17). |
| AC7: output usable by AIFHub Framework Lift eval | report schema, canonical JSON, state decoder, registered diagnostics, local Ajv gate | **PARTIAL.** A machine-readable report exists, but primary metrics, evidence pins, policy/comparison contracts and consumer import evidence are incomplete (#6, #8–#14, #16, #19). |

Metric coverage is not equivalent to having a field with each metric's name:

| Issue metric | Implementation and test evidence | Remaining defect or limit |
| --- | --- | --- |
| Direct/transitive dependencies | `closure.rs`; hand-constructed closure test; live command probes | Direct-only targets missing (#2); required edge-occurrence series not emitted. |
| Closure tokens | F/O ledger sums in `plan`; changed-budget and separate-subtotals tests | Measured before selection as required; applicability and incomplete lower-bound semantics need correction (#4, #8). |
| Required files | source-map model count in `model_file_count` | Model count works for fixture; maintained-source/target mapping capability absent (#11). |
| Cross-module hops | `module_hop_attribution`, `hop_maximum` | Declared-graph implementation exists; no production zero/one alternate-path/cycle or paired integration vectors from the research are present. |
| Unresolved/ambiguous edges | accepted graph plus known-zero fields | Canonical accepted-graph zero is defensible; observed evidence domain/capabilities are not measured or pinned (#8, #16). |
| Effect/policy/scenario counts | collected required facts; `semantic_counts_of` | Effect count is operation count (#9); applicability broadens closure (#4); detected/attachment adapters absent (#8). |
| Largest artifact | `largest_required_fact` | Semantic fact is mislabeled as an artifact; actual file maximum absent (#11). |
| Duplicate context | multiplicity helper and arithmetic unit test | Actual requests are deduplicated before counting, making zero inevitable (#10). |
| Generated/maintained ratio | ownership structure plus hardcoded unknown | No ownership evidence adapter or meaningful ownership-vector test (#11). |
| Minimum safe size | `minimum_safe_estimate`, assessment; scaling and generic-boundary unit tests | Floor instead of ceiling, profile cost not used by gate, incompleteness still known (#7, #8). Empirical safety correctly stays unknown. |

## Verification and practical limits

Passed locally on the reviewed source:

- `cargo test -p lekalo-core context_budget --lib --locked`: 46 passed.
- `cargo test -p lekalo-cli --test context_budget --locked` with the five
  profile/policy/baseline tests skipped: 7 passed. Those skipped tests write to
  the real fixture directory; equivalent profile/policy/baseline behavior was
  probed in an external temporary fixture copy without changing this checkout.
- `cargo build -p lekalo-cli --locked` and `cargo fmt --check`.
- `node scripts/check-contract-versions.mjs --base origin/ichinya/M7`:
  product 0.6.3, 96 contract artifacts. The base-aware check covers the actual
  implementation changes; the report's `--base HEAD` command alone would not.
- `test-fixture-provenance.mjs` and `check-structure.mjs`.
- Pinned Ajv 8.17.1 gates: `test-context-budget-contracts.mjs` (liveChecked true),
  `test-classification-contracts.mjs`, `test-validation-contracts.mjs`,
  `test-context-contracts.mjs`, `test-semantic-diff-contracts.mjs`,
  `test-diagnostic-contracts.mjs`, `test-run-history-contracts.mjs`,
  `test-expressions-contracts.mjs`; also `check-privacy.mjs` and
  `check-authority.mjs`.

Failed: exact workspace/all-targets Clippy command, finding #1. No broad workspace
test pass or hosted-CI success is asserted. The existing gates were retained;
their fixture/scope coverage does not prove the missing new semantics.

Independent CLI and Ajv probes used a temporary copy outside the worktree.
They confirmed the numerical breakdown/count/simulation contradictions,
profile limits/rounding/recipe defects, malformed baseline acceptance,
description-only growth with invisible deltas, mandatory baseline omission,
same-pin policy denial with evidence under `payload.contextBudget`, profile pin
mismatch denial, and the two reason-coupling schema negatives. Repeated valid
fixture output and raw Git golden bytes are LF-only; the golden is compact
canonical JSON with sorted keys. No timestamp, UUID or host absolute path was
observed in the ordinary report. Unbounded caller profile strings still permit
self-schema violations, so that normal-fixture evidence is not universal proof.

Registry successor identities use product 0.6.3, all eight context rules are
registered, and the active registry gate checks 457 entries. The predecessor
0.4.0 registry/profile files were renamed away, consistently with current
`docs/versioning.md`'s no-old-data policy; the implementation note claiming the
frozen 0.4.0 generation remains present/accepted should be corrected. No
historical-contract retention failure is inferred from stale research wording.

The branch is not merge-ready. Correct the blocker and behavioral findings,
complete the missing acceptance integrations, then review the corrected SHA.
