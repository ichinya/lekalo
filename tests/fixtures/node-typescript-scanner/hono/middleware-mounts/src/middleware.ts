import type { Context, Middleware } from "hono";

// Path-filtered parent middleware with a wildcard: overlap with the
// mounted child routes cannot be proven — conditional, never guessed.
export async function gate(c: Context, next: () => Promise<void>) {
  c.set("gated", "1");
  await next();
}

// Literal parent filter: provably applicable to the /api subtree and
// provably disjoint from the /open subtree.
export async function audit(c: Context, next: () => Promise<void>) {
  c.set("audited", "true");
  await next();
}

// Global middleware on the mounted child itself.
export async function childLogger(c: Context, next: () => Promise<void>) {
  c.set("childLog", "1");
  await next();
}
