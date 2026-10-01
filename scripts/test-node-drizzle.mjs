/**
 * #116 Drizzle evidence suite: the committed Postgres and MySQL
 * fixtures, executed through the production bundle's embedded vendored
 * compiler and the pinned upstream drizzle-orm declaration closure.
 * Node built-ins only; no package manager, no shell, no project
 * scripts, no database. Every spawn is the current Node interpreter.
 */
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const testDir = join(root, "adapters", "node-typescript", "test");
const files = [
  "drizzle-evidence.test.mjs",
];

let failed = false;
for (const file of files) {
  const result = spawnSync(process.execPath, ["--test", join(testDir, file)], {
    stdio: "inherit",
  });
  if (result.status !== 0) {
    failed = true;
    process.stderr.write(`node-drizzle: ${file} failed (${result.status})\n`);
  }
}
if (failed) {
  process.exitCode = 1;
} else {
  process.stdout.write(
    JSON.stringify({ ok: true, gate: "node-drizzle", files: files.length }) + "\n",
  );
}
