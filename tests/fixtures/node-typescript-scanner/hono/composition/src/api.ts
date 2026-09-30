import { Hono } from "hono";
import { v1 } from "./v1";
import { apiHandler } from "./handlers";

export const api = new Hono();
api.get("/ping", apiHandler);
api.route("/v1", v1);
api.route("/v2", v1);
