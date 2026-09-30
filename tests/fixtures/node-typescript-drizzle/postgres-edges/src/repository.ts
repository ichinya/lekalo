import { eq } from "drizzle-orm";
import { drizzle } from "drizzle-orm/node-postgres";
import { users as aliasUsers } from "@app/schema.js";
import { posts, users } from "./schema.js";

export const db = drizzle.mock();

// Relational query API: a real Drizzle surface outside the qualified
// static subset — must emit an explicit limitation, never drop.
export async function relationalFindAll() {
  return db.query.users.findMany();
}

export async function relationalFindOne(id: string) {
  return db.query.users.findFirst({ where: eq(users.id, id) });
}

// Batch API: same honesty requirement.
export async function batchWrites() {
  return db.batch([
    db.insert(users).values({ email: "a@b.c", tenantId: "33333333-3333-3333-3333-333333333333" }),
    db.update(posts).set({ title: "batched" }),
  ]);
}

// The consumer tsconfig paths mapping (@app/*) must keep resolving
// after the drizzle pin attaches: merged, never replaced.
export async function aliasedFind(id: string) {
  return db.select().from(aliasUsers).where(eq(aliasUsers.id, id));
}

// Join RHS equality columns are field reads of the joined table, not
// external reference inputs.
export async function joinFieldReads(tenantId: string) {
  return db.select({ title: posts.title })
    .from(posts)
    .innerJoin(aliasUsers, eq(aliasUsers.id, posts.authorId))
    .where(eq(posts.tenantId, tenantId));
}

// A builder handed off by an explicit return is intentional — no
// builder-not-executed noise.
export function builderForLater() {
  return db.select().from(aliasUsers);
}
