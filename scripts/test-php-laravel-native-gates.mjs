#!/usr/bin/env node
/**
 * Issue #61 Composer/Laravel native gates harness (research doc §8).
 *
 * Pure planner legs run on every OS with nothing but a PHP interpreter:
 * a disposable copy of the planner fixture plus the committed
 * `native-gates` overlay drives the kernel's plan-native exchange
 * in-process, with process-launching functions disabled in the PHP
 * runtime (`-d disable_functions=...`) so every zero-spawn claim is
 * enforced by the interpreter itself, not by convention.
 *
 * Covered acceptance evidence:
 *   1. targeted selection keeps the mandatory cross-module gate and
 *      proves the unaffected/affected module join (§4);
 *   2. exactly-once suite execution: the widest-coverage aggregate owns
 *      the suite and suppresses the leaf gates (§2/§4/§8);
 *   3. required vs optional gates carry their policy flags (§6);
 *   4. the hostile Composer decoder refusals: shell interpolation,
 *      composer dispatch, install/update/require, networked or
 *      interactive artisan commands, script cycles, argv mismatch,
 *      manifest drift (§3);
 *   5. stale custody (changed manifest, wrong execution-policy digest,
 *      absent selection document) refuses with zero spawns (§3);
 *   6. an explicit release-full fallback enumerates the full confirmed
 *      inventory with the rule digest recorded (§4);
 *   7. the produced plan validates through the Node closed contract
 *      validator and — when LEKALO_BIN is available — through the Rust
 *      core's `native run` seam, which answers the honest plan-only
 *      refusal for a valid plan (§8; skipped with a recorded capability
 *      gap when the binary is not built).
 *
 * No Composer install/update ever runs here: planning needs bytes, not
 * a vendor tree, and the runtime qualification leg is pending (§8.5).
 * Dependency-free; run from the repo root.
 */
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { pathToFileURL } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const adapterRoot = join(repo, "adapters", "php-laravel");
const plannerFixture = join(repo, "tests", "fixtures", "php-laravel", "planner");
const overlayFixture = join(repo, "tests", "fixtures", "php-laravel", "native-gates");
const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "php-laravel-native-gates", reason, detail }, null, 2)}\n`);
  process.exit(1);
};
const sha256File = (path) =>
  "sha256:" + createHash("sha256").update(readFileSync(path)).digest("hex");
const step = (name, fn) => {
  if (typeof fn === "function") fn();
  process.stdout.write(`${JSON.stringify({ ok: true, step: name })}\n`);
};

/** Locate a PHP interpreter without any provisioning or install. */
function findPhp() {
  const candidates = (process.env.LEKALO_PHP ?? "php").split(";").filter(Boolean);
  for (const candidate of candidates) {
    const probe = spawnSync(candidate, ["-v"], { encoding: "utf8" });
    if (probe.status === 0) return candidate;
  }
  fail("php-missing", "no PHP interpreter found; install PHP >= 8.3 or set LEKALO_PHP");
}

const php = findPhp();
const policy = JSON.parse(readFileSync(join(adapterRoot, "composer-gates-policy.json"), "utf8"));

/**
 * The in-process driver: loads the repo source modules, runs one
 * requested operation against the staged sandbox, and prints one JSON
 * document. The launch-disabling `-d` arguments prove the zero-spawn
 * property at the interpreter level.
 */
const DRIVER_PATH = join(tmpdir(), "lekalo-native-gates-driver.php");
const disableFunctions = "disable_functions=proc_open,exec,shell_exec,system,passthru,popen,proc_close,proc_get_status,proc_terminate,pcntl_exec";
function runDriver(sandbox, operation, requestFile, expectSpawnless = true, extraFile = "") {
  const args = ["-n"];
  if (expectSpawnless) args.push("-d", disableFunctions);
  args.push(DRIVER_PATH, operation, sandbox, requestFile ?? "");
  if (extraFile !== "") args.push(extraFile);
  const result = spawnSync(php, args, { encoding: "utf8", timeout: 120_000, maxBuffer: 64 * 1024 * 1024 });
  if (result.status !== 0) {
    fail("driver-failed", `${operation}: ${result.stderr.slice(0, 400)}`);
  }
  try {
    return JSON.parse(result.stdout.trim().split("\n").pop());
  } catch {
    fail("driver-unparseable", `${operation}: ${result.stdout.slice(0, 200)}`);
  }
}

/** The driver source: written once, executed against any sandbox. */
function writeDriver() {
  const source = `<?php
