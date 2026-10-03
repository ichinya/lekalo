// Authored scanner aid for the MySQL table vocabulary, never SQLite.
interface Column { primaryKey(): Column; notNull(): Column; default(value: number): Column; }
export function varchar(name: string, options: { length: number }): Column;
export function int(name: string): Column;
export function mysqlTable(name: string, columns: Record<string, Column>): { id: unknown; $inferSelect: { id: string; title: string; priority: number } };
