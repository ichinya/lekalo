#!/usr/bin/env node
// Issue #114 observed-baseline gate.
//
// The Node planner implementation stays the observed baseline while the
// first contracted Laravel + Vue pilot runs on the critical path. This
// gate keeps that claim honest over `tests/fixtures/pilot/observed-baseline/`:
//
//   1. the closed manifest shape (closed key sets, digest grammar, no
//      self-reference);
//   2. every pinned digest matches the live bytes — a baseline drift is
//      a loud refusal, never a silent re-pin;
//   3. the pinned behavior baseline is CURRENT, not historical: the gate
//      re-derives the semantic outcome rows from a fresh corpus run
//      (the committed node-typescript adapter generates every scenario,
//      the generated tests execute under the real `node --test` runner,
//      the durable run records normalize to step/outcome rows) and
//      compares them row by row;
//   4. the equivalence artifact the removal gate consumes stays pinned.
//
// Dependency-free; run from the repo root. Provisioning never happens
// here: the gate needs only Node built-ins and the committed adapter.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const baselineDir = "tests/fixtures/pilot/observed-baseline";
const manifestPath = join(baselineDir, "node-planner.baseline.json");
const adapterPath = join(repoRoot, "adapters", "node-typescript", "adapter.mjs");
const projectFixture = join(repoRoot, "tests", "fixtures", "orchestration", "project");
const scenarioHome = join(projectFixture, "lekalo", "scenarios");
const irEvidence = join(repoRoot, "tests", "fixtures", "adapter-conformance", "inputs", "ir-minimal.json");
const GENERATED_DIR = "src/generated/node-typescript/scenario-tests";
const RUN_RECORD_DIR = join(".lekalo", "import", "scenario-runs");
const DIGEST_GRAMMAR = /^sha256:[0-9a-f]{64}$/;

const sha256File = (path) =>
  "sha256:" + createHash("sha256").update(readFileSync(path)).digest("hex");

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

const manifest = JSON.parse(readFileSync(join(repoRoot, manifestPath), "utf8"));

step("the baseline manifest is closed, self-consistent, and never self-pinning", () => {
  assert.deepEqual(
    Object.keys(manifest).sort(),
    ["digests", "identity", "note", "pins", "role", "schema_version", "target"],
    "closed top-level key set",
  );
  assert.equal(manifest.schema_version, "lekalo/observed-baseline/v0.1.0");
  assert.equal(manifest.identity, "dev.lekalo.observed-baseline@0.1.0");
  assert.equal(manifest.role, "observed-baseline");
  assert.equal(manifest.target, "node-typescript");
  const pinArtifacts = manifest.pins.map((pin) => pin.artifact).sort();
  assert.deepEqual(pinArtifacts, [...new Set(pinArtifacts)], "pin artifacts unique");
  for (const pin of manifest.pins) {
    assert.deepEqual(Object.keys(pin).sort(), ["artifact", "description", "path"], "closed pin shape");
    assert.ok(existsSync(join(repoRoot, ...pin.path.split("/"))), `pinned path exists: ${pin.path}`);
    assert.ok(manifest.digests[pin.path], `pin digest recorded: ${pin.path}`);
  }
  assert.deepEqual(
    Object.keys(manifest.digests).sort(),
    manifest.pins.map((pin) => pin.path).sort(),
    "digest map and pin list agree",
  );
  for (const [path, digest] of Object.entries(manifest.digests)) {
    assert.match(digest, DIGEST_GRAMMAR, `digest grammar: ${path}`);
  }
  assert.ok(!manifest.digests[manifestPath], "the manifest never pins itself");
  // No private naming: planner and target vocabulary only.
  const text = readFileSync(join(repoRoot, manifestPath), "utf8");
  assert.doesNotMatch(text, /taskhub/i, "no consumer-application naming");
});

step("every pinned digest matches the live bytes (drift is a refusal, never a re-pin)", () => {
  for (const [path, digest] of Object.entries(manifest.digests)) {
    const actual = sha256File(join(repoRoot, ...path.split("/")));
    assert.equal(actual, digest, `stale baseline pin: ${path}`);
  }
});

step("the event envelope pins the observed ledger shape and the run-record contract", () => {
  const envelope = JSON.parse(readFileSync(join(repoRoot, baselineDir, "event-envelope.json"), "utf8"));
  assert.deepEqual(Object.keys(envelope).sort(), [
    "binding", "envelope_members", "events", "identity", "note", "role", "schema_version", "target",
  ]);
  assert.deepEqual(envelope.envelope_members, ["id", "kind", "operation"]);
  assert.equal(envelope.binding.record_contract.identity, "dev.lekalo.scenario-run@0.4.0");
  // The pinned observed envelope is the shape the committed port ledger
  // actually emits: {kind, id, operation} per emission.
  const port = readFileSync(join(projectFixture, "src", "testing", "port.mjs"), "utf8");
  assert.match(port, /capture log entries/);
  assert.match(port, /event: \{ kind: "event", id: "planner\.task_focused" \}/);
  assert.match(port, /emissions\.push\(\{ \.\.\.outcome\.event, operation: operationId \}\)/);
});

