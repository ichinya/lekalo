# ADR-0012: The dependency graph of semantic symbols

Date: 2026-09-05
Status: accepted for issue #13

Custody: this issue published product 0.1.11 (annotated tag `v0.1.11` on
`007c01d`); issue #14 published product 0.1.12 (annotated tag `v0.1.12` on
`81666da`); issue #23 published product 0.1.20 (annotated tag `v0.1.20`
on `eef1863`); issue #22 published product 0.1.19 (annotated tag
`v0.1.19` on `31468e9`); issue #15 published product 0.1.21 (annotated
tag `v0.1.21` on `9ab5b07`); issue #21 published product 0.1.22
(annotated tag `v0.1.22` on `2dab70e`); issue #20 published product
0.1.23 (annotated tag `v0.1.23` on `15be55a`); issue #16 published
product 0.1.24 (annotated tag `v0.1.24` on `b4109e5`); issue #17 published
product 0.1.25 (annotated tag `v0.1.25` on `e627fe5`); issue #18 published product 0.1.26 (annotated tag `v0.1.26` on
`3710179`); issue #24 published product 0.1.27 (annotated tag `v0.1.27` on
`ef7680d`); issue #25 published product 0.1.28 (annotated tag `v0.1.28` on `967bf52`); issue #62 now carries the **prospective product candidate 0.1.29** in every accepted path (workspace `Cargo.toml`, both `lekalo`
packages in `Cargo.lock`, the `--version` behavior and its pinning tests,
`README.md`, `docs/cli.md`); issue #12 published product 0.1.10 (annotated
tag `v0.1.10` on `fdfbcb5`, followed by the CI-parity fix `47b2ec8`). The graph
contract version (`lekalo/graph/v1.0.0`, identity `dev.lekalo.graph@1.0.0`)
and the diagnostic registry increment (`1.1.0` → `1.2.0`) are independent of
the product release, of the Model/IR/protocol contract versions, and of the
lock wire by design.

## Context

Issues #3–#12 shipped the result envelope, structure rules, semantic IDs,
the loader, the typed IR, versioning, the committed lock, the diagnostic
contract, and semantic validation. The IR knew every symbol and every
resolved reference, but nothing answered dependency questions: what depends
on a symbol, what a symbol needs, whether a change can cross a module
boundary, how two symbols are connected. Issues #14 (effect graph), #15
(inspect), #16 (impact), #17 (context), #20 (cache), #33 (MCP), #37 (AI
Workspace), #77 (coupling lint), #83 (provider context), and #90 (shared
fixtures) all need one shared answer.

