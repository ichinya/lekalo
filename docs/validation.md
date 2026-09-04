# Semantic validation

Issue #12 adds pure semantic validation over the accepted typed IR. The
validator consumes a compiled [`Compilation`](ir.md) plus one closed
[validation profile](#profiles) and produces a normalized diagnostic set
through the accepted [diagnostic contract](diagnostics.md). It never reads
source files, never writes, never launches adapters, and never mutates the
project; structural, loader, source-map, semantic-ID, and versioning
failures pass through from their owning issues. The rule and profile
decisions recorded here are normative for issue #12 and are archived in
[ADR-0011](adr/0011-semantic-validation.md).

## Entry points

```text
lekalo validate [--project DIR] [--module MODULE] [--strict]
```

Library entry points mirror the CLI: `validator::validate` and
`validator::validate_scoped` consume `&Compilation` plus an embedded
`ValidationProfile` and return `Result<ValidationReport, DiagnosticSet>`.
`Ok` reports carry only warning/info diagnostics; `Err` is the normalized
error set. Exit mapping follows the accepted 0/1/3/4/5 envelope: any
surviving error is `invalid` (exit 1, stderr), a valid outcome is exit 0
(stdout); severity and category never compute the exit.

## Rules and reference-kind matrix

Rule ids use the `semantic.*` family with immutable `LEK-SEM-NNN` codes;
validator invocation rules use `validate.*` with `LEK-VAL-NNN` codes. The
kind expectations mirror the published model contract exactly, so every
model-valid project validates and no model-invalid project passes.

| Reference site | Expected kind(s) | Unresolved rule | Kind rule |
| --- | --- | --- | --- |
| field/input/payload/return type leaves | `enum`, `entity`, `scalar`, `value-object` | `semantic.type-ref-unresolved` | `semantic.type-ref-kind-mismatch` |
| `command.effects` | `effect` | `semantic.command-effect-unresolved` | `semantic.command-effect-kind-mismatch` |
| `query.reads` | `entity` | `semantic.query-read-unresolved` | `semantic.query-read-kind-mismatch` |
| `policy.applies_to` | `command` | `semantic.policy-operation-unresolved` | `semantic.policy-operation-kind-mismatch` |
| `effect.entity` | `entity` | `semantic.effect-resource-unresolved` | `semantic.effect-resource-kind-mismatch` |
| `effect.emits` | `event` | `semantic.effect-emits-unresolved` | `semantic.effect-emits-kind-mismatch` |
| `endpoint.invokes` | `command`, `query` | `semantic.endpoint-operation-unresolved` | `semantic.endpoint-operation-kind-mismatch` |
| `scenario.covers` | existence only | `semantic.scenario-operation-unresolved` | — (model layer has no kind rule) |

Whole-IR semantic rules:

- `semantic.entity-identity-field-missing` — an entity identity names a
  field the entity does not declare.
- `semantic.type-recursion` — a cycle among entity/value-object types
  through any type leaf, exactly the published `model.type-recursion`
  prohibition.
- `semantic.visibility-boundary-violation` (security) — a reference crosses
  a module boundary into a `visibility: module` symbol.
- `semantic.public-output-private-type` (security) — a project-visible query
  returns a module-private named type.
- `semantic.portable-target-reference` (compatibility, warning) — a
  `portable` definition directly references a `target-specific` definition.

Deliberately absent rules:

- **Query write effects.** The accepted `QueryDef` carries `reads` and
  `returns` only; the closed IR cannot represent a query write effect. The
  invariant is enforced by the type system itself and pinned by unit tests;
  a future Model successor that adds a write surface must revisit this
  decision.
- **Event references.** `effect.emits` is the only event reference site, so
  no distinct event rules exist.
- **Target bindings.** `target-binding.target` names a target config with
  no in-Model registry; the model layer owns `model.target-unresolved` and
  symbol-level binding semantics belong to the adapters issue (#27).
- **Duplicate declarations.** The loader owns `loader.duplicate-definition`
  and `loader.conflicting-declaration`; the closed IR types make duplicate
  identities unrepresentable through the accepted producer.

## Phases and determinism

Execution is phase-aware — resolution, then semantic, then portability —
and inside a phase follows the registry's sorted rule-id order; definitions
are visited in canonical (semantic-ID byte) order and reference occurrences
in the fixed site order (reference fields first, then type leaves, each in
source order). The #11 normalization deduplicates exactly and fixes the
total wire order, so one input always yields the same diagnostic multiset
in the same order regardless of filesystem state, locale, timezone, thread
scheduling, or working directory. Collection stops at the accepted
per-result bound of 256 diagnostics.

Spans come only from the accepted #8 source map, looked up by the
subject definition's source-map base plus the exact recorded pointer
suffix (`/effects/0`, `/fields/1/type/optional`, …). A source-derived
diagnostic always carries its span; a lookup miss emits the
`validate.span-unavailable` infrastructure invariant instead of silently
degrading. All echoed tokens are bounded through the closed token
invariant (cleaned, at most 256 bytes).

## Profiles

A validation profile is a closed, independently versioned contract
(`dev.lekalo.validation-profile@1.0.0`,
[validation-profile.schema.v1.0.0.json](../contracts/validation-profile.schema.v1.0.0.json))
that lists rule selections — `enabled` plus an optional
`severity_override` — for exactly the `semantic.*` and `validate.*`
inventory, sorted by rule id, duplicates rejected. Every profile pins
`diagnostic_registry_version`; an unknown rule id, an unsupported profile
or registry version, an unsorted or duplicated entry, an extra field, or a
malformed document all fail closed. The two built-ins are embedded from
`contracts/` and parsed by the same closed parser:

- `default` — every owned rule enabled at its registry severity, with the
  single recorded downgrade `semantic.portable-target-reference` at info.
- `strict` — the same enabled set with no overrides, keeping the warning.

Severity overrides are legal only when the registry default severity is
not `error` and the override is strictly lower: P0 blockers are never
downgradable, and nothing may upgrade. `--strict` selects the strict
built-in; the profile files have no runtime config custody in #12, and
`--profile <path>` selection remains unassigned.

## Module scoping

`--module MODULE` always validates the whole project plus its dependency
closure. The report keeps diagnostics owned by the selected module at any
severity plus every error anywhere, so mandatory cross-module failures are
never hidden; overall status still becomes invalid if any error survives.
An unknown scope fails closed before any rule runs with
`validate.module-unresolved`.

## Fixtures

The fixture matrix lives under `tests/fixtures/validation/`: one clean
base project, one invalid project per source-reachable rule (each differing
from the base by exactly one defect, asserted to fire exactly one rule),
the warning fixture for the profile-difference behavior, golden envelope
and human bytes, and the adversarial cases (unknown module scope,
double-run byte determinism, no-write guarantee) pinned by
`crates/lekalo-cli/tests/validate_semantic.rs` and the core suite. The
contract gate is `scripts/test-validation-contracts.mjs` with exact Ajv
8.17.1.
