import { int, mysqlTable, varchar } from "drizzle-orm/mysql-core";

// All names and records are synthetic. MySQL declaration, not SQLite.
export const tasks = mysqlTable("tutorial_tasks", {
  id: varchar("id", { length: 32 }).primaryKey(),
  title: varchar("title", { length: 128 }).notNull(),
  priority: int("priority").notNull().default(0),
});
export type TaskRow = typeof tasks.$inferSelect;