Issue #13 owns that answer: a read-only, deterministic dependency-graph
projection and query library over the accepted #8 `CompiledProject`. It
never parses files, never reads source bytes, never validates semantics
(#12 stays owner), never detects effects (#14), and never writes anything
anywhere.

## Decision

### 1. Independent closed contract and registry

The graph publishes its own wire contract,
[`contracts/graph.schema.v1.0.0.json`](../../contracts/graph.schema.v1.0.0.json)
(discriminator `lekalo/graph/v1.0.0`, identity `dev.lekalo.graph@1.0.0`),
independent of every other contract family. Node kinds and relations are
registry-backed identifiers with fixed ranks — never caller strings and
never an exhaustive Rust enum that would block future kinds. Extension
kinds and relations enter as versioned, namespaced registry records
(`vendor.example/kind`, canonical SemVer, explicit acyclic policy); unknown
required extensions fail closed, and the closed core ids are never
reassigned. A future #14 effect relation or scenario-trace kind is an
additive registry successor, not a schema break.

### 2. Node granularity

Twelve core node kinds: `project`, `module`, `type` (subkinds
`scalar`/`enum`/`value-object`), `entity`, `operation` (subkinds
`command`/`query`), `policy`, `event`, `effect` (the declared symbol only —
not the #14 effect model), `endpoint`, `scenario`, `target-binding`, and
`requirement` (stable ids only; never requirement text; #36 owns provider
resolution). Node identity is semantic and path-independent:
`{kind}:{semantic-id}`. `EffectDeclaration` exists solely to preserve
declared `emits`/reference edges.

### 3. Relation emission table

Only the relations the accepted typed IR actually represents:

| IR surface                      | Edge                              |
| ------------------------------- | --------------------------------- |
| module `imports`                | `requires` (module → module)      |
| entity/value-object/event field type leaves | `references`          |
| command `effects`               | `references`                      |
| effect `entity`                 | `references`                      |
| scenario `covers`               | `references` (until #23)          |
| command `input` type leaves     | `accepts`                         |
| query `returns` type leaf       | `returns`                         |
| query `reads`                   | `reads`                           |
| policy `applies_to`             | `authorizes`                      |
| endpoint `invokes`              | `exposes`                         |
| effect `emits`                  | `emits`                           |
| `derived_from` (project and definitions) | `derived_from`           |

`writes`, `implements`, and `verifies` stay **registered but not emitted**
until #14 (field-level writes), an accepted target-binding semantic
contract (#22/#27/#29), and #23 (native scenario verification) contribute
typed evidence. The graph never infers them from names, target files, or
source scans. Scenario `covers` remains a generic `references` edge; #23
owns `verifies`.

### 4. Kind-specific cycle policy

`requires` and `derived_from` are the graph-owned acyclic relations: any
strongly connected component with more than one node, or a self-loop, is a
fatal `graph.cycle-forbidden` diagnostic with bounded canonical members and
edge keys, and no graph is produced. All other core relations may cycle
(mutual type references, accepts/returns loops, evidence relations) and
stay traversal-safe by visited keys; the loader's `import-cycle` and #12's
`type-recursion` remain their own owners. Type recursion, query-write
legality, effect conflicts, and scenario semantics are #12/#14/#23
concerns.

### 5. Provenance, confidence, and spans

`EdgeProvenance` is a closed sum: `canonical-ir` (reference role,
occurrence ordinal, source symbol), `adapter-evidence` (validated
namespaced adapter/target/protocol ids, opaque `sha256` digest, closed
evidence status), and `derived` (algorithm id/version, at most
[`MAX_PROVENANCE_RECORDS`](../../crates/lekalo-core/src/graph/version.rs)
parent edge keys). Confidence is a closed non-numeric vocabulary —
`canonical`, `verified`, `extracted`, `inferred`, `unknown` — derived from
the provenance kind, so an edge can never claim more trust than its
evidence; stale or unknown evidence is a visible degraded state, never an
optimistic confirmation. Canonical graph bytes are path-independent:
source paths and spans resolve through the #8 source map only via the
explicit `--spans` sidecar (declaration spans per node, kind-aware so a
project and a module sharing an id never cross).

### 6. Limits (v1, owner-approved)

Graph: 100000 nodes, 1000000 direct edges. Queries: depth 256, 50000
result nodes, 250000 result edges, 256 path nodes, 128 filter terms, 8
provenance records per edge. Export: 32 MiB. Every bound rejects with an
explicit registered diagnostic (`graph.traversal-limit`,
`graph.export-limit`) and no partial graph — results are never truncated
by arrival order. Construction is O(V+E) indexing; transitive edges are
never materialized, which keeps #20 cache keys stable.

### 7. Diagnostics through the accepted #11 seam

Eight graph-owned rules, `LEK-GRAPH-001`–`008`, in the `graph.` namespace:
`cycle-forbidden` (semantic), `export-limit`,
`input-invalid`, `path-not-found` (semantic), `provenance-incomplete`
(semantic warning, the only non-invalid status), `traversal-limit`,
`unknown-node`, and `unknown-relation`. This is a wire-shape-preserving
registry minor increment (`1.1.0` → `1.2.0`): the registry version is
metadata on every diagnostic and inside the #12 validation report by
design, so those version strings update while every rule decision, span,
and message stays identical. Graph failures use the accepted #3 envelope
(`invalid`, exit 1, stderr); graph severity never computes an exit.

### 8. CLI handoff

`lekalo graph show | callers | path | export` only select the project,
symbol, direction, and format; every graph decision lives in the core. The
export is the canonical bytes inside the accepted success envelope; the
optional `--spans` flag adds the sidecar. `graph path` that finds nothing
is an explicit `graph.path-not-found` failure — never an empty success.

## Consequences

- #15/#16/#17 consume exact node lookup, reverse/transitive closures, and
  bounded `GraphSlice`s with explicit completeness and truncation reasons —
  without source rescans.
- #14 builds its typed effect model beside this graph — an independent
  contract reusing these identities, indexes, and traversal APIs instead of
  overloading generic relations; #23 turns scenario `covers` references into `verifies` when
  native evidence exists; #83 providers contribute `extracted` edges only
  through typed bounded envelopes.
- #20 may key caches by the canonical graph digest plus the exact graph, IR,
  Model, and registry versions; the graph itself never writes caches.
- Validation output changes only in the registry-version metadata field
  mandated by the #11/#12 contracts; every validation decision is byte-for-
  byte the one #12 published.

## References

- [docs/graph.md](../graph.md) — the graph surface and guarantees.
- [ADR-0007](0007-ir.md) — the typed IR the graph projects.
- [ADR-0010](0010-diagnostics.md) — the diagnostic contract and registry.
- [ADR-0011](0011-semantic-validation.md) — semantic validation ownership.
