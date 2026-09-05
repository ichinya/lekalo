# ADR-0013: The effect graph of reads, writes, events, and external calls

Date: 2026-09-05
Status: accepted for issue #14

Custody: this issue published product 0.1.12 (annotated tag `v0.1.12` on
`81666da`); issue #23 published product 0.1.20 (annotated tag `v0.1.20`
on `eef1863`); issue #22 now carries the **prospective product candidate
0.1.19** in every accepted path (workspace `Cargo.toml`, both `lekalo`
packages in `Cargo.lock` including the regenerated committed golden lock
and its digests, the `--version` behavior and its pinning tests,
`README.md`, `docs/cli.md`); issue #13 published product 0.1.11 (annotated
tag `v0.1.11` on `007c01d`). The effect contract version
(`lekalo/effects/v1.0.0`, identity `dev.lekalo.effects@1.0.0`) is
independent of the product release, of the Model/IR/graph/protocol
contract versions, and of the diagnostic registry by design.

## Context

Issue #13 shipped the dependency graph of semantic symbols. It answers
what references what, but nothing models the actual radius of state
change: which operations read an entity, which write it, what they emit,
and where an operation reaches beyond the canonical model. Issue #14 owns
that surface for AI consumers, validators, and parallel-change checks.

The research briefs (run run_088695f63032: briefs msg_5f4a604776e6 and
msg_c40722f473de, correction msg_56f1f3da3d31, worker_done
msg_5e876391c107) recorded the owner decisions this ADR adopts.

## Decision

### 1. Independent closed contract, built on the #13 graph APIs

The effect graph publishes its own wire contract,
[`contracts/effect-graph.schema.v1.0.0.json`](../../contracts/effect-graph.schema.v1.0.0.json)
(discriminator `lekalo/effects/v1.0.0`, identity `dev.lekalo.effects@1.0.0`),
independent of every other contract family. `contracts/` gains exactly
this one new file; the diagnostic registry file is unchanged. The model is
a separate typed effect model — effect meaning never enters the generic
#13 graph relations — but implementation-wise it reuses the accepted #13
identity grammar (`NodeId` for operations), the confidence vocabulary, and
the same construction, index, and canonicalization discipline.

### 2. Typed identities, closed kinds

`ResourceId` is a closed kind plus a validated id: `canonical` (semantic
entities) and `event` (declared event symbols) are semantic; every other
kind (`target-resource`, `external-service`, `cache`, `job`, `output`,
`audit`) is adapter-owned and never pretends to be a Lekalo entity. A
subject is one explicit resource plus an optional exact field — entity
level is explicit, never an empty sentinel. The P0 kinds are exactly:
`read`, `create`, `update`, `delete`, `write-field` (with the closed
action set/clear/append/replace/merge), `emit-event`, `enqueue-job`,
`external-call`, `cache-read/write/invalidate`, `publish-output`,
`audit-log`, and `transaction-boundary`. Entity and field scope are
distinct everywhere: an entity read does not imply field reads, a field
write does not imply entity replacement, CRUD is not a free-form write
alias, and a `write-field` without a field or a field-scoped delete is a
fatal construction rejection.

### 3. The narrow declared projection

Declared effects are exactly what the accepted Model declares — nothing
invented from names: query `reads` project entity-level `read` edges,
command `effects` project CRUD edges on the referenced effect definition's
entity (the effect symbol is the edge origin), and effect `emits` project
`emit-event` edges, all attributed to the declaring operation. Field
writes, jobs, externals, cache, publication, audit, and transaction facts
do not exist in the Model, so the declared projection never emits them;
they enter only as typed detected evidence. Every declared reference must
resolve to its declared kind; any miss is a fatal `graph.input-invalid`
and no graph is produced. The projection carries the `sha256` digest of
the canonical IR it was built from.

### 4. Declared versus detected provenance

Declared provenance is `canonical-ir` (IR digest, reference role,
occurrence, source symbol) with `canonical` confidence. Detected effects
enter only through a typed evidence envelope (namespaced adapter/target
ids, exact protocol version, `sha256` evidence digest, closed trust
state); `rejected` trust is refused, stale/unknown/inferred trust attaches
with visibly degraded confidence and never satisfies a gate. The envelope
never mutates the canonical model: `attach_detected` returns a graph that
shares the declared edges. This module validates envelope shape and
binding only — production, transport, and provider policy stay with
#27/#28/#29.

