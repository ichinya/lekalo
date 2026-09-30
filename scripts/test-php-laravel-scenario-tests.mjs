#!/usr/bin/env node
// Issue #56 end-to-end Laratesto gate (plans S4/S7).
//
// Drives the COMMITTED single-file PHP adapter artifact exactly like CI
// does: the planner fixture project is materialized into a disposable
// runtime root, the bundled adapter generates the Laratesto scenario
// tests through a dry-run/apply exchange, the project harness runs them
// under the real pinned Testo/Laratesto/Laravel stack, and the durable
// run records are asserted against the closed evidence contract. The
// never-pass rules are asserted literally on separate disposable copies:
// the concurrency scenario records unsupported rows and skips, a lying
// port turns the happy scenario into a recorded assertion failure, and
// a broken fixture bootstrap produces a distinguishable infrastructure
// failure (no per-step evidence at all).
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const adapterPath = join(repoRoot, "adapters", "php-laravel", "adapter.php");
const fixtureRoot = join(repoRoot, "tests", "fixtures", "php-laravel", "planner");
const scenarioHome = join(repoRoot, "tests", "fixtures", "orchestration", "project", "lekalo", "scenarios");
const irEvidence = join(repoRoot, "tests", "fixtures", "adapter-conformance", "inputs", "ir-minimal.json");
const SCENARIO_DIR = "src/generated/php-laravel/scenario-tests";
const RUN_RECORD_DIR = join(".lekalo", "import", "scenario-runs");
// Issue #114: the corpus grew to six scenarios — the four issue-#56
// legs plus the authorization leg (focus_denied) and the transaction
// leg (focus_rollback). Only the race case is unsupported-only; every
// executed scenario must pass all of its assertion rows.
const GENERATED_TEST_COUNT = 6;

/** Locate a runnable PHP interpreter (CI provisions one; the script never
 * silently skips: a missing runtime is a hard failure with the exact
 * provisioning instruction). */
function locatePhp() {
  const candidates = [process.env.PHP_BINARY, "php"].filter(Boolean);
  for (const candidate of candidates) {
    const probe = spawnSync(candidate, ["-v"], { encoding: "utf8" });
    if (probe.status === 0 && /PHP/.test(probe.stdout)) return candidate;
  }
  console.error("no runnable PHP interpreter found; provision PHP >= 8.3 first");
  process.exit(1);
}

const php = locatePhp();

let failures = 0;
const step = (name, body) => {
  try {
    body();
    process.stdout.write(`ok - ${name}\n`);
  } catch (error) {
    failures += 1;
    process.stdout.write(`FAIL - ${name}\n${error?.stack ?? error}\n`);
  }
};

/** Provision the fixture vendor tree per the committed composer.lock
 * (a provisioning step: never inside a compiler or adapter process,
 * never with scripts). */
function ensureVendor() {
  // Issue #61: verification never provisions. A missing vendor tree is
  // a blocker with a separate provisioning instruction — zero downloads
  // and zero install/update subprocesses from any gate or harness.
  if (!existsSync(join(fixtureRoot, "vendor", "autoload.php"))) {
    assert.fail(
      "provisioned vendor tree missing at " + join(fixtureRoot, "vendor", "autoload.php") +
        "; run the operator bootstrap (composer install --no-interaction --prefer-dist --no-scripts) outside verification, then re-run this harness",
    );
  }
}

/** One adapter exchange over stdin; returns the parsed envelope. */
function adapterCall(root, request) {
  const result = spawnSync(php, [adapterPath], {
    cwd: root,
    input: JSON.stringify(request),
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    timeout: 120_000,
  });
  if (result.status !== 0) {
    throw new Error(`adapter exited ${result.status}: ${result.stderr.slice(0, 2000)}`);
  }
  const lines = result.stdout.split("\n").filter((line) => line.trim().startsWith("{"));
  const envelope = JSON.parse(lines[lines.length - 1]);
  if (envelope.status !== "ok") {
    throw new Error(`adapter refused ${request.operation}: ${JSON.stringify(envelope.error)}`);
  }
  return envelope;
}

