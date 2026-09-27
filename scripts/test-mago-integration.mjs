// Issue #55: the Mago integration gate for the PHP/Laravel adapter.
//
// Two independent modes:
//
//   node scripts/test-mago-integration.mjs --fake
//     The REQUIRED fake suite. Runs everywhere PHP runs, needs no Mago
//     binary and no network: exercises the receipt decoder, the four
//     analysis states, the strict-profile mapping over the recorded
//     fixtures, the bounded wire projection, and the kernel dispatch
//     gates — through the committed adapter artifact exactly like core
//     drives it (the shipped `adapter.php` bundle is what gets tested).
//
//   node scripts/test-mago-integration.mjs --real [--require-available]
//     The REAL suite. Requires a Mago executable (LEKALO_MAGO or
//     `mago` on PATH) at the pinned toolchain version. Probes
//     `--version`, runs lint/analyze/guard JSON over the recorded
//     fixtures, checks JSON/SARIF parity, verifies the dry-run fix
//     preview never modifies source, and asserts the exit-code
//     semantics recorded in the toolchain lock. `--require-available`
//     turns an unavailable Mago into a hard failure instead of a
//     visible skip; a platform skip never counts as acceptance.
//
// Exit code 0 = every executed check passed.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const adapterRoot = join(root, "adapters", "php-laravel");
const fixtureRoot = join(root, "tests", "fixtures", "mago", "toolchain");
const lock = JSON.parse(readFileSync(join(adapterRoot, "mago-toolchain.lock.json"), "utf8"));
const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};
const step = (name, extra = {}) =>
  process.stdout.write(`${JSON.stringify({ ok: true, step: name, ...extra })}\n`);

const mode = process.argv.includes("--real") ? "real" : "fake";

/** Locate a PHP interpreter without any provisioning or install. */
function findPhp() {
  const candidates = (process.env.LEKALO_PHP ?? "php").split(";").filter(Boolean);
  for (const candidate of candidates) {
    const probe = spawnSync(candidate, ["-v"], { encoding: "utf8" });
    if (probe.status === 0) return candidate;
  }
  return null;
}

/** One bounded request/response exchange against the committed artifact. */
function exchange(php, request, cwd = tmpdir(), extraArgs = []) {
  const result = spawnSync(php, [join(adapterRoot, "adapter.php"), ...extraArgs], {
    cwd,
    encoding: "utf8",
    timeout: 60_000,
    input: JSON.stringify(request),
  });
  if (result.status !== 0) {
    fail("exchange-crashed", `request ${request.operation}: ${String(result.stderr).slice(0, 300)}`);
  }
  try {
    return JSON.parse(result.stdout);
  } catch {
    fail("exchange-unparseable", `request ${request.operation}`);
  }
}

const requestId = (n) => `req-${"0".repeat(64 - String(n).length)}${n}`;
let counter = 0;
const request = (operation, extra = {}) => {
  counter += 1;
  return {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    operation,
    request_id: requestId(counter),
    project_root: ".",
    ...extra,
  };
};

// ---------------------------------------------------------------------------
// Shared fake suite: the artifact's gates through the production wire.
// ---------------------------------------------------------------------------

