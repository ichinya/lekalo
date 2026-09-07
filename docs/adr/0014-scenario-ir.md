# ADR-0014: The portable Scenario IR for multi-target conformance tests

Date: 2026-09-05
Status: accepted for issue #23

Custody: this issue published product 0.1.20 (annotated tag `v0.1.20` on
`eef1863`); issue #22 published product 0.1.19 (annotated tag `v0.1.19`
on `31468e9`); issue #15 published product 0.1.21 (annotated tag
`v0.1.21` on `9ab5b07`); issue #21 published product 0.1.22 (annotated
tag `v0.1.22` on `2dab70e`); issue #20 published product 0.1.23
(annotated tag `v0.1.23` on `15be55a`); issue #16 published product 0.1.24
(annotated tag `v0.1.24` on `b4109e5`); issue #17 published product 0.1.25
(annotated tag `v0.1.25` on `e627fe5`); issue #18 published product 0.1.26 (annotated tag `v0.1.26` on
`3710179`); issue #24 published product 0.1.27 (annotated tag `v0.1.27` on
`ef7680d`); issue #25 published product 0.1.28 (annotated tag `v0.1.28` on `967bf52`); issue #62 now carries the **prospective product candidate 0.1.29** in every accepted path (workspace
`Cargo.toml`, both `lekalo` packages in
`Cargo.lock`, the `--version` behavior and its pinning tests, the
regenerated committed lock golden and its digests, `README.md`,
`docs/cli.md`); issue #14 published product 0.1.12
(annotated tag `v0.1.12` on `81666da`). The Scenario IR contract version
(`lekalo/scenario-ir/v1.0.0`, identity `dev.lekalo.scenario-ir@1.0.0`)
is independent of the product release, of the Model and IR contract
versions, of the diagnostic registry, and of every execution backend by
design.

## Context

Issue #23 asks for one portable Scenario IR so a behavioral scenario is
authored once and checked against the native test backend of every
target. The research briefs (run run_088695f63032: briefs
msg_d6d28e3adbb3, msg_b3ac8ea03b65, msg_e6bd96d2f772, worker_done
msg_80767e864a4d) recorded the candidate decisions; this ADR adopts
their narrow reading as the owner decisions for v1.

## Decision

### 1. Independent closed contract; sidecar custody

