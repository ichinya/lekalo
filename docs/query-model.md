# Lekalo declarative query model

Issue #64 makes the read semantics of queries first-class,
machine-checkable contract data. One closed, versioned attachment —
[`contracts/query-model.schema.v1.0.0.json`](../contracts/query-model.schema.v1.0.0.json)
(`lekalo/query-model/v1.0.0`, identity `dev.lekalo.query-model@1.0.0`) —
binds one project to one exact Model pin and declares, per query:
source entities, result cardinality, parameters resolved from the
query input, a closed filter grammar, deterministic sort with a
mandatory identity tie-breaker, the limit/offset/cursor pagination
contract, semantic-link includes, tenant-scoped entities with a
strict-profile tenant filter requirement, consistency/freshness
profiles, bounded read-cost hints, and the foreign/custom escape
hatch. See [ADR-0036](adr/0036-query-model.md) for the owner
decisions.

The attachment is pure declaration and validation data: it never
executes, never generates SQL or ORM calls, never claims runtime
behavior, and has no write surface — a `command` or `effect` symbol
can never occupy the query, source, policy, or scenario slot, and any
such declaration fails with `query.source-invalid`
(`LEK-QRY-003`). Contract identity is independent of the product
release, of the Model/IR versions, of the Scenario IR, of the
authorization and error-contract families, and of the diagnostic
registry.

## Filters

Ten closed comparison operators over typed field references:
`eq`, `ne`, `lt`, `le`, `gt`, `ge`, `in`, `not-in`, `is-null`,
`is-not-null`, combined by bounded `and`/`or`/`not` combinators
(depth 8, fanout 16, at most 64 comparison leaves per filter). Filter
values are typed literals (boolean, integer, canonical decimal,
string, date, datetime, uuid, uri, enum member, or a closed set) or
references to declared parameters. There is no expression language
here: richer conditions reference the future #66 typed-expression
family by opaque reference, and nothing in this AST evaluates
anything or mutates anything.

At resolution time every leaf is typed against the bound Model:

- filter fields must exist on the source entity;
- scalar literals must carry the field's scalar base; enum literals
  must be declared members of the field's enum;
- range operators are restricted to the orderable bases (`string`,
  `number`, `date`, `datetime`); `boolean`, `uuid`, and `uri`
  support only equality, membership, and nullness;
- parameter types are parsed with the bounded Model type-expression
  grammar and must match the filtered field's type.

## Sort and pagination

Sort keys carry `asc`/`desc` and keep their declared order. A query
with `page` or `stream` cardinality must declare a total order, and
that order must end in the source entity's declared identity fields
as a trailing segment — the deterministic tie-breaker that keeps
repeated runs stable where the API requires it.

One closed pagination shape serves every target projection:
`offset` (limit, optional offset) or `cursor` (limit plus a key
parameter whose type must match the trailing identity sort key).
Because the Node, PHP, and every other adapter map the same wire —
and the same canonical plan — the pagination contract is identical
across targets by construction.

## Projections and semantic links

`selection` projects explicit output fields (with optional output
aliases); the declared order is behavioral and preserved. Fields must
exist on the source entity, must project scalar/enum/value-object
leaves (relations are read through includes, not through row
payloads), and must respect the visibility boundary: a
project-visibility query never exposes a module-visibility type.

`includes` join related entities through semantic links: bounded
paths (at most 4 segments) of entity-typed relation fields. Every
path segment is kind-checked against the bound Model.

## Tenancy, consistency, and hints

The attachment declares which entities are tenant-scoped and which
field carries the tenant key. Under the strict profile, a query over
a tenant-scoped source must constrain that field with `eq`/`in` in
the logical sense: every row satisfying the filter must satisfy a
tenant `eq`/`in` leaf. The check follows the filter structure — a
tenant leaf under `not` constrains nothing, and every `or` branch
must be tenant-constrained on its own, while one constraining
`and` conjunct (for example a factored tenant leaf) covers the whole
conjunction (`query.tenant-filter-missing`, `LEK-QRY-007`). An
include whose terminal entity is tenant-scoped is refused unless the
grammar can constrain it — the default profile stays silent.

The consistency profile (`strong` | `bounded` | `stale-ok`) is a
requirement on the executing target, never a guarantee by
declaration; `bounded` requires a declared staleness bound in
seconds and the other profiles forbid one. Cost hints (`maxRows`,
`expensive`) are planner requirements and review evidence, never
execution guarantees.

## The foreign escape hatch

A query whose mapping exceeds the closed managed grammar (ranked
search, window functions, custom storage behavior) is declared
`foreign` with a capability token and a bounded reason. The plan
projection then emits no managed steps: managed generation is
blocked for that query and the mapping belongs to the foreign
implementation (ADR-0033). This is the only way out of the managed
grammar — raw target SQL, Eloquent, and ORM-specific syntax have no
representation in the portable section.

## The plan projection

`lekalo_core::query_model::plan::QueryPlan::build` turns one
validated declaration into a deterministic, target-neutral operator
list — filter, sort, offset/cursor, limit, project, include, in
canonical order — with compact canonical bytes. It is the artifact
scenario tests (and target conformance fixtures) can pin: the same
plan maps to SQL, to Eloquent, or to any other storage mapping, and
the mapping equivalence stays with the conformance owners, not with
core runtime code. `QueryGraphFacts` provides the typed
dependencies-and-effects data (reads, includes, policy, scenario
edges) a graph or context consumer may project.

## Diff

`lekalo_core::query_model::compare` is a pure semantic diff over
same-family attachments: **breaking** (a query removed, source or
cardinality changed, a projection field or parameter removed, a
policy reference dropped, a tenancy scope removed), **non-breaking**
(additions, cost hints, scenario references, staleness tightening),
and **policy-change** (filter, sort, pagination window, consistency,
foreign escape, or tenancy additions). Foreign projects and mixed
Model/IR/attachment revisions are the typed error set, never a
guessed classification.

## Diagnostics

Failures emit the accepted #11 diagnostic contract with the registry
minor 1.20.0 → 1.21.0 (additions only):
`query.input-invalid` (LEK-QRY-001),
`query.contract-invalid` (LEK-QRY-002),
`query.source-invalid` (LEK-QRY-003),
`query.filter-invalid` (LEK-QRY-004),
`query.sort-invalid` (LEK-QRY-005),
`query.pagination-invalid` (LEK-QRY-006),
`query.tenant-filter-missing` (LEK-QRY-007),
`query.visibility-boundary` (LEK-QRY-008),
`query.reference-invalid` (LEK-QRY-009), and
`query.export-limit` (LEK-QRY-010), category `semantic`, with
bounded fixed tokens only. See
[docs/diagnostics.md](diagnostics.md).

## Boundaries

No execution, adapter generation, SQL or ORM mapping, transaction
semantics (#24), authorization execution (#25), scenario execution
(#23), or report/trace surface belongs here. Error identity stays
with #62 (opaque refs only), capability registries and profile
resolution stay with #27/#28/#29, expression evaluation stays with
#66/#107. Tests are hermetic; fixtures live under
[`tests/fixtures/query-model/`](../tests/fixtures/query-model/).
