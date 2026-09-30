# node-typescript-drizzle fixtures (issue #116)

Synthetic, offline Drizzle ORM consumer projects. Nothing here connects
to a database, executes migrations, or requires `node_modules`: the
scanner resolves the pinned upstream declarations embedded in the
shipped adapter bundle (`drizzle-orm@0.44.7`).

## Layout

- `postgres/` — `pg-core` surface: tables, columns, defaults, primary/
  unique/index/foreign constraints, `relations`, select/insert/update/
  delete with `returning` and `onConflictDoUpdate`, raw SQL and
  `$dynamic()` negatives, transaction boundaries (nested + rollback),
  `drizzle.config.ts` plus a `folders-journal` migration folder, the
  owner-supplied binding input (`drizzle.bindings.json`), and a storage
  projection attachment (`drizzle.projection.json`) with deliberate
  divergences.
- `mysql/` — the paired `mysql-core` surface: autoincrement and serial
  alias sugar, `datetime(6)` precision, `$returningId()`,
  `onDuplicateKeyUpdate`, tenant-scoped predicates, and the same
  transaction/config/migration layout.
- `postgres-edges/` — the fix-round regression anchors: aliased
  imports (`pgTable as table`, `relations as rel`), the out-of-subset
  relational query and batch APIs (`db.query.*`, `db.batch`), an
  ambiguous export name across two modules, a `check()` constraint
  with a raw SQL body, a consumer tsconfig `paths` mapping (`@app/*`)
  exercised through an aliased import, a join whose RHS equality
  columns must read as field reads, and a builder handed off by an
  explicit `return`.
- `postgres-callees/` — the fix-round-3 regression anchors: recognized
  constructs behind non-Identifier callees (namespace property access
  `pg.pgTable`/`orm.relations`, const/let rebinding `const f = pgTable`)
  which must extract by resolved symbol identity, construct-named but
  unprovable callees (element access into a property bag) which must
  produce explicit `callee-unproven` limitations and section gaps, and
  a same-named non-Drizzle `db` local which must never inherit the
  proven receiver identity of the module-level Drizzle client.

The paired dialect fixtures exist to prove AC8: the neutral evidence
(document shape, effect actions, binding and completeness semantics) is
identical across dialects, while dialect facts stay in target rows
(type tokens, identity/autoincrement, upsert shapes, returning shapes)
and never leak into a Model.

## Honesty markers baked into the sources

- raw SQL (`sql\`...\``), `$dynamic()`, dynamic `values`/`set` and an
  unawaited builder produce explicit limitations;
- one fixture query touches a tenant-key table without a scope
  predicate (`scope-predicate-missing`);
- the binding input references one entity with no matching table
  (`binding-unresolved`) so staleness/refusal paths stay exercised;
- the projection attachment deliberately diverges on one column type
  and carries one extra column, so the comparison reports findings
  instead of a fake match.
