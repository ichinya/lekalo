# Extended effect contracts (issue #26)

One independent, closed, versioned attachment —
`lekalo/extended-effects/v1.0.0`, identity
`dev.lekalo.extended-effects@1.0.0` — declaring the non-database effect
semantics of a project: event contracts (schema and version, local and
durable delivery, ordering and deduplication, correlation and causation
fields), job contracts (payload, queue class, retry and backoff,
idempotency, timeout and dead-letter policy), external-call contracts
(provider capability, request/response/error contract, timeout and
retry, read/write/destructive classification, compensation), cache
contracts (key contract, read/write/invalidate operations, consistency
and freshness), and publication contracts (destination, explicit opt-in
and approval, sensitivity classification, immutable revisioned
snapshots).

The contract version is independent of the product release, of the Model
and IR contract versions, of the effect graph, of the Scenario IR, of
the error-contract family, and of the diagnostic registry. The
attachment is pure declaration and validation data: nothing executes,
nothing writes, and nothing claims runtime enforcement. Owner decisions
are recorded in [ADR-0023](adr/0023-extended-effects.md).

## Document shape

Every attachment binds one `projectId` to one exact Model pin
(`modelVersion` + digest), one exact IR digest, and — when effect-graph
cross-validation is used — one exact effect-graph digest. The closed
top-level members are `schemaVersion`, `identity`, `projectId`,
`modelRef`, `irRef`, `effectGraphRef` (optional), `events`, `jobs`,
`calls`, `cacheContracts`, `publications`, `partialFailureCases`, and
`capabilityRequirements`. Unknown fields, duplicate identities,
malformed references, and bound violations reject before any semantic
processing; canonical bytes are compact UTF-8 JSON with byte-sorted keys
and no trailing LF.

## Common contract members

Every contract kind carries one closed common member set:

- `contractRef` — the namespaced contract id (`planner.event/focused`);
- `contractVersion` — the source semantic version, not a contract
  version;
- `effectRef` — the exact #14 EffectId spelling the contract governs.
  The kind part is fixed per family: events bind `emit-event` edges,
  jobs bind `enqueue-job`, calls bind `external-call`, cache contracts
  bind one of `cache-read`/`cache-write`/`cache-invalidate`, and
  publications bind `publish-output`;
- `sensitivity` — `standard` or `sensitive`; a sensitive effect
  automatically requires its `securityGate` review hint and the
  `security.review_gate` capability record;
- `securityGate` — the opaque typed review-contract pointer;
- `portability` — `portable` or `target-specific`. Runtime-specific
  details live only in the referenced target profile
  (`targetProfileRef` is mandatory for target-specific contracts and
  forbidden for portable ones);
- `contributions` — the explicit inclusion matrix over the effect
  graph, impact analysis, context capsules, and scenarios. Nothing is
  ever contributed implicitly: a surface receives the contract only
  when its flag is `true`;
- `capabilityRequirementRefs` — references to the attachment's
  capability records.

## Events

`eventVersion` and `schemaDigest` pin the exact event schema; `delivery`
is `local` or `durable`; `ordering` is `unordered`, `per_key`, or
`total`; `deduplication` is `none`, `key`, or `durable_key` with the
exact payload key field for the keyed modes; `correlationId` and
`causationId` name the traceability payload fields. Declared guarantees
carry their capability records: durable delivery needs
`delivery.durable`, per-key and total ordering need `ordering.per_key`
and `ordering.total`, and durable deduplication needs
`dedup.durable_key`.

## Jobs

`payloadDigest` pins the payload schema; `queueClass` names the queue
capability; `retry` declares the attempt budget (at most 16) with the
`none`/`fixed`/`exponential` backoff and its ceiling; `idempotency` is
`none`, `key`, or `intrinsic` with the exact key field; `timeoutMillis`
bounds every attempt; `deadLetter` is `none`, `park`, or `escalate`.
Retries never violate idempotency: a retrying job must be idempotent,
and contradictions reject through `contract.retry-conflict`. A
dead-letter policy needs the `queue.dead_letter` capability record.

## External calls

`providerContract` names the provider capability in the accepted #14
provider grammar (`vendor.mail/send@1.2.0`); `requestDigest` and
`responseDigest` pin the wire schemas; `errorRefs` is the closed typed
error vocabulary — at least one owner-supplied #62 reference, never a
catch-all; `timeoutMillis` bounds every attempt; `retry` carries the
attempt budget and the `safe`/`conditional_idempotent`/`unsafe` safety
declaration (an unsafe call never retries automatically);
`classification` is `read`, `write`, or `destructive`. Write and
destructive calls must declare a typed `compensationRef` to an
idempotent compensating operation — a recovery declaration, never proof
of atomicity — and the `call.compensation` capability record.

## Cache

