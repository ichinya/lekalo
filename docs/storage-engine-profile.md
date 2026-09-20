# Storage engine profile

- Contract: `contracts/storage-engine-profile.schema.v0.4.0.json`
- Wire discriminator: `lekalo/storage-engine-profile/v0.4.0`
- Contract identity: `dev.lekalo.storage-engine-profile@0.4.0`
- Status: implemented (issue #117)

The storage engine profile is one versioned attachment carrying the exact
capability evidence of **one** storage engine generation. MySQL and
MariaDB are separate profiles, never one optimistic `mysql-family`
profile: real divergences exist in the grammar itself (native `uuid`,
sequences, `json` storage class, `FOR SHARE` syntax), and the issue
forbids merging them without version evidence.

## What the profile carries

The envelope follows the established attachment shape: `schemaVersion`,
`identity`, `attachmentRevision`, `projectId`, `modelRef`, `irRef`.

### Engine identity

```json
{
  "engine": "mysql",
  "engineVersion": "8.0.36",
  "variant": "mysql-community",
  "sqlMode": ["STRICT_TRANS_TABLES", "NO_ZERO_DATE"],
  "defaultStorageEngine": "innodb",
  "charset": "utf8mb4",
  "collation": "utf8mb4_0900_ai_ci",
  "timeZone": "+00:00",
  "evidence": { "kind": "vendor-docs", "ref": "mysql-8.0-en" }
}
```

- `engine` is the closed `mysql | mariadb` token.
- `engineVersion` is one exact `major.minor.patch` release; a range or a
  wildcard is a refusal (`engine-version-range`), never a claim.
- `sqlMode` is the mandatory closed token list; the mode is always
  declared, never implicit.
- `collation` is the declared server collation; the implicit server
  default is never accepted (the MariaDB 11.x mid-release default
  collation change is exactly why).
- `evidence` is one bounded provenance record with the closed kinds
  `vendor-docs | engine-reference | adapter-docs | observed-run`.

### Capability records

Every declared capability is
`{ "support": "full|partial|unsupported", "evidence": {...} }`.
`partial` requires its bounded `bounds` note. An absent capability id is
`unknown` and is never treated as yes (the #24/#29 rule). The closed id
vocabulary spans transactions, isolation, locking (including
`lock.nowait`/`lock.skip_locked`), concurrency, idempotency, index
kinds (descending, functional, invisible), constraint kinds (check,
deferred, exclusion), generated columns, prefix/fulltext indexes,
sequences, collation awareness, pagination, introspection, and the
test lifecycle.

### Test lifecycle

```json
{
  "create": { "support": "full", "evidence": {...} },
  "drop": { "support": "full", "evidence": {...} },
  "isolation": "schema-per-run",
  "production": "forbidden",
  "testSchemaPrefix": "lekalo_test",
  "evidence": {...}
}
```

`production: "forbidden"` is a closed token a strict consumer can gate
on; the test lifecycle may only create and drop ephemeral schemas under
the declared prefix.

### Adapter evidence

The optional `adapters[]` list records runtime-adapter mapping evidence
(closed `drizzle` token with its dialect declaration). Evidence only:
the profile never depends on an adapter and never selects the
application runtime or ORM.

## Boundaries

- The profile is pure declaration and evidence data. It never executes,
  never connects, and never carries credentials, hosts, URLs, or ports.
- It does not replace the coarse component registry: `mysql-sql` and
  `mariadb-sql` component ids remain the composition surface; the
  profile is the exact per-engine evidence beside them.
- `capabilities → CapabilitySnapshot` bridges the profile into
  `transaction_concurrency::map_capabilities`; the strict profile
  blocks on `unsupported`, `unknown`, and unapproved `partial`.
- `portability(from, to)` compares two profiles and reports per-
  capability gains/losses plus the named PostgreSQL-specific semantics
  block — see `lekalo storage-profile portability`.

## Diagnostics

Refusals use the `storage.profile-invalid` (LEK-SEP-001) and
`storage.profile-limit` (LEK-SEP-002) rules; the same-family diff
refusals use `storage.profile-diff-invalid` (LEK-SEP-003).
