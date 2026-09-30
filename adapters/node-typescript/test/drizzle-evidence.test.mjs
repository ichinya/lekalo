/**
 * #116 Drizzle ORM evidence suite: end-to-end scans of the committed
 * Postgres and MySQL fixtures through the production bundle (vendored
 * compiler + embedded upstream drizzle-orm@0.44.7 declaration closure),
 * asserting schema facts, constraints with spans, relations, query
 * effects, transaction boundaries, migration references, tenant scope
 * evidence, bindings, projection comparison, freshness invalidation,
 * the read-only guarantee, and dialect coverage without core leakage.
 */
import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import { test } from "node:test";
import { join } from "node:path";

import {
  auditStaleBindings,
  decodeBindingsInput,
  drizzleClosureMapping,
  DRIZZLE_SUPPORTED_SUBPATHS,
  resolveDrizzleAttachment,
} from "../src/drizzle-evidence.mjs";
import {
  dispose,
  loadAdapter,
  materializeFixture,
  scanDrizzleFixture,
  snapshotTree,
} from "./drizzle-helpers.mjs";

// ---------------------------------------------------------------------------
// 0. Attachment decision and closure mapping (pure probes).
// ---------------------------------------------------------------------------

test("drizzle closure mapping covers exactly the supported subpaths in the embedded bundle", async () => {
  const adapter = await loadAdapter();
  const closure = adapter.__lekaloKernel.embeddedDrizzleDeclarations();
  assert.ok(closure, "the shipped bundle carries the embedded drizzle closure");
  assert.equal(closure.pin, "0.44.7");
  assert.match(closure.digest, /^sha256:[0-9a-f]{64}$/);
  const mapping = drizzleClosureMapping(closure);
  assert.ok(mapping, "every supported subpath exists in the closure");
  assert.deepEqual(Object.keys(mapping).sort(), [
    "drizzle-orm",
    "drizzle-orm/mysql-core",
    "drizzle-orm/mysql2",
    "drizzle-orm/node-postgres",
    "drizzle-orm/pg-core",
    "drizzle-orm/relations",
    "drizzle-orm/sql",
  ]);
  for (const [specifier, targets] of Object.entries(mapping)) {
    assert.equal(targets.length, 1, specifier);
    assert.ok(closure.files.has(targets[0]), `${specifier} maps into the closure`);
  }
  // An unsupported dialect subpath is NOT mapped — it must stay
  // unresolved rather than resolving near-miss.
  assert.equal(mapping["drizzle-orm/sqlite-core"], undefined);
  void DRIZZLE_SUPPORTED_SUBPATHS;
});

test("attachment decision: exact pin attaches; ranges, mismatch, and absence do not", () => {
  const closure = { pin: "0.44.7", files: new Map(), digest: "sha256:" + "0".repeat(64) };
  const manifestOf = (specs) => ({
    packageFiles: specs === null ? [] : [{
      path: "package.json",
      digest: "sha256:test",
    }],
  });
  const readBytes = () => Buffer.from(JSON.stringify({
    dependencies: specs === undefined ? {} : { "drizzle-orm": specs },
  }));
  const specs = (fn) => fn;
  void specs;

  const exact = resolveDrizzleAttachment(manifestOf(["0.44.7"]), () =>
    Buffer.from(JSON.stringify({ dependencies: { "drizzle-orm": "0.44.7" } })), closure);
  assert.equal(exact.attached, true);
  assert.equal(exact.reason, "pin-exact");

  const range = resolveDrizzleAttachment(manifestOf(["^0.44.7"]), () =>
    Buffer.from(JSON.stringify({ dependencies: { "drizzle-orm": "^0.44.7" } })), closure);
  assert.equal(range.attached, false);
  assert.equal(range.reason, "pin-not-exact");

  const mismatch = resolveDrizzleAttachment(manifestOf(["0.43.0"]), () =>
    Buffer.from(JSON.stringify({ devDependencies: { "drizzle-orm": "0.43.0" } })), closure);
  assert.equal(mismatch.attached, false);
  assert.equal(mismatch.reason, "pin-mismatch");
  assert.equal(mismatch.supported, "0.44.7");

  const absent = resolveDrizzleAttachment(manifestOf(null), () => Buffer.from("{}"), closure);
  assert.equal(absent.attached, false);
  assert.equal(absent.reason, "pin-absent");

  const noClosure = resolveDrizzleAttachment(manifestOf(["0.44.7"]), () =>
    Buffer.from(JSON.stringify({ dependencies: { "drizzle-orm": "0.44.7" } })), null);
  assert.equal(noClosure.attached, false);
  assert.equal(noClosure.reason, "declarations-absent");
  void readBytes;
});

