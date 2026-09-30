import { integer, pgTable, serial, text, uuid } from "drizzle-orm/pg-core";

// The base schema: ordinary direct-import declarations every other
// shape in this fixture must extract exactly like.
export const users = pgTable("users", {
  id: uuid("id").primaryKey(),
  email: text("email").notNull(),
  tenantId: uuid("tenant_id").notNull(),
});

export const posts = pgTable("posts", {
  id: serial("id").primaryKey(),
  authorId: integer("author_id").notNull(),
  title: text("title"),
});
