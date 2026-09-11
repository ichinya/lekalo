# ADR-0033: foreign and custom implementation escape hatches

Status: accepted for issue #30. Custody: this issue carries the reserved
product candidate **0.2.8** (the per-issue 0.2.x tag order of the M2
release policy); the implementation contract version, the diagnostic
registry version, and every other contract family stay independent of
it.

## Context

Lekalo's Model and IR are deliberately not a universal language
(Model v0.1 recorded the exclusion: complex logic is "a future
foreign-implementation contract, not an untyped escape hatch"). Real
projects need schedule arithmetic, per-provider integrations, and
framework-entangled logic written as ordinary code — without forcing
Lekalo to grow expressions, loops, or target syntax, and without
pretending that generated code is the only possible implementation.
What was missing is the ownership mode itself: how existing target-local
implementations are declared, checked, and reported while the canonical
input/output/error/effect contract stays in the Model.

## Decision

1. **One closed, immutable wire contract.**
   [`contracts/implementation.schema.v1.0.0.json`](../../contracts/implementation.schema.v1.0.0.json)
   (discriminator `lekalo/implementation/v1.0.0`, identity
   `dev.lekalo.implementation@1.0.0`) binds one project identity to one
   exact Model pin and one exact canonical-IR digest. Every hook
   contract names one operation symbol (a command or query), one
   hook-interface contract name, and per-target bindings over the
   closed implementation kinds `generated`, `custom` (checked/custom
   files), `foreign` (a foreign symbol reference), `external` (an
   external service/port), and `unsupported` (explicitly unsupported
   for that target). Absence of an entry remains the default
   `generated` path; an explicit `generated` binding is a record, not
   an obligation.

2. **The escape hatch replaces implementation only.** The attachment
   carries no schemas, no types, no error vocabularies, and no source
   paths: the canonical input/output/error/effect contract is and stays
   the Model's. A hook contract may acknowledge declared effect
   references; any acknowledged reference outside the operation's
   declared surface rejects (`implementation.effect-mismatch`), so a
   custom hook can never covertly extend declared effects.

3. **Target symbols are references, never commands.** A foreign binding
   carries a bounded reference token (`@example/core-domain#
   calculateSchedule`, `App\\Schedule\\CalculateSchedule::__invoke`,
   `schedule.Calculate`) whose closed grammar admits letters, digits,
   and the separator set ` _ @ # . : ( ) / - \\` — and refuses every
   reserved scheme spelling (`exec:`, `file:`, `https:`, `node:`, ...)
   plus traversal dots. There is no field a setup script, command, or
   URL can occupy; the schema's `additionalProperties: false` and the
   Rust decoder both refuse unknown members. Binding existence and
   signature checking is the adapter's job over the published process
   protocol (#27); the core never launches or reads target code, so
   arbitrary setup scripts are unrepresentable rather than forbidden.

4. **Multiple implementations require explicit selection.** Two hook
   contracts may bind the same operation on the same target only when
   exactly one competing binding carries `selected: true`
   (`implementation.selection-ambiguous`); a selection with no
   competitor rejects (`implementation.selection-unknown`). Selection
   is resolved deterministically before any downstream consumer runs.

5. **Missing implementations are portability data.** The deterministic
   projection answers, per hook contract, one row per target in the
   union of declared and project-bound targets (the project's
   `target-binding` definitions). A project target without an entry is
   `missing` and emits the `implementation.target-missing` warning —
   visible in validation, verify, and any future portability surface,
   never silently passed. A declared `unsupported` target is a
   recorded decision, not a gap. Target-specific code never becomes
   portable by declaration: the projection reports kinds, and nothing
   in the attachment claims portability.

6. **Custom files stay user-owned.** The escape hatch declares the
   mode; the artifact-ownership manifest (#21) records the files. A
   `custom` manifest entry (lifecycle `custom`, policy `manual-only`)
   is reported on drift, never overwritten, and can never enter a
   clean plan; orphans under the managed root keep their own
   preview-and-confirm authority. Integration tests pin both
   behaviors through the real CLI.

7. **One scenario suite for every implementation.** Scenarios cover
   operations (#23), never implementations, so a scenario that covers
   an operation applies to every implementation of that operation —
   foreign, custom, external, or generated. The projection carries the
   covering scenario ids per operation as proof data; nothing in the
   wire can exempt a target implementation from the suite.

8. **Registry 1.18.0 on this base.** The `implementation.*` family
   (LEK-IMPL-001..007) joins the registry as a wire-shape-preserving
   additive minor increment with full predecessor custody; the
   validation profiles re-issue against the new registry pin exactly as
   every prior minor increment did. 1.17.0 stays reserved by its
   parallel owner and is not part of this integrated line; the family
   content is the identity that must survive, not the base-relative
   number.

## Consequences

- Complex target-specific logic stays ordinary code; Lekalo does not
  become a programming language.
- The DSL boundary is explicit: the Model owns what an operation means;
  the attachment owns where its implementation lives; the adapter owns
  whether the binding exists and matches a signature.
- Generator custody is preserved by construction: nothing in the
  escape hatch grants write authority, and #21 keeps custom files
  outside every repair path.
- Real native-adapter binding checks land with the target-protocol
  integration (issues #27/#31/#91): this issue owns the seam, the
  diagnostics, and the semantics, and the fixture matrix proves the
  full flow without claiming native-adapter coverage.
