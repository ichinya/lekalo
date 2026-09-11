# Lekalo domain relations and storage projections

Issue #65 separates the target-neutral domain model from target-namespaced
storage projections. One closed, versioned attachment —
[`contracts/storage-projection.schema.v1.0.0.json`](../contracts/storage-projection.schema.v1.0.0.json)
(`lekalo/storage-projection/v1.0.0`, identity
`dev.lekalo.storage-projection@1.0.0`) — binds one project to one exact
Model pin, IR digest, and attachment revision, and declares the domain
layer (entity identity, value types, aggregate ownership, semantic
relations with cardinality and explicit delete behavior, invariant and
lifecycle references, visibility) beside the storage layer (tables,
columns, keys, indexes, foreign keys, join tables, technical and
generated fields, soft-delete and tenant policies, audit timestamps,
migration history). See
[ADR-0025](adr/0025-storage-projection.md) for the owner decisions.

The attachment is pure declaration and validation data: it never
executes, never generates SQL, never claims runtime behavior, and never
carries source text, physical paths, raw tokens, credentials, runtime
values, timestamps, host data, or provider output. Contract identity is
independent of the product release, of the Model/IR/Scenario versions,
of the error-contract, invariant-transition, and authorization families,
and of the diagnostic registry.

## Domain layer

A domain entity binds one stable **entity key** to one Model symbol id.
The key is the join point for relations and projections, so a Model
rename (`planner.focus_task` to `planner.work_item`) rebinds exactly one
domain member and never implies a table rename. Each entity declares:

- `external` — an external or remote entity that this project never
  stores locally. Such an entity is modeled and referenced but must not
  appear in any storage projection: storage mapping is optional for
  external entities and forbidden at the same time.
- `aggregateRoot` / `aggregateOwner` — explicit aggregate ownership.
  An owner must be a declared root; ownership chains terminate (the
  root-plus-owner combination is invalid, so cycles are unreachable).
- `visibility` — `public`, `internal`, or `private`. Only public
  members enter the public DTO projection.
- `fields` — closed value types (`boolean`, `integer`, `decimal`,
  `string`, `text`, `uuid`, `date`, `timestamp`, `binary`, `json`)
  with exact parameters, declared optionality, and visibility. No
  Eloquent or SQL syntax exists anywhere in the domain layer.
- `invariants` / `stateSpaces` — opaque references into the #63
  invariant-transition family. Resolution stays with that owner.

## Relations

Seven closed kinds: `one_to_one`, `one_to_many`, `many_to_many`,
`external_reference`, `aggregate_child`, `optional_reference`, and
`polymorphic`. Every relation declares explicit cardinality
(`min`..`max` per owner), explicit delete behavior (`cascade`,
`restrict`, or `detach` — there is no default), and scenario or
constraint coverage: at least one pinned Scenario IR reference or one
opaque #63 constraint reference. Kind coherence is enforced:

| Kind | Requires | Forbids |
| --- | --- | --- |
| `one_to_one` | `max = 1`, foreign key | detach above `min = 0` |
| `one_to_many` | `max >= 2`, foreign key | detach above `min = 0` |
| `many_to_many` | join table per projection | foreign key |
| `external_reference` | external target | cascade, restrict, foreign key |
| `aggregate_child` | target owned by this aggregate | detach |
| `optional_reference` | `min = 0`, foreign key | cascade, `min > 0` |
| `polymorphic` | per-namespace materialization | foreign key |

`external_reference` models a reference by external identity: the
remote entity is never stored locally, so the relation detaches and
materializes nothing. `polymorphic` exists only as an explicit target
capability: a namespace materializes it only where it declares the key
and discriminator columns; nothing is implicit.

## Storage projections

A projection is one closed target namespace. v1 ships `postgres` and
`laravel`. Every local entity is mapped exactly once per namespace;
external entities are never mapped. Declared storage facts are
explicit: primary key, technical columns (storage-only, with declared
type, purpose, and nullability), generated columns (identity,
computed, sequence), soft-delete policy, tenant partition key, audit
timestamps, indexes, join tables for every many-to-many relation,
polymorphic materializations, and the migration history with a visible
data risk (`none`, `backfill_required`, `destructive`) per record.

