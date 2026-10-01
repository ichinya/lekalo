import { Hono } from "hono";

export const api = new Hono();
api.get("/api/ping", (c) => c.json({ pong: true }));
