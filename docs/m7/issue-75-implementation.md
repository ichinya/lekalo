# Issue #75 implementation: context-budget and local-understandability metrics

Implemented on `ichinya/m7-issue-75` against the research in
[issue-75-research.md](issue-75-research.md) (commit `feba13dc`). The
research document is the spec: the surface inventory, the metric
definitions M1–M10, the diagnostic family, the profile/policy formats,
and the acceptance mapping below all follow it. Product version at
implementation is `0.6.3`; every changed or new contract takes that
version per `docs/versioning.md` and `scripts/check-contract-versions.mjs`.

## What was built

### 1. Core service: `lekalo_core::context_budget` (new module)

| File | Content |
| --- | --- |
| `version.rs` | Report contract identity `dev.lekalo.context-budget-report@0.6.3`, discriminator `lekalo/context-budget-report/v0.6.3`, metric version `context-budget-semantics/1`, hard bounds (subjects, closure nodes/edges, facts, breakdown rows, suggestions, report bytes). |
| `value.rs` | The exact four-state M6 wrapper `StateValue<T>`: only `known` carries a value; `unknown`/`withheld`/`unsupported` are state-only spellings; a value under a state-only spelling refuses in Rust and the JSON Schema; known zero round-trips as known zero. |
| `closure.rs` | `dependency_closure` (canonical BFS: direct ⊆ transitive, `indirectOnly = transitive − direct`, cycle-safe, bound hits mark `complete = false` — never a silently smaller closure) and `module_hop_attribution` (exact minimum module-boundary hops per reachable dependency by strict-improvement relaxation; non-dependency-bearing targets such as requirements end hop paths). |
| `facts.rs` | The deterministic required-fact fixed point: subject contracts, referenced contracts, incoming policy applicability, covering scenarios, and per-operation effect edges (declared + detected). Every required fact carries id, class, inclusion reason, owning module, and estimated tokens; supporting facts O are ranked separately; optional source S is never collected here. Closed gap vocabulary (`closure-bounded`, `detected-effects-absent`, `error-contracts-unrepresentable`). |
| `metrics.rs` | The M1–M10 arithmetic and the closed assessment vocabulary (`within-budget`/`over-budget`/`indeterminate`) with the exact `overBy = required − available` remainder. A reduced completeness is `indeterminate`, except when a proven lower bound already exceeds the budget (definitely over, bound recorded). |
| `estimate.rs` | The pinned chars-4 estimator reused from the accepted capsule contract plus the profile projection `ceil(required × margin / denominator) + framing` and the `available = window − reservations` arithmetic, all checked. |
| `profile.rs` | The closed budget-profile document (`lekalo/context-budget-profile/v0.6.3`): estimator identity/version/spec-digest pin, budget arithmetic, selection recipe, resource limits, unique ids, order-normalized canonical digest. The generic `--budget N` handle resolves the same estimator with an explicit caller budget. No default budget exists. |
| `policy.rs` | The closed mandatory policy document (`lekalo/context-budget-policy/v0.6.3`): `failOn` over `over-budget`/`required-incomplete`/`baseline-regression`, either-bound regression allowances (absolute and rational relative, equality passes, zero base skips the relative bound), and the exact profile pin. |
| `compare.rs` | Baseline comparison: subjects matched by exact id (added/removed are explicit incomparable rows, never zeros), signed absolute plus rational relative deltas over the closed metric vocabulary, a shared unknown is a comparable row without a delta, asymmetric availability and profile/estimator changes are incomparable with closed reasons, and the policy verdict applies the either-bound semantics. |
| `diagnostic.rs` | The `context.*` rules routed through the accepted #11 registry constructor with bounded tokens: `input-invalid` (invalid), `profile-unsupported` (unsupported-version, no fallback), the advisory warnings `closure-incomplete`/`artifact-evidence-incomplete`/`baseline-incomparable`/`budget-exceeded`/`baseline-regression` (valid), and `policy-denied` (denied). |
| `mod.rs` | `plan(request, selection, compilation)`: builds the accepted graph/effect projections, resolves the subject set per scope, computes the per-subject report (metrics, assessment, breakdown with canonical attribution, ledger, gaps, suggestions, simulation), and the project summary with the deduplicated union. The canonical compact JSON projection (byte-sorted keys) and the Markdown projection render the same typed value. |
| `request.rs` | The typed request: exactly one of symbol/module/all, the exclusive budget handles. |

