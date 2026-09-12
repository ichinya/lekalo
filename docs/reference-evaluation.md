# The reference evaluator (issue #107)

One deterministic, in-memory reference evaluation of one validated
Scenario IR against one exact pinned compiled IR, one pinned
[#63](adr/0024-invariant-transition.md) invariant-transition
attachment, and one pinned [#62](adr/0022-error-contracts.md) error
registry. The evaluator is the conformance oracle and golden-behavior
definition for adapters: it validates semantics before target
generation and lets Node/PHP/Go backends compare their normalized
execution against reference traces. It is not a production runtime,
not a database, and it proves nothing about production equivalence —
every behavior outside the supported subset is an explicit
`unsupported` outcome, never a guess.

The contract, guarantees, and limits live in
[ADR-0041](adr/0041-reference-evaluation.md); the wire shape is
[`contracts/reference-evaluation.schema.v1.0.0.json`](../contracts/reference-evaluation.schema.v1.0.0.json)
(discriminator `lekalo/reference-evaluation/v1.0.0`, identity
`dev.lekalo.reference-evaluation@1.0.0`); hermetic fixtures are under
`tests/fixtures/reference-evaluation/`.

## Identity and pinning

Every trace names the exact scenario id, the digest of the canonical
scenario bytes, the Model version, the IR digest, and the attachment
revision it interpreted, plus the separate
`dev.lekalo.reference-semantics@1.0.0` identity of the executable
semantics — reference behavior is versioned with Model/IR. Before any
step runs, the evaluator recomputes the pinned project's canonical IR
digest and refuses the whole evaluation (typed `refusal` token,
`unsupported` status, no partial sections) when scenario and
attachment pins disagree. The consumed contracts are read-only data;
nothing is written, read from disk, spawned, or fetched at any point.

## The supported deterministic subset

- **Given.** State preconditions materialize typed entity rows keyed
  by the canonical JSON of the entity's declared identity fields;
  actors, clocks, and ID sources are recorded controls; opaque
  fixtures are never materialized.
- **Commands.** Exactly one pinned #63 transition per command
  (`no-transition` / `multiple-transitions` otherwise); the row is
  located by identity fields in the input (`identity-incomplete`
  otherwise; `row-not-found` when absent); entry preconditions
  evaluate over input, prior row, and the evaluation clock; ordered
  assignments write literal, input, prior-field, or clock (`now`)
  sources; every invariant of the transition's state space is
  enforced post-write over the staged state (field-value, cross-field,
  temporal, conditional-requirement, one-active, uniqueness,
  cardinality, aggregate-consistency kinds). Any violation rolls the
  whole transaction back and reports the sorted violated invariant
  ids. States themselves stay opaque — the #63 wire binds no row
  field to state identifiers, so `member_of_set` and
  `immutable_after_state` are reported `state-field-unbound` instead
  of guessed.
- **Queries.** One read entity, exact-match filtering on input fields
  that name entity fields, identity-key order, full-row projection,
  `list` or single-row shapes. The #64 filter grammar, pagination,
  and includes are `unsupported` seams.
- **Effects.** A successful command records one entity-write entry
  plus the declared event intents of its effects, in declaration
  order, into a gap-free effect log with a canonical digest.
- **Idempotency.** A durable key replays the recorded result — same
  outcome, zero new effects; an explicit `replay` must match
  operation and input.
- **Clock and IDs.** The step's clock reference, else the first
  declared clock, else the epoch fallback `1970-01-01T00:00:00Z`;
  UUIDv4 IDs derive from SHA-256(`seed:index`) with the RFC 4122
  version/variant bits pinned, sequence IDs spell `seed-index`.
- **Assertions.** `result`, `error` (membership in the #62 registry
  union of the operation), `entity_state`, `emitted`,
  `forbidden_effect`, `idempotency`, and `unsupported` are decided;
  `authorization`, `contract_match`, and `deterministic_fixture` are
  `unsupported` verdicts. An actor-carrying invoke whose transition
  declares a policy is not executed at all.
- **Overall status.** `fail` when any assertion failed, `unsupported`
  when any step or assertion hit unsupported semantics, else `pass`.

## Determinism and comparison

Canonical trace bytes are compact UTF-8 JSON with byte-sorted object
keys, behavioral order preserved, and no trailing LF, bounded at
1 MiB. The same pinned inputs produce byte-identical traces on every
host; canonical state and effect digests allow adapters to compare
only the sections they need. The Rust suite
(`crates/lekalo-core/tests/reference_evaluation.rs`) proves the
planner happy / error / idempotency scenarios execute in memory over
the committed board fixtures, transaction rollback and invariant
failures are deterministic, given-order permutation keeps state and
effects byte-stable, durable-key replays never duplicate effects, and
the goldens are byte-identical. The independent Node gate
(`scripts/test-reference-evaluation-contracts.mjs` with exact
Ajv 8.17.1 on Node 18 and 24, part of the CI pinned-Ajv list)
re-validates every golden against the published schema, re-derives
the canonical form, and verifies the digest manifest.

## Boundaries

Pure library surface in `lekalo_core::reference_evaluation`; no CLI
command in v1. Diagnostics reuse the accepted #11 infrastructure rules
(`graph.export-limit`, `graph.traversal-limit`); the diagnostic
registry file is unchanged. Purely declarative validation (#12) never
invokes the evaluator. Authorization decisions (#25), fixture
materialization, #66 expressions, #64 query filters and pagination,
contract-match and fixture-digest verification, and #24 race
schedules are the recorded `unsupported` seams; the closed capability
sets are recorded in every trace.

## References

- [ADR-0041](adr/0041-reference-evaluation.md) — owner decisions.
- [The Scenario IR](scenario-ir.md) — the consumed behavioral
  documents.
- [Invariants and state transitions](invariant-transition.md) — the
  consumed command semantics.
- [Error contracts](error-contracts.md) — the consumed typed error
  unions.
