# ADR-0044: Laravel storage migrations from the storage projection

- Status: accepted
- Date: 2026-09-27
- Issue: #57 (M5)
- Depends on: #69 (postgres storage engine), #65 (storage projection),
  #54 (php-laravel adapter), #24 (transaction concurrency)

## Context

Issue #57 asks for safe Laravel migrations derived from the
language-independent storage model and its semantic diff. The engine
layer already derives the deterministic PostgreSQL DDL document
(#69), the gated migration plan, the drift comparison, and the
runtime-neutral engine input. The Laravel *namespace* projection
(#65) is declared data — it is not a migration emitter, and it must
not become a second SQL planner.

## Decisions

1. **SQL-backed migrations, one planner.** The PHP adapter emits
   Laravel migration classes whose `up()` executes exactly the
   statements the core migration planner produced, one `DB::statement`
   per step. The adapter never re-derives SQL, never reorders steps,
   and never converts types: PostgreSQL precision, defaults, quoted
   identifiers, partial indexes, and `timestamptz` semantics survive
   verbatim. The Laravel namespace projection stays a portability
   report, not generation input; mixing `postgres` and `laravel`
   namespace names is a refusal.
2. **Generation is not execution.** `generate` produces bytes only —
   no artisan, no subprocess, no network, no database connection.
   Applying migrations against a real database is the fixture
   harness's explicit job, and the adapter never claims production
   execution support (`plan-native` stays unsupported, matching #54).
3. **Effective gate = destructive ∪ backfill.** The engine plan gates
   destructive steps only. The Laravel generation policy unions that
   gate with `backfill_required` steps: any plan carrying either risk
   stays blocked until the caller names the exact `planId`. A blocked
   generation writes zero bytes.
4. **Rename history is validated evidence, not inference.** Storage
   renames travel in a closed, versioned history attachment
   (`StorageRenameMap`): project/base/candidate digests, the entity,
   the old and new physical names, and the semantic-history reference.
   The planner reuses it only after validation — the old name exists,
   the new name is unoccupied, no chains or cycles. Renaming a Model
   symbol never renames a table; without evidence a rename stays a
   destructive drop+add pair.
5. **Rollback is classified, never reversed text.** Every plan step is
   classified `reversible`, `data-loss-on-rollback`, or
   `irreversible` from typed metadata (never by regex-parsing SQL).
   `down()` renders from the typed reverse plan only; unsafe or
   unclassifiable steps refuse before the first statement. Schema
   reversibility is not data recovery: restoring a dropped column
   does not return its rows.
6. **Bounded input document.** Generation consumes one canonical
   `laravel-migration-input` document — plan, effective gate, rename
   map, rollback classification, engine pin, and the digests of every
   input — closed by contract
   `contracts/laravel-migration-input.schema.v0.4.0.json` and
   rendered with the same byte-sorted canonical JSON discipline as
   the engine plan. No credentials, hosts, or URLs.
7. **Append-only published migrations.** Generated artifacts live
   under `.lekalo/generated/php-laravel/<target>/migrations/`; the
   ledger records every published file's digest and plan. Published
   migration files are append-only: byte-identical regeneration is a
   no-op, a changed or missing published file refuses, and clean
   planning skips retained migration files.
8. **Timestamps come from policy, never the wall clock.** Migration
   filenames carry one declared UTC timestamp base plus the input's
   short digest; two runs with identical inputs are byte-identical.
   The filename carries no semantic ordinal: publication sequence is
   the ledger's append-only ordinal, not alphabetical filename order,
   and the digest suffix exists to make each published file
   content-addressed, not to sequence execution. Plans that must
   compose with already-published migrations are generated and applied
   as one composed input, never sequenced by digest.

## Consequences

- The adapter stays dependency-free (PHP built-ins only), confined to
  `.lekalo/generated/php-laravel/**` writes, and single-file packaged.
- Real-Database acceptance (Laratesto scenario runs) needs an
  installed Laravel fixture with PostgreSQL; until that fixture lands,
  the executable proof is the Node harness applying the emitted SQL to
  a disposable PostgreSQL container and comparing the resulting
  catalog against the projection.
- Schema-version families touched by this ADR publish at the current
  product version per the release policy.
