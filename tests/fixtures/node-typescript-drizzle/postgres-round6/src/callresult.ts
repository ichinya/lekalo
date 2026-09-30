import { integer } from "drizzle-orm/pg-core";

// ---------------------------------------------------------------------------
// Fix round 6 residual: a call-result initializer keeps its type anchor
// but the extraction is marked `type-sourced` on the row — the identity
// comes from the declared return type, never claimed as direct
// construct proof.
// ---------------------------------------------------------------------------

declare function makeTable(): typeof import("drizzle-orm/pg-core").pgTable;
const factory = makeTable();
export const made = factory("made", {
  id: integer("id").primaryKey(),
});