### 2. Diagnostic registry successor

The active registry advances `0.4.0 → 0.6.3` with the eight registered
`context.*` rules (`LEK-CONTEXT-001..008`), wire shape frozen. The
validation-profile contract follows with its registry pin advanced and no
rule-content change (the #69 successor precedent). The predecessor files are renamed away per
docs/versioning.md's no-old-data policy; the frozen 0.4.0 bytes remain recoverable
from Git history. Positive and negative
status vectors live in the Rust diagnostic tests, the core
`context_budget::diagnostic` tests, and the Node gate.

### 3. Report contract

`contracts/context-budget-report.schema.v0.6.3.json` pins the closed
report wire: the fixed metric vocabulary, the four-state wrapper (with
the state/value shape enforced), the subject row (assessment, over-by
state, explainable breakdown, required/supporting ledgers with
reason-class coupling, closed gaps, advisory suggestions, opt-in
simulation), and the reconciling summary. `additionalProperties: false`
throughout.

### 4. CLI: `lekalo context-budget`

Exactly one of `--symbol`/`--module`/`--all`; an explicit `--budget` or
`--budget-profile` + `--profiles` is required. `--simulate-capsule`
attaches the legacy capsule at the same budget, `--suggest` emits the
advisory extraction boundaries, `--policy FILE` selects the mandatory
gate (the only exit-3 path, with the full report mirrored in the denied
envelope), and `--baseline FILE` compares against an immutable prior
report. Exit classes: advisory valid 0 (including over-budget), usage
and malformed input 1, mandatory policy denial 3, unsupported
profile/estimator/policy contract 5.

### 5. Gates and fixtures

`scripts/test-context-budget-contracts.mjs` validates the registry
family vectors, the Rust identity constants, the live CLI report against
the closed schema (including adversarial value-under-state,
unknown-field, negative-count, ledger reconciliation, determinism, and
privacy negatives), and the canonical golden. The synthetic
`tests/fixtures/context-budget/planner` fixture family (registered in
`tests/fixtures/fixture-provenance.json`) and the core crate-local
`crates/lekalo-core/tests/fixtures/context-budget/planner` copy serve
the CLI and lib suites respectively; the lib suite never mutates the
process working directory (the fixture compiles once into a
`OnceLock`).

## Decisions

- **Advisory by default.** An over-budget report is a valid exit-0
  envelope carrying the explainable breakdown, the exact over-by
  remainder, and the `LEK-CONTEXT-006` warning. Only an explicitly
  selected mandatory policy denies, and it can never be weakened by
  overrides because it pins the effective-profile digest before
  evaluating anything.
- **Closure before selection.** The full dependency closure and the
  required-fact sum are measured before any budget walk, so lowering the
  budget changes the simulation, never the measured cost (AC1, tested by
  `changed_profile_not_comparable`).
- **No universal thresholds.** There is no default budget: a request
  without an explicit budget or profile is a usage error. The chars-4
  estimator stays the only accepted identity; a named-model tokenizer
  would need its own pinned spec and calibration, and unsupported ids
  refuse closed.
- **Shared unknowns are comparable.** The baseline comparison treats
  "unknown on both sides" as agreement (a comparable row without a
  delta), never as a fake zero; only asymmetric availability produces
  `metric-unknown` incomparability.
- **Lib-test hygiene.** The in-crate planner fixture lives under the
  core crate and loads without chdir, eliminating a process-global
  working-directory race with the cache/doctor suites that otherwise
  flaked under parallel test threads.
- **Registry successor over a new family file.** The `context.*` rules
  ride the existing embedded registry as an additive successor
  (`0.4.0 → 0.6.3`), keeping every #11 invariant (sorted ids, unique
  codes, closed statuses, message-id coupling) and the existing gates
  green.

## Acceptance mapping