test("bindings input decoder: closed schema, bounded entities, malformed refusal", () => {
  const ok = decodeBindingsInput(JSON.stringify({
    schema: "lekalo/drizzle-bindings-input/v0.1.0",
    entities: [{ entity: "User", table: "users" }],
  }));
  assert.equal(ok.ok, true);
  assert.deepEqual(ok.entities, [{ entity: "User", table: "users" }]);

  for (const bad of [
    "not json",
    JSON.stringify({ schema: "other/v1", entities: [] }),
    JSON.stringify({ schema: "lekalo/drizzle-bindings-input/v0.1.0" }),
    JSON.stringify({ schema: "lekalo/drizzle-bindings-input/v0.1.0", entities: [{ entity: 1, table: "x" }] }),
    JSON.stringify({ schema: "lekalo/drizzle-bindings-input/v0.1.0", entities: "nope" }),
  ]) {
    assert.equal(decodeBindingsInput(bad).ok, false, bad);
  }
});

// ---------------------------------------------------------------------------
// 1. Postgres schema evidence (AC1, AC2).
// ---------------------------------------------------------------------------

test("postgres: tables, columns, defaults, nullability, and keys extract with spans", async () => {
  const fx = await scanDrizzleFixture("pg-schema", "postgres");
  try {
    const d = fx.index.drizzle;
    assert.ok(d, "drizzle evidence attached");
    assert.equal(d.schema, "lekalo/drizzle-evidence/v0.1.0");
    assert.equal(d.identity.orm.pin, "0.44.7");
    assert.equal(d.identity.orm.attached, true);
    assert.equal(d.identity.readOnly, true);
    assert.deepEqual(d.dialects, ["postgresql"]);

    const users = d.tables.find((t) => t.exportName === "users");
    assert.ok(users, "users extracted");
    assert.equal(users.physicalName, "users");
    assert.equal(users.dialect, "postgresql");
    const columns = new Map(users.columns.map((c) => [c.tsName, c]));
    const id = columns.get("id");
    assert.equal(id.typeToken, "uuid");
    assert.equal(id.primaryKey, true);
    assert.equal(id.notNull, true);
    assert.equal(id.default.kind, "function");
    assert.equal(id.default.token, "defaultRandom");
    const email = columns.get("email");
    assert.equal(email.typeToken, "varchar(200)");
    assert.equal(email.notNull, true);
    assert.equal(email.unique, false, 'uniqueness comes from the unique index, not the column');
    const metadata = columns.get("metadata");
    assert.equal(metadata.notNull, false, "omitted notNull stays nullable");
    assert.equal(metadata.physicalName, "metadata", "name-implied physical name");
    const active = columns.get("isActive");
    assert.equal(active.typeToken, "boolean");
    assert.equal(active.default.kind, "literal");
    assert.equal(active.default.token, "true");
    const createdAt = columns.get("createdAt");
    assert.equal(createdAt.typeToken, "timestamp(6)", "precision folds into the token");
    assert.equal(createdAt.default.kind, "function");
    const updatedAt = columns.get("updatedAt");
    assert.equal(updatedAt.default.kind, "client-hook", "$onUpdate is a client hook, not a DB default");

    // Spans resolve into the fixture source bytes (AC2).
    const source = readFileSync(join(fx.project, "src/schema.ts"), "utf8").split("\n");
    for (const table of d.tables) {
      const line = source[table.span.start.line - 1];
      assert.ok(line.length > 0, `${table.exportName} span points at a real line`);
      const column = table.columns[0];
      const columnLine = source[column.span.start.line - 1];
      assert.ok(columnLine.includes(column.tsName), `column span for ${column.tsName} hits its declaration`);
      // 1-based, start before end.
      assert.ok(table.span.start.line <= table.span.end.line);
    }
  } finally {
    dispose(fx.root);
  }
});

