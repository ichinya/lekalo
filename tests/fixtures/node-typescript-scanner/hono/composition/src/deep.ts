import { Hono } from "hono";
import { deepLeaf } from "./handlers";

// A third-level child: app -> api -> deep. The full resolved path must
// carry every ancestor prefix (/api/deep/leaf), never /deep/leaf.
export const deep = new Hono();
deep.get("/leaf", deepLeaf);