function runFakeSuite(php) {
  // 1. Describe: the honest no-evidence composition, in-envelope.
  const described = exchange(php, request("describe"));
  if (described.capabilities.adapter.id !== "lekalo-target-php-laravel") {
    fail("describe-identity", described.capabilities.adapter.id);
  }
  if (described.capabilities.capabilities["verify.scenarios"] !== "unsupported") {
    fail("scenarios-must-stay-unsupported", "Mago success never satisfies scenario verification");
  }
  step("describe", { adapter: described.capabilities.adapter.version });

  // 2. Validate with no receipt: no claims either way.
  const validateRequest = request("validate", { ir_path: ".lekalo/ir/planner.json" });
  const unavailable = exchange(php, validateRequest);
  if (unavailable.status !== "ok" || unavailable.result.findings.length !== 0) {
    fail("unavailable-validate", JSON.stringify(unavailable).slice(0, 300));
  }
  step("validate-unavailable", { findings: 0 });

  // 3. The fake gate drives the same gates with staged evidence by
  //    running the kernel suites (which inject fakes through the same
  //    decoder) — the unit suites cover ok/incompatible/failed.
  const suites = ["tests/protocol.php", "tests/process.php", "tests/analyzer.php"];
  for (const suite of suites) {
    const outcome = spawnSync(php, ["-n", join(adapterRoot, suite)], {
      cwd: adapterRoot,
      encoding: "utf8",
      timeout: 120_000,
    });
    if (outcome.status !== 0) {
      fail("php-suite-failed", `${suite}: ${String(outcome.stderr).slice(0, 300)}`);
    }
    const summary = JSON.parse(outcome.stdout.trim().split("\n").pop());
    if (summary.failures !== 0) fail("php-suite-failures", `${suite}: ${JSON.stringify(summary)}`);
    step("php-suite", { suite, checks: summary.checks });
  }

  // 4. Packaging: the artifact embeds the toolchain lock identity
  //    (upgrade custody), and the committed bytes match the
  //    deterministic rebuild.
  const buildCheck = spawnSync(php, ["-n", join(adapterRoot, "build.php"), "--check"], {
    cwd: adapterRoot,
    encoding: "utf8",
  });
  if (buildCheck.status !== 0) fail("build-check", buildCheck.stderr ?? buildCheck.stdout);
  const artifactBytes = readFileSync(join(adapterRoot, "adapter.php"));
  const artifactText = artifactBytes.toString("utf8");
  const pinnedVersion = lock.tool.version;
  if (!artifactText.includes(`const MAGO_PINNED_TOOL_VERSION = '${pinnedVersion}';`)) {
    fail("lock-not-bundled", `tool version ${pinnedVersion} missing from the artifact`);
  }
  step("packaging", { digest: "sha256:" + createHash("sha256").update(artifactBytes).digest("hex") });

  // 5. The recorded fixtures are present and pinned to the same version.
  for (const recording of [
    "recording.lint.json", "recording.analyze.json", "recording.guard.json",
    "recording.sarif.json", "recording.fix-preview.json", "recording.rules.json",
  ]) {
    const doc = JSON.parse(readFileSync(join(fixtureRoot, recording), "utf8"));
    if (doc.version !== lock.tool.version) {
      fail("fixture-pin-mismatch", `${recording}: ${doc.version} vs ${lock.tool.version}`);
    }
  }
  step("recordings", { fixtureRoot: "tests/fixtures/mago/toolchain" });

  // 6. No apply surface exists: no request field or operation can turn
  //    advice into writes. Unknown members are refused on request
  //    decode — the closed request validator refuses `fix`/`apply`
  //    outright (a bounded stderr diagnostic plus exit 1, never an
  //    envelope). We assert through the raw process, not the exchange
  //    helper, because a refusal is exactly the expected outcome.
  const malicious = request("validate", { ir_path: ".lekalo/ir/planner.json", fix: "apply", apply: true });
  const refusal = spawnSync(php, [join(adapterRoot, "adapter.php")], {
    cwd: tmpdir(),
    encoding: "utf8",
    timeout: 60_000,
    input: JSON.stringify(malicious),
  });
  if (refusal.status === 0 || refusal.stdout !== "") {
    fail("apply-must-be-unrepresentable", `status=${refusal.status} stdout=${String(refusal.stdout).slice(0, 120)}`);
  }
  step("no-apply-surface", {});
}

// ---------------------------------------------------------------------------
// Real suite: the pinned binary against the recorded fixtures.
// ---------------------------------------------------------------------------

function findMago() {
  const candidates = (process.env.LEKALO_MAGO ?? "mago").split(";").filter(Boolean);
  for (const candidate of candidates) {
    const probe = spawnSync(candidate, ["--version"], { encoding: "utf8", timeout: 30_000 });
    if (probe.status === 0 && String(probe.stdout).includes(lock.tool.version)) return candidate;
  }
  return null;
}

/** A disposable read-only project view for one Mago invocation. */
function withSandbox(files, fn) {
  const sandbox = mkdtempSync(join(tmpdir(), "lekalo-mago-"));
  for (const [name, content] of Object.entries(files)) {
    const path = join(sandbox, name);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, content);
  }
  try {
    return fn(sandbox);
  } finally {
    rmSync(sandbox, { recursive: true, force: true });
  }
}

