# Coupling and change radius (issue #77)

`lekalo coupling` reports declared architecture facts and advisory findings. It
does not execute providers or targets, change source, update a baseline, or
persist a report. Capture stdout explicitly when a reviewed report is needed.

```sh
lekalo --json coupling --symbol planner.task --project tests/fixtures/coupling/planner
lekalo --json coupling --symbol planner.task --field state --project tests/fixtures/coupling/planner
lekalo --json coupling --module planner --project tests/fixtures/coupling/planner
lekalo --json coupling --all --project tests/fixtures/coupling/planner
lekalo --json coupling --changed-input changes.json --project PROJECT
lekalo --json coupling --symbol planner.task --context-budget 5000 --project PROJECT
```

Exactly one selection is required. `--field` requires a symbol with that declared
field. Changed inputs use the closed `coupling-change-input` contract, then the
existing typed `impact::ChangedInputSet`; the command does not parse Git changes.
Module/all selections include symbol and field rows plus union summaries.

## Metrics and evidence

Semantic IDs are the units. Fan-in/out counts unique admitted neighbors; repeated
field occurrences retain distinct witness edges but do not inflate a neighbor
count. Module fan counts foreign modules. Module/project summaries use boundary
neighbors and set unions rather than sums of overlapping symbol closures.

| Metric | Meaning and evidence |
| --- | --- |
| `fanInSymbols`, `fanOutSymbols`, `fanInModules`, `fanOutModules` | Unique canonical semantic neighbors; import edges are recorded separately. |
| `publicContractsAffected`, `internalSymbolsAffected`, `semanticSymbolsAffected`, `modulesAffected` | Reverse closure traverses internal bridges before classifying visibility. Root inclusion is explicit. Effects, policies and scenarios are supporting symbols. |
| `crossModuleCycles` | Symbol SCC and module SCC series are separately named; each carries actual admitted edge keys. |
| `sharedMutableResources` | Declared resources with two or more distinct writer operations, with exact effect references. A model reference alone does not imply a write. |
| `affectedArtifacts`, `affectedTests`, `requiredChecks` | Existing owners validate attachment bytes, digests and current input pins. Exact tests/checks additionally require full confirmed trace and explicit current revision. |
| `affectedTargets` | Unsupported: available artifact producer identities do not prove a generated-target set. |
| `transactionSpread` | Number of affected atomic groups spanning more than one explicitly configured domain group. Each group's modules/classification is retained. Independent transactions are not pooled. Missing domain classification is unknown. |
| `sharedAbstractionRadius` | Public reverse consumers of a type, excluding the root. This fact needs a configured threshold before it becomes a finding. |
| `publicTargetExposure` | Transitive forward exposure of a public contract to declared target-specific symbols, with exact paths. |
| `duplicationDivergence` | Divergent typed field/presence/identity shapes under explicit reviewed replica obligations. Equal replicated shapes are harmless; no obligation means unsupported. |

`known`, `unknown`, `withheld`, and `unsupported` are closed states. An unavailable
state carries no value. Field rows retain exact field dependencies and verified
query participation; owner-wide reverse consumers remain `possible`, with unknown
radius rather than an invented exact count or lower bound. Observed architecture
is explicitly unsupported in this release and is never mixed into declared facts.

Each report includes a digest-bound semantic projection, unique sets, ordered
occurrence-safe witness paths, owner/input pins, coverage, suggestions, and
planning facts. Baseline decoding replays the projection to verify fan/closure,
classification, cycles, declared shared writers and target exposure. Optional
attachment-derived sets retain their owner pins. Neither a baseline nor a digest
is an independent claim that a native test ran.

Suggestions identify exact witness references and ask for a reviewed consumer
projection or boundary cut. They preserve semantic identity, public compatibility,
types/nullability/presence, policy/errors, effects/transactions and invariant
ownership. Suggestions are never applied by this command.

## Profiles and reviewed baselines

`--coupling-profile ID --profiles FILE` selects one closed profile document whose
`profileId` equals ID. Start from
[`advisory.profile.json`](../tests/fixtures/coupling/golden/advisory.profile.json).
`project.limits` holds `{metric,maximum}` rows; module `thresholds` override limits
for their module and inherit unspecified metrics. There are no universal numeric
defaults. A central symbol is a fact, not a defect. Reviewed
`centralityDeclarations` can annotate fan/public/shared-abstraction findings;
they cannot waive baseline regression or incomplete required evidence.

`regressionLimits` hold `metric`, `absoluteIncrease`, and a closed optional
`relativeIncrease` rational `{numerator,denominator}`. `--baseline FILE` reads an
immutable prior report. Comparison requires compatible scope, recipe,
classification and evidence capabilities, and compares matching known metrics.
Threshold edits do not erase measurement comparability. Added/removed subjects
are explicit; two unavailable states never become comparable zeroes. Equality
with an allowance passes. A zero baseline has no relative ratio; its absolute
delta still applies.

Strict denial requires a deliberately selected profile with `gate.mode: strict`,
`failOn` including `baseline-regression`, and `baselineReadyRef` containing the
SHA-256 of the exact reviewed baseline file bytes. A missing/unreviewed baseline,
incompatible measurement, unavailable required regression metric, or excessive
comparable regression denies. `threshold-exceeded` and `required-incomplete` are
additional selectable strict checks. Advisory profiles remain successful.

JSON success contains `coupling` and, when requested, `couplingComparison`.
Exit 3 uses the existing denied envelope: its `payload` retains both reports and
its registered terminal diagnostic is `coupling.policy-denied`. Malformed input
exits 1; unsupported own-family versions exit 5. All findings use the registry
family `coupling.*`, codes `LEK-COUPLING-001..015`.

## Context planning and limits

`planning` carries public review contracts, internal context, declared mutation
resources, transaction groups, required checks and evidence gaps. Optional
`--context-budget` attaches the existing forward context-budget report with
required facts and capsule simulation. Its contract remains at 0.6.3. Optional
runtime conflict evidence is computed by the existing effects owner for affected
operations and explicitly reports declared coverage; it does not prove runtime
safety or acquire locks.

The core is bounded: 2,000 subjects including summaries, 50,000 traversed nodes,
250,000 visited edges per closure, 1,000,000 traversal/work units, 256-edge witness paths,
50,000 witnesses and 32 MiB input/report bytes. Oversized allocations fail closed;
truncated traversal remains unknown with explicit gaps. Strict required metrics
cannot pass on incomplete evidence.

Five own contracts use product generation 0.6.4. The diagnostic registry,
validation profiles, validation receipts and provider discovery take successors to keep exact pins
consistent. The provider's ten-operation vocabulary is unchanged. The mandatory
[`test-coupling-contracts.mjs`](../scripts/test-coupling-contracts.mjs) gate uses
the built CLI and Ajv **8.17.1** through `LEKALO_AJV_NODE_PATH`, with no extra
dependencies. It is wired after `cargo build` in `build-test`.
