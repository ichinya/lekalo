# ADR-0024: First-class invariants and state transitions

Date: 2026-09-08
Status: accepted for issue #63

Custody: this issue carries the **prospective product candidate 0.1.31** in
every accepted path (workspace `Cargo.toml`, both `lekalo` packages in
`Cargo.lock` via `cargo update -w`, the regenerated committed golden lock
and its digest, the `--version` behavior and its pinning tests,
`README.md`, `docs/cli.md`); issue #26 published product 0.1.30
(annotated tag `v0.1.30` on `9020558`). The invariant-transition
contract version (`lekalo/invariant-transition/v1.0.0`, identity
`dev.lekalo.invariant-transition@1.0.0`) is independent of the product
release, of the Model/IR/Scenario contract versions, of the
error-contract, authorization, and expression families, and of the
diagnostic registry by design. The publication-order version may
differ at integration; the coordinator reconciles the custody number
before publication without touching this contract's identity.

## Context

The Model declares entities, fields, commands, and queries, but the
domain's real rules — which states an entity may occupy, which
invariants must hold, which state changes are legal — live only in
prose. Issue #63 makes them machine-checkable declarations, per the
recorded research decision: Model v0.1.0/v1.0.0 stay immutable, and no
runtime enforcement, adapter generation, SQL or method enforcement,
transaction runtime (#24), authorization runtime (#25), scenario
runner (#23), reference evaluator (#107), or report/trace surface is
introduced here.

## Decision

### 1. One closed, immutable wire contract

[`contracts/invariant-transition.schema.v1.0.0.json`](../../contracts/invariant-transition.schema.v1.0.0.json)
(discriminator `lekalo/invariant-transition/v1.0.0`, identity
`dev.lekalo.invariant-transition@1.0.0`) is a closed Draft 2020-12
document binding one project identity and one exact attachment
revision to one exact Model pin, one exact IR digest, and an optional
accepted source-map reference. Top-level members: `schemaVersion`,
`identity`, `attachmentRevision`, `projectId`, `modelRef`, `irRef`,
optional `sourceMapRef`, `stateSpaces`, `invariants`, `transitions`,
`verificationMappings`, optional `propertyHints`. No roots, native
paths, source bytes, runtime values, credentials, timestamps, host
data, URLs, snippets, provider output, or arbitrary JSON may enter
canonical bytes or diagnostics.

### 2. Bounded state spaces with explicit policies

Each state space binds one entity to a closed, explicitly declared
state set (1..256 states) where every state may declare `initial` and
`terminal`, at least one initial state is mandatory, and two owner
policies are explicit: `cyclePolicy` (`allow` records cycles as
facts, `forbid` makes any cycle including self-cycles invalid) and
`deadPolicy` (`forbid` makes a reachable nonterminal state without
outgoing transitions invalid). Identifier grammars reuse the accepted
#6 semantic and namespaced spellings; nothing new is invented except
the closed state identifier and the opaque #66 expression reference.

### 3. Eleven closed invariant kinds over a minimal predicate AST

`field_value`, `cross_field`, `uniqueness`, `cardinality`, `temporal`,
`aggregate_consistency`, `conditional_requirement`, `one_active`,
`member_of_set`, `immutable_after_state`, and `target_capability` —
each with a fixed coherence contract between the kind and its bound
members (checked semantically, refused closed). Predicates are a
minimal closed AST: fourteen finite operations (`equal`,
`not_equal`, `is_null`, `not_null`, `in_set`, `all`, `any`, `and`,
`or`, `before`, `after`, `within`, `count`, `member_of`) over closed
value nodes (null, boolean, bounded integer, bounded UTF-8 string,
canonical decimal, date, datetime, UUID, URI, enum member, typed
field/input/prior references, the deterministic clock reference
`now`, bounded lists and key-sorted objects). Depth is bounded at 8,
fanout at 16; no floats, no dynamic lookup, no loops, no recursion,
no reflection, no target snippets, no SQL, no network. Richer
conditions reference the future #66 typed-expression family by
opaque reference; no expression language is implemented inside #63.

### 4. Typed transitions with explicit separation of concerns

Each transition binds one state space, a closed canonically sorted
from-state set, one target state, one owning command, an ordered (or
explicitly parallel) assignment set over closed sources (bounded
literal, input field, prior-state field, `now`, or a #66 expression
reference), operation-entry preconditions, an optional #25
authorization policy reference, branch-specific #62 error references,
and requirement/scenario/capability references. Duplicate fields in
the assignment set, prior-state reads after the same-field write, and
transitions out of terminal states are invalid. Precondition
(operation-entry guard), invariant (condition holding for all
accepted states), and authorization (actor/scope/policy permission)
stay three separate concerns: a policy is never encoded as a value
predicate, an invariant never implies actor permission, and a failed
authorization is never reported as a domain precondition conflict.

### 5. Deterministic local state-graph checks

Edges are built from every declared from-state to its target with the
owning transition; identifiers sort by unsigned UTF-8. The checks are
local to the declared machine — distinct from the #13 generic
dependency graph, which may consume typed edges but never infers
state semantics. Validation covers unknown states, duplicate
transitions (same from-set, target, and command), missing initial
states, unreachable declared states, unreachable transitions, dead
nonterminal states under the explicit policy, and bounded iterative
SCC detection under the explicit cycle policy (forbidden self or
mutual cycles are invalid; allowed cycles are reported as facts).
No recursive traversal, no unbounded closure.

### 6. Verification mappings: declared layers and typed evidence only

Every mapping binds one subject (invariant or transition), one closed
relation, one typed target reference (closed target kind plus a
bounded opaque owner-namespaced identifier — never a file path, URL,
method signature, or SQL text), one declared enforcement layer
(validator, runtime_method, database_constraint, storage, native_test,
adapter, unknown), optional typed evidence (profile/adapter/protocol
references, exact executable digest, bound source revision,
scenario/test/gate reference, sanitized result status, opaque evidence
digest), and an explicit status (`full`, `verified`, `partial`,
`gap`, `dangling`, `stale`, `conflict`, `unsupported`, `unknown`) with
confidence. Status and evidence status stay distinct: `full` and
`verified` require current bound evidence at the exact attachment
revision, `gap` requires absence, `stale` requires a mismatch. No
enforcement claim is ever made from a declaration, method name, SQL
constraint name, adapter capability name, test existence, or equal
product version; runtime values, lock tokens, ETags, user data,
source snippets, logs, stacks, transcripts, and timestamps never
enter canonical bytes or public diagnostics. #22 owns trace manifests,
#103 owns reports; #63 emits typed traceability facts only.

### 7. Pure diff and typed contributions

`compare` is a pure semantic diff over same-family attachments with
deterministic byte-sorted paths and three closed classes:
**breaking** (a declared guarantee removed or weakened — narrowed
from-set, lost assignment, changed kind or predicate, removed
record), **non-breaking** (pure addition, description, precondition,
policy, or scenario-reference change), and **policy-change**
(verification mapping or property-hint change). Foreign projects and
mixed Model/IR/attachment revisions are the typed error set, never a
guessed classification. #12 consumes the attachment through the
single #11 DiagnosticSet; #13 receives typed relations after its
registry accepts them; #14/#24 receive exact typed refs and keep all
effect, transaction, and concurrency semantics; #16/#17/#18 consume
the diff and projection data; #23 receives scenario-anchored
assertions and bounded property hints. Node and PHP mappings preserve
identifiers and statuses; their equivalence belongs to the
conformance owners, not to core runtime code.

### 8. Diagnostics through the #11 seam: registry minor 1.8.0 → 1.9.0

The contract adds its own rule family — reusing `transaction.*` or
`contract.*` would blur contract families. The registry takes its
next wire-shape-preserving minor increment to
[`diagnostic-registry.v1.9.0.json`](../../contracts/diagnostic-registry.v1.9.0.json)
with `invariant.input-invalid`, `invariant.contract-invalid`,
`invariant.state-invalid`, `invariant.transition-invalid`,
`invariant.mapping-invalid`, `invariant.graph-invalid`, and
`invariant.export-limit` (LEK-INV-001..007, category `semantic`),
additions only, zero mutations of the 200 published entries, strictly
id-sorted, assembled through the shared registry-backed constructor
with bounded fixed tokens only. Every bound rejects with no partial
result. The validation profiles and their schema pin the registry
version and move with it.

### 9. Bounds

256 state spaces, 256 states per space, 10,000 invariants,
10,000 transitions, 10,000 verification mappings, 256 property
hints, predicate depth 8, logical fanout 16, 64 value-list items,
32 object entries, 256 literal bytes, 64 fields per invariant,
64 from-states, 64 assignments, 16 preconditions, 64 error refs,
32 requirement/scenario/capability refs, 256 allowed states,
1,000,000 cardinality ceiling, ±9,007,199,254,740,991 integer
literals, 32 MiB canonical payload. Rejection, never truncation.

## Consequences

Issue #63 closes the last M1 semantics gap declared by the milestone:
invariants and state transitions become first-class, machine-checkable
contract data with stable identifiers, deterministic local graph
checks, explicit evidence-bound traceability, and typed contributions
to the graph, diff, impact, context, scenario, and error owners.
Enforcement itself deliberately remains with the targets and their
owners; this contract only ever carries declarations and evidence
about them.
