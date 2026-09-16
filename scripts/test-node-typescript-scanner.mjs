/**
 * #44 scanner suite: the committed fixtures, executed through the
 * production bundle's exported scanner session against materialized
 * fixture projects. Node built-ins only; no package manager, no shell,
 * no project scripts. Every spawn is the current Node interpreter.
 *
 * Suites (plan §8/§10):
 *   unit-scanner  — identity/signature/bounds pure probes
 *   fixtures      — esm/cjs/references/aliases/pnpm end-to-end scans
 *   uncertainty   — any/unresolved/dynamic surfaces recorded, never pass
 *   exclusions    — poison stays out of the index and the read counters
 *   incremental   — body/signature edits and cold==warm parity
 *   process       — launch-input, protocol, and bundle probes
 */
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const testDir = join(root, "adapters", "node-typescript", "test");
const files = [
  "scanner-unit.test.mjs",
  "scanner-fixtures.test.mjs",
  "scanner-process.test.mjs",
];

let failed = false;
for (const file of files) {
  const result = spawnSync(process.execPath, ["--test", join(testDir, file)], {
    stdio: "inherit",
  });
  if (result.status !== 0) {
    failed = true;
    process.stderr.write(`node-typescript-scanner: ${file} failed (${result.status})\n`);
  }
}
if (failed) {
  process.exitCode = 1;
} else {
  process.stdout.write(
    JSON.stringify({ ok: true, gate: "node-typescript-scanner", files: files.length }) + "\n",
  );
}
