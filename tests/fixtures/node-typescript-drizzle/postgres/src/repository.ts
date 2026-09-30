import { and, asc, eq, sql } from "drizzle-orm";
import { drizzle } from "drizzle-orm/node-postgres";
import { posts, users } from "./schema.js";

export const db = drizzle.mock();

export async function findUser(id: string) {
  return db.select().from(users).where(eq(users.id, id));
}

export async function listUserEmails(tenantId: string) {
  return db.select({ email: users.email, active: users.isActive })
    .from(users)
    .where(eq(users.tenantId, tenantId))
    .orderBy(asc(users.email));
}

export async function createUser(input: { email: string; tenantId: string }) {
  return db.insert(users).values(input).returning({ id: users.id });
}

export async function createStaticUser() {
  return db.insert(users).values({
    email: "someone@example.com",
    tenantId: "11111111-1111-1111-1111-111111111111",
    isActive: true,
  }).returning();
}

export async function deactivateUser(id: string) {
  await db.update(users).set({ isActive: false }).where(eq(users.id, id));
}

export async function removeUser(id: string) {
  await db.delete(users).where(eq(users.id, id));
}

export async function removeUserUnscoped(id: string) {
  await db.delete(users).where(and(eq(users.isActive, true)));
  void id;
}

export async function postsWithAuthors(tenantId: string) {
  return db.select({ slug: posts.slug, author: users.email })
    .from(posts)
    .innerJoin(users, eq(users.id, posts.authorId))
    .where(eq(posts.tenantId, tenantId));
}

export async function upsertPost(id: number, slug: string) {
  return db.insert(posts)
    .values({ id, slug, authorId: 1, tenantId: "22222222-2222-2222-2222-222222222222", status: "draft" })
    .onConflictDoUpdate({ target: posts.id, set: { slug } })
    .returning({ id: posts.id });
}

export async function rawTenantCount() {
  return db.execute(sql`select count(*) from users`);
}

export async function dynamicUserQuery(id: string) {
  const base = db.select().from(users).where(eq(users.id, id)).$dynamic();
  return base;
}

export const pendingBuilder = db.select().from(users);
