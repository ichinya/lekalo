import { Hono } from "hono";
import { validateBody, validateHeader, validateQuery } from "./validators";
import { boom, created, listUsers, missing, page } from "./handlers";

export const app = new Hono();

app.onError((err, c) => c.json({ error: String(err) }, 500));
app.notFound((c) => c.json({ error: "not-found" }, 404));

app.get("/users", validateQuery, listUsers);
app.post("/users", validateBody, validateHeader, created);
app.get("/missing", missing);
app.get("/boom", boom);
app.get("/page", page);
