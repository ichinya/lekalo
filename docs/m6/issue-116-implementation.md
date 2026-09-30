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

## Fix round 3 (re-review: one major survives, two residual minors)

The re-review verified every round-2 fix and found one major in the
same defect class — callee-shape gates — plus two residual minors.
All three are fixed with regression fixtures/tests, one commit per
finding, in the same honesty contract: a recognized Drizzle surface is
either extracted with provenance or recorded as an explicit
limitation; sections covering dropped constructs never claim
`complete`.

Major — non-Identifier callees silently dropped recognized constructs.
`import * as d ... then d.relations(...)` / `d.pgTable(...)`,
`const r = relations then r(...)`, and `const t = pgTable then
t("gadgets", ...)` each produced ZERO extracted rows while
`completeness.sections.relations`/`tables` claimed `complete` with no
limitation. Root causes: the relations walk gated on
`expression.kind === Identifier` before resolution (the table path
already accepted PropertyAccess; the relations path never got the
treatment), and `closureCalleeSymbol` resolved only Alias-flag symbols
so variable-bound callees failed before the in-closure test. Fix:
`closureCalleeSymbol` now follows the resolved declaration identity
through namespace property access AND through bounded const/let
rebinding chains (`MAX_ALIAS_HOPS`); `extractRelations` gates with the
same resolver instead of its own Identifier-only one. Callees that
stay unprovable but are RECOGNIZABLE by their construct spelling
(element access into a property bag naming `pgTable`/`relations`) are
never dropped: they emit the new `callee-unproven` limitation
(disjoint table/relations recognition sets keep exactly one limitation
per construct family) and mark the section gap.

Minor — `db.batch`/`db.query` receiver misattribution. The
`db.*` receiver short-circuit memoized proven identities by the
root's NAME, so a same-named non-Drizzle local (`const db = {...}`
shadowing the Drizzle client) inherited the proven identity and
produced bogus out-of-subset limitations. The memo is now keyed by the
resolved declaration symbol (`state.dbSymbols`, exact proven kind
stored and propagated), and the transaction walker's receiver lookup
resolves through the same symbol path.

Minor — `tableByExport` last-writer consultation. Select-all
projection and tenantKey scope evidence re-consulted the export-name
map, whose last writer wins — with two modules exporting the same name
the projection read the wrong module's columns and the
`scope-predicate-missing` verdict flipped with walk order. Both sites
now re-find the exact targeted row by its unique native id (module +
export + physical name).

