#!/usr/bin/env node
// Issue #51 pilot harness: the explicit target/profile/case execution
// and report surface.
//
//   node scripts/test-worklog-changelog-pilot.mjs --target php --profile strict --case all
//   node scripts/test-worklog-changelog-pilot.mjs --target go --profile permissive --case all
//   node scripts/test-worklog-changelog-pilot.mjs --target go --profile strict --case all --expect-denied
//
// Closed flags: --target php|go, --profile strict|permissive, and
// --case all|<exact-stable-scenario-id>; --expect-denied turns the
// honest strict refusal into the negative gate (the report is still
// emitted and still schema-valid). No shell command, provider URL, or
// consumer-path flags. The gate emits the schema-valid evidence report
// on stdout, bounded infrastructure diagnostics on stderr, and exits 0
// only when the selected supported obligations pass; a strict required
// unsupported/unknown semantics exits nonzero while retaining the
// report. No setup, package install, credential lookup, or live network
// call occurs here.
//
// Before the verdict the harness runs the static composition checks:
// every forbidden-effect assertion of the selected scenario documents
// is evaluated against the compiled Model's canonical effect
// projection through the production binary — a failed static check
// exits 1 regardless of profile, because the composition contract
// itself failed. Runtime assertions stay not_executed/unsupported and
// are never counted as passes.

import { createRequire } from "node:module";
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  ({ default: Ajv2020 } = require("ajv/dist/2020"));
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "ajv-unavailable", detail: error?.message }, null, 2)}\n`,
  );
  process.exit(1);
}
if (ajvVersion !== "8.17.1") {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "ajv-version", detail: ajvVersion }, null, 2)}\n`,
  );
  process.exit(1);
}
const ajv = new Ajv2020({ strict: true, allErrors: true });

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
// The production binary resolves per-host: an explicit LEKALO_BIN wins,
// then the Windows debug name, then the POSIX one (precedent:
// test-context-budget-contracts.mjs). A missing binary is a loud
// failure, never a silent skip.
const resolveBinary = () => {
  if (process.env.LEKALO_BIN) return process.env.LEKALO_BIN;
  const windows = resolve(root, "target/debug/lekalo.exe");
  const posix = resolve(root, "target/debug/lekalo");
  if (existsSync(windows)) return windows;
  if (existsSync(posix)) return posix;
  return windows;
};
const binary = resolveBinary();
const project = "tests/fixtures/pilot/worklog-changelog/model";
const fixtureRoot = "tests/fixtures/pilot/worklog-changelog";

// ---------------------------------------------------------------------------
// Closed flag parsing (a violation is a usage failure, exit 2).
// ---------------------------------------------------------------------------

const args = process.argv.slice(2);
const flag = (name) => {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
};
const target = flag("--target");
const profile = flag("--profile");
const scenario = flag("--case") ?? "all";
const expectDenied = args.includes("--expect-denied");
const usage = (detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason: "usage", detail }, null, 2)}\n`);
  process.exit(2);
};
if (!["php", "go"].includes(target)) usage("--target must be php or go");
if (!["strict", "permissive"].includes(profile)) usage("--profile must be strict or permissive");
if (typeof scenario !== "string" || scenario.length === 0) usage("--case must be all or a scenario id");

const run = (args) => execFileSync(binary, args, { cwd: root, encoding: "utf8" }).trim();
const runJson = (args) => JSON.parse(run(args));
// One envelope-producing run that tolerates the documented unsupported
// class: a strict refusal exits 4 and still carries the receipt bytes
// on stdout. Any other nonzero exit stays a thrown failure.
const runJsonAllowUnsupported = (args) => {
  try {
    return { exit: 0, envelope: JSON.parse(run(args)) };
  } catch (error) {
    const exit = typeof error.status === "number" ? error.status : -1;
    if (exit !== 4) throw error;
    return { exit, envelope: JSON.parse(String(error.stdout ?? "").trim()) };
  }
};
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));

// ---------------------------------------------------------------------------
// 1. The composition gate through the production binary.
// ---------------------------------------------------------------------------

let composition;
try {
  const envelope = runJson([
    "--json",
    "--no-cache",
    "pilot",
    "validate",
    `${fixtureRoot}/attachments/pilot.json`,
    "--project",
    project,
  ]);
  // A success receipt is the payload itself; a failure envelope
  // carries status + diagnostics and the process exits nonzero.
  if (!envelope.pilot || envelope.pilot.status !== "valid") {
    process.stderr.write(
      `${JSON.stringify({ ok: false, reason: "composition-invalid", envelope }, null, 2)}\n`,
    );
    process.exit(1);
  }
  composition = envelope.pilot;
} catch (error) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "binary-unavailable", detail: String(error).slice(0, 400) }, null, 2)}\n`,
  );
  process.exit(1);
}

