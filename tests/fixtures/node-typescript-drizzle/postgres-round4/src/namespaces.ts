import * as orm from "drizzle-orm";
import { users } from "./schema.js";

// ---------------------------------------------------------------------------
// Fix round 4 MAJOR 1: a module namespace object is never a database
// handle, no matter how the binding is spelled. Before the fix, the
// namespace type resolved INTO the embedded closure and the receiver
// was typed `db` — fabricating a query row from `orm.select()` and
// misattributing `orm.batch`/`orm.query.*` as client-surface gaps.
// Every member call below must emit `namespace-receiver-unsupported`
// and degrade the covered section — never a fabricated row, never a
// batch/relational/transaction misattribution.
// ---------------------------------------------------------------------------

export function namespaceQueries() {
  const q = orm.select().from(users);
  orm.batch([q]);
  return orm.query.users.findMany();
}

export function namespaceTransaction() {
  return orm.transaction(async (tx) => {
    return tx.select().from(users);
  });
}
