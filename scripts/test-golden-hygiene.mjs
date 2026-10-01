#!/usr/bin/env node
// Golden-suite hygiene gate (issue #90, AC7).
//
// Scans every tracked suite file (and the suite gates) for real
// credentials and host-specific absolute paths. The suite is fully
// synthetic: no family-wide exemptions exist. Detection reuses the
// privacy leak corpus's closed classes (secret-token, url, path-
// fragment, email) as inputs and refuses:
//
// - credential-shaped strings (bearer/api keys, JWTs, AWS keys,
//   password assignments) with no allowlist at all;
// - host-anchored absolute paths: workspace roots, TEMP/HOME roots,
//   drive-letter and UNC spellings, file:// URIs;
// - this repository's own absolute build/checkout paths.
//
// Clean and failing controls prove the scanner is live: a hostile
// synthetic string must be caught, a clean marker must not.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { tmpdir, homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const {
  REPO_ROOT,
  SUITE_ROOT,
  repoPath,
  failGate,
  passGate,
} = await import(new URL(`file:///${join(here, "lib/fixture-catalog.mjs").split("\\").join("/")}`).href);

const errors = [];
const scanned = [];

const walk = (current, logical) => {
  for (const name of readdirSync(current).sort()) {
    const full = join(current, name);
    const child = `${logical}/${name}`;
    if (statSync(full).isDirectory()) walk(full, child);
    else scanned.push({ logical: child, text: readFileSync(full, "utf8") });
  }
};
walk(repoPath(SUITE_ROOT), SUITE_ROOT);
scanned.push({ logical: "scripts/run-golden.mjs", text: readFileSync(join(REPO_ROOT, "scripts", "run-golden.mjs"), "utf8") });
scanned.push({ logical: "scripts/lib/fixture-catalog.mjs", text: readFileSync(join(REPO_ROOT, "scripts", "lib", "fixture-catalog.mjs"), "utf8") });

// The closed hostile patterns. Every match is a failure; there are no
// exemptions because the suite contains only authored synthetic data.
const hostRoots = [REPO_ROOT, tmpdir(), homedir()]
  .map((root) => root.split("\\").join("/"))
  .filter((root) => root.length > 0);
const patterns = [
  // Credentials: never allowed in suite content.
  { name: "secret-assignment", re: /(?:password|passwd|api[-_]?key|secret|token|bearer)\s*[:=]\s*["']?[A-Za-z0-9+/_-]{16,}/i },
  { name: "jwt", re: /eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}/ },
  { name: "aws-access-key", re: /AKIA[0-9A-Z]{16}/ },
  { name: "private-key-block", re: /-----BEGIN [A-Z ]*PRIVATE KEY-----/ },
  { name: "github-token", re: /gh[pousr]_[A-Za-z0-9]{20,}/ },
  { name: "slug-url-credential", re: /https?:\/\/[^\s/:@]+:[^\s/@]+@[^\s/]+/ },
  // Host-anchored paths: the suite is repository-relative only.
  { name: "file-uri", re: /file:\/\// },
  { name: "unc-path", re: /\\\\\\\\[A-Za-z0-9._-]+\\\\/ },
  { name: "drive-letter-path", re: /(?:^|["'\s,(])[A-Za-z]:[\\\\/](?:Users|Windows|Program|temp|Temp)/ },
];

for (const file of scanned) {
  const isFixtureContent = file.logical.startsWith(`${SUITE_ROOT}/`);
  for (const { name, re } of patterns) {
    // file:// appears legitimately in the gate import machinery (.mjs
    // code); it is hostile only inside fixture content.
    if (name === "file-uri" && !isFixtureContent) continue;
    const found = file.text.match(re);
    if (found) errors.push(`${name}: ${file.logical}: ${JSON.stringify(found[0]).slice(0, 80)}`);
  }
  for (const root of hostRoots) {
    if (root.length < 8) continue;
    if (file.text.includes(root)) {
      errors.push(`host-root-path: ${file.logical}: ${root}`);
    }
  }
}

// Live-scanner controls: hostile content must be caught, the clean
// marker must not. Run on synthetic in-memory strings.
const hostile = [
  { name: "control-secret-assignment", text: "password: \"hunter2hunter2hunter2\"" },
  { name: "control-jwt", text: "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJVadQssw5c" },
  { name: "control-aws", text: "AKIAIOSFODNN7EXAMPLE" },
  { name: "control-private-key", text: "-----BEGIN RSA PRIVATE KEY-----" },
  { name: "control-file-uri", text: "file:///C:/Users/host/proj" },
  { name: "control-drive", text: "C:\\Users\\host\\proj" },
];
const detector = (text) =>
  patterns.some(({ re }) => re.test(text))
  || hostRoots.some((root) => root.length >= 8 && text.includes(root));
for (const control of hostile) {
  if (!detector(control.text)) errors.push(`scanner-blind: ${control.name} was not caught`);
}
for (const clean of ["planner.task", "lekalo/modules/planner/entities.yaml", "sha256:0000000000000000000000000000000000000000000000000000000000000000"]) {
  if (detector(clean)) errors.push(`scanner-false-positive: ${JSON.stringify(clean)}`);
}

if (errors.length > 0) failGate("golden-hygiene", errors);
passGate("golden-hygiene", {
  files: scanned.length,
  controls: hostile.length,
  hostRootsChecked: hostRoots.length,
});
