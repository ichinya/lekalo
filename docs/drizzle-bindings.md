# Drizzle bindings (issue #116)

Target-side evidence adapter for Drizzle ORM on the Node/TypeScript
target. Drizzle stays an implementation detail of the target: nothing
in this document enters the core Model, IR, or effect vocabulary. The
adapter maps statically decodable Drizzle surfaces onto a closed
target-evidence document consumed by tests and — through the existing
observed scan path — by the host.

## What the adapter produces

One read-only compiler pass over the scanner's Program attaches
`index.drizzle` (schema `lekalo/drizzle-evidence/v0.1.0`) to every
scan:

- **tables** — `pgTable`/`mysqlTable` declarations with physical
  names, columns (TS name vs physical name, bounded type token,
  nullability, primary key, uniqueness, autoincrement/identity),
  defaults (literal, function sugar, SQL expression, client
  `$defaultFn`/`$onUpdate` hooks), and inline/extra-config
  primary/unique/index/foreign constraints with 1-based source spans,
  member order, and referential actions;
- **relations** — `relations(table, ({one, many}) => ...)` endpoints
  with fields/references pairs, cardinality evidence, and explicit
  `uniqueness-unproven` / `relation-policy-missing` limitations;
  join-table candidates are evidence (`joinTableCandidate`), never
  declarations;
- **queries** — select/insert/update/delete builder chains rooted at a
  compiler-verified Drizzle database/transaction receiver, with field
  reads (projection, predicate, join, order, returning), writes
  (`.values`/`.set` keys), projections, bounded inputs with compiler
  type spellings, and effect rows (`action` read/create/update/delete,
  `confidence: "extracted"`, `canonical: false`);
- **transactions** — `db.transaction` boundaries with member query
  ids, nesting, rollback sites, conditional-flow marks, and
  `query-not-tx-bound` / `tx-escaped` honesty limitations;
- **migrations** — `drizzle.config.*` literals (dialect, schema refs,
  out) plus bounded migration-folder references with digests and
  layout detection; nothing is executed and file presence never
  proves an applied state;
- **scope** — tenant/workspace equality predicates and create-time
  tenant keys as evidence rows, with `scope-predicate-missing`
  limitations on tenant-key tables whose queries carry no scope
  predicate (a completeness note, never an authorization verdict);
- **bindings** — the owner-supplied `drizzle.bindings.json` input
  validated against extracted tables (confirmed/unresolved), plus the
  storage projection comparison (`drizzle.projection.json`) reporting
  matched/type-divergent/nullability/missing rows.

## Evidence identity and integrity

Every document carries: the adapter id, extractor version and rule
set, the exact ORM pin, the embedded declaration-closure digest, the
input manifest revision (the same key the scanner session uses for
warm/cold parity), and a SHA-256 digest over the canonical document.
Unchanged inputs produce byte-identical evidence; any input edit —
including a query body change under an unchanged type signature —
changes the revision and the digest.

## Reliability rules

1. **Compiler identity is primary.** A recognizer only trusts callees
   whose declarations resolve into the embedded upstream closure
   (`drizzle-orm@0.44.7`, vendored at build time, type-context-only).
   Name similarity is never evidence.
2. **Pin policy.** The closure is offered to a program only when every
   consumer manifest that declares `drizzle-orm` pins exactly the
   supported release (`x.y.z`). Ranges, mismatched pins, or missing
   pins leave imports unresolved (uncertainty), never quasi-Drizzle.
3. **Raw SQL and dynamic construction are explicit unknowns.** Raw
   `sql` fragments, `execute`, `$dynamic()`, dynamic `values`/`set`,
   computed keys, spreads, and unknown receivers produce limitation
   rows (`raw-sql`, `dynamic-builder`, `dynamic-values`,
   `receiver-unknown`, ...) and partial section completeness — never a
   silently empty complete set.
4. **Declared schema ≠ live database.** `completeness.databaseState`
   is `unknown` by construction; the projection comparison is a
   declaration-vs-projection check only.
5. **Detected effects are evidence, not declarations.** Query effects
   are `extracted` with `canonical: false`; upserts record the
   create-or-update alternative instead of claiming both happened.
6. **Renames invalidate.** A table's native identity is derived from
   module + export + physical name; a rename changes it, and
   `auditStaleBindings` marks previous bindings stale
   (`binding-stale-table-native-missing`). A content edit under the
   same identity is `binding-stale-table-source-changed`. Rebinding
   requires confirmed rename history, which this adapter does not
   synthesize.
7. **Read-only, credential-free.** The adapter never executes
   migrations, never opens a connection, and carries no
   credential/connection fields. Fixtures prove byte-identical project
   trees before and after scans.
8. **Bounds.** Every array is bounded (tables 256, columns 128,
   queries 512, limitations 512, ...) and the document digest is
   computed over the bounded form, so overflow is visible in the
   evidence itself (`truncated` limitations + partial completeness).

## Dialect handling (MySQL / PostgreSQL)

The paired fixtures cover: uuid/defaultRandom vs int-autoincrement,
serial alias sugar (recorded as a limitation, never a storage type),
`datetime(6)`/`timestamp(6)` precision, jsonb vs json, `$returningId`
vs `.returning(...)`, `onDuplicateKeyUpdate` vs `onConflictDoUpdate`,
and folder-journal migration layouts. Neutral document shape, effect
actions, binding semantics, and completeness vocabulary are identical
across dialects; dialect facts stay in target rows.

## Known limitations

- The kernel's portable path vocabulary (`[a-z0-9._-]` segments)
  excludes upstream's `meta/_journal.json` spelling; the fixtures use
  the portable `meta/journal.json` spelling and the recognizer accepts
  both when a grant makes them visible.
- Core-side surfacing (protocol wire transport of the evidence
  document, inspect/impact/context rendering of transaction groups)
  is successor work; the evidence document and its transaction
  boundaries exist and are tested at the adapter boundary today.
- The recognizer covers the qualified static subset listed above;
  wrappers, dynamic assembly, and runtime-only behavior degrade to
  explicit limitations rather than guesses.
