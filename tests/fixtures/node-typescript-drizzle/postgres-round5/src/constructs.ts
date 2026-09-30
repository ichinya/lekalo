import { relations } from "drizzle-orm";
import { integer, text } from "drizzle-orm/pg-core";
import { posts, users } from "./schema.js";

// ---------------------------------------------------------------------------
// Fix round 5 MAJOR 2: type-provable construct callees. A declare-const
// binding (or a parameter) typed by a construct type query has no value
// declaration to follow — previously a silent zero-row drop while the
// sections claimed complete. The declared type IS the identity anchor.
// ---------------------------------------------------------------------------

// The factory type query anchors to the callable interface
// (`PgTableFn`): the construct identity comes from the import type's
// member spelling.
declare const pt: typeof import("drizzle-orm/pg-core").pgTable;
export const widgets = pt("widgets", {
  id: integer("id").primaryKey(),
  label: text("label"),
});

// The construct function type query anchors to the vendored function
// symbol directly.
declare const rel: typeof relations;
export const typedRelation = rel(users, ({ many }) => ({
  labels: many(posts),
}));

// Typed parameter form of the same anchor.
export function typedParamCallee(cb: typeof relations) {
  return cb(posts, ({ one }) => ({
    author: one(users, { fields: [posts.authorId], references: [users.id] }),
  }));
}
