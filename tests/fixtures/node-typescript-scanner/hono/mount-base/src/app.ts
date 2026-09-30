import { Hono } from "hono";
import { aliasHandler, enabled, inHandler, inlineHandler, lateHandler } from "./handlers";
import { inner2, view } from "./view";

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

// MAJOR regression: the mount target is a const ALIAS of the view.
// The alias and its target are the same runtime object, so the
// target's registrations are the mounted subtree.
const aliased = view;
app.route("/bp2", aliased);
aliased.get("/alias-in", aliasHandler);

// Shared route-table family: mounting the OWNER exposes the view's
// routes too (one shared routes array), each member's registrations
// under its own base.
app.route("/x", inner2);

// A late owner-side registration after both mounts: snapshot-honest
// bounded evidence, never a guessed served route.
inner2.get("/late", lateHandler);
