import { Hono } from "hono";

export const app = new Hono();

export function listHandler(c: any) {
  return c.json([]);
}

export const other = new Hono();
other.get("/other", listHandler);

app.get("/items", listHandler);
app.post("/items", (c) => c.json({ created: true }, 201));
app.get("/items/:id", (c) => c.json({ id: c.get("id") }));
app.route("/sub", other);

// A bare helper that shares the conventional request() name: not a
// Hono app call, never evidence and never an uncertainty.
export function request(path: string): string {
  return path;
}
