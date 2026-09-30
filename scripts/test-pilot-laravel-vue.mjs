#!/usr/bin/env node
// Issue #114 pilot-path gate: one command that walks the contracted
// Laravel + Vue pilot path end to end through the EXISTING harnesses.
//
//   model (neutral, target-free)
//     -> Node observed baseline (pinned + current)
//     -> Laravel command/query/policy/storage bindings (operations)
//     -> HTTP/OpenAPI projection + runtime battery (routes)
//     -> generated/checked TypeScript client + maintained Vue screen (ui)
//     -> Mago analysis gate (fake always; real when LEKALO_MAGO is set)
//     -> Composer/Laravel native gates
//     -> Laratesto execution of the portable scenario corpus
//     -> neutral node/laravel equivalence (parity)
//
// This script only orchestrates: every leg is the committed harness a
// CI job runs on its own. Provisioning stays outside verification (the
// planner vendor tree per composer.lock; Ajv 8.17.1, typescript, and
// @vue/compiler-sfc outside the checkout through NODE_PATH), exactly as
// for the individual gates. Dependency-free; run from the repo root.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const vendorAutoload = join(repoRoot, "tests", "fixtures", "php-laravel", "planner", "vendor", "autoload.php");

const legs = [
  {
    name: "model-neutrality",
    role: "the semantic planner model carries no target-specific concepts",
    script: "scripts/test-planner-model-neutrality.mjs",
    ajv: false,
  },
  {
    name: "observed-baseline",
    role: "the Node observed baseline is digest-pinned and still current",
    script: "scripts/test-observed-baseline.mjs",
    ajv: false,
  },
  {
    name: "php-laravel-operations",
    role: "command/query/policy/storage bindings generate and verify (13 stages)",
    script: "scripts/test-php-laravel-operations.mjs",
    ajv: false,
  },
  {
    name: "php-laravel-routes",
    role: "the HTTP/OpenAPI projection serves the runtime battery (9 stages)",
    script: "scripts/test-php-laravel-routes.mjs",
    ajv: false,
  },
  {
    name: "php-laravel-ui",
    role: "the generated/checked TS client, the maintained Vue screen, and the E2E bindings (6 stages)",
    script: "scripts/test-php-laravel-ui.mjs",
    ajv: true,
  },
  {
    name: "mago",
    role: "the Mago analysis gate (fake suite; set LEKALO_MAGO for the real toolchain)",
    script: "scripts/test-mago-integration.mjs",
    args: ["--fake"],
    ajv: false,
    // When the pinned Mago toolchain is provisioned, the real suite is
    // the acceptance leg and runs right after the fake one.
    realArgs: ["--real", "--require-available"],
  },
  {
    name: "php-laravel-native-gates",
    role: "the Composer/Laravel native gate planning battery (zero-spawn legs)",
    script: "scripts/test-php-laravel-native-gates.mjs",
    ajv: false,
  },
  {
    name: "php-laravel-scenario-tests",
    role: "the portable corpus executes under the pinned Testo/Laratesto stack (6 scenarios)",
    script: "scripts/test-php-laravel-scenario-tests.mjs",
    ajv: false,
  },
  {
    name: "php-laravel-parity",
    role: "the neutral node/laravel equivalence over the same corpus (6 scenarios)",
    script: "scripts/test-php-laravel-parity.mjs",
    ajv: false,
  },
];

let ajvNote = "";
if (legs.some((leg) => leg.ajv)) {
  const probe = spawnSync(process.execPath, ["-e", "require('ajv/dist/2020.js')"], {
    env: process.env,
    encoding: "utf8",
  });
  if (probe.status !== 0) {
    ajvNote = " (warning: ajv is not reachable through NODE_PATH - the ui leg will refuse; provision Ajv 8.17.1, typescript, and @vue/compiler-sfc outside the checkout as CI does)";
  }
}

if (!existsSync(vendorAutoload)) {
  process.stderr.write(
    `${JSON.stringify({
      ok: false,
      gate: "pilot-laravel-vue",
      reason: "vendor-missing",
      detail: "provision the planner fixture vendor tree first: composer install --no-interaction --prefer-dist --no-scripts --working-dir tests/fixtures/php-laravel/planner",
    }, null, 2)}\n`,
  );
  process.exit(1);
}

const results = [];
let failures = 0;
const legQueue = [];
for (const leg of legs) {
  legQueue.push(leg);
  if (leg.realArgs && (process.env.LEKALO_MAGO || magoOnPath())) {
    legQueue.push({ ...leg, name: `${leg.name}-real`, role: `${leg.role} (real toolchain)`, args: leg.realArgs });
  }
}

function magoOnPath() {
  const probe = spawnSync(process.env.LEKALO_MAGO ?? "mago", ["--version"], { encoding: "utf8" });
  return probe.status === 0;
}

for (const leg of legQueue) {
  const started = Date.now();
  const env = { ...process.env };
  const child = spawnSync(process.execPath, [join(repoRoot, ...leg.script.split("/")), ...(leg.args ?? [])], {
    cwd: repoRoot,
    encoding: "utf8",
    timeout: 900_000,
    maxBuffer: 64 * 1024 * 1024,
    env,
  });
  const durationMs = Date.now() - started;
  const ok = child.status === 0;
  if (!ok) failures += 1;
  results.push({ leg: leg.name, ok, duration_ms: durationMs });
  process.stdout.write(
    `${ok ? "ok" : "FAIL"} - ${leg.name} (${(durationMs / 1000).toFixed(1)}s): ${leg.role}\n`,
  );
  if (!ok) {
    const tail = `${child.stdout}${child.stderr}`.trim().split("\n").slice(-15).join("\n");
    process.stdout.write(`${tail}\n`);
    break;
  }
}

if (failures > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "pilot-laravel-vue", failures, results, note: ajvNote }, null, 2)}\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({ ok: true, gate: "pilot-laravel-vue", legs: results.length, results, note: ajvNote }, null, 2)}\n`);
