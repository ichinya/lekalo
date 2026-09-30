import { relations as rel } from "drizzle-orm";
import { posts, users } from "./schema.js";

// Aliased helper: `relations as rel` must extract exactly like the
// spelled name — the walker resolves the callee symbol before gating.
export const usersRelations = rel(users, ({ many }) => ({
  posts: many(posts),
}));

export const postsRelations = rel(posts, ({ one }) => ({
  author: one(users, { fields: [posts.authorId], references: [users.id] }),
}));
