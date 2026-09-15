#!/usr/bin/env node
// Issue #43 kernel test runner: the four committed test files, executed
// with Node directly — no package manager, no package scripts, no
// install step. Hermetic apart from the pinned Node itself.
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const testDir = join(root, "adapters", "node-typescript", "test");
const files = [
  "kernel.test.mjs",
  "process.test.mjs",
  "roots.test.mjs",
  "fixtures.test.mjs",
];

let failed = false;
for (const file of files) {
  const result = spawnSync(process.execPath, ["--test", join(testDir, file)], {
    stdio: "inherit",
    ...(process.platform === "win32" ? {} : {}),
  });
  if (result.status !== 0) {
    failed = true;
    process.stderr.write(`node-typescript-kernel: ${file} failed (${result.status})\n`);
  }
}
if (failed) {
  process.exitCode = 1;
} else {
  process.stdout.write(
    JSON.stringify({ ok: true, gate: "node-typescript-kernel", files: files.length }) + "\n",
  );
}
