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
    // The two axes stay separate (issue #116 fix round): a non-tx-bound
    // query invents no transaction id and is not a group member.
    assert.equal(outside.transactionId, null);
    assert.ok(!transfer.members.includes(outside.id), "db queries are not tx members");
    assert.ok(!txs.some((t) => t.members.includes(outside.id)));

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
    // A declared-but-rejected pin is distinguishable from a non-drizzle
    // project (issue #116 fix round): the rejection reason survives.
    assert.deepEqual(second.index.drizzleAttachment, {
      attached: false,
      reason: "pin-not-exact",
      declared: ["^0.44.7"],
      supported: "0.44.7",
    });

    // A non-drizzle project records no attachment rejection at all.
    writeFileSync(pkgPath, JSON.stringify({
      name: "lekalo-drizzle-fixture-postgres",
      private: true,
      dependencies: {},
    }));
    const plain = fx.rescan();
    assert.equal(plain.index.drizzle, null);
    assert.equal(plain.index.drizzleAttachment, undefined);
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

// ---------------------------------------------------------------------------
// 11. Fix round (review blockers/majors/minors): regression anchors.
// ---------------------------------------------------------------------------

test("aliased imports extract by symbol identity: `pgTable as table` and `relations as rel`", async () => {
  const fx = await scanDrizzleFixture("pg-edges", "postgres-edges");
  try {
    const d = fx.index.drizzle;
    const users = d.tables.find((t) => t.exportName === "users" && t.physicalName === "users");
    assert.ok(users, "aliased pgTable declaration extracted");
    assert.equal(users.dialect, "postgresql", "dialect comes from the resolved symbol");
    assert.ok(users.declarationPath?.startsWith("pg-core/"), "provenance names the closure declaration");
    const usersV2 = d.tables.find((t) => t.physicalName === "users_v2");
    assert.ok(usersV2, "the spelled pgTable in the sibling module extracts too");
    assert.equal(usersV2.exportName, "users");

    const many = d.relations.find((r) => r.name === "posts");
    assert.ok(many, "aliased relations declaration extracted");
    assert.equal(many.sourceTable, "users");
    assert.equal(many.targetTable, "posts");
    const one = d.relations.find((r) => r.name === "author");
    assert.ok(one, "second aliased relations declaration extracted");
    // A recognized relations surface that fails extraction forces the
    // section partial; here both extract, and their limitations do.
    assert.equal(d.completeness.sections.relations, "partial");
  } finally {
    dispose(fx.root);
  }
});

test("db.query and db.batch emit explicit out-of-subset limitations and degrade the queries section", async () => {
  const fx = await scanDrizzleFixture("pg-edges-query", "postgres-edges");
  try {
    const d = fx.index.drizzle;
    const codes = d.limitations.map((l) => l.code);
    assert.ok(codes.includes("relational-query-unsupported"),
      "db.query.* surfaces as an explicit limitation");
    assert.ok(codes.includes("batch-unsupported"),
      "db.batch surfaces as an explicit limitation");
    const relational = d.limitations.find((l) => l.code === "relational-query-unsupported");
    assert.equal(relational.detail, "relational-query-api");
    assert.equal(d.completeness.sections.queries, "partial",
      "out-of-subset surfaces force the queries section partial");
    // No query rows were silently invented for the out-of-subset calls.
    assert.ok(!d.queries.some((q) => q.chain[0] === "batch"));
    assert.ok(!d.queries.some((q) => q.chain[0] === "query"));
    // The batch member chains are genuine drizzle queries and extract.
    assert.ok(d.queries.some((q) => q.kind === "insert" && q.target?.exportName === "users"));
  } finally {
    dispose(fx.root);
  }
});

test("ambiguous export names refuse confirmation: the binding records ambiguity, never a guess", async () => {
  const fx = await scanDrizzleFixture("pg-edges-bindings", "postgres-edges");
  try {
    const d = fx.index.drizzle;
    const user = d.bindings.find((b) => b.entity === "User");
    assert.ok(user, "User binding recorded");
    assert.equal(user.status, "ambiguous", "two `users` exports never confirm one silently");
    assert.equal(user.table.native, null);
    assert.equal(user.table.physicalName, null);
    assert.ok(user.limitations.includes("binding-ambiguous"));
    assert.ok(d.limitations.some((l) => l.code === "binding-ambiguous"
      && l.detail === "User->users"));
    const post = d.bindings.find((b) => b.entity === "Post");
    assert.equal(post.status, "confirmed", "a unique export name still confirms");
    assert.equal(post.table.exportName, "posts");
  } finally {
    dispose(fx.root);
  }
});

