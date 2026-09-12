# ADR-0041: The in-memory reference evaluator for basic semantics and the Scenario IR

Date: 2026-09-12
Status: accepted for issue #107

Custody: this issue carries the **prospective product candidate 0.2.16** in
every accepted path (workspace `Cargo.toml`, both `lekalo` packages in
`Cargo.lock` via `cargo update -w`, the regenerated committed golden lock
and its digest, the `--version` behavior and its pinning tests,
`README.md`, `docs/cli.md`); issue #63 carried prospective product
0.1.31. The reference-evaluation contract version
(`lekalo/reference-evaluation/v1.0.0`, identity
`dev.lekalo.reference-evaluation@1.0.0`) is independent of the product
release, of the Model/IR/Scenario contract versions, of the
invariant-transition and error-registry attachments it consumes, and of
the diagnostic registry by design. The diagnostic registry is
**unchanged** by this issue: the evaluator reuses the accepted #11
infrastructure rules (`graph.export-limit`, `graph.traversal-limit`)
with fixed detail tokens, exactly as the Scenario IR did before it. No
ADR-consuming registry reservation is spent.

## Context

Issue #107 asks for a minimal deterministic reference evaluator of the
basic Lekalo semantics so that Model/Scenario IR and adapters can be
checked independently of any Node/PHP/Go runtime. The executable
meaning of a command is not declared by the Model (commands carry only
inputs and effect references) and not declared by the Scenario IR
(scenarios only observe). The accepted prerequisite contracts that do
declare executable meaning are the #63 invariant-transition attachment
(operation-entry preconditions, ordered assignments, post-write
invariants, state spaces) and the #62 error registry (the typed
`Result<Output, ErrorUnion>` binding per operation). The #24
concurrency race schedules explicitly name #107 as an execution owner
but pin their guarantees to native harnesses, and the #25
authorization, #26 extended-effect, and future #66 expression families
own semantics that are not runnable from this repository's data alone.

## Decision

### 1. One library evaluator, no CLI command in v1

