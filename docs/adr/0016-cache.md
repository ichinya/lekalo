# ADR-0016: The incremental cache, fingerprints, and selective recomputation

Date: 2026-09-05
Status: accepted for issue #20

Custody: issue #18 published product 0.1.26 (annotated tag `v0.1.26` on
`3710179`); issue #24 published product 0.1.27 (annotated tag `v0.1.27` on
`ef7680d`); issue #25 now carries the **prospective product candidate 0.1.28** in every accepted path (workspace `Cargo.toml`, both `lekalo`
packages in `Cargo.lock` including the regenerated committed golden lock
and its digests, the `--version` behavior and its pinning tests,
`README.md`, `docs/cli.md`); issue #17 published product 0.1.25 (annotated
tag `v0.1.25` on `e627fe5`); issue #16 published product 0.1.24
(annotated tag `v0.1.24` on `b4109e5`); this issue published product 0.1.23
(annotated tag `v0.1.23` on `15be55a`); issue #21 published product
0.1.22 (annotated tag `v0.1.22` on `2dab70e`); issue #14 published
product 0.1.12 (annotated tag `v0.1.12` on `81666da`). The cache contract version
(`lekalo/cache/v1.0.0`, identity `dev.lekalo.cache@1.0.0`) is independent
of the product release, of every Model/IR/graph/effect/lock/protocol
contract version, and of the diagnostic registry by design.

## Context

Issues #7/#8/#13/#14 published the loader, the typed IR, the dependency
graph, and the effect graph. Every command re-reads and re-decodes every
source document on every run; nothing remembers the previous run. Issue
#20 owns the incremental cache: normalized source hashes, parsed module
fragments, IR/graph/effect fragment fingerprints, and dependency-aware
selective invalidation — never a second semantic calculator.

The research briefs (run run_088695f63032: briefs msg_74a270101b0f,
msg_efdb9625894c, msg_7c3cc8176d2e, correction msg_0b79828c5f57, addendum
msg_c0a8b53313a4, reconciliation msg_4a1ad089336c, handoff msg_28b52ab9cfac,
precision msg_f3733593aa79, worker_done msg_5495a302bc32) recorded the
owner decisions this ADR adopts.

## Decision

### 1. Independent closed contract, Lekalo-owned derived data only

The cache publishes its own wire contract,
[`contracts/cache.schema.v1.0.0.json`](../../contracts/cache.schema.v1.0.0.json)
(discriminator `lekalo/cache/v1.0.0`, identity `dev.lekalo.cache@1.0.0`),
covering the closed record envelope and the bounded health projection.
`contracts/` gains exactly this one new file. The cache is Lekalo-owned
derived/cached data under `.lekalo/cache/**` only — never a canonical
source, never promoted, never synced into `lekalo/**`. The historical
`.lekalo/cache.sqlite` placement is denied: the SQLite file is exactly
`.lekalo/cache/cache.sqlite`, with WAL/SHM/temp sidecars and the quarantine
home inside the same governed directory tree. Runtime files are never
model input; the loader never reads them.

### 2. Content digests, not mtime

Every key embeds content digests. A `source` record carries both the raw
SHA-256 of the exact file bytes and the normalized SHA-256 of the canonical
spanned-tree serialization without spans (two spellings with one data tree
share the normalized digest; the raw digest stays the validity authority).
mtime, wall-clock time, and access telemetry never authorize a hit and
never enter semantic bytes. Key digests are SHA-256 over deterministic
canonical bytes — compact UTF-8 JSON with byte-sorted object keys — never
over a path string or database row order.

### 3. Closed record kinds and the typed payload boundary

The closed v1 vocabulary is exactly `source`, `parsed-fragment`,
`ir-fragment`, `graph-fragment`, `effect-fragment`, `adapter-capability`,
`adapter-result`, `artifact-manifest-key`, and `context-key`. The store is
generic over the vocabulary; producers exist for the first five.
`adapter-capability`/`adapter-result` producers wait for accepted
#27/#28/#29; `artifact-manifest-key` waits for #21; `context-key` waits for
#17 — their key shapes exist so those owners plug in without a schema
change. Payloads are bounded typed snapshots — digests, counts, logical
project-relative paths, bounded enums — never source text, raw documents,
provider output, credentials, environment data, host identity, or native
paths. Cache payloads are local-private (privacy policy 1.0.6).

### 4. Storage, integrity, and recovery

The backend is SQLite through `rusqlite` with the bundled SQLite
amalgamation (pinned exact versions, MIT-licensed, MSRV-audited below),
opened on the descriptor-validated physical path inside `.lekalo/cache/`.
A record commits as one durable transaction (`journal_mode=WAL`,
`synchronous=FULL`) writing the entry row and its dependency edge rows
atomically; readers see whole records only. The index table is keyed by
`entry_key_digest` (the canonical key-bytes digest); the dependency table
is `(entry_key_digest, dependency_key_digest, role, ordinal)`; every read
and export uses explicit canonical `ORDER BY`, never rowid or insertion
order. On open the store runs bounded validation: schema/binding metadata,
per-record digest checks (a bad record is a miss, then a recompute), and a
bounded integrity check; global corruption quarantines the whole database
by rename to `.lekalo/cache/quarantine/cache-<digest-prefix>.sqlite`
(never overwriting an existing item, bounded item count) and recreates a
fresh empty store. Cache corruption is non-semantic: it degrades to safe
recomputation with identical outputs; path/authority/privacy violations of
the cache home stay fail-closed `structure.*` denials.

