#!/usr/bin/env node
// Issue #56 semantic parity gate (plan S7, AC "Node and Laravel results
// normalize to the same semantic scenario status").
//
// Drives the SAME committed scenario corpus through BOTH backends over
// their committed single-file adapter artifacts: the Node fixture runs
// the generated tests under `node --test`, the Laravel fixture runs them
// under the pinned Testo/Laratesto stack, and the durable run records of
// every scenario are compared as semantic row tuples
// (step_id, observes, kind, outcome) — runner identity, test paths,
// fingerprints, profiles, and durations are excluded by construction.
// A shared lying-port mutation then proves that a recorded assertion
// failure normalizes identically on both backends (a fail row at the
// same semantic step, never a backend-specific shape).
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const nodeAdapterPath = join(repoRoot, "adapters", "node-typescript", "adapter.mjs");
const phpAdapterPath = join(repoRoot, "adapters", "php-laravel", "adapter.php");
const nodeFixture = join(repoRoot, "tests", "fixtures", "orchestration", "project");
const phpFixture = join(repoRoot, "tests", "fixtures", "php-laravel", "planner");
const scenarioHome = join(nodeFixture, "lekalo", "scenarios");
const irEvidence = join(repoRoot, "tests", "fixtures", "adapter-conformance", "inputs", "ir-minimal.json");
const NODE_SCENARIO_DIR = "src/generated/node-typescript/scenario-tests";
const RUN_RECORD_DIR = join(".lekalo", "import", "scenario-runs");
const GENERATED_TEST_COUNT = 4;

const scenarioIds = readdirSync(scenarioHome)
  .filter((name) => name.endsWith(".json"))
  .sort();

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

const requestId = (seed) =>
  "req-" + createHash("sha256").update(seed).digest("hex");

/** Locate a runnable PHP interpreter (a missing runtime is a hard
 * failure with the provisioning instruction, never a silent skip). */
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

/** Provision the PHP fixture vendor tree per the committed composer.lock
 * (a provisioning step: never inside a compiler or adapter process,
 * never with scripts). */
function ensureVendor() {
  // Issue #61: verification never provisions. A missing vendor tree is
  // a blocker with a separate provisioning instruction — zero downloads
  // and zero install/update subprocesses from any gate or harness.
  if (!existsSync(join(phpFixture, "vendor", "autoload.php"))) {
    assert.fail(
      "provisioned vendor tree missing at " + join(phpFixture, "vendor", "autoload.php") +
        "; run the operator bootstrap (composer install --no-interaction --prefer-dist --no-scripts) outside verification, then re-run this harness",
    );
  }
}

// --- node-typescript backend -----------------------------------------------

function writeNodeProfile(root) {
  const profile = {
    id: "scenario-fixture",
    mode: "observed",
    target: "node-typescript",
    readRoots: [
      { path: "lekalo", kind: "tree" },
      { path: ".lekalo", kind: "tree" },
      { path: "src", kind: "tree" },
    ],
    exclusions: [],
    provenance: {
      origin: "declared",
      revision: "issue-56-parity-0001",
      disposition: "public-fixture",
    },
  };
  const path = join(root, "scenario.profile.json");
  writeFileSync(path, JSON.stringify(profile));
  return path;
}

function materializeNodeProject(root) {
  mkdirSync(join(root, "lekalo"), { recursive: true });
  cpSync(join(nodeFixture, "lekalo", "test-port.json"), join(root, "lekalo", "test-port.json"));
  mkdirSync(join(root, "lekalo", "scenarios"), { recursive: true });
  cpSync(scenarioHome, join(root, "lekalo", "scenarios"), { recursive: true });
  mkdirSync(join(root, ".lekalo", "cache", "ir"), { recursive: true });
  cpSync(irEvidence, join(root, ".lekalo", "cache", "ir", "planner.json"));
  mkdirSync(join(root, "src", "testing"), { recursive: true });
  cpSync(join(nodeFixture, "src", "testing", "port.mjs"), join(root, "src", "testing", "port.mjs"));
  cpSync(
    join(nodeFixture, "src", "testing", "port.selftest.mjs"),
    join(root, "src", "testing", "port.selftest.mjs"),
  );
  writeFileSync(join(root, "package.json"), JSON.stringify({ type: "module", private: true }));
  return writeNodeProfile(root);
}