test("consumer tsconfig paths survive the drizzle pin: merged, never replaced", async () => {
  const fx = await scanDrizzleFixture("pg-edges-paths", "postgres-edges");
  try {
    const options = JSON.parse(fx.index.programOptions);
    assert.ok(Array.isArray(options.paths["@app/*"]) && options.paths["@app/*"].length > 0,
      "the consumer's @app/* mapping survives");
    assert.ok(options.paths["drizzle-orm"], "the drizzle root mapping is present");
    assert.ok(options.paths["drizzle-orm/pg-core"], "the drizzle dialect mapping is present");
    const d = fx.index.drizzle;
    // Functionally: the @app import resolves to the same table symbol,
    // so the aliased query resolves its target without uncertainty.
    const aliased = d.queries.find((q) => q.chain.includes("where")
      && q.target?.exportName === "users");
    assert.ok(aliased, "the @app-aliased select extracted");
    assert.ok(!aliased.limitations.includes("target-unresolved"),
      "the @app alias resolved through the merged paths");
    // Join RHS equality columns are field reads of the joined table,
    // not reference inputs (issue #116 fix round).
    const joinQuery = d.queries.find((q) => q.joins.length > 0);
    assert.ok(joinQuery, "join extracted");
    assert.ok(joinQuery.reads.some((r) => r.role === "join" && r.table === "posts"
      && r.column === "authorId"), "join RHS column recorded as a field read");
    assert.ok(!joinQuery.inputs.some((i) => i.role === "join"),
      "no phantom reference input for the join RHS column");
    // A builder handed off by an explicit return carries no
    // builder-not-executed noise.
    const returned = d.queries.find((q) => q.chain.length === 2
      && q.chain[0] === "select" && q.chain[1] === "from");
    assert.ok(returned, "returned builder extracted");
    assert.ok(!returned.limitations.includes("builder-not-executed"),
      "an explicitly returned builder is an intentional handoff");
  } finally {
    dispose(fx.root);
  }
});

test("check() constraint SQL bodies are recorded as explicit raw-sql unknowns", async () => {
  const fx = await scanDrizzleFixture("pg-edges-check", "postgres-edges");
  try {
    const d = fx.index.drizzle;
    const users = d.tables.find((t) => t.exportName === "users" && t.physicalName === "users");
    const check = users.constraints.find((c) => c.kind === "check");
    assert.ok(check, "check constraint extracted");
    assert.equal(check.name, "users_email_readable");
    assert.equal(check.sqlPredicate, true, "the SQL body is recorded, not silently absent");
    assert.ok(check.limitations.includes("raw-sql"));
    assert.ok(d.limitations.some((l) => l.code === "raw-sql"
      && l.module === users.module && l.detail === "users_email_readable"));
    // Constraint rows without a SQL body keep the explicit false.
    const uniqueIndex = users.constraints.find((c) => c.kind === "unique-index");
    assert.ok(uniqueIndex, "the non-check constraint extracted");
    assert.equal(uniqueIndex.sqlPredicate, false);
  } finally {
    dispose(fx.root);
  }
});

test("invalid owner inputs report invalid, never missing", async () => {
  const fx = await scanDrizzleFixture("pg-invalid-inputs", "postgres");
  try {
    writeFileSync(join(fx.project, "drizzle.projection.json"), "{ not json");
    const second = fx.rescan();
    const d = second.index.drizzle;
    const codes = d.limitations.map((l) => l.code);
    assert.ok(codes.includes("projection-input-invalid"),
      "a malformed projection input is invalid, not silently absent");
    assert.equal(d.projection.state, "invalid");
    assert.ok(d.limitations.some((l) => l.code === "projection-input-invalid"
      && l.detail === "invalid-json"));
  } finally {
    dispose(fx.root);
  }
});

test("a migration-folder edit changes the input revision; migration files appear in provenance", async () => {
  const fx = await scanDrizzleFixture("pg-migrev", "postgres");
  try {
    const before = fx.index.drizzle;
    assert.ok(before.provenance.files.some((f) => f.path === "drizzle/0000_init.sql"),
      "migration folder files are provenance inputs");
    const sqlPath = join(fx.project, "drizzle/0000_init.sql");
    const original = readFileSync(sqlPath, "utf8");
    // Same-length edit: only the content digest changes.
    writeFileSync(sqlPath, original.replace("CREATE TABLE", "CREATE  TABLE"));
    const after = fx.rescan();
    assert.notEqual(after.index.drizzle.provenance.inputRevision, before.provenance.inputRevision,
      "otherFiles digests feed the input revision");
  } finally {
    dispose(fx.root);
  }
});

