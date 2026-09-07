# ADR-0022: Explicit error contracts and typed results

Date: 2026-09-07
Status: accepted for issue #62

Custody: issue #62 carries the **prospective product candidate 0.1.29** in
every accepted path (workspace `Cargo.toml`, both `lekalo` packages in
`Cargo.lock` including the regenerated committed golden lock and its
digests, the `--version` behavior and its pinning tests, `README.md`,
`docs/cli.md`); issue #25 published product 0.1.28 (annotated tag
`v0.1.28` on `967bf52`); issue #24 published product 0.1.27 (annotated
tag `v0.1.27` on `ef7680d`); issue #18 published product 0.1.26
(annotated tag `v0.1.26` on `3710179`). The error-contract and error-registry versions
(`lekalo/error-contract/v1.0.0`, `lekalo/error-registry/v1.0.0`,
identities `dev.lekalo.error-contract@1.0.0` and
`dev.lekalo.error-registry@1.0.0`) are independent of the product
release, of the Model/IR/diagnostic/protocol contract versions, and of
every other registered family by design. The placeholder 0.1.29 is
reconciled to the publication-order version at integration.

## Context

Issue #62 requires operation errors to be a formal, versioned part of the
behavior contract: typed unions, stable semantic identity, closed payload
schemas, public/private message separation, retry/idempotency/effect
metadata, coverage or waiver for every declared error, mappings that
never lose the canonical identity, and a semantic diff with explicit
breaking rules.

The research briefs (run run_088695f63032: briefs msg_278d6f6081e,
msg_9f74b82e4e0d, msg_e520c7d6f75a, msg_c0a0b51ae37f, and worker_done
msg_1de2cf979040) established the hard constraint: the accepted Model
contracts have closed command/query shapes with no `errors` field, and
their bytes are immutable. The owner decision recorded by the coordinator
adopts the briefs' narrow recommendation: an **independent, versioned
Error Contract binding** — not a Model successor — with a typed operation
attachment instead of in-place IR embedding.

## Decision

### 1. An independent closed contract family, not a Model successor

Two immutable wire contracts are published:
[`contracts/error-contract.schema.v1.0.0.json`](../../contracts/error-contract.schema.v1.0.0.json)
(discriminator `lekalo/error-contract/v1.0.0`) for one typed operation
binding, and
[`contracts/error-registry.schema.v1.0.0.json`](../../contracts/error-registry.schema.v1.0.0.json)
(discriminator `lekalo/error-registry/v1.0.0`, identity
`dev.lekalo.error-registry@1.0.0`) for the registry envelope, plus the
canonical registry instance
[`contracts/error-registry.v1.0.0.json`](../../contracts/error-registry.v1.0.0.json).
Model v0.1.0 and v1.0.0 are untouched; IR v1 is untouched — the binding
is a typed sidecar, and any future IR embedding requires a reviewed IR
successor.

### 2. The typed Result binding

`OperationErrorContract { operation, kind, output, errors }` is the
explicit `Result<Output, ErrorUnion>` of one command or query: `output`
is `null` (unit) for commands and the accepted Model type expression for
queries; the union is closed, non-empty, unique, and canonically sorted
by unsigned UTF-8 error id. There is no catch-all member. Every member
resolves to a registry-declared error.

### 3. Immutable identity; category is classification, not inference