function runRealSuite(mago) {
  if (mago === null) {
    const message = "no Mago executable at the pinned version found (set LEKALO_MAGO)";
    if (process.argv.includes("--require-available")) fail("mago-missing", message);
    process.stdout.write(`${JSON.stringify({
      ok: true, step: "real-suite", skipped: true,
      reason: "mago-unavailable", detail: "a platform skip never counts as Mago acceptance",
    })}\n`);
    return;
  }

  const violations = readFileSync(join(fixtureRoot, "violations.php"), "utf8");
  const clean = readFileSync(join(fixtureRoot, "project.php"), "utf8");
  const guardSource = readFileSync(join(fixtureRoot, "guard-source.php"), "utf8");
  const config = `${readFileSync(join(fixtureRoot, "mago.toml"), "utf8")}\n`;

  // 1. lint JSON over the violations fixture: recorded codes at the
  //    recorded exit code.
  const lintOutcome = withSandbox({ "mago.toml": config, "src/violations.php": violations }, (sandbox) =>
    spawnSync(mago, [
      "--workspace", ".",
      "lint",
      "--only", "strict-types",
      "--only", "no-variable-variable",
      "--reporting-format", "json",
      "--reporting-target", "stdout",
    ], { cwd: sandbox, encoding: "utf8", timeout: 120_000 }));
  const lintExpected = JSON.parse(readFileSync(join(fixtureRoot, "recording.lint.json"), "utf8"));
  if (lintOutcome.status !== lintExpected.exitCode) {
    fail("lint-exit", `expected ${lintExpected.exitCode}, got ${lintOutcome.status}`);
  }
  let lintReport;
  try {
    lintReport = JSON.parse(lintOutcome.stdout);
  } catch {
    fail("lint-unparseable", String(lintOutcome.stdout).slice(0, 200));
  }
  const codes = (lintReport.issues ?? []).map((issue) => issue.code);
  for (const expected of lintExpected.recordedFindings) {
    if (!codes.includes(expected.code)) fail("lint-code-missing", expected.code);
  }
  step("lint-json", { codes: [...new Set(codes)] });

  // 2. analyze JSON: error findings exit 1 — findings ≠ infrastructure
  //    is decided by the decoded report, never by the exit code alone.
  const analyzeOutcome = withSandbox({
    "mago.toml": config,
    "src/project.php": clean,
    "src/broken.php": "<?php\n$x = $undefined;\n",
  }, (sandbox) =>
    spawnSync(mago, ["--workspace", ".", "analyze", "--reporting-format", "json", "--reporting-target", "stdout"],
      { cwd: sandbox, encoding: "utf8", timeout: 120_000 }));
  const analyzeExpected = JSON.parse(readFileSync(join(fixtureRoot, "recording.analyze.json"), "utf8"));
  if (analyzeOutcome.status !== analyzeExpected.exitCode) {
    fail("analyze-exit", `expected ${analyzeExpected.exitCode}, got ${analyzeOutcome.status}`);
  }
  const analyzeReport = JSON.parse(analyzeOutcome.stdout);
  const analyzeCodes = (analyzeReport.issues ?? []).map((issue) => issue.code);
  for (const expected of analyzeExpected.recordedFindings) {
    if (!analyzeCodes.includes(expected.code)) fail("analyze-code-missing", expected.code);
  }
  step("analyze-json", { exitCode: analyzeOutcome.status, codes: analyzeCodes });

  // 3. SARIF parity over the same fixture: same rule ids, 1-based
  //    regions, absolute file URIs only.
  const sarifOutcome = withSandbox({ "mago.toml": config, "src/violations.php": violations }, (sandbox) =>
    spawnSync(mago, ["--workspace", ".", "lint", "--only", "strict-types",
      "--reporting-format", "sarif", "--reporting-target", "stdout"],
      { cwd: sandbox, encoding: "utf8", timeout: 120_000 }));
  const sarif = JSON.parse(sarifOutcome.stdout);
  const sarifRules = (sarif.runs?.[0]?.results ?? []).map((r) => r.ruleId);
  if (!sarifRules.includes("strict-types")) fail("sarif-parity", "strict-types missing from SARIF results");
  for (const result of sarif.runs?.[0]?.results ?? []) {
    const uri = result.locations?.[0]?.physicalLocation?.artifactLocation?.uri ?? "";
    if (!/^(\/|[A-Za-z]:|\\\\)/.test(uri)) fail("sarif-uri", uri);
  }
  step("sarif-parity", { results: sarifRules.length });

  // 4. Dry-run fix preview: the diff appears, the file bytes never move.
  const before = createHash("sha256").update(violations).digest("hex");
  const fixOutcome = withSandbox({ "mago.toml": config, "src/violations.php": violations }, (sandbox) => {
    const outcome = spawnSync(mago, ["--workspace", ".", "lint", "--fix", "--dry-run", "--unsafe",
      "--only", "strict-types"], { cwd: sandbox, encoding: "utf8", timeout: 120_000 });
    const after = createHash("sha256").update(readFileSync(join(sandbox, "src", "violations.php"))).digest("hex");
    return { outcome, after };
  });
  if (fixOutcome.after !== before) fail("fix-preview-mutated", "dry-run modified the source file");
  const fixExpected = JSON.parse(readFileSync(join(fixtureRoot, "recording.fix-preview.json"), "utf8"));
  if (fixOutcome.outcome.status !== fixExpected.exitCode) fail("fix-exit", String(fixOutcome.outcome.status));
  step("fix-preview", { mutated: false, structuralEdits: fixExpected.preview.structuralEdits });

  // 5. Guard: the perimeter rule fires on the recorded cross-layer use.
  const guardOutcome = withSandbox({
    "mago.toml": config,
    "src/guard-source.php": guardSource,
  }, (sandbox) => {
    writeFileSync(join(sandbox, "mago.toml"), `${config}
[guard.perimeter]
layering = ['Vendor\\Core', 'Vendor\\Framework']

[[guard.perimeter.rules]]
namespace = 'Vendor\\Core'
permit = ['@native']
`);
    return spawnSync(mago, ["--workspace", ".", "guard", "--reporting-format", "json",
      "--reporting-target", "stdout"], { cwd: sandbox, encoding: "utf8", timeout: 120_000 });
  });
  const guardExpected = JSON.parse(readFileSync(join(fixtureRoot, "recording.guard.json"), "utf8"));
  if (guardOutcome.status !== guardExpected.exitCode) {
    fail("guard-exit", `expected ${guardExpected.exitCode}, got ${guardOutcome.status}`);
  }
  const guardReport = JSON.parse(guardOutcome.stdout);
  const guardCodes = [...new Set((guardReport.issues ?? []).map((issue) => issue.code))];
  for (const expected of guardExpected.recordedFindings) {
    if (!guardCodes.includes(expected.code)) fail("guard-code-missing", expected.code);
  }
  step("guard-json", { codes: guardCodes });

  // 6. Rules listing carries the consumed strict-profile codes.
  const rulesOutcome = withSandbox({ "mago.toml": config, "src/project.php": clean }, (sandbox) =>
    spawnSync(mago, ["lint", "--list-rules", "--json"], { cwd: sandbox, encoding: "utf8", timeout: 120_000 }));
  const rules = JSON.parse(rulesOutcome.stdout);
  const rulesExpected = JSON.parse(readFileSync(join(fixtureRoot, "recording.rules.json"), "utf8"));
  const ruleCodes = Array.isArray(rules) ? rules.map((r) => r.code) : [];
  for (const consumed of rulesExpected.consumedRules) {
    if (!ruleCodes.includes(consumed.code)) fail("rule-missing", consumed.code);
  }
  step("rules", { total: ruleCodes.length });

  process.stdout.write(`${JSON.stringify({
    ok: true, gate: "mago-integration-real", tool: "mago", version: lock.tool.version,
  })}\n`);
}

// ---------------------------------------------------------------------------
// Main.
// ---------------------------------------------------------------------------

const php = findPhp();
if (php === null) fail("php-missing", "no PHP interpreter found; install PHP >= 8.3 or set LEKALO_PHP");

if (mode === "fake") {
  runFakeSuite(php);
  process.stdout.write(`${JSON.stringify({ ok: true, gate: "mago-integration-fake" })}\n`);
} else {
  runRealSuite(findMago());
}
