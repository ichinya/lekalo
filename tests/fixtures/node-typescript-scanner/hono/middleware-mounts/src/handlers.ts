import type { Context } from "hono";

export function pingHandler(c: Context) {
  return c.json({ ping: true });
}

export function otherHandler(c: Context) {
  return c.json({ other: true });
}

export function openHandler(c: Context) {
  return c.json({ open: true });
}
