import { z } from "zod";

export const CreateUserSchema = z.object({
  name: z.string(),
});

export const ListQuerySchema = z.object({
  limit: z.number(),
});