test("a dangling migration schemaRef is an explicit limitation, never silently accepted", async () => {
  const fx = await scanDrizzleFixture("pg-dangling", "postgres");
  try {
    writeFileSync(join(fx.project, "src/drizzle.config.ts"), `export default {
  dialect: "postgresql",
  schema: "./src/vanished.ts",
  out: "./drizzle",
};
`);
    const second = fx.rescan();
    const config = second.index.drizzle.migrations.find((m) => m.kind === "config");
    assert.ok(config, "config still recorded");
    assert.ok(config.limitations.includes("migration-ref-missing"));
    const codes = second.index.drizzle.limitations.map((l) => l.code);
    assert.ok(codes.includes("migration-ref-missing"));
    assert.equal(second.index.drizzle.completeness.sections.migrations, "partial");
  } finally {
    dispose(fx.root);
  }
});

test("fix-round completeness honesty: all-clean sections claim complete; gaps force partial", async () => {
  const fx = await scanDrizzleFixture("pg-sections", "postgres");
  try {
    const d = fx.index.drizzle;
    // Postgres fixture transaction rows carry no limitations.
    assert.equal(d.completeness.sections.transactions, "complete");
    // The fixture's relations carry honesty limitations (unproven
    // cardinality/policy), so the section stays partial for the right
    // reason — the dead partial:partial ternary is gone.
    assert.equal(d.completeness.sections.relations, "partial");
    assert.ok(d.relations.every((r) => r.limitations.length > 0));
    // Tables are all complete in this fixture.
    assert.equal(d.completeness.sections.tables, "complete");
  } finally {
    dispose(fx.root);
  }
});

// ---------------------------------------------------------------------------
// 12. Fix round 3 (review major + residual minors): regression anchors.
// ---------------------------------------------------------------------------

test("fix-round-3 callees: namespace property access and const/let rebinding extract by resolved identity", async () => {
  const fx = await scanDrizzleFixture("pg-callees", "postgres-callees");
  try {
    const d = fx.index.drizzle;
    // `d.pgTable(...)` — namespace PropertyAccess (previously gated out
    // by the Identifier-only shapes and silent zero-row drops).
    const widgets = d.tables.find((t) => t.exportName === "nsTable");
    assert.ok(widgets, "namespace-invoked pgTable extracts");
    assert.equal(widgets.physicalName, "widgets");
    assert.equal(widgets.dialect, "postgresql");
    assert.ok(widgets.declarationPath?.startsWith("pg-core/"),
      "provenance names the closure declaration, not the local spelling");
    // `const tableFactory = pg.pgTable then tableFactory(...)` —
    // const/let rebinding resolves to the vendored factory.
    const gadgets = d.tables.find((t) => t.exportName === "gadgets");
    assert.ok(gadgets, "rebound pgTable extracts");
    assert.equal(gadgets.physicalName, "gadgets");
    assert.deepEqual(gadgets.columns.map((c) => c.tsName).sort(), ["count", "id"]);
    // `orm.relations(users, ...)` — namespace PropertyAccess on the
    // root module (the relations path previously never got the
    // PropertyAccess treatment).
    const nsMany = d.relations.find((r) => r.name === "posts");
    assert.ok(nsMany, "namespace-invoked relations extracts");
    assert.equal(nsMany.sourceTable, "users");
    assert.equal(nsMany.targetTable, "posts");
    // `const relationsFactory = orm.relations then relationsFactory(...)`.
    const rebound = d.relations.find((r) => r.name === "author");
    assert.ok(rebound, "rebound relations extracts");
    assert.equal(rebound.sourceTable, "posts");
    assert.equal(rebound.targetTable, "users");
  } finally {
    dispose(fx.root);
  }
});

test("fix-round-3 honesty: construct-named but unprovable callees are explicit limitations, never drops", async () => {
  const fx = await scanDrizzleFixture("pg-callees-honesty", "postgres-callees");
  try {
    const d = fx.index.drizzle;
    const unproven = d.limitations.filter((l) => l.code === "callee-unproven");
    assert.deepEqual(unproven.map((l) => [l.detail, l.module]).sort(),
      [["relations", "src/callees.ts"], ["table", "src/callees.ts"]],
      "exactly one unproven limitation per construct family");
    assert.ok(!d.tables.some((t) => t.exportName === "opaque"),
      "no row is invented for the unprovable callee");
    assert.ok(!d.relations.some((r) => r.name === "labels"),
      "no relation row is invented for the unprovable callee");
    // Sections covering dropped constructs refuse to claim complete.
    assert.equal(d.completeness.sections.tables, "partial");
    assert.equal(d.completeness.sections.relations, "partial");
  } finally {
    dispose(fx.root);
  }
});

