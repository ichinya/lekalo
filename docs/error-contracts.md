# Error contracts and typed results

Issue #62 makes operation errors a formal, versioned part of the
behavior contract. The error-contract family (`lekalo/error-contract/
v1.0.0`, `dev.lekalo.error-contract@1.0.0`) and the error-registry
family (`lekalo/error-registry/v1.0.0`, `dev.lekalo.error-registry@
1.0.0`) are independent of the product release and of every other
contract family. The contracts are published as:

- [`contracts/error-contract.schema.v1.0.0.json`](../contracts/error-contract.schema.v1.0.0.json) — one typed operation binding,
- [`contracts/error-registry.schema.v1.0.0.json`](../contracts/error-registry.schema.v1.0.0.json) — the registry envelope,
- [`contracts/error-registry.v1.0.0.json`](../contracts/error-registry.v1.0.0.json) — the canonical registry instance (planner seed).

Owner decisions and the rationale live in
[ADR-0022](adr/0022-error-contracts.md).

## The typed Result binding

Each command or query may declare its errors as one closed binding:

```json
{
  "schema_version": "lekalo/error-contract/v1.0.0",
  "operation": "planner.focus_task",
  "kind": "command",
  "output": null,
  "errors": [
    "planner.focus_conflict",
    "planner.focus_denied",
    "planner.input_invalid",
    "planner.store_unavailable",
    "planner.task_not_found"
  ]
}
```

`output` is the `Result` success side (`null` for the command unit, the
Model type expression for a query); `errors` is the `ErrorUnion`: closed,
non-empty, unique, sorted by unsigned UTF-8 bytes. There is no catch-all.

## Error identity and metadata

Every declared error carries exactly:

- `id` — the stable semantic id (qualified #6 grammar); `code` — the
  immutable `LEK-ERR-NNN` machine code. Both are unique forever; retired
  codes stay tombstoned and are never reused.
- `category` — exactly `validation`, `auth`, `conflict`, `not-found`,
  `domain`, or `infrastructure`. Declared classification, never inferred.
- `payload` — closed named-field schema over the Model type grammar
  (depth ≤ 4, ≤ 64 fields), each field `required`/optional and
  `public`/`private`.
- `messages` — one public template id and an optional private template
  id, each listing the payload fields it may render. Public templates
  reference only public fields; private fields never cross the boundary.
- `retry` — `never`, `safe`, or `conditional` with the bounded condition
  `idempotency-key` or `reconciliation`.
- `idempotency` — `guaranteed`, `key-required`, `not-guaranteed`, or
  `not-applicable`.
- `effect` — `none`, `read`, `write`, `destructive`, or `external`.
- `observability` — `info`, `warning`, `error`, `critical` (independent
  of diagnostics and of process exits).
- `coverage` — scenario refs, test refs, or an explicit waiver
  (reference, owner, bounded reason, optional expiry).
- `source` — the logical path plus a 0-based half-open byte range with
  1-based line/column, and the declared `invariant` text.

### Retry / idempotency / effect agreement

| effect | idempotency | retry |
| --- | --- | --- |
| `none`, `read` | `not-applicable` | `never` or `safe` only |
| `write` | `guaranteed` | `safe` allowed |
| `write` | `key-required` | `conditional` allowed |
| `destructive` | never `guaranteed` | never `safe` |
| `external` | any | `safe` only when `guaranteed`; key retries need `key-required` |

Contradictions refuse at construction with
`error.retry-idempotency-conflict`.

## Reachability

Every declared error is reachable in one of exactly three ways:

1. referenced by at least one operation union;
2. covered by at least one scenario reference;
3. explicitly waived (owner, reason, optional expiry).

Test-only coverage without a binding is `error.unreachable`. Scenario
and test references are opaque; the scenario owner resolves them later.

## Validation and diagnostics

`lekalo_core::error_contract::validate` runs the error-specific rules
over an accepted, compiled project: binding operations must exist with
the declared kind and output type, union members must resolve, payload
leaves must resolve to declared scalars or enums, and every error must
be reachable. Generic Model/IR semantics stay with semantic validation;
nothing here re-reads source files. Findings are registered `error.*`
rules (LEK-ERR-001..012, registry minor 1.6.0 → 1.7.0, additions-only)
rendered through the shared #11 envelope. A diagnostic id is never an
error id; `reasonCodes` stay derived diagnostic ids.

## Unknown infrastructure is a separate channel

Timeouts, crashes, invalid processes, unknown IO, and provider failures
are `InfrastructureFailure` values (`timeout`, `crash`,
`invalid-process`, `io`, `provider`) on their own outcome channel. They
never coerce into a declared `validation`/`auth`/`conflict`/
`not-found`/`domain` error, never use a declared code or payload, and
project only a fixed generic summary — the declared
`planner.store_unavailable` contract, by contrast, is a named dependency
with its own metadata.

## Mappings are projections

The language-neutral vectors keep the canonical quadruple
`{id, code, category, public payload}` intact:

- Node: `{"ok":false,"error":{id, code, category, payload}}`
- PHP: a typed result whose fields expose the same quadruple
- Go: `(T, error)` with typed `ID/Code/Category/Payload` accessors

Exception classes, wrapper text, framework names, localized messages,
and HTTP statuses are projections, never identity. The core contract
carries no status; a mapping entry keyed on status alone is refused, a
catch-all mapping is refused under the strict profile
(`error.mapping-invalid`), and a strict mapping that misses a union
member reports `error.mapping-missing`.

## Semantic diff and breaking rules

`diff` compares two validated registries and emits changes in fixed
unsigned-byte path order, each classified:

- **non-breaking** — adding an error or binding, tombstones, private
  message changes, observability, source anchors;
- **breaking** — removing an error, binding, union member, or payload
  field; id, code, category, type, exposure, or requiredness changes;
  public message changes; tightened retry; weakened idempotency;
  strengthened effects; output shape changes;
- **policy-change** — optional payload additions, loosened retry or
  strengthened idempotency, coverage, waivers, invariants;
- **invalid** — a code reused by a different error.

Compatibility is never inferred from numeric versions, statuses, or
exception names.

## Bounds and determinism

10000 errors/bindings/tombstones, 256 union members, 64 payload
fields/values, 64 coverage refs, 2000-byte text values, 256-byte tokens,
32 MiB canonical result. Canonical bytes are compact UTF-8 JSON with
byte-sorted keys and no final LF; the embedded registry must equal its
canonical re-rendering, so duplicate keys and noncanonical bytes fail
closed. Permutation of any array is normalized before comparison.

## Testing

The pinned Ajv 8.17.1 gate (`scripts/test-error-contracts.mjs`,
Node 18 and 24) validates both schemas, the registry instance, the
golden bindings, and the invalid matrix with the schema-inexpressible
invariants. The Rust integration suite
(`crates/lekalo-core/tests/error_contract.rs`) byte-compares the
embedded registry, asserts the exact refusal rule per invalid fixture,
covers the validation seam, the outcome channels, the diff classes, and
the projection vectors. No transport runs and no adapter executes
anywhere in this surface.