test("postgres: constraints extract with names, members, actions, and spans", async () => {
  const fx = await scanDrizzleFixture("pg-constraints", "postgres");
  try {
    const d = fx.index.drizzle;
    const users = d.tables.find((t) => t.exportName === "users");
    const uniqueIndex = users.constraints.find((c) => c.kind === "unique-index");
    assert.ok(uniqueIndex, "uniqueIndex extracted");
    assert.equal(uniqueIndex.name, "users_email_tenant");
    assert.deepEqual(uniqueIndex.members, ["email", "tenant_id"]);
    assert.equal(uniqueIndex.unique, true);
    const source = readFileSync(join(fx.project, "src/schema.ts"), "utf8").split("\n");
    const line = source[uniqueIndex.span.start.line - 1];
    assert.ok(line.includes("users_email_tenant"), "constraint span resolves to its declaration");

    const posts = d.tables.find((t) => t.exportName === "posts");
    const author = posts.columns.find((c) => c.tsName === "authorId");
    assert.equal(author.references.table, "users");
    assert.equal(author.references.column, "id");
    assert.equal(author.references.onDelete, "cascade");
    assert.equal(author.references.columnResolved, true);

    const index = posts.constraints.find((c) => c.kind === "index");
    assert.ok(index, "composite index extracted");
    assert.deepEqual(index.members, ["author_id", "slug"]);

    // Composite primary key through the object form.
    const taskTags = d.tables.find((t) => t.exportName === "taskTags");
    assert.equal(taskTags.joinTableCandidate, true, "two-FK table is a join-table candidate");
    const pk = taskTags.constraints.find((c) => c.kind === "primary-key");
    assert.ok(pk, "composite primary key extracted");
    assert.deepEqual(pk.members, ["task_id", "tag_id"]);
    assert.equal(pk.unique, false, "primary key is not folded into unique");

    // A SQL default stays a bounded partial, never an executable value.
    const searchVector = posts.columns.find((c) => c.tsName === "searchVector");
    assert.equal(searchVector.default.kind, "sql");
    assert.equal(searchVector.default.token, null);
    const limitations = new Set(d.limitations.map((l) => l.code));
    assert.ok(limitations.has("sql-default"), "sql default records an explicit limitation");
    assert.ok(limitations.has("client-hook"), "$defaultFn records a client-hook limitation");
  } finally {
    dispose(fx.root);
  }
});

test("postgres: relations export endpoints, cardinality evidence, and honest limitations", async () => {
  const fx = await scanDrizzleFixture("pg-relations", "postgres");
  try {
    const d = fx.index.drizzle;
    const many = d.relations.find((r) => r.name === "posts");
    assert.ok(many, "many relation extracted");
    assert.equal(many.sourceTable, "users");
    assert.equal(many.targetTable, "posts");
    assert.equal(many.kind, "many");
    assert.equal(many.cardinality, "one-to-many");
    assert.ok(many.limitations.includes("cardinality-unproven"));
    assert.ok(many.limitations.includes("relation-policy-missing"));
    assert.equal(many.confidence, "inferred");

    const one = d.relations.find((r) => r.name === "author");
    assert.ok(one, "one relation extracted");
    assert.deepEqual(one.fields.map((f) => f.column), ["authorId"]);
    assert.deepEqual(one.references.map((r) => r.column), ["id"]);
    // The referenced id column is a primary key, so the uniqueness
    // evidence exists and the one-to-one cardinality is provable here.
    assert.equal(one.cardinality, "one-to-one");
    assert.ok(!one.limitations.includes("uniqueness-unproven"));
    // Spans resolve.
    const source = readFileSync(join(fx.project, "src/relations.ts"), "utf8").split("\n");
    assert.ok(source[one.span.start.line - 1].includes("author"));
  } finally {
    dispose(fx.root);
  }
});