// ---------------------------------------------------------------------------
// 2. Static composition checks: forbidden-effect assertions never
//    intersect the observed operation's canonical effect projection.
// ---------------------------------------------------------------------------

const scenarioDir = `${fixtureRoot}/scenarios`;
const scenarioFiles = readdirSync(resolve(root, scenarioDir)).sort();
const staticResults = [];
let staticFailures = 0;
for (const name of scenarioFiles) {
  const doc = read(`${scenarioDir}/${name}`);
  if (scenario !== "all" && doc.scenarioId !== scenario) continue;
  // The observed operation of every `then` step: the step id the
  // assertion observes must be a `when` step of this scenario.
  const observed = new Map(doc.when.map((step) => [step.stepId, step.action.operation]));
  for (const thenStep of doc.then) {
    if (thenStep.assertion.kind !== "forbidden_effect") continue;
    const operation = observed.get(thenStep.observes);
    if (!operation) {
      staticResults.push({ scenario: doc.scenarioId, step: thenStep.stepId, result: "unobserved" });
      staticFailures += 1;
      continue;
    }
    const projection = runJson([
      "--json",
      "--no-cache",
      "effects",
      "show",
      operation,
      "--project",
      project,
    ]);
    if (projection.status !== "valid") {
      staticResults.push({ scenario: doc.scenarioId, step: thenStep.stepId, result: "projection-invalid" });
      staticFailures += 1;
      continue;
    }
    const declared = new Set(projection.effects.edges.map((edge) => edge.effect));
    const forbidden = `effect:${thenStep.assertion.effect}`;
    if (declared.has(forbidden)) {
      // The observed operation legitimately declares the effect: the
      // assertion forbids it on the denial path (gate refusal), which
      // only a native run can evaluate. It is retained as runtime
      // evidence, never counted as a static pass.
      staticResults.push({
        scenario: doc.scenarioId,
        step: thenStep.stepId,
        result: "runtime",
        effect: thenStep.assertion.effect,
      });
      continue;
    }
    staticResults.push({
      scenario: doc.scenarioId,
      step: thenStep.stepId,
      result: "pass",
      effect: thenStep.assertion.effect,
    });
  }
}
const runtimeChecks = staticResults.filter((row) => row.result === "runtime").length;
if (staticFailures > 0) {
  process.stderr.write(
    `${JSON.stringify(
      { ok: false, reason: "static-composition-checks-failed", staticFailures, staticResults },
      null,
      2,
    )}\n`,
  );
  process.exit(1);
}

// ---------------------------------------------------------------------------
// 3. The committed evidence report: schema validity, then admission.
// ---------------------------------------------------------------------------

const validateEvidence = ajv.compile(
  read("contracts/worklog-changelog-evidence.schema.v0.8.0.json"),
);
const evidence = read(`${fixtureRoot}/evidence/evidence.json`);
if (!validateEvidence(evidence)) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "evidence-invalid", errors: validateEvidence.errors }, null, 2)}\n`,
  );
  process.exit(1);
}
// The report pins the exact composition bytes.
const { createHash } = await import("node:crypto");
const compositionBytes = readFileSync(
  resolve(root, `${fixtureRoot}/attachments/pilot.json`),
);
const compositionDigest =
  "sha256:" +
  createHash("sha256")
    .update(Buffer.concat([Buffer.from("lekalo/worklog-changelog-pilot/"), compositionBytes]))
    .digest("hex");
if (evidence.compositionRef.digest !== compositionDigest) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "composition-pin-drift" }, null, 2)}\n`,
  );
  process.exit(1);
}

