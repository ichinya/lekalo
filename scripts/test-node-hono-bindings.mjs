/**
 * Issue #115 gate: the Hono framework-binding suites, executed with
 * the current Node interpreter through the committed production
 * bundle. Package scripts are poisoned by design; this gate spawns
 * `node --test` directly on real filenames.
 */
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const testDir = join(root, "adapters", "node-typescript", "test");
const files = [
  "hono-evidence.test.mjs",
  "hono-scanner.test.mjs",
  "hono-middleware.test.mjs",
  "hono-http.test.mjs",
  "hono-process.test.mjs",
];

let failed = false;
for (const file of files) {
  const result = spawnSync(process.execPath, ["--test", join(testDir, file)], {
    stdio: "inherit",
    timeout: 600000,
  });
  if (result.status !== 0) {
    failed = true;
    process.stderr.write(`node-hono-bindings: ${file} failed (${result.status})\n`);
  }
}
if (failed) {
  process.exitCode = 1;
} else {
  process.stdout.write(
    JSON.stringify({ ok: true, gate: "node-hono-bindings", files: files.length }) + "\n",
  );
}
