# Issue #116 implementation report — Drizzle ORM bindings for the Node/TypeScript target

Implemented on `ichinya/m6-issue-116` (worktree `m6-issue-116`).
Implementation commits: `fbd736a7` (vendored declarations + resolution
seam), `9d905f8b` (extractor, paired dialect fixtures, gates), plus the
docs commit carrying this file. Design rationale lives in
`docs/drizzle-bindings.md`; the research baseline is
`docs/m6/issue-116-research.md`.

## Delivered surface

- `adapters/node-typescript/build.mjs` — deterministic build-time
  embedding of the pinned upstream declaration closure
  (`drizzle-orm@0.44.7`, 303 `.d.ts` files, closure digest
  `sha256:ef22631e127ec6531b24da5da2b30bb0045c8bf5510d7cca64d1e78ecccf320a`)
  with a version-drift refusal; artifact regenerated and manifest
  golden updated (entry `sha256:93ed2de682366404b2471189807c543b1de8bf28cd023607758382987e5fc60e`,
  14,613,981 bytes).
- `adapters/node-typescript/src/kernel.mjs` — the
  `__attachDrizzleDeclarations` seam (identity-checked attachment) and
  the `embeddedDrizzleDeclarations()` getter.
- `adapters/node-typescript/src/scanner.mjs` — restricted-host serving
  of `/lekalo/deps/**` (fileExists/readFile/directoryExists), the
  pin-gated `paths` mapping onto exact public subpaths only, and the
  `attachDrizzleEvidence` pass appended to `runScan` with an internal
  evidence summary in the scan outcome envelope (never the wire
  detail).
- `adapters/node-typescript/src/drizzle-evidence.mjs` — the extractor:
  attachment decision, closure mapping, tables/columns/constraints,
  relations, queries/effects, transactions, migrations, scope,
  bindings decode, projection comparison, staleness audit, document
  assembly and digest. Closed limitation vocabulary (57 codes) and
  closed bounds on every array.
- `adapters/node-typescript/test/drizzle-evidence.test.mjs` (19 tests)
  + `test/drizzle-helpers.mjs`; gate `scripts/test-node-drizzle.mjs`,
  registered in `.github/workflows/ci.yml` next to the scanner suite.
- Fixtures `tests/fixtures/node-typescript-drizzle/{postgres,mysql}/`
  with owner inputs (`drizzle.bindings.json`,
  `drizzle.projection.json`), migration folders, and planted
  divergences; `README.md` documents the honesty markers.
- `adapters/node-typescript/package.json` — `drizzle-orm@0.44.7` as an
  exact build-time devDependency (custody only, poison scripts intact,
  never resolved at scan time); `THIRD_PARTY_NOTICES.md` carries the
  Apache-2.0 attribution.

## Acceptance criteria mapping

### AC1 — A Drizzle table can be bound to a Lekalo storage projection

Evidence: `drizzle.bindings.json` (owner input, closed decoder) is
validated against extracted tables; confirmed bindings carry the table
native id and file digest; unknown tables stay `unresolved`. The
supplied `drizzle.projection.json` is compared per column with
explicit findings. Tests: "owner-supplied bindings confirm against
extracted tables..." and "schema and query edits change the evidence
digest..." (`adapters/node-typescript/test/drizzle-evidence.test.mjs`).
The planted divergences are asserted exactly: `email`
`varchar(200)` declared vs `varchar(255)` projected reports
`type-divergent`; `legacy_flag` reports `missing-in-declaration`;
unprojected tables report `missing-in-projection`. Implementation
doc note: this is the adapter-side declaration-vs-projection check;
the host-side `verify.schema-projection` operation keeps its existing
introspection semantics and was not redefined.

### AC2 — Relations/cardinality/index constraints export with source spans

Evidence: constraints rows carry `kind`, `name`, ordered `members`
(physical names), `unique`, referenced targets with
`onDelete`/`onUpdate`, and 1-based spans that the tests resolve back
into fixture source bytes (the unique-index span line contains
`users_email_tenant`). Relations carry source/target table natives,
`fields`/`references` pairs, cardinality (`one-to-many`,
`one-to-one`, or `one-to-many-or-one-to-one` when uniqueness is
unproven) and spans. Tests: "postgres: constraints extract..." and
"postgres: relations export endpoints...".

