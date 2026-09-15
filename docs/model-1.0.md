# Lekalo Model: stable semantic IDs

The current contract version is **0.2.16**. The normative artifacts are
[Model schema](../contracts/model.schema.v0.2.16.json) and
[semantic IDs](../contracts/semantic-ids.v0.2.16.json). See
[Model](model.md) and [versioning](versioning.md).

Every project and module document must declare exactly `0.2.16`. Unknown or
mixed versions fail as `model.schema-version`.

## ID-bearing fields

Project and module definitions use their one-segment ID classes. All twelve
other definitions and every model reference use symbolId. Module imports use
moduleId. A symbol is either module.name or module.kind_token.name, and the
optional token must match the actual kind.

The module qualifier is the semantic ID inside module.yaml. It is not the
physical directory name. Duplicate module IDs fail as
semantic-id.module-duplicate; duplicate live symbol IDs retain
model.duplicate-id.

## Symbol history and project registry

Only symbol definitions may have renamed_from. It is a non-empty, unique
array of direct historical IDs. Project and module IDs are immutable.

Only the project definition may have id_registry. The object is closed,
non-empty, and contains non-empty rename_history and/or tombstones arrays.
Entry shapes are closed in the schema; graph, live-ID, convergence, and
version relationships are validated project-wide.

Old IDs are never reference aliases. Resolution still consults only the map
of live symbols. A model that keeps an old ID in a field type, query, command,
policy, event, endpoint, scenario, or other reference fails
model.ref-unresolved even when that ID appears in history.

## Deterministic output

For 0.2.16, the validation report exposes semantic module IDs and live symbols
in ascending unsigned UTF-8 byte order. Physical module directories never
appear as semantic module identities. The complete symbol string is also the
verbatim canonical key that the semantic-ID contract requires of its closed
consumer list (target-adapters, generated-artifacts, inspect, impact,
context.capsule, trace.manifest); every member is bound to consume it
unchanged, and all of them remain deferred to their owning issues.

## Commands

    node scripts/check-model.mjs
    node scripts/check-model.mjs --project tests/fixtures/model/valid-planner
    node scripts/check-model.mjs --project tests/fixtures/model-v1/valid-planner
    node scripts/check-model.mjs --check-id planner.policy.update
    node scripts/test-model-contracts.mjs

Exit 0 means valid. Exit 1 covers usage, unknown/mixed schema versions, shape,
ID, registry, and semantic failures on stderr. Exit 3 preserves a
structure-policy denial on stdout before any model document is read.
