#!/usr/bin/env node
// Issue #45 Zod generator suite: the mapper, the emitter (with golden
// byte-compare and runtime execution), the policy grammar, and the
// kernel-dispatch extension vectors — executed with Node directly, no
// package manager, no package scripts, no install step.
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const testDir = join(root, "adapters", "node-typescript", "test");
const files = [
  "zod-map.test.mjs",
  "zod-emit.test.mjs",
  "zod-policy.test.mjs",
  "zod-gen.test.mjs",
];

let failed = false;
for (const file of files) {
  const result = spawnSync(process.execPath, ["--test", join(testDir, file)], {
    stdio: "inherit",
  });
  if (result.status !== 0) {
    failed = true;
    process.stderr.write(`node-zod-gen: ${file} failed (${result.status})\n`);
  }
}
if (failed) {
  process.exitCode = 1;
} else {
  process.stdout.write(
    JSON.stringify({ ok: true, gate: "node-zod-gen", files: files.length }) + "\n",
  );
}
