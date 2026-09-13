# Foreign and custom implementation escape hatches (issue #30)

One independent, closed, versioned attachment —
`lekalo/implementation/v1.0.0`, identity
`dev.lekalo.implementation@1.0.0` — lets complex or target-specific
logic stay ordinary code. A hook contract binds one compiled operation
symbol (command or query) to per-target implementations over the
closed kinds `generated`, `custom` (checked/custom files), `foreign`
(a foreign symbol reference such as
`@example/core-domain#calculateSchedule`,
`App\\Schedule\\CalculateSchedule::__invoke`, or `schedule.Calculate`),
`external` (an external service/port), and `unsupported` (explicitly
unsupported for that target). Absence of an entry stays the default
generated path.

The contract version is independent of the product release, of the
Model/IR/protocol contract versions, of the target-profile family, and
of the diagnostic registry. Owner decisions are recorded in
[ADR-0033](adr/0033-foreign-implementation-escape-hatch.md).

## The DSL boundary

The escape hatch replaces **implementation only**:

- the Model owns what an operation means — its input, output, error,
  and effect contracts are never restated in the attachment (the
  closed schema has no such fields, and unknown members reject);
- the attachment owns where an implementation lives, per target;
- the adapter owns whether the binding exists and matches a signature,
  over the published process protocol (#27). The core never launches
  or reads target code, and a binding can never carry a command,
  setup script, or URL: the symbol grammar refuses every reserved
  scheme spelling (`exec:`, `file:`, `https:`, ...) and traversal dots,
  and no field accepts free text.

A hook contract may acknowledge declared effect references
(`effects`); any acknowledged reference outside the operation's
declared surface rejects with `implementation.effect-mismatch`, so a
custom hook can never covertly extend declared effects.

## Multiple implementations and explicit selection

Hook contracts are unique by `(symbol, contract)`. Two contracts may
bind the same operation on the same target only when exactly one
competing binding carries `selected: true`; otherwise the document
rejects with `implementation.selection-ambiguous`. A selection with no
competitor rejects with `implementation.selection-unknown`. Selection
resolves deterministically at decode time, before any consumer runs.

## Portability: missing implementations are visible

The projection answers, per hook contract, one row per target in the
sorted union of declared targets and the project's bound targets (the
`target-binding` definitions of the compiled IR):

- the row carries the declared kind (`generated`, `custom`, `foreign`,
  `external`, `unsupported`); or
- `missing` when a project target has no entry — this emits the
  `implementation.target-missing` warning.

A declared `unsupported` target is a recorded decision, never a gap.
Target-specific code never becomes portable by declaration; the
projection reports kinds and nothing claims portability. Each entry
also carries the sorted scenario ids covering the operation: one
scenario suite applies to every implementation of an operation,
because scenarios cover operations (#23), never implementations.

## Custom file ownership

The attachment declares the mode; the artifact-ownership manifest
(#21) records the files. A custom file is a manifest entry with
lifecycle `custom` and policy `manual-only`:

- drift is reported (`manual-drift`) without blocking the check;
- the generator never overwrites it, and it can never enter a clean
  plan;
- only manifest-unclaimed orphans under `.lekalo/generated/` are clean
  candidates, through the preview-and-confirm flow of #21.

The integration tests in `crates/lekalo-cli/tests/generate.rs` pin both
behaviors through the real CLI.

## Diagnostics

The `implementation.*` family (registry 1.18.0, LEK-IMPL-001..007):

| Rule | Code | Severity | Fires when |
| --- | --- | --- | --- |
| `implementation.document-invalid` | LEK-IMPL-001 | error | the attachment violates the closed contract (shape, identity, pins, bounds, duplicates, kind coherence) |
| `implementation.effect-mismatch` | LEK-IMPL-002 | error | an acknowledged effect is outside the declared surface |
| `implementation.reference-unresolved` | LEK-IMPL-003 | error | the operation symbol is not a command or query in the bound IR |
| `implementation.selection-ambiguous` | LEK-IMPL-004 | error | competing bindings lack exactly one explicit selection |
| `implementation.selection-unknown` | LEK-IMPL-005 | error | a selection has no competitor |
| `implementation.symbol-invalid` | LEK-IMPL-006 | error | a foreign symbol spelling is refused (length, charset, scheme, traversal) |
| `implementation.target-missing` | LEK-IMPL-007 | warning | a project target has no entry for an escape-hatched operation |

## Validation seams and gates

- Library: `implementation::ImplementationDocument::from_value` (wire),
  `implementation::validate(&document, &compilation)` (pins,
  references, effects, warnings), `implementation::portability` (the
  deterministic projection), `canonical_bytes`/`digest`.
- Contract gate: `node scripts/test-implementation-contracts.mjs`
  (exact Ajv 8.17.1, Node 18/24 in CI) over
  `tests/fixtures/implementation/{valid,invalid}`.
- Semantic matrix: `tests/fixtures/implementation/semantic/` driven by
  `crates/lekalo-core/tests/implementation.rs` against the compiled
  full-kinds fixture IR.
- Ownership: the custom-lifecycle tests in
  `crates/lekalo-cli/tests/generate.rs`.

Boundaries: no runtime execution, no source parsing, no paths, no
adapter calls, no generation. Scenario execution stays with #23's
owner harnesses; binding existence and signature checking stays with
the target adapter; file custody stays with #21.
