# Transaction and concurrency contracts (issue #24)

One independent, closed, versioned attachment —
`lekalo/transaction-concurrency/v1.0.0`, identity
`dev.lekalo.transaction-concurrency@1.0.0` — declaring how a project's
effects commit under failure and concurrency: transaction semantics,
atomic effect groups, optimistic and pessimistic control, isolation
requirements, unique invariants, idempotency, retry safety, partial
failure boundaries, compensation references, capability requirements,
and deterministic race cases keyed to Scenario IR.

The contract version is independent of the product release, of the Model
and IR contract versions, of the effect graph, of the Scenario IR, of the
error-contract family, and of the diagnostic registry. The attachment is
pure declaration and validation data: nothing executes, nothing writes,
and nothing claims runtime enforcement. Owner decisions are recorded in
[ADR-0020](adr/0020-transaction-concurrency.md).

## Document shape

Every attachment binds one `projectId` to one exact Model pin
(`modelVersion` + digest), one exact IR digest, and — when effect-graph
cross-validation is used — one exact effect-graph digest. The closed
top-level members are `schemaVersion`, `identity`, `projectId`,
`modelRef`, `irRef`, `effectGraphRef` (optional), `operations`,
`invariants`, `concurrencyCases`, and `capabilityRequirements`. Unknown
fields, duplicate identities, malformed references, and bound violations
reject before any semantic processing; canonical bytes are compact UTF-8
JSON with byte-sorted keys and no trailing LF.

## Operation contracts

Each operation carries:

- `operationRef` — the typed qualified id `operation:<semantic-id>`;
- `operationVersion` — the source semantic version, not a contract
  version;
- `transaction` — `required`, `optional`, or `forbidden`:

  - `required` — every declared atomic group commits all-or-nothing in
    one local boundary; a failure exposes zero committed effects from the
    group. Never a claim about external calls.
  - `optional` — no guarantee; effects are potentially partial and
    evidence must state what actually happened. Never upgraded to a
    guarantee.
  - `forbidden` — no boundary or group may be declared; effects stay
    independently observable.

- `isolationRequirement` — `none`, `read_committed`, `repeatable_read`,
  `snapshot`, or `serializable`;
- `atomicEffectGroups` — local all-or-nothing groups (mandatory under
  `required`, prohibited under `forbidden`, descriptive under
  `optional`);
- `preconditions` — the closed union `version` (compare-and-swap),
  `etag` (If-Match), `lock` (pessimistic). Optimistic checks are always
  `compare: exact` and `check: at_commit`; the ETag reference names where
  the opaque token is carried and never contains the token itself;
- `failureBoundaries` — `atomic_group`, `non_atomic_effect`, and
  `compensation` entries making partial failure visible; external effects
  carry a typed `compensationRef`, and compensation boundaries always
  declare `recovery_required`;
- `idempotency` — mode, key field, scope, record durability, duplicate
  policy (`replay_result`, `reject_same_key`, `join_in_flight`); a same-key
  different-request replay is always the typed conflict;
- `retry` — `safe | conditional | unsafe` with the exact required
  condition (`none`, `idempotency_key`, `reconciliation`, `manual_only`)
  and the closed phases `before_commit`, `after_commit_unknown`
  (blind replay is forbidden), `external_partial`;
- `capabilityRequirements` — references to the attachment's capability
  records.

Idempotency and retry are independent declarations. Retry safety is
never inferred from idempotency, and contradictions reject through
`concurrency.retry-conflict`.

## External effects and compensation

External calls, jobs, published outputs, and cache side effects can
never sit inside a local atomic group. Under `required` or `optional`
they must carry a `non_atomic_effect` failure boundary with a typed
compensation reference; a compensating action is a recovery declaration
with the `recovery_required` outcome — never proof of atomicity, never a
silently reported success. Partial success is a first-class degraded
result.

## Isolation

The owner-published relation: `none < read_committed <
repeatable_read < serializable`. `snapshot` satisfies only itself; every
snapshot relation with the committed-read chain is undeclared — unknown —
and unknown never satisfies a strict requirement. There is no lexical
ordering anywhere.

