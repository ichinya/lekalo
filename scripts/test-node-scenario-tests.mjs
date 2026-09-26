#!/usr/bin/env node
// Issue #47 end-to-end scenario-test gate (plans S7/S11).
//
// Drives the COMMITTED single-file adapter artifact exactly like CI does:
// the bundled adapter is spawned with a launch profile, a generate
// dry-run/apply exchange materializes the scenario tests, the project
// harness runs them under `node --test` with type stripping, and the
// durable run records are asserted against the closed evidence contract.
// Re-run cleanliness (byte-identical records over a second full run) and
// the never-pass rule for the concurrency scenario are asserted
// literally.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const adapterPath = join(repoRoot, "adapters", "node-typescript", "adapter.mjs");
const projectFixture = join(repoRoot, "tests", "fixtures", "orchestration", "project");
const scenarioHome = join(projectFixture, "lekalo", "scenarios");
const irEvidence = join(
  repoRoot, "tests", "fixtures", "adapter-conformance", "inputs", "ir-minimal.json",
);
const SCENARIO_DIR = "src/generated/node-typescript/scenario-tests";
const RUN_RECORD_DIR = join(".lekalo", "import", "scenario-runs");

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

/** The launch profile: read roots cover the contract home, evidence, and src. */
function writeProfile(root) {
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
      revision: "issue-47-scenario-e2e-0001",
      disposition: "public-fixture",
    },
  };
  const path = join(root, "scenario.profile.json");
  writeFileSync(path, JSON.stringify(profile));
  return path;
}

function materializeProject(root) {
  mkdirSync(join(root, "lekalo"), { recursive: true });
  cpSync(join(projectFixture, "lekalo", "test-port.json"), join(root, "lekalo", "test-port.json"));
  mkdirSync(join(root, "lekalo", "scenarios"), { recursive: true });
  cpSync(scenarioHome, join(root, "lekalo", "scenarios"), { recursive: true });
  mkdirSync(join(root, ".lekalo", "cache", "ir"), { recursive: true });
  cpSync(irEvidence, join(root, ".lekalo", "cache", "ir", "planner.json"));
  mkdirSync(join(root, "src", "testing"), { recursive: true });
  cpSync(
    join(projectFixture, "src", "testing", "port.mjs"),
    join(root, "src", "testing", "port.mjs"),
  );
  writeFileSync(join(root, "package.json"), JSON.stringify({ type: "module", private: true }));
  cpSync(
    join(projectFixture, "src", "testing", "port.selftest.mjs"),
    join(root, "src", "testing", "port.selftest.mjs"),
  );
  writeProfile(root);
}

