import { validator } from "hono/validator";
import { zValidator } from "@hono/zod-validator";
import { CreateUserSchema, ListQuerySchema } from "./schemas";

export const validateBody = zValidator("json", CreateUserSchema);

export const validateQuery = zValidator("query", ListQuerySchema);

export const validateHeader = validator("header", (value) => value);
