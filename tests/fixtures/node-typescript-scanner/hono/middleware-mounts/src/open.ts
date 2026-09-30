import { Hono } from "hono";
import { openHandler } from "./handlers";

// A sibling subtree the parent's /api filters provably do not cover.
export const open = new Hono();
open.get("/leaf", openHandler);