`keyContract` pins the exact key version and the sorted key fields;
`operations` declares the closed `read`/`write`/`invalidate` set and
the bound effect kind must be one of them; `consistency` is `eventual`,
`read_your_writes`, or `strong`; `freshness` carries the optional
TTL and staleness bounds. Strong and read-your-writes consistency need
their capability records (`cache.strong`, `cache.read_your_writes`).

## Publications

`destination` names the typed publication target; `consent` is the
explicit opt-in (`optIn` is the fixed value `true`) plus the
`draft`/`preview`/`approved` approval level — approved publication
names its authority through `approverRef`, and publication without the
explicit consent contract rejects through
`publication.consent-missing`; `sensitivity` records the privacy
classification; `snapshot` is always the fixed `revisioned` mode with
the immutable revision field. Publications need the `publish.approval`
and `publish.revisioned_snapshot` capability records.

## Capability requirements and target profiles

Capability records name the closed ids — `delivery.durable`,
`ordering.total`, `ordering.per_key`, `dedup.durable_key`,
`queue.dead_letter`, `call.compensation`, `cache.strong`,
`cache.read_your_writes`, `publish.approval`,
`publish.revisioned_snapshot`, `security.review_gate` — or a
provider-owned adapter capability (`provider.<ns>.<name>`) whose
concrete owner is the named provider contract. Declared guarantees must
reference their records; unresolved references reject through
`contract.capability-missing`. The pure mapping (`map_capabilities`)
answers requirements against a caller-supplied snapshot under two
profiles:

- **strict** — blocks on `unsupported`, `unknown`, and unapproved
  `partial`; an adapter capability mismatch blocks the required
  semantics;
- **permissive** — degrades explicitly, never passes on `unsupported`
  or `unknown`.

`unknown` is never yes. Discovery, negotiation, and profile resolution
belong to #27/#28/#29; the verdicts are plain data a portability report
may render, so an unsupported delivery guarantee is always visible.

## Partial-failure cases

A case binds one Scenario IR document (stable id, explicit contract
version, exact IR digest) and proves, as portable data, what one
declared effect sequence promises when a named fault class
(`timeout`, `error`, `infrastructure`, `cancel`) fires at a named step:
a deterministic partial-order schedule (join sets, acyclicity
validated, every step scheduled exactly once), one closed expected
outcome per scheduled contract
(`success|conflict|error|infrastructure|unsupported|degraded|
recovery_required`), and the capability references the proof depends
on. A disturbed step never reports plain success; a compensating step
reports `recovery_required`. Execution and evidence belong to the owner
harnesses.

## Effect-graph and Scenario contributions

The attachment contributes effect bindings as plain typed data keyed by
exact #14 EffectIds with the per-surface inclusion flags; it never
mutates any graph. `validate_against_graph` cross-checks the attachment
against a supplied `EffectGraph`: every graph-contributed contract
reference must resolve to a declared edge of its operation with the
bound kind. Error references are opaque typed pointers; #62 owns error
identity.

## Diagnostics

Failures emit the accepted #11 diagnostic contract with the registry minor 1.7.0 → 1.8.0 (additions only): LEK-XE-001
`extended.input-invalid`, LEK-XE-002 `event.contract-invalid`,
LEK-XE-003 `job.contract-invalid`, LEK-XE-004 `call.contract-invalid`,
LEK-XE-005 `cache.contract-invalid`, LEK-XE-006
`publication.consent-missing`, LEK-XE-007
`contract.capability-missing`, LEK-XE-008 `contract.retry-conflict`,
LEK-XE-009 `case.case-invalid`, LEK-XE-010 `contract.gate-required`,
LEK-XE-011 `extended.export-limit`. Every echo is a bounded fixed
token; no attacker-controlled text enters a diagnostic.

## Bounds (v1, owner-approved)

10,000 contracts per kind; 512 cases; 64 steps, 1,024 schedule nodes,
64 joins per node, and 64 outcomes per case; 64 error references; 64
key fields; 256 requirement records; 32 capability references per
contract or case; retry budgets of 16 attempts; timeouts up to 24 hours
for jobs and 1 hour for calls; a 32 MiB canonical payload. Rejection,
never truncation.

## Semantic diff

Comparing two same-family attachments yields sorted paths and closed
classes: `breaking` (weakened or removed guarantees — delivery
downgrades, ordering loss, dedup loss, payload or schema changes,
classification downgrades, consent downgrades, key-contract changes),
`non-breaking` (additions, guarantee strengthenings, gate additions),
and `policy-change` (capability, inclusion, target-profile, queue,
and timeout mapping). Mixed revisions or foreign identities are the
typed error set — never a guessed classification.

## Boundaries

No runtime execution, no queue or provider implementation, no adapter
call, no transaction semantics (#24 owns commit and concurrency), no
authorization enforcement (#25 owns consent), no effect discovery, no
profile resolution, no report persistence, no source attachment home.
The attachment is an external typed document accepted through the typed
API; a canonical `lekalo/**` source home requires reviewed #4/#7
successors.
