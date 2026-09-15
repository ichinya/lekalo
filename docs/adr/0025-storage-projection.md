# ADR-0025: domain relations and storage projections

Status: accepted for issue #65. The contract is an implementation
candidate; tagging and publication stay coordinator-owned.

## Context

The Model owns entities, fields, and typed references (#5/#6), and #63
owns invariants and state transitions. Two things are still missing,
and they fail in the same direction: semantic relations exist only as
field type references (no cardinality, no delete behavior, no
aggregate ownership), and storage reality (tables, columns, keys,
indexes, join tables, technical fields, soft-delete policies, tenant
keys, migration history) has no contract at all. Without a boundary,
storage concerns leak into the domain model and vice versa: renaming a
symbol forces a table migration, a technical ORM column appears in
public DTOs, and hidden framework behavior replaces explicit decisions.
The #18 semantic-diff already reserves `storage-review` hints and a
`storage-consumer` profile awaiting typed storage evidence.

## Decision

One closed, independent, versioned attachment family —
`lekalo/storage-projection/v1.0.0` — binds one project to one exact
Model pin, IR digest, and attachment revision, and declares:

1. **Domain layer, target-neutral.** Stable entity keys independent of
   both the Model symbol id and every table name; closed value types;
   explicit aggregate ownership (root or owner, never both, owners must
   be roots, so ownership cycles are unreachable); entity and field
   visibility; opaque references to #63 invariants and state spaces;
   seven closed relation kinds with explicit cardinality, explicit
   delete behavior, and mandatory scenario or constraint coverage. No
   Eloquent or SQL syntax exists in the domain layer.
2. **Storage layer, namespaced.** Closed namespaces in v1 (`postgres`,
   `laravel`); every local entity mapped exactly once, external
   entities never mapped; explicit tables, technical and generated
   columns, soft-delete, tenant keys, timestamps, indexes, join tables,
   explicit polymorphic materializations, and migration history with a
   visible data risk per record. Every name and type is declared;
   nothing is invented.
3. **Derivation, not duplication.** `project` is the published pure
   function from the attachment to the namespace projection: domain
   fields through a published namespace type table, foreign keys from
   relation declarations (typed by the referenced single-column primary
   key, nullable only under detach, carrying the declared delete
   action), plus the declared storage facts. Both namespaces derive
   from the same domain model.
4. **Public DTO derivation.** `public_fields` reads the domain layer
   only; storage-only technical columns are structurally excluded from
   the public surface.
5. **Layer-classifying diff.** `compare` separates domain, storage,
   and wire (envelope) changes per path, classifies breaking versus
   non-breaking versus policy-change, and attaches a visible data risk
   to storage obligations. Unlike #63, envelope differences classify
   as wire paths instead of refusing: the acceptance criterion demands
   that wire, domain, and storage changes be distinguishable in one
   comparison.
6. **Diagnostics through the #11 seam: registry minor 1.9.0 → 1.22.0.**
   The contract adds its own `storage.*` rule family — reusing
   `invariant.*` or `transaction.*` would blur contract families. The
   registry takes its wire-shape-preserving additive increment to
   [`diagnostic-registry.v1.22.0.json`](../../contracts/diagnostic-registry.v1.22.0.json)
   with `storage.input-invalid`, `storage.domain-invalid`,
   `storage.relation-invalid`, `storage.projection-invalid`,
   `storage.mapping-invalid`, `storage.diff-invalid`, and
   `storage.export-limit` (LEK-STO-001..007, category `semantic`),
   zero mutations of the published entries, strictly id-sorted,
   bounded fixed tokens only. The validation profiles and their schema
   pin the registry version and move with it.

## Alternatives considered

- **Two families (domain and storage).** Rejected: the domain relations
  and their storage consequences must be validated against each other;
  two families would need a third to bind them.
- **Declaring full column lists per projection.** Rejected: it
  duplicates the domain model and makes the projections two models
  instead of two renderings. Derivation keeps one source and makes the
  acceptance "generated from the same domain model" literal.
- **Inventing default names (implicit join tables, implicit foreign
  keys).** Rejected: hidden ORM behavior is exactly what the issue
  forbids. Every name is explicit at the domain or projection layer.
- **Model-level relations instead of an attachment.** Rejected: the
  Model is a frozen published contract (1.0.0); relations with storage
  semantics are a new independent concern with its own versioning.

## Consequences

- Renaming a Model symbol is a one-member domain rebind; table names
  follow entity keys, so no storage change is implied. The diff proves
  this per path instead of asserting it.
- External and remote entities are modeled but never projected; the
  validator refuses any storage mapping for them.
- Laravel migrations (#57), the PostgreSQL profile (#69), the
  TaskExternalLink pilot (#52), and Drizzle bindings (#116) consume
  this contract instead of re-deriving storage decisions.
- #18 consumes `storage.*` evidence through the diff classification;
  the `storage-review` hint keeps its meaning.

## Boundaries

Pure declaration, validation, derivation, and comparison. No runtime
storage, no SQL emission, no adapter execution, no transaction
semantics (#24), no scenario execution (#23), no report or trace
surface (#22/#103), and no enforcement claim without target evidence.
