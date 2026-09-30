import type { Context } from "hono";

export function inHandler(c: Context) {
  return c.json({ in: true });
}

export function inlineHandler(c: Context) {
  return c.json({ inline: true });
}

export function leafHandler(c: Context) {
  return c.json({ leaf: true });
}

export function aliasHandler(c: Context) {
  return c.json({ alias: true });
}

export function ownHandler(c: Context) {
  return c.json({ own: true });
}

export function lateHandler(c: Context) {
  return c.json({ late: true });
}

export const enabled = true;
