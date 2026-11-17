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
import { lstatSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const adapterRoot = join(root, "adapters", "php-laravel");
const vendorRoot = join(adapterRoot, "vendor");
const lockPath = join(adapterRoot, "php-packages.lock.json");
const args = process.argv.slice(2);
if (args.length > 1 || (args.length === 1 && !["--sync", "--check"].includes(args[0]))) {
  throw new Error("usage: sync-php-packages.mjs [--sync|--check]");
}
const check = args[0] === "--check";

// Manifests own implementation inputs; these extra files provide package-local
// build and validation tools. Core does not maintain a second module list.
const PACKAGES = [
  {
    name: "lekalo-php", kind: "runtime",
    repo: "https://github.com/ichinya/lekalo-php", env: "LEKALO_PHP_DIR",
    extras: ["build-lib.php", "build.php", "tests/protocol.php", "tests/analyzer.php", "tests/composition.php"],
  },
  {
    name: "lekalo-php-laravel", kind: "rules",
    repo: "https://github.com/ichinya/lekalo-php-laravel", env: "LEKALO_PHP_LARAVEL_DIR",
    extras: ["tests/laravel.php"],
  },
];

const safePath = (path) => typeof path === "string" && path.length <= 256 &&
  path.split("/").every((part) => /^[a-zA-Z0-9_.-]+$/.test(part) && part !== "." && part !== "..");
const digest = (bytes) => "sha256:" + createHash("sha256").update(bytes).digest("hex");
const sha256 = (path) => digest(readFileSync(path));

function verifySnapshot() {
  const lock = JSON.parse(readFileSync(lockPath, "utf8"));
  let ok = true;
  const seen = [];
  for (const pkg of lock.packages) {
    if (!PACKAGES.some((item) => item.name === pkg.name) || !/^[a-f0-9]{40}$/.test(pkg.commit)) {
      throw new Error("invalid PHP package pin");
    }
    const home = join(vendorRoot, pkg.name);

    for (const [path, digest] of Object.entries(pkg.files)) {
      if (!safePath(path)) throw new Error("invalid PHP package file path");
      seen.push(`${pkg.name}/${path}`);
      const vendored = join(home, path);
      try {
        let current = vendorRoot;
        for (const part of [pkg.name, ...path.split("/")]) {
          current = join(current, part);
          if (lstatSync(current).isSymbolicLink()) throw new Error("vendor symlink");
        }
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
  if (!ok) throw new Error("existing PHP snapshot has local changes; refusing overwrite");
}

verifySnapshot();
if (check) process.exit(0);

// Stage all committed inputs in memory before touching any snapshot.
const addsDir = process.env.LEKALO_ADDS_DIR ?? join(dirname(root), "lekalo-adds");
const lock = { formatVersion: 1, packages: [] };
const pending = [];
for (const pkg of PACKAGES) {
  const source = process.env[pkg.env] ?? join(addsDir, pkg.name);
  const stat = statSync(source, { throwIfNoEntry: false });
  if (!stat?.isDirectory()) {
    console.error(`package checkout not found: ${source} (set ${pkg.env})`);
    process.exit(1);
  }
  const commit = execFileSync("git", ["-C", source, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
  const blob = (path) => {
    if (!safePath(path)) throw new Error(`invalid package path: ${path}`);
    const entry = execFileSync("git", ["-C", source, "ls-tree", commit, "--", path], { encoding: "utf8" });
    if (!/^100(?:644|755) blob /.test(entry)) throw new Error(`package input is not a committed regular file: ${path}`);
    return execFileSync("git", ["-C", source, "show", `${commit}:${path}`], { maxBuffer: 4 * 1024 * 1024 });
  };
  const manifestBytes = blob("lekalo.package.json");
  const manifest = JSON.parse(manifestBytes);
  if (manifest.schema !== "lekalo/php-package/v1" || manifest.id !== pkg.name || manifest.kind !== pkg.kind
      || !Array.isArray(manifest.modules) || !manifest.resources || typeof manifest.resources !== "object") {
    throw new Error(`invalid package manifest: ${pkg.name}`);
  }
  const paths = [...new Set(["lekalo.package.json", ...manifest.modules, ...Object.values(manifest.resources), ...pkg.extras])].sort();
  const files = {}; const contents = new Map();
  for (const path of paths) {
    const bytes = path === "lekalo.package.json" ? manifestBytes : blob(path);
    contents.set(path, bytes);
    files[path] = digest(bytes);
  }
  pending.push({ name: pkg.name, contents });
  lock.packages.push({ name: pkg.name, repo: pkg.repo, commit, files });
}
for (const pkg of pending) {
  const home = join(vendorRoot, pkg.name);
  rmSync(home, { recursive: true, force: true });
  for (const [path, bytes] of pkg.contents) {
    const to = join(home, path);
    mkdirSync(dirname(to), { recursive: true });
    writeFileSync(to, bytes);
  }
}
writeFileSync(lockPath, JSON.stringify(lock, null, 2) + "\n");
console.log(JSON.stringify({ ok: true, gate: "php-packages-sync", packages: lock.packages.length }));
