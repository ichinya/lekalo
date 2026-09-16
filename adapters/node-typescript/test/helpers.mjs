/**
 * Shared test utilities for the #43 kernel suite.
 * Node built-ins only; the suite never launches a package manager,
 * a shell, or a project command — every spawned process is `node`
 * itself or the lekalo binary under an explicit argv allowlist.
 */
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const repoRoot = realpathSync(resolve(dirname(fileURLToPath(import.meta.url)), "../../.."));

export const adapterPath = join(repoRoot, "adapters/node-typescript/adapter.mjs");
/** The unbundled source entry used by the in-process suites. */
export const sourceEntryPath = join(repoRoot, "adapters/node-typescript/main.mjs");
export const kernelSourcePath = join(repoRoot, "adapters/node-typescript/src/kernel.mjs");
export const fixtureRoot = join(repoRoot, "tests/fixtures/node-typescript-kernel");

export function readFixture(relative) {
  return JSON.parse(readFileSync(join(fixtureRoot, relative), "utf8"));
}

export function readFixtureBytes(relative) {
  return readFileSync(join(fixtureRoot, relative));
}

export function sha256Bytes(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

/** A fresh disposable copy of the synthetic fixture project. */
export function materializeFixtureProject(tag) {
  const root = realpathSync(mkdtempSync(join(tmpdir(), `lekalo-k43-${tag}-`)));
  const project = join(root, "project");
  cpSync(join(fixtureRoot, "project"), project, { recursive: true });
  return { root, project };
}

export function disposableRoot(tag) {
  return realpathSync(mkdtempSync(join(tmpdir(), `lekalo-k43-${tag}-`)));
}

export function dispose(root) {
  // Only ever called on roots this suite created under os.tmpdir().
  if (root.startsWith(realpathSync(tmpdir()))) {
    rmSync(root, { recursive: true, force: true });
  }
}

/** Create a symlink (or junction on Windows); false when unsupported. */
export function symlinkIfPossible(project, linkName, target) {
  try {
    symlinkSync(target, join(project, linkName), "junction");
    return true;
  } catch {
    return false;
  }
}

/** Snapshot every fixture project file (path → sha256), recursively. */
export function snapshotProject(project) {
  const entries = {};
  const walk = (current) => {
    for (const entry of readdirSorted(current)) {
      const full = join(current, entry.name);
      if (entry.isDirectory()) {
        walk(full);
      } else {
        entries[full.slice(project.length + 1).replaceAll("\\", "/")] = sha256Bytes(
          readFileSync(full),
        );
      }
    }
  };
  walk(project);
  return entries;
}

import { readdirSync } from "node:fs";
function readdirSorted(directory) {
  return readdirSync(directory, { withFileTypes: true })
    .sort((left, right) => (left.name < right.name ? -1 : 1));
}

/**
 * Run the adapter as a real one-shot process. The ONLY program this
 * helper ever spawns is the Node interpreter running the adapter
 * script — the suite's explicit allowlist.
 */
export function runAdapter(requestBytes, extraArgs = [], options = {}) {
  return spawnSync(
    process.execPath,
    [adapterPath, ...extraArgs],
    {
      input: requestBytes,
      encoding: "buffer",
      timeout: 30000,
      ...options,
    },
  );
}

export function canonical(value) {
  if (Array.isArray(value)) {
    return `[${value.map(canonical).join(",")}]`;
  }
  if (value !== null && typeof value === "object") {
    const keys = Object.keys(value).sort((a, b) => {
      const left = Buffer.from(a, "utf8");
      const right = Buffer.from(b, "utf8");
      const length = Math.min(left.length, right.length);
      for (let index = 0; index < length; index += 1) {
        if (left[index] !== right[index]) {
          return left[index] - right[index];
        }
      }
      return left.length - right.length;
    });
    return `{${keys.map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

export function deterministicDescribeRequest() {
  return {
    limits: { max_output_bytes: 8388608, timeout_ms: 600000 },
    operation: "describe",
    project_root: ".",
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.1",
    request_id: "req-1b2c9180d660f980e22741574e778fa6dcd11fd7a960b9ad89f7a48485e5988c",
  };
}

export const projectProfile = () => readFixture("profiles/standalone.valid.json");
export const expectedNormalization = () => readFixture("expected/normalization.json");
export const extensionResults = () => readFixture("extensions/results.json");

export { join, resolve };
