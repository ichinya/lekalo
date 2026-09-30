import { relations } from "drizzle-orm";
import { posts, users } from "./schema.js";

// ---------------------------------------------------------------------------
// Fix round 6 MAJOR 1 honest negatives: the type anchor fires only when
// no initializer value can contradict it. A cast over data is an
// assertion, and a let/var binding can diverge from its initializer
// before the call — both were fabricated in the round-5 widening and
// must now stay explicitly `callee-unproven` (family-disjoint), never
// rows, never silent.
// ---------------------------------------------------------------------------

const r = (null as unknown) as typeof relations;
export const castRow = r(users, ({ many }) => ({
  labels: many(posts),
}));

let r2 = relations;
r2 = function replaced() {
  return null;
};
export const staleRow = r2(posts, ({ many }) => ({
  stale: many(users),
}));
