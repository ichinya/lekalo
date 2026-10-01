import { Router } from "./fake-router";
import { Hono } from "./local-hono";
import { get as fakeGet } from "./fake-get";

const router = new Router();
router.get("/items", () => []);

const localApp = new Hono();
localApp.get("/local", () => []);

fakeGet("/fake", () => []);
