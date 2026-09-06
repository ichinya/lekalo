# ADR-0020: Transaction and concurrency contracts

Date: 2026-09-06
Status: accepted for issue #24

Custody: this issue carried the **prospective product candidate 0.1.27** in
every accepted path (workspace `Cargo.toml`, both `lekalo` packages in
`Cargo.lock`, the regenerated committed golden lock and its digest, the
`--version` behavior and its pinning tests, `README.md`, `docs/cli.md`);
the candidate was published as product 0.1.27 (annotated tag `v0.1.27` on
`ef7680d`); issue #18 published product 0.1.26 (annotated tag `v0.1.26` on
`3710179`).
The transaction-concurrency contract version
(`lekalo/transaction-concurrency/v1.0.0`, identity
`dev.lekalo.transaction-concurrency@1.0.0`) is independent of the product
release, of the Model/IR/effect/Scenario contract versions, of the
error-contract family, and of the diagnostic registry by design. The
publication-order version may differ at integration; the coordinator
reconciles the custody number before publication without touching this
contract's identity.

## Context

Issues #13/#14 exposed semantic symbols and effects; #23 made behavioral
scenarios portable. What the Model still lacked was an explicit, portable
statement of **how effects commit under failure and concurrency**:
transaction semantics, atomicity, optimistic and pessimistic control,
idempotency, retry, partial failure, and compensation. Issue #24 owns that
surface as one independent closed attachment, per the recorded research
decision: Model v0.1.0 stays immutable, and no source syntax, runtime
execution, adapter, or report surface is introduced here.

The research briefs (run run_088695f63032: worker_done msg_d1571b168e39
and briefs msg_73d36d84de8b, msg_1a6a9cf39d01, msg_11fc73051cf2,
msg_ec06240d2fd6) recorded the owner decisions this ADR adopts.

## Decision

### 1. One closed, immutable wire contract

[`contracts/transaction-concurrency.schema.v1.0.0.json`](../../contracts/transaction-concurrency.schema.v1.0.0.json)
(discriminator `lekalo/transaction-concurrency/v1.0.0`, identity
`dev.lekalo.transaction-concurrency@1.0.0`) is a closed Draft 2020-12
document binding one project identity to one exact Model pin, one exact
IR digest, and an optional effect-graph digest. Everything is typed
qualified references and bounded enums; source text, physical paths,
literal tokens, ETag values, credentials, runtime values, timestamps, and
host data are refused by construction. The attachment is immutable after
publication: additive optional fields require a reviewed minor successor;
removal, retyping, tightening, or new closed-enum meanings require a
major successor; no automatic migration exists.

### 2. Required, optional, and forbidden are exactly what they say

`transaction: required` promises that every declared atomic group commits
all-or-nothing inside one local transactional boundary — an error or
abort exposes zero committed effects from that group — and never claims
that an external call is atomic. `optional` promises nothing: effects are
potentially partial and evidence must state whether an actual run was
atomic; the contract never upgrades `optional` to a guarantee because a
target happens to support transactions. `forbidden` refuses every
declared boundary or group; non-transactional, autocommit, and external
effects stay independently observable. No silent upgrade of `optional` to
`required`, or `forbidden` to `required`, exists.

### 3. Atomic effect groups are local and closed

`AtomicEffectGroup` carries the closed v1 values `scope: local`,
`atomicity: all_or_nothing`, `failurePolicy: abort`; `commitBoundary`
records declared sequencing relative to external effects without
claiming transactional external calls. Membership references are exact
#14 EffectId canonical spellings; membership order is semantic and is
preserved. Every effect belongs to at most one group, group references
must name the owning operation, and external kinds (external calls, jobs,
published outputs, cache side effects) may never sit inside a local
group: the distributed-protocol exception needs a protocol owner and
proof that v1 does not have, so the rejection is unconditional
(recorded owner decision; a reviewed successor may relax it). Event
emission stays group-legal — the transactional-outbox shape — and audit
entries stay local by definition.

### 4. Optimistic and pessimistic control as a closed precondition union

Optimistic preconditions are `version` compare-and-swap and `etag`
If-Match: always `compare: exact`, always `check: at_commit`, always
participating in the required local commit; no blind read-then-write and
no automatic retry may claim the guarantee. The ETag reference names
where the opaque validator token is carried (header, metadata, input
field) — never the literal token bytes, which never enter canonical
output. Pessimistic locks carry `entity|key|range` scope, `shared|
exclusive` mode, `before_read|before_write` acquisition, a bounded
deterministic `orderKey` for global acquisition ordering (duplicate
order keys are invalid), and a closed timeout policy. Locks never
substitute for a stronger isolation guarantee.

### 5. Isolation is a declared relation, not a lexical order

