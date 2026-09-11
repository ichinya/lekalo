# Binding registry (`lekalo scan`, `lekalo bindings`)

Issue #42 collects target evidence about existing code and relates native
symbols, files, endpoints, and tests to Lekalo semantic ids without
turning a heuristic into a fact. The observed index (issue #39) is the
binding registry: `lekalo scan` fills it through a target adapter, and
`lekalo bindings` proposes, confirms, lists, and audits its rows. The
design decision is [ADR-0035](adr/0035-bindings-registry.md).

The wire extensions are additive successors of the frozen issue #39
contracts: [`contracts/observed-scan.schema.v1.1.0.json`](../contracts/observed-scan.schema.v1.1.0.json)
and [`contracts/observed-index.schema.v1.1.0.json`](../contracts/observed-index.schema.v1.1.0.json).
The 1.0.0 documents stay published and their fixture-era documents still
decode. Both versions remain independent of the product release, of the
Model/IR/protocol contract versions, of each other, and of the diagnostic
registry version.

## The binding data

Every registry row carries the closed issue vocabulary:

- the semantic id and the definition kind;
- the target and adapter profile of the producing scan (set once: a later
  scan naming a different target refuses);
- the native symbol identity (the adapter's stable key, e.g.
  `src/tasks.ts#createTask`);
- the relative path and declaration line;
- signature evidence and the source fingerprint;
- the relation: symbol bindings `implements`, endpoint bindings `exposes`,
  native test bindings `verifies`;
- the source: `explicit` (user-declared), `user-confirmed` (a confirmed
  adapter mapping), `detected` (an adapter mapping at a recorded
  location), or `inferred` (an adapter mapping without one);
- the confidence word, the adapter id, and the exact scan revision;
- the freshness state (`current`/`stale`/`unknown`).

## Commands

```text
lekalo scan --target node-typescript [--profile PROFILE] [--timeout-ms MS] [--project DIR] PROGRAM [ARGS]...
lekalo bindings list [--project DIR]
lekalo bindings propose [--project DIR]
lekalo bindings confirm PROPOSAL [--candidate NATIVE] [--project DIR]
lekalo bindings confirm --batch --preview [--project DIR]
lekalo bindings confirm --batch --confirm sha256:PLAN_ID [--project DIR]
lekalo bindings audit [--project DIR]
```

`lekalo scan` discovers and selects the adapter through the accepted
#27/#28 protocol (safe describe handshake, strict capability policy — the
scanner must declare `scan.symbols: full` and the requested target), runs
the read-only `scan` exchange inside the confined sandbox, and merges the
inventory through the accepted #39 merge rules. The reference
node-typescript scanner (`tests/fixtures/bindings/ts-scanner.mjs`) is a
real native adapter: it extracts exported classes, functions, interfaces,
types, enums, and constants with stable keys and proposed semantic ids.
Any future PHP/Go adapter speaks the same protocol and gets the same
pipeline.

`bindings list` projects the whole registry with the data above.
`bindings propose` derives one deterministic proposal (`prop-<64 hex>`)
per inferred binding. `bindings confirm` turns inferred bindings into
user-confirmed facts; `bindings audit` is the staleness gate.

## Ambiguity, confirmation, and freshness

- **Several candidates are never resolved by first match.** When an
  adapter (or the core's grouping of duplicate proposals) finds two or
  more plausible native identities for one semantic id, the binding
  records the whole candidate set and no location. The proposal is
  flagged `ambiguous`; confirming it without `--candidate` refuses with
  the registered `bindings.ambiguous` rule, and the named candidate must
  be a member of the set. The choice is always the user's.
- **An inferred binding never becomes confirmed automatically.** Only
  `bindings confirm` changes the status, and provenance survives it:
  origin, adapter, and the exact scan revision stay on the record.
  Explicit bindings have priority — a scan never downgrades a user-owned
  fact, never erases its recorded location, and never lets it gather
  proposals or candidates.
- **Batch confirmation is planned and confirmed.** `--batch --preview`
  plans every unambiguous proposal and prints the plan identity
  (`sha256:<64 lowercase hex>`); `--batch --confirm` recomputes the plan
  and refuses on any drift (`bindings.plan-mismatch`), so a batch can
  never confirm anything its preview did not name. Ambiguous proposals
  are excluded from every batch.
- **Freshness is byte truth.** Fingerprints — including on native test
  bindings — are computed by the core from the real tree. `bindings
  audit` re-fingerprints everything after source changes: a changed
  signature or path is stale (exit 1 with the registered
  `observed.stale-binding` diagnostics) or correctly re-resolved through
  stable keys, and evidence-free bindings are `unknown`, never stale.
- **No writes, no leaks.** The scan never writes a source file (the
  operation declares no writes and the sandbox grants none), sensitive
  paths are excluded by the adapter's minimal read scopes and closed skip
  list, and the registry can only reference paths the confined view
  actually contained.

Adapter evidence lives in the registry under the accepted
`lekalo.observed-model-draft` authority home (`.lekalo/import/**`),
outside the canonical Model; only the #39 promotion workflow moves
confirmed facts into the canonical model.

## Diagnostics

The bindings family is three additive rules of the reserved registry
1.20.0, over the frozen 1.16.0 predecessor: `bindings.proposal-unknown`
(`LEK-BND-001`), `bindings.ambiguous` (`LEK-BND-002`), and
`bindings.plan-mismatch` (`LEK-BND-003`). Every other refusal keeps its
accepted family: protocol and selection failures project their registered
`target.*` rules, and merge or wire violations project the `observed.*`
family.
