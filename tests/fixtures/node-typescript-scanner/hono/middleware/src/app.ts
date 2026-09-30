import { Hono } from "hono";
import { auth, cacheHeaders, logger, maintenance, tenant } from "./middleware";
import { down, health, whoami } from "./handlers";

export const app = new Hono();

// Global middleware: applies to every route in registration order.
app.use(logger);
app.use(tenant);

// Path-filtered middleware: applies to /admin/* shapes only.
app.use("/admin/*", auth);

// Const-alias filter: resolves exactly like a literal — it must stay a
// PATH FILTER, never be resolved into a middleware endpoint.
const consolePrefix = "/console";
app.use(consolePrefix, auth);
app.get("/console/panel", (c) => c.json({ console: true }));

app.get("/whoami", whoami);
app.get("/health", cacheHeaders, health);
app.get("/admin/panel", (c) => c.json({ panel: true }));
app.post("/admin/reset", maintenance, (c) => c.json({ reset: true }));