const requestId = (seed) =>
  "req-" + createHash("sha256").update(seed).digest("hex");

/** The dry-run/apply exchange for one scenario document. */
function generate(root, scenarioFile) {
  const base = {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    project_root: ".",
    operation: "generate",
    ir_path: `lekalo/scenarios/${scenarioFile}`,
    target: "php-laravel",
    profile: "default",
  };
  const dry = adapterCall(root, { ...base, request_id: requestId(`dry-${scenarioFile}`), dry_run: true });
  assert.match(dry.evidence.plan_id, /^plan-[0-9a-f]{64}$/);
  // 3 support files + 1 test + 1 sidecar per scenario document.
  assert.equal(dry.writes.length, 5, `${scenarioFile}: ${JSON.stringify(dry.writes.map((w) => w.path))}`);
  for (const entry of dry.writes) {
    assert.ok(entry.path.startsWith(`${SCENARIO_DIR}/`), entry.path);
  }
  const apply = adapterCall(root, {
    ...base,
    request_id: requestId(`apply-${scenarioFile}`),
    dry_run: false,
    plan_id: dry.evidence.plan_id,
  });
  assert.deepEqual(apply.writes, dry.writes, `${scenarioFile}: apply echo`);
  for (const entry of dry.writes) {
    const bytes = readFileSync(join(root, ...entry.path.split("/")));
    const digest = "sha256:" + createHash("sha256").update(bytes).digest("hex");
    assert.equal(digest, entry.sha256, entry.path);
  }
}

/** The pinned Testo suite over the generated + project tests. */
function runTesto(root) {
  return spawnSync(php, [join("vendor", "bin", "testo"), "run", "--config", "testo.php", "--suite=Laravel"], {
    cwd: root,
    encoding: "utf8",
    timeout: 300_000,
    maxBuffer: 64 * 1024 * 1024,
  });
}

function materializeProject(root) {
  // The whole fixture including the provisioned vendor tree: a runtime
  // root copy, not a checkout mutation.
  cpSync(fixtureRoot, root, { recursive: true });
  mkdirSync(join(root, "lekalo", "scenarios"), { recursive: true });
  cpSync(scenarioHome, join(root, "lekalo", "scenarios"), { recursive: true });
  mkdirSync(join(root, ".lekalo", "cache", "ir"), { recursive: true });
  cpSync(irEvidence, join(root, ".lekalo", "cache", "ir", "planner.json"));
}

const recordPath = (root, scenarioId) =>
  join(root, ...RUN_RECORD_DIR.split("/"), `${scenarioId}.json`);

const readRecord = (root, scenarioId) =>
  JSON.parse(readFileSync(recordPath(root, scenarioId), "utf8"));

ensureVendor();
const scenarioIds = readdirSync(scenarioHome)
  .filter((name) => name.endsWith(".json"))
  .sort();