function nodeAdapterCall(root, profilePath, request) {
  const result = spawnSync(
    process.execPath,
    [nodeAdapterPath, "--lekalo-project-profile-json", readFileSync(profilePath, "utf8")],
    { cwd: root, input: JSON.stringify(request), encoding: "utf8", maxBuffer: 64 * 1024 * 1024, timeout: 120_000 },
  );
  if (result.status !== 0) {
    throw new Error(`node adapter exited ${result.status}: ${result.stderr.slice(0, 2000)}`);
  }
  const lines = result.stdout.split("\n").filter((line) => line.trim().startsWith("{"));
  const envelope = JSON.parse(lines[lines.length - 1]);
  if (envelope.status !== "ok") {
    throw new Error(`node adapter refused ${request.operation}: ${JSON.stringify(envelope.error)}`);
  }
  return envelope;
}

function nodeGenerate(root, profilePath, scenarioFile) {
  const base = {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    project_root: ".",
    operation: "generate",
    ir_path: `lekalo/scenarios/${scenarioFile}`,
    target: "node-typescript",
    profile: "scenario-fixture",
  };
  const dry = nodeAdapterCall(root, profilePath, {
    ...base, request_id: requestId(`node-dry-${scenarioFile}`), dry_run: true,
  });
  const apply = nodeAdapterCall(root, profilePath, {
    ...base,
    request_id: requestId(`node-apply-${scenarioFile}`),
    dry_run: false,
    plan_id: dry.evidence.plan_id,
  });
  assert.deepEqual(apply.writes, dry.writes, `node ${scenarioFile}: apply echo`);
}

function nodeGeneratedTestFiles(root) {
  const home = join(root, ...NODE_SCENARIO_DIR.split("/"));
  const found = [];
  const walk = (current) => {
    for (const entry of readdirSync(current)) {
      const full = join(current, entry);
      if (!full.includes(".")) {
        walk(full);
      } else if (entry.endsWith(".test.ts")) {
        found.push(full.slice(home.length + 1).split("\\").join("/"));
      }
    }
  };
  walk(home);
  return found.sort();
}

function runNodeSuite(root) {
  const env = { ...process.env };
  delete env.NODE_TEST_CONTEXT;
  delete env.NODE_TEST_RUNNER;
  const home = join(root, ...NODE_SCENARIO_DIR.split("/"));
  return spawnSync(process.execPath, ["--test", ...nodeGeneratedTestFiles(root)], {
    cwd: home, encoding: "utf8", timeout: 180_000, env,
  });
}

// --- php-laravel backend ----------------------------------------------------

function materializePhpProject(root) {
  // The whole fixture including the provisioned vendor tree: a runtime
  // root copy, not a checkout mutation.
  cpSync(phpFixture, root, { recursive: true });
  mkdirSync(join(root, "lekalo", "scenarios"), { recursive: true });
  cpSync(scenarioHome, join(root, "lekalo", "scenarios"), { recursive: true });
  mkdirSync(join(root, ".lekalo", "cache", "ir"), { recursive: true });
  cpSync(irEvidence, join(root, ".lekalo", "cache", "ir", "planner.json"));
}

function phpAdapterCall(root, request) {
  const result = spawnSync(php, [phpAdapterPath], {
    cwd: root, input: JSON.stringify(request), encoding: "utf8", maxBuffer: 64 * 1024 * 1024, timeout: 120_000,
  });
  if (result.status !== 0) {
    throw new Error(`php adapter exited ${result.status}: ${result.stderr.slice(0, 2000)}`);
  }
  const lines = result.stdout.split("\n").filter((line) => line.trim().startsWith("{"));
  const envelope = JSON.parse(lines[lines.length - 1]);
  if (envelope.status !== "ok") {
    throw new Error(`php adapter refused ${request.operation}: ${JSON.stringify(envelope.error)}`);
  }
  return envelope;
}

