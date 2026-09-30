import { Hono } from "hono";
import { inHandler } from "./handlers";
import { n1 } from "./deep";

// The base-path view under test: its OWN /v prefix joins every mount
// that carries it at runtime, exactly like the child's own routes do.
export const inner2 = new Hono();
export const view = inner2.basePath("/v");
view.get("/in", inHandler);

// Depth-2: the based view itself mounts a grandchild that again
// carries its own /nb prefix — mountPrefix + childBase + nested mount
// path + grandchildBase + route path.
view.route("/n", n1);
