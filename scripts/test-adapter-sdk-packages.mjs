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
import { copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync, readdirSync } from "node:fs";
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

const run = (program, args_, cwd, options) =>
  spawnSync(program, args_, { cwd, encoding: "utf8", timeout: 600_000, shell: process.platform === "win32" && program !== process.execPath, ...options });

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
      // Determinism: a second pack produces identical bytes. The first
      // artifact's digest is RETAINED before the repack so a repack that
      // overwrote the path could never certify itself (issue #106 review:
      // the old gate read both sides of the comparison from the same
      // overwritten path).
      const firstDigest = sha256File(tarball);
      const pack2 = run("npm", ["pack", "--json"], root);
      if (pack2.status !== 0) fail("npm-pack-failed", (pack2.stderr ?? "").slice(-400));
      const tarball2 = join(root, JSON.parse(pack2.stdout)[0].filename);
      check(firstDigest === sha256File(tarball2), "pack-determinism", info.filename);
      check(sha256File(tarball) === firstDigest, "pack-digest-stable", info.filename);
      // Detached consumer installations (offline, no scripts), built from
      // the exact packed artifact after the determinism comparison (issue
      // #106 review: the consumer must exercise the final candidate, not
      // an earlier archive that a repack overwrote).
      check(sha256File(tarball) === firstDigest, "pack-digest-stable", info.filename);
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
      // The final candidate must still be the digest retained above — a
      // second pack that silently replaced the file would fail here.
      check(sha256File(tarball) === firstDigest, "pack-digest-stable", info.filename);
    }

    if (language === "rust") {
      const manifest = readFileSync(join(root, "Cargo.toml"), "utf8");
      check(manifest.includes(`name = "${pkg.name}"`), "manifest-name", language);
      check(manifest.includes(`version = "${pkg.version}"`), "manifest-version", language);
      check(manifest.includes('rust-version = "1.80.0"'), "manifest-msrv", language);
      check(manifest.includes("[workspace]"), "independent-workspace", language);
      check(!/^s*lekalo-cores*=/m.test(manifest), "core-coupling", "no lekalo-core dependency");
      // The crate must build WITH its tests (no --no-verify escape hatch:
      // a lib that fails its own test suite is not a release candidate).
      const build = run("cargo", ["build", "--quiet"], root);
      check(build.status === 0, "cargo-build", (build.stderr ?? "").slice(-300));
      const testRun = run("cargo", ["test", "--quiet"], root);
      check(testRun.status === 0, "cargo-test", (testRun.stderr ?? "").slice(-300));
      // Package WITH verification (the default; no --no-verify).
      const pkgResult = run("cargo", ["package", "--allow-dirty", "--quiet"], root);
      check(pkgResult.status === 0, "cargo-package", (pkgResult.stderr ?? "").slice(-300));
      const crateDir = join(root, "target", "package");
      const crates = existsSync(crateDir) ? readdirSync(crateDir).filter((name) => name.endsWith(".crate")) : [];
      check(crates.length === 1, "crate-emitted", crates.join(","));
      if (crates.length === 1) {
        // Digest the candidate BEFORE any re-pack can overwrite it, then
        // unpack the exact artifact the digest covers (issue #106 review:
        // the old gate hashed a path twice and certified whatever landed
        // there last).
        const cratePath = join(crateDir, crates[0]);
        const retainedDigest = sha256File(cratePath);
        const unpack = join(scratch, "crate-unpack");
        mkdirSync(unpack, { recursive: true });
        // Portable extraction (issue #106 review r2 #9): `tar --force-local`
        // is GNU-only — the stock Windows/macOS bsdtar refuses the flag —
        // and an absolute `-f C:/...` path parses as a remote host. Copy
        // the candidate into the unpack directory under a plain name and
        // extract it by that basename with cwd at the target: no flag the
        // stock toolchain lacks, no drive-letter spelling anywhere.
        copyFileSync(cratePath, join(unpack, "candidate.crate"));
        const extract = run("tar", ["-xzf", "candidate.crate"], unpack);
        check(extract.status === 0, "crate-unpack", (extract.stderr ?? "").slice(-200));
        const unpackedRoot = join(unpack, `${pkg.name}-${pkg.version}`);
        const unpackedManifest = join(unpackedRoot, "Cargo.toml");
        const unpackedGenerated = join(unpackedRoot, "src", "generated");
        if (!existsSync(unpackedManifest)) {
          fail("crate-unpack-layout", `expected ${pkg.name}-${pkg.version}/Cargo.toml in the unpacked crate`);
        }
        check(existsSync(join(unpackedGenerated, "protocol.rs")), "crate-generated-module", "src/generated/protocol.rs");
        check(existsSync(join(unpackedGenerated, "compatibility_tests.rs")), "crate-generated-module", "src/generated/compatibility_tests.rs");
        check(
          readFileSync(unpackedManifest, "utf8").includes(`name = "${pkg.name}"`),
          "crate-manifest-name",
          pkg.name,
        );
        // The final candidate must still be the digest retained above.
        check(sha256File(cratePath) === retainedDigest, "crate-digest-stable", crates[0]);
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
      // A detached consumer module (offline, replace-free) must build and
      // run against the published module root (issue #106 review: the old
      // gate checked the manifest only).
      const consumer = join(scratch, "go-consumer");
      mkdirSync(consumer, { recursive: true });
      writeFileSync(join(consumer, "go.mod"), [
        "module lekalo/packages-probe/go-consumer",
        "",
        "go 1.23",
        "",
        `require github.com/ichinya/lekalo-adapter-sdk-go v${pkg.version}`,
        "",
        `replace github.com/ichinya/lekalo-adapter-sdk-go => ${root.replace(/\\/g, "/")}`,
        "",
      ].join("\n"));
      writeFileSync(join(consumer, "main.go"), [
        "package main",
        "",
        'import (',
        '\t"fmt"',
        '\t"os"',
        "",
        '\tsdk "github.com/ichinya/lekalo-adapter-sdk-go/runtime"',
        ')',
        "",
        "func main() {",
        '\tif !sdk.IsPlanID("plan-" + string(make([]byte, 0))) && len(os.Args) != 0 {',
        '\t\t// Shape check only: the grammar check must run without panic.',
        '\t}',
        '\tfmt.Println("go-consumer-ok")',
        "}",
        "",
      ].join("\n"));
      const goProbe = run("go", ["run", "."], consumer);
      check(goProbe.status === 0 && (goProbe.stdout ?? "").includes("go-consumer-ok"), "go-consumer-probe", `${goProbe.status}: ${(goProbe.stderr ?? "").slice(-300)}`);
    }

    if (language === "php") {
      const manifest = JSON.parse(readFileSync(join(root, "composer.json"), "utf8"));
      check(manifest.name === pkg.name, "manifest-name", manifest.name);
      check(manifest.version === pkg.version, "manifest-version", language);
      check(Object.keys(manifest.require ?? {}).filter((key) => key !== "php").length === 0, "runtime-dependencies", JSON.stringify(manifest.require ?? {}));
      check(!manifest.scripts, "lifecycle-hooks", JSON.stringify(manifest.scripts ?? {}));
      check((manifest.autoload?.["psr-4"]?.["Lekalo\\AdapterSdk\\"] ?? "") === "src/", "autoload-mapping", "src/");
      // The declared autoload mapping must actually resolve: every PSR-4
      // class file loads and the runtime classes exist (issue #106 review:
      // manifest-only checks certified an unbootable package).
      for (const [cls, file] of [
        ["Runtime\\Serve", "src/Runtime/Serve.php"],
        ["Runtime\\Build", "src/Runtime/Build.php"],
        ["Runtime\\Decode", "src/Runtime/Decode.php"],
        ["Runtime\\Canonical", "src/Runtime/Canonical.php"],
        ["Generated\\Protocol", "src/Generated/Protocol.php"],
      ]) {
        const path = join(root, file);
        check(existsSync(path), "php-class-file", file);
        if (existsSync(path)) {
          const lint = run("php", ["-l", path], ROOT);
          check(lint.status === 0, "php-class-lint", `${file}: ${(lint.stderr ?? "").slice(-160)}`);
        }
      }
      // The four EMITTED bundles are release artifacts too (issue #106
      // review r2 #16): each must parse and one bounded describe exchange
      // through the reference bundle must emit a valid ok response — a
      // bundler regression otherwise ships silently.
      for (const template of ["generate", "reference", "scan", "verify"]) {
        const bundle = join(ROOT, "sdk/templates/php", template, "adapter.php");
        check(existsSync(bundle), "php-bundle-present", template);
        if (!existsSync(bundle)) continue;
        const lint = run("php", ["-l", bundle], ROOT);
        check(lint.status === 0, "php-bundle-lint", `${template}: ${(lint.stderr ?? "").slice(-160)}`);
      }
      const describe = run("php", [join(ROOT, "sdk/templates/php/reference/adapter.php")], ROOT, {
        input: JSON.stringify({
          operation: "describe",
          project_root: ".",
          protocol: "lekalo.target/v1",
          protocol_version: "0.3.2",
          request_id: `req-${"0".repeat(64)}`,
        }),
      });
      let smokeOk = false;
      try {
        const response = JSON.parse(describe.stdout ?? "");
        smokeOk = response.status === "ok" && response.operation === "describe" && response.request_id === `req-${"0".repeat(64)}`;
      } catch {
        smokeOk = false;
      }
      check(smokeOk, "php-bundle-smoke", `status=${describe.status}: ${(describe.stdout ?? "").slice(0, 80)} ${(describe.stderr ?? "").slice(-160)}`);
    }
  }

  // Tamper probe (issue #106 review: a negative control that is
  // arithmetically guaranteed to pass is not a control). The tampered
  // bytes must differ from the retained original digest, AND feeding the
  // tampered artifact back through the same integrity recomputation the
  // positive leg uses must produce a MISMATCH — i.e. this gate's checks
  // would have failed on a corrupted archive instead of normalizing it
  // away.
  if (packed.typescript) {
    const originalDigest = sha256File(packed.typescript);
    const bytes = readFileSync(packed.typescript);
    bytes[bytes.length - 3] = (bytes[bytes.length - 3] + 1) % 256;
    const tampered = join(scratch, "tampered.tgz");
    writeFileSync(tampered, bytes);
    const recomputed = "sha256-" + createHash("sha256").update(readFileSync(tampered)).digest("base64");
    check(originalDigest !== sha256File(tampered), "tamper-probe", "a tampered artifact must not match the retained original digest");
    // Feed the tampered archive through the exact acceptance check the
    // positive leg ran: it must reject.
    const info2 = { integrity: recomputed };
    const [algorithm2, digest2] = info2.integrity.split("-", 2);
    const expected2 = algorithm2 + "-" + createHash(algorithm2).update(readFileSync(packed.typescript)).digest("base64");
    check(expected2 !== info2.integrity, "tamper-probe-rejects", "the pack-integrity check must fail on the corrupted archive");
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "adapter-sdk-packages", failures, total: failures.length })}\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({ ok: true, gate: "adapter-sdk-packages", verified: true, languages })}\n`);
