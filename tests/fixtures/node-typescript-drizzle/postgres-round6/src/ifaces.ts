import { integer, text, type PgTableFn } from "drizzle-orm/pg-core";
import { int, varchar, type MySqlTableFn } from "drizzle-orm/mysql-core";

// ---------------------------------------------------------------------------
// Fix round 6 MAJOR 2: bindings typed by a factory's callable interface
// anchor to exactly one construct through the closed interface→factory
// map (PgTableFn→pgTable, MySqlTableFn→mysqlTable, and siblings) —
// previously the resolver returned the interface symbol and the
// extractor dropped it silently on the factory-name gate while the
// tables section claimed complete.
// ---------------------------------------------------------------------------

declare const t: PgTableFn;
export const widgets = t("widgets", {
  id: integer("id").primaryKey(),
  label: text("label"),
});

declare const mt: MySqlTableFn;
export const mysqlWidgets = mt("mysql_widgets", {
  id: int("id").primaryKey(),
  label: varchar("label", { length: 32 }),
});

// The interface-typed parameter form shares the same anchor.
export function interfaceParam(pt: PgTableFn) {
  const built = pt("param_built", {
    id: integer("id").primaryKey(),
  });
  return built;
}
