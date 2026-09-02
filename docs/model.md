# Lekalo Model v0.1

Status: issue #5 is accepted and published as product 0.1.2 at exact commit `b2ace5e893ffd62c80250099792d2e33a0aff3a7`, with immutable annotated tag `v0.1.2`; issue #3 remains only the prospective 0.1.3 candidate until fresh independent PASS, publication, remote verification and closure evidence. Product versions remain independent from Model, schema, semantic-ID, IR and other contract versions.

The model is language-neutral and uses the same semantic vocabulary for
TypeScript, PHP and Go targets. It contains no target-language class, package,
framework or runtime concepts.

The normative artifacts are:

- [`contracts/model.schema.v0.1.0.json`](../contracts/model.schema.v0.1.0.json) —
  JSON Schema Draft 2020-12 with `$id
  https://lekalo.dev/schemas/model/0.1.0/schema.json` and
  `schema_version: 0.1.0`;
- [`scripts/check-model.mjs`](../scripts/check-model.mjs) — dependency-free
  reference shape and semantic validator;
- [`tests/fixtures/model/valid-planner`](../tests/fixtures/model/valid-planner) —
  the planner example covering all fourteen definition kinds;
- `tests/fixtures/model/invalid-*` — twelve classified rejection fixtures.

ADR-0004 records the decision. If prose, schema and validator disagree, a
consumer must fail closed; prose cannot broaden the machine contract.

## Documents and file homes

File homes are owned by ADR-0003 and issue #4. Every semantic document is a
closed object:

```json
{ "schema_version": "0.1.0", "definitions": [ ... ] }
```

| File | Allowed definition kinds |
| --- | --- |
| `lekalo/project.yaml` | exactly one `project` |
| `lekalo/modules/<module>/module.yaml` | exactly one `module` |
| `entities.yaml` | `scalar`, `enum`, `value-object`, `entity` |
| `commands.yaml` | `command`, `effect` |
| `queries.yaml` | `query` |
| `policies.yaml` | `policy` |
| `events.yaml` | `event` |
| `scenarios.yaml` | `scenario` |
| `bindings.yaml` | `endpoint`, `target-binding` |

A definition in the wrong file is rejected as
`model.kind-not-allowed-in-file`, even when its shape is otherwise valid.
Documents are parsed as JSON in #5. JSON is a YAML subset; block-YAML parsing,
imports, source spans and canonical normalization belong to #7.

## Common fields

Every definition is closed and has these common fields:

| Field | Required | Rule |
| --- | --- | --- |
| `id` | yes | provisional v0.1 grammar: one or two lowercase dot-separated segments, 3–129 characters |
| `kind` | yes | one of the fourteen kinds |
| `version` | yes | integer greater than or equal to 1 |
| `description` | no | non-empty string, at most 2000 characters |
| `derived_from` | no | unique requirement IDs such as `PLANNER-REQ-001` |
| `visibility` | no | `module` or `project` |
| `portability` | no | `portable` or `target-specific` |

`derived_from` is requirement provenance, not identity. Paths, line/column
locations and parser provenance are loader metadata and never become semantic
IDs. Stable path-independent IDs, rename history, aliases and tombstones are
owned by #6 and are intentionally absent from Model v0.1.

Project and module definitions have separate identity namespaces. Other
definitions share one project-wide namespace and use `<module>.<name>`; in
v0.1 the module segment and module definition must match the containing module
directory. This coupling is explicitly provisional for #6 to replace with a
stable semantic-ID contract.

## Named types

Fields always reference a named `scalar`, `enum`, `value-object` or `entity`.
References may be wrapped by `list` or `optional`, one wrapper per level, with
maximum nesting depth four. Anonymous primitive field types and recursion are
not part of v0.1. The named `value-object`/`entity` dependency graph must be
acyclic; direct and mutual recursion fail with `model.type-recursion`.

## Definition kinds

