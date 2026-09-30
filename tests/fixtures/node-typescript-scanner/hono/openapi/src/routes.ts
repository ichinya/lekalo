import { createRoute, OpenAPIHono } from "@hono/zod-openapi";
import { UserSchema } from "./schemas";
import type { Context } from "hono";

export function getUserHandler(c: Context) {
  return c.json({ id: 1 });
}

const getUserRoute = createRoute({
  method: "get",
  path: "/users/{id}",
  operationId: "users.show",
  request: {},
  responses: {},
});

// Dynamic method: recorded as an incomplete definition, never guessed.
const dynamicRoute = createRoute({
  method: envMethod(),
  path: "/dynamic",
  responses: {},
});

function envMethod(): string {
  return "get";
}

export const app = new OpenAPIHono();

app.openapi(getUserRoute, getUserHandler);
app.openapi(dynamicRoute, (c) => c.json({ dynamic: true }));
void UserSchema;
