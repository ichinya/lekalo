import { Hono } from "hono";
import { api } from "./api";
import { lateChildRoute, rootHandler } from "./handlers";
import { shared } from "./shared";
import { v1 } from "./v1";

export const app = new Hono();
app.get("/", rootHandler);
app.route("/api", api);

// basePath view of the same app: a second registration surface.
const v1direct = app.basePath("/v1direct");
v1direct.get("/users", rootHandler);

// A late registration on the mounted child: snapshot semantics make
// this incomplete evidence, never a guessed route.
v1.get("/late", lateChildRoute);

// The shared router is also mounted at a second parent prefix.
app.route("/shared", shared);