### 5. One writer, bounded waits, safe degradation

Writer serialization is delegated to SQLite (`BEGIN IMMEDIATE` with a
bounded busy timeout); readers run concurrently under WAL. There is no
PID-only stale-lock trust and no lock file to adopt blindly: a process
crash releases the OS lock, and leftover temp artifacts are validated or
quarantined, never silently reused. On a lock timeout the cache degrades
to read-free full recomputation with a typed `locked` health state;
semantic correctness never depends on cache availability.

### 6. Selective invalidation and honest reuse

Every stage key embeds its upstream digests, so a changed source byte
invalidates exactly its own parsed fragment and the downstream closure's
keys; unrelated modules keep their parsed fragments (byte-proven by warm
runs re-decoding only the changed document). Persisted dependency edges
carry the closure for status and for downstream consumers. The v1 hit path
skips the decode stage (the expensive strict frontend parse) by
reconstructing the typed document from the cache-owned spanned-tree
snapshot; IR, graph, and effect projections are recomputed in memory from
the reused documents and recorded as typed fragment digests with counts —
the owner issues publish no decode seam for them, so no byte reuse is
claimed beyond what the store proves. Clean and incremental runs are
byte-for-byte identical for every semantic output; cache state never
enters the semantic projection.

### 7. Deterministic eviction

Eviction is bounded, deterministic, and non-semantic. Limits are checked
before allocation (entries, bytes, payload size, path length, lock wait,
quarantine items); over the limit the store evicts by frozen retention
class — opaque-reference kinds first, `source` last — tie-broken by larger
payload bytes, then lexicographically smaller key digest. No wall-clock
LRU, no access-order dependence, no pinned-entry eviction.

### 8. Diagnostics through the accepted #11 seam, registry unchanged

No registry rules are added (the registry file is unchanged). Cache
corruption, IO failure, and lock timeouts degrade silently to recompute;
the cache home path/link/reparse violations reuse the `structure.*` rules
through the shared constructors (denied, exit 3, stdout); `cache clear`
without the explicit `--yes` flag is a `cli.usage` failure (exit 1,
stderr). `lekalo cache status` is read-only and prints the closed health
projection (`{"status":"valid","cache":{...}}`, exit 0) in both
projections of one `DomainResult`; `lekalo doctor` remains #92's surface.

### 9. CLI handoff

`--no-cache` is a global flag: it bypasses every cache read, write, lock,
and file creation, and its outputs are byte-identical to a clean rebuild.
`load`, `validate`, `graph *`, and `effects *` consult the cache;
`lock`, `update`, `migrate`, and `compatibility` keep their published
behavior untouched. `cache status` and `cache clear --yes` affect only
`.lekalo/cache/**` after containment and reparse checks; the clear never
touches `.lekalo/cache/migrations/**` (issue #9 custody: journals and
immutable backups). No hidden repair,
install, or update exists.

### 10. Dependency audit (SQLite backend)

`rusqlite` `=0.32.1` with `hashlink =0.9.1` and bundled `libsqlite3-sys
=0.30.1` (features `bundled`) are added as exact-pinned `lekalo-core`
dependencies: MIT OR Apache-2.0 / MIT licensed, rust-version 1.70.0 ≤ the
workspace 1.80.0 MSRV, no build scripts beyond the bundled SQLite
amalgamation via `cc`, and the Cargo.lock diff is exactly these packages
plus their transitive closure. The bundled amalgamation keeps the build
hermetic (no system libsqlite3 dependency) on all three CI platforms.

## Consequences

- #27/#28/#29, #21, and #17 add typed producers against the frozen
  key/payload shapes without a schema change; #92 consumes the health
  projection; #91 consumes cache operations in orchestration; #103 may
  project cache facts into reports. None of them recompute or invalidate
  cache entries directly.
- The warm path's module locality is test-enforced: a one-module change
  re-decodes exactly that document and nothing else, and warm/cold/no-cache
  outputs are byte-identical.
- Cache eviction, corruption, and lock states are invisible to every
  published output contract; the cache can always be deleted wholesale.

## References

- [docs/cache.md](../cache.md) — the cache surface, wire, and guarantees.
- [ADR-0006](0006-loader.md) — the loader whose decode stage is reused.
- [ADR-0007](0007-ir.md), [ADR-0012](0012-dependency-graph.md),
  [ADR-0013](0013-effect-graph.md) — the recorded fragment producers.
- [ADR-0003](0003-canonical-structure-and-path-safety.md) — the path
  safety and runtime-placement contract the cache home lives under.
- [ADR-0010](0010-diagnostics.md) — the diagnostic contract.
