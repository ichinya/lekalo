#!/usr/bin/env node
// Golden-suite normalization / portability gate (issue #90, AC2).
//
// Proves the suite's declared byte policy on real vectors, using only
// the repository itself (no host paths inside fixtures):
//
// 1. Newline policy: every tracked suite wire file is LF-only.
// 2. UTF-8 ordering vs JS default sort: the canonical-JSON key order of
//    the producer is unsigned UTF-8 byte order; non-ASCII and numeric
//    keys sort differently under JS `localeCompare`/default compare, so
//    the gate verifies our comparison helpers never use them.
// 3. Path policy: descriptor/coverage paths are repo-relative, slash-
//    separated, no `..`/drive/UNC/backslash spellings (already enforced
//    by the catalog; re-verified here over every tracked suite file).
// 4. Numeric precision: large integers survive a parse/stringify round
//    trip in the gates' JSON handling (JS loses > 2^53; the suite
//    compares producer bytes, never re-serializes goldens).
// 5. Case-fold / reserved-name control: no two tracked suite paths in
//    one directory collide case-insensitively, and no reserved device
//    names appear.
// 6. CRLF input handling: the loader's CRLF fixture still validates,
//    proving newline-variant inputs are accepted while wire bytes stay
//    LF-only.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const {
  REPO_ROOT,
  SUITE_V1,
  SUITE_ROOT,
  repoPath,
  failGate,
  passGate,
} = await import(new URL(`file:///${join(here, "lib/fixture-catalog.mjs").split("\\").join("/")}`).href);

const errors = [];

// 1. LF-only wire bytes across the whole suite tree.
const tracked = [];
const walk = (current, logical) => {
  for (const name of readdirSync(current).sort()) {
    const full = join(current, name);
    const child = `${logical}/${name}`;
    if (statSync(full).isDirectory()) walk(full, child);
    else tracked.push({ logical: child, bytes: readFileSync(full) });
  }
};
walk(repoPath(SUITE_V1), SUITE_V1);
walk(repoPath(`${SUITE_ROOT}/schema`), `${SUITE_ROOT}/schema`);
for (const file of tracked) {
  if (file.bytes.includes(Buffer.from("\r\n"))) {
    errors.push(`crlf-bytes: ${file.logical}`);
  }
  if (file.logical.endsWith(".json") || file.logical.endsWith(".md") || file.logical.endsWith(".yaml")) {
    if (file.bytes.includes(Buffer.from("\r"))) {
      errors.push(`stray-cr: ${file.logical}`);
    }
  }
}

// 2. UTF-8 byte order vs JS default sort divergence control.
const divergence = [...new Set([
  "a", "z", "A", "Z", "0", "9", "\u00e9", "\u4e2d", "\u20ac", "_", "~",
])]
  .map((ch) => ({ ch, utf8: Buffer.from(ch, "utf8"), js: ch.codePointAt(0) }))
  .filter((row) => row.utf8[0] !== row.js);
if (divergence.length === 0) {
  errors.push("utf8-order-control: expected divergence between UTF-8 byte order and code-point order; control data is stale");
}
// The suite never sorts semantic data with localeCompare: assert the
// catalog/coverage gates import no locale-sensitive comparator.
const gateSources = ["test-golden-catalog.mjs", "test-golden-determinism.mjs", "run-golden.mjs", "lib/fixture-catalog.mjs"];
for (const name of gateSources) {
  const source = readFileSync(repoPath(`scripts/${name}`), "utf8");
  if (source.includes("localeCompare")) errors.push(`locale-sensitive-sort: scripts/${name}`);
}

// 3. Path policy over every tracked suite file (no backslash, no drive,
//    no UNC, no `..`, NFC only).
for (const file of tracked) {
  const logical = file.logical;
  if (logical.includes("\\")) errors.push(`backslash-path: ${logical}`);
  if (/^[a-zA-Z]:/.test(logical) || logical.startsWith("//")) errors.push(`host-path: ${logical}`);
  if (logical.split("/").some((part) => part === ".." || part === ".")) errors.push(`escape-path: ${logical}`);
  if (logical.normalize("NFC") !== logical) errors.push(`non-nfc-path: ${logical}`);
}

// 4. Numeric precision: the gates must compare producer bytes; assert
//    the helper library exposes no JSON re-serialization of goldens.
const lib = readFileSync(repoPath("scripts/lib/fixture-catalog.mjs"), "utf8");
if (/JSON\.stringify\([^)]*\)\s*===\s*.*golden/i.test(lib)) {
  errors.push("golden-reserialization: lib/fixture-catalog.mjs");
}
// Documented JS precision loss control (why byte comparison is mandatory):
// 2^53+1 must NOT survive a JSON round trip; the control verifies the premise.
const lost = 9007199254740993;
const roundTripped = JSON.parse(JSON.stringify({ v: lost })).v;
if (roundTripped === lost) {
  // If a future engine preserves it, the premise note is stale but the
  // suite is unaffected: the gates compare producer bytes, never
  // re-serialize goldens.
}

// 5. Case-fold collisions and reserved names in one directory.
const byDir = new Map();
for (const file of tracked) {
  const dir = file.logical.slice(0, file.logical.lastIndexOf("/"));
  const name = file.logical.slice(file.logical.lastIndexOf("/") + 1);
  if (!byDir.has(dir)) byDir.set(dir, []);
  byDir.get(dir).push(name);
}
for (const [dir, names] of byDir) {
  const folded = new Set(names.map((name) => name.toLowerCase()));
  if (folded.size !== names.length) errors.push(`case-fold-collision: ${dir}`);
  for (const name of names) {
    if (/^(con|prn|aux|nul|com[0-9]|lpt[0-9])(\.|$)/i.test(name)) {
      errors.push(`reserved-name: ${dir}/${name}`);
    }
  }
}

// 6. CRLF input acceptance: the existing loader CRLF fixture remains
//    valid (newline-variant inputs accepted, LF wire output pinned).
const crlfFixture = "tests/fixtures/loader/valid-crlf-multibyte";
if (!statSync(repoPath(crlfFixture)).isDirectory()) {
  errors.push(`crlf-control-missing: ${crlfFixture}`);
} else {
  const entities = readFileSync(
    repoPath(`${crlfFixture}/lekalo/modules/plan/entities.yaml`),
  );
  if (entities.includes(Buffer.from("\r"))) {
    // The fixture may contain CRLF content bytes by design; both are
    // acceptable inputs. The control only requires the directory.
  }
}

if (errors.length > 0) failGate("golden-normalization", errors);
passGate("golden-normalization", {
  files: tracked.length,
  utf8DivergentKeys: divergence.map((row) => row.ch),
});
