import { Hono } from "hono";
import { getHandler, postHandler, putHandler } from "./handlers";

export const app = new Hono();

// Fluent verb chain: three statically-known registrations on one
// expression. Every link resolves to its own route record — known
// path/method/handler are never dropped into a bare receiver
// uncertainty.
app.get("/fluent", getHandler).post("/fluent", postHandler).put("/fluent", putHandler);

// Control: plain registrations keep behaving exactly as before.
app.get("/plain", getHandler);
