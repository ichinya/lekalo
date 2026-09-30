import { posts } from "./schema.js";

// ---------------------------------------------------------------------------
// Fix round 4 MINOR: the recognition-by-spelling net must stay silent
// on provably non-Drizzle receivers. Both members below are real
// project declarations whose types resolve OUTSIDE the embedded
// closure — they merely spell `relations`/`pgTable`. Flagging them as
// `callee-unproven` would fabricate a Drizzle gap.
// ---------------------------------------------------------------------------

class LocalQueryBuilder {
  relations() {
    return posts;
  }

  pgTable() {
    return "not-a-table";
  }
}

export function foreignSpellings() {
  const builder = new LocalQueryBuilder();
  builder.relations();
  builder.pgTable();
  return builder;
}
