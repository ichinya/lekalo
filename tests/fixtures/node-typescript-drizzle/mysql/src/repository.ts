import { eq } from "drizzle-orm";
import { drizzle } from "drizzle-orm/mysql2";
import { invoices, users } from "./schema.js";

export const db = drizzle.mock();

export async function findUser(id: number) {
  return db.select().from(users).where(eq(users.id, id));
}

export async function createInvoice(input: { ownerId: number; tenantId: number }) {
  return db.insert(invoices).values(input);
}

export async function upsertInvoiceMemo(id: number, memo: string) {
  return db.insert(invoices)
    .values({ id, ownerId: 1, tenantId: 7, memo })
    .onDuplicateKeyUpdate({ set: { memo } })
    .$returningId();
}

export async function rawCount() {
  return db.execute(`select count(*) from users`);
}

export async function dynamicInvoiceQuery(id: number) {
  return db.select().from(invoices).where(eq(invoices.id, id)).$dynamic();
}

export const pendingBuilder = db.select().from(users);

export async function listTenantInvoices(tenantId: number) {
  return db.select({ id: invoices.id, memo: invoices.memo })
    .from(invoices)
    .where(eq(invoices.tenantId, tenantId));
}