| Kind | Required kind-specific fields | Optional kind-specific fields |
| --- | --- | --- |
| `project` | — | — |
| `module` | — | `imports` (placement only; semantics are #7) |
| `scalar` | `base` | — |
| `enum` | non-empty `values` | — |
| `value-object` | non-empty `fields` | — |
| `entity` | non-empty `fields`, non-empty `identity` | — |
| `command` | — | `input`, `effects` |
| `query` | non-empty `reads` | `returns` |
| `policy` | non-empty `applies_to`, `decision` | — |
| `event` | — | `payload` |
| `effect` | `operation`, `entity` | `emits` |
| `endpoint` | `invokes`, `method`, `path` | — |
| `scenario` | `summary` | `covers` |
| `target-binding` | `target` | — |

Any other `kind` value, including prototype-like strings such as
`constructor`, `toString` and `__proto__`, is classified as `model.constraint`
before kind-specific dispatch.

The model has no arbitrary expression language. Policy decisions are the
closed values `allow` and `deny`; effects are `create`, `update` or `delete`;
endpoint methods are transport-neutral HTTP verbs.

## Semantic validation

JSON Schema owns document and definition shape. The reference validator owns
project semantics that JSON Schema cannot resolve by itself:

- project-wide uniqueness for all non-project/non-module IDs;
- module-directory qualification and module manifest agreement;
- typed cross-reference resolution, distinguishing `model.ref-unresolved`
  from `model.ref-kind-mismatch`;
- acyclic named-type dependencies, reported as `model.type-recursion`;
- entity identity membership (`identity` must name declared fields);
- target-binding resolution against `lekalo/targets/*.yaml`.

Before reading any model document, the checker calls the exported #4 structure
validator on the selected project. A malformed structure is mapped to exit `1`,
stderr and `reasonCodes: ["model.structure-invalid", <structure reasons...>]`.
A physical-policy denial is preserved as exit `3`, stdout and
`reasonCodes: ["model.structure-denied", <structure reasons...>]`; it is never
downgraded to model invalidity. Structure reason order is preserved, and only
logical project-relative paths may appear in either envelope.

After that precondition, only `ENOENT` means that an optional kind document is
absent. A present canonical file that cannot be read fails closed as
`model.scan-failed`; the structure validator separately requires every
discovered module's `module.yaml`. JSON Schema string limits are counted in
Unicode code points, not UTF-16 code units.

Enum entries are objects, so JSON Schema `uniqueItems` cannot express
uniqueness of the nested `value` property when descriptions differ. Duplicate
enum values are therefore an explicit semantic-only `model.constraint` rule.

| Reference | Required target kind |
| --- | --- |
| field, input, payload and return types | `scalar`, `enum`, `value-object`, `entity` |
| `command.effects` | `effect` |
| `query.reads` | `entity` |
| `policy.applies_to` | `command` |
| `effect.entity` | `entity` |
| `effect.emits` | `event` |
| `endpoint.invokes` | `command`, `query` |
| `scenario.covers` | any non-project/non-module definition |
| `target-binding.target` | existing target file |

Import visibility, cycles, short-reference normalization and source locations
are not simulated here; they remain #7 scope.

## Version and evolution policy

Schema version, per-definition version and product version are independent.
`schema_version` is exact and unknown fields are rejected everywhere.

- Non-breaking schema change: add an optional field, add a new optional
  reusable definition, or add a new kind without changing existing branches.
  It requires a reviewed minor schema successor.
- Breaking schema change: remove/rename/re-type a field, tighten an existing
  constraint, change ID grammar, change a closed enum, or remap file homes.
  It requires a reviewed major schema successor plus migration guidance.
- Per-definition `version` increments when that definition changes; it does
  not replace the schema version.
- Published schema bytes are immutable. Unknown/future fields never silently
  pass an older validator.

## Explicit exclusions

Model v0.1 does not include arbitrary cycles, recursion, a full expression
language, UI layout or distributed workflow orchestration. Complex logic is a
future foreign-implementation contract, not an untyped escape hatch in v0.1.

## Commands and exit protocol

```sh
node scripts/check-model.mjs
node scripts/check-model.mjs --project tests/fixtures/model/valid-planner
node scripts/test-model-contracts.mjs
```

Exit `0` means valid. Exit `1` means usage, malformed structure or model shape,
or semantic invalidity and writes a stable leading `model.*` reason to stderr.
Exit `3` preserves a well-formed #4 physical-policy denial and writes its
deterministic JSON envelope to stdout. Model semantics do not introduce an
independent policy-denied class.
