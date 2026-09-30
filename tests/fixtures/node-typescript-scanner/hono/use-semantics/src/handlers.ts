import type { Context } from "hono";

export function readerHandler(c: Context) {
  const seen = c.get("seen");
  return c.json({ seen: String(seen) });
}
