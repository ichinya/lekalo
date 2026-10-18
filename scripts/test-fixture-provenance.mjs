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
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join, resolve } from "node:path";

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
// defined over TRACKED files, so a working copy without repository
// metadata (the docs-examples replay copy) simply has nothing to check.
let tracked;
try {
  tracked = execFileSync("git", ["ls-files", "-z", "--", "tests/fixtures"], { cwd: root, encoding: "utf8" }).split("\0").filter(Boolean);
} catch {
  tracked = [];
}
let binaryFixtures = 0;
for (const path of tracked) {
  const bytes = await readFile(join(root, path));
  if (!bytes.includes(0) && !path.endsWith('.sqlite')) continue;
  binaryFixtures += 1;
  const attributes = execFileSync("git", ["check-attr", "-z", "text", "eol", "--", path], { cwd: root, encoding: "utf8" }).split("\0");
  assert.equal(attributes[2], "unset", `${path}: binary fixture must be -text`);
  assert.equal(attributes[5], "unspecified", `${path}: binary fixture must not set eol`);
  const canary = Buffer.concat([bytes, Buffer.from("\r\nclean-filter-canary\r\n")]);
  const clean = execFileSync("git", ["hash-object", "--stdin", `--path=${path}`], { cwd: root, input: canary, encoding: "utf8" });
  const raw = execFileSync("git", ["hash-object", "--stdin", "--no-filters"], { cwd: root, input: canary, encoding: "utf8" });
  assert.equal(clean, raw, `${path}: clean filtering rewrites binary fixture bytes`);
}
assert.ok(tracked.length === 0 || binaryFixtures > 0, "binary fixture filter regression must exercise real fixtures");

console.log(JSON.stringify({
  ok: true,
  binaryFixtures,
  families: onDisk.length,
  synthetic,
  evidenceBacked,
}));
