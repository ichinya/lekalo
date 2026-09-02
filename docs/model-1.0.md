# Lekalo Model 1.0

Status: implementation candidate for issue #6. Model contract version 1.0.0
is independent of reserved product candidate 0.1.4.

Model 1.0.0 is the stable-semantic-ID successor to published Model 0.1.0.
The normative schema is
[contracts/model.schema.v1.0.0.json](../contracts/model.schema.v1.0.0.json);
the ID rules are
[dev.lekalo.semantic-ids@0.1.0](../contracts/semantic-ids.v0.1.0.json).
[Model 0.1](model.md) remains the immutable published record.

## Exact version dispatch

Every project document has exactly one schema_version. The reference checker
accepts only the complete exact sets 0.1.0 and 1.0.0. The project document
selects the validation path and every discovered module document must carry
the same version. An unknown or mixed value fails as model.schema-version.

This is intentionally finite dispatch, not a generic SemVer support range,
loader, migration engine, or deprecation policy.

## Preserved Model surface

Model 1.0.0 was cloned from the exact schema published at commit
b2ace5e893ffd62c80250099792d2e33a0aff3a7. Outside version identity,
ID-bearing fields, symbol rename metadata, and the project registry, it
preserves Model 0.1.0:

- the same fourteen closed definition kinds and file-to-kind mapping;
- the same required and optional kind fields;
- the same requirement, field, description, endpoint, enum, and target
  constraints;
- the same four-level finite type-expression grammar;
- the same typed reference-kind checks and recursion prohibition;
- the same Unicode code-point interpretation of string length;
- the same structure-first physical containment and exit protocol.

The 0.1.0 schema file is not edited. The checker supports both exact contract
versions; this 1.0.0 contract remains an implementation candidate.

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

For 1.0.0, the validation report exposes semantic module IDs and live symbols
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
