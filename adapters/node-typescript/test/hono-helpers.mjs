/**
 * Shared helpers for the issue #115 Hono provider suite. Only the
 * current Node interpreter is ever spawned; fixture projects are
 * materialized into disposable os.tmpdir() roots and scanned through
 * the production bundle.
 */
import { cpSync, mkdtempSync, realpathSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const repoRoot = realpathSync(resolve(dirname(fileURLToPath(import.meta.url)), "../../.."));

/** The production bundle (vendored compiler + scanner + kernel). */
export async function loadAdapter() {
  return import("file:///" + join(repoRoot, "adapters/node-typescript/adapter.mjs").split("\\").join("/"));
}

export const honoFixtureRoot = join(repoRoot, "tests/fixtures/node-typescript-scanner/hono");

/** A fresh disposable copy of one Hono fixture project. */
export function materializeHonoFixture(tag, project) {
  const root = realpathSync(mkdtempSync(join(tmpdir(), `lekalo-hono-${tag}-`)));
  const copy = join(root, "project");
  cpSync(join(honoFixtureRoot, project), copy, { recursive: true });
  return { root, project: copy };
}

export function dispose(root) {
  if (root.startsWith(realpathSync(tmpdir()))) {
    rmSync(root, { recursive: true, force: true });
  }
}

const BASE_ROOTS = [
  { kind: "tree", path: "src", scope: "src/**" },
  { kind: "tree", path: "types", scope: "types/**" },
  { kind: "file", path: "package.json", scope: "package.json" },
  { kind: "file", path: "tsconfig.json", scope: "tsconfig.json" },
];

function rootsFor(project) {
  const roots = [...BASE_ROOTS];
  if (project === "tests") {
    roots.push({ kind: "tree", path: "tests", scope: "tests/**" });
  }
  if (project === "static") {
    roots.push({ kind: "file", path: "lekalo/endpoints.json", scope: "lekalo/endpoints.json" });
  }
  return roots;
}

/**
 * Scan one Hono fixture project through the production bundle. Returns
 * the scan context; the caller owns `root` and must dispose it.
 */
export async function scanHonoFixture(tag, project, options = {}) {
  const adapter = await loadAdapter();
  const kernel = adapter.__lekaloKernel;
  const scanner = adapter.__lekaloScanner;
  const copy = materializeHonoFixture(tag, options.pristine === false ? project : project);
  const roots = options.roots ?? rootsFor(project);
  const profile = kernel.validateResolvedProjectProfile({
    id: "standalone",
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
  const readView = kernel.createReadView(copy.project, roots, profile);
  const session = new scanner.ScannerSession();
  const result = session.scan({
    profile,
    readView,
    permittedProjectRoot: copy.project,
    frameworks: options.frameworks ?? ["hono"],
  });
  return {
    adapter,
    kernel,
    scanner,
    index: result.index,
    session,
    profile,
    readView,
    project: copy.project,
    root: copy.root,
    hono: result.index.frameworks?.hono ?? null,
  };
}

export function recordsOf(context) {
  return context.hono?.records ?? [];
}

export function recordsOfRelation(context, name) {
  return recordsOf(context).filter((record) => record.relation === `dev.lekalo.hono/${name}`);
}

export function oneRecord(context, name, message) {
  const rows = recordsOfRelation(context, name);
  assertHas(rows.length >= 1, message ?? `expected at least one ${name} record`);
  return rows;
}

import assert from "node:assert/strict";
export const assertHas = (condition, message) => assert.ok(condition, message);
