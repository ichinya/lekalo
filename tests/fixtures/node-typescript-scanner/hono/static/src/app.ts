import { Hono } from "hono";
import { createUser, getUser, listUsers } from "./handlers";

export const app = new Hono();

app.get("/users", listUsers);
app.post("/users", createUser);
app.get("/users/:id", getUser);
app.get("/health", (c) => c.json({ ok: true }));

const version = "v1";
app.get(`/${version}/status`, (c) => c.text("ok"));
