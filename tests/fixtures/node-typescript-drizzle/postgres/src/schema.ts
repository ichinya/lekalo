import { sql } from "drizzle-orm";
import {
  boolean,
  integer,
  jsonb,
  pgEnum,
  pgTable,
  primaryKey,
  serial,
  text,
  timestamp,
  uniqueIndex,
  index,
  uuid,
  varchar,
} from "drizzle-orm/pg-core";

export const postStatus = pgEnum("post_status", ["draft", "published"]);

export const users = pgTable("users", {
  id: uuid("id").primaryKey().defaultRandom(),
  email: varchar("email", { length: 200 }).notNull(),
  tenantId: uuid("tenant_id").notNull(),
  metadata: jsonb("metadata"),
  isActive: boolean("is_active").notNull().default(true),
  createdAt: timestamp("created_at", { precision: 6 }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { precision: 6 }).notNull().$onUpdate(() => new Date()),
}, (t) => [
  uniqueIndex("users_email_tenant").on(t.email, t.tenantId),
]);

export const posts = pgTable("posts", {
  id: serial("id").primaryKey(),
  authorId: integer("author_id").notNull().references(() => users.id, { onDelete: "cascade" }),
  tenantId: uuid("tenant_id").notNull(),
  status: postStatus("status").notNull().default("draft"),
  slug: varchar("slug", { length: 300 }).notNull(),
  body: text("body"),
  searchVector: text("search_vector").default(sql`to_tsvector('english', body)`),
}, (t) => [
  index("posts_author_idx").on(t.authorId, t.slug),
]);

export const taskTags = pgTable("task_tags", {
  taskId: uuid("task_id").notNull().references(() => users.id),
  tagId: uuid("tag_id").notNull().references(() => posts.id),
}, (t) => [
  primaryKey({ columns: [t.taskId, t.tagId] }),
]);

export const auditLog = pgTable("audit_log", {
  id: serial("id").primaryKey(),
  entity: varchar("entity", { length: 64 }).notNull(),
  payload: jsonb("payload").$defaultFn(() => ({})),
});
