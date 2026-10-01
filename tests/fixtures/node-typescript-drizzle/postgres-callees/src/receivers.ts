import { drizzle } from "drizzle-orm/node-postgres";
import { users } from "./schema.js";

export const db = drizzle.mock();

// The genuine Drizzle relational query API: the explicit
// `relational-query-unsupported` limitation fires HERE and only here.
export async function genuineRelational() {
  return db.query.users.findMany();
}

export function shadowedRelational() {
  // A same-named NON-Drizzle local: receiver identity must come from
  // the resolved symbol, so this call is not Drizzle and must stay
  // limitation-free — the bare name `db` never inherits the identity
  // proven for the module-level handle (fix round 3 minor).
  const db = { query: { users: { findMany: async () => [] } } };
  return db.query.users.findMany();
}
