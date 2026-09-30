import * as orm from "drizzle-orm";
import { users } from "./schema.js";

// ---------------------------------------------------------------------------
// Fix round 5 MAJOR 1: module-TYPED values are module namespaces, no
// matter how the value itself is declared. The round-4 declaration-kind
// test sees ordinary variables/parameters, and the export-surface type
// resolves INTO the embedded closure — so `ns.select()` fabricated
// clean query rows and batch/relational/transaction members were
// misattributed as client gaps. Every receiver below must emit
// `namespace-receiver-unsupported` — never a fabricated row.
// ---------------------------------------------------------------------------

// Form A: a declared binding whose type is the module type query.
declare const ns: typeof import("drizzle-orm");
export function typedNsDecl() {
  const q = ns.select().from(users);
  ns.batch([q]);
  return ns.query.users.findMany();
}

// Form B: a parameter typed through a type alias of the module type.
type ORM = typeof import("drizzle-orm");
export function aliasTypedParam(ns2: ORM) {
  return ns2.select().from(users);
}

// Form C: a parameter typed by a type query over a namespace import.
export function importTypeParam(ns3: typeof orm) {
  return ns3.transaction(async (tx) => tx.select().from(users));
}

// Consistency minor: an await-import binding is the same namespace
// value — the honest limitation, not a receiver-unknown shrug.
const dynamic = await import("drizzle-orm");
export function awaitImportReceiver() {
  return dynamic.select().from(users);
}
