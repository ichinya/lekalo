// Issue #56 (plan S3): the PHP adapter's binding custody gate —
// scaffold-once emission and the checked-binding join, exercised
// through the real one-shot adapter exchange against a minimal
// materialized project (no vendor tree: generation and verification
// are pure compile exchanges that never execute project code).
//
// Scaffolded custody contract:
//   - the first generation emits the test once under the user-owned
//     `tests/lekalo/scenario-tests/**` scope plus its map marker;
//   - every later generation plans the marker only — user bytes are
//     never rewritten;
//   - a removed test whose marker survives is a `scenario.scaffold-
//     missing` verify finding, never a silent recreate;
//   - clean never touches the scaffold scope.
//
// Checked custody contract (the Node `joinCheckedBindings` port):
//   - `mode: "checked"` emits no test file at all;
//   - verify joins the declared `test` id against the observed index's
//     `test_bindings` claims: missing, ambiguous, and stale evidence
//     digests are typed findings; an absent index is legal silence.

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const adapterPath = join(repoRoot, "adapters", "php-laravel", "adapter.php");
const fixtureRoot = join(repoRoot, "tests", "fixtures", "php-laravel", "planner");
const scenarioHome = join(repoRoot, "tests", "fixtures", "orchestration", "project", "lekalo", "scenarios");
const irEvidence = join(repoRoot, "tests", "fixtures", "adapter-conformance", "inputs", "ir-minimal.json");
const php = process.env.LEKALO_PHP ?? "php";

const GENERATED_DIR = "src/generated/php-laravel/scenario-tests";
const SCAFFOLD_DIR = "tests/lekalo/scenario-tests";
const OBSERVED_INDEX = ".lekalo/import/observed/index.json";

