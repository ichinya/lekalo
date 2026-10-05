// Authored scanner aid. Native checks exclude this declaration.
type Row = { id: string; title: string; priority: number };
interface Selection extends PromiseLike<Row[]> { from(table: unknown): Selection; where(predicate: unknown): Selection; }
interface Database { select(): Selection; insert(table: unknown): { values(row: Row): PromiseLike<unknown> }; }
export const drizzle: { mock(): Database };