The closed v1 levels are `none`, `read_committed`, `repeatable_read`,
`snapshot`, and `serializable`. The owner-published relation table
declares `none < read_committed < repeatable_read < serializable` on the
committed-read chain and exactly one snapshot fact: snapshot satisfies
snapshot. Snapshot versus every other level is **undeclared** — `unknown`
— and unknown never passes a strict requirement. Compatibility answers
come from `IsolationLevel::satisfies(required)` as `Some(true)`,
`Some(false)`, or `None` (unknown); no lexical string comparison exists
anywhere.

### 6. Idempotency and retry are separate declarations

`idempotency` declares the mode (`not_applicable|not_guaranteed|
intrinsic|key_required|key_optional`), the exact typed request-key field
(mandatory for both key modes, forbidden otherwise), the record
durability (`key_required` forces `durable`; `not_applicable` and
`not_guaranteed` force `none`), and the duplicate policy (`replay_result|
reject_same_key|join_in_flight`). A replay with the same key and the same
canonical request digest returns the prior result and creates no new
effects; the same key with a different digest is the typed conflict
(`sameKeyDifferentRequest` is the fixed value `conflict`); key records
and runtime payloads never enter canonical bytes. `retry` declares
`safety|condition|phases` independently: `safe` forces `condition: none`,
`conditional` forces `idempotency_key` or `reconciliation`, `unsafe`
forces `manual_only`; `after_commit_unknown` is a first-class phase where
blind replay is forbidden. Retry safety is never inferred from
idempotency; contradictions reject through `concurrency.retry-conflict`.

### 7. Partial failure is visible; compensation is a declaration

`FailureBoundary` partitions `atomic_group | non_atomic_effect |
compensation`. Every external effect referenced by a `required` or
`optional` operation must carry a `non_atomic_effect` boundary with a
typed `compensationRef` to an idempotent compensating operation — an
external effect whose failure can survive local rollback without one is
rejected (`transaction.partial-unacknowledged`). Compensation boundaries
always declare the `recovery_required` outcome: a compensating action is
a recovery declaration, never proof of atomicity, and it may produce
`recovery_required` — never a silently reported original success. Partial
success is a first-class degraded result and is never masked as atomic
success.

### 8. Capability requirements; strict profiles block, permissive degrade

Typed `CapabilityRequirement` records name closed capability ids
(`transaction.atomic_group`, `transaction.rollback`, `isolation.<level>`,
`lock.shared|lock.exclusive|lock.key|lock.range`,
`concurrency.compare_and_set`, `concurrency.etag_if_match`,
`invariant.unique_concurrent`, `idempotency.durable_key`,
`idempotency.replay`, `external.compensation`) or a namespaced
distributed-protocol capability whose concrete owners approve it later.
Declared guarantees must reference their capability records: a required
transaction needs atomic-group and rollback, preconditions need
compare-and-swap or If-Match, locks need their mode (and key/range
scope), a non-`none` isolation requirement needs its level,
`key_required` needs durable-key and replay, compensation references need
`external.compensation` — a missing record rejects through
`concurrency.capability-missing`. The pure mapping
(`map_capabilities`) answers requirements against a caller-supplied
snapshot under two closed profiles: **strict** blocks on `unsupported`,
`unknown`, and unapproved `partial`; **permissive** degrades explicitly
and never passes. `unknown` is never yes. Discovery, negotiation, and
profile resolution stay with #27/#28/#29.

### 9. Race scenarios as typed data keyed to Scenario IR

`ConcurrencyCase` binds a stable scenario id, its explicit contract
version, and the digest of the exact canonical IR payload. Participants
and invocations reference existing #23 `when` steps; the schedule is an
explicit deterministic partial order (join sets, acyclicity-validated)
with named barriers over two or more nodes; expected outcomes are the
closed enum `success|conflict|error|infrastructure|unsupported|degraded|
recovery_required`, one per participant. The committed planner fixture
(`planner-focus-race.json`) encodes the issue's use case: two concurrent
`focus_task` invocations for different task ids and the same user must
leave at most one active focus — one success, one typed conflict — while
the companion case proves different users proceed independently. A serial
execution of two invokes never satisfies a race fixture; execution
belongs to the owner harnesses (#47/#56/#107/#31), and an unsupported
backend reports `unsupported`/`infrastructure`, never pass.

### 10. Typed contributions to #14 and #23 as data

The attachment contributes a `TransactionGroupMembership` projection
(group id, operation, exact EffectId) as plain typed data #14 may
display; it never creates graph nodes, mutates graphs, infers effects,
or owns conflicts. `validate_against_graph` cross-checks the attachment
against a supplied `EffectGraph`: every group reference must resolve to a
declared edge of its operation, and every external declared edge of an
attached operation must carry its compensating boundary. Scenario
identity, reachability, values, and evidence stay with #23; error
identities stay with #62 (only opaque typed `errors.*/name` references
are carried); #24 allocates no ErrorId, no ErrorCode, and no second
diagnostic envelope.

