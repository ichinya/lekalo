import * as orm from "drizzle-orm";
import { relations as rel3 } from "drizzle-orm";
import { posts, users } from "./schema.js";

// ---------------------------------------------------------------------------
// Fix round 4 MAJOR 2: renamed bindings of the `relations` construct.
// The local spelling is never identity — the destructured property name
// (`{ relations: rel2 }`) and the aliased import specifier
// (`relations as rel3`) both resolve to the vendored export and must
// EXTRACT exactly like the spelled import; when unprovable, the
// construct spelling keeps them honest via `callee-unproven` instead
// of a silent zero-row drop.
// ---------------------------------------------------------------------------

const { relations: rel2 } = orm;
export const destructured = rel2(users, ({ many }) => ({
  labels: many(posts),
}));

export const aliasedDirect = rel3(posts, ({ one }) => ({
  author: one(users, { fields: [posts.authorId], references: [users.id] }),
}));

// Shorthand destructure (no rename): same resolution path.
const { relations } = orm;
export const shorthand = relations(posts, ({ many }) => ({
  replies: many(users),
}));