The evaluator lives in `lekalo_core::reference_evaluation` as a pure
library surface: `ReferenceEvaluation::new(&CompiledProject,
&InvariantTransitionAttachment, &ErrorRegistry)` binds one evaluation,
and `execute(&ScenarioIr)` produces one `ReferenceTrace`. There is no
CLI command in v1 (matching the Scenario IR precedent): traces are
contract data compared by gates and later by native runners
(#47/#56), not a report surface. The core stays fully usable without
the evaluator — purely declarative validation (#12) never calls into
it.

### 2. One closed, immutable trace contract

[`contracts/reference-evaluation.schema.v1.0.0.json`](../../contracts/reference-evaluation.schema.v1.0.0.json)
(discriminator `lekalo/reference-evaluation/v1.0.0`, identity
`dev.lekalo.reference-evaluation@1.0.0`) is a closed Draft 2020-12
document. Top-level members: `schemaVersion`, `identity`, `semantics`,
`scenario`, `modelVersion`, `irDigest`, `attachmentRevision`, optional
`refusal`, `capabilities`, `status`, `steps`, `effects`,
`effectDigest`, `state`, `stateDigest`, `assertions`. Every identity
is a stable semantic identifier; the trace pins the exact canonical
scenario digest, the exact IR digest, the Model version, and the
attachment revision it interpreted, plus the separate
`dev.lekalo.reference-semantics@1.0.0` identity of the executable
semantics. Reference behavior is thereby versioned with Model/IR: a
semantics change is a reviewed successor of this contract, never a
silent drift.

### 3. Pins must agree or the evaluation refuses

Before any step runs, the evaluator recomputes the canonical IR bytes
of the pinned `CompiledProject` and compares their digest with the
scenario's `irRef`; then it compares scenario/attachment IR digests
and Model versions. Any disagreement produces a refused trace
(`status: "unsupported"`, `refusal` token `ir-pin-mismatch`,
`attachment-ir-mismatch`, or `model-pin-mismatch`) with empty step,
effect, and assertion sections. An evaluator never interprets
contracts it was not pinned to.

### 4. The declared reference subset

Only the explicitly supported deterministic subset executes:

- `given` state rows materialize into byte-sorted in-memory maps
  keyed by the canonical JSON of the entity's declared identity
  fields; actors, clocks, and ID sources are recorded controls.
  Opaque fixtures are never materialized (`fixture`).
- `when` invokes run as staged all-or-nothing transactions over the
  in-memory state. A command needs exactly one transition bound to it
  in the pinned attachment (zero or several are `no-transition` /
  `multiple-transitions`); its state-space entity locates the target
  row by identity fields present in the input; entry preconditions
  evaluate over input, prior row, and the evaluation clock; ordered
  assignments write literal, input, prior, or clock (`now`) sources;
  post-write, every invariant of the transition's state space is
  enforced (field-value, cross-field, temporal, conditional
  requirement, one-active with its partition and `maxActive`, uniqueness,
  cardinality over a collection field, aggregate consistency over the
  referenced entity rows). Any violation rolls the whole transaction
  back deterministically and reports the sorted violated invariant
  ids. An actor-carrying invoke whose transition declares an
  authorization policy is not executed at all (`authorization`):
  policy decisions are owned by #25. #66 expression assignments are
  `expression-ref`, never evaluated here.
- Queries use the closed reference read semantics: one read entity,
  exact-match filtering on input fields that name entity fields,
  identity-key order, full-row projection, `list` or single-row
  return shapes. Pagination, #64 filter operators, includes, and
  cursors are `unsupported`, not approximated.
- Event intents of a successful command are recorded as declared
  effect-log entries (`event_intent`) from the command's effect
  definitions; jobs have no declaring seam in the consumed contracts
  and are recorded only as the `job_intent` kind for adapters.
- Durable-key idempotency: a `when` step carrying an idempotency key
  replays the recorded result of the first execution with the same
  canonical `(operation, key)` — same outcome, zero new effects; an
  explicit `replay` must additionally match operation and input.
- The evaluation clock is the step's explicit clock reference, else
  the first declared `given` clock, else the documented epoch fallback
  `1970-01-01T00:00:00Z`. Deterministic IDs derive from
  SHA-256(`seed:index`) with the RFC 4122 v4 version and variant bits
  pinned (UUIDv4) or the exact `seed-index` spelling (sequence).
- `then` assertions verify result values (exact canonical spelling),
  typed error membership against the pinned #62 registry union,
  entity state with presence/count and field matchers, emission
  counts, forbidden effects against the effect log, and replay
  idempotency. Authorization, contract-match, and
  deterministic-fixture assertions are `unsupported` verdicts; the
  `unsupported` assertion passes exactly for the closed known-absent
  capability set recorded in every trace.

### 5. Determinism and canonical bytes

Traces serialize through the same canonical discipline as every other
contract: compact UTF-8 JSON, byte-sorted object keys, behavioral
order preserved, no trailing LF, bounded at 1 MiB. No filesystem,
network, process, wall-clock, randomness, locale, or host value ever
enters an evaluation. The same pinned scenario, IR, attachment, and
registry produce byte-identical traces on every host; the committed
goldens are re-proven byte-identical by the Rust suite and the
independent Node gate (`scripts/test-reference-evaluation-contracts.mjs`
with exact Ajv 8.17.1, added to the CI pinned-Ajv list), which also
re-derives the canonical form and verifies the digest manifest.

### 6. No production-equivalence claim

The evaluator is a conformance oracle and golden-behavior definition.
It does not prove production equivalence, does not replace a database,
does not execute external or foreign code, and every place where a
native backend may legitimately differ (isolation, concurrency,
authorization, external calls) is an explicit `unsupported` outcome in
the trace rather than a guessed result.

## Consequences

- Adapters and native runners (#47/#56) can compare their normalized
  execution against reference traces as plain JSON with no Rust
  coupling.
- Integration with the accepted #64 query model and #66 expression
  family extends the supported subset through a reviewed successor of
  this contract; the `query-filter` and `expression` capability
  entries are the recorded seams.
- The #24 race-schedule execution stays with the native harnesses;
  the reference evaluator executes serial scenario semantics only,
  and `race-schedule` is declared absent.
