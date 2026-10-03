import { eq } from "drizzle-orm";
import { drizzle } from "drizzle-orm/mysql2";
import { tasks, type TaskRow } from "./schema.ts";

export interface TaskRepository { create(row: TaskRow): Promise<TaskRow>; }
// Offline query-shape evidence; never used as persistence proof.
export const db = drizzle.mock();
export async function findTaskById(id: string) { return db.select().from(tasks).where(eq(tasks.id, id)); }

export async function insertTask(row: TaskRow): Promise<TaskRow> {
  await db.insert(tasks).values(row);
  const [stored] = await db.select().from(tasks).where(eq(tasks.id, row.id));
  if (!stored) throw new Error("persist-failed");
  return stored;
}
