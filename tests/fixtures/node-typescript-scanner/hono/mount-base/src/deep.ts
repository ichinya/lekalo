import { Hono } from "hono";
import { leafHandler } from "./handlers";

// A grandchild router with its OWN basePath, mounted inside the
// already-based view: both prefixes must survive composition.
export const plain2 = new Hono();
export const n1 = plain2.basePath("/nb");
n1.get("/leaf", leafHandler);