// ---------------------------------------------------------------------------
// The committed public DTO schema: the lane's public transport
// projection must satisfy it exactly (payloadVersion, camelCase
// allowlist fields, UUID entry id, ISO-8601 UTC timestamp).
const publicSchema = read(`${fixtureRoot}/public/changelog-entry.schema.json`);
ajv.addFormat("uuid", /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
const validatePublicEntry = ajv.compile(publicSchema);

// ---------------------------------------------------------------------------
// 3b. The native runtime lane (target php): the committed domain model
//     executes every scenario assertion in-process with the scripted
//     fake transport, and the live results are bound into the report.
//     A missing interpreter or a failing assertion fails the gate; the
//     lane is never skipped and never counted as a pass without
//     execution.
// ---------------------------------------------------------------------------
const nativeResults = {};
if (target === "php") {
  const phpCandidates = (process.env.LEKALO_PHP ?? "php").split(";").filter(Boolean);
  let phpBinary;
  for (const candidate of phpCandidates) {
    const probe = spawnSync(candidate, ["-v"], { encoding: "utf8" });
    if (probe.status === 0) {
      phpBinary = candidate;
      break;
    }
  }
  if (!phpBinary) {
    process.stderr.write(
      JSON.stringify({ ok: false, reason: "php-missing", detail: "no PHP interpreter found; install PHP >= 8.3 or set LEKALO_PHP" }, null, 2) + "\n",
    );
    process.exit(1);
  }
  const lanePath = resolve(root, fixtureRoot, "php/run.php");
  const laneRun = spawnSync(phpBinary, [lanePath], { cwd: root, encoding: "utf8", timeout: 120_000, maxBuffer: 64 * 1024 * 1024 });
  if (laneRun.status !== 0) {
    process.stderr.write(
      JSON.stringify({ ok: false, reason: "native-lane-failed", exit: laneRun.status, stderr: String(laneRun.stderr).slice(0, 400) }, null, 2) + "\n",
    );
    process.exit(1);
  }
  let lane;
  try {
    lane = JSON.parse(laneRun.stdout.trim().split("\n").pop());
  } catch {
    process.stderr.write(
      JSON.stringify({ ok: false, reason: "native-lane-unparseable" }, null, 2) + "\n",
    );
    process.exit(1);
  }
  if (lane.target !== "php" || lane.execution !== "executed") {
    process.stderr.write(
      JSON.stringify({ ok: false, reason: "native-lane-not-executed", lane: { target: lane.target, execution: lane.execution } }, null, 2) + "\n",
    );
    process.exit(1);
  }
  // The lane asserts exactly the committed scenario corpus.
  const corpusIds = scenarioFiles
    .map((name) => name.replace(/\.scenario\.json$/, ""))
    .sort();
  const laneIds = lane.assertions.map((row) => row.scenario).sort();
  if (JSON.stringify(corpusIds) !== JSON.stringify(laneIds)) {
    process.stderr.write(
      JSON.stringify({ ok: false, reason: "native-lane-corpus-drift", corpus: corpusIds.length, lane: laneIds.length }, null, 2) + "\n",
    );
    process.exit(1);
  }
  const failed = lane.assertions.filter((row) => row.status === "fail");
  if (failed.length > 0) {
    process.stderr.write(
      JSON.stringify({ ok: false, reason: "native-assertions-failed", failed: failed.map((row) => row.scenario) }, null, 2) + "\n",
    );
    process.exit(1);
  }
  // The lane's own assertions are not trusted by name alone: every
  // committed scenario document's outcome assertions (typed errors,
  // entity presence, idempotent replay, contract match) must be
  // observed by the lane exactly as declared. An assertion row that
  // reports pass while the committed IR's outcomes were not observed
  // fails the gate — inverting a lane assertion cannot flip the
  // verdict.
  const observationOf = (scenarioId, stepId) =>
    lane.assertions.find((row) => row.scenario === scenarioId)?.observations?.[stepId];
  for (const name of scenarioFiles) {
    const doc = read(`${scenarioDir}/${name}`);
    if (scenario !== "all" && doc.scenarioId !== scenario) continue;
    for (const thenStep of doc.then) {
      const kind = thenStep.assertion.kind;
      let expected;
      if (kind === "error") expected = `error:${thenStep.assertion.error}`;
      else if (kind === "entity_state") {
        const presence =
          thenStep.assertion.expect?.count !== undefined
            ? `count=${thenStep.assertion.expect.count}`
            : (thenStep.assertion.expect?.presence ?? "exists");
        expected = `entity:${thenStep.assertion.entity}:${presence}`;
      } else if (kind === "idempotency") expected = `replay:${thenStep.assertion.replay}`;
      else if (kind === "contract_match") expected = `contract:${thenStep.assertion.contract}`;
      else continue; // forbidden_effect is asserted statically / retained as runtime
      const observed = observationOf(doc.scenarioId, thenStep.stepId);
      if (observed !== expected) {
        process.stderr.write(
          JSON.stringify(
            { ok: false, reason: "scenario-observation-mismatch", scenario: doc.scenarioId, step: thenStep.stepId, expected, observed },
            null,
            2,
          ) + "\n",
        );
        process.exit(1);
      }
    }
  }
  // The public transport projection is validated against the exact
  // committed public DTO schema: a lane emitting the internal
  // snake_case shape instead of the wire DTO fails here.
  if (lane.publicEntry == null) {
    process.stderr.write(
      JSON.stringify({ ok: false, reason: "public-entry-missing" }, null, 2) + "\n",
    );
    process.exit(1);
  }
  if (!validatePublicEntry(lane.publicEntry)) {
    process.stderr.write(
      JSON.stringify({ ok: false, reason: "public-entry-schema-invalid", errors: validatePublicEntry.errors }, null, 2) + "\n",
    );
    process.exit(1);
  }
  nativeResults.php = lane;
  // Bind the live results: the php target row reflects what the native
  // runtime just executed, never a stale committed claim. The wire
  // schema closes assertion rows to {scenario, status}; the outcome
  // observations stay a lane-result surface only.
  const phpRow = evidence.targets.find((row) => row.target === "php");
  if (!phpRow) {
    process.stderr.write(
      JSON.stringify({ ok: false, reason: "target-missing", target: "php" }, null, 2) + "\n",
    );
    process.exit(1);
  }
  phpRow.execution = "executed";
  phpRow.backend = lane.backend;
  phpRow.assertions = lane.assertions.map(({ scenario: id, status }) => ({ scenario: id, status }));
  phpRow.capabilities = lane.capabilities;
  phpRow.blocking = [];
  process.stderr.write(
    JSON.stringify({ nativeLane: "php", assertions: lane.assertions.length, allPassed: true }) + "\n",
  );
}

// The profile the selected target runs under: the admission re-derives
// the verdict from the rows through the production core (`lekalo pilot
// admit`), so the report's own verdict claim cannot drift. The
// documented exit contract is enforced here: a strict refusal exits 4
// (status `unsupported`) while the receipt stays on stdout; a passing
// admission exits 0. Any other exit is a broken contract.
const tempRoot = mkdtempSync(join(tmpdir(), "lekalo-pilot-"));
const evidencePath = join(tempRoot, "evidence.json");
writeFileSync(evidencePath, JSON.stringify(evidence));
// The composition is the committed document itself, not a temp copy:
// its consumed attachments ride the composition directory, and the
// admission's custody join must see exactly those bytes. The digest
// equality above already proved the temp report pins these bytes.
const compositionPath = resolve(root, `${fixtureRoot}/attachments/pilot.json`);
let admissionExit;
let admission;
try {
  ({ exit: admissionExit, envelope: admission } = runJsonAllowUnsupported([
    "--json",
    "--no-cache",
    "pilot",
    "admit",
    evidencePath,
    compositionPath,
    "--profile",
    profile,
    "--project",
    project,
  ]));
} catch (error) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "admission-unavailable", detail: String(error).slice(0, 400) }, null, 2)}\n`,
  );
  process.exit(1);
}
const admissionVerdict =
  admission?.pilot?.verdict ??
  // A strict refusal rides the failure envelope: the receipt is the
  // embedded `payload` of the unsupported result.
  admission?.payload?.pilot?.verdict ??
  "unknown";
// The single-pass case runs (`--case <id>`) trim the report before it
// is written to the admission temp file, so the receipt verdict can be
// the report-level degraded/pass; only the all-cases run mirrors the
// committed unsupported report exactly. The contract: `unsupported`
// exits 4, every other verdict exits 0.
if (admissionVerdict === "unsupported" && admissionExit !== 4) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "admission-exit-contract", detail: `unsupported verdict exited ${admissionExit}, documented exit is 4` }, null, 2)}\n`,
  );
  process.exit(1);
}
if (admissionVerdict !== "unsupported" && admissionExit !== 0) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "admission-exit-contract", detail: `${profile} ${admissionVerdict} verdict exited ${admissionExit}, documented exit is 0` }, null, 2)}\n`,
  );
  process.exit(1);
}

const targetReport = evidence.targets.find((row) => row.target === target);
if (!targetReport) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "target-missing", target }, null, 2)}\n`,
  );
  process.exit(1);
}
const selectedScenarios = evidence.scenarios.filter(
  (row) => scenario === "all" || row.scenario === scenario,
);
if (selectedScenarios.length === 0 && scenario !== "all") {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "scenario-missing", scenario }, null, 2)}\n`,
  );
  process.exit(1);
}

// ---------------------------------------------------------------------------
// 4. The emitted report (schema-valid evidence document) on stdout.
// ---------------------------------------------------------------------------

const report = {
  ...evidence,
  profiles: profile === "strict" ? ["strict"] : ["strict", "permissive"],
};
// Trim to the selected case for a single-scenario run: every row that
// does not name the selected scenario is out of scope for this run.
if (scenario !== "all") {
  report.scenarios = selectedScenarios;
  for (const row of report.targets) {
    row.assertions = row.assertions.filter((item) => item.scenario === scenario);
  }
}
if (!validateEvidence(report)) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "report-invalid", errors: validateEvidence.errors }, null, 2)}\n`,
  );
  process.exit(1);
}
process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);