## Unique invariants

The closed v1 form: one namespaced invariant id, one resource selector,
sorted key fields (at most 64), the finite predicate `field_not_null` or
`field_equals` with a closed typed literal, an opaque typed violation
reference, and the enforcement mechanism (`optimistic`, `pessimistic`,
`database_constraint`, `either`). No expression language, no arbitrary
code, no target constraint names.

## Capability requirements and target profiles

Capability records name closed ids — `transaction.atomic_group`,
`transaction.rollback`, `isolation.<level>`, `lock.shared`,
`lock.exclusive`, `lock.key`, `lock.range`,
`concurrency.compare_and_set`, `concurrency.etag_if_match`,
`invariant.unique_concurrent`, `idempotency.durable_key`,
`idempotency.replay`, `external.compensation` — or an approved
`distributed.*` protocol capability. Declared guarantees must reference
their records. The pure mapping answers requirements against a resolved
snapshot under two profiles:

- **strict** — blocks on `unsupported`, `unknown`, and unapproved
  `partial`;
- **permissive** — degrades explicitly, never passes on `unsupported` or
  `unknown`.

`unknown` is never yes. Discovery, negotiation, and profile resolution
belong to #27/#28/#29.

## Concurrency cases

A case binds one Scenario IR document (stable id, explicit contract
version, exact IR digest), racing participants and invocations that
reference existing `when` steps, a deterministic partial-order schedule
with acyclicity validation, named barriers over at least two nodes, the
invariants the committed state must satisfy, one closed expected outcome
per participant (`success`, `conflict`, `error`, `infrastructure`,
`unsupported`, `degraded`, `recovery_required`), and capability
references. A serial execution of two invokes is never race evidence;
execution and evidence belong to the owner harnesses (#47/#56/#107/#31).

## Effect-graph and Scenario contributions

The attachment contributes group membership as plain typed data keyed by
exact #14 EffectIds; it never mutates any graph. The graph cross-check
proves every group reference resolves to a declared edge of its
operation and every external declared edge carries its compensation.
Error references are opaque typed pointers; #62 owns error identity.

## Diagnostics

Failures emit the accepted #11 diagnostic contract with the registry
minor 1.5.0 → 1.6.0 (additions only): LEK-TC-001
`transaction.input-invalid`, LEK-TC-002 `transaction.group-missing`,
LEK-TC-003 `transaction.group-overlap`, LEK-TC-004
`transaction.external-atomic`, LEK-TC-005
`transaction.partial-unacknowledged`, LEK-TC-006
`concurrency.precondition-invalid`, LEK-TC-007
`concurrency.invariant-invalid`, LEK-TC-008 `concurrency.case-invalid`,
LEK-TC-009 `concurrency.capability-missing`, LEK-TC-010
`concurrency.retry-conflict`, LEK-TC-011 `transaction.export-limit`.
Every echo is a bounded fixed token; no attacker-controlled text enters
a diagnostic.

## Bounds (v1, owner-approved)

10,000 operations; 1,024 groups; 256 effects per group; 256
preconditions and 512 failure boundaries per operation; 256 invariants;
512 cases; 64 participants, invocations, and outcomes per case; 1,024
schedule nodes and 128 barriers per case; 128 distinct lock resources;
64 key fields; 256 requirement records; 32 capability references per
operation or case; 32 MiB canonical payload. Rejection, never
truncation.

## Semantic diff

Comparing two same-family attachments yields sorted paths and closed
classes: `breaking` (weakened or removed required guarantees), `non-breaking`
(additions, `forbidden → optional/required`, declared isolation
strengthening, enforcement-mechanism changes), and `policy-change`
(capability mapping). Mixed revisions or foreign identities are the
typed error set — never a guessed classification.

## Boundaries

No runtime transaction execution, no adapter, no effect discovery, no
profile resolution, no report persistence, no source attachment home. The
attachment is an external typed document accepted through the typed API;
a canonical `lekalo/**` source home requires reviewed #4/#7 successors.
