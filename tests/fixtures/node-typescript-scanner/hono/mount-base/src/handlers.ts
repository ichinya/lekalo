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

export const enabled = true;
