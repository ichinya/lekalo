# ADR-0005: Stable semantic IDs

Status: implementation candidate for issue #6 and reserved product candidate
0.1.4. Issue #3 published product 0.1.3. Product, Model schema, and
semantic-ID contract versions are independent.

Date: 2026-09-02

## Context

Issue #6 requires a target-neutral primary identity for application elements.
Model 0.1.0 deliberately shipped a provisional one-or-two-segment definition
ID coupled to a module directory. The final contract must survive file moves,
generated-name changes, and semantic renames while preventing ambiguous
resolution or reuse of a retired meaning.

Model 0.1.0 is already published at commit
b2ace5e893ffd62c80250099792d2e33a0aff3a7 under annotated product tag
v0.1.2. Its schema bytes cannot change. The ID grammar change is breaking
under ADR-0004, so it requires a major Model schema successor even though the
semantic-ID data contract begins at 0.1.0.

## Decision

Adopt
[dev.lekalo.semantic-ids@0.1.0](../../contracts/semantic-ids.v0.1.0.json)
and [Model schema 1.0.0](../../contracts/model.schema.v1.0.0.json).

1. Human-readable dotted IDs are the sole primary approach. Opaque IDs are
   rejected for M1 because reviews and traces must expose stable,
   understandable keys.
2. Every segment is ASCII and matches ^[a-z][a-z0-9_]{0,62}$. Input is never
   normalized. Dot is the segment separator and underscore the word
   separator.
3. Project and module IDs have one segment and reserve lekalo and dev.
   Project and module semantic IDs are immutable. Module IDs are unique in a
   project and independent of physical directories.
4. Symbol IDs have two or three segments and a maximum length of 191. Their
   first segment is the declared module ID. The optional middle segment is
   the exact closed kind token; it is never inferred or treated as equivalent
   to an unqualified spelling. The 191 figure is the schema-level arithmetic
   ceiling of three 63-byte segments plus two dots; the closed kind namespace
   imposes the stricter accepted contextual maximum of 142 characters for
   kind-namespaced IDs and 127 for plain IDs, enforced through segment rules
   rather than a total-length branch.
5. Live symbols are globally unique. Canonical order is ascending unsigned
   UTF-8 bytes of the complete ID. Source order is not a validity condition.
6. renamed_from exists only on symbols and names direct historical sources.
   A closed project id_registry carries the matching versioned rename graph
   and permanent deleted/replaced tombstones.
7. Rename history is functional and acyclic, terminates in live symbols, and
   has strictly increasing definition versions along each chain. Direct
   incoming sources exactly match the terminal symbol metadata.
8. Multiple old IDs may converge only when every incoming edge explicitly
   sets same_identity to true. This records one identity with several prior
   names. Semantic merges and breaking replacements use tombstones instead.
9. Historical and tombstoned IDs never resolve live references. Their only
   purpose is traceability.
10. Target adapters retain the exact semantic ID as their canonical key.
    Target-side names are additional metadata and cannot rewrite identity.
11. The reference checker dispatches only exact Model 0.1.0 and 1.0.0
    documents and rejects mixed projects. This bounded dispatch does not
    implement the generic loader or migration system owned by later issues.

## Consequences

- Model schema 0.1.0 remains byte-identical and published. Existing 0.1.0
  projects continue through their exact legacy validation path.
- Model schema 1.0.0 changes only version identity, ID-bearing fields, and
  the symbol-history/project-registry additions. All fourteen kind shapes,
  finite type depth, string limits, file homes, and reference kinds are
  preserved.
- A physical module move has no identity effect. A semantic rename is
  explicit and reviewable. Reuse of a deleted or replaced meaning fails
  closed.
- The machine contract closes the canonical-key consumer list at exactly six
  members (target-adapters, generated-artifacts, inspect, impact,
  context.capsule, trace.manifest) and binds every one of those six members
  to the verbatim semantic ID; the checker is the only implemented surface.
  Those six consumers, plus the separate downstream IR work that sits outside
  this closed list, stay owned by their respective issues without issue #6
  implementing any of them.
- Migration can be a schema_version-only rewrite only for projects whose IDs
  already conform. Nonconforming IDs require owner-authored semantic changes;
  automatic case-folding, truncation, hyphen translation, and fuzzy aliasing
  are forbidden.
- The stricter grammar is a Model major version, not a product major release.
