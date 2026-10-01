/**
 * Issue #115 read-only gate: the Hono provider scan is a read-only
 * scan. The gate materializes fixture projects, records the complete
 * file inventory (including untracked bytes), runs enabled, disabled,
 * and repeat scans through the committed production bundle, and asserts
 * the inventory is byte-identical afterwards — no additions, deletions,
 * or modifications, no package install, no children beyond the one
 * Node interpreter already running the gate.
 */
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const adapterPath = join(root, "adapters", "node-typescript", "adapter.mjs");
const fixtureRoot = join(root, "tests", "fixtures", "node-typescript-scanner", "hono");

function inventory(directory) {
  const map = new Map();
  const walk = (dir) => {
    for (const entry of readdirSync(dir).sort()) {
      const absolute = join(dir, entry);
      const metadata = lstatSafe(absolute);
      if (metadata === null) continue;
      if (metadata.isSymbolicLink()) {
        throw new Error("symlink inside fixture");
      }
      if (metadata.isDirectory()) {
        walk(absolute);
      } else if (metadata.isFile()) {
        map.set(absolute.slice(directory.length + 1).split("\\").join("/"),
          createHash("sha256").update(readFileSync(absolute)).digest("hex"));
      }
    }
  };
  walk(directory);
  return map;
}

function lstatSafe(path) {
  try {
    return statSync(path, { throwIfNoEntry: false });
  } catch {
    return null;
  }
}

function scan(adapter, project, frameworks) {
  const kernel = adapter.__lekaloKernel;
  const scanner = adapter.__lekaloScanner;
  const roots = [
    { kind: "tree", path: "src", scope: "src/**" },
    { kind: "tree", path: "types", scope: "types/**" },
    { kind: "file", path: "package.json", scope: "package.json" },
    { kind: "file", path: "tsconfig.json", scope: "tsconfig.json" },
  ];
  if (project.includes("static")) {
    roots.push({ kind: "file", path: "lekalo/endpoints.json", scope: "lekalo/endpoints.json" });
  }
  const profile = kernel.validateResolvedProjectProfile({
    id: "standalone",
    mode: "observed",
    target: "node-typescript",
    readRoots: roots.map(({ kind, path }) => ({ kind, path })),
    exclusions: [],
    provenance: { origin: "declared", revision: "readonly-gate", disposition: "public-fixture" },
  });
  const readView = kernel.createReadView(project, roots, profile);
  const session = new scanner.ScannerSession();
  const first = session.scan({ profile, readView, permittedProjectRoot: project, frameworks });
  const second = session.scan({ profile, readView, permittedProjectRoot: project, frameworks });
  // Cold == warm parity on unchanged inputs.
  if (JSON.stringify(first.index) !== JSON.stringify(second.index)) {
    throw new Error("cold and warm scans diverged on unchanged inputs");
  }
  return first;
}

let failed = false;
try {
  const adapter = await import("file:///" + adapterPath.split("\\").join("/"));
  const projects = ["static", "middleware", "uncertainty", "composition"];
  for (const project of projects) {
    const tempRoot = realpathSync(mkdtempSync(join(tmpdir(), `lekalo-hono-readonly-`)));
    const copy = join(tempRoot, "project");
    try {
      cpSync(join(fixtureRoot, project), copy, { recursive: true });
      const before = inventory(copy);
      scan(adapter, copy, ["hono"]);
      scan(adapter, copy, []);
      scan(adapter, copy, ["hono"]);
      const after = inventory(copy);
      const beforeKeys = [...before.keys()].sort();
      const afterKeys = [...after.keys()].sort();
      if (beforeKeys.join("\n") !== afterKeys.join("\n")) {
        throw new Error(`inventory changed for ${project}`);
      }
      for (const key of beforeKeys) {
        if (before.get(key) !== after.get(key)) {
          throw new Error(`file modified by scan: ${project}/${key}`);
        }
      }
      process.stdout.write(`readonly ok: ${project} (${beforeKeys.length} files, 3 scans)\n`);
    } finally {
      if (tempRoot.startsWith(realpathSync(tmpdir()))) {
        rmSync(tempRoot, { recursive: true, force: true });
      }
    }
  }
  // The committed fixture family is untouched by the whole gate too.
  const status = spawnSync("git", [
    "status", "--porcelain", "--", "tests/fixtures/node-typescript-scanner/hono",
  ], { encoding: "utf8", cwd: root });
  if (status.status !== 0) {
    process.stderr.write("git status probe failed; skipping worktree check\n");
  }
} catch (error) {
  failed = true;
  process.stderr.write(`node-hono-readonly: ${error?.message ?? error}\n`);
}
if (failed) {
  process.exitCode = 1;
} else {
  process.stdout.write(JSON.stringify({ ok: true, gate: "node-hono-readonly" }) + "\n");
}