### AC3 — Static insert/update/delete reflect in field-level effect evidence

Evidence: query rows carry `reads` (role: projection/predicate/join/
order/returning), `writes` (values/set keys), and `effects` rows with
neutral actions (`read`/`create`/`update`/`delete`) at
`confidence: "extracted"`, `canonical: false`. A bare `select()`
reads all declared columns; dynamic `values`/`set` keep the
entity-level effect and record `dynamic-values`/`dynamic-set` instead
of inventing fields; delete carries no per-field clears. Tests:
"postgres: select/insert/update/delete yield field-level effect
evidence...". Core effect-graph ingestion remains successor work
(marked partial below).

### AC4 — Raw SQL and dynamic construction give explicit completeness warnings

Evidence: limitation codes `raw-sql` (SQL-tagged defaults and
predicates, computed projections, `db.execute`),
`dynamic-builder` (`$dynamic()`), `dynamic-values`/`dynamic-set`,
`receiver-unknown`, `builder-not-executed`, `alias-continuation`,
`upsert-alternative`, and per-query rows that never claim a shape
they did not see (raw executes produce the warning and no fabricated
query row). Section completeness degrades to `partial` over any
unknown; the document state and per-section map are asserted in
"evidence is deterministic and completeness degrades over unknowns".

### AC5 — Transaction evidence is visible

Evidence: transaction rows with member query ids, parent/nested
links, rollback spans, `conditional-flow`, `query-not-tx-bound`,
`tx-escaped`, `nested-transaction` limitations; members resolve to
real query rows whose `transactionId` links back. Tests: "postgres:
transaction groups carry members, nesting, rollback, and escape
honesty". **Partial (explicit):** visibility inside the core
`inspect`/`impact`/`context` commands requires the protocol successor
transport and host attachment described in the research plan (§4,
§6-S4); today the evidence exists, is digest-stable, and is tested at
the adapter boundary, but the CLI does not render it. Not claimed as
done.

### AC6 — Schema/query change invalidates stale bindings

Evidence: every document carries the input manifest revision and a
digest; unchanged inputs rescan byte-identical; a column-type edit
changes both and `auditStaleBindings` marks previously confirmed
bindings `binding-stale-table-source-changed`; an export rename
changes the native identity and yields
`binding-stale-table-native-missing` with no nearest-name rebinding.
Tests: "schema and query edits change the evidence digest; stale
bindings audit marks the damage" (also covers cold/warm parity of the
whole `index.drizzle` document).

### AC7 — Adapter works read-only without DB credentials

Evidence: `identity.readOnly: true` in the document; the scan
pipeline never opens sockets or subprocesses (kernel launches are the
core's confinement responsibility; the adapter makes no network or
child-process calls); the evidence text contains no
credential/connection surface (asserted against a banned-token list).
Test: "scanning is read-only: the project tree is byte-identical
before and after" — the materialized fixture tree is snapshotted
before and after two rescans and must be equal. Migration configs
escaping the root are recorded as limitations without being read
("migration config escaping the project root...").

### AC8 — MySQL/PostgreSQL dialect coverage without core leakage

Evidence: paired fixtures cover uuid/defaultRandom vs
int-autoincrement, mysql `serial` alias sugar (recorded as the
`mysql-serial-alias` limitation, never folded into a storage type),
`datetime(6)`/`timestamp(6)` precision, jsonb vs json,
`$returningId` (projection `primary-keys`) vs `.returning({...})`,
`onDuplicateKeyUpdate` vs `onConflictDoUpdate`, and folder-journal
layouts in both dialects. Tests: "mysql: dialect facts stay
target-side..." and "dialect parity: neutral document shape matches
across fixtures; target facts differ" — the same schema identity,
section vocabulary, effect actions, binding semantics, and scope
kinds across dialects, with type tokens differing exactly where the
dialects differ. Nothing dialect-specific leaves the target evidence
document: no core Model/IR file was touched by this implementation.

## Reliability requirements mapping