| Acceptance criterion | Evidence |
| --- | --- |
| AC1 deterministic computation per model/profile version | `same_pins_same_bytes`, `changed_profile_not_comparable`, `repeated_runs_are_byte_identical` (byte-identical reruns, no timestamps/paths); profile digest binds estimator + budget + limits; Node gate determinism probe. |
| AC2 over-budget symbols have an explainable dependency breakdown | `over_budget_symbol_yields_explainable_breakdown_and_exact_boundary` + CLI `over_budget_is_advisory_valid_exit_zero_with_explainable_breakdown`: exact boundary passes, one token below reports `overByTokens = 1`, breakdown rows and ledger reconcile to the required total, `LEK-CONTEXT-006` warning present. |
| AC3 suggestions advisory unless policy makes the gate mandatory | `mandatory_policy_denies_and_passes_on_the_pin`, `policy_pin_mismatch_denies`; suggestions are data-only rows (`blocker: advisory-only-extraction`), no command execution, no file writes; policy denial preserves the full report in the denied envelope. |
| AC4 minimum required semantic facts separated from optional source | `minimum_required_is_separate_from_supporting` (closure = required + supporting, `optionalSourceTokens` unknown without the opt-in recipe, every required fact carries a reason); `tiny_budget_simulation_exposes_missing_required` (F never fits ⇒ `requiredFits=false` with explicit missing ids and no safe-capsule claim). |
| AC5 baseline regression visible in semantic diff/QA | `compare.rs` tests (comparable delta, configuration change, removed subject, unknown metric, regression verdict) + CLI `baseline_comparison_records_verdicts` (comparable quiet, changed-profile incomparable, malformed baseline invalid). Exit-status/JSON QA integration via `scripts/test-context-budget-contracts.mjs`; the semantic-diff envelope attachment is left to the diff-family successor as scoped in the research (§ baseline/diff) — this issue ships the comparison engine and its QA gate evidence without touching the frozen `semantic-diff.v0.2.16` bytes. |
| AC6 planner reference compared against broader integration module | `planner_reference_is_narrower_than_module_closure` (core) and `planner_reference_is_narrower_than_module` (CLI): the module union covers its reference symbol, the union dedups shared facts, and per-subject rows are compared, not just totals. |
| AC7 output usable by AIFHub Framework Lift evaluation | Closed versioned report schema with exact pins (metric version, estimator identity/version/digest, profile id/version/digest, scope state), four-state metric leaves, machine-readable diagnostics with registered ids, canonical deterministic bytes, and the Node gate round-trip; no source bytes, paths, or timestamps ever enter the wire. |

Issue-metric coverage: M1 dependency counts (`closure.rs` + counts
test), M2 closure tokens measured before budget (`changed_profile_not_comparable`),
M3 required model files (`modelFiles` from the source map), M4
cross-module hops (`module_hop_attribution`), M5 unresolved/ambiguous
edges (accepted-graph zero + explicit states; observed-evidence joins
stay unknown rather than zero), M6 effect/policy/scenario counts
(declared effects counted, detected unknown when no envelope contributed),
M7 largest required artifact, M8 duplicate supporting requests,
M9 ownership ratio (unknown without ownership evidence, never a
name heuristic), M10 minimum-safe estimate (structural, margin/framing
from the profile; `empiricallySafeContextTokens` stays unknown —
calibration belongs to #100).

## Deliberately deferred (per the research staging)

- The `run-observation`/`run-record` successor carrying a closed
  `contextBudget` summary (needs its own versioned contract; the
  unchanged `run-observation.v0.4.0` still receives actual capsule
  metadata under operation `context`).
- The semantic-diff CLI assessment envelope binding (the frozen
  `semantic-diff.v0.2.16` payload stays unchanged; the comparison engine
  and its policy thresholds ship here).
- Opt-in source measurement beyond `unknown`/`unsupported` states
  (`--source-context mapped-files` currently records the recipe choice;
  whole-file byte accounting needs the filesystem evidence adapter and
  its bounded-read policy).
- Exact named-model tokenizers and empirical calibration (#100).

## Verification

```
cargo fmt --check
cargo clippy -p lekalo-core -p lekalo-cli -- -D warnings
cargo test -p lekalo-core -p lekalo-cli
node scripts/test-context-budget-contracts.mjs      # needs Ajv 8.17.1 via NODE_PATH
node scripts/check-contract-versions.mjs --base HEAD
node scripts/test-fixture-provenance.mjs
node scripts/test-context-contracts.mjs
node scripts/test-validation-contracts.mjs
node scripts/test-classification-contracts.mjs
node scripts/test-diagnostic-contracts.mjs
node scripts/test-run-history-contracts.mjs
node scripts/test-expressions-contracts.mjs
node scripts/check-structure.mjs
node scripts/check-privacy.mjs
node scripts/check-authority.mjs
```

All green at the implementation commit; 96 contract artifacts.
