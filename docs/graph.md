# The dependency graph of semantic symbols

Issue #13 implements a read-only, deterministic dependency graph over the
accepted #8 typed IR: direct edges with closed provenance and confidence,
precomputed reverse indexes, relation-filtered traversal, bounded slices,
shortest paths, kind-specific cycle policy, module-boundary analysis, and
canonical export. The owner decisions and their rationale live in
[ADR-0012](adr/0012-dependency-graph.md).

The graph never reads files, never re-parses source, never validates
semantics, and never writes anything: it consumes the compiled IR and
answers queries from in-memory indexes. Reverse lookups answer from the
precomputed incoming index — never a source rescan or an IR rebuild.

## Contract identity

- Discriminator: `lekalo/graph/v1.0.0`
- Identity: `dev.lekalo.graph@1.0.0`
- Schema: [`contracts/graph.schema.v1.0.0.json`](../contracts/graph.schema.v1.0.0.json)
- Independent of the product release, the Model/IR/protocol versions, and
  the diagnostic registry version.

## Nodes and relations

Twelve core node kinds — `project`, `module`, `type`
(`scalar`/`enum`/`value-object`), `entity`, `operation`
(`command`/`query`), `policy`, `event`, `effect`, `endpoint`, `scenario`,
`target-binding`, `requirement` — with kind-qualified ids such as
`entity:planner.task`. Twelve core relations; the emission table below is
exactly what the accepted IR represents:

```text
requires      module imports (module -> module)
references    field/payload type leaves, command effects,
              effect entities, scenario covers
accepts       command input type leaves
returns       query return type leaves
reads         query reads
emits         declared effect emits
authorizes    policy applies_to
exposes       endpoint invokes
derived_from  requirement provenance (project and definitions)
```

`writes`, `implements`, and `verifies` are registered but not emitted until
#14, #22/#27/#29, and #23 supply accepted typed evidence. Nothing is
inferred from names, target files, or source scans.

## Guarantees

- **Deterministic**: nodes sort by kind rank, module, semantic id,
  subkind; edges by endpoints, relation, occurrence, provenance kind. The
  same IR always yields byte-identical canonical export, whatever the
  frontend, directory order, locale, or platform was.
- **Path-independent**: canonical bytes carry no physical roots, raw
  source, timestamps, or host state. Source locations resolve only through
  the #8 source map, on explicit request, as the `--spans` sidecar.
- **Closed**: provenance is a closed sum (canonical IR, typed adapter
  evidence, derived) and confidence is a closed vocabulary — `canonical`,
  `verified`, `extracted`, `inferred`, `unknown` — never a guessed score.
  Repeated identical references at different source sites stay separate
  edges through their occurrence ordinals.
- **Bounded**: 100000 nodes, 1000000 direct edges, depth 256, 50000 result
  nodes, 250000 result edges, 256 path nodes, 32 MiB export, 128 filter
  terms, 8 provenance records per edge. Bounds reject with explicit
  diagnostics instead of truncating.
- **Cycle policy**: `requires` and `derived_from` are acyclic — any cycle
  is a fatal `graph.cycle-forbidden`. Every other relation may cycle and
  stays traversal-safe.
- **Typed failures**: every failure is one registered `graph.*` rule
  (`LEK-GRAPH-001`–`008`) projected through the accepted #11 envelope on
  the accepted `0/1/3/4/5` exit classes.

## CLI

```sh
lekalo graph show planner.focus_task            # node + direct deps/dependents
lekalo graph callers planner.task_focused       # reverse dependencies
lekalo graph callers planner.task --transitive  # bounded reverse closure
lekalo graph path planner.api_focus planner.task_focused
lekalo graph export --project DIR --format json # canonical bytes
lekalo graph export --spans                     # + declaration-span sidecar
```

Every subcommand accepts `--project DIR` (default: the invocation
directory, or `LEKALO_PROJECT`), `--json`, and the global `--help`.
Successes exit 0 on stdout; `graph.unknown-node`,
`graph.unknown-relation`, `graph.path-not-found`, `graph.traversal-limit`,
and `graph.cycle-forbidden` exit 1 on stderr as normalized envelopes.

## Library surface

The `lekalo_core::graph` module is the reusable engine (#15 inspect, #16
impact, #17 context, #77 coupling lint consume it without the CLI):

```rust
use lekalo_core::graph::{build, Direction, TraversalSpec};

let graph = build(&compilation.project)?;
let root = graph.resolve_id("planner.focus_task").unwrap();
let closure = graph.transitive(root, &TraversalSpec::new(Direction::Reverse))?;
let export = graph.to_canonical_json()?; // canonical bytes, size-guarded
```

Queries are side-effect free; `GraphSlice` reports `complete` only when the
closure was fully explored and otherwise carries the truncation reason and
frontier count — it never silently returns the first N results.

## Fixtures and gates

- Hermetic planner fixture: `tests/fixtures/graph/planner/` (two modules,
  cross-module `requires`, every core kind, repeated type occurrences,
  requirement provenance).
- Pinned golden: `tests/fixtures/graph/golden/planner.graph.json`.
- Independent Node gate: `node scripts/test-graph-contracts.mjs` — the
  exact Ajv 8.17.1 schema gate plus order, coherence, and acyclic-policy
  invariants, run in CI on Node 18 and 24.
- Rust suites: `crates/lekalo-core/tests/graph.rs` (loader-seam
  integration) and the `graph::cycle` internal unit tests (cycle policy on
  hand-assembled graphs).
