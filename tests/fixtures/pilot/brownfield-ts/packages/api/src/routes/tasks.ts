// Brownfield fixture API surface (issue #118): the submission flow the
// pilot binds as `task.submit`. The POST /tasks route delegates to the
// named handler below, which persists through the data package's
// Drizzle repository. Synthetic; every name is fictional.
import { Hono, type Context } from "hono";
import { insertTask } from "@brownfield-ts/data";

export const tasksRouter = new Hono();

/** The named submission handler: validate, persist, answer the row. */
export function submitTask(c: Context) {
  const body = c.req.valid() as { title?: string; priority?: number };
  if (typeof body.title !== "string" || body.title.length === 0) {
    return c.json({ error: "title-required" }, 400);
  }
  const row = insertTask({
    id: crypto.randomUUID(),
    title: body.title,
    state: "open",
    priority: body.priority ?? 0,
    createdAt: new Date().toISOString(),
  });
  if (row === null) {
    return c.json({ error: "persist-failed" }, 500);
  }
  return c.json(row, 201);
}

tasksRouter.post("/tasks", submitTask);