| Requirement | Where satisfied |
| --- | --- |
| Compiler info + Drizzle declarations as primary evidence | Closure identity via `symbolInClosure` on checker-resolved callees; alias symbols resolved; receiver identity via declaration-initializer chain or closure type. Name similarity alone never extracts. |
| Raw SQL/dynamic = partial/unknown with explicit warning | Closed limitation vocabulary + per-query limitations + section completeness degradation (AC4). |
| Declared schema ≠ live DB state | `completeness.databaseState: "unknown"`; projection comparison labelled declaration-vs-projection. |
| Detected effects are evidence, not canonical declarations | `confidence: "extracted"`, `canonical: false` on every query; asserted for every row. |
| Rename invalidates binding or uses confirmed rename history | Native identity includes export+physical name; `auditStaleBindings` reasons; no guessed rebind (AC6). |
| Transactions/constraints/relations preserve source+provenance | Spans on every constraint/relation/transaction; document provenance (input revision + contributing file digests); ORM pin and declaration-closure digest in identity. |
| Read-only, no migrations, no DB credentials | AC7 tests; no execution paths exist in the extractor. |

## Gates executed (local receipts)

- `node adapters/node-typescript/build.mjs` — regenerated; `--check`
  byte-identical (`{"ok":true,...,"digest":"sha256:93ed2de682366404b2471189807c543b1de8bf28cd023607758382987e5fc60e","bytes":14613981}`).
- `node scripts/test-adapter-manifest-golden.mjs` — ok (entry digest +
  package digest recompute over the committed artifact).
- `node scripts/test-node-typescript-scanner.mjs` — ok (3 files; 12
  fixture/unit tests, no regressions from the resolution seam).
- `node scripts/test-node-typescript-kernel.mjs` — ok (4 files).
- `node scripts/test-node-drizzle.mjs` — ok (19/19 tests).
- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` —
  clean.
- `cargo test --workspace --locked --no-fail-fast` — 88 test binaries,
  0 failures.
- CI: the new Drizzle gate is registered in
  `.github/workflows/ci.yml` after the scanner suite, inside the same
  provisioning job (which already runs `npm ci --ignore-scripts` +
  `build.mjs --check` before the Node suites).

## Explicit partial/unsupported items

1. **Core/CLI surfacing of Drizzle evidence** (AC5 rendering, wire
   transport of the document): successor protocol work per the
   research plan §6-S1/S4. The evidence document, its transaction
   groups, and its digest/refresh semantics are complete and tested
   adapter-side; nothing was smuggled into the closed v0.3.2 wire.
2. **`scan.schema` capability**: deliberately NOT reinterpreted
   (collision with #117 information-schema introspection). The
   extractor rides the existing `scan` operation internally; a
   distinct negotiated capability name is successor registry work.
3. **Upstream `meta/_journal.json` spelling**: excluded by the
   kernel's portable path segment grammar (`[a-z0-9._-]`); fixtures
   use the portable `meta/journal.json` and the recognizer accepts
   both spellings when granted. Recorded as a known limitation in
   `docs/drizzle-bindings.md`.
4. **Supported ORM surface**: `drizzle-orm@0.44.7` exactly, the seven
   public subpaths mapped in `DRIZZLE_SUPPORTED_SUBPATHS`, static
   builder subset. Other versions/dialects degrade to
   unresolved-import uncertainty (never quasi-evidence). Relational
   query API (`db.query.*` preview objects), prepared-statement
   reuse, and batch APIs are outside this qualified subset and
   surface as unknown shapes rather than guesses.
5. **Runtime verification**: no database, no migration execution, no
   runtime receipts — by scope and by AC7.

## Fixture honesty markers (regression anchors)

The fixtures deliberately contain: one SQL default, two client hooks,
a join-table candidate, a many-relation without uniqueness evidence, a
dynamic insert, a `$dynamic()` query, an unawaited builder, a raw
execute, an unscoped delete on a tenant-key table, a nested
transaction, a rollback, an out-of-callback db use, a ghost binding,
and a projection type divergence. Tests assert each produces its
specific evidence or limitation — they cannot pass with the negatives
silently dropped.