Each error carries one stable semantic id (the accepted #6 qualified
grammar) and one immutable machine code (`LEK-ERR-NNN`). Codes are unique
forever; retired codes are tombstoned in the registry and never
reassigned — reuse is a diff `invalid`, not a compatibility guess. The
category vocabulary is exactly
`validation|auth|conflict|not-found|domain|infrastructure`; it is a
declared classification, never inferred from code prefixes, and it never
implies authorization behavior (#25 owns that).

### 4. Closed payloads and the public/private boundary

Payloads are closed named-field schemas over the accepted Model type
grammar (depth at most four, at most 64 fields), each field with explicit
`public`/`private` exposure. Message templates are stable ids plus the
payload fields they may render; public templates may reference only
public payload fields. Values are validated against the declared schema
(scalar base or enum member), unknown fields fail closed, and private
fields never enter any public projection, payload, or vector.

### 5. Retry, idempotency, and effect agree or the contract is refused

The closed agreement table (v1): `none`/`read` effects force
`not-applicable` idempotency and only `never`/`safe` retries; `write`
effects allow `safe` retries only for guaranteed-idempotent operations
and `conditional` retries only with a key or an already-guaranteed
replay; `destructive` effects are never `safe` and never `guaranteed`;
`external` effects may be key-guarded. Field-level effect graphs,
transactions, and detected-vs-declared comparisons stay with #14/#26.

### 6. Reachability: union membership, scenario, or waiver

Every declared error must be reachable — referenced by some operation
union — or carry scenario references, or carry an explicit bounded
waiver (reference, owner, reason, optional expiry). Test-only coverage
cannot witness reachability of an unbound error. Scenario and test
references stay opaque until #23 publishes its registry; #62 never
defines scenario steps or executes tests.

### 7. Diagnostics through the #11 seam: registry minor 1.6.0 → 1.7.0

The contract adds its own rule family — typed `error.*` identities — so
the diagnostic registry takes its next wire-shape-preserving minor
increment to [`diagnostic-registry.v1.7.0.json`](../../contracts/diagnostic-registry.v1.7.0.json)
with `error.contract-invalid`, `error.binding-invalid`,
`error.payload-invalid`, `error.unreachable`, `error.code-reused`,
`error.retry-idempotency-conflict`, `error.coverage-invalid`,
`error.mapping-missing`, `error.mapping-invalid`, `error.diff-invalid`,
`error.limit-exceeded`, and `error.registry-invalid` (LEK-ERR-001..012,
category `compatibility`), additions-only with zero mutations of the
published 154 entries. Every echoed token is bounded before
construction. A diagnostic id is never an error id; diagnostics reference
error identity as data, and `reasonCodes` stay derived diagnostic ids.

### 8. Mappings are projections; identity is never projected away

Language-neutral vectors (Node's discriminated `{ok,value}` result,
PHP's typed result fields, Go's `(T, error)` accessors) all carry the
same canonical `{id, code, category, public payload}` quadruple; HTTP
status is absent from the core contract entirely and a status-only entry
is non-conformant in any profile. Target mappings declare their entries
per immutable code; a catch-all mapping is refused under the strict
profile (`error.mapping-invalid`), a strict mapping that misses a union
member reports `error.mapping-missing`, and entries that fail to
preserve identity cannot even be built. Unknown infrastructure stays a
separate typed channel (`InfrastructureFailure`:
`timeout|crash|invalid-process|io|provider`) that never masquerades as a
declared error and renders only a fixed generic summary.

### 9. Semantic diff with explicit breaking rules

`diff` compares two validated registries and classifies every change as
`breaking`, `non-breaking`, `policy-change`, or `invalid` in fixed
unsigned-byte path order: adding an error is non-breaking; removing an
error or a union member, changing id/code/category, reusing a code,
payload field changes (except an optional additive field, which is a
policy decision), public message changes, tightening retry, weakening
idempotency, and strengthening effects are breaking; observability,
source, and private-message changes are non-breaking; coverage, policy,
and loosening changes are policy decisions. Compatibility is never
inferred from numeric versions, statuses, or exception names.

### 10. Bounds and determinism

10000 errors/bindings/tombstones, 256 union members, 64 payload
fields/values, 64 coverage refs, 2000-byte text values, 256-byte tokens,
32 MiB canonical result. Canonical bytes are compact UTF-8 JSON with
byte-sorted keys and no final line feed; the embedded registry must
equal its own canonical re-rendering, so duplicate keys, unknown fields,
and noncanonical bytes fail closed. Permutation of any array is
normalized before comparison.

## Consequences

- Adapters and target profiles (#27/#29) consume the binding and the
  vectors without owning identity; exact status defaults remain theirs.
- #23 can resolve the opaque scenario references without shape changes.
- #12 continues to own generic semantics; error-specific rules run only
  after that validation and never re-read source files.
- Every extension (new categories, richer effects, per-binding source
  anchors) requires a versioned successor of this contract family.

## References

- [docs/error-contracts.md](../error-contracts.md) — the surface,
  vocabularies, agreement table, and guarantees.
- [ADR-0010](0010-diagnostics.md) — the diagnostic contract and registry
  this family routes through.
- [ADR-0005](0005-semantic-ids.md) — the qualified id grammar the error
  ids reuse.
- [ADR-0007](0007-ir.md) — the typed IR the validation seam consumes.
