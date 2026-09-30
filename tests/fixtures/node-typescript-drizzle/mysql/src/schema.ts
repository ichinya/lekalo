import {
  datetime,
  int,
  json,
  mysqlTable,
  serial,
  text,
  uniqueIndex,
  varchar,
} from "drizzle-orm/mysql-core";

export const users = mysqlTable("users", {
  id: int("id").autoincrement().primaryKey(),
  email: varchar("email", { length: 200 }).notNull(),
  tenantId: int("tenant_id").notNull(),
  preferences: json("preferences"),
  createdAt: datetime("created_at", { precision: 6 }).notNull().defaultNow(),
}, (t) => [
  uniqueIndex("users_email_tenant").on(t.email, t.tenantId),
]);

export const invoices = mysqlTable("invoices", {
  id: serial("id").primaryKey(),
  ownerId: int("owner_id").notNull().references(() => users.id, { onDelete: "cascade" }),
  tenantId: int("tenant_id").notNull(),
  memo: text("memo"),
});

export const invoiceLines = mysqlTable("invoice_lines", {
  invoiceId: int("invoice_id").notNull().references(() => invoices.id),
  lineNo: int("line_no").notNull().references(() => users.id),
});
