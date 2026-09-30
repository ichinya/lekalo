import { eq } from "drizzle-orm";
import { posts, users } from "./schema.js";
import { db } from "./repository.js";

export async function transferOwnership(postId: number, nextAuthor: string) {
  await db.transaction(async (tx) => {
    await tx.update(users).set({ isActive: true }).where(eq(users.id, nextAuthor));
    await tx.update(posts).set({ authorId: 1 }).where(eq(posts.id, postId));
  });
}

export async function nestedMove(postId: number) {
  await db.transaction(async (tx) => {
    await tx.update(posts).set({ slug: "moving" }).where(eq(posts.id, postId));
    await tx.transaction(async (inner) => {
      await inner.update(posts).set({ slug: "moved" }).where(eq(posts.id, postId));
    });
  });
}

export async function guardedDelete(postId: number) {
  await db.transaction(async (tx) => {
    try {
      await tx.delete(posts).where(eq(posts.id, postId));
    } catch {
      await tx.rollback();
    }
  });
}

export async function conditionalTouch(postId: number, flag: boolean) {
  await db.transaction(async (tx) => {
    if (flag) {
      await tx.update(posts).set({ body: "flagged" }).where(eq(posts.id, postId));
    }
  });
}

export async function outsideUse(postId: number) {
  await db.transaction(async (tx) => {
    await db.update(posts).set({ body: "outside" }).where(eq(posts.id, postId));
    void tx;
  });
}
