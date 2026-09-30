import { Hono } from "hono";
import { createApp } from "./factory";

export const app = new Hono();

// Dynamic path: built by a call — unknown, never guessed.
const env = ("production");
app.get("/dynamic/" + env.toUpperCase(), (c) => c.text("dynamic"));

// `on` with a dynamic method array — unknown methods.
const methods = ["GET"];
app.on(methods, "/on-dynamic", (c) => c.text("on"));

// Mutable alias: reassigned below; aliases through it are refused.
let alias = app;
alias.get("/mutable", (c) => c.text("mutable"));
alias = new Hono();

// Factory receiver with Hono type: unsupported receiver evidence.
const produced = createApp();
produced.get("/produced", (c) => c.text("produced"));

// Template literal with a const alias resolves; dynamic hole does not.
const base = "/templated";
app.get(`${base}/ok`, (c) => c.text("ok"));
