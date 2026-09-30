import type { Context } from "hono";

export function getHandler(c: Context) {
  return c.json({ verb: "GET" });
}

export function postHandler(c: Context) {
  return c.json({ verb: "POST" });
}

export function putHandler(c: Context) {
  return c.json({ verb: "PUT" });
}
