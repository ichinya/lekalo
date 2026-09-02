# Stable semantic IDs

Status: implementation candidate for issue #6. The prospective product
candidate is 0.1.4 because issue #3 published product 0.1.3. Product releases,
Model schema versions, and the semantic-ID contract version are independent.

The normative machine artifacts are
[dev.lekalo.semantic-ids@0.1.0](../contracts/semantic-ids.v0.1.0.json) and
[Model schema 1.0.0](../contracts/model.schema.v1.0.0.json). The design
decision is [ADR-0005](adr/0005-semantic-ids.md), and
[Model 1.0](model-1.0.md) describes the enclosing document contract. If these
artifacts and prose disagree, consumers fail closed.

## Identity classes

A semantic ID is the primary application-element identifier. It never comes
from a target language, generated class name, source path, or physical module
directory.

| Class | Shape | Additional rule |
| --- | --- | --- |
| project | one segment | immutable; lekalo and dev are reserved |
| module | one segment | immutable, unique in the project, independent of its directory, and lekalo and dev are reserved |
| symbol | two or three segments | globally unique; first segment is the declared module ID |

Both project and module IDs reserve lekalo and dev. The machine contract
lists the reserved set for both ID classes, and the validator rejects either
reserved word in either class.

Every segment matches the ASCII-only expression:

    ^[a-z][a-z0-9_]{0,62}$

Input is used exactly as supplied. Validation never normalizes, case-folds,
translates hyphens, or truncates. Dot separates segments and underscore
separates words. Uppercase, Unicode, hyphen, whitespace, colon, slash, an
empty segment, and a segment longer than 63 bytes are invalid.

Symbol forms are:

    planner.focus_task
    planner.event.focus_changed

The optional middle segment is an exact kind namespace. Its closed token set
is scalar, enum, value_object, entity, command, query, policy, event, effect,
endpoint, scenario, and target_binding. When present, the token must match the
definition kind after replacing a hyphen in the kind with underscore.
Namespaced and plain IDs are distinct; no fuzzy equivalence exists.

Two length layers exist and must not be confused. The schema-level ceiling is
191 characters, the arithmetic maximum of three 63-byte segments plus two
dots. The accepted contextual maximum is stricter because the optional middle
segment must come from the closed kind namespace: 142 characters for a
kind-namespaced symbol (63 + 1 + 14 + 1 + 63) and 127 for a plain
two-segment symbol (63 + 1 + 63). The validator enforces segment grammar and
the closed token set and has no total-length branch; a longer well-formed
input fails its segment or class rule, never a length rule.

## Stable module qualification and ordering

The first symbol segment matches the module ID declared inside module.yaml,
not the directory component under lekalo/modules. For example, a directory
may move from work-items to planner-v2 while its module ID remains planner and
every planner.* symbol remains unchanged.

Canonical ordering compares the complete dotted UTF-8 encoding as unsigned
bytes in ascending order. Valid IDs are ASCII, so this rule has one
cross-platform result. Source arrays need not already be sorted; canonical
consumers sort them. Locale comparison is never canonical.

## Rename taxonomy

| Change | Semantic ID action |
| --- | --- |
| cosmetic display or description change | keep the ID; no history entry |
| target-side binding or generated-name change | keep the ID; record only target metadata |
| same symbol receives a new semantic ID | add symbol-only renamed_from and a rename_history edge |
| different meaning replaces or merges an old symbol | create a new live ID and tombstone the old ID |

Project and module IDs are immutable in this contract. renamed_from is
allowed only on the twelve symbol kinds.

Historical IDs are traceability metadata, never live aliases. All model
references must name an exact current symbol. A reference to an ID found only
in renamed_from, rename_history, or tombstones is model.ref-unresolved.

## Rename graph

The optional project id_registry is a closed, non-empty object containing one
or both non-empty arrays: rename_history and tombstones.

A rename entry has from, to, definition_version, optional note, and optional
same_identity with the sole value true. The graph obeys all of these rules:

- every from has exactly one outgoing edge;
- from is neither live nor tombstoned;
- to is a later rename source or an exact live symbol;
- the graph is acyclic and every chain ends at one live symbol;
- a live symbol's renamed_from set exactly equals its direct incoming sources;
- definition_version strictly increases along a chain and never exceeds the
  terminal live symbol version;
- two or more edges may converge only when every converging edge explicitly
  has same_identity true.

Convergence asserts that historical names are the same identity. It is not a
semantic merge. A merge or breaking replacement uses tombstones.

Example chain:

    {
      "rename_history": [
        {"from":"planner.old","to":"planner.former","definition_version":2},
        {"from":"planner.former","to":"planner.current","definition_version":3}
      ]
    }

Only planner.former appears in planner.current.renamed_from because it is the
direct incoming source.

## Tombstones

A deleted tombstone has id, reason deleted, and since. A replaced tombstone
also requires replaced_by, which must be an exact live symbol. deleted
forbids replaced_by. Tombstone IDs are unique and disjoint from live IDs and
every rename-graph ID. since is the project definition version at retirement
and cannot exceed the current project version.

Once published, a tombstone is permanent. Removing it and reusing its ID for
another meaning violates the contract even if one isolated snapshot could not
reconstruct the missing history. Any registry mutation increments the project
definition version; a project at version 1 cannot contain a registry.

## Target canonical keys

The machine contract closes the list of canonical-key consumers:
target-adapters, generated-artifacts, inspect, impact, context.capsule, and
trace.manifest. Every one of them uses the semantic symbol ID verbatim as its
canonical key and must never invent, derive, replace, rewrite, case-fold,
translate, or synthesize the canonical ID. Target bindings may add
target-side names only.

Issue #6 is a contract-foundation issue. The reference checker is the one
implemented canonical-key surface: the validation report emits live symbols
verbatim and --check-id repeats the input as canonicalKey after validation.
Every listed consumer is deferred to its owning issue and, once implemented,
must consume the checker-verified key exactly. The list is closed: adding a
canonical-key consumer requires a semantic-ID contract revision, not an
adapter-local decision.

## Validation and diagnostics

Validation precedence is structure containment, document parse and closed
shape, exact/uniform schema version, ID grammar/class/reservations,
module/symbol uniqueness, qualification/kind namespace, registry integrity,
live-only reference resolution, and finally named-type recursion. Within
registry checks, IDs are compared in canonical byte order, so source-array
reordering does not choose a different first failure.

The semantic-ID reason family is closed in the machine contract. Important
examples include semantic-id.grammar, semantic-id.id-class,
semantic-id.module-duplicate, semantic-id.kind-namespace,
semantic-id.alias-ambiguous, semantic-id.rename-cycle,
semantic-id.rename-version, semantic-id.registry-overlap, and
semantic-id.tombstone-reuse.

Run a grammar probe or validate a complete versioned project:

    node scripts/check-model.mjs --check-id planner.event.focus_changed
    node scripts/check-model.mjs --project tests/fixtures/model-v1/valid-planner
    node scripts/test-model-contracts.mjs

The ID probe returns exit 0 and repeats the input as canonicalKey when valid.
It returns exit 1 with semantic-id.* diagnostics when invalid. Complete model
validation retains the structure-first exit protocol documented for Model
0.1.