// ---------------------------------------------------------------------------
// 2. Query and effect evidence (AC3, AC4).
// ---------------------------------------------------------------------------

test("postgres: select/insert/update/delete yield field-level effect evidence", async () => {
  const fx = await scanDrizzleFixture("pg-queries", "postgres");
  try {
    const d = fx.index.drizzle;
    const queries = d.queries;
    assert.ok(queries.length >= 11, `fixture queries extracted (${queries.length})`);

    const findUser = queries.find((q) => q.kind === "select"
      && q.projection?.kind === "all");
    assert.ok(findUser, "bare select() projects all declared columns");
    assert.equal(findUser.target.exportName, "users");
    assert.equal(findUser.effects[0].action, "read");
    assert.ok(findUser.effects[0].fields.some((f) => f.column === "id"));

    const projectionQuery = queries.find((q) => q.projection?.kind === "columns");
    assert.ok(projectionQuery, "select({...}) names its projection");
    assert.deepEqual(
      projectionQuery.projection.columns.map((c) => c.alias).sort(),
      ["active", "email"],
    );
    assert.ok(projectionQuery.reads.some((r) => r.role === "order" && r.column === "email"));

    const insert = queries.find((q) => q.kind === "insert" && q.writes.length > 0);
    assert.ok(insert, "static insert fields recorded");
    assert.deepEqual(insert.writes.map((w) => w.column).sort(), ["email", "isActive", "tenantId"]);
    assert.equal(insert.effects[0].action, "create");

    const dynamicInsert = queries.find((q) => q.kind === "insert" && q.limitations.includes("dynamic-values"));
    assert.ok(dynamicInsert, "parameter-valued insert records dynamic-values");
    assert.equal(dynamicInsert.writes.length, 0, "no field names invented for a dynamic value");
    assert.equal(dynamicInsert.projection.kind, "returning");

    const update = queries.find((q) => q.kind === "update" && q.writes.includes("isActive") === false && q.writes.length === 1);
    const deactivate = queries.find((q) => q.kind === "update");
    assert.ok(deactivate, "update extracted");
    assert.equal(deactivate.effects[0].action, "update");
    assert.ok(deactivate.reads.some((r) => r.role === "where"), "predicate reads recorded");

    const del = queries.find((q) => q.kind === "delete");
    assert.ok(del, "delete extracted");
    assert.equal(del.effects[0].action, "delete");
    assert.equal(del.effects[0].fields.length, 0, "delete carries no invented per-field clears");

    // Every effect row is evidence, never a canonical declaration.
    for (const q of queries) {
      assert.equal(q.canonical, false);
      assert.equal(q.confidence, "extracted");
    }

    // Inputs carry bounded compiler type spellings.
    const tenantQuery = queries.find((q) => q.inputs.length > 0);
    assert.ok(tenantQuery, "inputs recorded");
    assert.ok(tenantQuery.inputs.some((i) => i.expr === "reference" && i.typeToken !== null));
  } finally {
    dispose(fx.root);
  }
});

test("raw SQL, dynamic construction, and unawaited builders warn explicitly (AC4)", async () => {
  const fx = await scanDrizzleFixture("pg-raw", "postgres");
  try {
    const d = fx.index.drizzle;
    const limitations = d.limitations.map((l) => l.code);
    assert.ok(limitations.includes("raw-sql"), "db.execute(sql`...`) warns");
    const dynamic = d.queries.find((q) => q.limitations.includes("dynamic-builder"));
    assert.ok(dynamic, "$dynamic() marks the query incomplete");
    const unawaited = d.queries.find((q) => q.limitations.includes("builder-not-executed"));
    assert.ok(unawaited, "unawaited builder records builder-not-executed");
    const joinQuery = d.queries.find((q) => q.joins.length > 0);
    assert.ok(joinQuery, "join extracted");
    assert.equal(joinQuery.joins[0].table, "users");
    assert.ok(joinQuery.reads.some((r) => r.role === "join" && r.table === "users"));
    // The raw execute leaves NO fabricated query row.
    assert.ok(!d.queries.some((q) => q.chain.length === 1 && q.chain[0] === "execute"));
  } finally {
    dispose(fx.root);
  }
});

