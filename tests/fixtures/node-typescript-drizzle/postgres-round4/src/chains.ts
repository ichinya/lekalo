import { relations } from "drizzle-orm";
import { posts, users } from "./schema.js";

// ---------------------------------------------------------------------------
// Fix round 4 MINOR: the alias-resolve hop bound must be explicit.
// A rebind chain of four hops still resolves (the closure test runs
// before the bound check); the fifth rebind dies at MAX_ALIAS_HOPS and
// must emit `callee-unproven` — the boundedness is fine, the silence
// was not.
// ---------------------------------------------------------------------------

const h1 = relations;
const h2 = h1;
const h3 = h2;
const h4 = h3;
const h5 = h4;

export const withinBound = h4(users, ({ many }) => ({
  hopWithin: many(posts),
}));

export const beyondBound = h5(posts, ({ many }) => ({
  hopBeyond: many(users),
}));
