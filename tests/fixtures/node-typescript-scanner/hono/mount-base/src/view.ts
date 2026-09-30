import { Hono } from "hono";
import { inHandler, ownHandler } from "./handlers";
import { n1 } from "./deep";

// The base-path view under test: its OWN /v prefix joins every mount
// that carries it at runtime, exactly like the child's own routes do.
// Hono #clone() shares the routes ARRAY across the whole family, so
// the owner's own registration below lives in the same table the view
// exposes — mounting any member serves the whole family.
export const inner2 = new Hono();
inner2.get("/own", ownHandler);
export const view = inner2.basePath("/v");
view.get("/in", inHandler);

// Depth-2: the based view itself mounts a grandchild that again
// carries its own /nb prefix — mountPrefix + childBase + nested mount
// path + grandchildBase + route path.
view.route("/n", n1);
