import { Hono } from "hono";
import { v1Posts, v1Users } from "./handlers";

// A shared child router: mounted twice from api.ts.
export const v1 = new Hono();
v1.get("/users", v1Users);
v1.get("/posts", v1Posts);
