import { Hono } from "hono";
import { aliasHandler, enabled, inHandler, inlineHandler } from "./handlers";
import { view } from "./view";

export const app = new Hono();

// BLOCKER regression: the mounted child carries its OWN basePath. At
// runtime the route serves /bp/v/in — the child's prefix is part of
// the mounted surface, never dropped.
app.route("/bp", view);

// Same blocker, inline form: the based child is built in the mount
// argument itself and fully evaluated before the mount executes.
app.route("/bpi", new Hono().basePath("/iv").get("/in", inlineHandler));

// Depth-2 mount status propagation: a conditional mount of the based
// child constrains the whole prefixed subtree.
if (enabled) {
  app.route("/bpc", view);
}

// A deferred mount of the based child stays incomplete evidence too.
function deferredMount() {
  app.route("/bpd", view);
}
void deferredMount;
