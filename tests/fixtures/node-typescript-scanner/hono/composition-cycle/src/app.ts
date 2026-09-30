import { Hono } from "hono";
import { b } from "./b";

export const a = new Hono();
a.get("/a", (c) => c.text("a"));
// A mount cycle: static analysis must refuse, not invent.
a.route("/b", b);
