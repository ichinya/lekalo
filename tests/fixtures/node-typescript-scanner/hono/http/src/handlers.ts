import { HTTPException } from "hono/http-exception";
import type { Context } from "hono";

export function listUsers(c: Context) {
  return c.json({ users: [] }, 200);
}

export function missing(c: Context) {
  throw new HTTPException(404, { message: "no user" });
}

export function boom(c: Context) {
  throw new HTTPException();
}

export function created(c: Context) {
  c.status(201);
  return c.json({ id: 1 });
}

export function page(c: Context) {
  return c.html("<html>plain string</html>");
}
