import { Hono } from "hono";
import { auth, runtimePrefix } from "./middleware";

// Non-literal filter position: a runtime-computed string the scanner
// cannot resolve. The filter stays unknown (dynamic-path-filter) and
// NO handler endpoint may be fabricated from the argument.
export const runtimeApp = new Hono();
runtimeApp.use(runtimePrefix(), auth);
runtimeApp.get("/runtime/panel", (c) => c.json({ runtime: true }));
