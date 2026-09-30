import type { Context } from "hono";

// A top-level const the conditional mount site tests: the condition
// itself is runtime data — the structural `if` is what matters.
export const featureFlag = true;

export async function childMiddleware(c: Context, next: () => Promise<void>) {
  c.set("traceId", "t-1");
  await next();
}

export function childHandler(c: Context) {
  const trace = c.get("traceId");
  auditTrace(String(trace));
  return c.json({ trace });
}

export function auditTrace(trace: string) {
  return trace.length > 0;
}

export function pingHandler(c: Context) {
  return c.json({ ping: true });
}
