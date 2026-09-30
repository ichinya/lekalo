import { Hono } from "hono";
import { hubOwn } from "./handlers";
import { v1 } from "./v1";

// A shared PARENT that itself has a nested mount, mounted twice from
// app.ts: every ancestor chain must resolve its subtree independently
// (per-chain scope keys), and the nested mounts-router fact (/h) is
// recorded once, not once per ancestor chain.
export const hub = new Hono();
hub.get("/own", hubOwn);
hub.route("/h", v1);
