# ADR-0014: The single-symbol inspect projection

Date: 2026-09-05
Status: accepted for issue #15

Custody: this issue published product 0.1.21 (annotated tag `v0.1.21` on
`9ab5b07`); issue #21 published product 0.1.22 (annotated tag `v0.1.22`
on `2dab70e`); issue #20 now carries the **prospective product candidate
0.1.23** in every accepted path (workspace `Cargo.toml`, both `lekalo`
packages in `Cargo.lock` including the regenerated committed golden
lock and its digest, the `--version` behavior and its pinning tests,
`README.md`,
`docs/cli.md`); issue #23 published product 0.1.20 (annotated tag
`v0.1.20` on `eef1863`); issue #22 published product 0.1.19 (annotated
tag `v0.1.19` on `31468e9`); issue #14 published product 0.1.12
(annotated tag `v0.1.12` on `81666da`). The inspect contract version
(`lekalo/inspect/v1.0.0`, identity `dev.lekalo.inspect@1.0.0`) is
independent of the product release, of the Model/IR/graph/effect/protocol
contract versions, and of the diagnostic registry by design.

## Context

Issue #13 shipped the dependency graph and issue #14 the effect graph, but
a human or an AI consumer that wants to change one symbol still has to
assemble its context by hand: identity, contract, invariants, policies,
effects, relations, scenarios, bindings. Issue #15 owns the read-only
single-symbol view that answers that question deterministically, without
reading the whole repository.

The research briefs (run run_088695f63032: worker_done msg_f204080a36e1,
briefs msg_88a3735f3e89, msg_54b7c5088c7a, msg_600e06c5b82f) recorded the
owner decisions this ADR adopts.

## Decision

### 1. Independent closed contract over accepted APIs only

`contracts/inspect.schema.v1.0.0.json` is the single new contract file.
The projection consumes exactly the accepted #8 IR, #13 dependency graph,
and #14 effect graph (plus the loader's source map for the one logical
source location). It never parses source bytes, never reads target files,
generated code, or manifests, never re-scans or rebuilds indexes, and
never writes anywhere. `--include` is the closed include filter
(`bindings`, `scenarios`); filter application happens before bounds, and
the selected symbol is always retained.

### 2. Safe selector resolution (owner decisions adopted)

The selector is grammar-validated before any project discovery
(`^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*){0,2}$`, at most 191 bytes) and is
never passed to a filesystem API. A valid full id resolves
case-sensitively through the canonical definition index; an exact miss may
consult the accepted #6 alias registry (`rename_history`) — one live
target resolves with mode `alias`, tombstoned/dead/conflicting targets
fail closed. A valid one-segment token is a safe short name matching final
segments only. Zero matches and many matches are the distinct stable
diagnostics `inspect.short-name-unknown` and
`inspect.short-name-ambiguous`; ambiguity always requires
disambiguation, never a first match. Malformed selectors are `cli.usage`
failures with no echo of the rejected bytes — the B2 attacker-echo class
cannot reappear.

### 3. Diagnostic ids: registry minor increment (owner decision adopted)

The briefs name four stable inspect diagnostics. The accepted #11 registry
is the only source of rule identity, and its lifecycle rule defines the
mechanism: adding a rule without a wire-shape change is a registry minor
increment. This issue therefore extends the embedded registry
`1.2.0` → `1.3.0` with exactly four additive entries —
`inspect.symbol-unknown` (`LEK-INS-001`),
`inspect.short-name-unknown` (`LEK-INS-002`),
`inspect.short-name-ambiguous` (`LEK-INS-003`),
`inspect.output-limit` (`LEK-INS-004`) — following the #12 and #13
precedent (the `semantic.*`/`validate.*` and `graph.*` families). No
existing rule is touched; the two embedded validation profiles and their
schema const are re-pinned to `1.3.0` as part of the same increment, as
#13 did. Unlike #14 — which could reuse the `graph.*` family unchanged —
the issue's acceptance criteria require distinct unknown/ambiguous
diagnostics that no registered rule provides, and reusing a `loader.*` or
`lock.*` id inside inspect output would misattribute domain ownership.

### 4. Every mandatory section, explicit states, no silent omission

All fourteen sections are present in every result in the fixed wire order
identity, contract, invariants, policies, effects, dependencies,
dependents, scenarios, bindings, ownership, portability, trace,
completeness, diagnostics. Closed section states are
`available | empty | unknown | unsupported | truncated | error`. An absent
data source is `unsupported` with a fixed reason token, never a missing
key or a falsely empty array; `empty` is reserved for a valid symbol that
declares nothing in that section. `ownership` (#21) and `trace` (#22) are
`unsupported` with `owner-not-accepted`; richer scenario detail stays with
#23 and is recorded as `scenario-details-unavailable`; binding target
details are typed, namespaced, and absent until #29 profile records exist.
`completeness` aggregates the fixed reason tokens and the exact
omitted-item count, so v1 results are honestly `partial` while those
owners are absent.

Entity identity membership is projected as the one invariant the accepted
Model can declare; nothing else is invented.

### 5. Bounds and determinism (v1, owner-approved)

Section items returned: 256. Ambiguity candidates: 32 (plus the exact
`matched` total). Semantic id echo: 192 bytes — the briefs recommended 129,
but that would reject valid Model 1.0.0 symbol ids (191 bytes), so the cap
follows the #13 `NodeId` bound instead; the brief's intent (bounded ids) is
preserved. Description/message echo: 4 096 bytes. Provenance refs per item:
8. Whole payload: 1 MiB. Only the payload bound fails the invocation
(`inspect.output-limit`); every other bound degrades the affected section
to `truncated` with returned/omitted counts and the deterministic
`frontier`. Effect items reuse the accepted #14 wire bytes verbatim;
relation provenance reuses the #13 record shapes; canonical bytes are
compact UTF-8 JSON with fixed wire order and byte-sorted set-like arrays,
byte-identical across reruns, filesystem orders, and invocation
directories.

### 6. CLI handoff

`lekalo inspect SYMBOL [--include SECTIONS] [--project DIR]` only selects,
renders, and maps exits onto the accepted 0/1 envelope (the success
payload reuses the graph-class carrier with `{"status":"valid",
"inspect":{...}}` bytes, exactly as #14 reused it for `effects`). The
selector is grammar-validated before any project discovery; unknown and
ambiguous selectors are explicit `invalid` failures on stderr with exit 1
— never empty successes.

## Consequences

- #16 impact and #17 context consume the same resolution, section, and
  bound seams instead of re-deriving them.
- #21 ownership, #22 trace, and #23 scenario detail plug into typed
  section seams (state `unsupported`, reasons, include vocabulary) without
  a shape change here; when they land, the sections flip to `available`
  and `completeness` moves toward `complete` without a breaking change.
- The registry increment is additive and wire-shape-preserving; retired
  codes stay retired and the next increment is `1.4.0`.
- The pinned goldens (`tests/fixtures/inspect/golden/`) are gate-checked
  with exact Ajv 8.17.1 on Node 18 and 24.

## References

- [docs/inspect.md](../inspect.md) — the inspect surface and guarantees.
- [ADR-0012](0012-dependency-graph.md) — the dependency graph consumed here.
- [ADR-0013](0013-effect-graph.md) — the effect graph consumed here.
- [ADR-0007](0007-ir.md) — the typed IR the projection reads.
- [ADR-0010](0010-diagnostics.md) — the diagnostic contract and registry.