function phpGenerate(root, scenarioFile) {
  const base = {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    project_root: ".",
    operation: "generate",
    ir_path: `lekalo/scenarios/${scenarioFile}`,
    target: "php-laravel",
    profile: "default",
  };
  const dry = phpAdapterCall(root, { ...base, request_id: requestId(`php-dry-${scenarioFile}`), dry_run: true });
  const apply = phpAdapterCall(root, {
    ...base,
    request_id: requestId(`php-apply-${scenarioFile}`),
    dry_run: false,
    plan_id: dry.evidence.plan_id,
  });
  assert.deepEqual(apply.writes, dry.writes, `php ${scenarioFile}: apply echo`);
}

function runTesto(root) {
  return spawnSync(php, [join("vendor", "bin", "testo"), "run", "--config", "testo.php", "--suite=Laravel"], {
    cwd: root, encoding: "utf8", timeout: 300_000, maxBuffer: 64 * 1024 * 1024,
  });
}

// --- semantic comparison -----------------------------------------------------

const recordPath = (root, scenarioId) =>
  join(root, ...RUN_RECORD_DIR.split("/"), `${scenarioId}.json`);

const readRecord = (root, scenarioId) =>
  JSON.parse(readFileSync(recordPath(root, scenarioId), "utf8"));

/** The semantic row tuple of one assertion row: identity and outcome,
 * never the backend-specific detail text. */
const semanticRow = (row) => ({
  step_id: row.step_id,
  observes: row.observes,
  kind: row.kind,
  outcome: row.outcome,
});

const semanticRows = (record) => record.assertions.map(semanticRow);

/** The backend-neutral record view: scenario identity/IR binding, the
 * binding mode, and the full semantic row inventory in declared order. */
const semanticRecord = (record) => ({
  scenario: {
    id: record.scenario.id,
    version: record.scenario.version,
    ir_digest: record.scenario.ir_digest,
    symbols: record.scenario.symbols,
    operations: record.scenario.operations,
  },
  binding_mode: record.binding_mode,
  assertions: semanticRows(record),
});

/** Lying-port mutations (one per backend): the emissions capture answers
 * empty, so every emitted-observation assertion fails for real while the
 * given/when steps stay healthy. */
function lieNodePort(root) {
  const portFile = join(root, "src", "testing", "port.mjs");
  const original = readFileSync(portFile, "utf8");
  const mutated = original.replace(
    "emissions() {\n    return emissions.map((entry) => ({ ...entry }));\n  }",
    "emissions() {\n    return [];\n  }",
  );
  assert.notEqual(mutated, original, "the node mutation applies");
  writeFileSync(portFile, mutated);
}

function liePhpPort(root) {
  const portFile = join(root, "tests", "Support", "PlannerPort.php");
  const original = readFileSync(portFile, "utf8");
  const mutated = original.replace(
    "public function emissions(): array\n    {\n        return array_map(static fn (array $e): array => [...$e], $this->emissionLog);\n    }",
    "public function emissions(): array\n    {\n        return [];\n    }",
  );
  assert.notEqual(mutated, original, "the php mutation applies");
  writeFileSync(portFile, mutated);
}

ensureVendor();