step("the pinned behavior baseline is current: a fresh corpus run reproduces every row", () => {
  const baseline = JSON.parse(readFileSync(
    join(repoRoot, baselineDir, "behavior", "node.scenario-rows.json"),
    "utf8",
  ));
  assert.equal(baseline.schema_version, "lekalo/observed-behavior-baseline/v0.1.0");
  assert.equal(baseline.runner, "node:test");
  const scenarioFiles = readdirSync(scenarioHome).filter((name) => name.endsWith(".json")).sort();
  assert.equal(scenarioFiles.length, baseline.corpus.scenario_count, "corpus size");
  for (const file of scenarioFiles) {
    const digest = "sha256:" + createHash("sha256")
      .update(readFileSync(join(scenarioHome, file))).digest("hex");
    const pinned = baseline.corpus.scenarios.find((entry) => entry.id === file.replace(/\.json$/, ""));
    assert.ok(pinned, `corpus pin: ${file}`);
    assert.equal(pinned.digest, digest, `corpus pin current: ${file}`);
  }

  // --- the fresh node corpus run -----------------------------------------
  const root = realpathSync(mkdtempSync(join(tmpdir(), "lekalo-observed-baseline-")));
  try {
    mkdirSync(join(root, "lekalo", "scenarios"), { recursive: true });
    cpSync(join(projectFixture, "lekalo", "test-port.json"), join(root, "lekalo", "test-port.json"));
    cpSync(scenarioHome, join(root, "lekalo", "scenarios"), { recursive: true });
    mkdirSync(join(root, ".lekalo", "cache", "ir"), { recursive: true });
    cpSync(irEvidence, join(root, ".lekalo", "cache", "ir", "planner.json"));
    mkdirSync(join(root, "src", "testing"), { recursive: true });
    cpSync(join(projectFixture, "src", "testing", "port.mjs"), join(root, "src", "testing", "port.mjs"));
    cpSync(join(projectFixture, "src", "testing", "port.selftest.mjs"), join(root, "src", "testing", "port.selftest.mjs"));
    writeFileSync(join(root, "package.json"), JSON.stringify({ type: "module", private: true }));
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
        revision: "issue-114-observed-baseline",
        disposition: "public-fixture",
      },
    };
    const profilePath = join(root, "scenario.profile.json");
    writeFileSync(profilePath, JSON.stringify(profile));

    const call = (request) => {
      const result = spawnSync(
        process.execPath,
        [adapterPath, "--lekalo-project-profile-json", readFileSync(profilePath, "utf8")],
        { cwd: root, input: JSON.stringify(request), encoding: "utf8", maxBuffer: 64 * 1024 * 1024, timeout: 120_000 },
      );
      assert.equal(result.status, 0, `adapter exited ${result.status}: ${result.stderr.slice(0, 2000)}`);
      const lines = result.stdout.split("\n").filter((line) => line.trim().startsWith("{"));
      const envelope = JSON.parse(lines[lines.length - 1]);
      assert.equal(envelope.status, "ok", `adapter refused: ${JSON.stringify(envelope.error)}`);
      return envelope;
    };
    const requestId = (seed) => "req-" + createHash("sha256").update(seed).digest("hex");
    const base = {
      protocol: "lekalo.target/v1",
      protocol_version: "0.3.2",
      project_root: ".",
      operation: "generate",
      ir_path: "lekalo/scenarios/planner.scenario.focus_happy.json",
      target: "node-typescript",
      profile: "scenario-fixture",
    };
    let counter = 0;
    for (const file of scenarioFiles) {
      counter += 1;
      const irPath = `lekalo/scenarios/${file}`;
      const dry = call({ ...base, request_id: requestId(`dry-${counter}`), ir_path: irPath, dry_run: true });
      call({ ...base, request_id: requestId(`apply-${counter}`), ir_path: irPath, dry_run: false, plan_id: dry.evidence.plan_id });
    }

    const home = join(root, ...GENERATED_DIR.split("/"));
    const testFiles = [];
    const walk = (current) => {
      for (const entry of readdirSync(current)) {
        const full = join(current, entry);
        if (!full.includes(".")) walk(full);
        else if (entry.endsWith(".test.ts")) testFiles.push(full);
      }
    };
    walk(home);
    const env = { ...process.env };
    delete env.NODE_TEST_CONTEXT;
    delete env.NODE_TEST_RUNNER;
    const run = spawnSync(process.execPath, ["--test", ...testFiles.sort()], {
      cwd: home, encoding: "utf8", timeout: 180_000, env,
    });
    assert.equal(run.status, 0, `node suite failed:\n${`${run.stdout}${run.stderr}`.slice(0, 4000)}`);

    const semanticRows = (record) => record.assertions.map((row) => ({
      step_id: row.step_id,
      observes: row.observes,
      kind: row.kind,
      outcome: row.outcome,
    }));
    const observed = {};
    for (const file of scenarioFiles) {
      const id = file.replace(/\.json$/, "");
      const recordPath = join(root, ...RUN_RECORD_DIR.split("/"), `${id}.json`);
      assert.ok(existsSync(recordPath), `run record: ${id}`);
      observed[id] = semanticRows(JSON.parse(readFileSync(recordPath, "utf8")));
      assert.deepEqual(
        observed[id],
        baseline.scenarios[id],
        `${id}: the observed rows moved - re-pin the baseline consciously, never silently`,
      );
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

step("the equivalence artifact stays pinned for the removal gate", () => {
  const cmp = JSON.parse(readFileSync(
    join(repoRoot, "tests/fixtures/pilot/equivalence/node-laravel.scenario-comparison.json"), "utf8",
  ));
  assert.equal(cmp.identity, "dev.lekalo.scenario-comparison@0.1.0");
  for (const [id, rows] of Object.entries(cmp.scenarios)) {
    assert.equal(rows.equal, true, `${id}: the comparison artifact must stay all-equal`);
  }
});

if (failures > 0) {
  process.stderr.write(`${failures} observed-baseline gate step(s) failed\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({ ok: true, gate: "observed-baseline", target: "node-typescript" })}\n`);
