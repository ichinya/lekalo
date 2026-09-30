import { sql } from "drizzle-orm";
import {
  boolean,
  check,
  integer,
  pgTable as table,
  serial,
  text,
  uniqueIndex,
  uuid,
  varchar,
} from "drizzle-orm/pg-core";

// Aliased factory: `pgTable as table` must extract exactly like the
// spelled name — factory identity is the resolved declaration symbol,
// never the local callee text.
export const users = table("users", {
  id: uuid("id").primaryKey().defaultRandom(),
  email: varchar("email", { length: 200 }).notNull(),
  tenantId: uuid("tenant_id").notNull(),
  active: boolean("active").notNull().default(true),
}, (t) => [
  uniqueIndex("users_email_tenant").on(t.email, t.tenantId),
  // The check body is raw SQL by construction: recorded as an explicit
  // unknown, never silently absent.
  check("users_email_readable", sql`length(${t.email}) > 3`),
]);

export const posts = table("posts", {
  id: serial("id").primaryKey(),
  authorId: integer("author_id").notNull(),
  title: text("title"),
});