declare(strict_types=1);
$repo = ${JSON.stringify(repo)};
require $repo . '/adapters/php-laravel/src/analyzer.php';
require $repo . '/adapters/php-laravel/src/strict-profile.php';
require $repo . '/adapters/php-laravel/src/kernel.php';
require $repo . '/adapters/php-laravel/src/native-policy.php';
require $repo . '/adapters/php-laravel/src/native-plan.php';
$operation = $argv[1];
$sandbox = $argv[2];
chdir($sandbox);
if ($operation === 'plan') {
    $request = json_decode(file_get_contents($argv[3]), true, 64, JSON_THROW_ON_ERROR);
    try {
        $envelope = dispatch($request);
    } catch (RequestRefusal $refusal) {
        echo json_encode(['envelope' => ['status' => 'refused', 'code' => $refusal->getMessage()], 'plan' => null]), "\\n";
        exit(0);
    }
    $plan = native_last_plan();
    echo json_encode(['envelope' => $envelope, 'plan' => $plan], JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE), "\\n";
    exit(0);
}
if ($operation === 'decoder') {
    // The hostile Composer script decoder probes (research doc section 3):
    // every probe must be refused with its bounded reason.
    $probes = [
        ['interpolation-dollar', ['gate:x' => '@php echo \\$HOME'], 'script-shell-syntax'],
        ['interpolation-percent', ['gate:x' => '@php echo %PATH%'], 'script-shell-syntax'],
        ['composer-dispatch', ['gate:x' => '@composer install'], 'script-package-manager'],
        ['composer-invocation', ['gate:x' => 'composer update'], 'script-package-manager'],
        ['install-subcommand', ['gate:x' => '@php composer.phar install'], 'script-package-manager'],
        ['networked-artisan', ['gate:x' => '@php artisan serve'], 'script-network-or-interactive'],
        ['interactive-artisan', ['gate:x' => '@php artisan tinker'], 'script-network-or-interactive'],
        ['env-assignment', ['gate:x' => 'FOO=1 @php bin/console'], 'script-package-manager'],
        ['redirect', ['gate:x' => '@php bin/a > out.txt'], 'script-shell-syntax'],
    ];
    $failures = [];
    foreach ($probes as [$name, $scripts, $expected]) {
        $decoded = php_decode_composer_script($scripts, 'gate:x');
        if (($decoded['ok'] ?? true) || ($decoded['reason'] ?? '') !== $expected) {
            $failures[] = $name . ':' . ($decoded['reason'] ?? 'accepted');
        }
    }
    // Cycles: a -> b -> a must be refused, never spun.
    $decoded = php_decode_composer_script(['a' => '@b', 'b' => '@a'], 'a');
    if (($decoded['ok'] ?? true) || ($decoded['reason'] ?? '') !== 'script-cycle') {
        $failures[] = 'cycle:' . ($decoded['reason'] ?? 'accepted');
    }
    // A valid literal recipe still decodes.
    $decoded = php_decode_composer_script(['gate:ok' => '@php vendor/bin/testo run --config testo.php'], 'gate:ok');
    if (!($decoded['ok'] ?? false) || $decoded['commands'] !== [['php', 'vendor/bin/testo', 'run', '--config', 'testo.php']]) {
        $failures[] = 'valid-literal:' . ($decoded['reason'] ?? json_encode($decoded['commands'] ?? null));
    }
    echo json_encode(['ok' => $failures === [], 'failures' => $failures]), "\\n";
    exit(0);
}
if ($operation === 'policy-probes') {
    // The confirmation-level refusals: drift and argv mismatch, each
    // against the real manifest bytes of this sandbox.
    $policy = json_decode(file_get_contents($argv[3]), true, 64, JSON_THROW_ON_ERROR);
    $manifestBytes = file_get_contents('composer.json');
    $manifest = json_decode($manifestBytes, true, 64, JSON_THROW_ON_ERROR);
    $failures = [];
    // 1. Manifest drift: a stale custody digest refuses.
    $confirmation = $policy['confirmations'][0];
    $confirmation['manifest_digest'] = 'sha256:' . str_repeat('e', 64);
    $joined = php_verify_one_confirmation($confirmation, '.=' . $manifest['name'], 'sha256:' . hash('sha256', $manifestBytes), $manifest['scripts'], 'native_read_project_bytes');
    if (($joined['ok'] ?? true) || ($joined['reason'] ?? '') !== 'manifest-digest-drift') {
        $failures[] = 'manifest-drift:' . ($joined['reason'] ?? 'accepted');
    }
    // 2. Argv mismatch: the policy argv is verified, never copied.
    $confirmation = $policy['confirmations'][2];
    $confirmation['argv'] = ['php', 'vendor/bin/mago', 'analyze', '--invented-flag'];
    $joined = php_verify_one_confirmation($confirmation, '.=' . $manifest['name'], 'sha256:' . hash('sha256', $manifestBytes), $manifest['scripts'], 'native_read_project_bytes');
    if (($joined['ok'] ?? true) || !str_starts_with($joined['reason'] ?? '', 'argv-mismatch')) {
        $failures[] = 'argv-mismatch:' . ($joined['reason'] ?? 'accepted');
    }
    echo json_encode(['ok' => $failures === [], 'failures' => $failures]), "\\n";
    exit(0);
}
if ($operation === 'fallback') {
    // The explicit release-full fallback: a checked rule with a recorded
    // digest expands the selection to the full confirmed inventory.
    $policy = json_decode(file_get_contents($argv[3]), true, 64, JSON_THROW_ON_ERROR);
    $policy['fallback_rule'] = ['mode' => 'release-full', 'rule_digest' => 'sha256:' . str_repeat('7', 64)];
    $digestInput = $policy;
    unset($digestInput['policy_digest']);
    $policy['policy_digest'] = php_domain_digest(PHP_POLICY_DIGEST_DOMAIN, $digestInput);
    $verification = php_verify_confirmations($policy, 'native_read_project_bytes');
    if (!$verification['ok']) {
        echo json_encode(['ok' => false, 'reason' => $verification['unverifiable'][0] ?? 'unverifiable']), "\\n";
        exit(0);
    }
    $selection = json_decode(file_get_contents('.lekalo/import/native-selection.json'), true, 64, JSON_THROW_ON_ERROR);
    $composer = json_decode(file_get_contents('composer.json'), true, 64, JSON_THROW_ON_ERROR);
    $request = json_decode(file_get_contents($argv[4] ?? ''), true, 64, JSON_THROW_ON_ERROR);
    $native = $request['native_request'];
    $custody = [
        'input_manifest_digest' => $native['input_manifest_digest'],
        'tool_catalog_digest' => php_tool_catalog_digest($verification['tool_catalog']),
        'capability_snapshot_digest' => $native['capability_snapshot_digest'],
        'scan_ref' => $native['scan_ref']['digest'],
        'observed_ref' => $native['observed_ref']['digest'] ?? 'sha256:' . str_repeat('0', 64),
        'profile_id' => 'default',
        'profile_digest' => 'sha256:' . hash('sha256', 'default@php-laravel'),
        'adapter_identity' => adapter_identity(),
    ];
    try {
        $plan = php_build_native_plan([
            'composer' => $composer,
            'composer_lock_state' => 'absent',
            'composer_lock_digest' => null,
            'policy' => $policy,
            'confirmed' => $verification['confirmed'],
            'tool_catalog' => $verification['tool_catalog'],
            'changes' => $native['changes'],
            'selection_manifest' => $selection,
            'custody' => $custody,
        ]);
        echo json_encode(['ok' => true, 'plan' => $plan], JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE), "\\n";
    } catch (PhpPlanRefusal $refusal) {
        echo json_encode(['ok' => false, 'reason' => $refusal->getMessage()]), "\\n";
    }
    exit(0);
}
fwrite(STDERR, 'unknown operation ' . $operation . "\\n");
exit(1);
`;
  writeFileSync(DRIVER_PATH, source);
}

/** The staged sandbox: planner fixture plus the committed overlay. */
function makeSandbox() {
  const sandbox = realpathSync(mkdtempSync(join(tmpdir(), "lekalo-native-gates-")));
  cpSync(plannerFixture, sandbox, { recursive: true });
  cpSync(overlayFixture, sandbox, { recursive: true });
  return sandbox;
}

/** One plan-native request envelope over the sandbox custody. */
function nativeRequest(sandbox, changedFiles) {
  return {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    operation: "plan-native",
    request_id: "req-" + "1".repeat(64),
    project_root: ".",
    native_request: {
      changes: { files: changedFiles, symbols: [] },
      scan_ref: { digest: sha256File(join(sandbox, "composer.json")) },
      execution_policy_ref: { digest: policy.policy_digest },
      input_manifest_digest: sha256File(join(sandbox, "composer.json")),
      tool_catalog_digest: policy.policy_digest,
      capability_snapshot_digest: sha256File(join(sandbox, "composer.json")),
    },
  };
}

async function main() {
  writeDriver();
  const { validateNativePlan, recomputePlanDigest } = await import(
    pathToFileURL(join(repo, "adapters", "node-typescript", "src", "native-contract.mjs")).href
  );

  // --- Leg 1: the hostile Composer decoder refusals -----------------------
  const decoderSandbox = makeSandbox();
  const decoder = runDriver(decoderSandbox, "decoder");
  if (!decoder.ok) fail("decoder-probes", decoder.failures);
  step("hostile composer decoder refusals hold (interpolation, dispatch, install, network, cycles)");

  // --- Leg 2: targeted selection over a planner change --------------------
  const sandbox = makeSandbox();
  const request = nativeRequest(sandbox, [
    { path: "app/application.php", change: "modified" },
  ]);
  const requestPath = join(sandbox, "lekalo-native-request.json");
  writeFileSync(requestPath, JSON.stringify(request));
  const outcome = runDriver(sandbox, "plan", requestPath);
  const envelope = outcome.envelope;
  if (envelope.status !== "ok") fail("planner-envelope", JSON.stringify(envelope).slice(0, 300));
  const plan = outcome.plan;
  if (!plan || plan.kind !== "native-plan") fail("plan-missing", "the kernel produced no plan evidence");

  const summary = envelope.result.native_plan;
  if (summary.digest !== plan.plan_digest) fail("summary-digest", "the wire summary does not bind the plan evidence");
  const commandIds = plan.commands.map((command) => command.id);
  const gateIds = plan.commands.map((command) => command.gate_id);

  // The mandatory cross-module gate rides the selection.
  if (!gateIds.includes("application-boot")) {
    fail("mandatory-gate-dropped", "targeted selection must not skip the mandatory cross-module gate");
  }
  // The consumer edge pulls the legacy module in; the widest-coverage
  // aggregate owns both suites and suppresses the leaf gates — the
  // suites execute exactly once.
  if (!plan.selection.modules.includes("planner") || !plan.selection.modules.includes("legacy")) {
    fail("module-join", `selection modules incomplete: ${JSON.stringify(plan.selection.modules)}`);
  }
  if (!gateIds.includes("testo-full")) fail("aggregate-gate", "the covering aggregate gate must own both suites");
  if (gateIds.includes("scenario-tests") || gateIds.includes("legacy-tests")) {
    fail("double-run", "leaf gates bound to covered suites must be suppressed");
  }
  const covered = new Set(plan.commands.flatMap((command) => command.covers_suite_ids));
  if (!covered.has("laravel") || !covered.has("legacy-suite")) {
    fail("suite-coverage", "the executed plan must cover both suites exactly once");
  }
  if (plan.selection.mode !== "targeted") fail("selection-mode", plan.selection.mode);
  // Required vs optional flags travel from the policy to the commands.
  const byGate = Object.fromEntries(plan.commands.map((command) => [command.gate_id, command]));
  if (byGate["mago-format"].required !== false) fail("optional-flag", "the formatter gate is optional in the fixture policy");
  if (byGate["mago-lint"].required !== true || byGate["application-boot"].required !== true) {
    fail("required-flag", "mandatory gates must carry required=true");
  }
  // The selection document is pinned inside every command.
  const expectedSelectionRef = createHash("sha256")
    .update("lekalo.native-selection.v0.4.0")
    .update(Buffer.from(JSON.stringify(sortedCanonical(plan.selection)), "utf8"))
    .digest("hex");
  for (const command of plan.commands) {
    if (command.selection_ref !== "sha256:" + expectedSelectionRef) {
      fail("selection-ref", "the pinned selection reference drifted from the selection document");
    }
  }
  step("targeted planner change keeps the mandatory cross-module gate and runs both suites exactly once");

  // --- Leg 3: the closed contract validators accept the plan --------------
  if (validateNativePlan(plan) !== true) fail("node-validator", "the closed Node validator refused the produced plan");
  if (recomputePlanDigest(plan) !== plan.plan_digest) fail("digest-recompute", "the plan digest does not recompute");
  if (summary.commands !== plan.commands.length || summary.packages !== plan.workspace.packages.length) {
    fail("summary-shape", "the closed native_plan summary disagrees with the plan evidence");
  }
  step("the produced plan validates through the closed v0.4.0 contract with a matching digest");

  // --- Leg 4: hostile custody refuses with zero spawns --------------------
  // A stale manifest: the policy pins composer.json bytes; one edited
  // byte refuses planning before anything could run.
  const staleSandbox = makeSandbox();
  const staleDescription = JSON.parse(readFileSync(join(staleSandbox, "composer.json"), "utf8"));
  staleDescription.description = "tampered (issue #61 canary)";
  writeFileSync(join(staleSandbox, "composer.json"), JSON.stringify(staleDescription, null, 4) + "\n");
  const staleRequestPath = join(staleSandbox, "request.json");
  writeFileSync(staleRequestPath, JSON.stringify(nativeRequest(staleSandbox, [{ path: "app/application.php", change: "modified" }])));
  const stale = runDriver(staleSandbox, "plan", staleRequestPath);
  if (stale.envelope.status !== "error" || (stale.envelope.error?.message ?? "") !== "manifest-digest-drift") {
    fail("stale-manifest", JSON.stringify(stale.envelope).slice(0, 300));
  }
  if (stale.plan !== null) fail("stale-plan-leak", "a refused request must not leave plan evidence");
  step("stale manifest custody refuses with the bounded drift token and zero spawns (launch-disabled interpreter)");

  // A wrong execution-policy digest: custody mismatch, honest refusal.
  const wrongPolicySandbox = makeSandbox();
  const wrongRequest = nativeRequest(wrongPolicySandbox, [{ path: "app/application.php", change: "modified" }]);
  wrongRequest.native_request.execution_policy_ref.digest = "sha256:" + "f".repeat(64);
  const wrongRequestPath = join(wrongPolicySandbox, "request.json");
  writeFileSync(wrongRequestPath, JSON.stringify(wrongRequest));
  const wrong = runDriver(wrongPolicySandbox, "plan", wrongRequestPath);
  if (wrong.envelope.status !== "error" || wrong.envelope.error?.code !== "execution-policy-mismatch") {
    fail("policy-mismatch", JSON.stringify(wrong.envelope).slice(0, 300));
  }
  // An absent selection document: the targeting input is mandatory.
  const absentSandbox = makeSandbox();
  rmSync(join(absentSandbox, ".lekalo", "import", "native-selection.json"));
  const absentRequestPath = join(absentSandbox, "request.json");
  writeFileSync(absentRequestPath, JSON.stringify(nativeRequest(absentSandbox, [{ path: "app/application.php", change: "modified" }])));
  const absent = runDriver(absentSandbox, "plan", absentRequestPath);
  if (absent.envelope.status !== "error" || absent.envelope.error?.code !== "selection-manifest-absent") {
    fail("selection-absent", JSON.stringify(absent.envelope).slice(0, 300));
  }
  step("wrong policy custody and an absent selection document refuse before any planning");

  // The confirmation-level refusals (drift + argv mismatch).
  const policyProbePath = join(adapterRoot, "composer-gates-policy.json");
  const probes = runDriver(makeSandbox(), "policy-probes", policyProbePath);
  if (!probes.ok) fail("policy-probes", probes.failures);
  step("confirmation custody drift and argv mismatch are verified, never copied");

  // --- Leg 5: an empty selection is an honest blocked plan ----------------
  const emptySandbox = makeSandbox();
  const emptyRequestPath = join(emptySandbox, "request.json");
  writeFileSync(emptyRequestPath, JSON.stringify(nativeRequest(emptySandbox, [])));
  const empty = runDriver(emptySandbox, "plan", emptyRequestPath);
  if (empty.envelope.status !== "ok") fail("empty-envelope", JSON.stringify(empty.envelope).slice(0, 300));
  if (empty.plan.commands.length !== 0 || empty.plan.run_eligibility.state !== "blocked") {
    fail("empty-selection", "a selection with no affected modules must be an honest blocked plan");
  }
  if (empty.plan.selection.mandatory_gate_ids.length !== 0) {
    fail("empty-coverage", "an empty selection must not claim mandatory coverage");
  }
  step("an empty selection answers a blocked plan with no claimed coverage");

  // --- Leg 6: the explicit release-full fallback --------------------------
  const full = runDriver(sandbox, "fallback", join(adapterRoot, "composer-gates-policy.json"), false, requestPath);
  // The fallback driver needs the request for custody digests only.
  if (!full.ok) fail("fallback-refused", full.reason);
  const fullGateIds = full.plan.commands.map((command) => command.gate_id);
  // Full expansion covers every module's gates plus the mandatory ones;
  // exactly-once suite ownership still suppresses duplicate leaves.
  for (const mandatory of ["application-boot", "mago-analyze", "mago-lint", "testo-full", "migration-static"]) {
    if (!fullGateIds.includes(mandatory)) fail("fallback-incomplete", `release-full must enumerate ${mandatory}`);
  }
  if (full.plan.selection.mode !== "release-full" || full.plan.selection.fallback_rule_ref !== "sha256:" + "7".repeat(64)) {
    fail("fallback-rule", "the release-full fallback must record its rule digest");
  }
  if (fullGateIds.includes("scenario-tests") || fullGateIds.includes("legacy-tests")) {
    fail("fallback-double-run", "even release-full keeps exactly-once suite ownership");
  }
  if (full.plan.selection.mandatory_gate_ids.length !== full.plan.commands.filter((command) => command.required).length) {
    // Every required command's gate id must appear in the mandatory set
    // semantics: mandatory gates are a subset of the executed set.
    for (const gateId of full.plan.selection.mandatory_gate_ids) {
      if (!fullGateIds.includes(gateId)) fail("fallback-mandatory", `${gateId} must be executed in release-full`);
    }
  }
  if (validateNativePlan(full.plan) !== true) fail("fallback-validator", "the fallback plan violates the closed contract");
  step("the explicit release-full fallback enumerates the full confirmed inventory with its rule digest recorded");

  // --- Leg 7: the Rust core accepts the produced plan (when built) --------
  const lekaloBin = process.env.LEKALO_BIN
    ? resolve(process.env.LEKALO_BIN)
    : join(repo, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
  if (existsSync(lekaloBin)) {
    const rustSandbox = makeSandbox();
    mkdirSync(join(rustSandbox, "plans"), { recursive: true });
    writeFileSync(join(rustSandbox, "plans", "plan.json"), JSON.stringify(plan, null, 2) + "\n");
    const rustRun = spawnSync(lekaloBin, ["--json", "native", "run", "plans/plan.json"], {
      cwd: rustSandbox,
      encoding: "utf8",
      timeout: 120_000,
    });
    if (rustRun.status !== 0) fail("rust-native-run", (rustRun.stderr ?? "").slice(0, 400));
    const receipt = JSON.parse(rustRun.stdout.trim());
    if (receipt.kind !== "native-run-result" || receipt.outcome !== "unsupported") {
      fail("rust-receipt", JSON.stringify(receipt).slice(0, 300));
    }
    if (receipt.verdict !== "blocked") fail("rust-verdict", receipt.verdict);
    step("the Rust core validates the plan and answers the honest plan-only refusal");
  } else {
    process.stdout.write(`${JSON.stringify({ ok: true, step: "rust-core-leg", skipped: true, reason: "lekalo-binary-not-built", evidence: "set LEKALO_BIN or build the workspace to run the Rust acceptance leg" })}\n`);
  }

  // The runtime qualification leg is honestly pending (research doc §8.5):
  // confined PHP runtimes and provisioned vendor trees are operator/CI
  // custody, never a gate-time install.
  process.stdout.write(`${JSON.stringify({ ok: true, step: "runtime-qualification-leg", skipped: true, reason: "pending-runtime-qualification", evidence: "execution is proven by the Rust runner battery; confined PHP runtime provisioning stays operator/CI custody" })}\n`);

  process.stdout.write(`${JSON.stringify({ ok: true, gate: "php-laravel-native-gates", planDigest: plan.plan_digest, commands: commandIds.length })}\n`);
}

/**
 * Canonical JSON ordering mirroring the Rust/Node byte-compatible
 * encoder: recursively bytewise key-sorted members.
 */
function sortedCanonical(value) {
  if (Array.isArray(value)) return value.map(sortedCanonical);
  if (value !== null && typeof value === "object") {
    const out = {};
    for (const key of Object.keys(value).sort()) out[key] = sortedCanonical(value[key]);
    return out;
  }
  return value;
}

main();