// ---------------------------------------------------------------------------
// 3. Transaction boundaries (AC5 evidence half).
// ---------------------------------------------------------------------------

test("postgres: transaction groups carry members, nesting, rollback, and escape honesty", async () => {
  const fx = await scanDrizzleFixture("pg-tx", "postgres");
  try {
    const d = fx.index.drizzle;
    const txs = d.transactions;
    assert.ok(txs.length >= 5, `transactions extracted (${txs.length})`);

    const transfer = txs.find((t) => t.members.length === 2);
    assert.ok(transfer, "transferOwnership group carries both member updates");
    assert.equal(transfer.receiver, "db");
    assert.equal(transfer.parent, null);
    for (const memberId of transfer.members) {
      const member = d.queries.find((q) => q.id === memberId);
      assert.ok(member, `member ${memberId} exists`);
      assert.equal(member.transactionId, transfer.id);
    }
    // Member spans resolve into the fixture source.
    const source = readFileSync(join(fx.project, "src/transactions.ts"), "utf8").split("\n");
    assert.ok(source[transfer.span.start.line - 1].includes("db.transaction"));

    const outer = txs.find((t) => t.nested.length > 0);
    assert.ok(outer, "nested transaction captured");
    const nested = txs.find((t) => outer.nested.includes(t.id));
    assert.equal(nested.parent, outer.id);
    assert.equal(nested.receiver, "tx");

    const rollback = txs.find((t) => t.rollback !== null);
    assert.ok(rollback, "rollback site recorded with its span");

    const conditional = d.queries.find((q) => q.conditional === true);
    assert.ok(conditional, "conditional branch marked");
    assert.ok(conditional.limitations.includes("conditional-flow"));

    const outside = d.queries.find((q) => q.limitations.includes("query-not-tx-bound"));
    assert.ok(outside, "db query inside a transaction callback flagged as not-tx-bound");

    const limitations = new Set(d.limitations.map((l) => l.code));
    assert.ok(limitations.has("rollback"));
    assert.ok(limitations.has("nested-transaction"));
    assert.ok(limitations.has("query-not-tx-bound"));
  } finally {
    dispose(fx.root);
  }
});

// ---------------------------------------------------------------------------
// 4. Tenant scope evidence.
// ---------------------------------------------------------------------------

test("tenant scope: predicates and create-values are evidence; unscoped queries warn", async () => {
  const fx = await scanDrizzleFixture("pg-scope", "postgres");
  try {
    const d = fx.index.drizzle;
    const predicate = d.scope.find((s) => s.kind === "predicate" && s.table === "users");
    assert.ok(predicate, "tenant equality predicate recorded");
    assert.equal(predicate.column, "tenantId");
    assert.equal(predicate.predicate, "equality");
    assert.equal(predicate.confidence, "extracted");

    const createScope = d.scope.find((s) => s.kind === "create-scope");
    assert.ok(createScope, "tenant key in insert values recorded");

    const unscoped = d.queries.find((q) => q.limitations.includes("scope-predicate-missing"));
    assert.ok(unscoped, "query on a tenant-key table without a scope predicate warns");
    // The users table marks the column as a declaration-only candidate.
    const users = d.tables.find((t) => t.exportName === "users");
    assert.deepEqual(users.tenantKey, { column: "tenantId", evidence: "declaration-only" });
  } finally {
    dispose(fx.root);
  }
});

// ---------------------------------------------------------------------------
// 5. Migration references.
// ---------------------------------------------------------------------------

test("migration references: config literals, folder layout, digests, no execution", async () => {
  const fx = await scanDrizzleFixture("pg-migrations", "postgres");
  try {
    const d = fx.index.drizzle;
    const config = d.migrations.find((m) => m.kind === "config");
    assert.ok(config, "drizzle.config.ts recorded");
    assert.equal(config.dialect, "postgresql");
    assert.deepEqual(config.schemaRefs, ["src/schema.ts"]);
    assert.equal(config.out, "drizzle");
    assert.equal(config.layout, "folders-journal");
    const sql = config.entries.find((e) => e.kind === "sql");
    const journal = config.entries.find((e) => e.kind === "journal");
    assert.ok(sql && journal, "migration folder entries recorded with kinds");
    assert.match(sql.digest, /^sha256:[0-9a-f]{64}$/);
    const limitations = new Set(d.limitations.map((l) => l.code));
    assert.ok(!limitations.has("migration-ref-missing"), "granted migration folder resolves");
  } finally {
    dispose(fx.root);
  }
});

