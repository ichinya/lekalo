import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import mysql from "mysql2/promise";
import { eq } from "drizzle-orm";
import { drizzle } from "drizzle-orm/mysql2";
import { app } from "../packages/api/src/routes/tasks.ts";
import { tasks } from "../packages/data/src/schema.ts";
import { createRepository } from "../packages/data/runtime/mysql.ts";

test("isolated MySQL: explicit migration, real Hono/driver create/read, transaction rollback", async () => {
  assert.ok(process.env.LEKALO_DOCS_MYSQL_PORT, "required MySQL lane: missing port is a failure, never a skip");
  // Only the dedicated tutorial database is admitted; never use a production URL.
  const pool = mysql.createPool({ host: "127.0.0.1", port: Number(process.env.LEKALO_DOCS_MYSQL_PORT), user: "tutorial", password: "synthetic-tutorial-only", database: "lekalo_docs_tutorial", connectionLimit: 2 });
  const db = drizzle(pool);
  try {
    const [[version]] = await pool.query("SELECT VERSION() AS version");
    assert.match(version.version, /^8\.4\./, "qualified MySQL 8.4 service");
    await pool.query(readFileSync(new URL("../migrations/0000_tasks.sql", import.meta.url), "utf8"));
    // IDs are owned solely by this test in the dedicated synthetic database.
    await db.delete(tasks).where(eq(tasks.id, "docs-created"));
    await db.delete(tasks).where(eq(tasks.id, "docs-rollback"));
    const response = await app.request("/tasks", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ id: "docs-created", title: "Synthetic persisted task", priority: 3 }) }, { repository: createRepository(db) });
    assert.equal(response.status, 201);
    assert.deepEqual(await response.json(), { id: "docs-created", title: "Synthetic persisted task", priority: 3 });
    const [stored] = await db.select().from(tasks).where(eq(tasks.id, "docs-created"));
    assert.equal(stored.title, "Synthetic persisted task");
    await assert.rejects(db.transaction(async (tx) => {
      await tx.insert(tasks).values({ id: "docs-rollback", title: "Rolled back", priority: 0 });
      throw new Error("synthetic-rollback");
    }), /synthetic-rollback/);
    assert.deepEqual(await db.select().from(tasks).where(eq(tasks.id, "docs-rollback")), []);
    await assert.rejects(db.insert(tasks).values(stored), error => error.cause?.code === "ER_DUP_ENTRY" || error.code === "ER_DUP_ENTRY", "actual MySQL duplicate primary key refuses");
    await db.delete(tasks).where(eq(tasks.id, "docs-created"));
  } finally { await pool.end(); }
});