The Scenario IR publishes its own wire contract,
[`contracts/scenario-ir.schema.v1.0.0.json`](../../contracts/scenario-ir.schema.v1.0.0.json)
(discriminator `lekalo/scenario-ir/v1.0.0`, identity
`dev.lekalo.scenario-ir@1.0.0`). `contracts/` gains exactly this one
new file. Scenario IR is a derived test contract: not canonical Model
source (no #5/#7 successor is embedded), not the compiled IR (#8), and
not runtime evidence. The **neutral evidence/result envelope is not
published here** — its production belongs to #31/#107 and its
aggregation to #91, so the IR carries only typed binding references
and expected evidence digests. If a later owner assigns the envelope
to #23, it lands as a separate versioned contract. The source map is a
separate digest-bound sidecar keyed by scenario ID, step ID, role and
member path, reference role, and occurrence ordinal; the IR references
it (`sourceMapRef`, identity
`dev.lekalo.scenario-sourcemap@1.0.0`, at most 8192 entries) and never
contains logical paths or spans itself.

### 2. Explicit stable identity, ordered steps

Scenario identity is the inherited #5 semantic grammar
(`planner.scenario.switch_focus`) plus an explicit `scenarioVersion`;
project identity is a one-segment root id. Every step carries a
scenario-local `stepId`; identity is `scenario_id + step_id`, array
position is never identity, and the occurrence ordinal exists only for
source and reference disambiguation. `given`/`when`/`then` arrays stay
ordered because ordering is behavior; only set-like collections (tags,
binding capabilities and order, contract-match projections, bindings)
and map keys are canonically sorted.

### 3. Closed shapes; no inference from names

`given` preconditions establish data and controls only: typed entity
state (selector plus fields — bare field names make entity/field scope
confusion structurally impossible), an opaque fixture with version and
capability set, a typed actor, a deterministic clock, or a
deterministic ID source. Reusable setup is allowed only with its
fixture reference visible. `when` is exactly `invoke` in v1: qualified
operation, typed input, optional actor and clock controls, explicit
idempotency key, and optional bounded `replay` metadata pointing at a
prior action. Writes, effects, authorization, retries, and error
categories are never inferred from operation names. `then` carries the
ten closed assertion kinds, including first-class `forbidden_effect`
and `idempotency` (both demanded by the issue) and the explicit
`unsupported` expectation, which is a declaration of a known-absent
capability and never an implicit pass. Assertions are separate from
metrics; a metric or timing observation cannot satisfy an assertion.
Entity-state field expectations support exact typed values plus a
closed matcher set (`datetime`, `uuid`, `uri`, `decimal`, `non-null`)
for unpinned literals.

### 4. Typed values and references

`TypedValue` is a closed recursive union with explicit tags and
canonical literal spellings (canonical decimal, UTC datetimes without
offsets, lowercase UUIDs, credential-free URIs); floats, arbitrary
JSON, unbounded maps, implicit coercion, and locale-dependent spellings
are rejected. `Ref` is a closed typed union over semantic and
scenario-local targets with optional bounded member path and expected
type; step-local kinds (`step-output`, `given-value`, `clock`,
`id-source`) carry step identifiers and are resolved by the
scenario-local data-flow pass.

### 5. Scenario-local data flow; owner seams untouched

Validation fails closed with one registered diagnostic and no partial
IR. Beyond shape, the data-flow pass rejects duplicate step IDs,
empty scenarios, dangling observations, forward step-output
references, unestablished given values or controls, control
references to steps of the wrong kind, two state setups claiming the
same entity row with overlapping fields, an idempotency assertion
without key or replay control, an actor reference without a declared
actor, and unreachable `then` steps. Then-step reachability means the
observation traces to an action: it observes a `when` step or a given
step some action consumed. Operation existence, kinds, input/output
types, and visibility are #12 checks — scenario documents reference
operations, errors, effects, and policies by typed ID only; the
referenced ErrorId being inside the operation ErrorUnion is #62's
check once accepted, and until then unresolved error references stay
typed references. Forbidden-effect assertions carry typed refs for
#14 without creating effect nodes or comparisons; transaction and
concurrency semantics stay with #24; actor and policy semantics stay
with #25.

### 6. Coverage without preemption

Every valid scenario derives a stable `CoverageVector` (scenario,
step, action, assertion identities, covered operations, expected
outcome kind, optional typed error/effect/policy refs). It is plain
rebuildable data — the typed contribution a later graph `verifies`
decision may consume; this issue emits no graph nodes, write edges,
or declared-versus-detected comparisons, and no execution.

### 7. Diagnostics, limits, determinism

No registry rules are added in v1 (the registry file is unchanged):
fatal failures reuse `graph.input-invalid`, `graph.traversal-limit`,
and `graph.export-limit` through the shared registry-backed
constructor; `detail` is always a fixed class token and `role` is a
bounded, control-cleaned, grammar-checked member name — no
attacker-controlled echo. Owner-approved v1 bounds: 256 given, 256
when, 512 then, 1024 total steps per scenario; 128 typed values per
step; 32 typed-value depth levels; 4096 items per list or object;
4096 code points per scalar; 64 selector terms and 64 field entries;
64 tags of at most 64 bytes; 64 metadata entries of at most 64 KiB
canonical bytes; 32 bindings; 64 capabilities per binding or fixture;
32 projection paths; 8192 source-map entries; 1 MiB canonical payload;
a 1 KiB summary. Canonical bytes are compact UTF-8 JSON with
byte-sorted object keys and sorted set-like collections, behaviorally
ordered arrays untouched, path-independent, with no trailing LF. The
wire transport and the #7 loader own byte-level input classification
(BOM, comments, trailing bytes, duplicate JSON keys); the committed
Node gate proves every golden is canonical and duplicate-key-free.

### 8. No CLI command in v1

`lekalo` gains no scenario command: CLI vectors require an
owner-approved command and the accepted #11 renderer handoff, so the
surface ships as the pure typed library, the committed fixtures, and
the two release gates (`scripts/test-scenario-contracts.mjs` under
exact Ajv 8.17.1 on Node 18 and 24 — added only to the pinned-Ajv CI
step — and `crates/lekalo-core/tests/scenario.rs`).

## Consequences

- #31/#107/#47/#56 consume scenario bindings and produce neutral
  evidence at their own seams; #91 aggregates; #103 reports.
- #13 may later accept the coverage vector as a typed `verifies`
  contribution; #14 and #25 receive typed forbidden-effect and
  authorization references.
- Adding assertion kinds, precondition kinds, or action kinds is a
  breaking wire change that must extend the closed schema, the typed
  model, and the registry checks together.

## References

- [docs/scenario-ir.md](../scenario-ir.md) — the scenario surface.
- [ADR-0005](0005-semantic-ids.md) — the inherited ID grammar.
- [ADR-0007](0007-ir.md) — the bound IR contract.
- [ADR-0010](0010-diagnostics.md) — the diagnostic contract.
- [ADR-0013](0013-effect-graph.md) — the referenced typed seams.
