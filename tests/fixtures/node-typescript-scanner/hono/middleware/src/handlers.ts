import type { Context } from "hono";

export function whoami(c: Context) {
  return c.json({ tenant: c.var.tenantId, session: c.get("session") });
}

export function health(c: Context) {
  return c.text("ok");
}

export function down(c: Context) {
  return c.json({ ok: false });
}
