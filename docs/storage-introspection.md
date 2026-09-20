# Storage introspection

- Contract: `contracts/storage-introspection.schema.v0.4.0.json`
- Wire discriminator: `lekalo/storage-introspection/v0.4.0`
- Contract identity: `dev.lekalo.storage-introspection@0.4.0`
- Status: implemented (issue #117)

One adapter-produced, read-only schema introspection of one explicitly
configured test schema in explicit checked mode. The runtime adapter —
never the core — runs the read-only information-schema queries against
the named test schema and emits this document; the core validates it
and compares it against a declared projection. The core never speaks to
a database.

## The checked-mode flow

1. The runtime adapter runs read-only queries over
   `information_schema.{TABLES,COLUMNS,STATISTICS,KEY_COLUMN_USAGE,...}`
   plus the exact server variables (`@@version`, `@@sql_mode`,
   `@@character_set_server`, `@@collation_server`, `@@time_zone`,
   `@@default_storage_engine`) against one explicitly named **test
   schema** — the `scan.schema` capability operation.
2. The adapter emits one `lekalo/storage-introspection/v0.4.0`
   document: `mode: "checked"`, `readOnly: true`, the exact engine
   identity echo, the one bound `testSchema`, the observed tables with
   columns/indexes/foreign keys, and the `observedDigest` over the
   canonical observed payload.
3. `lekalo storage introspect-check --projection P --evidence E` runs
   the pure drift comparison: declared-vs-observed per
   table/column/index/collation with the closed drift kinds
   (`missing-table`, `type-mismatch`, `collation-mismatch`,
   `missing-index`, `engine-mismatch`, `sql-mode-mismatch`,
   `version-mismatch`). Drift is data, never a guessed repair.

## Denials

- Credentials, hosts, URLs, ports, users, passwords, and data source
  names are inexpressible: the closed member sets refuse them before
  they can serialize.
- The grammar carries no unchecked form: `mode` is the constant
  `checked`, `readOnly` the constant `true`.
- The evidence binds exactly one test schema name; there is no
  connection string anywhere in the family.
- Evidence absent means `unknown` in any consumer, never clean.

## Diagnostics

Wire and bound refusals use `storage.introspection-invalid`
(LEK-SEP-004); a malformed comparison input uses
`storage.introspection-diff-invalid` (LEK-SEP-005).
