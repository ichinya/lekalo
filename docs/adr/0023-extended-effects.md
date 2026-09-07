# ADR-0023: Extended effect contracts

Date: 2026-09-07
Status: accepted for issue #26

Custody: this issue carries the **prospective product candidate 0.1.30** in
every accepted path (workspace `Cargo.toml`, both `lekalo` packages in
`Cargo.lock`, the regenerated committed golden lock and its digest, the
`--version` behavior and its pinning tests, `README.md`, `docs/cli.md`);
issue #62 published product 0.1.29 (annotated tag `v0.1.29` on `de6f8a7`).
The extended-effects contract version (`lekalo/extended-effects/v1.0.0`,
identity `dev.lekalo.extended-effects@1.0.0`) is independent of the
product release, of the Model/IR/effect/Scenario contract versions, of
the error-contract family, and of the diagnostic registry by design.
The publication-order version may differ at integration; the
coordinator reconciles the custody number before publication without
touching this contract's identity.

## Context

Issue #14 made every operation's declared effects — database reads and
writes, but also events, jobs, external calls, cache side effects, and
published outputs — visible as typed data. What the Model still lacked
was an explicit, portable statement of the **semantics of those
non-database effects**: what an event delivery guarantee promises, when
a job may retry, which provider errors an external call can return,
what a cache key means, and under which consent a publication happens.
Issue #26 owns that surface as one independent closed attachment, per
the recorded research decision: Model v0.1.0 stays immutable, and no
source syntax, runtime execution, queue, provider, adapter, or report
surface is introduced here.

## Decision

### 1. One closed, immutable wire contract

[`contracts/extended-effects.schema.v1.0.0.json`](../../contracts/extended-effects.schema.v1.0.0.json)
(discriminator `lekalo/extended-effects/v1.0.0`, identity
`dev.lekalo.extended-effects@1.0.0`) is a closed Draft 2020-12 document
binding one project identity to one exact Model pin, one exact IR
digest, and an optional effect-graph digest. Everything is typed
qualified references and bounded enums; source text, physical paths,
literal tokens, credentials, runtime values, timestamps, and host data
are refused by construction. The attachment is immutable after
publication: additive optional fields require a reviewed minor
successor; removal, retyping, tightening, or new closed-enum meanings
require a major successor; no automatic migration exists.

### 2. Five contract kinds under one common member set

Events, jobs, external calls, cache, and publications share one closed
common member set — contract reference, source version, exact #14
effect binding, sensitivity with its security gate, target-neutrality
declaration, the explicit contribution matrix, and capability
references — and add the kind-specific vocabulary the issue defines.
The bound effect kind is fixed per family (event → `emit-event`, job →
`enqueue-job`, call → `external-call`, cache → one cache kind,
publication → `publish-output`) and is validated from the canonical
EffectId spelling alone; subject legality stays decidable only against
a real #14 graph.

### 3. Events: delivery, ordering, deduplication, causation

`delivery` is `local` (transactional outbox) or `durable`; `ordering`
is `unordered`, `per_key`, or `total`; `deduplication` is `none`,
`key`, or `durable_key` with the exact payload key field for the keyed
modes; `correlationId` and `causationId` name the traceability fields.
Every declared guarantee carries its capability record
(`delivery.durable`, `ordering.per_key`, `ordering.total`,
`dedup.durable_key`) — a guarantee without a record rejects, so an
unsupported delivery guarantee can never hide.

### 4. Jobs: bounded retry that never violates idempotency

The attempt budget is at most 16 with the closed
`none`/`fixed`/`exponential` backoff and a declared ceiling; the
idempotency mode is `none`, `key`, or `intrinsic` with the exact key
field; every attempt is timeout-bounded; the dead-letter policy is
`none`, `park`, or `escalate` (with the `queue.dead_letter` record).
Retries never violate idempotency: a retrying job must be idempotent,
and the contradiction rejects through `contract.retry-conflict`.

### 5. External calls: typed errors, classification, compensation

