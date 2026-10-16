#!/usr/bin/env node
// Issue #33 r7: black-box proof that every MCP and landed sibling gate
// refuses a registry missing any landed M8 family. All mutations live in a disposable Git
// repository; the checkout's registry/index never changes.
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
execFileSync(process.execPath, [join(root, "scripts/check-registry-union.mjs")], { cwd: root });
const registryPath = "contracts/diagnostic-registry.v0.8.0.json";
const registry = JSON.parse(readFileSync(join(root, registryPath), "utf8"));
const gates = ["test-mcp-contracts.mjs", "test-mcp-boundaries.mjs", "test-mcp-stdio.mjs", "test-mcp-host.mjs",
  "test-adoption-contracts.mjs", "test-format-contracts.mjs", "test-managed-contracts.mjs",
  "test-portability-contracts.mjs", "test-module-api-contracts.mjs", "test-task-links-contracts.mjs",
  "test-worklog-changelog-contracts.mjs"];
const families = ["format.", "adoption.", "managed.", "portability.", "tasklink.", "worklog-changelog.", "module-api.", "mcp."];
const stage = mkdtempSync(join(tmpdir(), "lekalo-mcp-union-r7-"));
let checks = 0;
try {
  mkdirSync(join(stage, "scripts"));
  mkdirSync(join(stage, "contracts"));
  for (const script of [...gates, "check-registry-union.mjs", "registry-union.mjs", "gen-managed-contracts.mjs"]) {
    copyFileSync(join(root, "scripts", script), join(stage, "scripts", script));
  }
  const writeRegistry = (document) => writeFileSync(join(stage, registryPath), `${JSON.stringify(document, null, 2)}\n`);
  writeRegistry(registry);
  const git = (...args) => execFileSync("git", args, { cwd: stage, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  git("init", "--quiet");
  git("-c", "core.autocrlf=false", "add", "--", registryPath);
  git("-c", "user.name=MCP union regression", "-c", "user.email=mcp-union@example.invalid", "-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "synthetic landed union");
  git("branch", "ichinya/M8");
  // Positive control: this exact synthetic baseline is a complete union.
  execFileSync(process.execPath, [join(stage, "scripts/check-registry-union.mjs")], { cwd: stage });
  checks += 1;
  for (const family of families) {
    assert.ok(registry.entries.some((entry) => entry.id.startsWith(family)), `landed family ${family} absent from control`);
    writeRegistry({ ...registry, entries: registry.entries.filter((entry) => !entry.id.startsWith(family)) });
    for (const gate of gates) {
      const run = spawnSync(process.execPath, [join(stage, "scripts", gate)], {
        cwd: stage, encoding: "utf8", timeout: 120000, maxBuffer: 8 * 1024 * 1024,
      });
      assert.equal(run.error, undefined, `${gate} must run rather than time out`);
      assert.notEqual(run.status, 0, `${gate} accepted missing ${family}`);
      assert.match(run.stderr, /registry union guard:/, `${gate} must refuse through the shared union guard, not a missing dependency/schema`);
      assert.ok(run.stderr.includes(`dropped landed families (${family})`), `${gate} did not identify missing ${family}`);
      checks += 1;
    }
  }
  console.log(JSON.stringify({ ok: true, gate: "mcp-union", gates: gates.length, landedFamilies: families.length, checks }));
} finally {
  rmSync(stage, { recursive: true, force: true });
}
