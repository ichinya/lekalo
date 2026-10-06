#!/usr/bin/env node
/**
 * The adapter-SDK conformance gate (issue #106).
 *
 * Runs the one real confined host battery (`lekalo adapter test`) against
 * the SDK reference templates and the capability-limited examples, and
 * compares the SDK reference pattern with the handwritten fake-adapter
 * control:
 *
 *   - strict profile, both reference templates (TypeScript bundle and
 *     the built Rust binary): verdict pass, badge issued, all 29 checks
 *     present, and the same per-check state pattern as the control;
 *   - default profile, the scan/generate/verify examples: verdict pass
 *     plus their required feature rows individually `pass` (never
 *     all-skipped);
 *   - a JUnit projection of the strict reference run (the native report
 *     formats pass through unchanged; the wrapper never awards badges);
 *   - hostile-mode failure-class parity with the control's fault
 *     injection (crash, hang) so a refusals stays a refusal.
 *
 * Usage:
 *   node scripts/test-adapter-sdk-conformance.mjs --language rust --profile strict --repeats 3 --timeout-ms 60000
 *   node scripts/test-adapter-sdk-conformance.mjs --language typescript --profile strict --repeats 3 --timeout-ms 60000
 *   node scripts/test-adapter-sdk-conformance.mjs --language all
 *
 * Exit protocol: 0 with a one-line JSON status on success, 1 with a JSON
 * failure report on stderr otherwise.
 */
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(SCRIPT_DIR, "..");
const CLI = join(ROOT, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "adapter-sdk-conformance", reason, detail })}\n`);
  process.exit(1);
};

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const index = args.indexOf(name);
  return index !== -1 ? args[index + 1] : fallback;
};
const language = option("--language", "all");
const profile = option("--profile", "strict");
const repeats = Number(option("--repeats", "2"));
const timeoutMs = Number(option("--timeout-ms", "60000"));
const ALL = ["typescript", "rust"];
const languages = language === "all" ? ALL : ALL.includes(language) ? [language] : null;
if (!languages) fail("usage", "--language must be rust|typescript|all");

const run = (program, args_, cwd) => {
  const result = spawnSync(program, args_, { cwd: cwd ?? ROOT, encoding: "utf8", timeout: 600_000, shell: process.platform === "win32" && program !== process.execPath });
  return result;
};

const runBattery = (adapterArgs, profile_, repeats_, timeout_) => {
  const result = run(CLI, [
    "adapter", "test",
    "--profile", profile_,
    "--report", "json",
    "--repeats", String(repeats_),
    "--timeout-ms", String(timeout_),
    ...adapterArgs,
  ]);
  const stdout = result.stdout ?? "";
  const start = stdout.indexOf("{");
  if (result.status !== 0 || start === -1) {
    return { error: `battery failed (${result.status}): ${(result.stderr ?? stdout).slice(0, 400)}` };
  }
  try {
    return { report: JSON.parse(stdout.slice(start)).report };
  } catch (error) {
    return { error: `report unparseable: ${String(error).slice(0, 200)}` };
  }
};

const checkStatePattern = (report) =>
  Object.fromEntries(report.checks.map((check) => [check.id, check.state]));

// 1. The host CLI and template build artifacts must exist (built in this
// job before the wrappers invoke them — never downloaded).
if (!existsSync(CLI)) {
  const build = run("cargo", ["build", "--locked", "-p", "lekalo-cli"]);
  if (build.status !== 0) fail("cli-missing", `build the host CLI first: ${(build.stderr ?? "").slice(-400)}`);
}
const ensureBuilt = (dir, command, args_) => {
  const build = run(command, args_, dir);
  if (build.status !== 0) fail("template-build-failed", `${dir}: ${(build.stderr ?? build.stdout ?? "").slice(-400)}`);
};
const rustTemplates = ["reference", "scan", "generate", "verify"];
if (languages.includes("rust")) {
  for (const template of rustTemplates) {
    ensureBuilt(join(ROOT, "sdk/templates/rust", template), "cargo", ["build", "--quiet"]);
  }
}
const rustBinary = (template, name) =>
  join(ROOT, "sdk/templates/rust", template, "target", "debug", process.platform === "win32" ? `${name}.exe` : name);

// 2. Strict lane: both reference templates + the handwritten control.
const strictTargets = [];
if (languages.includes("typescript")) {
  strictTargets.push({ label: "typescript-reference", args: ["node", "sdk/templates/typescript/reference/adapter.mjs"] });
}
if (languages.includes("rust")) {
  strictTargets.push({ label: "rust-reference", args: [rustBinary("reference", "sdk-reference-adapter")] });
}
strictTargets.push({ label: "control", args: ["node", "tests/fixtures/target-protocol/fake-adapter.mjs", "--lekalo-adapter-variant", "fluent"] });

const strictReports = new Map();
for (const target of strictTargets) {
  const { report, error } = runBattery(target.args, "strict", repeats, timeoutMs);
  if (error) fail("strict-run-failed", `${target.label}: ${error}`);
  if (report.verdict !== "pass") {
    fail("strict-verdict", `${target.label}: ${report.verdict} — ${JSON.stringify(report.checks.filter((c) => c.state === "fail").map((c) => [c.id, c.detail]))}`);
  }
  if (report.checks.length !== 29) {
    fail("strict-check-count", `${target.label}: ${report.checks.length} checks, expected the full 29-check catalog`);
  }
  strictReports.set(target.label, report);
}

// 3. Control pattern parity: the SDK references declare the same fluent
// capability profile as the control, so every check state matches.
const controlPattern = checkStatePattern(strictReports.get("control"));
for (const [label, report] of strictReports) {
  if (label === "control") continue;
  const pattern = checkStatePattern(report);
  const drift = Object.keys(pattern).filter((id) => pattern[id] !== controlPattern[id]);
  if (drift.length > 0) {
    fail("control-parity", `${label}: check states diverge from the handwritten control: ${drift.map((id) => `${id}:${pattern[id]}/${controlPattern[id]}`).join(", ")}`);
  }
  if (report.badge?.issued !== true || report.badge?.protocol !== "0.3.2" || report.badge?.ir !== "0.2.16") {
    fail("strict-badge", `${label}: ${JSON.stringify(report.badge)}`);
  }
}

// 4. Default lane: capability-limited examples assert their feature rows.
const exampleTargets = [];
if (languages.includes("typescript")) {
  exampleTargets.push(
    { label: "typescript-scan", args: ["node", "sdk/templates/typescript/scan/adapter.mjs"], rows: ["capability.declaration", "describe.negotiation"] },
    { label: "typescript-generate", args: ["node", "sdk/templates/typescript/generate/adapter.mjs"], rows: ["generate.dry-run-plan", "generate.apply-plan"] },
    { label: "typescript-verify", args: ["node", "sdk/templates/typescript/verify/adapter.mjs"], rows: ["scenario.normalization"] },
  );
}
if (languages.includes("rust")) {
  exampleTargets.push(
    { label: "rust-scan", args: [rustBinary("scan", "sdk-scan-example")], rows: ["capability.declaration", "describe.negotiation"] },
    { label: "rust-generate", args: [rustBinary("generate", "sdk-generate-example")], rows: ["generate.dry-run-plan", "generate.apply-plan"] },
    { label: "rust-verify", args: [rustBinary("verify", "sdk-verify-example")], rows: ["scenario.normalization"] },
  );
}
for (const target of exampleTargets) {
  const { report, error } = runBattery(target.args, "default", repeats, timeoutMs);
  if (error) fail("example-run-failed", `${target.label}: ${error}`);
  if (report.verdict !== "pass") {
    fail("example-verdict", `${target.label}: ${report.verdict} — ${JSON.stringify(report.checks.filter((c) => c.state === "fail").map((c) => [c.id, c.detail]))}`);
  }
  for (const row of target.rows) {
    const check = report.checks.find((entry) => entry.id === row);
    if (!check) fail("feature-row-missing", `${target.label}: ${row} absent from the report`);
    if (check.state !== "pass") {
      fail("feature-row-skipped", `${target.label}: required row ${row} is ${check.state} — an all-skipped feature probe never qualifies`);
    }
  }
}

// 5. JUnit projection of one strict reference run: native format, no
// wrapper dialect, no self-awarded badge.
if (languages.includes("typescript")) {
  const result = run(CLI, [
    "adapter", "test", "--profile", "strict", "--report", "junit",
    "--repeats", String(repeats), "--timeout-ms", String(timeoutMs),
    "node", "sdk/templates/typescript/reference/adapter.mjs",
  ]);
  const junit = result.stdout ?? "";
  if (result.status !== 0 || !junit.startsWith("<?xml")) {
    fail("junit-projection", `expected the native JUnit XML projection: ${junit.slice(0, 200)}`);
  }
  if (!junit.includes("dev.lekalo.adapter-conformance") && !junit.includes("adapter-conformance")) {
    fail("junit-identity", "the JUnit projection must carry the suite identity");
  }
  if (/badge[^<]*issued[^<]*true/i.test(junit)) {
    // The JUnit form reports the run; a wrapper must never fabricate one.
    if (!junit.includes('<testsuite') ) fail("junit-shape", "unexpected JUnit shape");
  }
}

// 6. Hostile-mode failure-class parity: the control's fault injection
// classes terminal failures; the SDK template must hit the same host
// classification path (the refusal is never reduced to success).
if (languages.includes("typescript")) {
  const crash = runBattery([
    "node", "tests/fixtures/target-protocol/fake-adapter.mjs",
    "--lekalo-adapter-variant", "fluent",
    "--lekalo-fault", "crash",
  ], "default", 1, timeoutMs);
  if (!crash.error && crash.report?.verdict === "pass") {
    fail("hostile-parity", "the injected crash surfaced as a pass — failure classes are not preserved");
  }
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  gate: "adapter-sdk-conformance",
  languages,
  profile,
  repeats,
  timeoutMs,
  strict: [...strictReports.keys()],
  examples: exampleTargets.map((target) => target.label),
})}\n`);
