import * as orm from "drizzle-orm";
import * as pg from "drizzle-orm/pg-core";
import { posts, users } from "./schema.js";

// ---------------------------------------------------------------------------
// Recognized constructs behind non-Identifier callees: identity is the
// resolved vendored declaration symbol, never the callee shape.
// ---------------------------------------------------------------------------

// Namespace property access: `pg.pgTable(...)` must extract exactly
// like the spelled import (fix round 3 major).
export const nsTable = pg.pgTable("widgets", {
  id: pg.uuid("id").primaryKey(),
  label: pg.text("label"),
});

// Namespace property access through the root module: `orm.relations`.
export const nsRelations = orm.relations(users, ({ many }) => ({
  posts: many(posts),
}));

// Const rebinding: `const factory = pg.pgTable` is the same vendored
// factory; the rebound callee must extract exactly like the spelled
// name (fix round 3 major).
const tableFactory = pg.pgTable;
export const gadgets = tableFactory("gadgets", {
  id: pg.integer("id").primaryKey(),
  count: pg.integer("count"),
});

const relationsFactory = orm.relations;
export const reboundRelations = relationsFactory(posts, ({ one }) => ({
  author: one(users, { fields: [posts.authorId], references: [users.id] }),
}));

// ---------------------------------------------------------------------------
// Honesty negatives: construct-NAMED callees the checker cannot prove.
// They must produce explicit `callee-unproven` limitations and section
// gaps — never silent drops, never invented rows.
// ---------------------------------------------------------------------------

const tableBag = { pgTable: pg.pgTable };
export const opaque = tableBag["pgTable"]("opaque", {
  id: pg.integer("id"),
});

const relationsBag = { relations: orm.relations };
export const opaqueRelations = relationsBag["relations"](users, ({ many }) => ({
  labels: many(posts),
}));
