import type { Context } from "hono";

export function rootHandler(c: Context) {
  return c.json({ root: true });
}

export function apiHandler(c: Context) {
  return c.json({ api: true });
}

export function v1Users(c: Context) {
  return c.json({ users: [] });
}

export function v1Posts(c: Context) {
  return c.json({ posts: [] });
}

export function sharedA(c: Context) {
  return c.text("a");
}

export function lateChildRoute(c: Context) {
  return c.text("late");
}
