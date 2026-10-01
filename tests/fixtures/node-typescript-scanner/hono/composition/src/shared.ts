import { Hono } from "hono";
import { sharedA } from "./handlers";

export const shared = new Hono();
shared.get("/a", sharedA);
