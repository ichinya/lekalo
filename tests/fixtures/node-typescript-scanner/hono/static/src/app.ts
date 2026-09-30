import { Hono } from "hono";
import { createUser, getUser, listUsers } from "./handlers";

export const app = new Hono();

app.get("/users", listUsers);
app.post("/users", createUser);
app.get("/users/:id", getUser);
app.get("/health", (c) => c.json({ ok: true }));

const version = "v1";
app.get(`/${version}/status`, (c) => c.text("ok"));

// Multi-method route: the on() vocabulary with a literal method
// array. The endpoint-contract join must label the record with the
// JOINED contract method, not methods[0].
app.on(["GET", "POST"], "/multi", (c) => c.json({ multi: true }));