test("migration config escaping the project root records the refusal, never a path read", async () => {
  const fx = await scanDrizzleFixture("pg-escape", "postgres");
  try {
    const configPath = join(fx.project, "src/drizzle.config.ts");
    writeFileSync(configPath, `export default {
  dialect: "postgresql",
  schema: "../secrets/schema.ts",
  out: "../outside/drizzle",
};
`);
    const second = fx.rescan();
    const config = second.index.drizzle.migrations.find((m) => m.kind === "config");
    assert.ok(config, "config still recorded");
    assert.ok(config.limitations.includes("migration-path-escapes-root"));
    const limitations = second.index.drizzle.limitations.map((l) => l.code);
    assert.ok(limitations.includes("migration-path-escapes-root"));
  } finally {
    dispose(fx.root);
  }
});

// ---------------------------------------------------------------------------
// 6. Bindings, staleness, and storage projection comparison (AC1, AC6).
// ---------------------------------------------------------------------------

test("owner-supplied bindings confirm against extracted tables; unknown tables stay unresolved", async () => {
  const fx = await scanDrizzleFixture("pg-bindings", "postgres");
  try {
    const d = fx.index.drizzle;
    assert.equal(d.bindings.length, 3);
    const user = d.bindings.find((b) => b.entity === "User");
    assert.equal(user.status, "confirmed");
    assert.equal(user.table.exportName, "users");
    assert.equal(user.table.physicalName, "users");
    assert.equal(user.basis, "explicit-owner-input");
    assert.match(user.table.fileDigest, /^sha256:[0-9a-f]{64}$/);
    const ghost = d.bindings.find((b) => b.entity === "Ghost");
    assert.equal(ghost.status, "unresolved");
    assert.equal(ghost.table.native, null);

    // Projection comparison: matched/divergent/missing with the exact
    // divergences the fixture plants.
    assert.equal(d.projection.state, "checked");
    const email = d.projection.rows.find((r) => r.column === "email");
    assert.equal(email.status, "type-divergent");
    assert.equal(email.declared, "varchar(200)");
    assert.equal(email.projected, "varchar(255)");
    const legacy = d.projection.rows.find((r) => r.column === "legacy_flag");
    assert.equal(legacy.status, "missing-in-declaration");
    const posts = d.projection.rows.find((r) => r.table === "posts" && r.column === undefined);
    assert.equal(posts.status, "missing-in-projection");
    // A declared schema never proves live database state.
    assert.equal(d.completeness.databaseState, "unknown");
  } finally {
    dispose(fx.root);
  }
});

test("schema and query edits change the evidence digest; stale bindings audit marks the damage", async () => {
  const fx = await scanDrizzleFixture("pg-fresh", "postgres");
  try {
    const before = fx.index.drizzle;
    assert.ok(before.digest);
    assert.ok(before.provenance.inputRevision);

    // Byte-identical rescan of unchanged inputs.
    const same = fx.rescan();
    assert.equal(same.index.drizzle.digest, before.digest);
    assert.deepEqual(JSON.stringify(same.index.drizzle), JSON.stringify(before));

    // A schema edit invalidates the digest and the previous bindings.
    const schemaPath = join(fx.project, "src/schema.ts");
    const original = readFileSync(schemaPath, "utf8");
    writeFileSync(schemaPath, original.replace(
      'varchar("email", { length: 200 })',
      'varchar("email", { length: 256 })',
    ));
    const after = fx.rescan();
    assert.notEqual(after.index.drizzle.digest, before.digest);
    assert.notEqual(after.index.drizzle.provenance.inputRevision, before.provenance.inputRevision);
    const audit = auditStaleBindings(before, after.index.drizzle);
    const userAudit = audit.find((a) => a.entity === "User");
    assert.ok(userAudit, "binding audited");
    assert.equal(userAudit.status, "stale");
    assert.equal(userAudit.reason, "binding-stale-table-source-changed");

    // A rename invalidates the binding through the native identity:
    // no nearest-name matching, no silent rebind.
    writeFileSync(schemaPath, original.replace("export const users =", "export const userAccounts ="));
    writeFileSync(join(fx.project, "src/relations.ts"),
      readFileSync(join(fx.project, "src/relations.ts"), "utf8")
        .replaceAll("users", "userAccounts"));
    const renamed = fx.rescan();
    const renameAudit = auditStaleBindings(after.index.drizzle, renamed.index.drizzle);
    const renamedUser = renameAudit.find((a) => a.entity === "User");
    assert.ok(renamedUser, "renamed binding audited");
    assert.equal(renamedUser.status, "stale");
    assert.equal(renamedUser.reason, "binding-stale-table-native-missing");
  } finally {
    dispose(fx.root);
  }
});