let passed = 0;
function step(name, fn) {
  try {
    fn();
    passed += 1;
    console.log(`ok - ${name}`);
  } catch (error) {
    console.error(`not ok - ${name}`);
    console.error(error instanceof Error ? error.stack : String(error));
    process.exit(1);
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
  return JSON.parse(lines[lines.length - 1]);
}

const requestId = (seed) => "req-" + createHash("sha256").update(seed).digest("hex");

function baseRequest(root, operation, scenarioFile) {
  return {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    project_root: ".",
    operation,
    ir_path: `lekalo/scenarios/${scenarioFile}`,
    target: "php-laravel",
    profile: "default",
  };
}

function generate(root, scenarioFile, seed) {
  const dry = adapterCall(root, {
    ...baseRequest(root, "generate", scenarioFile),
    request_id: requestId(`dry-${seed}`),
    dry_run: true,
  });
  assert.equal(dry.status, "ok", JSON.stringify(dry));
  const apply = adapterCall(root, {
    ...baseRequest(root, "generate", scenarioFile),
    request_id: requestId(`apply-${seed}`),
    dry_run: false,
    plan_id: dry.evidence.plan_id,
  });
  assert.equal(apply.status, "ok", JSON.stringify(apply));
  return { dry, apply };
}

function verify(root, scenarioFile, seed) {
  const response = adapterCall(root, {
    ...baseRequest(root, "verify", scenarioFile),
    request_id: requestId(`verify-${seed}`),
  });
  assert.equal(response.status, "ok", JSON.stringify(response));
  return response.result.findings ?? [];
}

/** The minimal project: port declaration, scenario doc, IR evidence. */
function materialize(root) {
  mkdirSync(join(root, "lekalo", "scenarios"), { recursive: true });
  cpSync(join(fixtureRoot, "lekalo", "php-test-port.json"), join(root, "lekalo", "php-test-port.json"));
  mkdirSync(join(root, ".lekalo", "cache", "ir"), { recursive: true });
  cpSync(irEvidence, join(root, ".lekalo", "cache", "ir", "planner.json"));
}

/** One scenario doc derived from the corpus happy path, rebound. */
function scenarioDoc(root, file, binding) {
  const doc = JSON.parse(readFileSync(join(scenarioHome, "planner.scenario.focus_happy.json"), "utf8"));
  doc.bindings = binding === null ? [] : [binding];
  writeFileSync(join(root, "lekalo", "scenarios", file), JSON.stringify(doc, null, 2) + "\n");
  return doc;
}

const nativeBinding = (mode, test, extra = {}) => ({
  backend: "native",
  runner: "laratesto",
  runnerVersion: "bundled-toolchain",
  capabilities: [],
  capabilityDigest: "sha256:" + "0".repeat(64),
  mode,
  test,
  ...extra,
});

const writeIndex = (root, testBindings) => {
  mkdirSync(join(root, ".lekalo", "import", "observed"), { recursive: true });
  writeFileSync(join(root, OBSERVED_INDEX), JSON.stringify({
    schema_version: "lekalo/observed-index/v0.2.16",
    test_bindings: testBindings,
  }, null, 2) + "\n");
};

const tempBase = tmpdir();
try {
  // --- scaffolded custody -------------------------------------------------
  const scaffoldRoot = realpathSync(mkdtempSync(join(tempBase, "lekalo-php-scaffold-")));
  const scaffoldFile = "planner.scenario.scaffolded.json";
  const scaffoldId = "planner.scenario.focus_happy";
  const scaffoldTest = `${SCAFFOLD_DIR}/planner/${scaffoldId}.test.php`;
  const scaffoldMap = `${SCAFFOLD_DIR}/planner/${scaffoldId}.test.map.json`;

  step("a scaffolded binding emits once into the user-owned scaffold scope", () => {
    materialize(scaffoldRoot);
    scenarioDoc(scaffoldRoot, scaffoldFile, nativeBinding("scaffolded", scaffoldId));
    const { dry } = generate(scaffoldRoot, scaffoldFile, "scaffold-first");
    const paths = dry.writes.map((entry) => entry.path).sort();
    assert.deepEqual(paths, [
      `${GENERATED_DIR}/port.php`,
      `${GENERATED_DIR}/scenario-reporter.php`,
      `${GENERATED_DIR}/scenario-test-kit.php`,
      scaffoldMap,
      scaffoldTest,
    ].sort());
    assert.ok(existsSync(join(scaffoldRoot, ...scaffoldTest.split("/"))), scaffoldTest);
    const text = readFileSync(join(scaffoldRoot, ...scaffoldTest.split("/")), "utf8");
    assert.ok(text.includes(`// lekalo:${scaffoldId}`), "the scaffold carries the claim marker");
    assert.ok(text.includes("dirname(__DIR__, 4)"), "the scaffold requires reach the managed home");
  });

  step("regeneration never rewrites the user-owned scaffolded test", () => {
    const edited = "// user edit — the scaffold is mine now\n" +
      readFileSync(join(scaffoldRoot, ...scaffoldTest.split("/")), "utf8");
    writeFileSync(join(scaffoldRoot, ...scaffoldTest.split("/")), edited);
    const { dry } = generate(scaffoldRoot, scaffoldFile, "scaffold-regen");
    assert.ok(
      dry.writes.every((entry) => entry.path !== scaffoldTest),
      `frozen test must not re-enter the write plan: ${JSON.stringify(dry.writes.map((w) => w.path))}`,
    );
    const findings = verify(scaffoldRoot, scaffoldFile, "scaffold-edited");
    assert.ok(
      !findings.some((finding) => finding.path === scaffoldTest && finding.code === "scenario.drift"),
      `user bytes are not drift: ${JSON.stringify(findings)}`,
    );
    assert.equal(
      readFileSync(join(scaffoldRoot, ...scaffoldTest.split("/")), "utf8"),
      edited,
      "the user edit survived regeneration and verification",
    );
  });

  step("a removed scaffolded test is a finding, never a silent recreate", () => {
    unlinkSync(join(scaffoldRoot, ...scaffoldTest.split("/")));
    const { dry } = generate(scaffoldRoot, scaffoldFile, "scaffold-removed");
    assert.ok(
      dry.writes.every((entry) => entry.path !== scaffoldTest),
      "the marker's survival means the scaffold was emitted once — no recreate",
    );
    const findings = verify(scaffoldRoot, scaffoldFile, "scaffold-gone");
    assert.ok(
      findings.some((finding) =>
        finding.path === scaffoldTest && finding.code === "scenario.scaffold-missing"),
      `expected scenario.scaffold-missing: ${JSON.stringify(findings)}`,
    );
  });

  step("the clean wire can only ever delete inside the generated home", () => {
    // plan-clean carries no ir_path: it plans over the kernel artifact
    // alone, so a scenario path — managed or scaffolded — can never
    // enter a delete plan. The scaffold scope's kernel refusal is the
    // second line of defence, asserted here at the wire level.
    const plan = adapterCall(scaffoldRoot, {
      protocol: "lekalo.target/v1",
      protocol_version: "0.3.2",
      project_root: ".",
      operation: "plan-clean",
      target: "php-laravel",
      profile: "default",
      request_id: requestId("plan-clean-scaffold"),
    });
    assert.equal(plan.status, "ok", JSON.stringify(plan));
    for (const entry of plan.writes) {
      assert.equal(entry.action, "delete", JSON.stringify(entry));
      assert.ok(
        entry.path.startsWith(".lekalo/generated/"),
        `clean may only target the generated home: ${entry.path}`,
      );
      assert.ok(
        !entry.path.startsWith(`${SCAFFOLD_DIR}/`) && !entry.path.startsWith(`${GENERATED_DIR}/`),
        `scenario files are outside clean's reach: ${entry.path}`,
      );
    }
    const apply = adapterCall(scaffoldRoot, {
      protocol: "lekalo.target/v1",
      protocol_version: "0.3.2",
      project_root: ".",
      operation: "clean",
      target: "php-laravel",
      profile: "default",
      request_id: requestId("clean-scaffold"),
      plan_id: plan.evidence.plan_id,
    });
    assert.equal(apply.status, "ok", JSON.stringify(apply));
    assert.ok(
      existsSync(join(scaffoldRoot, ...scaffoldMap.split("/"))),
      "the scaffold marker survives clean",
    );
  });

  // --- checked custody ----------------------------------------------------
  const checkedRoot = realpathSync(mkdtempSync(join(tempBase, "lekalo-php-checked-")));
  const checkedFile = "planner.scenario.checked.json";
  const checkedId = "planner.scenario.focus_happy";

  step("a checked binding emits no test file — only the shared support", () => {
    materialize(checkedRoot);
    scenarioDoc(checkedRoot, checkedFile, nativeBinding("checked", checkedId));
    const { dry } = generate(checkedRoot, checkedFile, "checked-first");
    const paths = dry.writes.map((entry) => entry.path);
    assert.deepEqual(paths.sort(), [
      `${GENERATED_DIR}/port.php`,
      `${GENERATED_DIR}/scenario-reporter.php`,
      `${GENERATED_DIR}/scenario-test-kit.php`,
    ].sort());
  });

  step("an absent observed index is legal silence for the checked join", () => {
    const findings = verify(checkedRoot, checkedFile, "checked-no-index");
    assert.ok(
      !findings.some((finding) => finding.code.startsWith("scenario.binding-")),
      `no binding findings without an index: ${JSON.stringify(findings)}`,
    );
  });

  step("an empty index reports the checked binding missing", () => {
    writeIndex(checkedRoot, []);
    const findings = verify(checkedRoot, checkedFile, "checked-empty-index");
    assert.ok(
      findings.some((finding) =>
        finding.code === "scenario.binding-missing" && finding.detail === "no-scanned-test"),
      JSON.stringify(findings),
    );
  });

  step("a scanned claim joins the checked binding with no finding", () => {
    writeIndex(checkedRoot, [{
      id: `tests/feature/focus_checked_test.php#lekalo:${checkedId}`,
      symbol: "Tests\\Feature\\FocusCheckedTest",
      path: "tests/feature/focus_checked_test.php",
      fingerprint: "sha256:" + "a".repeat(64),
      state: "scanned",
    }]);
    const findings = verify(checkedRoot, checkedFile, "checked-claimed");
    assert.ok(
      !findings.some((finding) => finding.code.startsWith("scenario.binding-")),
      `clean join: ${JSON.stringify(findings)}`,
    );
  });

  step("two claims of the same id are ambiguous", () => {
    writeIndex(checkedRoot, [
      { id: `tests/a_test.php#lekalo:${checkedId}`, symbol: "A", fingerprint: null },
      { id: `tests/b_test.php#lekalo:${checkedId}`, symbol: "B", fingerprint: null },
    ]);
    const findings = verify(checkedRoot, checkedFile, "checked-ambiguous");
    assert.ok(
      findings.some((finding) =>
        finding.code === "scenario.binding-ambiguous" && finding.detail === "claimed-by-2-tests"),
      JSON.stringify(findings),
    );
  });

  step("a stale evidence digest is a mismatch finding", () => {
    scenarioDoc(checkedRoot, checkedFile, nativeBinding("checked", checkedId, {
      evidenceDigest: "sha256:" + "f".repeat(64),
    }));
    writeIndex(checkedRoot, [{
      id: `tests/focus_checked_test.php#lekalo:${checkedId}`,
      symbol: "FocusCheckedTest",
      fingerprint: "sha256:" + "a".repeat(64),
    }]);
    const findings = verify(checkedRoot, checkedFile, "checked-stale");
    assert.ok(
      findings.some((finding) =>
        finding.code === "scenario.binding-mismatch" && finding.detail === "stale-evidence-digest"),
      JSON.stringify(findings),
    );
  });

  step("a non-native checked binding never joins the observed index", () => {
    scenarioDoc(checkedRoot, checkedFile, {
      backend: "fixture",
      runner: "laratesto",
      capabilities: [],
      mode: "checked",
      test: checkedId,
    });
    writeIndex(checkedRoot, []);
    const findings = verify(checkedRoot, checkedFile, "checked-fake");
    assert.ok(
      !findings.some((finding) => finding.code.startsWith("scenario.binding-")),
      `only native bindings join: ${JSON.stringify(findings)}`,
    );
  });
} finally {
  for (const dir of readdirSync(tempBase)) {
    if (dir.startsWith("lekalo-php-scaffold-") || dir.startsWith("lekalo-php-checked-")) {
      rmSync(join(tempBase, dir), { recursive: true, force: true });
    }
  }
}

console.log(JSON.stringify({ ok: true, suite: "php-laravel-scenario-bindings", checks: passed }));
