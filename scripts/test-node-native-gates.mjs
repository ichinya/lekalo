#!/usr/bin/env node
/**
 * Issue #48 native gate suite: the committed workspace/plan/contract
 * fixtures, executed with Node directly — no package manager, no shell,
 * no project scripts. Everything runs in-process against the source
 * modules; the fixture runner Rust suites cover the execution half.
 */
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const testDir = join(root, "adapters", "node-typescript", "test");
const files = [
  "native-workspace.test.mjs",
  "native-plan.test.mjs",
  "native-contract.test.mjs",
  "native-extension.test.mjs",
];

let failed = false;
for (const file of files) {
  const result = spawnSync(process.execPath, ["--test", join(testDir, file)], {
    stdio: "inherit",
  });
  if (result.status !== 0) {
    failed = true;
    process.stderr.write(`node-native-gates: ${file} failed (${result.status})\n`);
  }
}
if (failed) {
  process.exitCode = 1;
} else {
  process.stdout.write(
    JSON.stringify({ ok: true, gate: "node-native-gates", files: files.length }) + "\n",
  );
}
