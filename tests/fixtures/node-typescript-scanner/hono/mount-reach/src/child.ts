import { Hono } from "hono";
import { childHandler, childMiddleware } from "./handlers";

// The mounted child: one global middleware (top-level use, provably
// ordered before the child's route) plus one route whose handler reads
// context, calls a service, and returns a response — every derived
// relation of the chain, in one subtree.
export const child = new Hono();
child.use(childMiddleware);
child.get("/kid", childHandler);
