import { relations } from "drizzle-orm";
import { invoices, users } from "./schema.js";

export const usersRelations = relations(users, ({ many }) => ({
  invoices: many(invoices),
}));

export const invoicesRelations = relations(invoices, ({ one }) => ({
  owner: one(users, { fields: [invoices.ownerId], references: [users.id] }),
}));