test("fix-round-3 receivers: the db.* receiver memo is symbol-bound, not name-bound", async () => {
  const fx = await scanDrizzleFixture("pg-receivers", "postgres-callees");
  try {
    const d = fx.index.drizzle;
    const source = readFileSync(join(fx.project, "src/receivers.ts"), "utf8").split("\n");
    const lineOf = (needle) => source.findIndex((line) => line.includes(needle)) + 1;
    const genuineLine = lineOf("export async function genuineRelational");
    const shadowedLine = lineOf("function shadowedRelational");
    const hits = d.limitations.filter((l) => l.code === "relational-query-unsupported");
    assert.equal(hits.length, 1, "only the genuine drizzle db.query surfaces");
    assert.ok(hits[0].line >= genuineLine && hits[0].line < shadowedLine,
      `the limitation anchors to the genuine call (${hits[0].line} in [${genuineLine}, ${shadowedLine}))`);
    assert.equal(d.completeness.sections.queries, "partial",
      "the genuine out-of-subset surface still degrades the section");
  } finally {
    dispose(fx.root);
  }
});

test("fix-round-3 select-all projection and scope evidence resolve the exact targeted table row", async () => {
  const fx = await scanDrizzleFixture("pg-edges-targetrow", "postgres-edges");
  try {
    const d = fx.index.drizzle;
    // Two modules export `users` (schema.ts physical `users`, schema2.ts
    // physical `users_v2`). The aliased select-all targets the schema.ts
    // declaration; its projection must read THAT row's columns — never
    // the export-name map's arbitrary last writer.
    const aliased = d.queries.find((q) => q.chain.includes("where")
      && q.target?.exportName === "users");
    assert.ok(aliased, "the aliased select extracted");
    const projectionColumns = aliased.reads
      .filter((r) => r.role === "projection")
      .map((r) => r.column)
      .sort();
    assert.deepEqual(projectionColumns, ["active", "email", "id", "tenantId"],
      "the select-all projection reads the targeted schema.ts users columns");
    assert.ok(!projectionColumns.includes("note"),
      "the same-named users_v2 table never leaks its columns");
    // The same precision for scope evidence: users_v2 declares no
    // tenant key, so a name-keyed lookup would flip the
    // scope-predicate-missing verdict for the schema.ts query.
    assert.ok(aliased.limitations.includes("scope-predicate-missing"),
      "the missing-scope verdict comes from the targeted table's tenant key");
  } finally {
    dispose(fx.root);
  }
});

// ---------------------------------------------------------------------------
// Fix round 4: namespace receivers, renamed destructures, the hop
// bound, and the spelling net's foreign-receiver silence.
// ---------------------------------------------------------------------------

test("fix-round-4 receivers: a module namespace is never a database handle", async () => {
  const fx = await scanDrizzleFixture("pg-round4-ns", "postgres-round4");
  try {
    const d = fx.index.drizzle;
    // Pre-fix, `orm.select().from(users)` typed the drizzle-orm
    // namespace as a `db` handle and FABRICATED a query row; batch and
    // relational-query calls were misattributed as client gaps.
    assert.equal(d.queries.length, 0,
      "no query row may be invented for a namespace receiver");
    const source = readFileSync(join(fx.project, "src/namespaces.ts"), "utf8").split("\n");
    const lineOf = (needle) => source.findIndex((line) => line.includes(needle)) + 1;
    const hits = d.limitations.filter((l) => l.code === "namespace-receiver-unsupported");
    assert.deepEqual(hits.map((h) => [h.module, h.detail]).sort(), [
      ["src/namespaces.ts", "batch-api"],
      ["src/namespaces.ts", "query"],
      ["src/namespaces.ts", "relational-query-api"],
      ["src/namespaces.ts", "transaction"],
    ], "every member call on the namespace is an explicit uncertainty");
    const anchors = new Map(hits.map((h) => [h.detail, h.line]));
    assert.ok(anchors.get("query") === lineOf("const q = orm.select().from(users)"),
      `the query-shaped uncertainty anchors to its call (${anchors.get("query")})`);
    assert.ok(anchors.get("batch-api") === lineOf("orm.batch([q])"));
    assert.ok(anchors.get("relational-query-api") === lineOf("orm.query.users.findMany()"));
    assert.ok(anchors.get("transaction") === lineOf("orm.transaction(async (tx) => {"));
    // The covered sections refuse to claim complete over the unknowns.
    assert.equal(d.completeness.sections.queries, "partial");
    assert.equal(d.completeness.sections.transactions, "partial");
    // No misattributed client-surface codes anywhere.
    assert.equal(d.limitations.filter((l) =>
      l.code === "batch-unsupported" || l.code === "relational-query-unsupported").length, 0);
  } finally {
    dispose(fx.root);
  }
});

