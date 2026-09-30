import type { Context, Middleware } from "hono";

/**
 * Request logger.
 * @lekalo-logging
 */
export async function logger(c: Context, next: () => Promise<void>) {
  c.set("requestId", "req-1");
  await next();
}

/**
 * Tenant guard.
 * @lekalo-tenant
 */
export async function tenant(c: Context, next: () => Promise<void>) {
  const tenantId = c.get("tenant");
  c.set("tenantId", tenantId);
  await next();
}

/**
 * Session auth guard.
 * @lekalo-auth
 */
export async function auth(c: Context, next: () => Promise<void>) {
  const session = c.var.session;
  if (!session) {
    return c.json({ error: "unauthorized" }, 401);
  }
  await next();
}

// No annotation: presence only, never a role claim.
export async function cacheHeaders(c: Context, next: () => Promise<void>) {
  await next();
  c.set("cache", "miss");
}

// Short-circuit candidate: no next() call anywhere in the body.
export function maintenance(c: Context) {
  return c.json({ error: "down" }, 503);
}