const tempBase = tmpdir();
try {
  const root = realpathSync(mkdtempSync(join(tempBase, "lekalo-php-scenario-e2e-")));

  step("the planner fixture materializes with the port declaration and the IR evidence", () => {
    materializeProject(root);
    assert.ok(existsSync(join(root, "lekalo", "php-test-port.json")), "the adapter-owned port declaration ships");
    assert.ok(existsSync(join(root, "tests", "Support", "PlannerPort.php")), "the declared ScenarioPort class ships");
    assert.ok(existsSync(join(root, ".lekalo", "cache", "ir", "planner.json")));
    assert.equal(scenarioIds.length, GENERATED_TEST_COUNT);
  });

  step("the bundled adapter generates every scenario (dry-run then apply)", () => {
    for (const scenarioFile of scenarioIds) generate(root, scenarioFile);
  });

  step("a second generation round plans byte-identical writes", () => {
    for (const scenarioFile of scenarioIds) generate(root, scenarioFile);
  });

  // S3 scaffold-once through the real suite: the scaffolded scenario
  // emits once into the user-owned home, survives user edits and
  // regeneration, and still executes — its record says "scaffolded".
  const scaffoldedFile = "planner.scenario.focus_scaffolded.json";
  const scaffoldedId = "planner.scenario.focus_scaffolded";
  const scaffoldedTest = join(
    root, "tests", "lekalo", "scenario-tests", "planner",
    `${scaffoldedId}.test.php`,
  );
  step("a scaffolded scenario emits once and survives user custody", () => {
    const doc = JSON.parse(
      readFileSync(join(scenarioHome, "planner.scenario.focus_happy.json"), "utf8"),
    );
    doc.scenarioId = scaffoldedId;
    doc.bindings = [{
      backend: "native",
      runner: "laratesto",
      runnerVersion: "bundled-toolchain",
      capabilities: [],
      capabilityDigest: "sha256:" + "0".repeat(64),
      mode: "scaffolded",
      test: scaffoldedId,
    }];
    writeFileSync(
      join(root, "lekalo", "scenarios", scaffoldedFile),
      JSON.stringify(doc, null, 2) + "\n",
    );
    const base = {
      protocol: "lekalo.target/v1",
      protocol_version: "0.3.2",
      project_root: ".",
      operation: "generate",
      ir_path: `lekalo/scenarios/${scaffoldedFile}`,
      target: "php-laravel",
      profile: "default",
    };
    const dry = adapterCall(root, {
      ...base, request_id: requestId("dry-scaffolded"), dry_run: true,
    });
    assert.ok(
      dry.writes.some((entry) =>
        entry.path === `tests/lekalo/scenario-tests/planner/${scaffoldedId}.test.php`),
      `the scaffold write is planned: ${JSON.stringify(dry.writes.map((w) => w.path))}`,
    );
    adapterCall(root, {
      ...base,
      request_id: requestId("apply-scaffolded"),
      dry_run: false,
      plan_id: dry.evidence.plan_id,
    });
    assert.ok(existsSync(scaffoldedTest), "the scaffolded test was emitted");
    // User custody: edit the file, then regenerate — the frozen write
    // never re-enters the plan and the edited bytes survive.
    const edited = readFileSync(scaffoldedTest, "utf8")
      .replace("<?php", "<?php\n\n// user-owned scaffold — edits persist");
    assert.notEqual(edited, readFileSync(scaffoldedTest, "utf8"), "the edit applied");
    writeFileSync(scaffoldedTest, edited);
    const regen = adapterCall(root, {
      ...base, request_id: requestId("regen-scaffolded"), dry_run: true,
    });
    const scaffoldedTestPath = `tests/lekalo/scenario-tests/planner/${scaffoldedId}.test.php`;
    assert.ok(
      regen.writes.every((entry) => entry.path !== scaffoldedTestPath),
      `the frozen test stays out of the plan: ${JSON.stringify(regen.writes.map((w) => w.path))}`,
    );
    assert.ok(
      regen.writes.some((entry) =>
        entry.path === `tests/lekalo/scenario-tests/planner/${scaffoldedId}.test.map.json`),
      "the managed marker stays in the plan",
    );
    assert.equal(readFileSync(scaffoldedTest, "utf8"), edited, "user bytes preserved");
  });

  step("the generated tests execute under the pinned Testo suite", () => {
    const result = runTesto(root);
    const output = `${result.stdout}${result.stderr}`;
    assert.equal(result.status, 0, `the suite failed:\n${output.slice(0, 4000)}`);
    assert.match(output, /1 skipped/, "the concurrency scenario skips");
  });

  step("run records satisfy the closed evidence contract", () => {
    for (const scenarioFile of scenarioIds) {
      const scenarioId = scenarioFile.replace(/\.json$/, "");
      const record = readRecord(root, scenarioId);
      assert.equal(record.schema_version, "lekalo/scenario-run/v0.4.0");
      assert.equal(record.identity, "dev.lekalo.scenario-run@0.4.0");
      assert.equal(record.scenario.id, scenarioId);
      assert.equal(record.binding_mode, "generated");
      assert.equal(record.runner.id, "laratesto");
      assert.match(record.test.fingerprint, /^sha256:[0-9a-f]{64}$/);
      assert.ok(record.assertions.length >= 1, `${scenarioId}: at least one assertion row`);
      for (const row of record.assertions) {
        assert.ok(
          ["pass", "fail", "unsupported", "infrastructure", "degraded"].includes(row.outcome),
          `${scenarioId}: closed outcome: ${row.outcome}`,
        );
      }
      if (scenarioId === "planner.scenario.focus_concurrent") {
        assert.ok(
          record.assertions.every((row) => row.outcome === "unsupported"),
          "the race case records unsupported rows only",
        );
      } else {
        assert.ok(
          record.assertions.every((row) => row.outcome === "pass"),
          `${scenarioId}: every assertion passed`,
        );
      }
      // Redaction: no host paths inside the record content.
      const content = readFileSync(recordPath(root, scenarioId), "utf8");
      assert.doesNotMatch(content, /[/\\]Users[/\\]|[/\\]Temp[/\\]|tmpdir/, `${scenarioId}: no host paths`);
    }
    // The scaffolded scenario ran under the suite too: its durable
    // record says binding_mode "scaffolded" and carries the user's
    // fingerprint of the edited file — user-owned bytes that still
    // produce honest evidence.
    const scaffolded = readRecord(root, scaffoldedId);
    assert.equal(scaffolded.binding_mode, "scaffolded");
    assert.equal(scaffolded.runner.id, "laratesto");
    assert.ok(
      scaffolded.assertions.every((row) => row.outcome === "pass"),
      "the edited scaffold still passes",
    );
    const editedFingerprint = "sha256:" +
      createHash("sha256").update(readFileSync(scaffoldedTest)).digest("hex");
    assert.equal(
      scaffolded.test.fingerprint,
      editedFingerprint,
      "the record fingerprints the user's bytes, not the emitted ones",
    );
  });

  step("the toolchain custody record carries the exact observed versions", () => {
    // Plan S1: the durable custody document records the facts of the
    // run that actually happened — PHP version, resolved Laratesto /
    // Testo / Laravel package versions, and the composer.lock digest —
    // so evidence never has to trust declared constraints.
    const custodyPath = join(root, ".lekalo", "import", "toolchain", "php-laravel.json");
    assert.ok(existsSync(custodyPath), "the suite wrote the toolchain custody record");
    const custody = JSON.parse(readFileSync(custodyPath, "utf8"));
    assert.deepEqual(Object.keys(custody).sort(), [
      "adapter", "identity", "runner", "schema_version", "toolchain",
    ]);
    assert.equal(custody.schema_version, "lekalo/scenario-toolchain/v0.1.0");
    assert.equal(custody.identity, "dev.lekalo.scenario-toolchain@0.1.0");
    assert.equal(custody.adapter.id, "lekalo-target-php-laravel");
    assert.equal(custody.runner.id, "laratesto");

    const phpVersion = spawnSync(php, ["-r", "echo PHP_VERSION;"], { encoding: "utf8" });
    assert.equal(phpVersion.status, 0);
    assert.equal(custody.toolchain.php, phpVersion.stdout.trim(), "the observed PHP version");

    const lock = JSON.parse(readFileSync(join(fixtureRoot, "composer.lock"), "utf8"));
    const lockDigest = "sha256:" +
      createHash("sha256").update(readFileSync(join(fixtureRoot, "composer.lock"))).digest("hex");
    assert.equal(custody.toolchain.composer_lock, lockDigest, "the exact lock custody");
    const locked = new Map(
      lock.packages.map((pkg) => [pkg.name, pkg.version]),
    );
    assert.deepEqual(Object.keys(custody.toolchain.packages).sort(), [
      "ichinya/laratesto", "laravel/framework", "testo/testo",
    ]);
    for (const [pkg, version] of Object.entries(custody.toolchain.packages)) {
      assert.equal(version, locked.get(pkg), `${pkg}: observed version equals the lock`);
    }
  });

  step("reruns are clean: the second suite run reproduces identical records", () => {
    const before = new Map(scenarioIds.map((scenarioFile) => {
      const scenarioId = scenarioFile.replace(/\.json$/, "");
      return [scenarioId, readFileSync(recordPath(root, scenarioId), "utf8")];
    }));
    const result = runTesto(root);
    assert.equal(result.status, 0, `the rerun failed:\n${result.stdout.slice(0, 2000)}`);
    for (const [scenarioId, bytes] of before) {
      assert.equal(readFileSync(recordPath(root, scenarioId), "utf8"), bytes, `${scenarioId}: identical record`);
    }
  });
  rmSync(root, { recursive: true, force: true });

  // --- negative controls on their own disposable copies ---------------------

  const lieRoot = realpathSync(mkdtempSync(join(tempBase, "lekalo-php-scenario-lie-")));
  step("negative control: a lying port turns the happy scenario into a recorded assertion failure", () => {
    materializeProject(lieRoot);
    generate(lieRoot, "planner.scenario.focus_happy.json");
    const portFile = join(lieRoot, "tests", "Support", "PlannerPort.php");
    const original = readFileSync(portFile, "utf8");
    // One mutation on the user-owned port surface: the emissions capture
    // answers empty, so the emitted-event assertion fails for real while
    // the given/when steps stay healthy.
    const mutated = original.replace(
      "public function emissions(): array\n    {\n        return array_map(static fn (array $e): array => [...$e], $this->emissionLog);\n    }",
      "public function emissions(): array\n    {\n        return [];\n    }",
    );
    assert.notEqual(mutated, original, "the mutation applies");
    writeFileSync(portFile, mutated);
    const result = runTesto(lieRoot);
    assert.notEqual(result.status, 0, "the mutated suite must fail");
    const record = readRecord(lieRoot, "planner.scenario.focus_happy");
    assert.ok(
      record.assertions.some((row) => row.outcome === "fail" && row.step_id === "emitted"),
      `the failure is a recorded fail row: ${JSON.stringify(record.assertions)}`,
    );
  });
  rmSync(lieRoot, { recursive: true, force: true });

  const bootRoot = realpathSync(mkdtempSync(join(tempBase, "lekalo-php-scenario-boot-")));
  step("negative control: a broken fixture bootstrap is infrastructure, not an assertion failure", () => {
    materializeProject(bootRoot);
    generate(bootRoot, "planner.scenario.focus_happy.json");
    // A green baseline first: the record exists and passes.
    assert.equal(runTesto(bootRoot).status, 0);
    assert.ok(readRecord(bootRoot, "planner.scenario.focus_happy").assertions.every((row) => row.outcome === "pass"));
    // Break the fixture's own bootstrap: the test never runs, so no
    // per-step evidence can exist — the missing expected inventory is
    // the infrastructure signature, never a pass.
    rmSync(recordPath(bootRoot, "planner.scenario.focus_happy"));
    const bootstrap = join(bootRoot, "app", "application.php");
    const original = readFileSync(bootstrap, "utf8");
    const broken = original.replace("<?php\n", "<?php\nthrow new \\RuntimeException('boot-broken');\n");
    assert.notEqual(broken, original, "the mutation applies");
    writeFileSync(bootstrap, broken);
    const result = runTesto(bootRoot);
    assert.notEqual(result.status, 0, "the broken suite must fail");
    assert.ok(
      !existsSync(recordPath(bootRoot, "planner.scenario.focus_happy")),
      "a boot failure produces no per-step record — detectable as missing expected inventory, never a pass",
    );
  });
  rmSync(bootRoot, { recursive: true, force: true });
} finally {
  // Any leftover temp roots are removed by the per-root cleanups above;
  // this line only guards against an early throw between allocations.
}

if (failures > 0) {
  process.stdout.write(`${JSON.stringify({ ok: false, gate: "php-laravel-scenario-tests", failures })}\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({ ok: true, gate: "php-laravel-scenario-tests", scenarios: GENERATED_TEST_COUNT })}\n`);
