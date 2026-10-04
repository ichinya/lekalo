# Issue #77 — implementation review round 1 (devin)

Verdict: **ISSUES** — one minor finding; the feature itself verifies
cleanly. Fix the help text before merge.

Scope: `a56ee578..5d48bea7` on `ichinya/m7-issue-77` — 186 files,
+20.6k/-273, new `lekalo_core::coupling` module, `lekalo coupling`
command, five contracts at product/contract generation 0.6.4, planner/
taskhub/provider fixture families, a 36-check gate and CI wiring.
Verified live on Node 24.13.0, Windows, Ajv 8.17.1.

## Finding (minor — must fix)

**R1-1. `coupling --help` describes a different command.**
`crates/lekalo-cli/src/main.rs:257-266`: the doc comment above
`Commands::Coupling` is the `ContextBudget` variant's text verbatim —
it advertises "context-budget and local-understandability metrics",
"the explainable over-budget breakdown", and demands an explicit
`--budget`/`--budget-profile` flag that does not exist on `coupling`.
Live `lekalo --help` and `lekalo coupling --help` both print it.

Collateral: `Commands::ContextBudget` immediately below now has no doc
comment, so `lekalo context-budget --help` displays the args-struct
comment ("The context-budget flag family (issue #75): the selector
triple…") as its description — the real #75 description was displaced,
not duplicated. Fix by giving `Coupling` its own #77 summary and
restoring the #75 paragraph to `ContextBudget`. `docs/coupling.md` and
the `docs/cli.md` entry are already correct — only the enum comment is
wrong.

## Verified claims

- **Gate: 36/36 checks pass** (`test-coupling-contracts.mjs`,
  Ajv 8.17.1), including the hard probes: `formatting-invariant`
  (AC: formatting does not move metrics), `strict-baseline-regression`
  (allowance 0 → exit 3 retaining report+comparison),
  `unknown-required-denies`, `denial-retains-report`,
  `internal-bridge`, `reverse-closure-oracle`,
  `duplicate-keys-fail`, `unsupported-version`,
  `binary-missing-fails-closed`, `closed-schemas`.
- **Live report is real evidence**, not golden theatre:
  `coupling --symbol planner.focus_task` on the planner fixture →
  `status: valid`, 3 subjects, `fanIn: [endpoint:planner.api_focus]`,
  `fanOut: [effect:planner.create_task, type:planner.task_id]`,
  `publicContracts` separated from internal/supporting, real
  `importEdgeKeys`, metric states explicit (unknown where unsupported).
- **Cargo**: `lekalo-core --test coupling` 12/12; CLI coverage is via
  the contract gate (no dedicated cli test target, consistent with the
  repo's gate-first convention for read-only commands).
- **Version discipline**: product `0.6.4`,
  `check-contract-versions --base a56ee578` → 109 artifacts ok;
  five new closed schemas.
- **Hygiene**: `cargo fmt --check` clean, `git diff --check` clean,
  provenance/diagnostic-count style preserved.
- **Advisory-by-default honored**: `Report.suggestions[].applied` is
  always false; centrality is a fact with explicit reviewed-centrality
  annotation only — no auto-applied recommendations.
- **Design separation**: pure `coupling` core module, adapter-free;
  `--changed-input` reuses `impact::ChangedInputSet` (no second Git
  parser); `--context-budget` is a planning input, not the #75 metric.

## Out of scope (documented honestly by the implementer)

The report bounds field-level reverse evidence as `unknown` rather
than inventing counts, and target amplification stays unsupported
without producer identity — both conservative-and-honest choices
consistent with the issue's "exact affected paths/reasons" bar.
