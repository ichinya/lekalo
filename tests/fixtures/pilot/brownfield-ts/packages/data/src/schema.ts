// Brownfield fixture storage surface (issue #118): the Drizzle SQLite
// table the submission flow writes through. Synthetic; every name is
// generic and fictional.
import { integer, sqliteTable, text } from "drizzle-orm/sqlite-core";

export const tasks = sqliteTable("tasks", {
  id: text("id").primaryKey(),
  title: text("title").notNull(),
  state: text("state").notNull().default("open"),
  priority: integer("priority").notNull().default(0),
  createdAt: text("created_at").notNull(),
});

export type TaskRow = typeof tasks.$inferSelect;
export type NewTaskRow = typeof tasks.$inferInsert;
