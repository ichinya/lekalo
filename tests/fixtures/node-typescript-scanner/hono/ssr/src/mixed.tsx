import { Hono } from "hono";

export const mixed = new Hono();

// Mixed: one branch API, one branch SSR.
mixed.get("/mixed", (c) => {
  if (c.get("wants-json")) {
    return c.json({ ok: true });
  }
  return c.html(<h1>page</h1>);
});

// A .tsx file whose handler returns JSON only: API, not SSR.
mixed.get("/tsx-but-api", (c) => c.json({ api: true }));
