import { Hono } from "hono";
import { audit, gate } from "./middleware";
import { child } from "./child";
import { open } from "./open";

export const app = new Hono();

// Path-filtered parent middleware over the mount: must compose onto the
// mounted child routes — conditional for the wildcard, applicable for
// the literal — and must yield records, never silence.
app.use("/api/*", gate);
app.use("/api", audit);

app.route("/api", child);
app.route("/open", open);