/** One adapter exchange over stdin; returns the parsed envelope. */
function adapterCall(root, profilePath, request) {
  const result = spawnSync(
    process.execPath,
    // The launch profile travels INLINE (the exact profile JSON text),
    // matching the CI spellings and the kernel's extract contract.
    [adapterPath, "--lekalo-project-profile-json", readFileSync(profilePath, "utf8")],
    {
      cwd: root,
      input: JSON.stringify(request),
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
      timeout: 120000,
    },
  );
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

function generatedTestFiles(root) {
  const home = join(root, ...SCENARIO_DIR.split("/"));
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

/** The node:test run over the generated tests; the env stays runner-clean. */
function runGeneratedTests(root) {
  const env = { ...process.env };
  delete env.NODE_TEST_CONTEXT;
  delete env.NODE_TEST_RUNNER;
  const home = join(root, ...SCENARIO_DIR.split("/"));
  return spawnSync(process.execPath, ["--test", ...generatedTestFiles(root)], {
    cwd: home,
    encoding: "utf8",
    timeout: 180000,
    env,
  });
}

const root = realpathSync(mkdtempSync(join(tmpdir(), "lekalo-scenario-e2e-")));
try {
  step("fixture project materializes with the port and the IR evidence", () => {
    materializeProject(root);
    assert.ok(existsSync(join(root, "lekalo", "test-port.json")));
    assert.ok(existsSync(join(root, ".lekalo", "cache", "ir", "planner.json")));
    assert.equal(scenarioIds.length, 4);
  });

  const requestId = (byte) =>
    "req-" + createHash("sha256").update(String(byte)).digest("hex");
  const requestBase = {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    project_root: ".",
    operation: "generate",
    ir_path: "lekalo/scenarios/planner.scenario.focus_happy.json",
    target: "node-typescript",
    profile: "scenario-fixture",
  };

  const plans = new Map();
  step("the bundled adapter generates every scenario (dry-run then apply)", () => {
    const profilePath = join(root, "scenario.profile.json");
    let counter = 0;
    for (const scenarioFile of scenarioIds) {
      counter += 1;
      const irPath = `lekalo/scenarios/${scenarioFile}`;
      const dry = adapterCall(root, profilePath, {
        ...requestBase,
        request_id: requestId(String(counter)),
        ir_path: irPath,
        dry_run: true,
      });
      // 3 support files + 1 test + 1 sidecar per scenario document.
      assert.equal(dry.writes.length, 5, `${scenarioFile}: ${JSON.stringify(dry.writes.map((w) => w.path))}`);
      assert.match(dry.evidence.plan_id, /^plan-[0-9a-f]{64}$/);
      for (const entry of dry.writes) {
        assert.ok(entry.path.startsWith(`${SCENARIO_DIR}/`), entry.path);
      }
      const apply = adapterCall(root, profilePath, {
        ...requestBase,
        request_id: requestId(`apply-${counter}`),
        ir_path: irPath,
        dry_run: false,
        plan_id: dry.evidence.plan_id,
      });
      assert.deepEqual(apply.writes, dry.writes, `${scenarioFile}: apply echo`);
      for (const entry of dry.writes) {
        const bytes = readFileSync(join(root, ...entry.path.split("/")));
        const digest = "sha256:" + createHash("sha256").update(bytes).digest("hex");
        assert.equal(digest, entry.sha256, entry.path);
      }
      plans.set(scenarioFile, dry.writes);
    }
  });

  step("the port self-test passes against the generated project copy", () => {
    const result = spawnSync(
      process.execPath,
      [join("src", "testing", "port.selftest.mjs")],
      { cwd: root, encoding: "utf8", timeout: 60000 },
    );
    assert.equal(result.status, 0, result.stderr);
  });

  step("the generated tests run green and the concurrency case skips", () => {
    const result = runGeneratedTests(root);
    const output = `${result.stdout}${result.stderr}`;
    assert.equal(result.status, 0, `generated suite failed:\n${output.slice(0, 4000)}`);
    assert.match(output, /skipped 1/, "the concurrency scenario skips");
    assert.doesNotMatch(output, /lekalo:planner\.scenario\.focus_concurrent \(\d+\.?\d*ms\)\n✖/);
  });

  step("run records satisfy the closed evidence contract", () => {
    for (const scenarioId of [
      "planner.scenario.focus_happy",
      "planner.scenario.focus_error",
      "planner.scenario.focus_idempotent",
      "planner.scenario.focus_concurrent",
    ]) {
      const path = join(root, ...RUN_RECORD_DIR.split("/"), `${scenarioId}.json`);
      assert.ok(existsSync(path), `run record for ${scenarioId}`);
      const record = JSON.parse(readFileSync(path, "utf8"));
      assert.equal(record.schema_version, "lekalo/scenario-run/v0.4.0");
      assert.equal(record.identity, "dev.lekalo.scenario-run@0.4.0");
      assert.equal(record.scenario.id, scenarioId);
      assert.equal(record.binding_mode, "generated");
      assert.equal(record.runner.id, "node:test");
      assert.match(record.test.fingerprint, /^sha256:[0-9a-f]{64}$/);
      assert.ok(record.assertions.length >= 1);
      const outcomes = new Set(record.assertions.map((row) => row.outcome));
      for (const row of record.assertions) {
        assert.ok(
          ["pass", "fail", "unsupported", "infrastructure", "degraded"].includes(row.outcome),
          `closed outcome: ${row.outcome}`,
        );
      }
      if (scenarioId === "planner.scenario.focus_concurrent") {
        assert.ok(
          record.assertions.every((row) => row.outcome === "unsupported"),
          "the race case records unsupported rows only",
        );
      } else {
        assert.ok(!outcomes.has("unsupported"), `${scenarioId}: no silent unsupported rows`);
        assert.ok(outcomes.has("pass"), `${scenarioId}: the assertions passed`);
      }
      // Redaction: no host paths inside the record content.
      const content = readFileSync(path, "utf8");
      assert.doesNotMatch(content, /[/\\]Users[/\\]|[/\\]Temp[/\\]|tmpdir/);
    }
  });

  step("reruns are clean: the second full run reproduces identical records", () => {
    const before = new Map();
    for (const name of readdirSync(join(root, ...RUN_RECORD_DIR.split("/")))) {
      before.set(name, readFileSync(join(root, ...RUN_RECORD_DIR.split("/"), name), "utf8"));
    }
    const result = runGeneratedTests(root);
    assert.equal(result.status, 0, `${result.stdout}${result.stderr}`.slice(0, 2000));
    for (const [name, bytes] of before) {
      assert.equal(
        readFileSync(join(root, ...RUN_RECORD_DIR.split("/"), name), "utf8"),
        bytes,
        `${name} is byte-identical over reruns`,
      );
    }
  });

  step("verify over the generated project reports no drift", () => {
    const profilePath = join(root, "scenario.profile.json");
    const envelope = adapterCall(root, profilePath, {
      ...requestBase,
      request_id: requestId(3),
      operation: "verify",
      protocol_version: "0.3.2",
      ir_path: requestBase.ir_path,
      target: "node-typescript",
      profile: "scenario-fixture",
    });
    assert.deepEqual(envelope.result?.findings ?? [], []);
  });

  if (failures > 0) {
    process.stderr.write(`${failures} gate step(s) failed\n`);
    process.exit(1);
  }
  process.stdout.write(`scenario e2e gate: ok (${scenarioIds.length} scenarios)\n`);
} finally {
  rmSync(root, { recursive: true, force: true });
}
