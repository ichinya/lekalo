import type { Context } from "hono";

export function featureFlag(): boolean {
  return true;
}

export function provenHandler(c: Context) {
  return c.json({ ok: true });
}

export function conditionalHandler(c: Context): unknown {
  return c.json({ conditional: true });
}

export function deferredMiddleware(c: Context, next: () => Promise<void>): unknown {
  return next();
}

export function unreachableHandler(c: Context): unknown {
  return c.text("never");
}