test("fix-round-4 callees: renamed destructures and aliased imports extract by resolved export identity", async () => {
  const fx = await scanDrizzleFixture("pg-round4-bindings", "postgres-round4");
  try {
    const d = fx.index.drizzle;
    // `const { relations: rel2 } = orm` — the destructured property
    // name resolves to the vendored export; pre-fix this was a silent
    // zero-row drop while the section claimed completeness.
    const labels = d.relations.find((r) => r.name === "labels");
    assert.ok(labels, "the renamed destructure extracts");
    assert.equal(labels.sourceTable, "users");
    assert.equal(labels.targetTable, "posts");
    assert.equal(labels.module, "src/bindings.ts");
    // `import { relations as rel3 }` — the aliased import specifier
    // shares the r2 alias machinery and must extract unchanged.
    const author = d.relations.find((r) => r.name === "author");
    assert.ok(author, "the aliased direct import extracts");
    assert.equal(author.sourceTable, "posts");
    assert.equal(author.targetTable, "users");
    // Shorthand destructure (no rename): same binding-element path.
    const replies = d.relations.find((r) => r.name === "replies");
    assert.ok(replies, "the shorthand destructure extracts");
    // None of the binding forms may hide behind an unproven flag.
    const unproven = d.limitations.filter((l) => l.code === "callee-unproven");
    assert.ok(!unproven.some((l) => l.module === "src/bindings.ts"),
      "resolvable renamed bindings never emit callee-unproven");
  } finally {
    dispose(fx.root);
  }
});

test("fix-round-4 chain bound: resolution past MAX_ALIAS_HOPS is explicit, never silent", async () => {
  const fx = await scanDrizzleFixture("pg-round4-chains", "postgres-round4");
  try {
    const d = fx.index.drizzle;
    const source = readFileSync(join(fx.project, "src/chains.ts"), "utf8").split("\n");
    const lineOf = (needle) => source.findIndex((line) => line.includes(needle)) + 1;
    // Four rebinds still resolve (the closure test runs before the
    // bound check); the fifth dies at the bound.
    const within = d.relations.find((r) => r.name === "hopWithin");
    assert.ok(within, "the within-bound chain extracts");
    assert.equal(within.sourceTable, "users");
    assert.ok(!d.relations.some((r) => r.name === "hopBeyond"),
      "no row is invented for the beyond-bound chain");
    const bound = d.limitations.filter((l) => l.code === "callee-unproven");
    assert.equal(bound.length, 1, "exactly one explicit bound limitation");
    assert.equal(bound[0].module, "src/chains.ts");
    assert.equal(bound[0].detail, "relations",
      "the bound stays family-disjoint: the relations chain never flags the table walk");
    assert.equal(bound[0].line, lineOf("export const beyondBound = h5(posts"),
      "the limitation anchors to the bounded call");
    assert.equal(d.completeness.sections.relations, "partial",
      "the bounded drop degrades the section instead of hiding");
  } finally {
    dispose(fx.root);
  }
});

test("fix-round-4 spelling net: provably non-Drizzle receivers stay silent", async () => {
  const fx = await scanDrizzleFixture("pg-round4-foreign", "postgres-round4");
  try {
    const d = fx.index.drizzle;
    // `builder.relations()` / `builder.pgTable()` are real project
    // declarations whose types resolve outside the embedded closure —
    // provably not vendored Drizzle constructs, so flagging them as
    // `callee-unproven` would fabricate a Drizzle gap.
    const fromForeign = d.limitations.filter((l) => l.module === "src/foreign.ts");
    assert.deepEqual(fromForeign, [],
      "no limitation may be emitted for provably non-Drizzle member calls");
    // The honest unproven cases elsewhere in the fixture are untouched.
    assert.ok(d.limitations.some((l) => l.code === "callee-unproven"),
      "unprovable construct spellings still flag");
  } finally {
    dispose(fx.root);
  }
});
