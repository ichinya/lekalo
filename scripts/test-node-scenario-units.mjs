/**
 * #47 scenario unit suites (review F-11): the mapper, emitter, binding,
 * and extension suites run as a named gate, not only as an ad-hoc
 * `node --test` glob. Node built-ins only; no package manager, no
 * shell, no project scripts. Every spawn is the current Node
 * interpreter, so the gate is portable across CI shells.
 *
 * Suites (issue #47 plans S3-S7):
 *   scenario-map       — scenario-ir mapping, leaves, bounds, ids
 *   scenario-emit      — deterministic test-file emission, redaction
 *   scenario-bindings  — checked bindings and multi-claim joins
 *   scenario-extension — the compiler extension's wire surface
 */
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const testDir = join(root, "adapters", "node-typescript", "test");
const files = [
  "scenario-map.test.mjs",
  "scenario-emit.test.mjs",
  "scenario-bindings.test.mjs",
  "scenario-extension.test.mjs",
];

let failed = false;
for (const file of files) {
  const result = spawnSync(process.execPath, ["--test", join(testDir, file)], {
    stdio: "inherit",
  });
  if (result.status !== 0) {
    failed = true;
    process.stderr.write(`node-scenario-units: ${file} failed (${result.status})\n`);
  }
}
if (failed) {
  process.exitCode = 1;
} else {
  process.stdout.write(
    JSON.stringify({ ok: true, gate: "node-scenario-units", files: files.length }) + "\n",
  );
}
