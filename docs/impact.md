# Impact and change-radius analysis

`lekalo impact` answers one question before any edit happens: if this
changes, what else changes with it? It is a read-only projection over the
accepted #13 dependency graph and #14 effect graph; it never parses Git
inside the core, never rescans source, and never writes anything.

Contract: `lekalo/impact/v1.0.0` (identity `dev.lekalo.impact@1.0.0`,
algorithm `dev.lekalo.impact.algorithm@1.0.0`), pinned in
[`contracts/impact.schema.v1.0.0.json`](../contracts/impact.schema.v1.0.0.json).
The contract version is independent of the product release, of the
Model/IR/graph/effect versions, and of the diagnostic registry
([ADR-0017](adr/0017-impact.md)).

## Commands

```text
lekalo impact SYMBOL [--depth N] [--module ID] [--target ID] [--relation KIND] [--profile default|strict] [--project DIR]
lekalo impact --changed [--base REF] [--head REF] [--worktree] [--depth N] [...] [--project DIR]
```

- Symbol mode roots the radius at one semantic id; the default depth is 3
  (the depth the issue examples use) and the maximum is 256.
- `--changed` is the only Git mode and is exclusive with a positional
  symbol. The default candidate is the current index/worktree against
  HEAD; `--base REF` selects a committed base, `--head REF` a committed
  candidate (exclusive with `--worktree`), and a bare `--base` compares
  the base with HEAD. Revisions are resolved read-only into opaque commit
  identities; refs themselves never appear in any output.
- Exit protocol: valid or degraded 0 stdout, invalid 1 stderr, strict
  denial 3 stdout; loader/IR failures pass through untouched.

## The result

Every payload carries, in frozen schema order: the contract identity, the
input (mode, opaque revision references, changed-input digest), the
request (depth, filters, profile), the roots, the three radius sections,
the risk vector, the affected targets, artifacts, scenarios, and tests,
the gate selection, bounded explanation paths, evidence, completeness,
non-blocking diagnostic references, and the digest.

- `direct`, `transitive`, `mandatoryPublic` hold the affected subjects
  with scope, distance, stable reason references, explanation path
  references, evidence state, confidence, fired risk dimensions, and the
  gates each item requires. `mandatoryPublic` is a depth-free closure
  over public and structural symbols: a low depth can never hide public
  impact.
- `risks` is the closed nine-dimension vector. Facts come only from
  canonical data; absent, stale, or unsupported evidence is `unknown` /
  `stale` / `unsupported` — never safe. Migration and public-contract
  risks are first-class dimensions and are `required` where the evidence
  says so.
- `gates` is a neutral selection (`impact.gate.*`): semantic validation is
  always required, every fired dimension adds its required gate, and each
  row carries its evidence state. `--profile strict` turns required
  unknown/stale evidence into `blocked` and denies the whole result
  (exit 3); `--profile default` keeps it a visible warning.
- `explanations` are semantic discovery chains: ordered canonical edge
  keys with relations, the confidence meet, and provenance kinds — never
  filesystem paths.
- `completeness` is never optimistic: missing detected-effect evidence,
  unresolvable changed inputs, or depth truncation outside the
  mandatory-public closure mark the result `incomplete` with bounded
  frontier and omitted counts.

## Canonical bytes and digest

Canonical impact bytes are compact UTF-8 JSON with object keys in schema
order and set-like arrays sorted by unsigned UTF-8 of their typed keys.
The digest is the SHA-256 of the canonical bytes with the digest field
empty; the CLI appends exactly one LF. Equivalent inputs produce
byte-identical output on every platform, and the pinned golden export is
gate-checked with exact Ajv 8.17.1 on Node 18 and 24
(`scripts/test-impact-contracts.mjs`).

## The typed Git handoff

The only Git-aware code is the CLI-edge adapter. It runs read-only Git
through argv (no shell), pins `--no-renames` so physical renames are
deterministic delete/add pairs, validates every changed path against the
closed logical-path grammar, and resolves symbols through the #8 source
map. A changed path with no semantic surface becomes an explicit
unknown-evidence entry that degrades completeness — never an empty change
set. Patch text, stderr, repository identity, and physical paths never
survive into a handoff or a diagnostic.

## Limits and privacy

Owner-approved v1 caps: 256 depth, 50000 items, 250000 path-edge
references, 50000 entries, 128 roots, 128 filter terms, 8 provenance
records, 32 MiB export. Every bound rejects with a typed diagnostic
instead of truncating. Impact output contains only validated semantic
ids, typed relation/effect/provenance ids, opaque digests, and the closed
reason vocabulary — no raw source, no patch text, no repository identity,
no physical paths, no secrets, no runtime values.

## References

- [ADR-0017](adr/0017-impact.md) — the recorded owner decisions.
- [graph.md](graph.md) — the dependency graph and traversal contracts.
- [effect-graph.md](effect-graph.md) — the effect projection.
- [diagnostics.md](diagnostics.md) — the diagnostic contract and registry.
