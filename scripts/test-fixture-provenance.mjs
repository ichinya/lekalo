#!/usr/bin/env node
// Fixture provenance gate (issue #119, plan S7).
//
// Every public-facing fixture family under `tests/fixtures/` must be
// declared in `tests/fixtures/fixture-provenance.json` with either the
// exact `synthetic` origin or explicit permission/license/consent
// evidence refs, mirroring the evaluator's public-fixture rules. The
// gate fails closed: an undeclared family, an unknown origin, missing
// evidence refs, or a family directory that disappeared all fail the
// gate. No private material may become a public fixture.

import assert from "node:assert/strict";
import { existsSync, readdirSync, statSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { basename, dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fixtureGit } from "./lib/fixture-git.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const fixturesDir = resolve(root, "tests/fixtures");
const manifestPath = join(fixturesDir, "fixture-provenance.json");

const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
assert.equal(manifest.manifestId, "dev.lekalo.fixture-provenance", "manifest identity");
assert.ok(typeof manifest.version === "string" && manifest.version.length > 0, "manifest version");
assert.ok(Array.isArray(manifest.families) && manifest.families.length > 0, "families list");

// The closed set of on-disk families. Files (like the manifest
// itself) are not families.
const onDisk = readdirSync(fixturesDir)
  .filter((name) => !name.startsWith("."))
  .filter((name) => statSync(join(fixturesDir, name)).isDirectory())
  .sort();

const declared = manifest.families.map((family) => family.family).sort();
assert.deepEqual(
  declared,
  onDisk,
  "the manifest must cover exactly the fixture families on disk (fail-closed)",
);

let synthetic = 0;
let evidenceBacked = 0;
for (const family of manifest.families) {
  assert.ok(
    existsSync(join(fixturesDir, family.family)),
    `family ${family.family} must exist on disk`,
  );
  assert.ok(
    family.origin === "synthetic" || family.origin === "evidence-backed",
    `family ${family.family}: origin must be "synthetic" or "evidence-backed"`,
  );
  if (family.origin === "synthetic") {
    synthetic += 1;
    continue;
  }
  // Evidence-backed: the exact three purpose-bound refs, each an
  // opaque evidence identity with a stable kind and purpose.
  evidenceBacked += 1;
  for (const refName of ["permissionRef", "licenseRef", "consentRef"]) {
    const evidence = family[refName];
    assert.ok(evidence, `family ${family.family}: missing ${refName}`);
    assert.match(
      evidence.evidenceId,
      /^evidence-sha256:[0-9a-f]{64}$/,
      `family ${family.family}/${refName}: opaque evidence identity`,
    );
    assert.ok(
      typeof evidence.note === "string" && evidence.note.length > 0,
      `family ${family.family}/${refName}: human trace note`,
    );
  }
}

// No declared family may carry anything but the declared members.
for (const family of manifest.families) {
  const allowed =
    family.origin === "synthetic"
      ? ["family", "origin", "note"]
      : ["family", "origin", "permissionRef", "licenseRef", "consentRef", "note"];
  for (const member of Object.keys(family)) {
    assert.ok(
      allowed.includes(member),
      `family ${family.family}: unknown member ${member}`,
    );
  }
}

// Binary fixtures must survive Git's clean filter byte-for-byte (issue #41
// r7 devin F4). Check every tracked NUL-bearing fixture, not just one SQLite
// example, and append a CRLF canary to exercise normalization even if a
// fixture's current bytes happen not to contain any CRLF. The check is
// defined over TRACKED files. Replay copies create a private index; a
// missing or broken Git inventory must fail rather than skip this leg.
const tracked = fixtureGit(root, ["ls-files", "-z", "--", "tests/fixtures"], "inventory").split("\0").filter(Boolean);
let binaryFixtures = 0;
const scratch = realpathSync.native(mkdtempSync(join(tmpdir(), "lekalo-fixture-provenance-")));
try {
for (const path of tracked) {
  const bytes = await readFile(join(root, path));
  if (!bytes.includes(0) && !path.endsWith('.sqlite')) continue;
  binaryFixtures += 1;
  const attributes = fixtureGit(root, ["check-attr", "-z", "text", "eol", "--", path], "attributes").split("\0");
  assert.equal(attributes[2], "unset", `${path}: binary fixture must be -text`);
  assert.equal(attributes[5], "unspecified", `${path}: binary fixture must not set eol`);
  const canary = Buffer.concat([bytes, Buffer.from("\r\nclean-filter-canary\r\n")]);
  // Use a real file to avoid a large buffered stdin pipe to synchronous Git.
  // --path retains the tracked fixture's actual attributes and clean filter.
  const canaryPath = join(scratch, "canary");
  writeFileSync(canaryPath, canary);
  const clean = fixtureGit(root, ["hash-object", `--path=${path}`, "--", canaryPath], "clean-hash");
  const raw = fixtureGit(root, ["hash-object", "--no-filters", "--", canaryPath], "raw-hash");
  assert.equal(clean, raw, `${path}: clean filtering rewrites binary fixture bytes`);
}
} finally {
  assert.equal(dirname(scratch), realpathSync.native(tmpdir()));
  assert.ok(basename(scratch).startsWith("lekalo-fixture-provenance-"));
  rmSync(scratch, { recursive: true, force: true });
}
assert.ok(binaryFixtures > 0, "binary fixture filter regression must exercise real fixtures");

console.log(JSON.stringify({
  ok: true,
  binaryFixtures,
  families: onDisk.length,
  synthetic,
  evidenceBacked,
}));