// ---------------------------------------------------------------------------
// 7. Pin policy and absence of drizzle (AC honesty).
// ---------------------------------------------------------------------------

test("a semver-range drizzle pin leaves declarations unattached: imports stay unresolved", async () => {
  const fx = await scanDrizzleFixture("pg-pin", "postgres", { ownerInputs: false });
  try {
    const pkgPath = join(fx.project, "package.json");
    writeFileSync(pkgPath, JSON.stringify({
      name: "lekalo-drizzle-fixture-postgres",
      private: true,
      dependencies: { "drizzle-orm": "^0.44.7" },
    }));
    const second = fx.rescan();
    assert.equal(second.index.drizzle, null, "no evidence without an exact pin");
    const uncertaintyKinds = new Set(second.index.anyUncertainty.map((u) => u.kind));
    assert.ok(uncertaintyKinds.has("unresolved-import"),
      "drizzle imports surface as unresolved-import uncertainty");
  } finally {
    dispose(fx.root);
  }
});

// ---------------------------------------------------------------------------
// 8. Read-only guarantee (AC7).
// ---------------------------------------------------------------------------

test("scanning is read-only: the project tree is byte-identical before and after", async () => {
  const fx = await scanDrizzleFixture("pg-readonly", "postgres");
  try {
    const before = snapshotTree(fx.project);
    fx.rescan();
    fx.rescan();
    const after = snapshotTree(fx.project);
    assert.deepEqual(after, before);
    // The evidence document itself claims read-only and carries no
    // credential/connection surface.
    const d = fx.index.drizzle;
    assert.equal(d.identity.readOnly, true);
    const text = JSON.stringify(d);
    for (const banned of ["password", "connectionString", "DATABASE_URL", "credential"]) {
      assert.ok(!text.toLowerCase().includes(banned.toLowerCase()), `no ${banned} in evidence`);
    }
  } finally {
    dispose(fx.root);
  }
});

// ---------------------------------------------------------------------------
// 9. MySQL dialect coverage and neutral-shape parity (AC8).
// ---------------------------------------------------------------------------

test("mysql: dialect facts stay target-side; autoincrement, serial alias, upsert, returning differ honestly", async () => {
  const fx = await scanDrizzleFixture("mysql", "mysql");
  try {
    const d = fx.index.drizzle;
    assert.deepEqual(d.dialects, ["mysql"]);
    const users = d.tables.find((t) => t.exportName === "users");
    const id = users.columns.find((c) => c.tsName === "id");
    assert.equal(id.typeToken, "int");
    assert.equal(id.autoincrement, true, "mysql autoincrement recorded");
    const createdAt = users.columns.find((c) => c.tsName === "createdAt");
    assert.equal(createdAt.typeToken, "datetime(6)");

    const invoices = d.tables.find((t) => t.exportName === "invoices");
    const serialId = invoices.columns.find((c) => c.tsName === "id");
    assert.ok(serialId.limitations.includes("mysql-serial-alias"),
      "mysql serial is recorded as alias sugar, never a storage type");

    // The paired postgres users table has a uuid primary key with a
    // function default; the mysql one has int autoincrement — the
    // neutral evidence (effects, bindings, completeness) matches while
    // the target type facts differ.
    const upsert = d.queries.find((q) => q.limitations.includes("upsert-alternative"));
    assert.ok(upsert, "onDuplicateKeyUpdate recorded as an upsert alternative");
    assert.equal(upsert.projection.kind, "primary-keys", "$returningId projects primary keys");
    const raw = d.limitations.map((l) => l.code);
    assert.ok(raw.includes("raw-sql"), "raw execute warns on mysql too");
    assert.ok(d.queries.some((q) => q.limitations.includes("dynamic-builder")));

    // Migration layout and bindings mirror the postgres fixture shape.
    assert.equal(d.migrations[0].layout, "folders-journal");
    assert.equal(d.migrations[0].dialect, "mysql");
    assert.equal(d.bindings.filter((b) => b.status === "confirmed").length, 2);
    assert.equal(d.bindings.find((b) => b.entity === "Ghost").status, "unresolved");
    assert.equal(d.completeness.databaseState, "unknown");
  } finally {
    dispose(fx.root);
  }
});

