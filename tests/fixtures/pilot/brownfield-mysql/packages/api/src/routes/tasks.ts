import { Hono, type Context } from "hono";
import type { TaskRepository } from "../../../data/src/repository.ts";

type Environment = { Bindings: { repository: TaskRepository } };
export const app = new Hono<Environment>();

export async function submitTask(c: Context<Environment>) {
  let value: unknown;
  try { value = await c.req.json(); } catch { return c.json({ error: "invalid-json" }, 400); }
  if (typeof value !== "object" || value === null || Array.isArray(value)) return c.json({ error: "invalid-input" }, 400);
  const row = value as Record<string, unknown>;
  if (typeof row.id !== "string" || !/^[a-z0-9_-]{1,32}$/.test(row.id) || typeof row.title !== "string" || row.title.length < 1 || row.title.length > 128 || !Number.isInteger(row.priority) || Number(row.priority) < 0 || Number(row.priority) > 9) {
    return c.json({ error: "invalid-input" }, 400);
  }
  const stored = await c.env.repository.create({ id: row.id, title: row.title, priority: Number(row.priority) });
  return c.json(stored, 201);
}

app.post("/tasks", submitTask);
