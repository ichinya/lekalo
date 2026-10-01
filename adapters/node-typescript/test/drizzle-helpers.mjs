/**
 * Shared helpers for the #116 Drizzle evidence suite (mirrors the #44
 * scanner helpers). Only the current Node interpreter is ever spawned;
 * fixture projects are materialized into disposable os.tmpdir() roots.
 */
import { cpSync, mkdtempSync, readdirSync, realpathSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const repoRoot = realpathSync(resolve(dirname(fileURLToPath(import.meta.url)), "../../.."));

/** The production bundle (vendored compiler + scanner + kernel). */
export async function loadAdapter() {
  return import("file:///" + join(repoRoot, "adapters/node-typescript/adapter.mjs").split("\\").join("/"));
}

export const fixtureRoot = join(repoRoot, "tests/fixtures/node-typescript-drizzle");

/** A fresh disposable copy of one committed fixture project. */
export function materializeFixture(tag, relative = "postgres") {
  const root = realpathSync(mkdtempSync(join(tmpdir(), `lekalo-i116-${tag}-`)));
  const project = join(root, "project");
  cpSync(join(fixtureRoot, relative), project, { recursive: true });
  return { root, project };
}

export function dispose(root) {
  if (root.startsWith(realpathSync(tmpdir()))) {
    rmSync(root, { recursive: true, force: true });
  }
}

/** Byte snapshot of the whole project tree (read-only proof, AC7). */
export function snapshotTree(project) {
  const files = [];
  const walk = (dir, prefix) => {
    for (const name of readdirSync(dir).sort()) {
      const full = join(dir, name);
      const key = prefix === "" ? name : `${prefix}/${name}`;
      if (statSync(full).isDirectory()) walk(full, key);
      else files.push([key, statSync(full).size]);
    }
  };
  walk(project, "");
  return files;
}

const BASE_ROOTS = [
  { kind: "tree", path: "src", scope: "src/**" },
  { kind: "tree", path: "drizzle", scope: "drizzle/**" },
  { kind: "file", path: "package.json", scope: "package.json" },
  { kind: "file", path: "tsconfig.json", scope: "tsconfig.json" },
];

function rootsFor(includeOwnerInputs) {
  if (!includeOwnerInputs) return BASE_ROOTS;
  return [
    ...BASE_ROOTS,
    { kind: "file", path: "drizzle.bindings.json", scope: "drizzle.bindings.json" },
    { kind: "file", path: "drizzle.projection.json", scope: "drizzle.projection.json" },
  ];
}

/**
 * Build a scan-enabled kernel over the materialized fixture with the
 * standard roots and run exactly one scan. Returns the full scan index
 * plus the materialization handles for freshness edits.
 */
export async function scanDrizzleFixture(tag, relative, options = {}) {
  const adapter = await loadAdapter();
  const kernel = adapter.__lekaloKernel;
  const scanner = adapter.__lekaloScanner;
  const { root, project } = materializeFixture(tag, relative);
  const roots = rootsFor(options.ownerInputs ?? true);
  const profile = kernel.validateResolvedProjectProfile({
    id: options.profileId ?? "standalone",
    mode: "observed",
    target: "node-typescript",
    readRoots: roots.map(({ kind, path }) => ({ kind, path })),
    exclusions: options.exclusions ?? [],
    provenance: {
      origin: "declared",
      revision: options.revision ?? "fixture-revision-0001",
      disposition: "public-fixture",
    },
  });
  const readView = kernel.createReadView(project, roots, profile);
  const session = new scanner.ScannerSession();
  const scan = () => session.scan({ profile, readView, permittedProjectRoot: project });
  try {
    const first = scan();
    return {
      adapter, kernel, scanner, profile, readView, session,
      index: first.index, project, root,
      rescan: scan,
      error: (error) => { dispose(root); throw error; },
    };
  } catch (error) {
    dispose(root);
    throw error;
  }
}
