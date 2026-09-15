/**
 * Shared helpers for the #44 scanner suite. Only the current Node
 * interpreter is ever spawned; fixture projects are materialized into
 * disposable os.tmpdir() roots.
 */
import { cpSync, mkdtempSync, readFileSync, realpathSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const repoRoot = realpathSync(resolve(dirname(fileURLToPath(import.meta.url)), "../../.."));

/** The production bundle (vendored compiler + scanner + kernel). */
export async function loadAdapter() {
  return import("file:///" + join(repoRoot, "adapters/node-typescript/adapter.mjs").split("\\").join("/"));
}

export const fixtureRoot = join(repoRoot, "tests/fixtures/node-typescript-scanner");

/** A fresh disposable copy of one committed fixture project. */
export function materializeFixture(tag, relative = "esm") {
  const root = realpathSync(mkdtempSync(join(tmpdir(), `lekalo-s44-${tag}-`)));
  const project = join(root, "project");
  cpSync(join(fixtureRoot, relative), project, { recursive: true });
  return { root, project };
}

export function dispose(root) {
  if (root.startsWith(realpathSync(tmpdir()))) {
    rmSync(root, { recursive: true, force: true });
  }
}

/**
 * Build a scan-enabled kernel over the materialized project with the
 * given roots (defaults: src/** + manifests) and run exactly one scan.
 */
export async function scanFixture(tag, relative, options = {}) {
  const adapter = await loadAdapter();
  const kernel = adapter.__lekaloKernel;
  const scanner = adapter.__lekaloScanner;
  const { root, project } = materializeFixture(tag, relative);
  const roots = options.roots ?? [
    { kind: "tree", path: "src", scope: "src/**" },
    { kind: "file", path: "package.json", scope: "package.json" },
    { kind: "file", path: "tsconfig.json", scope: "tsconfig.json" },
  ];
  const profile = kernel.validateResolvedProjectProfile({
    id: options.profileId ?? "standalone",
    mode: "observed",
    target: "node-typescript",
    readRoots: roots.map(({ kind, path }) => ({ kind, path })),
    exclusions: options.exclusions ?? [],
    provenance: { origin: "declared", revision: options.revision ?? "fixture-revision-0001", disposition: "public-fixture" },
  });
  const readView = kernel.createReadView(project, roots, profile);
  const session = new scanner.ScannerSession();
  try {
    const result = session.scan({ profile, readView, permittedProjectRoot: project });
    return { adapter, kernel, scanner, index: result.index, session, project, root, profile, readView };
  } catch (error) {
    dispose(root);
    throw error;
  }
}

/** Assert one scan result is byte-identical to a prior scan's JSON. */
export function assertColdWarmParity(assert, session, options) {
  const second = session.scan(options);
  assert.equal(
    JSON.stringify(second.index),
    JSON.stringify(session.retain.index === undefined ? second.index : session.retain.index),
  );
  return second;
}
