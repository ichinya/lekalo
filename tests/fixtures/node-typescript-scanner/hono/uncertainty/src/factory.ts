import { Hono } from "hono";

// An untracked factory returning a Hono instance: the receiver type is
// Hono, so routes through it are recorded as unsupported receivers —
// never guessed.
export function createApp(): Hono {
  return new Hono();
}