const tempBase = tmpdir();
const nodeRoot = realpathSync(mkdtempSync(join(tempBase, "lekalo-parity-node-")));
const phpRoot = realpathSync(mkdtempSync(join(tempBase, "lekalo-parity-php-")));
try {
  step("both fixtures materialize over the same scenario corpus and IR evidence", () => {
    materializePhpProject(phpRoot);
    assert.equal(scenarioIds.length, GENERATED_TEST_COUNT);
    assert.ok(existsSync(join(phpRoot, "lekalo", "php-test-port.json")));
    assert.ok(existsSync(join(phpRoot, ".lekalo", "cache", "ir", "planner.json")));
    const profilePath = materializeNodeProject(nodeRoot);
    assert.ok(existsSync(join(nodeRoot, "lekalo", "test-port.json")));
    assert.ok(existsSync(join(nodeRoot, ".lekalo", "cache", "ir", "planner.json")));
    assert.ok(existsSync(profilePath));
  });

  step("each backend generates every scenario (dry-run then apply)", () => {
    const profilePath = join(nodeRoot, "scenario.profile.json");
    for (const scenarioFile of scenarioIds) {
      nodeGenerate(nodeRoot, profilePath, scenarioFile);
      phpGenerate(phpRoot, scenarioFile);
    }
  });

  step("both generated suites execute under their real runners", () => {
    const node = runNodeSuite(nodeRoot);
    assert.equal(node.status, 0, `node suite failed:\n${`${node.stdout}${node.stderr}`.slice(0, 4000)}`);
    const phpRun = runTesto(phpRoot);
    assert.equal(phpRun.status, 0, `testo suite failed:\n${`${phpRun.stdout}${phpRun.stderr}`.slice(0, 4000)}`);
  });

  step("run records normalize to the same semantic status on both backends", () => {
    for (const scenarioFile of scenarioIds) {
      const scenarioId = scenarioFile.replace(/\.json$/, "");
      assert.ok(existsSync(recordPath(nodeRoot, scenarioId)), `node: missing record ${scenarioId}`);
      assert.ok(existsSync(recordPath(phpRoot, scenarioId)), `php: missing record ${scenarioId}`);
      const nodeRecord = semanticRecord(readRecord(nodeRoot, scenarioId));
      const phpRecord = semanticRecord(readRecord(phpRoot, scenarioId));
      assert.deepEqual(phpRecord, nodeRecord,
        `${scenarioId}: semantic rows diverge\nnode: ${JSON.stringify(nodeRecord.assertions)}\nphp:  ${JSON.stringify(phpRecord.assertions)}`);
    }
  });

  step("the executed subset is non-empty on both backends", () => {
    for (const scenarioFile of scenarioIds) {
      const scenarioId = scenarioFile.replace(/\.json$/, "");
      for (const [backend, root] of [["node", nodeRoot], ["php", phpRoot]]) {
        const record = readRecord(root, scenarioId);
        if (scenarioId === "planner.scenario.focus_concurrent") {
          assert.ok(
            record.assertions.every((row) => row.outcome === "unsupported"),
            `${backend} ${scenarioId}: the race case records unsupported rows only`,
          );
        } else {
          assert.ok(
            record.assertions.some((row) => row.outcome === "pass"),
            `${backend} ${scenarioId}: at least one executed pass row`,
          );
          assert.ok(
            !record.assertions.some((row) => row.outcome === "unsupported"),
            `${backend} ${scenarioId}: no silent unsupported rows`,
          );
        }
      }
    }
  });

  step("a shared lying-port mutation records the same fail row on both backends", () => {
    lieNodePort(nodeRoot);
    liePhpPort(phpRoot);
    const node = runNodeSuite(nodeRoot);
    assert.notEqual(node.status, 0, "the mutated node suite must fail");
    const phpRun = runTesto(phpRoot);
    assert.notEqual(phpRun.status, 0, "the mutated testo suite must fail");
    // focus_happy asserts the emitted observation: both backends must
    // record a fail row at the same semantic step, never a divergent
    // backend-specific outcome.
    const nodeRecord = semanticRecord(readRecord(nodeRoot, "planner.scenario.focus_happy"));
    const phpRecord = semanticRecord(readRecord(phpRoot, "planner.scenario.focus_happy"));
    assert.deepEqual(phpRecord, nodeRecord,
      `post-mutation semantic rows diverge\nnode: ${JSON.stringify(nodeRecord.assertions)}\nphp:  ${JSON.stringify(phpRecord.assertions)}`);
    for (const [backend, record] of [["node", nodeRecord], ["php", phpRecord]]) {
      assert.ok(
        record.assertions.some((row) => row.outcome === "fail" && row.step_id === "emitted"),
        `${backend}: the mutation is a recorded fail row at step emitted`,
      );
    }
  });
} finally {
  rmSync(nodeRoot, { recursive: true, force: true });
  rmSync(phpRoot, { recursive: true, force: true });
}

if (failures > 0) {
  process.stdout.write(`${JSON.stringify({ ok: false, gate: "php-laravel-parity", failures })}\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({ ok: true, gate: "php-laravel-parity", scenarios: scenarioIds.length })}\n`);
