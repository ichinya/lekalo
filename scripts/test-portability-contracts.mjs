#!/usr/bin/env node
// Issue #19 contract gate: the four new portability schemas, the
// reviewed recipe instance, the diagnostic-registry union successor,
// the committed fixtures, and fresh production CLI output with an
// independent classification/arithmetic oracle.
//
// Exact Ajv 8.17.1 is provisioned outside this checkout (CI does the
// same) and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH. The live
// portability gate requires the built binary; a missing binary is gate
// failure, not skip.

import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  Ajv2020 = require("ajv/dist/2020").default;
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "ajv-unavailable",
    detail: error?.code ?? error?.message,
  }, null, 2)}\n`);
  process.exit(1);
}
if (ajvVersion !== "8.17.1") {
  fail("ajv-version", `expected 8.17.1, found ${ajvVersion}`);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(join(root, relative), "utf8"));
const readBytes = (relative) => readFileSync(join(root, relative));
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function fail(reason, detail) {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
}

const ajv = new Ajv2020({ strict: true, allErrors: true });

// 1. Every new schema compiles under strict Draft 2020-12.
const schemas = {
  targets: read("contracts/portability-targets.schema.v0.8.0.json"),
  evidence: read("contracts/portability-evidence.schema.v0.8.0.json"),
  recipeSchema: read("contracts/portability-recipe.schema.v0.8.0.json"),
  report: read("contracts/portability-report.schema.v0.8.0.json"),
  registry: read("contracts/diagnostic-registry.schema.v0.8.0.json"),
  diagnostic: read("contracts/diagnostic.schema.v0.2.16.json"),
};
const validate = {};
for (const [name, schema] of Object.entries(schemas)) {
  validate[name] = ajv.compile(schema);
}

// 2. The recipe instance validates and its digest matches the evidence pin.
const recipe = read("contracts/portability-recipe.v0.8.0.json");
if (!validate.recipeSchema(recipe)) fail("recipe-instance", validate.recipeSchema.errors);
const recipeDigest = `sha256:${sha256(readBytes("contracts/portability-recipe.v0.8.0.json"))}`;
if (recipe.schemaVersion !== "lekalo/portability-recipe/v0.8.0") fail("recipe-version", recipe.schemaVersion);
if (recipe.classification.precedence[0] !== "unsupported") fail("recipe-precedence", recipe.classification.precedence);

// 3. The committed diagnostic union: the complete 0.6.5 predecessor plus
//    exactly the nine registered portability codes, sorted, unique codes.
const predecessor = read("contracts/diagnostic-registry.v0.6.5.json");
const union = read("contracts/diagnostic-registry.v0.8.0.json");
if (!validate.registry(union)) fail("registry-instance", validate.registry.errors);
if (union.identity !== "dev.lekalo.diagnostic-registry@0.8.0") fail("registry-identity", union.identity);
if (union.registry_version !== "0.8.0") fail("registry-version", union.registry_version);
const predecessorIds = new Set(predecessor.entries.map((entry) => entry.id));
const unionIds = new Set(union.entries.map((entry) => entry.id));
if (union.entries.length !== predecessor.entries.length + 9) {
  fail("registry-size", `${union.entries.length} vs ${predecessor.entries.length} + 9`);
}
for (const entry of predecessor.entries) {
  if (!unionIds.has(entry.id)) fail("registry-dropped", entry.id);
  const successorEntry = union.entries.find((candidate) => candidate.id === entry.id);
  if (JSON.stringify(successorEntry) !== JSON.stringify(entry)) {
    fail("registry-mutated", entry.id);
  }
}
const portabilityEntries = union.entries.filter((entry) => entry.id.startsWith("portability."));
if (portabilityEntries.length !== 9) fail("portability-family", portabilityEntries.length);
const expectedCodes = {
  "portability.input-invalid": "LEK-PORT-001",
  "portability.version-unsupported": "LEK-PORT-002",
  "portability.evidence-incomplete": "LEK-PORT-003",
  "portability.mapping-partial": "LEK-PORT-004",
  "portability.capability-unsupported": "LEK-PORT-005",
  "portability.implementation-required": "LEK-PORT-006",
  "portability.behavior-loss": "LEK-PORT-007",
  "portability.analysis-incomplete": "LEK-PORT-008",
  "portability.check-denied": "LEK-PORT-009",
};
for (const [id, code] of Object.entries(expectedCodes)) {
  const entry = portabilityEntries.find((candidate) => candidate.id === id);
  if (!entry) fail("portability-missing", id);
  if (entry.code !== code) fail("portability-code", `${id}: ${entry.code}`);
  if (entry.lifecycle !== "active") fail("portability-lifecycle", id);
}
for (let index = 1; index < union.entries.length; index += 1) {
  if (union.entries[index].id <= union.entries[index - 1].id) {
    fail("registry-sort", union.entries[index].id);
  }
}

// 4. The committed fixtures validate against their schemas.
const targets = read("tests/fixtures/portability/planner/config/portability-targets.json");
const evidence = read("tests/fixtures/portability/planner/reports/portability-evidence.json");
if (!validate.targets(targets)) fail("targets-fixture", validate.targets.errors);
if (!validate.evidence(evidence)) fail("evidence-fixture", validate.evidence.errors);
if (evidence.recipeRef.digest !== recipeDigest) fail("evidence-recipe-pin", evidence.recipeRef.digest);
if (evidence.families.find((family) => family.family === "implementation").digest
    !== `sha256:${sha256(readBytes("tests/fixtures/portability/planner/reports/implementation.json"))}`) {
  fail("evidence-artifact-pin", "implementation.json");
}

// 5. The generator --check proves fixture/description coherence.
execFileSync(process.execPath, [join(root, "scripts/gen-portability-fixtures.mjs"), "--check"], {
  stdio: ["ignore", "pipe", "inherit"],
});

// 6. Fresh production runs: the built binary must exist and reproduce
//    the committed goldens byte-for-byte, twice (determinism).
const binary = join(root, "target/debug/lekalo")
  + (process.platform === "win32" ? ".exe" : "");
if (!existsSync(binary)) fail("binary-missing", binary);
const project = join(root, "tests/fixtures/portability/planner");
const run = (args, cwd = project) =>
  execFileSync(binary, args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] });
const tryRun = (args, cwd = project) => {
  try {
    return { code: 0, stdout: execFileSync(binary, args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }) };
  } catch (error) {
    return {
      code: error.status ?? 1,
      stdout: String(error.stdout ?? ""),
      stderr: String(error.stderr ?? ""),
    };
  }
};

const fresh = run(["--json", "portability", "--evidence", "reports/portability-evidence.json"]);
const freshAgain = run(["--json", "portability", "--evidence", "reports/portability-evidence.json"]);
if (fresh !== freshAgain) fail("nondeterministic", "repeated CLI bytes differ");
const golden = readBytes("tests/fixtures/portability/golden/report.envelope.json").toString("utf8");
if (fresh !== golden) fail("golden-drift", "report.envelope.json");
const deniedResult = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json", "--check"]);
if (deniedResult.code !== 3) fail("check-exit", String(deniedResult.code));
const deniedRun = deniedResult.stdout;
const goldenDenied = readBytes("tests/fixtures/portability/golden/check-denied.envelope.json").toString("utf8");
if (deniedRun !== goldenDenied) fail("golden-drift", "check-denied.envelope.json");

// 7. The independent oracle: recompute the report arithmetic from the
//    subject rows with plain integer math and compare to the summaries.
const report = JSON.parse(fresh).portability;
if (!validate.report(report)) fail("report-schema", validate.report.errors);
if (report.recipeRef.digest !== recipeDigest) fail("report-recipe-pin", report.recipeRef.digest);
// The coverage counters describe the emitted rows exactly (F10): the
// obligation budget is charged per emitted row, and non-applicable rows
// are counted wherever they are emitted.
{
  const allRows = report.subjects.flatMap((subject) => subject.rows);
  if (report.coverage.obligationRows !== allRows.length) {
    fail("coverage-rows", `${report.coverage.obligationRows} vs ${allRows.length}`);
  }
  const nonApplicable = allRows.filter((row) => row.classification === "non-applicable").length;
  if (report.coverage.nonApplicableRows !== nonApplicable) {
    fail("coverage-non-applicable", `${report.coverage.nonApplicableRows} vs ${nonApplicable}`);
  }
}
for (const summary of report.summaries) {
  for (const [dimension, entry] of Object.entries(summary.dimensions)) {
    const rows = report.subjects
      .flatMap((subject) => subject.rows)
      .filter((row) => row.destination === summary.destination && row.dimension === dimension);
    const buckets = { portable: 0, supportedWithMapping: 0, partial: 0, targetSpecific: 0, unsupported: 0, unknown: 0, nonApplicable: 0 };
    for (const row of rows) {
      if (row.classification === "portable") buckets.portable += 1;
      else if (row.classification === "supported-with-mapping") buckets.supportedWithMapping += 1;
      else if (row.classification === "partial") buckets.partial += 1;
      else if (row.classification === "target-specific") buckets.targetSpecific += 1;
      else if (row.classification === "unsupported") buckets.unsupported += 1;
      else if (row.classification === "unknown") buckets.unknown += 1;
      else buckets.nonApplicable += 1;
    }
    for (const [key, value] of Object.entries(buckets)) {
      if (entry[key] !== value) fail("summary-arithmetic", `${summary.destination}/${dimension}/${key}: ${entry[key]} vs ${value}`);
    }
    const applicable = rows.length - buckets.nonApplicable;
    if (entry.eligibleApplicable !== applicable) fail("summary-denominator", `${summary.destination}/${dimension}`);
    if (dimension === "structural") {
      if (applicable === 0) {
        if (entry.structuralPortable !== null) fail("ratio-zero-denominator", summary.destination);
      } else if (entry.structuralPortable.numerator !== buckets.portable
        || entry.structuralPortable.denominator !== applicable) {
        fail("ratio-arithmetic", `${summary.destination}: ${JSON.stringify(entry.structuralPortable)} vs ${buckets.portable}/${applicable}`);
      }
    }
    const verification = { untested: 0, recordedPass: 0, recordedFail: 0, notApplicable: 0 };
    for (const row of rows) {
      if (row.verification === "untested") verification.untested += 1;
      else if (row.verification === "recorded-pass") verification.recordedPass += 1;
      else if (row.verification === "recorded-fail") verification.recordedFail += 1;
      else verification.notApplicable += 1;
    }
    for (const [key, value] of Object.entries(verification)) {
      if (entry.verification[key] !== value) fail("verification-arithmetic", `${summary.destination}/${dimension}/${key}`);
    }
  }
  const blockers = report.subjects
    .flatMap((subject) => subject.rows)
    .filter((row) => row.destination === summary.destination && row.blocking).length;
  if (summary.blockers !== blockers) fail("blocker-count", summary.destination);
  const unknowns = report.subjects
    .flatMap((subject) => subject.rows)
    .filter((row) => row.destination === summary.destination && row.classification === "unknown").length;
  if (summary.unknowns !== unknowns) fail("unknown-count", summary.destination);
}

// 8. Unsupported never collapses into unknown: the Go schedule hook is
//    an explicit, blocking verdict from the complete inventory.
const schedule = report.subjects.find((subject) => subject.subject === "command:planner.schedule_calculate");
const goSchedule = schedule.rows.find((row) => row.destination === "go-service" && row.dimension === "behavioral");
if (goSchedule.classification !== "unsupported" || goSchedule.binding !== "missing" || !goSchedule.blocking) {
  fail("go-schedule-state", JSON.stringify(goSchedule));
}
const highStructural = report.summaries.find((summary) => summary.destination === "go-service");
if (highStructural.dimensions.structural.portable === 0 || highStructural.blockers === 0) {
  fail("ratio-concealment", "the fixture must keep high structural portability beside the behavioral blocker");
}

// 9. The planning rows round-trip through a machine consumer: blockers,
//    required implementations, storage reviews, and affected joins are
//    collectable without parsing human prose.
const consumer = { blockers: [], implementHooks: [], storageReviews: [], verifyScenarios: [], affectedScenarios: report.affectedScenarios.map((scenario) => scenario.scenario), affectedGates: report.affectedGates.map((gate) => gate.gate) };
for (const row of report.migrationSurface.rows) {
  if (row.action === "implement-hook") consumer.implementHooks.push(row);
  if (row.action === "review-storage-policy") consumer.storageReviews.push(row);
  if (row.action === "verify-scenario") consumer.verifyScenarios.push(row);
}
for (const subject of report.subjects) {
  for (const row of subject.rows) {
    if (row.blocking) consumer.blockers.push({ subject: subject.subject, destination: row.destination, dimension: row.dimension, reason: row.reason });
  }
}
if (consumer.implementHooks.length === 0) fail("consumer-implement-hooks", "planning rows must name the missing Go hook");
if (!consumer.implementHooks.some((row) => row.subject === "command:planner.schedule_calculate" && row.destination === "go-service")) {
  fail("consumer-missing-go-hook", "the planning consumer cannot find the Go schedule hook row");
}
if (consumer.storageReviews.length === 0) fail("consumer-storage", "the precision-loss review row is missing");
if (consumer.blockers.length !== report.migrationSurface.unresolvedObligations) {
  fail("consumer-blockers", `${consumer.blockers.length} vs ${report.migrationSurface.unresolvedObligations}`);
}
const rowIds = new Set(report.migrationSurface.rows.map((row) => row.id));
if (rowIds.size !== report.migrationSurface.rows.length) fail("row-id-duplicates", "planning row ids must be unique");

// The committed consumer receipt: recompute the whole projection from
// the fresh report and compare byte-for-byte with the fixture.
const consumerReceipt = {
  schemaVersion: "lekalo/portability-report/v0.8.0",
  consumer: "migration-planning/1",
  blockers: [],
  requiredImplementations: [],
  storageReviews: [],
  verificationTasks: [],
  affectedScenarios: report.affectedScenarios.map((s) => ({ scenario: s.scenario, state: s.state, subjects: s.subjects })),
  affectedGates: report.affectedGates.map((g) => ({ gate: g.gate, state: g.state, scenarios: g.scenarios })),
};
for (const subject of report.subjects) {
  for (const row of subject.rows) {
    if (row.blocking) consumerReceipt.blockers.push({ subject: subject.subject, destination: row.destination, dimension: row.dimension, reason: row.reason });
  }
}
for (const row of report.migrationSurface.rows) {
  if (row.action === "implement-hook") consumerReceipt.requiredImplementations.push({ id: row.id, subject: row.subject, destination: row.destination });
  if (row.action === "review-storage-policy") {
    const storageRow = report.subjects.find((s) => s.subject === row.subject).rows.find((r) => r.destination === row.destination && r.dimension === "storage");
    consumerReceipt.storageReviews.push({ id: row.id, subject: row.subject, destination: row.destination, required: storageRow.required });
  }
  if (row.action === "verify-scenario") consumerReceipt.verificationTasks.push({ id: row.id, subject: row.subject, destination: row.destination });
}
consumerReceipt.totals = {
  blockers: consumerReceipt.blockers.length,
  requiredImplementations: consumerReceipt.requiredImplementations.length,
  storageReviews: consumerReceipt.storageReviews.length,
  verificationTasks: consumerReceipt.verificationTasks.length,
  unresolvedObligations: report.migrationSurface.unresolvedObligations,
};
const committedConsumer = read("tests/fixtures/portability/planning/consumer-receipt.json");
if (JSON.stringify(committedConsumer) !== JSON.stringify(consumerReceipt)) {
  fail("consumer-receipt-drift", "tests/fixtures/portability/planning/consumer-receipt.json");
}

// 10. The invalid controls are refused with the expected reason token,
//     exercised through the real binary in a temporary project copy.
//     `schemaInvalid` names the controls whose committed bytes must ALSO
//     fail exact Ajv validation (both directions of the contract
//     boundary); `null` skips the schema direction (duplicate keys are
//     invisible to a JSON parser).
const controls = [
  ["invalid/duplicate-keys.evidence.json", "duplicate-key", 1, null],
  ["invalid/stale-project-pin.evidence.json", "project-pin", 1, true],
  ["invalid/recipe-pin.evidence.json", "recipe-pin", 1, true],
  ["invalid/unknown-source.targets.json", "default-source", 1, true],
  ["invalid/duplicate-binding.targets.json", "target-binding", 1, true],
  ["invalid/stale-profile-pin.targets.json", "profile-pin", 1, true],
  ["invalid/unknown-member.evidence.json", "closed-shape", 1, false],
  ["invalid/omitted-nullable.evidence.json", "closed-shape", 1, false],
  ["invalid/path-traversal.evidence.json", "artifact-ref", 1, false],
  ["invalid/duplicate-defaults.targets.json", "default-destination", 1, false],
  ["invalid/empty-targets.targets.json", "target-binding", 1, false],
  ["invalid/adapter-missing-version.evidence.json", "adapter-snapshot", 1, false],
  ["invalid/receipt-conflict.evidence.json", "duplicate-receipt", 1, true],
  ["invalid/bad-family-digest.evidence.json", "family-digest", 1, true],
];
for (const [relative, reason, expectedExit, schemaValid] of controls) {
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    const target = relative.endsWith(".targets.json")
      ? join(temp, "config/portability-targets.json")
      : join(temp, "reports/portability-evidence.json");
    cpSync(join(root, "tests/fixtures/portability", relative), target, { force: true });
    const refused = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
    if (refused.code !== expectedExit) fail("control-exit", `${relative}: ${refused.code}`);
    const diagnosticsText = refused.stderr + refused.stdout;
    if (!diagnosticsText.includes("portability.input-invalid")) fail("control-diagnostic", relative);
    // The named check, not just an arbitrary input refusal: the reason
    // token must be the exact one the control was authored for.
    if (!diagnosticsText.includes(`"${reason}"`)) fail("control-reason", `${relative}: expected ${reason}`);
    if (schemaValid === false) {
      // Both directions: the schema must reject the same bytes the
      // production decoder refuses.
      const document = JSON.parse(readFileSync(target, "utf8"));
      const validateControl = relative.endsWith(".targets.json") ? validate.targets : validate.evidence;
      if (validateControl(document)) fail("control-schema-admits", relative);
    } else if (schemaValid === true) {
      // The refusal is semantic (pins, conflicts, digest custody): the
      // schema admits the shape and the production decoder refuses it.
      const document = JSON.parse(readFileSync(target, "utf8"));
      const validateControl = relative.endsWith(".targets.json") ? validate.targets : validate.evidence;
      if (!validateControl(document)) fail("control-schema-rejects", relative);
    }
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

// 10b. The cross-target mapping control: a mapping re-scoped onto
// go-service whose capability the go profile never declares at full
// support must keep the go schedule hook blocked — never a green
// `supported-with-mapping` fabricated from a foreign recipe.
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    cpSync(
      join(root, "tests/fixtures/portability/invalid/cross-target-mapping.evidence.json"),
      join(temp, "reports/portability-evidence.json"),
      { force: true },
    );
    const leaked = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
    if (leaked.code !== 0) fail("control-exit", `cross-target-mapping: ${leaked.code}`);
    const report = JSON.parse(leaked.stdout).portability;
    const schedule = report.subjects.find((s) => s.subject === "command:planner.schedule_calculate");
    const goRow = schedule.rows.find((r) => r.destination === "go-service" && r.dimension === "behavioral");
    if (goRow.classification === "supported-with-mapping") fail("cross-target-leak", JSON.stringify(goRow));
    if (!goRow.blocking) fail("cross-target-green", JSON.stringify(goRow));
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

// 10c. Finding-class behavior probes: each repaired class keeps a live
// gate probe that would fail on regression.
const withEvidence = (temp, mutate) => {
  const manifest = JSON.parse(readFileSync(join(temp, "reports/portability-evidence.json"), "utf8"));
  mutate(manifest);
  writeFileSync(join(temp, "reports/portability-evidence.json"), JSON.stringify(manifest, null, 2) + "\n");
};

// F1 — semantic mapping admission: a record that stops describing the
// obligation (foreign requirement, invented conditions, fabricated pins)
// never credits the hook; and a partial claim reconciles a full one
// conservatively instead of any-full-wins.
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    cpSync(join(root, "tests/fixtures/portability/invalid/mapping-unqualified.evidence.json"), join(temp, "reports/portability-evidence.json"), { force: true });
    const run = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
    if (run.code !== 0) fail("f1-unqualified-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    const schedule = report.subjects.find((s) => s.subject === "command:planner.schedule_calculate");
    const phpRow = schedule.rows.find((r) => r.destination === "php-laravel" && r.dimension === "behavioral");
    if (phpRow.classification === "supported-with-mapping") fail("f1-foreign-credit", JSON.stringify(phpRow));
    if (!phpRow.blocking) fail("f1-foreign-green", JSON.stringify(phpRow));
    if (phpRow.reason !== "hook-foreign-unmapped") fail("f1-foreign-reason", phpRow.reason);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    cpSync(join(root, "tests/fixtures/portability/invalid/mapping-conflict.evidence.json"), join(temp, "reports/portability-evidence.json"), { force: true });
    const run = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
    if (run.code !== 0) fail("f1-conflict-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    const reading = report.subjects.find((s) => s.subject === "query:planner.count_focused");
    const goRow = reading.rows.find((r) => r.destination === "go-service" && r.dimension === "behavioral");
    if (goRow.classification !== "partial") fail("f1-conflict-reconcile", JSON.stringify(goRow));
    if (!goRow.blocking) fail("f1-conflict-green", JSON.stringify(goRow));
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

// F2 — declared family completeness without verified artifact custody is
// an evidence gap: the decimal obligation stays unknown and blocking,
// never portable.
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    cpSync(join(root, "tests/fixtures/portability/invalid/forged-storage.evidence.json"), join(temp, "reports/portability-evidence.json"), { force: true });
    const run = tryRun(["--json", "portability", "--to", "go-service", "--symbol", "planner.weight", "--evidence", "reports/portability-evidence.json"], temp);
    if (run.code !== 0) fail("f2-gap-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    const weight = report.subjects.find((s) => s.subject === "scalar:planner.weight");
    const storageRow = weight.rows.find((r) => r.destination === "go-service" && r.dimension === "storage");
    if (storageRow.classification !== "unknown") fail("f2-forged-portable", JSON.stringify(storageRow));
    if (!storageRow.blocking) fail("f2-forged-green", JSON.stringify(storageRow));
    const denied = tryRun(["--json", "portability", "--to", "go-service", "--symbol", "planner.weight", "--evidence", "reports/portability-evidence.json", "--check"], temp);
    if (denied.code !== 3) fail("f2-check-denied", String(denied.code));
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

// F3/F4 — receipts: a fabricated manifest cannot upgrade a join to
// executed, an unqualified claim surfaces as a named gap, and a
// qualified recorded failure dominates a pass.
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    cpSync(join(root, "tests/fixtures/portability/invalid/forged-receipt.evidence.json"), join(temp, "reports/portability-evidence.json"), { force: true });
    const run = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
    if (run.code !== 0) fail("f3-forged-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    const joined = report.affectedScenarios.find((s) => s.scenario === "planner.schedule_flow");
    if (joined.state === "executed" || joined.state === "failed") fail("f3-fabricated-executed", JSON.stringify(joined));
    if (!report.diagnostics.some((d) => d.code === "portability.evidence-incomplete" && d.detail === "receipt-unqualified")) {
      fail("f3-gap-silent", JSON.stringify(report.diagnostics));
    }
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    withEvidence(temp, (manifest) => {
      const focus = manifest.scenarios.find((s) => s.scenario === "planner.focus_flow");
      focus.state = "recorded-fail";
      manifest.scenarios.push({
        scenario: "planner.focus_flow",
        targetId: "go-service",
        state: "recorded-pass",
        receiptRef: focus.receiptRef,
        receiptDigest: focus.receiptDigest,
      });
    });
    const run = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
    if (run.code !== 0) fail("f4-dominance-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    const joined = report.affectedScenarios.find((s) => s.scenario === "planner.focus_flow");
    if (joined.state !== "failed") fail("f4-failure-masked", JSON.stringify(joined));
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

// F5 — one storage row per policy obligation: a second requirement keeps
// its own exact pair instead of overwriting the loss explanation.
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    withEvidence(temp, (manifest) => {
      manifest.storageRequired.push({ subject: "scalar:planner.weight", policy: { kind: "isolation", value: "repeatable-read" } });
      manifest.storageOffered.push({ targetId: "go-service", policy: { kind: "isolation", value: "repeatable-read" } });
    });
    const run = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
    if (run.code !== 0) fail("f5-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    const weight = report.subjects.find((s) => s.subject === "scalar:planner.weight");
    const phpRows = weight.rows.filter((r) => r.destination === "php-laravel" && r.dimension === "storage");
    if (phpRows.length !== 2) fail("f5-row-per-policy", String(phpRows.length));
    const decimalRow = phpRows.find((r) => r.required && r.required.kind === "decimal");
    if (!decimalRow || decimalRow.classification !== "partial" || decimalRow.reason !== "storage-policy-loss") {
      fail("f5-loss-explained", JSON.stringify(decimalRow));
    }
    if (decimalRow.offered.precision !== 10 || decimalRow.offered.scale !== 2) fail("f5-witness-pair", JSON.stringify(decimalRow.offered));
    const isolationRow = phpRows.find((r) => r.required && r.required.kind === "isolation");
    if (!isolationRow || isolationRow.classification !== "unknown") fail("f5-second-obligation", JSON.stringify(isolationRow));
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

// F6/F7 — scoped reports join dependency-witness scenarios and planning
// rows carry the gates of their covering scenarios.
{
  const run = tryRun(["--json", "portability", "--to", "go-service", "--symbol", "planner.task", "--evidence", "reports/portability-evidence.json"]);
  if (run.code !== 0) fail("f6-exit", String(run.code));
  const report = JSON.parse(run.stdout).portability;
  if (report.affectedScenarios.length === 0) fail("f6-witness-joins-missing", "affectedScenarios empty");
  if (!report.affectedScenarios.some((s) => s.scenario === "planner.schedule_flow")) {
    fail("f6-dependency-witness", JSON.stringify(report.affectedScenarios));
  }
  if (report.affectedGates.length === 0) fail("f6-gates-missing", "affectedGates empty");
  const projectRun = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"]);
  const projectReport = JSON.parse(projectRun.stdout).portability;
  const hookRow = projectReport.migrationSurface.rows.find(
    (row) => row.subject === "command:planner.schedule_calculate" && row.destination === "go-service" && row.action === "implement-hook",
  );
  if (!hookRow || hookRow.gates.length === 0) fail("f7-planning-gates", JSON.stringify(hookRow));
  if (!hookRow.scenarios.includes("planner.schedule_flow")) fail("f7-planning-scenarios", JSON.stringify(hookRow));
}

// F8 — distinct target ids never collide in planning identity: a legal
// alias target keeps its own rows.
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    const config = JSON.parse(readFileSync(join(temp, "config/portability-targets.json"), "utf8"));
    const alias = JSON.parse(JSON.stringify(config.targets.find((t) => t.targetId === "go-service")));
    alias.targetId = "goservice";
    config.targets.push(alias);
    config.defaultDestinations = ["go-service", "goservice", "php-laravel"];
    writeFileSync(join(temp, "config/portability-targets.json"), JSON.stringify(config, null, 2) + "\n");
    // Both alias targets must stay blocking: strip the offered policies
    // so each storage row is an unresolved unknown.
    withEvidence(temp, (manifest) => {
      manifest.storageOffered = [];
    });
    const run = tryRun(["--json", "portability", "--symbol", "planner.weight", "--evidence", "reports/portability-evidence.json"], temp);
    if (run.code !== 0) fail("f8-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    const goRow = report.migrationSurface.rows.find((r) => r.destination === "go-service" && r.subject === "scalar:planner.weight" && r.dimension === "storage");
    const aliasRow = report.migrationSurface.rows.find((r) => r.destination === "goservice" && r.subject === "scalar:planner.weight" && r.dimension === "storage");
    if (!goRow || !aliasRow) fail("f8-dropped-work", JSON.stringify({ goRow, aliasRow }));
    if (goRow.id === aliasRow.id) fail("f8-id-collision", `${goRow.id} vs ${aliasRow.id}`);
    const ids = report.migrationSurface.rows.map((row) => row.id);
    if (new Set(ids).size !== ids.length) fail("f8-duplicate-ids", "planning row ids must stay unique");
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

// F9 — an observed-unsupported hook never hides sibling operational
// obligations, and every unmapped requirement keeps its identity.
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    withEvidence(temp, (manifest) => {
      manifest.operationalRequirements.push({ subject: "command:planner.focus_task", requirement: "verify.operations" });
      manifest.operationalRequirements.push({ subject: "command:planner.focus_task", requirement: "unmapped.requirement" });
    });
    const run = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
    if (run.code !== 0) fail("f9-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    const focus = report.subjects.find((s) => s.subject === "command:planner.focus_task");
    const goRows = focus.rows.filter((r) => r.destination === "go-service" && r.dimension === "operational");
    if (goRows.length < 3) fail("f9-siblings-dropped", JSON.stringify(goRows));
    const hook = goRows.find((r) => r.reason === "implicit-hook-unsupported");
    if (!hook) fail("f9-hook-row", JSON.stringify(goRows));
    const unmapped = goRows.filter((r) => r.evidenceRefs.includes("unmapped.requirement"));
    if (unmapped.length !== 1 || !unmapped[0].blocking) fail("f9-identity-lost", JSON.stringify(goRows));
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

// F11 — the bare no-destination assessment emits the structural
// inventory plus the explicit destination-evidence gap, and the strict
// gate denies it instead of ever producing a vacuous green report.
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    const config = JSON.parse(readFileSync(join(temp, "config/portability-targets.json"), "utf8"));
    delete config.defaultDestinations;
    config.targets = config.targets.filter((t) => t.targetId === "node-typescript");
    writeFileSync(join(temp, "config/portability-targets.json"), JSON.stringify(config, null, 2) + "\n");
    const run = tryRun(["--json", "portability"], temp);
    if (run.code !== 0) fail("f11-inventory-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    if (report.destinations.length !== 0) fail("f11-fabricated-destination", JSON.stringify(report.destinations));
    if (report.subjects.length === 0) fail("f11-inventory-empty", "no structural inventory");
    if (!report.diagnostics.some((d) => d.code === "portability.evidence-incomplete" && d.detail === "destination-evidence-gap")) {
      fail("f11-gap-silent", JSON.stringify(report.diagnostics));
    }
    const denied = tryRun(["--json", "portability", "--check"], temp);
    if (denied.code !== 3) fail("f11-check-denied", String(denied.code));
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

// F12 — the canonical lock is decoded and reconciled: garbage bytes
// refuse instead of reporting known custody.
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    writeFileSync(join(temp, "lekalo.lock"), "this is not a lock\n");
    const refused = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
    if (refused.code !== 1) fail("f12-lock-garbage", String(refused.code));
    if (!(refused.stderr + refused.stdout).includes('"lock-shape"')) fail("f12-lock-reason", refused.stderr);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

// F14 — a sole declared Model target outside the configuration universe
// is refused instead of reporting pinned provenance with no pin.
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    const bindings = join(temp, "lekalo/modules/planner/bindings.yaml");
    const text = readFileSync(bindings, "utf8")
      .replace(/target: (node-typescript|php-laravel|go-service)/g, "target: orphan-tgt");
    writeFileSync(bindings, text);
    const config = JSON.parse(readFileSync(join(temp, "config/portability-targets.json"), "utf8"));
    delete config.defaultSource;
    writeFileSync(join(temp, "config/portability-targets.json"), JSON.stringify(config, null, 2) + "\n");
    const refused = tryRun(["--json", "portability"], temp);
    if (refused.code !== 1) fail("f14-unconfigured-source", String(refused.code));
    if (!(refused.stderr + refused.stdout).includes('"target-unknown"')) fail("f14-reason", refused.stderr);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

// Self-assessment refusal: `--to <source>` is not an assessment.
{
  const refused = tryRun(["--json", "portability", "--to", "node-typescript", "--evidence", "reports/portability-evidence.json"]);
  if (refused.code !== 1) fail("self-assessment-exit", String(refused.code));
  if (!(refused.stderr + refused.stdout).includes('"destination-source"')) fail("self-assessment-reason", refused.stderr);
}

// 11. The strict --check denial: exit 3, status denied, report preserved.
const denied = JSON.parse(deniedRun);
if (denied.status !== "denied") fail("check-status", denied.status);
if (denied.payload.portability.schemaVersion !== "lekalo/portability-report/v0.8.0") {
  fail("check-report-preserved", denied.payload.portability.schemaVersion);
}
if (!denied.reasonCodes.includes("portability.check-denied")) fail("check-reason", JSON.stringify(denied.reasonCodes));

process.stdout.write(`${JSON.stringify({
  ok: true,
  gate: "portability-contracts",
  schemas: Object.keys(schemas).length,
  registryEntries: union.entries.length,
  portabilityCodes: portabilityEntries.length,
  subjects: report.subjects.length,
  planningRows: report.migrationSurface.rows.length,
  controls: controls.length,
})}\n`);