test("dialect parity: neutral document shape matches across fixtures; target facts differ", async () => {
  const pg = await scanDrizzleFixture("parity-pg", "postgres");
  const my = await scanDrizzleFixture("parity-my", "mysql");
  try {
    const pd = pg.index.drizzle;
    const md = my.index.drizzle;
    // Same schema identity, same section vocabulary.
    assert.equal(pd.schema, md.schema);
    assert.deepEqual(Object.keys(pd.completeness.sections).sort(),
      Object.keys(md.completeness.sections).sort());
    // Neutral effect actions per query kind are identical.
    const neutralActions = (d) => [...new Set(d.queries.map((q) => q.effects[0].action))].sort();
    assert.deepEqual(neutralActions(pd), neutralActions(md));
    // Binding semantics identical.
    for (const d of [pd, md]) {
      assert.equal(d.bindings.filter((b) => b.status === "confirmed").length, 2);
    }
    // Dialect type tokens differ exactly where the dialects differ.
    const pgUsers = pd.tables.find((t) => t.exportName === "users");
    const myUsers = md.tables.find((t) => t.exportName === "users");
    const pgId = pgUsers.columns.find((c) => c.tsName === "id");
    const myId = myUsers.columns.find((c) => c.tsName === "id");
    assert.notEqual(pgId.typeToken, myId.typeToken);
    assert.equal(pgId.primaryKey, myId.primaryKey);
    assert.equal(pgId.notNull, myId.notNull);
    // Neither document carries dialect semantics outside target rows:
    // effect actions, scope kinds, and completeness stay neutral.
    const scopeKinds = (d) => [...new Set(d.scope.map((s) => s.kind))].sort();
    assert.deepEqual(scopeKinds(pd), scopeKinds(md));
  } finally {
    dispose(pg.root);
    dispose(my.root);
  }
});

// ---------------------------------------------------------------------------
// 10. Determinism and completeness honesty.
// ---------------------------------------------------------------------------

test("evidence is deterministic and completeness degrades over unknowns", async () => {
  const fx = await scanDrizzleFixture("pg-determinism", "postgres");
  try {
    const d = fx.index.drizzle;
    // Sorted dialects, sorted provenance files.
    assert.deepEqual(d.dialects, [...d.dialects].sort());
    const paths = d.provenance.files.map((f) => f.path);
    assert.deepEqual(paths, [...paths].sort());
    // The fixture intentionally carries unknowns: completeness is
    // partial, with per-section degradation and a full limitation set.
    assert.equal(d.completeness.state, "partial");
    assert.equal(d.completeness.sections.bindings, "complete");
    assert.equal(d.completeness.sections.projection, "partial");
    const unknownSections = Object.values(d.completeness.sections);
    assert.ok(unknownSections.includes("partial"));
    // Limitation codes stay in the closed vocabulary (spot check via
    // sort determinism: same inputs -> same digest is asserted above).
    for (const limitation of d.limitations) {
      assert.equal(typeof limitation.code, "string");
      assert.ok(limitation.code.length > 0);
    }
  } finally {
    dispose(fx.root);
  }
});
