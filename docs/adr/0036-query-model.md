# ADR-0036: The declarative query model

Date: 2026-09-11
Status: accepted for issue #64

## Context

Issue #64 asks for typical read operations to be declarative,
portable, and analyzable — without embedding SQL, Eloquent, or
ORM-specific syntax into the core Model. The Model's `query`
definition kind carries only `reads` plus an optional `returns`; the
read semantics (filters, sort, pagination, projections, tenancy,
consistency) had no portable home, so every target invented its own.

## Decision

### 1. One independent attachment family, not a Model change

The Model contract stays at 1.0.0. Query read semantics live in one
closed, versioned attachment (`lekalo/query-model/v1.0.0`) following
the established #23/#24/#26/#62/#63 attachment pattern: bound to one
project and one exact Model pin, closed under
`additionalProperties: false`, identity independent of the product
release and of every other contract family. Changing the Model would
have forced a breaking Model successor, a migration, and downstream
churn in every consumer for data that is declaration, not identity.

### 2. Read-only is structural, not a promise

The attachment has no write surface at all. Every reference slot is
kind-checked against the bound Model: the query slot admits only
`query` definitions, the source slot only `entity` definitions, and
a `command`/`effect` anywhere is refused with an explicit registered
diagnostic naming the write surface. A filter AST cannot mutate; the
wire has no operation vocabulary for writes.

### 3. A closed grammar with hard bounds, and the foreign escape

Filters are a minimal closed AST — ten comparison operators, bounded
`and`/`or`/`not` (depth 8, fanout 16, 64 leaves) over typed
literals and declared parameters. Sort keys end in the entity's
declared identity (the deterministic tie-breaker). Pagination is one
closed limit/offset/cursor shape. Everything richer is out of scope
of the grammar and must go through the foreign escape hatch
(ADR-0033): a declaration with a capability token and a reason, which
blocks managed generation for that query. Raw target text has no
representation anywhere in the wire.

### 4. One plan contract for every target

`QueryPlan::build` projects a validated declaration into a
deterministic, target-neutral operator list (filter, sort,
offset/cursor, limit, project, include, canonical order) with
canonical bytes. It is the single surface adapters map to SQL, ORM
calls, or anything else — which is what makes the pagination
contract identical for the Node and PHP projections by construction,
and what gives scenarios (and future conformance fixtures) a
deterministic artifact to pin. The plan never executes and never
renders target text.

### 5. Tenancy is declared data with a strict gate

The Model has no tenancy concept, so the attachment declares
tenant-scoped entities and their tenant key fields. The default
profile stays silent; the strict profile diagnoses a source whose
tenant key is not constrained by the filter, and refuses includes
into tenant-scoped entities the grammar cannot constrain. Tenant
enforcement semantics (actor binding, evaluation) stay with #25;
this family only owns the declaration and the declaration-time gate.

### 6. Hints are evidence, never guarantees

Consistency profiles and cost hints are requirements on the executing
target and review evidence. No declaration ever claims an execution
guarantee; the plan and the diagnostics carry no performance claims.

### 7. Diff classifies; the verdict stays data

`compare` answers per changed path with breaking / non-breaking /
policy-change (removal of a query, projection field, parameter, or
tenancy scope; reshaping of filter/sort/pagination/consistency; the
foreign escape) over same-family attachments with identical pins;
mixed revisions are typed errors. Semantic-diff integration stays
with #18.

## Consequences

- `contracts/query-model.schema.v1.0.0.json` is the single new
  contract file; the diagnostic registry publishes the additive
  1.21.0 (exactly ten `query.*` rules, `LEK-QRY-001..010`, category
  `semantic`) over the frozen 1.20.0.
- `lekalo query-model validate|diff` is a thin CLI handoff; failures
  exit 1 on stderr, the verdict of a diff never changes the exit
  code.
- Product version, Model/IR/Scenario versions, and the other
  contract families are untouched by this design.
- #18 (semantic diff), #66 (typed expressions), #107 (reference
  evaluation), and the target/profile conformance owners may consume
  the typed data and the plan through the published seams; none of
  them recompute query semantics.

## References

- [Issue #64](https://github.com/ichinya/lekalo/issues/64) — the assignment.
- [ADR-0033](0033-foreign-implementation-escape-hatch.md) — the escape hatch.
- [ADR-0010](0010-diagnostics.md) — the diagnostic contract and registry.
- [ADR-0024](0024-invariant-transition.md) — the attachment pattern precedent.
- [docs/query-model.md](../query-model.md) — the family documentation.
