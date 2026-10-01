import { Hono } from "hono";
import { a } from "./a";

export const b = new Hono();
b.get("/b", (c) => c.text("b"));
// The back-reference: a mounts b, b mounts a — a composition cycle.
b.route("/a", a);
