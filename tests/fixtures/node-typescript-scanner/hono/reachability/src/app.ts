import { Hono } from "hono";
import {
  conditionalHandler,
  deferredMiddleware,
  featureFlag,
  provenHandler,
  unreachableHandler,
} from "./handlers";

export const app = new Hono();

// Top-level straight-line: proven to run at initialization (control).
app.get("/top", provenHandler);

// A route the deferred `use` below would have covered if it ran.
app.get("/plain", provenHandler);

// Conditional registration: may never run — incomplete, never complete.
const enabled = featureFlag();
if (enabled) {
  app.get("/conditional", conditionalHandler);
}

// Deferred registration: wrapped in a function nobody calls straight-
// line at top level — incomplete with deferred-registration.
function setup() {
  app.get("/deferred", deferredMiddleware);
}
void setup;

// Deferred `use`: the middleware claim inherits the same reachability.
function wire() {
  app.use(deferredMiddleware);
}
void wire;

// Proven-called registration: the module invokes setupCalled()
// straight-line at top level, so the body provably runs — complete.
function setupCalled() {
  app.get("/called", provenHandler);
}
setupCalled();

// Unreachable: preceded at the same block level by a return — unknown.
export function bootstrap(): Hono {
  return app;
  app.get("/unreachable", unreachableHandler);
}
