import type { Context } from "hono";

// One handler bound by four distinct use() events.
export async function reader(c: Context, next: () => Promise<void>) {
  c.set("seen", "yes");
  await next();
}