### 5. Comparison, conflicts, summaries: derived data, not diagnostics

The declared-versus-detected comparison joins both sets by their typed
canonical identity (operation, kind with action, resource, exact field)
and classifies every difference into one closed state: `declared-only`,
`detected-only`, `matched`, `action-mismatch`, `scope-mismatch`,
`stale-evidence`, `unknown-evidence`, `unsupported-capability`,
`conflicting-evidence`; bounded reports carry explicit completeness and a
bounded reason instead of pretending wholeness. Unknown and stale evidence
never collapse into `matched`. Each item carries a stable explanation
template built from canonical data only.

Conflict classification runs over direct effect edges for an explicitly
supplied `ChangeSet` — this graph never parses Git, owns a diff, or infers
changed symbols (#16 owns changed-input derivation). The closed
conservative matrix: write/write on the same scope is
`definite-write-write`, delete with anything on its resource is
`delete-overlap`, read/write is `potential-read-write`, entity-wide versus
field scope is `entity-field-overlap`, independent exact fields never
conflict, emit/job and external collisions require the same typed scope,
shared transaction groups surface as `transaction-group-overlap`,
classified sensitivity markers upgrade an overlap to
`sensitivity-policy-gate`, and degraded evidence yields `unknown`, never a
silent no-conflict. This module reports facts; scheduling policy stays
with #16/#20/#91.

Summaries are bounded, deterministic, per-operation counts (reads, writes,
emissions, infrastructure, declared/detected, degraded/sensitive flags)
with explicit completeness.

### 6. Diagnostics through the accepted #11 seam, registry unchanged

The effect contract adds no registry rules in v1 — the registry file is
unchanged by design (adding rules requires a registry minor increment, a
contract file this issue does not own). Fatal construction, limit, and
unknown-subject failures reuse the graph-family rules
`graph.input-invalid`, `graph.traversal-limit`, `graph.export-limit`, and
`graph.unknown-node` through the shared registry-backed constructor with
bounded tokens on every echo. Declared/detected mismatches are not
diagnostics at all: they are closed comparison states in the result data
with stable explanations. Failures use the accepted #3 envelope
(`invalid`, exit 1, stderr); severity never computes an exit.

### 7. Limits and determinism (v1, owner-approved)

Effects: 100000 edges (declared plus detected), 100000 envelope entries,
8 envelopes, 128 changed operations, 50000 comparison items, 50000
conflict items, 50000 summary rows, 250000 result edges, 32 MiB export.
Every bound rejects with a typed diagnostic and no partial graph or
result. Edges sort by operation, kind, action, resource kind, resource,
field, origin, occurrence, then provenance bytes; exact duplicates
collapse only when every machine field matches. Canonical bytes are
compact UTF-8 JSON with byte-sorted keys, path-independent, byte-identical
for the same IR and evidence.

### 8. CLI handoff

`lekalo effects show OPERATION`, `lekalo effects writers RESOURCE
[--readers]`, and `lekalo effects conflicts --changed OPS` only select,
render, and map exits onto the accepted 0/1 envelope. The writers selector
accepts an entity semantic id, an exact `entity.field` scope, or a typed
`kind:id` resource reference. `--changed` carries the typed change set in
the command line; wiring it to #16 changed-input detection is a later
decision. Unknown operations or selectors are explicit
`graph.unknown-node` failures — never empty successes; a known resource
with no matching effects is an empty success.

## Consequences

- #16 impact and parallel-change checks consume stable query APIs with
  completeness and trust markers instead of deriving effects themselves.
- #17 inspect capsules can summarize an operation's effect radius from
  the summary seam.
- #24 transaction semantics, #25 sensitivity policy, #23 Scenario IR, and
  the adapter protocol owners (#27/#28/#29) each plug into typed seams
  (transaction-group refs, sensitivity markers, evidence envelopes)
  without a shape change here.
- The declared projection stays byte-stable under the same golden
  discipline as the #13 graph; the pinned golden export
  (`tests/fixtures/effects/golden/planner.effects.json`) is gate-checked
  with exact Ajv 8.17.1 on Node 18 and 24.

## References

- [docs/effect-graph.md](../effect-graph.md) — the effect surface and guarantees.
- [ADR-0012](0012-dependency-graph.md) — the dependency graph whose APIs this consumes.
- [ADR-0007](0007-ir.md) — the typed IR the declared projection reads.
- [ADR-0010](0010-diagnostics.md) — the diagnostic contract and registry.
