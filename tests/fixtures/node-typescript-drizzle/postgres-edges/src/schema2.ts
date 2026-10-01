import { integer, pgTable, serial, text } from "drizzle-orm/pg-core";

// Deliberately ambiguous: the same export name as src/schema.ts but a
// different physical table. A binding naming `users` must refuse to
// guess between the two and record the ambiguity instead.
export const users = pgTable("users_v2", {
  id: serial("id").primaryKey(),
  note: text("note"),
});
