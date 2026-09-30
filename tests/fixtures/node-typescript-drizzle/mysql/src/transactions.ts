import { eq } from "drizzle-orm";
import { invoices, users } from "./schema.js";
import { db } from "./repository.js";

export async function transferOwnership(invoiceId: number, nextOwner: number) {
  await db.transaction(async (tx) => {
    await tx.update(users).set({ email: "a@example.com" }).where(eq(users.id, nextOwner));
    await tx.update(invoices).set({ ownerId: nextOwner }).where(eq(invoices.id, invoiceId));
  });
}

export async function nestedMove(invoiceId: number) {
  await db.transaction(async (tx) => {
    await tx.update(invoices).set({ memo: "moving" }).where(eq(invoices.id, invoiceId));
    await tx.transaction(async (inner) => {
      await inner.update(invoices).set({ memo: "moved" }).where(eq(invoices.id, invoiceId));
    });
  });
}

export async function guardedDelete(invoiceId: number) {
  await db.transaction(async (tx) => {
    try {
      await tx.delete(invoices).where(eq(invoices.id, invoiceId));
    } catch {
      await tx.rollback();
    }
  });
}
