import { eq } from "drizzle-orm";
import type { MySql2Database } from "drizzle-orm/mysql2";
import { tasks } from "../src/schema.ts";
import type { TaskRepository } from "../src/repository.ts";

// The fixture's actual persistence entrypoint. The offline pilot scopes src/types,
// not runtime configuration: it does not claim complete runtime coverage.
export function createRepository(database: MySql2Database): TaskRepository {
  return {
    async create(row) {
      await database.insert(tasks).values(row);
      const [stored] = await database.select().from(tasks).where(eq(tasks.id, row.id));
      if (!stored) throw new Error("persist-failed");
      return stored;
    },
  };
}