// ---------------------------------------------------------------------------
// 5. The verdict.
// ---------------------------------------------------------------------------

// The verdict comes from the admission receipt the harness just ran
// through the production core, never from the lane's self-report: the
// receipt's per-target verdict for the selected target decides, so
// admission-derived blockers (execution, backend provenance, assertion
// coverage, scenario conflicts) refuse the gate even when the lane's
// own capability rows claim otherwise.
const receiptTargets =
  admission?.pilot?.targets ?? admission?.payload?.pilot?.targets ?? [];
const selectedReceipt = receiptTargets.find((row) => row.target === target);
const receiptVerdict = selectedReceipt?.verdict ?? "unknown";
const strictPass =
  profile === "permissive" ||
  (admissionVerdict !== "unknown" && receiptVerdict === "pass");
const verdict = {
  ok: expectDenied ? true : strictPass,
  gate: "test-worklog-changelog-pilot",
  target,
  profile,
  case: scenario,
  composition: composition.pilot,
  staticChecks: staticResults.length,
  staticProven: staticResults.filter((row) => row.result === "pass").length,
  runtimeRetained: runtimeChecks,
  nativeLane: target === "php" ? { assertions: nativeResults.php.assertions.length, executed: true } : undefined,
  targetVerdict: targetReport.execution,
  receiptVerdict,
  admissionVerdict,
  admissionExit,
  expectedDenied: expectDenied,
  denialConfirmed: expectDenied ? !strictPass : undefined,
};
if (!strictPass && !expectDenied) {
  process.stderr.write(
    `${JSON.stringify({ ...verdict, ok: false, reason: "required-evidence-unsupported" }, null, 2)}\n`,
  );
  process.exit(1);
}
if (strictPass && expectDenied) {
  process.stderr.write(
    `${JSON.stringify({ ...verdict, ok: false, reason: "expected-denied-but-passed" }, null, 2)}\n`,
  );
  process.exit(1);
}
process.stderr.write(`${JSON.stringify(verdict, null, 2)}\n`);
process.exit(0);
