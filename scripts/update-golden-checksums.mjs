#!/usr/bin/env node
// Suite-v1 checksum sidecar writer (issue #90 fix round 1).
//
// Recomputes the sha256 of every file owned by each catalogued case and
// writes `tests/fixtures/suite/v1/checksums/<caseId>.json` sidecars.
// Deterministic; run through the reviewed update flow. The catalog gate
// verifies every sidecar entry against the tracked bytes and refuses
// drift, missing files, or unlisted case files.
//
// Usage: node scripts/update-golden-checksums.mjs

import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const {
  SUITE_V1,
  loadCatalog,
  loadCases,
  sha256,
  failGate,
  passGate,
} = await import(new URL(`file:///${join(here, "lib/fixture-catalog.mjs").split("\\").join("/")}`).href);

const catalogLoad = loadCatalog();
if (catalogLoad.errors.length > 0) failGate("golden-checksums", catalogLoad.errors);
const loaded = loadCases(catalogLoad.catalog);
if (loaded.errors.length > 0) failGate("golden-checksums", loaded.errors);

/** Every file under a repo-relative directory, as repo-relative paths. */
function caseFiles(caseDirLogical) {
  const absolute = join(repoRoot, caseDirLogical);
  const out = [];
  const walk = (current, logical) => {
    for (const name of readdirSync(current).sort()) {
      const full = join(current, name);
      const child = `${logical}/${name}`;
      if (statSync(full).isDirectory()) walk(full, child);
      else out.push(child);
    }
  };
  walk(absolute, caseDirLogical);
  return out;
}

let sidecars = 0;
let entries = 0;
for (const entry of loaded.cases) {
  const caseDir = entry.path.split("/").slice(0, -1).join("/");
  const files = caseFiles(caseDir);
  const sidecar = {
    caseId: entry.id,
    revision: entry.revision,
    algorithm: "sha256",
    files: files.map((path) => ({ path, sha256: sha256(readFileSync(join(repoRoot, path))) })),
  };
  writeFileSync(
    join(repoRoot, SUITE_V1, "checksums", `${entry.id}.json`),
    `${JSON.stringify(sidecar, null, 2)}\n`,
  );
  sidecars += 1;
  entries += sidecar.files.length;
}

passGate("golden-checksums", { sidecars, entries });
