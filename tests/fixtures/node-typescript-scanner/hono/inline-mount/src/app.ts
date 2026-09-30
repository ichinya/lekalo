import { Hono } from "hono";
import { deepHandler, enabled, insideHandler } from "./handlers";

export const app = new Hono();

// Inline constructed mount target: the whole argument evaluates before
// app.route executes. The routes of the inline app mount at /sub —
// linked to the mount, never orphan standalone routes.
app.route("/sub", new Hono().get("/inside", insideHandler));

// A multi-link inline target plus a nested inline mount: composition
// keeps every statically-known registration under its prefix.
app.route("/deep", new Hono().get("/a", insideHandler).post("/b", deepHandler).route("/n", new Hono().get("/c", deepHandler)));

// A conditional inline target: the mount site's reachability still
// constrains everything beneath it — included-by-construction is never
// a complete fact when the mount itself may not run.
if (enabled) {
  app.route("/maybe", new Hono().get("/x", insideHandler));
}
