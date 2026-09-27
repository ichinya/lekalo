// Issue #54: the PHP/Laravel adapter gate. Runs the kernel's PHP
// protocol and process suites and verifies the committed artifact's
// packaging determinism, then — when a PHP interpreter is available on
// PATH — runs the production conformance battery against the shipped
// artifact exactly like CI does (`lekalo adapter test`). Dependency-free;
// run from the repo root.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const adapterRoot = join(root, "adapters", "php-laravel");
const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

/** Locate a PHP interpreter without any provisioning or install. */
function findPhp() {
  const candidates = (process.env.LEKALO_PHP ?? "php").split(";").filter(Boolean);
  for (const candidate of candidates) {
    const probe = spawnSync(candidate, ["-v"], { encoding: "utf8" });
    if (probe.status === 0) return candidate;
  }
  return null;
}

function runPhp(php, script) {
  const result = spawnSync(php, ["-n", script], {
    cwd: adapterRoot,
    encoding: "utf8",
    timeout: 120_000,
  });
  return {
    code: result.status,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
  };
}

// 1. The kernel's PHP suites.
const php = findPhp();
if (php === null) {
  fail("php-missing", "no PHP interpreter found; install PHP >= 8.3 or set LEKALO_PHP");
}

for (const suite of ["tests/protocol.php", "tests/process.php"]) {
  const script = join(adapterRoot, suite);
  const outcome = runPhp(php, script);
  if (outcome.code !== 0) {
    fail("php-suite-failed", `${suite}: ${outcome.stderr.slice(0, 400)}`);
  }
  let summary;
  try {
    summary = JSON.parse(outcome.stdout.trim().split("\n").pop());
  } catch {
    fail("php-suite-unparseable", suite);
  }
  if (summary.failures !== 0) {
    fail("php-suite-failures", `${suite}: ${JSON.stringify(summary)}`);
  }
  process.stdout.write(`${JSON.stringify({ ok: true, suite, ...summary })}\n`);
}

// 2. Packaging: the committed artifact is exactly the deterministic
// rebuild output, with a matching reported digest.
const buildCheck = spawnSync(php, ["-n", join(adapterRoot, "build.php"), "--check"], {
  cwd: adapterRoot,
  encoding: "utf8",
});
if (buildCheck.status !== 0) {
  fail("build-check", buildCheck.stderr ?? buildCheck.stdout);
}
const checkReport = JSON.parse(buildCheck.stdout.trim());
const artifactBytes = readFileSync(join(adapterRoot, "adapter.php"));
if (createHash("sha256").update(artifactBytes).digest("hex") !== checkReport.digest.slice(7)) {
  fail("artifact-digest", "committed bytes do not match the reported build digest");
}
process.stdout.write(`${JSON.stringify({ ok: true, gate: "packaging", digest: checkReport.digest })}\n`);

// 3. The production conformance battery, when the confined runtime can
// run it. The strict profile proves the complete v1 operation surface
// on the Linux CI leg (the packaged PHP build is self-contained under
// the ro-bound /usr); on macOS/Windows the sandbox's interpreter copy
// cannot load the platform PHP build (seatbelt dyld refusal /
// STATUS_DLL_NOT_FOUND), so the leg is skipped with an explicit reason
// — visible in the step log, never a silent pass. The workflow keeps
// this step enabled on every OS so the skip reason stays visible.
const cli = ["run", "--locked", "-p", "lekalo-cli", "--", "adapter", "test",
  "--profile", "strict", "--report", "json", "--timeout-ms", "30000", "--",
  php, join(adapterRoot, "adapter.php")];
if (process.env.LEKALO_SKIP_CONFORMANCE !== "1") {
  const cargo = process.env.CARGO ?? "cargo";
  const conformance = spawnSync(cargo, cli, { cwd: root, encoding: "utf8", timeout: 600_000 });
  const report = (() => {
    try {
      return JSON.parse(conformance.stdout);
    } catch {
      return undefined;
    }
  })();
  if (report?.status === "unavailable" && report.report?.verdict === "process") {
    // The confined exchange died before an envelope: the platform's
    // PHP build cannot run under confinement. Report the skip loudly.
    process.stdout.write(`${JSON.stringify({
      ok: true,
      gate: "conformance",
      skipped: true,
      reason: "confined-php-runtime-unavailable-on-this-platform",
      detail: "the sandbox copies the interpreter plus the script; this platform's PHP build needs runtime siblings the copy cannot include",
    })}\n`);
  } else if (conformance.status !== 0) {
    fail("conformance-failed", `exit=${conformance.status} ${String(conformance.stderr).slice(0, 400)}`);
  } else if (report === undefined) {
    fail("conformance-unparseable", String(conformance.stdout).slice(0, 200));
  } else if (report.status !== "valid" || report.report?.verdict !== "pass") {
    fail("conformance-verdict", JSON.stringify(report.status));
  } else if (report.report.badge?.issued !== true) {
    fail("badge-missing", JSON.stringify(report.report.badge));
  } else {
    process.stdout.write(`${JSON.stringify({
      ok: true,
      gate: "conformance",
      profile: "strict",
      verdict: report.report.verdict,
      badge: report.report.badge,
    })}\n`);
  }
}
// 4. The adapter manifest golden, when the shared gate has run (the
// manifest covers the shipped bytes; the dedicated gate script checks
// both adapters together).
const manifestPath = join(adapterRoot, "adapter.manifest.json");
if (existsSync(manifestPath)) {
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  const entry = manifest.integrity?.files?.find((file) => file.path === "adapter.php");
  if (!entry) fail("manifest-entry", "adapter.php is not in integrity.files");
  const digest = "sha256:" + createHash("sha256").update(artifactBytes).digest("hex");
  if (entry.digest !== digest) fail("manifest-digest", `manifest=${entry.digest} actual=${digest}`);
  if (entry.bytes !== artifactBytes.length) fail("manifest-length", `${entry.bytes} vs ${artifactBytes.length}`);
  process.stdout.write(`${JSON.stringify({ ok: true, gate: "manifest-golden", adapter: manifest.adapter.id })}\n`);
} else {
  process.stdout.write(`${JSON.stringify({ ok: true, gate: "manifest-golden", skipped: "manifest-not-yet-committed" })}\n`);
}

process.stdout.write(`${JSON.stringify({ ok: true, gate: "php-laravel-adapter" })}\n`);
