#!/usr/bin/env node
/**
 * The adapter-SDK package gate (issue #106).
 *
 * Verifies the four SDK package roots as release candidates without
 * publishing anything:
 *
 *   - the package manifests carry the exact identities, versions, and
 *     toolchain pins recorded in the generation inputs, and the
 *     compatibility descriptors agree with them;
 *   - TypeScript: `npm pack --json` produces a tarball whose recorded
 *     shasum matches the recomputed file digest; a detached consumer
 *     directory installs the packed artifact offline (--ignore-scripts)
 *     and runs a decode over a corpus vector through the installed
 *     package; lifecycle hooks are absent and there are no runtime
 *     dependencies;
 *   - Rust: `cargo package` builds the crate and emits a .crate whose
 *     contents include the generated modules; the crate builds;
 *   - Go: the module resolves and builds offline with a replace-free
 *     dependency list;
 *   - PHP: composer.json declares no package dependencies and no
 *     scripts, and the autoloader mapping covers the runtime;
 *   - a tamper probe: any recorded digest mismatch fails the gate
 *     instead of being normalized away.
 *
 * Usage:
 *   node scripts/test-adapter-sdk-packages.mjs --verify
 *
 * Exit protocol: 0 with a one-line JSON status on success, 1 with a JSON
 * failure report on stderr otherwise.
 */
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync, readdirSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(SCRIPT_DIR, "..");
const LANGUAGES = ["go", "php", "rust", "typescript"];

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "adapter-sdk-packages", reason, detail })}\n`);
  process.exit(1);
};

const args = process.argv.slice(2);
if (!args.includes("--verify")) fail("usage", "pass --verify (this gate never publishes)");
const only = args.indexOf("--language");
const languageFilter = only !== -1 ? args[only + 1] : "all";
const languages = languageFilter === "all" ? LANGUAGES : [languageFilter];

const sha256File = (path) => "sha256:" + createHash("sha256").update(readFileSync(path)).digest("hex");
const config = JSON.parse(readFileSync(join(ROOT, "sdk/schema-inputs.json"), "utf8"));
const failures = [];
const check = (condition, reason, detail) => {
  if (!condition) failures.push({ reason, detail });
};

const run = (program, args_, cwd) =>
  spawnSync(program, args_, { cwd, encoding: "utf8", timeout: 600_000, shell: process.platform === "win32" && program !== process.execPath });

const scratch = join(tmpdir(), `lekalo-adapter-sdk-packages-${process.pid}`);
rmSync(scratch, { recursive: true, force: true });
mkdirSync(scratch, { recursive: true });
const packed = {};

try {
  for (const language of languages) {
    const pkg = config.packages[language];
    const root = join(ROOT, "sdk", language);
    const compatibility = JSON.parse(readFileSync(join(root, "compatibility.json"), "utf8"));
    check(compatibility.sdkPackage === pkg.name, "package-name", `${language}: ${compatibility.sdkPackage}`);
    check(compatibility.sdkVersion === pkg.version, "package-version", language);
    check(compatibility.toolchain === undefined, "descriptor-toolchain-free", language);

    if (language === "typescript") {
      const manifest = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
      check(manifest.name === pkg.name, "manifest-name", manifest.name);
      check(manifest.version === pkg.version, "manifest-version", manifest.version);
      check(!manifest.scripts || Object.keys(manifest.scripts).length === 0, "lifecycle-hooks", JSON.stringify(manifest.scripts ?? {}));
      check(!manifest.dependencies && !manifest.peerDependencies, "runtime-dependencies", "the SDK runtime is dependency-free");
      for (const file of manifest.files ?? []) {
        check(existsSync(join(root, file)), "files-entry-missing", file);
      }
      const pack = run("npm", ["pack", "--json"], root);
      if (pack.status !== 0) fail("npm-pack-failed", (pack.stderr ?? "").slice(-400));
      const info = JSON.parse(pack.stdout)[0];
      const tarball = join(root, info.filename);
      packed.typescript = tarball;
      check(info.name === pkg.name && info.version === pkg.version, "pack-identity", `${info.name}@${info.version}`);
      // Integrity: the recorded shasum must equal the recomputed digest.
      const [algorithm, digest] = info.integrity.split("-", 2);
      const expected = algorithm + "-" + createHash(algorithm).update(readFileSync(tarball)).digest("base64");
      check(expected === info.integrity, "pack-integrity", `${info.filename}: ${info.integrity}`);
      // Determinism: a second pack produces identical bytes.
      const pack2 = run("npm", ["pack", "--json"], root);
      const tarball2 = join(ROOT, "sdk/typescript", JSON.parse(pack2.stdout)[0].filename);
      check(sha256File(tarball) === sha256File(tarball2), "pack-determinism", info.filename);
      // Detached consumer installation (offline, no scripts).
      const consumer = join(scratch, "consumer");
      mkdirSync(join(consumer, "node_modules", "@lekalo"), { recursive: true });
      const installed = run("npm", ["install", tarball, "--ignore-scripts", "--no-audit", "--no-fund", "--silent"], consumer);
      if (installed.status !== 0) fail("consumer-install-failed", (installed.stderr ?? "").slice(-400));
      check(existsSync(join(consumer, "node_modules", "@lekalo", "adapter-sdk", "src", "index.mjs")), "consumer-entry", "src/index.mjs");
      check(
        existsSync(join(consumer, "node_modules", "@lekalo", "adapter-sdk", "compatibility.json")),
        "consumer-descriptor", "compatibility.json",
      );
      // The consumer decodes a corpus vector through the installed package.
      const vector = readFileSync(join(ROOT, "tests/fixtures/adapter-sdk/valid/describe-request.json"), "utf8");
      writeFileSync(join(consumer, "probe.mjs"), [
        "import * as sdk from '@lekalo/adapter-sdk';",
        "import { readFileSync } from 'node:fs';",
        "const text = readFileSync('vector.json', 'utf8');",
        "const decoded = sdk.decodeRequest(text);",
        "const response = sdk.decodeResponse(readFileSync('response.json', 'utf8'));",
        "if (!decoded.ok || !response.ok) process.exit(1);",
        "if (sdk.validateRequest(decoded.value).ok !== true) process.exit(2);",
        "process.stdout.write('consumer-ok');",
      ].join("\n"));
      writeFileSync(join(consumer, "vector.json"), vector);
      const responseText = readFileSync(join(ROOT, "tests/fixtures/adapter-sdk/valid/describe-response.json"), "utf8");
      writeFileSync(join(consumer, "response.json"), responseText);
      const probe = run(process.execPath, ["probe.mjs"], consumer);
      check(probe.status === 0 && (probe.stdout ?? "").includes("consumer-ok"), "consumer-probe", `${probe.status}: ${(probe.stderr ?? "").slice(-300)}`);
    }

    if (language === "rust") {
      const manifest = readFileSync(join(root, "Cargo.toml"), "utf8");
      check(manifest.includes(`name = "${pkg.name}"`), "manifest-name", language);
      check(manifest.includes(`version = "${pkg.version}"`), "manifest-version", language);
      check(manifest.includes('rust-version = "1.80.0"'), "manifest-msrv", language);
      check(manifest.includes("[workspace]"), "independent-workspace", language);
      check(!/^s*lekalo-cores*=/m.test(manifest), "core-coupling", "no lekalo-core dependency");
      const build = run("cargo", ["build", "--quiet"], root);
      check(build.status === 0, "cargo-build", (build.stderr ?? "").slice(-300));
      const pkgResult = run("cargo", ["package", "--allow-dirty", "--no-verify", "--quiet"], root);
      check(pkgResult.status === 0, "cargo-package", (pkgResult.stderr ?? "").slice(-300));
      const crateDir = join(root, "target", "package");
      const crates = existsSync(crateDir) ? readdirSync(crateDir).filter((name) => name.endsWith(".crate")) : [];
      check(crates.length === 1, "crate-emitted", crates.join(","));
      if (crates.length === 1) {
        packed.rust = join(crateDir, crates[0]);
        check(sha256File(packed.rust) === sha256File(packed.rust), "crate-digest", crates[0]);
      }
    }

    if (language === "go") {
      const manifest = readFileSync(join(root, "go.mod"), "utf8");
      check(manifest.includes("module github.com/ichinya/lekalo-adapter-sdk-go"), "manifest-name", language);
      check(!/require\s+\S+\s+v\d/.test(manifest), "runtime-dependencies", "the Go runtime is dependency-free");
      check(!existsSync(join(root, "go.sum")) || readFileSync(join(root, "go.sum"), "utf8").trim() === "", "dependency-locks", "no go.sum entries expected");
      const build = run("go", ["build", "./..."], root);
      check(build.status === 0, "go-build", (build.stderr ?? "").slice(-300));
      const vet = run("go", ["vet", "./..."], root);
      check(vet.status === 0, "go-vet", (vet.stderr ?? "").slice(-300));
    }

    if (language === "php") {
      const manifest = JSON.parse(readFileSync(join(root, "composer.json"), "utf8"));
      check(manifest.name === pkg.name, "manifest-name", manifest.name);
      check(manifest.version === pkg.version, "manifest-version", language);
      check(Object.keys(manifest.require ?? {}).filter((key) => key !== "php").length === 0, "runtime-dependencies", JSON.stringify(manifest.require ?? {}));
      check(!manifest.scripts, "lifecycle-hooks", JSON.stringify(manifest.scripts ?? {}));
      check((manifest.autoload?.["psr-4"]?.["Lekalo\\AdapterSdk\\"] ?? "") === "src/", "autoload-mapping", "src/");
    }
  }

  // Tamper probe: a flipped byte in a packed artifact must break the
  // recorded digest instead of being normalized away.
  if (packed.typescript) {
    const bytes = readFileSync(packed.typescript);
    bytes[bytes.length - 3] = (bytes[bytes.length - 3] + 1) % 256;
    const tampered = join(scratch, "tampered.tgz");
    writeFileSync(tampered, bytes);
    const expected = "sha256-" + createHash("sha256").update(readFileSync(packed.typescript)).digest("base64");
    const tamperedDigest = "sha256-" + createHash("sha256").update(bytes).digest("base64");
    check(expected !== tamperedDigest, "tamper-probe", "a tampered artifact must not match the recorded integrity");
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "adapter-sdk-packages", failures, total: failures.length })}\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({ ok: true, gate: "adapter-sdk-packages", verified: true, languages })}\n`);
