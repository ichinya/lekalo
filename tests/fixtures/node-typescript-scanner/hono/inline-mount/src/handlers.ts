import type { Context } from "hono";

export function insideHandler(c: Context) {
  return c.json({ inside: true });
}

export function deepHandler(c: Context) {
  return c.json({ deep: true });
}

export const enabled = true;
