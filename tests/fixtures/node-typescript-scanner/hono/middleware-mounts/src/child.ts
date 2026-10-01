import { Hono } from "hono";
import { childLogger } from "./middleware";
import { otherHandler, pingHandler } from "./handlers";

// The mounted child carries its own global middleware.
export const child = new Hono();
child.use(childLogger);
child.get("/ping", pingHandler);
child.get("/other", otherHandler);