New `postgres-callees` fixture: namespace and rebinding callee shapes
that must extract, construct-named-but-unprovable bag accesses that
must limit, and a shadowing `db` local beside a genuine `db.query`.
Four new gate tests (28 -> 32): namespace/rebinding extraction with
closure provenance; the unproven-construct honesty matrix; the
symbol-bound receiver memo (exactly one limitation, anchored to the
genuine call's line); and the exact targeted-row projection/scope
resolution. All four fail on the pre-fix bundle and pass after.

Bundle: rebuilt deterministically (`build.mjs --check` byte-identical,
entry `sha256:518fb509600088776c9d1c5612c53f83fa9f2088dc547a8b418549dd33c35d63`,
14,622,822 bytes), manifest package digest regenerated
(`sha256:f965bc5ba983914449fce876449b83939693ba7d7692244dd12b7a90ea78c53c`),
committed-manifest pin moved with the bytes it guards. Gates: `cargo
fmt --check`, `cargo clippy --workspace --all-targets --locked -D
warnings`, `cargo test --workspace --locked --no-fail-fast` (89 test
binaries, 0 failures, incl. `the_committed_adapter_manifest_parses`),
the drizzle gate (32/32), kernel/scanner/zod/native-gates/transport/
openapi/client-sdk/scenario Node suites, the fixture-provenance
fail-closed family gate (the `node-typescript-drizzle` family is now
declared synthetic), contract-versions/structure/authority/privacy/
model checks, the manifest golden, and the adapter conformance battery
(pass 10, fail 0). The ajv-based manifest-contract script stays
CI-owned: `ajv` is not provisioned in the offline worktree (identical
at the base commit).

## Fix round 4 (re-review: two new majors, two residual minors)

The re-review verified every round-3 fix (namespace access, const/let
rebinds including 3-hop chains, the callee-unproven net, the
symbol-bound receiver memo, native-id targets, the fixture manifest)
and found two new majors plus two residual minors in the same honesty
contract. All four are fixed with regression fixtures/tests, one
commit per finding: a recognized Drizzle surface is either extracted
with provenance or recorded as an explicit limitation, and sections
covering dropped constructs never claim `complete`.

Major — module-namespace receivers typed as `db` handles fabricated
evidence. `import * as orm from "drizzle-orm"` then
`orm.select().from(users)` emitted a clean query row
(`receiver:"db"`, the real target's columns), `orm.batch([...])`
misattributed `batch-unsupported`, and `orm.query.users.findMany()`
misattributed `relational-query-unsupported`. Root cause: the
namespace object's TYPE resolves into the embedded closure (the
module's export surface is the vendored declarations), so the closure
test alone typed the namespace as a client handle. Fix:
`isModuleNamespaceSymbol` excludes namespace symbols (SourceFile /
NamespaceImport / ExportSpecifier declarations) from db-handle typing
in `rootIdentityKind`; member calls on namespace receivers emit the new
`namespace-receiver-unsupported` limitation (query-shaped chains,
batch, relational query, transaction tails) and degrade the covered
section (queries; the transactions completeness now honors its section
gap too). `orm.pgTable(...)`/`orm.relations(...)` callee resolution is
untouched — namespace-ACCESS callees still extract exactly like r3.

Major — renamed destructure `const { relations: rel2 } = orm` silently
dropped. The callee's declaration is a BindingElement, which
`variableDeclarationOf` ignored, so the construct escaped both
extraction and the spelling net (the local name `rel2` names nothing).
Fix: BindingElement declarations resolve through the enclosing
declaration's initializer — the destructured property name selects the
export from the namespace symbol, and because star re-exports flatten
only through the checker (`export * from "./relations.js"` never lands
in the raw `.exports` table) the lookup goes through
`getExportsOfModule`. Renamed destructures, shorthand destructures,
and aliased direct imports (`relations as rel3`, already covered by
the r2 alias machinery — verified, not assumed) all extract with
closure provenance; when a destructure is unprovable, the destructured
property name counts as a construct spelling, so the drop stays an
explicit `callee-unproven`.

Minor — rebind chains past `MAX_ALIAS_HOPS` dropped silently. The
resolver returned null at the bound and the net only knew the local
spelling. The resolver now reports the bound hit through a probe and
the net recognizes the construct when the callee's vendored type names
the construct family — family-disjoint, so a relations chain never
also flags the table walk. Four hops still extract; the fifth emits
`callee-unproven` anchored to its call.

Minor — the spelling net over-flagged provably non-Drizzle member
calls. `builder.relations()` / `builder.pgTable()` on receivers whose
callee TYPE provably resolves outside the embedded closure (project
classes, other packages) emitted false `callee-unproven` Drizzle gaps.
The net now checks the callee's type provenance first: a type that
resolved to real declarations with none in the closure is provably not
a vendored construct and stays limitation-free; unresolvable
(any/error) types remain unprovable and keep the net's coverage.

New `postgres-round4` fixture: namespace receivers (query-shaped
chains, batch, relational query, transaction) that must emit
`namespace-receiver-unsupported` and never fabricate rows; renamed/
shorthand destructure and aliased-import bindings that must extract by
resolved export identity; a five-rebind chain explicit at the bound
while four hops extract; and provably non-Drizzle spellings that must
never flag. Four new gate tests (32 -> 36), each failing on the
pre-fix bundle and passing after.

Bundle: rebuilt deterministically (`build.mjs --check` byte-identical,
entry `sha256:3ba9b606066391fa96308ef6d7315a368963e3daa410124c2166c0f40616c833`,
14,628,767 bytes), manifest package digest regenerated
(`sha256:746777c214d74a37d48580e58c75cabdcd33b2da2244940e1f8a44795eaae63e`),
committed-manifest pin moved with the bytes it guards. Gates: `cargo
fmt --check`, `cargo clippy --workspace --all-targets --locked -D
warnings`, `cargo test --workspace --locked --no-fail-fast` (89 test
binaries, 0 failures, incl. `the_committed_adapter_manifest_parses`
over the new pin), the drizzle gate (36/36), the kernel/scanner/zod/
native-gates/transport/openapi/client-sdk/scenario Node suites, the
fixture-provenance family gate, contract-versions/structure/authority/
privacy/model checks, and the manifest golden.

## Fix round 5 (re-review: two majors in type-annotated territory)

The r4 review verified all four round-4 fixes and gates green, then
surfaced two majors in the same honesty contract's blind spot:
ANNOTATED types. Both are fixed with regression fixtures/tests, one
commit per finding.

Major — module-TYPED values still fabricated db rows.
`declare const ns: typeof import("drizzle-orm")`, parameters typed
`typeof orm` (or through an alias of the module type query), and
`await import(...)` bindings all classified as `receiver:"db"` and
produced fabricated clean select rows (real target columns, no
limitation), plus `batch-unsupported`/`relational-query-unsupported`
misattributions. Root cause: the round-4 exclusion tests DECLARATION
kinds (namespace imports), and these values are ordinary
variables/parameters — but their TYPE is the module's export surface,
which resolves INTO the embedded closure for vendored modules, so the
closure test won. Fix: `isModuleNamespaceType` extends the exclusion
to module-typed values (direct module types, type aliases, and the
anonymous `default`-wrapped shape a dynamic import produces) in both
the identifier and non-identifier receiver paths; member calls on them
emit `namespace-receiver-unsupported` with a section gap — including
the await-import form, which previously shrugged `receiver-unknown`.

Major — type-provable construct callees silently dropped on VALID
code. `declare const pt: typeof import("drizzle-orm/pg-core").pgTable`
and `declare const rel: typeof relations` produced ZERO rows and no
limitation while sections claimed `complete`: the value-declaration
walk had nothing to follow and the spelling net only knows construct
NAMES. Fix: when the declaration form has no value identity to follow
(a declare-const binding, a construct-typed parameter), the declared
TYPE is the identity anchor. `typeof relations` anchors to the vendored
function symbol directly; a factory type query anchors to its callable
interface (`PgTableFn`) — note `typeof import(...).pgTable` parses as
an IMPORT TYPE node, not a type query, so the construct identity comes
from the import type's member spelling resolved through the declaring
closure module's export table. Function types match by SYMBOL identity
(never structurally — an inline signature with the same shape has no
closure symbol and stays unprovable). Value-carrying forms are
deliberately excluded: property bags, destructures, and casts over
dynamic data keep the round-3/round-4 explicit `callee-unproven`
flags, and the round-4 bound-hit contract stands.

New `postgres-round5` fixture: the three typed-namespace receiver
forms plus the await-import binding (all must limit, never fabricate),
type-provable construct callees (must extract with closure
provenance), and honest negatives (dynamic-cast destructure and
beyond-bound chain stay explicit; four hops extract; local construct
spellings stay silent). Three new gate tests (36 -> 39); the two
major-finding tests fail on the pre-fix bundle and pass after.

Bundle: rebuilt deterministically (`build.mjs --check` byte-identical,
entry `sha256:4af648644c64dd0259ef48016b9b4e99c32f5e82717173185f773779b007ca3f`,
14,633,510 bytes), manifest package digest regenerated
(`sha256:ae71cdec1a9da2e464b1cb743f5c8ebe9d02ff30cc570b9f2266230832f6f6d4`),
committed-manifest pin moved with the bytes it guards.
