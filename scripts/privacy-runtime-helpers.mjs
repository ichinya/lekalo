#!/usr/bin/env node
// Shared lookup of the built `lekalo` CLI binary for the privacy
// runtime gates (issue #119).
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));

export function findLekaloBinary() {
  const binaryName = process.platform === "win32" ? "lekalo.exe" : "lekalo";
  for (const profile of ["debug", "release"]) {
    const candidate = join(root, "target", profile, binaryName);
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

export function buildLekaloBinary() {
  const build = spawnSync("cargo", ["build", "-p", "lekalo-cli", "--locked"], {
    cwd: root,
    encoding: "utf8",
    windowsHide: true,
  });
  if (build.status !== 0) {
    console.error(build.stderr || build.stdout);
  }
  assert.equal(build.status, 0, "the lekalo CLI must build");
  return findLekaloBinary();
}

export function runLekalo(binary, args, options = {}) {
  return spawnSync(binary, args, {
    encoding: "utf8",
    windowsHide: true,
    maxBuffer: 16 * 1024 * 1024,
    ...options,
  });
}