Every table name, column name, technical type, and tenant key type is
declared. Nothing is invented.

## Derivation from the same domain model

`lekalo_core::storage_projection::project` is the published pure
function from one validated attachment to the target-namespaced
projection. Every column is one of:

1. a declared domain field rendered through the namespace type table
   (for example `string(200)` renders `varchar(200)` in PostgreSQL and
   `string(200)` in Laravel; `timestamp` renders `timestamptz` versus
   `datetime`);
2. a foreign key derived from one relation's explicit `foreignKey`
   declaration, typed by the referenced entity's single-column primary
   key, nullable only when the declared behavior detaches, carrying the
   declared delete action (`cascade` to `CASCADE`, `restrict` to
   `RESTRICT`, `detach` to `SET NULL`; join rows sever with `CASCADE`);
3. an explicit declared storage fact (technical, generated, soft
   delete, tenant key, timestamps);
4. an explicit polymorphic materialization (target key typed by the
   target primary key, discriminator rendered from the namespace
   bounded-string type).

One-to-one foreign keys derive a unique index. Composite primary keys
are legal declarations, but any derivation that would need a key
column refuses explicitly instead of guessing. The same domain model
derives both namespaces — the committed derived goldens under
`tests/fixtures/storage-projection/derived/` are two renderings of one
source, proven byte-identical on every run.

## The public DTO projection

`public_fields` derives one entity's public DTO members from the
domain declarations only. A storage-only technical column lives in the
projection layer and has no domain declaration, so it can never enter
the public surface — the guarantee is structural, not conventional.

## Diagnostics

Failures emit the accepted #11 diagnostic contract with the registry
minor 1.9.0 → 1.22.0 (additions only):
`storage.input-invalid` (LEK-STO-001),
`storage.domain-invalid` (LEK-STO-002),
`storage.relation-invalid` (LEK-STO-003),
`storage.projection-invalid` (LEK-STO-004),
`storage.mapping-invalid` (LEK-STO-005),
`storage.diff-invalid` (LEK-STO-006), and
`storage.export-limit` (LEK-STO-007), category `semantic`, with
bounded fixed tokens only. The validation profiles and their schema
pin the registry version and move with it. See
[docs/diagnostics.md](diagnostics.md).

## Diff: domain, wire, and storage changes classify separately

`lekalo_core::storage_projection::compare` is a pure comparison of
same-family attachments. Every changed path carries its layer —
**domain**, **storage**, or **wire** (the contract envelope) — and one
compatibility class: **breaking** (a guarantee removed or narrowed:
an entity, relation, field, or table removed, a cardinality narrowed,
a type narrowed, visibility narrowed), **non-breaking** (additions and
widenings: a new relation, a lengthened string, a visibility widened,
a Model symbol rebind), and **policy-change** (explicit owner
decisions with unchanged guarantees: delete behavior, aggregate role,
soft-delete or tenant policy, polymorphic materializations,
migration-history bookkeeping). Storage paths additionally carry the
visible data risk, so a migration obligation is never hidden inside a
class. Because layers are separate, a domain rename is provably not a
table rename: the rebind produces the domain path and, with an
unchanged entity key, no storage path at all.

## Boundaries

No runtime storage, adapter generation, SQL emission, transaction
semantics (#24), authorization execution (#25), scenario execution
(#23), reference evaluation (#107), or report/trace surface belongs
here. Invariant and state-space identity stays with #63 (opaque refs
only); Laravel migrations (#57), the PostgreSQL profile (#69), the
multi-provider TaskExternalLink pilot (#52), and Drizzle bindings
(#116) consume this contract; capability registries and profile
resolution stay with #27/#28/#29. Tests are hermetic; fixtures live
under [`tests/fixtures/storage-projection/`](../tests/fixtures/storage-projection/).