The provider capability is named in the accepted #14 provider grammar;
request and response schemas are pinned by digest; the error
vocabulary is the closed set of owner-supplied #62 references with at
least one entry — errors never collapse to a catch-all. The
`read|write|destructive` classification drives the hard rules: write
and destructive calls must declare a typed `compensationRef` to an
idempotent compensating operation (never the calling operation itself)
and the `call.compensation` record, and an `unsafe` call never retries
automatically. Compensation is a recovery declaration, never proof of
atomicity.

### 6. Cache: exact keys and declared consistency

The key contract pins the key version and the sorted key fields; the
declared `read`/`write`/`invalidate` set must contain the bound effect
kind; consistency is `eventual`, `read_your_writes`, or `strong` with
freshness bounds, and the non-eventual levels need their capability
records and a declared read path.

### 7. Publications: explicit consent and immutable snapshots

Publication is never implicit. `optIn` is the fixed value `true`; the
approval level is `draft`, `preview`, or `approved` — approved
publication names its authority through `approverRef`, and an approved
consent without an authority rejects through
`publication.consent-missing`. The snapshot mode is fixed to
`revisioned` with the immutable revision field, and every publication
carries the `publish.approval` and `publish.revisioned_snapshot`
records.

### 8. Sensitive effects automatically require the review gate

A `sensitive` classification on any contract mandatorily carries the
`securityGate` review hint and the `security.review_gate` capability
record; the rejection (`contract.gate-required`) fires before any
other family rule, so a sensitive effect can never be declared without
its review hint, accidentally or otherwise.

### 9. Capability requirements; strict profiles block, permissive degrade

Typed requirement records name the closed capability ids or a
provider-owned adapter capability (`provider.<ns>.<name>`) whose
concrete owner is the named provider contract. Declared guarantees must
reference their records; unresolved references reject. The pure mapping
(`map_capabilities`) answers requirements against a caller-supplied
snapshot under two closed profiles: **strict** blocks on `unsupported`,
`unknown`, and unapproved `partial` — an adapter capability mismatch
blocks the required semantics — while **permissive** degrades
explicitly and never passes. `unknown` is never yes. The verdicts are
plain data a portability report may render; discovery, negotiation,
and profile resolution stay with #27/#28/#29.

### 10. Partial-failure cases keyed to Scenario IR

`PartialFailureCase` binds a stable scenario id, its explicit contract
version, and the digest of the exact canonical IR payload. Declared
steps carry the exercised contract and the injected fault class; the
schedule is an explicit deterministic partial order (join sets,
acyclicity validated, every step scheduled exactly once); expected
outcomes use the closed enum `success|conflict|error|infrastructure|
unsupported|degraded|recovery_required`, one per scheduled contract. A
disturbed step never reports plain success; a compensating step reports
`recovery_required`. Execution belongs to the owner harnesses.

### 11. Explicit contributions; nothing is implicit

The contribution matrix (`graph`, `impact`, `context`, `scenarios`) is
required on every contract, so inclusion in any surface is always an
explicit declaration: external writes and publications are never
included implicitly. The typed effect-binding projection contributes
plain data keyed by exact EffectIds; `validate_against_graph`
cross-checks every graph-contributed reference against a real `EffectGraph`
through the accepted loader seam. Scenario identity stays #23, error
identity stays #62, #14 stays read-only.

### 12. Diagnostics through the #11 seam: registry minor 1.7.0 → 1.8.0

The contract adds its own rule family — reusing `graph.*` would blur
contract families. The registry takes its next wire-shape-preserving
minor increment to
[`diagnostic-registry.v1.8.0.json`](../../contracts/diagnostic-registry.v1.8.0.json)
with `extended.input-invalid`, `event.contract-invalid`,
`job.contract-invalid`, `call.contract-invalid`,
`cache.contract-invalid`, `publication.consent-missing`,
`contract.capability-missing`, `contract.retry-conflict`,
`case.case-invalid`, `contract.gate-required`, and
`extended.export-limit` (LEK-XE-001..011, category `compatibility`),
additions only, zero mutations of the 189 published entries, strictly
id-sorted, assembled through the shared registry-backed constructor
with bounded fixed tokens only. Every bound rejects with no partial
result. The validation profiles and their schema pin the registry
version and move with it, exactly as in the 1.6.0 → 1.7.0 increment.

