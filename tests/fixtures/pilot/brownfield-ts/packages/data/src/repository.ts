// Brownfield fixture repository (issue #118): the storage-writing half
// of the submission flow. Synthetic; every name is fictional. The
// drizzle handle is type-level only — the fixture never runs.
import { drizzle } from "drizzle-orm/better-sqlite3";
import { tasks, type NewTaskRow, type TaskRow } from "./schema.js";

export const db = drizzle.mock();

/** Persist one submitted task row and answer the stored record. */
export function insertTask(row: NewTaskRow): TaskRow | null {
  const stored = db.insert(tasks).values(row).returning().all();
  return stored[0] ?? null;
}

/** Read one stored task row by its id. */
export function findTaskById(id: string): TaskRow | null {
  const stored = db.select().from(tasks).all() as unknown as TaskRow[];
  const match = stored.filter((row: TaskRow) => row.id === id);
  return match[0] ?? null;
}
