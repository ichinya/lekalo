#!/usr/bin/env node
// Syncs the vendored PHP adapter packages (issue #241 stage 4, M2)
// from the local package checkouts into adapters/php-laravel/vendor/
// and pins every vendored file by sha256 in php-packages.lock.json —
// the monorepo builds the transitional unified `adapter.php` from
// these pinned bytes, offline, with no network and no submodules.
//
//   node scripts/sync-php-packages.mjs           # copy + write the lock
//   node scripts/sync-php-packages.mjs --check    # verify the vendored digests
//
// Sources: LEKALO_PHP_DIR and LEKALO_PHP_LARAVEL_DIR, else
// $LEKALO_ADDS_DIR/lekalo-php(-laravel), else ../lekalo-adds/. The
// check mode never reads the sources: CI proves the committed vendor
// tree matches the committed lock byte for byte.
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const adapterRoot = join(root, "adapters", "php-laravel");
const vendorRoot = join(adapterRoot, "vendor");
const lockPath = join(adapterRoot, "php-packages.lock.json");
const check = process.argv.includes("--check");

// The closed vendor manifest: exactly these files, pinned by digest.
const PACKAGES = [
  {
    name: "lekalo-php",
    repo: "https://github.com/ichinya/lekalo-php",
    env: "LEKALO_PHP_DIR",
    files: [
      "mago-toolchain.lock.json",
      "src/provider.php",
      "src/ai-lint.php",
      "src/analyzer.php",
      "src/strict-profile.php",
      "src/kernel.php",
      "src/scenario-map.php",
      "src/type-policy.php",
      "src/type-map.php",
      "src/type-codec.php",
      "src/type-emit.php",
      "src/type-bindings.php",
      "src/operation-policy.php",
      "src/operation-map.php",
      "src/operation-emit.php",
      "src/operation-bindings.php",
      "src/route-policy.php",
      "src/route-map.php",
      "tests/protocol.php",
      "tests/analyzer.php",
    ],
  },
  {
    name: "lekalo-php-laravel",
    repo: "https://github.com/ichinya/lekalo-php-laravel",
    env: "LEKALO_PHP_LARAVEL_DIR",
    files: [
      "composer-gates-policy.json",
      "src/laravel-migrations.php",
      "src/laravel-lint.php",
      "src/laravel-provider.php",
      "src/scenario-emit.php",
      "src/route-emit.php",
      "src/native-policy.php",
      "src/native-plan.php",
      "tests/laravel.php",
    ],
  },
];

const sha256 = (path) => "sha256:" + createHash("sha256").update(readFileSync(path)).digest("hex");

if (check) {
  const lock = JSON.parse(readFileSync(lockPath, "utf8"));
  let ok = true;
  const seen = [];
  for (const pkg of lock.packages) {
    const home = join(vendorRoot, pkg.name);
    const actual = readdirSync(join(home, "src"), { recursive: true });
    for (const [path, digest] of Object.entries(pkg.files)) {
      seen.push(`${pkg.name}/${path}`);
      const vendored = join(home, path);
      try {
        if (sha256(vendored) !== digest) {
          console.error(`vendor digest mismatch: ${pkg.name}/${path}`);
          ok = false;
        }
      } catch {
        console.error(`vendor file missing: ${pkg.name}/${path}`);
        ok = false;
      }
    }
  }
  // No unpinned files ride the vendor tree.
  const walk = (dir) => readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = join(dir, e.name);
    return e.isDirectory() ? walk(p) : [relative(vendorRoot, p).split("\\").join("/")];
  });
  const present = walk(vendorRoot).sort();
  const pinned = seen.sort();
  if (JSON.stringify(present) !== JSON.stringify(pinned)) {
    console.error("vendor tree does not match the pinned file list");
    ok = false;
  }
  console.log(JSON.stringify({ ok, gate: "php-packages-vendor", files: pinned.length }));
  process.exit(ok ? 0 : 1);
}

// Sync mode: copy from the package checkouts and write the lock.
const addsDir = process.env.LEKALO_ADDS_DIR ?? join(dirname(root), "lekalo-adds");
const lock = { formatVersion: 1, packages: [] };
for (const pkg of PACKAGES) {
  const source = process.env[pkg.env] ?? join(addsDir, pkg.name);
  const stat = statSync(source, { throwIfNoEntry: false });
  if (!stat?.isDirectory()) {
    console.error(`package checkout not found: ${source} (set ${pkg.env})`);
    process.exit(1);
  }
  const commit = execFileSync("git", ["-C", source, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
  const home = join(vendorRoot, pkg.name);
  rmSync(home, { recursive: true, force: true });
  const files = {};
  for (const path of pkg.files.sort()) {
    const from = join(source, path);
    const to = join(home, path);
    mkdirSync(dirname(to), { recursive: true });
    copyFileSync(from, to);
    files[path] = sha256(to);
  }
  lock.packages.push({ name: pkg.name, repo: pkg.repo, commit, files });
}
writeFileSync(lockPath, JSON.stringify(lock, null, 2) + "\n");
console.log(JSON.stringify({ ok: true, gate: "php-packages-sync", packages: lock.packages.length }));
