import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

test("PHP snapshots bind committed blobs, refuse local drift and preflight every package", () => {
  const temp = mkdtempSync(join(process.env.BB_THREAD_STORAGE ?? tmpdir(), "php-pin-test-"));
  try {
    const core = join(temp, "core");
    const adapter = join(core, "adapters/php-laravel");
    mkdirSync(join(core, "scripts"), { recursive: true });
    cpSync(new URL("sync-php-packages.mjs", import.meta.url), join(core, "scripts/sync-php-packages.mjs"));
    const specs = [
      ["lekalo-php", "runtime", ["build.php", "build-lib.php", "tests/protocol.php", "tests/analyzer.php", "tests/composition.php"]],
      ["lekalo-php-laravel", "rules", ["tests/laravel.php"]],
    ];
    const git = (path, ...args) => execFileSync("git", ["-C", path, ...args], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
    const commit = (path) => {
      git(path, "add", ".");
      git(path, "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "fixture");
      return git(path, "rev-parse", "HEAD");
    };
    const lock = { formatVersion: 1, packages: [] };
    const sources = [];
    for (const [name, kind, extras] of specs) {
      const source = join(temp, name); sources.push(source);
      mkdirSync(source); git(source, "init", "-q");
      const files = ["src/custom.php", "resource.txt", ...extras];
      for (const path of files) {
        const full = join(source, path);
        mkdirSync(join(full, ".."), { recursive: true });
        writeFileSync(full, "fixture\n");
      }
      writeFileSync(join(source, "lekalo.package.json"), JSON.stringify({
        schema: "lekalo/php-package/v1", id: name, kind, version: "1.0.0",
        provider_api: "test", modules: ["src/custom.php"], resources: { CUSTOM: "resource.txt" },
      }));
      const sha = commit(source);
      const home = join(adapter, "vendor", name, "src"); mkdirSync(home, { recursive: true });
      writeFileSync(join(home, "old.php"), "old\n");
      lock.packages.push({ name, repo: "fixture", commit: sha,
        files: { "src/old.php": "sha256:" + createHash("sha256").update("old\n").digest("hex") } });
    }
    const lockPath = join(adapter, "php-packages.lock.json");
    writeFileSync(lockPath, JSON.stringify(lock));
    const run = (mode, env = {}) => spawnSync(process.execPath, [join(core, "scripts/sync-php-packages.mjs"), mode], {
      encoding: "utf8", env: { ...process.env, LEKALO_PHP_DIR: sources[0], LEKALO_PHP_LARAVEL_DIR: sources[1], ...env },
    });
    // Dirty worktree content must never masquerade as HEAD in a source receipt.
    writeFileSync(join(sources[0], "src/custom.php"), "uncommitted\n");
    let result = run("--sync"); assert.equal(result.status, 0, result.stderr);
    const vendored = join(adapter, "vendor/lekalo-php/src/custom.php");
    assert.equal(readFileSync(vendored, "utf8"), "fixture\n");
    assert.equal(JSON.parse(readFileSync(lockPath)).packages[0].commit, git(sources[0], "rev-parse", "HEAD"));
    assert.equal(run("--check", { LEKALO_PHP_DIR: "/missing", LEKALO_PHP_LARAVEL_DIR: "/missing" }).status, 0);
    const previousLock = readFileSync(lockPath, "utf8");
    writeFileSync(vendored, "local edit\n");
    assert.notEqual(run("--sync").status, 0);
    assert.equal(readFileSync(vendored, "utf8"), "local edit\n");
    assert.equal(readFileSync(lockPath, "utf8"), previousLock);
    writeFileSync(vendored, "fixture\n");
    // Package one has a new commit, package two has an invalid input. Neither
    // snapshot may change when preflight cannot complete the whole update.
    commit(sources[0]);
    rmSync(join(sources[1], "src/custom.php")); commit(sources[1]);
    assert.notEqual(run("--sync").status, 0);
    assert.equal(readFileSync(vendored, "utf8"), "fixture\n");
    assert.equal(readFileSync(lockPath, "utf8"), previousLock);
    const extra = join(adapter, "vendor/lekalo-php/unpinned.php");
    writeFileSync(extra, "unexpected\n");
    assert.notEqual(run("--check").status, 0);
  } finally { rmSync(temp, { recursive: true, force: true }); }
});
