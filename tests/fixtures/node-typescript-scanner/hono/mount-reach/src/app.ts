import { Hono } from "hono";
import { child } from "./child";
import { featureFlag, pingHandler } from "./handlers";

export const app = new Hono();

// Control: the same child shape mounted unconditionally at top level —
// every record beneath it may claim complete.
const plain = new Hono();
plain.get("/direct", pingHandler);
app.route("/ok", plain);

// Conditional mount: the mount site only runs under a runtime flag.
// Ordering proves the child's registrations are snapshot-included, but
// the mount occurrence itself is conditional — nothing beneath /cm may
// claim a complete fact.
if (featureFlag) {
  app.route("/cm", child);
}

// Deferred mount: the mount site is wrapped in a function nobody calls
// straight-line at top level. Same rule: nothing beneath /late may
// claim complete.
function wire() {
  app.route("/late", child);
}
void wire;
