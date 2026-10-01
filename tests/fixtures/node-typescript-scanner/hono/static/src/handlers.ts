import { findUser, queryUsers, saveUser } from "./service";
import type { Context } from "hono";

export function listUsers(c: Context) {
  return c.json(queryUsers());
}

export function getUser(c: Context) {
  const id = Number(c.get("id"));
  const user = findUser(id);
  if (user === null) {
    return c.json({ error: "missing" }, 404);
  }
  return c.json(user);
}

export function createUser(c: Context) {
  return c.json(saveUser({ id: 2, name: "grace" }), 201);
}
