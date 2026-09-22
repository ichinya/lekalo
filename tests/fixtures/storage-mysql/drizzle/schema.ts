// Issue #117 hermetic Drizzle/MySQL schema fixture (AC-2).
//
// The fixture declares the planner tables of the committed
// storage-projection golden in Drizzle's mysql-core dialect — the
// exact surface #116's TS scanner will parse. It is asserted
// contract-side only: this file is evidence, never a dependency of
// lekalo-core, and nothing here executes during tests.
//
// The declared columns bind one-to-one onto the mysql namespace
// projection of tests/fixtures/storage-projection/valid/planner-storage.json:
//
//   serial            -> bigint + identity (the alias sugar, never a type)
//   varchar           -> varchar(n)
//   boolean           -> tinyint(1)
//   datetime          -> datetime(6)
//   json              -> json (mysql) / longtext + JSON_VALID (mariadb)
//   uuid keys         -> binary(16)
//   uniqueIndex       -> the declared unique index members
//
// The real-world proof of the non-merge rule lives in this dialect:
// drizzle-orm's mysql serial once emitted `serial AUTO_INCREMENT`,
// invalid on MariaDB 10.6 (drizzle-orm #3333). MySQL and MariaDB stay
// separate versioned capability profiles.

import { boolean, datetime, int, json, mysqlTable, text, uniqueIndex, varchar } from "drizzle-orm/mysql-core";

export const task = mysqlTable(
  "task",
  {
    id: varchar("id", { length: 36 }).primaryKey(),
    title: varchar("title", { length: 200 }).notNull(),
    status: varchar("status", { length: 16 }).notNull(),
    dueDate: datetime("due_date"),
    note: text("note"),
    createdAt: datetime("created_at").notNull(),
    updatedAt: datetime("updated_at").notNull(),
  },
  (table) => [uniqueIndex("idx_task_tenant").on(table.tenantId)],
);

export const tag = mysqlTable("tag", {
  id: varchar("id", { length: 36 }).primaryKey(),
  label: varchar("label", { length: 64 }).notNull(),
});

export const taskTag = mysqlTable(
  "task_tag",
  {
    taskId: varchar("task_id", { length: 36 }).notNull(),
    tagId: varchar("tag_id", { length: 36 }).notNull(),
  },
  (table) => [uniqueIndex("task_tag_pair").on(table.taskId, table.tagId)],
);

export const focusSession = mysqlTable("focus_session", {
  id: int("id").autoincrement().primaryKey(),
  minutes: int("minutes").notNull(),
  startedAt: datetime("started_at").notNull(),
});

export const comment = mysqlTable("comment", {
  id: varchar("id", { length: 36 }).primaryKey(),
  body: text("body").notNull(),
  targetId: varchar("target_id", { length: 36 }).notNull(),
  targetType: varchar("target_type", { length: 64 }).notNull(),
});