### 11. Diagnostics through the #11 seam: registry minor 1.5.0 → 1.6.0

The contract adds its own rule family — reusing `graph.*` would blur
contract families. The registry takes its next wire-shape-preserving
minor increment to
[`diagnostic-registry.v1.6.0.json`](../../contracts/diagnostic-registry.v1.6.0.json)
with `transaction.input-invalid`, `transaction.group-missing`,
`transaction.group-overlap`, `transaction.external-atomic`,
`transaction.partial-unacknowledged`, `transaction.export-limit`,
`concurrency.precondition-invalid`, `concurrency.invariant-invalid`,
`concurrency.case-invalid`, `concurrency.capability-missing`, and
`concurrency.retry-conflict` (LEK-TC-001..011, category `compatibility`),
additions only, zero mutations of the 154 published entries, strictly
id-sorted, assembled through the shared registry-backed constructor with
bounded fixed tokens only. Every bound rejects with no partial result.

### 12. Determinism, canonical form, and bounds (v1, owner-approved)

Canonical bytes are compact UTF-8 JSON with byte-sorted keys, no BOM, no
trailing LF. Set-like collections (operations, invariants, cases,
requirement records, groups, key fields, capability references, expected
invariants and outcomes, join sets, barrier waits, precondition records)
normalize to unsigned-UTF-8-byte order; semantically ordered collections
(group membership, failure boundaries) keep declared order; schedules
keep their edges while nodes order canonically. Owner-approved v1
bounds: 10,000 operations, 1,024 groups, 256 effects per group, 256
preconditions per operation, 512 failure boundaries per operation, 256
invariants, 512 cases, 64 participants per case, 1,024 schedule nodes,
128 barriers, 128 lock resources, 64 key fields, 256 requirement
records, 32 capability references per operation or case, and a 32 MiB
canonical payload. Every bound rejects before allocation where possible
and never truncates by arrival order. The v1 predicate vocabulary for
unique invariants is exactly `field_not_null` and `field_equals` with a
closed typed literal — never an expression language, never arbitrary
code, never target constraint names.

### 13. No runtime, no adapter, no source home

This issue implements pure contract, normalization, validation, canonical
form, diff, capability mapping, and the graph cross-check. It does not
execute transactions, call adapters, discover effects, resolve profiles,
persist evidence, or write reports. A source attachment home (for example
`lekalo/attachments/transaction-concurrency.yaml`) would change the #4
canonical structure and the #7 loader; until those reviewed successor
seams exist the attachment is an external typed document accepted through
the typed API, not a canonical `lekalo/**` input.

### 14. The semantic diff and its classes

`diff::compare` answers two same-family attachments with deterministic
sorted paths and three classes. **Breaking**: demoting `required`, group
or coverage removal under `required`, isolation or lock or CAS/ETag or
invariant or idempotency/replay or compensation weakening, precondition
removal, conflict-behavior changes. **Non-breaking**: additions of
operations, groups, preconditions, boundaries, cases; `forbidden →
optional/required` (a guarantee is added, none removed); isolation
strengthening declared by the relation table; enforcement-mechanism
changes. **Policy-change**: capability-requirement record and reference
changes, which map targets to policy without changing contract
guarantees. `invalid` inputs (foreign identity, mixed Model/IR/effect
revisions) are the typed error set, never a guessed classification. A
changed schema or contract version alone is never proof of semantic
equality or compatibility.

## Consequences

- #25 authorization, #26 planner semantics, and #62 error contracts can
  reference stable typed transaction, precondition, invariant, and case
  data instead of prose; #62 later unifies the opaque error references.
- #27/#28/#29 own the capability registry, negotiation, and profile
  resolution that consume `CapabilityRequirement` records and snapshots;
  the strict/permissive decision table is normative for them.
- #31/#47/#56/#107 consume the race vectors through their accepted
  seams; the vectors here stay portable data with closed outcomes.
- The pinned goldens under `tests/fixtures/transaction-concurrency` are
  gate-checked with exact Ajv 8.17.1 on Node 18 and 24 by
  `scripts/test-transaction-concurrency-contracts.mjs` (additive CI line,
  pinned-Ajv step) and byte-compared by the Rust fixture suite.

## References

- [docs/transaction-concurrency.md](../transaction-concurrency.md) — the
  attachment surface, vocabularies, limits, and guarantees.
- [ADR-0013](0013-effect-graph.md) — the effect graph whose read-only
  discipline and EffectId spellings this attachment consumes.
- [ADR-0014 (Scenario IR)](0014-scenario-ir.md) — the Scenario IR the
  race cases key into.
- [ADR-0010](0010-diagnostics.md) — the diagnostic contract and registry.
- [ADR-0019](0019-semantic-diff.md) — the semantic-diff family whose
  classification discipline the attachment diff follows.
