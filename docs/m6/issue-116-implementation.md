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
  golden updated (entry `sha256:311f29438c74dac26e8311509debc3d7e5b7c9fdfecb0032a6f8d13d70f600f1`,
  14,620,383 bytes; regenerated again by the fix round below).
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
  assembly and digest. Closed limitation vocabulary (60 codes after
  the fix round) and
  closed bounds on every array.
- `adapters/node-typescript/test/drizzle-evidence.test.mjs` (19 tests
  before the fix round, 28 after)
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
  byte-identical (`{"ok":true,...,"digest":"sha256:93ed2de682366404b2471189807c543b1de8bf28cd023607758382987e5fc60e","bytes":14613981}`;
  the fix round re-verified the same property at
  `sha256:311f29438c74dac26e8311509debc3d7e5b7c9fdfecb0032a6f8d13d70f600f1`).
- `node scripts/test-adapter-manifest-golden.mjs` — ok (entry digest +
  package digest recompute over the committed artifact).
- `node scripts/test-node-typescript-scanner.mjs` — ok (3 files; 12
  fixture/unit tests, no regressions from the resolution seam).
- `node scripts/test-node-typescript-kernel.mjs` — ok (4 files).
- `node scripts/test-node-drizzle.mjs` — ok (19/19 tests before the
  fix round, 28/28 after).
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
   reuse, and batch APIs are outside this qualified subset and emit
   explicit `relational-query-unsupported` / `batch-unsupported`
   limitations with a reason code and force the queries section
   partial — never a silent drop (fix round).
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

## Fix round (two blocking reviews)

Both fix-round reviewers' findings are fixed with regression anchors
in the new `postgres-edges` fixture and a new test section in
`drizzle-evidence.test.mjs` (28 tests total in the gate).

Blockers:

- **Silent drops with `complete` sections.** Factory identity now
  resolves through the callee SYMBOL (declaration), not the local
  identifier text — `import { pgTable as pt }` and
  `import { relations as rel }` extract exactly like the spelled
  names (`closureCalleeSymbol`). The relations walk no longer gates
  on the literal `relations` text before symbol resolution.
  Recognized-but-out-of-subset surfaces (`db.query.*`, `db.batch`)
  emit explicit `relational-query-unsupported` / `batch-unsupported`
  limitation rows with a reason code; a per-section gap set
  (`sectionGaps`) plus the removal of the dead
  `'partial':'partial'` ternaries means a section claims `complete`
  only when it covered every recognized surface — all-clean rows are
  now honestly complete, gaps and row limitations force partial.

Majors:

- **tsconfig paths merge.** The drizzle `paths` mapping is merged
  with the consumer's parsed mapping (consumer wins key conflicts;
  drizzle added additively) instead of wholesale replacement — a
  project `@app/*` mapping keeps resolving after the pin attaches.
  Asserted byte-level via `index.programOptions` and functionally via
  an `@app`-aliased query resolving its target.
- **Ambiguous export names.** Bindings resolve via unique evidence
  only: a unique export match confirms; several modules exporting the
  same name (or several tables sharing a physical name) produce
  status `ambiguous` + `binding-ambiguous` — never a guessed
  `confirmed` pick.

Minors:

- `check(name, sql\`…\`)` bodies record `sqlPredicate: true` + a
  `raw-sql` limitation instead of being silently absent.
- `provenance.inputRevision` includes `otherFiles` digests — a
  same-length migration SQL edit changes the revision; migration
  folder files are provenance inputs and migration `schemaRefs` are
  existence-checked against the granted inventory
  (`migration-ref-missing`).
- `readOptionalProjectInput` preserves the failure reason; an invalid
  bindings/projection input reports `bindings-input-invalid` /
  `projection-input-invalid` (state `invalid`) instead of being
  mislabeled missing/absent.
- A rejected-pin project is distinguishable from a non-drizzle
  project: `index.drizzleAttachment` records the rejection reason,
  declared specs, and supported pin (plain absence stays unrecorded).
- Transaction membership and tx binding are separate axes: a
  non-tx-bound query inside a callback carries `transactionId: null`,
  is not a group member, and keeps the `query-not-tx-bound`
  limitation.
- Join RHS equality columns (`eq(users.id, posts.authorId)`) record
  field reads of the joined table instead of phantom `reference`
  inputs; the dead `query-shape-ignored` vocabulary is removed.
- An explicitly `return`-ed builder no longer adds
  `builder-not-executed` noise (an intentional handoff).

Bundle: rebuilt deterministically (`build.mjs --check` byte-identical),
manifest regenerated
(entry `sha256:311f29438c74dac26e8311509debc3d7e5b7c9fdfecb0032a6f8d13d70f600f1`,
14,620,383 bytes; package digest
`sha256:e63b12b9d29f965ec195eaf1a92d46cb4f6cd9192c138c4f2806df5a4ee357b5`),
and the committed-manifest test's pinned digest moved with the bytes
it guards. Gates: `cargo fmt --check`, `cargo clippy --workspace
--all-targets --locked -D warnings`, `cargo test --workspace --locked`
(all green incl. `the_committed_adapter_manifest_parses`), the
29-test Node adapter suites (294 tests, incl. the 28-test drizzle
gate), the manifest golden, and the kernel conformance battery
(10 pass / 0 fail).