### 13. Determinism, canonical form, and bounds (v1, owner-approved)

Canonical bytes are compact UTF-8 JSON with byte-sorted keys, no BOM,
no trailing LF. Set-like collections (contracts, cases, requirement
records, capability references, error references, key fields, cache
operations, expected outcomes, join sets) normalize to unsigned-UTF-8-
byte order; schedule nodes order canonically by node id while their
join sets keep the declared partial order. Owner-approved v1 bounds:
10,000 contracts per kind, 512 cases, 64 steps, 1,024 schedule nodes,
64 joins, 64 outcomes per case, 64 error references, 64 key fields,
256 requirement records, 32 capability references per contract or
case, 16 retry attempts, 24-hour job and 1-hour call timeouts, and a
32 MiB canonical payload. Every bound rejects before allocation where
possible and never truncates by arrival order.

### 14. No runtime, no adapter, no source home

This issue implements pure contract, normalization, validation,
canonical form, diff, capability mapping, and the graph cross-check.
It does not execute events, jobs, calls, cache operations, or
publications; it does not implement queues or providers, does not
discover adapters, does not resolve profiles, does not enforce
authorization, does not persist evidence, and does not write reports.
Transaction and concurrency semantics stay with #24. A source
attachment home (for example
`lekalo/attachments/extended-effects.yaml`) would change the #4
canonical structure and the #7 loader; until those reviewed successor
seams exist the attachment is an external typed document accepted
through the typed API, not a canonical `lekalo/**` input.

### 15. The semantic diff and its classes

`diff::compare` answers two same-family attachments with deterministic
sorted paths and three classes. **Breaking**: contract removal,
delivery/ordering/dedup/idempotency/dead-letter downgrades, payload or
schema changes, error-vocabulary removal, classification downgrades,
compensation removal, consent downgrades, key-contract changes,
destination changes, gate removal. **Non-breaking**: contract and case
additions, guarantee strengthenings, retry-budget increases, gate
additions, trace-field additions. **Policy-change**: capability
records and references, inclusion flags, target profiles, queue
classes, timeouts. `invalid` inputs (foreign identity, mixed
Model/IR/effect revisions) are the typed error set, never a guessed
classification.

## Consequences

- #16 impact, #17 context, and #31/#47/#56/#107 scenario harnesses can
  consume stable typed event, job, call, cache, and publication data
  through the declared contribution flags instead of prose.
- #27/#28/#29 own the capability registry, negotiation, and profile
  resolution that consume the requirement records and snapshots; the
  strict/permissive decision table is normative for them.
- #25 owns the enforcement side of publication consent; the consent
  and approval vocabulary here is its portable declaration.
- The pinned goldens under `tests/fixtures/extended-effects` are
  gate-checked with exact Ajv 8.17.1 on Node 18 and 24 by
  `scripts/test-extended-effects-contracts.mjs` (additive CI line,
  pinned-Ajv step) and byte-compared by the Rust fixture suite.

## References

- [docs/extended-effects.md](../extended-effects.md) — the attachment
  surface, vocabularies, limits, and guarantees.
- [ADR-0013](0013-effect-graph.md) — the effect graph whose read-only
  discipline and EffectId spellings this attachment consumes.
- [ADR-0014 (Scenario IR)](0014-scenario-ir.md) — the Scenario IR the
  partial-failure cases key into.
- [ADR-0020](0020-transaction-concurrency.md) — the sibling attachment
  family whose shape (closed wire, capability records, strict/
  permissive mapping, registry minor increment) this ADR follows.
- [ADR-0010](0010-diagnostics.md) — the diagnostic contract and registry.
- [ADR-0019](0019-semantic-diff.md) — the semantic-diff family whose
  classification discipline the attachment diff follows.
