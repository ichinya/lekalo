# Lekalo invariants and state transitions

Issue #63 makes domain invariants and state transitions first-class,
machine-checkable contract data. One closed, versioned attachment —
[`contracts/invariant-transition.schema.v1.0.0.json`](../contracts/invariant-transition.schema.v1.0.0.json)
(`lekalo/invariant-transition/v1.0.0`, identity
`dev.lekalo.invariant-transition@1.0.0`) — binds one project to one
exact Model pin, IR digest, and attachment revision, and declares
bounded state spaces, eleven closed invariant kinds, typed
transitions, verification mappings with declared enforcement layers
and typed evidence, and bounded property hints. See
[ADR-0024](adr/0024-invariant-transition.md) for the owner decisions.

The attachment is pure declaration and validation data: it never
executes, never claims runtime enforcement without target evidence,
and never carries source text, physical paths, raw tokens,
credentials, runtime values, timestamps, host data, or provider
transcripts. Contract identity is independent of the product release,
of the Model/IR/Scenario versions, of the error-contract and
authorization families, and of the diagnostic registry.

## State spaces

A state space binds one entity to an explicitly declared closed state
set. Every state may declare `initial` and `terminal`; at least one
initial state is mandatory. Two policies are explicit per space:

- `cyclePolicy: allow | forbid` — `allow` records cycles as facts;
  `forbid` makes any cycle, including self-cycles, invalid.
- `deadPolicy: allow | forbid` — `forbid` makes a reachable
  nonterminal state without outgoing transitions invalid.

The deterministic local graph checks cover unknown states, duplicate
transitions (same from-set, target, and command), missing initial
states, unreachable declared states, unreachable transitions, and the
dead-state policy. Detection is bounded and iterative; identifier
order is unsigned UTF-8 everywhere.

## Invariants

Eleven closed kinds, each with a fixed coherence contract:

| Kind | Binds | Refuses |
| --- | --- | --- |
| `field_value` | one field + predicate | missing predicate |
| `cross_field` | one entity + ≥2 fields + predicate | single field |
| `uniqueness` | key fields + typed conflict error ref | missing error ref |
| `cardinality` | bounded `min`/`max` range | missing bounds |
| `temporal` | fields/clock + ordering predicate | missing predicate |
| `aggregate_consistency` | aggregate ref + relationship predicate | missing ref |
| `conditional_requirement` | guard predicate + required field set | missing guard or fields |
| `one_active` | partition key + active predicate + `maxActive: 1` | missing partition |
| `member_of_set` | closed allowed state set | empty or unknown states |
| `immutable_after_state` | trigger state + immutable field set | missing trigger |
| `target_capability` | capability record with minimum guarantee | missing record |

Predicates are a minimal closed AST — fourteen finite operations over
closed value nodes with depth 8 and fanout 16. No floats, no dynamic
lookup, no loops, no recursion, no target snippets, no SQL, no
network. Richer conditions reference the future #66 typed-expression
family by opaque reference.

## Transitions

A transition declares one legal state change: the closed from-state
set (canonically sorted), the target state, the owning command, the
ordered (or explicitly parallel) assignment set, preconditions, an
optional authorization policy reference, and branch-specific error
references. Assignments write each field at most once from closed
sources (bounded literal, input field, prior-state field, the
deterministic clock `now`, or a #66 expression reference). Duplicate
writes, prior reads after the same-field write, and transitions out
of terminal states are invalid.

Precondition, invariant, and authorization stay three separate
concerns: a policy is never a value predicate, an invariant never
implies actor permission, and a failed authorization is never a
domain precondition conflict.

## Verification mappings and evidence

Every mapping binds a subject to a closed relation, a typed target
reference (closed target kind plus a bounded opaque identifier in the
owner's namespace), a declared enforcement layer, optional typed
evidence, an explicit status, and confidence. Status and evidence
stay distinct: `full`/`verified` require current bound evidence at
the exact attachment revision, `gap` requires absence, `stale`
requires a mismatch; `dangling`, `conflict`, `unsupported`, and
`unknown` are explicit degraded states. A pass in evidence is
evidence, never an enforcement claim; no claim is ever made from a
declaration, method name, SQL constraint name, adapter capability
name, test existence, or equal product version. #22 owns trace
manifests and #103 owns reports; this contract emits typed
traceability facts only.

## Diagnostics

Failures emit the accepted #11 diagnostic contract with the registry
minor 1.8.0 → 1.9.0 (additions only):
`invariant.input-invalid` (LEK-INV-001),
`invariant.contract-invalid` (LEK-INV-007),
`invariant.state-invalid` (LEK-INV-002),
`invariant.transition-invalid` (LEK-INV-003),
`invariant.mapping-invalid` (LEK-INV-004),
`invariant.graph-invalid` (LEK-INV-005), and
`invariant.export-limit` (LEK-INV-006), category `semantic`, with
bounded fixed tokens only. See
[docs/diagnostics.md](diagnostics.md).

## Diff

`lekalo_core::invariant_transition::compare` is a pure semantic diff
over same-family attachments: **breaking** (a declared guarantee
removed or weakened), **non-breaking** (pure addition, description,
precondition, policy, or scenario-reference change), and
**policy-change** (verification mapping or property-hint change).
Foreign projects and mixed Model/IR/attachment revisions are the
typed error set, never a guessed classification.

## Boundaries

No runtime enforcement, adapter generation, SQL or method
enforcement, transaction semantics (#24), authorization execution
(#25), scenario execution (#23), reference evaluation (#107), or
report/trace surface belongs here. Error identity stays with #62
(opaque refs only), capability registries and profile resolution
stay with #27/#28/#29. Tests are hermetic; fixtures live under
[`tests/fixtures/invariant-transition/`](../tests/fixtures/invariant-transition/).
