# Lekalo incremental cache

Issue #20 adds the incremental cache: content-addressed fingerprints over
the load pipeline, selective invalidation, and the thin `cache status` /
`cache clear` handoff. The cache is Lekalo-owned derived data under
`.lekalo/cache/**` only — disposable at any moment, never a canonical
source, never read as model input. The wire contract is
[`contracts/cache.schema.v1.0.0.json`](../contracts/cache.schema.v1.0.0.json)
(discriminator `lekalo/cache/v1.0.0`, identity `dev.lekalo.cache@1.0.0`);
the recorded owner decisions live in
[ADR-0016](adr/0016-cache.md).

## Commands

```text
lekalo [--no-cache] load [--project DIR] [--spans] [--ir]
lekalo [--no-cache] validate [--project DIR] [--module MODULE] [--strict]
lekalo [--no-cache] graph show|callers|path|export ...
lekalo [--no-cache] effects show|writers|conflicts ...
lekalo cache status
lekalo cache clear --yes
```

`--no-cache` is global. It bypasses every cache read, write, lock, and
file creation — no runtime file is created or touched — and its outputs
are byte-identical to a clean full rebuild. `lock`, `update`, `migrate`,
and `compatibility` do not consult the cache.

### `lekalo cache status`

Read-only. Opens the store without writing and prints the closed health
projection in both projections of the accepted envelope:

```json
{
  "status": "valid",
  "cache": {
    "schemaVersion": "lekalo/cache/v1.0.0",
    "identity": "dev.lekalo.cache@1.0.0",
    "state": "ok",
    "backend": "sqlite",
    "records": [
      { "kind": "source", "count": 8 },
      { "kind": "parsed-fragment", "count": 8 }
    ],
    "dependencyEdges": 16,
    "totalBytes": 24576
  }
}
```

The closed `state` vocabulary is `ok`, `empty`, `missing`, `disabled`,
`corrupt`, `quarantined`, `locked`, `unreadable`. Status carries no raw
paths, host values, or volatile telemetry. A corrupt or locked store is
still a successful status: the command reports, it never repairs.

### `lekalo cache clear --yes`

Clears every cache entry under `.lekalo/cache/**` — the store, its
WAL/SHM sidecars, and the quarantine home — after path containment and
reparse/link checks, with one standing exception: `.lekalo/cache/migrations/`
stays under issue #9's custody and is never touched. `--yes` is required
(the CI-safe explicit confirmation); without it the command is a
`cli.usage` failure (exit 1). A denied or unreadable cache home is a
`structure.*` denial (exit 3). There is no hidden repair, install, or
update.

## Storage layout

```text
.lekalo/cache/
  cache.sqlite            the store (WAL mode; -wal/-shm sidecars)
  quarantine/             whole-file quarantine of corrupt stores
  migrations/             issue #9 custody: journals and immutable
                          backups; never written or cleared here
```

The historical `.lekalo/cache.sqlite` placement is denied, as is any cache
byte outside `.lekalo/cache/**`. The store has two tables: entries keyed
by `entry_key_digest` (the canonical key-bytes digest) and dependency
edges `(entry_key_digest, dependency_key_digest, role, ordinal)`; all
reads and exports use explicit canonical `ORDER BY`, never rowid or
insertion order. One logical update is one durable transaction
(`journal_mode=WAL`, `synchronous=FULL`); writer serialization is
SQLite's `BEGIN IMMEDIATE` under a bounded busy timeout, and readers run
concurrently under WAL.

## Keys, records, and invalidation

Every record is self-describing: `schemaVersion`, `identity`,
`recordKind`, typed `key`, `binding` (the exact consumed producer
contract identities), sorted `dependencies`, bounded typed `payload`,
`payloadDigest`. A key digest is SHA-256 over the canonical key bytes —
compact UTF-8 JSON with byte-sorted keys. Source records carry both the
raw and the normalized content SHA-256; mtime and wall-clock never
authorize a hit.

Because every downstream key embeds its upstream digests, a changed
source byte invalidates exactly its own fragment and the downstream
closure; unrelated modules keep their fragments. A warm run re-decodes
only the changed documents and is byte-identical to a clean rebuild.
Missing, stale, corrupt, or locked cache states always degrade to safe
recomputation — semantic correctness never depends on cache
availability, and cache state never enters any semantic output.

`adapter-capability`, `adapter-result`, `artifact-manifest-key`, and
`context-key` key shapes are frozen for their owners (#27/#28/#29, #21,
#17); the v1 producers are `source`, `parsed-fragment`, `ir-fragment`,
`graph-fragment`, and `effect-fragment`. `lekalo doctor` cache diagnostics
remain issue #92's surface.

## Guarantees and limits

- Atomic commit per record; readers never observe half a record or half
  an edge set. A crash leaves either the previous or the new state.
- Per-record digest validation demotes a bad entry to a miss; global
  corruption quarantines the whole database file under
  `.lekalo/cache/quarantine/` (bounded, never overwriting) and recreates
  an empty store.
- Eviction is deterministic and non-semantic: frozen retention classes
  (opaque-reference kinds first, `source` last), payload-size then key
  digest tie-break, no wall-clock LRU.
- Bounded before allocation: path lengths, record and payload sizes,
  entry and edge counts, quarantine items, lock wait.
- Cache payloads are local-private: typed ids, logical paths, bounded
  enums and counts, digests. Never source text, raw documents,
  credentials, environment or host data, or provider transcripts.
