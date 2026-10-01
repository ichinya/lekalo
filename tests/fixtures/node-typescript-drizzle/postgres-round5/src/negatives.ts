import { relations } from "drizzle-orm";
import { posts, users } from "./schema.js";

// ---------------------------------------------------------------------------
// Fix round 5 honest negatives: the type anchor resolves DECLARATION
// forms only. Value-carrying unprovables keep the round-3/round-4
// explicit flags, and the round-4 hop-bound contract stands.
// ---------------------------------------------------------------------------

// A cast over dynamic data is an assertion, not a declaration anchor:
// the unproven flag stays.
const { relations: rel2 } = JSON.parse("{}") as Record<string, typeof relations>;
export const unprovenDestructure = rel2(users, ({ many }) => ({
  unproven: many(posts),
}));

// The round-4 bound: five rebinds stay explicit (`callee-unproven`),
// four hops still extract.
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

// A project-local helper that merely spells a construct never flags
// (round-4 foreign-receiver silence).
class Local {
  relations() {
    return posts;
  }
}

export function foreignSpells() {
  return new Local().relations();
}
