# The Lekalo effect graph

Issue #14 projects the deterministic effect graph of operations: which
operations read an entity or field, which create, update, or delete it,
which events and jobs they emit, and — through typed detected evidence —
where an operation reaches external services, caches, outputs, audit
sinks, or job queues. The core owns every decision (projection, evidence
attachment, comparison, conflicts, limits, diagnostics); the binary only
selects, renders, and maps exits onto the accepted 0/1 envelope. See
[ADR-0013](adr/0013-effect-graph.md) for the recorded decisions.

## Contract

The effect graph publishes its own closed contract,
`contracts/effect-graph.schema.v1.0.0.json` (discriminator
`lekalo/effects/v1.0.0`, identity `dev.lekalo.effects@1.0.0`), independent
of the product release and of every other contract family. Declared
effects are projected deterministically from the accepted typed IR;
detected effects enter only as typed evidence records and never mutate the
canonical model. Canonical export bytes are compact UTF-8 JSON with
byte-sorted keys, path-independent and byte-identical for the same IR.

## Effect kinds

Closed v1 kinds: `read`, `create`, `update`, `delete`, `write-field`
(with the closed action `set`/`clear`/`append`/`replace`/`merge`),
`emit-event`, `enqueue-job`, `external-call`, `cache-read`,
`cache-write`, `cache-invalidate`, `publish-output`, `audit-log`, and
`transaction-boundary`. Entity and field scope are distinct everywhere.
The Model can declare only what it has grammar for: query `reads`
(entity reads), command `effects` (CRUD on the effect's entity), and
effect `emits` (event emissions). Every other kind enters only through
typed detected evidence supplied by an adapter; this tooling never
detects effects from source and never infers them from names.

## Declared versus detected

Comparison joins both sets by their typed canonical identity and
classifies every difference into one closed state (`declared-only`,
`detected-only`, `matched`, `action-mismatch`, `scope-mismatch`,
`stale-evidence`, `unknown-evidence`, `unsupported-capability`,
`conflicting-evidence`) with a stable explanation. Unknown or stale
evidence stays visibly degraded and never collapses into `matched`.
Mismatch states are result data with bounded tokens, not diagnostics.

## Commands

```text
lekalo effects show OPERATION [--project DIR]
lekalo effects writers RESOURCE [--readers] [--project DIR]
lekalo effects conflicts --changed OPERATIONS [--project DIR]
```

- `effects show` — one operation's declared and detected edges. An
  unknown operation is an explicit `graph.unknown-node` failure
  (`LEK-GRAPH-007`, exit 1 on stderr), never an empty success.
- `effects writers` — the reverse view: operations with a direct write
  on the subject (or reads with `--readers`). The selector accepts an
  entity semantic id (`planner.task`), an exact field scope
  (`planner.task.title`), or a typed reference
  (`cache:vendor.app.key`). A known resource with no matching effects is
  an empty success.
- `effects conflicts --changed` — classify parallel-change conflicts for
  the explicitly supplied change set (comma-separated operation ids).
  The change set is a typed handoff; the tool never parses Git or infers
  changed symbols. A delete overlapping anything on its resource is
  `delete-overlap`, writer/writer is `definite-write-write`, reader/writer
  is `potential-read-write`, independent fields never conflict, and
  degraded evidence yields `unknown`, never a silent no-conflict.

## Example

```json
{
  "status": "valid",
  "effects": {
    "complete": true,
    "edges": [
      {
        "action": null,
        "confidence": "canonical",
        "effect": "effect:planner.create_task",
        "field": null,
        "kind": "create",
        "occurrence": 0,
        "operation": "operation:planner.focus_task",
        "provenance": {
          "irDigest": "sha256:…",
          "occurrence": 0,
          "role": "command-effect",
          "symbol": "planner.focus_task",
          "type": "canonical-ir"
        },
        "resource": { "id": "planner.task", "kind": "canonical" },
        "sensitivity": null,
        "transactionGroup": null
      }
    ],
    "identity": "dev.lekalo.effects@1.0.0",
    "operation": "operation:planner.focus_task"
  }
}
```

## Boundaries

Transaction groups are descriptive boundaries only; atomicity, isolation,
retry, and compensation semantics are #24. Sensitivity markers are opaque
references; actor, scope, authorization, and gate policy are #25.
Detected evidence is validated for shape and binding; adapter production,
protocol transport, and capability discovery are #27/#28/#29. Impact
analysis and parallel-change scheduling consume this graph's facts — they
are #16/#20 concerns. The effect graph never writes anything anywhere.
