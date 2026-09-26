# ADR-0042: The versioned storage engine profile and the checked-mode introspection evidence

Date: 2026-09-19
Status: accepted for issue #117

Custody: this issue carries the MySQL/MariaDB storage component surface
at product version 0.4.0 (`workspace.package.version` in `Cargo.toml`,
both `lekalo` packages in `Cargo.lock` via `cargo update -w`). The two
new contract families are independent attachments by design: the
storage engine profile (`lekalo/storage-engine-profile/v0.4.0`, identity
`dev.lekalo.storage-engine-profile@0.4.0`) and the storage
introspection evidence (`lekalo/storage-observation/v0.4.0`, identity
`dev.lekalo.storage-observation@0.4.0`). The storage-projection
family re-versions to 0.4.0 in the same step (its wire grammar grows
the `mysql`/`mariadb` namespaces, the index kind/prefix/descending
members, and the explicit charset/collation members). The diagnostic
registry takes the reviewed successor instance 0.4.0 with the additive
`storage.profile-*` family LEK-SEP-001..005. Issue #69 (PostgreSQL
profile) may reuse the same shared families in its own worktree; merge
reconciliation is the coordinator's job.

## Context

Issue #117 asks for a reusable MySQL storage component so Lekalo can
check and compare storage semantics in existing MySQL projects without
treating PostgreSQL as the only data model. The existing
storage-projection family (#65) already separates the target-neutral
domain model from per-namespace projections, but the `Namespace` set is
closed at `postgres|laravel`, and nowhere in the repository can exact
engine/version/sql-mode/collation evidence live: the component registry
(#29) deliberately stores only coarse `full|partial` support states.

## Decision

1. **Separate `mysql` and `mariadb` namespaces and profiles.** The
   issue forbids an optimistic merge without version evidence, and real
   divergences exist in the grammar itself (native `uuid`, `sequence`
   generation, `json` storage class, `FOR SHARE` syntax). Two mostly
   identical type tables are an acceptable cost for keeping divergence
   visible and refusable at the wire layer.
2. **A new attachment family, not a registry extension, carries the
   engine capability evidence.** Components stay coarse; the profile is
   attachment-shaped: engine identity (exact version, closed sql-mode
   token list, declared charset/collation/time-zone) plus a closed
   capability map where every record carries `full|partial|unsupported`
   with bounded evidence, `partial` requiring its bounds note, absence
   meaning `unknown` and never yes.
3. **Introspection evidence is its own family, separate from the
   profile.** The profile is reusable per engine; the evidence is
   per-database per-scan and adapter-produced (the #39 observed-scan
   precedent). The grammar refuses credentials, hosts, URLs, and ports;
   `mode` is the constant `checked` and `readOnly` the constant `true`,
   so an unchecked scan is inexpressible. The drift check joins
   projection and evidence at the CLI as pure data.
4. **`timestamp` renders canonically as `datetime(6)`** in both MySQL
   namespaces; the `timestamp` storage type stays available as a
   declared opt-in because MySQL `TIMESTAMP` performs session-time-zone
   conversion with bounded range — the behavior must be a declared
   choice, never implicit.
5. **`serial` is not a type.** It is alias sugar over `bigint unsigned
   AUTO_INCREMENT UNIQUE`; the contract spells `bigint` + `identity`.
   This keeps the vocabulary closed and binds Drizzle's `serial` to the
   honest semantics (the real-world drizzle-orm #3333 MariaDB breakage
   came from emitting the alias literally).
6. **Uniqueness is declared collation-visible.** Derived unique indexes
   over textual columns carry resolved `collation`/`caseSensitivity`
   evidence in the derived projection; a collation change on a column
   participating in a unique index classifies breaking with
   `DataRisk::Destructive`.
7. **The conformance catalog rows are a reviewed increment.** The
   catalog stays closed; if milestone pushback occurs the rows can ship
   in a follow-up while the contract fixtures prove the assertions
   locally.

## Consequences

- `Namespace` grows to four members; every projection table, derivation
  arm, validation rule, and diff path keys off the same closed enum.
- The `storage-engine-profile` module bridges its capability map into
  `transaction_concurrency::map_capabilities` (AC: exact
  isolation/locking evidence), publishes `portability(from, to)` with
  the named PostgreSQL-specific semantics block (AC: portability
  report), and refuses version ranges, merged engines, and implicit
  collations at the wire layer.
- Everything stays read-only and hermetic: no core code path connects
  to a database, and the test lifecycle declares
  `production: "forbidden"` with a schema-prefix grammar.
