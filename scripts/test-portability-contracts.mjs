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
import "./registry-union.mjs";
import { execFileSync } from "node:child_process";
import { appendFileSync, copyFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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
  family: read("contracts/portability-family.schema.v0.8.0.json"),
  receipt: read("contracts/portability-receipt.schema.v0.8.0.json"),
  adapterSnapshot: read("contracts/portability-adapter-snapshot.schema.v0.8.0.json"),
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
//    the nine registered portability codes, sorted, unique codes. The
//    union may carry the other landed 0.8.0 families (`format.*`,
//    `adoption.*`, `worklog-changelog.*`, `module-api.*`) additively.
const predecessor = read("contracts/diagnostic-registry.v0.6.5.json");
const union = read("contracts/diagnostic-registry.v0.8.0.json");
if (!validate.registry(union)) fail("registry-instance", validate.registry.errors);
if (union.identity !== "dev.lekalo.diagnostic-registry@0.8.0") fail("registry-identity", union.identity);
if (union.registry_version !== "0.8.0") fail("registry-version", union.registry_version);
const predecessorIds = new Set(predecessor.entries.map((entry) => entry.id));
const unionIds = new Set(union.entries.map((entry) => entry.id));
if (union.entries.length !== unionIds.size) {
  fail("registry-size", `${union.entries.length} entries vs ${unionIds.size} unique ids`);
}
if ([...predecessorIds].some((id) => !unionIds.has(id))) {
  fail("registry-size", "union dropped a predecessor id");
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
    // Subject-based oracle: the recipe's class counts are a partition of
    // eligible applicable SUBJECTS. A subject with several obligations
    // in one dimension counts once, at its conservative primary class
    // (recipe precedence), while every row stays in the report.
    const camel = {
      "portable": "portable",
      "supported-with-mapping": "supportedWithMapping",
      "partial": "partial",
      "target-specific": "targetSpecific",
      "unsupported": "unsupported",
      "unknown": "unknown",
      "non-applicable": "nonApplicable",
    };
    const rankOf = {
      "unsupported": 0,
      "partial": 1,
      "unknown": 2,
      "target-specific": 3,
      "supported-with-mapping": 4,
      "portable": 5,
    };
    const verificationRank = { "recorded-fail": 3, "recorded-pass": 2, "untested": 1, "not-applicable": 0 };
    const verificationCamel = {
      "untested": "untested",
      "recorded-pass": "recordedPass",
      "recorded-fail": "recordedFail",
      "not-applicable": "notApplicable",
    };
    const subjectsHere = report.subjects
      .map((subject) => ({
        rows: subject.rows.filter(
          (row) => row.destination === summary.destination && row.dimension === dimension,
        ),
      }))
      .filter((entry) => entry.rows.length > 0);
    const buckets = { portable: 0, supportedWithMapping: 0, partial: 0, targetSpecific: 0, unsupported: 0, unknown: 0, nonApplicable: 0 };
    const verification = { untested: 0, recordedPass: 0, recordedFail: 0, notApplicable: 0 };
    for (const { rows } of subjectsHere) {
      const applicable = rows.filter((row) => row.classification !== "non-applicable");
      if (applicable.length === 0) {
        buckets.nonApplicable += 1;
        continue;
      }
      const primary = applicable
        .map((row) => row.classification)
        .reduce((worst, classification) => (rankOf[classification] < rankOf[worst] ? classification : worst));
      buckets[camel[primary]] += 1;
      const worstVerification = applicable
        .map((row) => row.verification)
        .reduce((worst, candidate) => (verificationRank[candidate] > verificationRank[worst] ? candidate : worst));
      verification[verificationCamel[worstVerification]] += 1;
    }
    for (const [key, value] of Object.entries(buckets)) {
      if (entry[key] !== value) fail("summary-arithmetic", `${summary.destination}/${dimension}/${key}: ${entry[key]} vs ${value}`);
    }
    const applicableSubjects = subjectsHere.length - buckets.nonApplicable;
    if (entry.eligibleApplicable !== applicableSubjects) {
      fail("summary-denominator", `${summary.destination}/${dimension}: ${entry.eligibleApplicable} vs ${applicableSubjects} subjects`);
    }
    if (dimension === "structural") {
      if (applicableSubjects === 0) {
        if (entry.structuralPortable !== null) fail("ratio-zero-denominator", summary.destination);
      } else if (entry.structuralPortable.numerator !== buckets.portable
        || entry.structuralPortable.denominator !== applicableSubjects) {
        fail("ratio-arithmetic", `${summary.destination}: ${JSON.stringify(entry.structuralPortable)} vs ${buckets.portable}/${applicableSubjects}`);
      }
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
  // Custom hooks count distinct primary carriers (command|query
  // subjects); an effect joining its trigger's custom outcome never
  // doubles one hook.
  const customHooks = new Set(
    report.subjects
      .filter((subject) => subject.subject.startsWith("command:") || subject.subject.startsWith("query:"))
      .filter((subject) =>
        subject.rows.some(
          (row) =>
            row.destination === summary.destination
            && row.classification === "supported-with-mapping"
            && row.binding === "custom",
        ),
      )
      .map((subject) => subject.subject),
  );
  if (summary.customObligations !== customHooks.size) {
    fail("custom-hook-count", `${summary.destination}: ${summary.customObligations} vs ${customHooks.size}`);
  }
}

// 8. Unsupported never collapses into unknown: the Go schedule hook is
//    an explicit, blocking verdict from the complete inventory — and one
//    absent hook reached through its command and its triggering effect
//    counts exactly one missing implementation (the effect rows keep
//    their own planning rows, never their own implementation count).
const schedule = report.subjects.find((subject) => subject.subject === "command:planner.schedule_calculate");
const goSchedule = schedule.rows.find((row) => row.destination === "go-service" && row.dimension === "behavioral");
if (goSchedule.classification !== "unsupported" || goSchedule.binding !== "missing" || !goSchedule.blocking) {
  fail("go-schedule-state", JSON.stringify(goSchedule));
}
const effectSchedule = report.subjects.find((subject) => subject.subject === "effect:planner.schedule_write");
const goEffectSchedule = effectSchedule
  ? effectSchedule.rows.find((row) => row.destination === "go-service" && row.dimension === "behavioral")
  : undefined;
const goImplementHookRows = report.migrationSurface.rows.filter(
  (row) => row.destination === "go-service" && row.action === "implement-hook",
);
if (goEffectSchedule && goEffectSchedule.classification === "unsupported") {
  if (goImplementHookRows.length < 2) fail("effect-row-dropped", JSON.stringify(goImplementHookRows));
}
if (report.migrationSurface.missingImplementations !== 1) {
  fail("missing-hook-double-count", String(report.migrationSurface.missingImplementations));
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
  ["invalid/defaults-null.targets.json", "closed-shape", 1, false],
  ["invalid/revision-grammar.evidence.json", "recorded-context", 1, false],
  ["invalid/policy-precision-zero.evidence.json", "policy-shape", 1, false],
  ["invalid/policy-kind-evil.evidence.json", "policy-shape", 1, false],
  ["invalid/duplicate-required.evidence.json", "duplicate-requirement", 1, false],
  ["invalid/duplicate-offer.evidence.json", "duplicate-offer", 1, true],
  ["invalid/adapter-bad-version.evidence.json", "adapter-snapshot", 1, false],
  ["invalid/mapping-identity.evidence.json", "mapping-identity", 1, false],
  ["invalid/duplicate-storage-ref.targets.json", "duplicate-storage-ref", 1, false],
  ["invalid/duplicate-conditions.evidence.json", "mapping-conditions", 1, false],
  ["invalid/requirement-grammar.evidence.json", "operational-requirement", 1, false],
  ["invalid/gate-fabricated-executed.evidence.json", "gate-custody", 1, false],
  ["invalid/receipt-result-contradiction.evidence.json", "receipt-result", 1, true],
  ["invalid/gate-authored-failed.evidence.json", "gate-state", 1, false],
  ["invalid/gate-omitted-nullables.evidence.json", "closed-shape", 1, false],
  ["invalid/gate-bad-custody-grammar.evidence.json", "gate-identity", 1, false],
  ["invalid/policy-null-precision.evidence.json", "policy-shape", 1, false],
  ["invalid/duplicate-hooks.evidence.json", "duplicate-hook", 1, false],
  ["invalid/absent-adapter-bad-version.evidence.json", "adapter-snapshot", 1, false],
  ["invalid/receipt-ref-trailing-dash.evidence.json", "artifact-ref", 1, false],
  ["invalid/receipt-ref-device-stem.evidence.json", "artifact-ref", 1, false],
  ["invalid/storage-ref-over-length.targets.json", "artifact-ref", 1, false],
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

// F1b — mapping precondition/pin admission: an empty condition list is
// vacuously true and never credits, another target's precondition
// vocabulary never admits this destination, a cherry-picked subset is
// unreviewed, and a contradictory definition pin poisons the credit —
// every variant must stay blocked instead of `supported-with-mapping`.
{
  const admissionCases = [
    // empty conditions
    (mapping) => { mapping.conditions = []; },
    // foreign target's vocabulary
    (mapping) => { mapping.conditions = ["php-84", "laravel-11"]; },
    // cherry-picked subset
    (mapping) => { mapping.conditions = ["go-122"]; },
    // contradictory definition pin beside the correct one
    (mapping) => { mapping.definitionPins.push({ id: "storage.returning", version: "99.0.0" }); },
  ];
  for (const [index, mutate] of admissionCases.entries()) {
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      withEvidence(temp, (manifest) => {
        mutate(manifest.mappings.find((m) => m.id === "map.reading-go"));
      });
      const run = tryRun(["--json", "portability", "--to", "go-service", "--symbol", "planner.count_focused", "--evidence", "reports/portability-evidence.json"], temp);
      if (run.code !== 0) fail("f1b-exit", `${index}: ${run.code}`);
      const report = JSON.parse(run.stdout).portability;
      const reading = report.subjects.find((s) => s.subject === "query:planner.count_focused");
      const goRow = reading.rows.find((r) => r.destination === "go-service" && r.dimension === "behavioral");
      if (goRow.classification === "supported-with-mapping") fail("f1b-vacuous-credit", `${index}: ${JSON.stringify(goRow)}`);
      if (!goRow.blocking) fail("f1b-green", `${index}: ${JSON.stringify(goRow)}`);
      if (goRow.reason !== "mapping-unqualified") fail("f1b-reason", `${index}: ${goRow.reason}`);
      const denied = tryRun(["--json", "portability", "--to", "go-service", "--symbol", "planner.count_focused", "--evidence", "reports/portability-evidence.json", "--check"], temp);
      if (denied.code !== 3) fail("f1b-check", `${index}: ${denied.code}`);
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }
}

// F1c — operational requirements go through the same bridge admission: a
// bridged requirement without the reviewed preconditions and pin is an
// explicit unqualified gap, never portable capability credit.
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    withEvidence(temp, (manifest) => {
      manifest.operationalRequirements.push({
        subject: "command:planner.focus_task",
        requirement: "verify.operations",
        conditions: [],
        definitionPins: [],
      });
      // A php-side profile declares the capability at full support; the
      // unqualified record must still not read as portable.
      manifest.operationalRequirements.push({
        subject: "command:planner.focus_task",
        requirement: "storage.transactions",
        conditions: [],
        definitionPins: [],
      });
    });
    const run = tryRun(["--json", "portability", "--to", "php-laravel", "--symbol", "planner.focus_task", "--evidence", "reports/portability-evidence.json"], temp);
    if (run.code !== 0) fail("f1c-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    const focus = report.subjects.find((s) => s.subject === "command:planner.focus_task");
    const phpRows = focus.rows.filter((r) => r.destination === "php-laravel" && r.dimension === "operational");
    for (const row of phpRows) {
      if (row.classification === "portable") fail("f1c-unearned-portable", JSON.stringify(row));
    }
    const unqualified = phpRows.filter((r) => r.reason === "requirement-unqualified");
    if (unqualified.length !== 2) fail("f1c-unqualified-gap", JSON.stringify(phpRows));
    // The authored requirement identity survives on the row.
    if (!phpRows.some((r) => r.evidenceRefs.includes("storage.transactions"))) {
      fail("f1c-identity-lost", JSON.stringify(phpRows));
    }
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

// F2b — a declared-complete family artifact is decoded through its
// closed owner envelope: a digest-matching `{}` can never certify a
// complete inventory, a foreign project pin refuses, the envelope's
// complete inventory must equal the manifest's declared inventory, and
// a reconciled envelope with no restricted policy yields honest
// portable rows.
{
  const envelope = (projectId, inventory, irPinValue) => ({
    schemaVersion: "lekalo/portability-family/v0.8.0",
    identity: "dev.lekalo.portability-family@0.8.0",
    family: "storage",
    projectRef: {
      projectId,
      modelVersion: "0.2.16",
      irDigest: irPinValue,
    },
    storageRequired: inventory,
  });
  const withStorageFamily = (temp, artifactDocument) => {
    withEvidence(temp, (manifest) => {
      const family = manifest.families.find((entry) => entry.family === "storage");
      family.complete = true;
      family.artifactRef = "reports/storage.json";
      family.digest = `sha256:${sha256(Buffer.from(JSON.stringify(artifactDocument, null, 2) + "\n"))}`;
    });
    writeFileSync(join(temp, "reports/storage.json"), JSON.stringify(artifactDocument, null, 2) + "\n");
  };
  // 1. a digest-matching content-free object refuses as a family shape
  {
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      withEvidence(temp, (manifest) => {
        manifest.storageRequired = [];
      });
      withStorageFamily(temp, {});
      const refused = tryRun(["--json", "portability", "--to", "go-service", "--symbol", "planner.weight", "--evidence", "reports/portability-evidence.json"], temp);
      if (refused.code !== 1) fail("f2b-empty-exit", String(refused.code));
      if (!(refused.stderr + refused.stdout).includes('"family-shape"')) fail("f2b-empty-reason", refused.stderr);
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }
  // 2. a foreign project pin inside the artifact refuses
  {
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      withEvidence(temp, (manifest) => {
        manifest.storageRequired = [];
      });
      withStorageFamily(temp, envelope("foreign-project", [], evidence.projectRef.irDigest));
      const refused = tryRun(["--json", "portability", "--to", "go-service", "--symbol", "planner.weight", "--evidence", "reports/portability-evidence.json"], temp);
      if (refused.code !== 1) fail("f2b-project-exit", String(refused.code));
      if (!(refused.stderr + refused.stdout).includes('"family-project-pin"')) fail("f2b-project-reason", refused.stderr);
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }
  // 3. an inventory that contradicts the manifest refuses
  {
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      withEvidence(temp, (manifest) => {
        manifest.storageRequired = [];
      });
      withStorageFamily(temp, envelope("planner", [{
        subject: "scalar:planner.weight",
        policy: { kind: "decimal", value: "exact", precision: 18, scale: 6 },
      }], evidence.projectRef.irDigest));
      const refused = tryRun(["--json", "portability", "--to", "go-service", "--symbol", "planner.weight", "--evidence", "reports/portability-evidence.json"], temp);
      if (refused.code !== 1) fail("f2b-inventory-exit", String(refused.code));
      if (!(refused.stderr + refused.stdout).includes('"family-inventory"')) fail("f2b-inventory-reason", refused.stderr);
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }
  // 4. a reconciled envelope with the manifest's empty inventory yields
  //    the honest no-restricted-policy portable row on go
  {
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      withEvidence(temp, (manifest) => {
        manifest.storageRequired = [];
      });
      withStorageFamily(temp, envelope("planner", [], evidence.projectRef.irDigest));
      const run = tryRun(["--json", "portability", "--to", "go-service", "--symbol", "planner.weight", "--evidence", "reports/portability-evidence.json"], temp);
      if (run.code !== 0) fail("f2b-portable-exit", String(run.code));
      const report = JSON.parse(run.stdout).portability;
      const weight = report.subjects.find((s) => s.subject === "scalar:planner.weight");
      const storageRow = weight.rows.find((r) => r.destination === "go-service" && r.dimension === "storage");
      if (storageRow.classification !== "portable" || storageRow.reason !== "no-restricted-policy") {
        fail("f2b-portable-row", JSON.stringify(storageRow));
      }
      if (!validate.family(envelope("planner", [], evidence.projectRef.irDigest))) fail("f2b-envelope-schema", validate.family.errors);
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
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
    // 1. the pass receipt cannot evidence a claimed failure: the
    // claim/custody disagreement refuses outright — a fail receipt can
    // never be silently reported as a pass, and a pass receipt can
    // never be silently reported as a failure.
    withEvidence(temp, (manifest) => {
      manifest.scenarios[0].state = "recorded-fail";
    });
    const refused = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
    if (refused.code !== 1) fail("f4-contradiction-exit", String(refused.code));
    if (!(refused.stderr + refused.stdout).includes('"receipt-result"')) fail("f4-contradiction-reason", refused.stderr);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    // 2. a qualified failing receipt makes the join failed: the failure
    // dominates, provenance-pinned to this compilation and profile.
    const manifest = JSON.parse(readFileSync(join(temp, "reports/portability-evidence.json"), "utf8"));
    const targets = JSON.parse(readFileSync(join(temp, "config/portability-targets.json"), "utf8"));
    const laravel = targets.targets.find((t) => t.targetId === "php-laravel").profileRef;
    const failReceipt = {
      schemaVersion: "lekalo/portability-receipt/v0.8.0",
      identity: "dev.lekalo.portability-receipt@0.8.0",
      kind: "scenario",
      subject: "planner.focus_flow",
      targetId: "php-laravel",
      result: "fail",
      projectRef: manifest.projectRef,
      profileRef: { id: laravel.id, version: laravel.version, resolvedDigest: laravel.resolvedDigest },
    };
    const failBytes = Buffer.from(JSON.stringify(failReceipt, null, 2) + "\n");
    writeFileSync(join(temp, "reports/receipts/focus-flow-fail.json"), failBytes);
    manifest.scenarios[0] = {
      scenario: "planner.focus_flow",
      targetId: "php-laravel",
      state: "recorded-fail",
      receiptRef: "reports/receipts/focus-flow-fail.json",
      receiptDigest: `sha256:${sha256(failBytes)}`,
    };
    writeFileSync(join(temp, "reports/portability-evidence.json"), JSON.stringify(manifest, null, 2) + "\n");
    const run = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
    if (run.code !== 0) fail("f4-fail-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    const joined = report.affectedScenarios.find((s) => s.scenario === "planner.focus_flow");
    if (joined.state !== "failed") fail("f4-failure-masked", JSON.stringify(joined));
    if (!validate.receipt(failReceipt)) fail("f4-receipt-schema", validate.receipt.errors);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

// F3b — gate execution custody: an authored `executed` state with zero
// custody demotes to declared with a named gap, a qualified failing
// gate receipt reports failed, and an affected scenario with no gate
// record at all is an explicit unknown gate-coverage indication.
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    withEvidence(temp, (manifest) => {
      // The authored executed state presents custody of the wrong kind:
      // the scenario receipt cannot qualify a gate. The claim demotes
      // to declared with a named gap instead of reporting execution.
      manifest.gates[0].state = "executed";
      manifest.gates[0].targetId = "php-laravel";
      manifest.gates[0].receiptRef = manifest.scenarios[0].receiptRef;
      manifest.gates[0].receiptDigest = manifest.scenarios[0].receiptDigest;
    });
    const run = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
    if (run.code !== 0) fail("f3b-gate-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    const gate = report.affectedGates.find((g) => g.gate === "planner.native-scenario-gate");
    if (gate.state === "executed" || gate.state === "failed") fail("f3b-gate-executed", JSON.stringify(gate));
    if (!report.diagnostics.some((d) => d.code === "portability.evidence-incomplete" && d.detail === "gate-unqualified")) {
      fail("f3b-gap-silent", JSON.stringify(report.diagnostics));
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
      manifest.gates = [];
    });
    const run = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
    if (run.code !== 0) fail("f3b-coverage-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    if (report.affectedScenarios.length === 0) fail("f3b-affected-empty", "no affected scenarios");
    if (report.affectedGates.length !== 0) fail("f3b-gates-linger", JSON.stringify(report.affectedGates));
    if (!report.diagnostics.some((d) => d.code === "portability.evidence-incomplete" && d.detail === "gate-coverage-unknown")) {
      fail("f3b-coverage-silent", JSON.stringify(report.diagnostics));
    }
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    // A qualified failing gate receipt: the gate joins as failed, never
    // executed, from digest-bound provenance-pinned custody.
    const manifest = JSON.parse(readFileSync(join(temp, "reports/portability-evidence.json"), "utf8"));
    const targets = JSON.parse(readFileSync(join(temp, "config/portability-targets.json"), "utf8"));
    const laravel = targets.targets.find((t) => t.targetId === "php-laravel").profileRef;
    const gateFail = {
      schemaVersion: "lekalo/portability-receipt/v0.8.0",
      identity: "dev.lekalo.portability-receipt@0.8.0",
      kind: "gate",
      subject: "planner.native-scenario-gate",
      targetId: "php-laravel",
      result: "fail",
      projectRef: manifest.projectRef,
      profileRef: { id: laravel.id, version: laravel.version, resolvedDigest: laravel.resolvedDigest },
    };
    const gateBytes = Buffer.from(JSON.stringify(gateFail, null, 2) + "\n");
    writeFileSync(join(temp, "reports/receipts/gate-fail.json"), gateBytes);
    manifest.gates[0].state = "executed";
    manifest.gates[0].targetId = "php-laravel";
    manifest.gates[0].receiptRef = "reports/receipts/gate-fail.json";
    manifest.gates[0].receiptDigest = `sha256:${sha256(gateBytes)}`;
    writeFileSync(join(temp, "reports/portability-evidence.json"), JSON.stringify(manifest, null, 2) + "\n");
    const run = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
    if (run.code !== 0) fail("f3b-qualified-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    const gate = report.affectedGates.find((g) => g.gate === "planner.native-scenario-gate");
    if (gate.state !== "failed") fail("f3b-qualified-failed", JSON.stringify(gate));
    if (report.diagnostics.some((d) => d.detail === "gate-unqualified")) fail("f3b-qualified-flagged", JSON.stringify(report.diagnostics));
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
      manifest.operationalRequirements.push({
        subject: "command:planner.focus_task",
        requirement: "verify.operations",
        conditions: ["recorded-scenario-receipt"],
        definitionPins: [{ id: "testing.native-scenario", version: "0.4.0" }],
      });
      manifest.operationalRequirements.push({
        subject: "command:planner.focus_task",
        requirement: "unmapped.requirement",
        conditions: [],
        definitionPins: [],
      });
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

// F4b — an unsupported storage policy is a hard blocker with concrete
// migration work: it emits a planning row carrying the exact policy
// pair, and the row count can never be zero beside a known blocker.
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    withEvidence(temp, (manifest) => {
      const offered = manifest.storageOffered.find(
        (entry) => entry.targetId === "php-laravel" && entry.policy.kind === "decimal",
      );
      offered.policy = { kind: "decimal", value: "unsupported" };
    });
    const run = tryRun(["--json", "portability", "--to", "php-laravel", "--symbol", "planner.weight", "--evidence", "reports/portability-evidence.json"], temp);
    if (run.code !== 0) fail("f4b-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    const weight = report.subjects.find((s) => s.subject === "scalar:planner.weight");
    const storageRow = weight.rows.find((r) => r.destination === "php-laravel" && r.dimension === "storage");
    if (storageRow.classification !== "unsupported" || !storageRow.blocking) fail("f4b-row", JSON.stringify(storageRow));
    const planningRow = report.migrationSurface.rows.find(
      (row) => row.destination === "php-laravel" && row.subject === "scalar:planner.weight" && row.dimension === "storage",
    );
    if (!planningRow) fail("f4b-no-planning-row", JSON.stringify(report.migrationSurface));
    if (planningRow.action !== "review-storage-policy") fail("f4b-action", JSON.stringify(planningRow));
    if (!planningRow.required || planningRow.required.kind !== "decimal" || planningRow.required.precision !== 18) {
      fail("f4b-required-policy", JSON.stringify(planningRow));
    }
    if (report.migrationSurface.unresolvedObligations === 0) fail("f4b-unresolved-zero", "hard blocker with zero unresolved work");
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

// F10b — planning keeps the exact obligation identity: two same-kind
// decimal obligations produce two rows whose ids are textually distinct
// through their required policies (no suffix-collision) and whose
// required fields carry the distinct precision/scale pairs.
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    withEvidence(temp, (manifest) => {
      manifest.storageRequired.push({
        subject: "scalar:planner.weight",
        policy: { kind: "decimal", value: "exact", precision: 16, scale: 4 },
      });
    });
    const run = tryRun(["--json", "portability", "--to", "php-laravel", "--symbol", "planner.weight", "--evidence", "reports/portability-evidence.json"], temp);
    if (run.code !== 0) fail("f10b-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    const reviewRows = report.migrationSurface.rows.filter(
      (row) => row.destination === "php-laravel" && row.subject === "scalar:planner.weight" && row.action === "review-storage-policy",
    );
    if (reviewRows.length !== 2) fail("f10b-merged", JSON.stringify(reviewRows));
    const ids = reviewRows.map((row) => row.id);
    if (new Set(ids).size !== 2) fail("f10b-id-collision", JSON.stringify(ids));
    if (ids.some((id) => /\.\d$/.test(id) === false)) {
      // the ids must distinguish through the policy identity itself
      if (ids[0] === ids[1]) fail("f10b-suffix-only", JSON.stringify(ids));
    }
    const precisions = reviewRows.map((row) => row.required && row.required.precision).sort();
    if (JSON.stringify(precisions) !== JSON.stringify([16, 18])) fail("f10b-policy-loss", JSON.stringify(reviewRows));
    const offered = reviewRows.map((row) => row.offered && row.offered.precision);
    if (offered.some((value) => value !== 10)) fail("f10b-offered-loss", JSON.stringify(reviewRows));
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

// F13 — target-specific required work blocks the strict gate: a
// target-specific subject emits its bind-target replacement obligation
// AND denies --check; the risk tag never greens unresolved required
// work. The probe runs without the evidence manifest (a model
// mutation changes the IR pin), which the structural dimension and the
// planning surface do not need.
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    const entities = join(temp, "lekalo/modules/planner/entities.yaml");
    const text = readFileSync(entities, "utf8").replace("portability: portable", "portability: target-specific");
    writeFileSync(entities, text);
    const run = tryRun(["--json", "portability", "--to", "go-service", "--symbol", "planner.weight"], temp);
    if (run.code !== 0) fail("f13-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    const weight = report.subjects.find((s) => s.subject === "scalar:planner.weight");
    const structural = weight.rows.find((r) => r.destination === "go-service" && r.dimension === "structural");
    if (structural.classification !== "target-specific") fail("f13-tag", JSON.stringify(structural));
    if (!structural.blocking) fail("f13-nonblocking", JSON.stringify(structural));
    const bindTarget = report.migrationSurface.rows.find(
      (row) => row.destination === "go-service" && row.subject === "scalar:planner.weight" && row.action === "bind-target",
    );
    if (!bindTarget) fail("f13-bind-target", JSON.stringify(report.migrationSurface.rows));
    const denied = tryRun(["--json", "portability", "--to", "go-service", "--symbol", "planner.weight", "--check"], temp);
    if (denied.code !== 3) fail("f13-check-green", String(denied.code));
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

// F12b — recorded adapter provenance is constructed from a digest-bound
// snapshot artifact, never echoed from manifest fields: a recorded
// record without custody refuses, an arbitrary object refuses as a
// shape, a contradicting snapshot refuses as provenance, and a
// reconciled snapshot is admitted with its evidence digest.
{
  const withAdapterFixture = (temp, artifactDocument) => {
    const config = JSON.parse(readFileSync(join(temp, "config/portability-targets.json"), "utf8"));
    if (artifactDocument !== undefined) {
      config.targets.find((t) => t.targetId === "php-laravel").adapterEvidenceRef = "reports/adapter.json";
    }
    writeFileSync(join(temp, "config/portability-targets.json"), JSON.stringify(config, null, 2) + "\n");
    withEvidence(temp, (manifest) => {
      manifest.adapters[1] = {
        targetId: "php-laravel",
        state: "recorded",
        identity: "composer/lekalo-planner-adapter",
        version: "1.4.2",
        protocol: "http.json/1",
        capabilityDigest: `sha256:${"7".repeat(64)}`,
      };
    });
    if (artifactDocument !== undefined) {
      writeFileSync(join(temp, "reports/adapter.json"), JSON.stringify(artifactDocument, null, 2) + "\n");
    }
  };
  // 1. a recorded record with no snapshot custody refuses
  {
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      withAdapterFixture(temp, undefined);
      const refused = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
      if (refused.code !== 1) fail("f12b-missing-exit", String(refused.code));
      if (!(refused.stderr + refused.stdout).includes('"adapter-provenance"')) fail("f12b-missing-reason", refused.stderr);
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }
  // 2. an arbitrary JSON object is not a snapshot
  {
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      withAdapterFixture(temp, {});
      const refused = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
      if (refused.code !== 1) fail("f12b-shape-exit", String(refused.code));
      if (!(refused.stderr + refused.stdout).includes('"adapter-shape"')) fail("f12b-shape-reason", refused.stderr);
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }
  // 3. a snapshot naming another adapter or target refuses
  {
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      const manifest = JSON.parse(readFileSync(join(temp, "reports/portability-evidence.json"), "utf8"));
      const targets = JSON.parse(readFileSync(join(temp, "config/portability-targets.json"), "utf8"));
      const go = targets.targets.find((t) => t.targetId === "go-service").profileRef;
      withAdapterFixture(temp, {
        schemaVersion: "lekalo/portability-adapter-snapshot/v0.8.0",
        identity: "dev.lekalo.portability-adapter-snapshot@0.8.0",
        targetId: "php-laravel",
        adapter: {
          identity: "go-modules/other-adapter",
          version: "9.9.9",
          protocol: "grpc.proto/1",
          capabilityDigest: `sha256:${"7".repeat(64)}`,
        },
        profileRef: { id: go.id, version: go.version, resolvedDigest: go.resolvedDigest },
      });
      const refused = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
      if (refused.code !== 1) fail("f12b-contradiction-exit", String(refused.code));
      if (!(refused.stderr + refused.stdout).includes('"adapter-provenance"')) fail("f12b-contradiction-reason", refused.stderr);
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }
  // 4. a reconciled snapshot with this destination's profile pin is
  //    admitted and its evidence digest is published
  {
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      const targets = JSON.parse(readFileSync(join(temp, "config/portability-targets.json"), "utf8"));
      const laravel = targets.targets.find((t) => t.targetId === "php-laravel").profileRef;
      const snapshot = {
        schemaVersion: "lekalo/portability-adapter-snapshot/v0.8.0",
        identity: "dev.lekalo.portability-adapter-snapshot@0.8.0",
        targetId: "php-laravel",
        adapter: {
          identity: "composer/lekalo-planner-adapter",
          version: "1.4.2",
          protocol: "http.json/1",
          capabilityDigest: `sha256:${"7".repeat(64)}`,
        },
        profileRef: { id: laravel.id, version: laravel.version, resolvedDigest: laravel.resolvedDigest },
      };
      withAdapterFixture(temp, snapshot);
      const run = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
      if (run.code !== 0) fail("f12b-happy-exit", String(run.code));
      const report = JSON.parse(run.stdout).portability;
      const destination = report.destinations.find((d) => d.targetId === "php-laravel");
      if (!destination.adapter || destination.adapter.state !== "recorded") fail("f12b-happy-snapshot", JSON.stringify(destination.adapter));
      if (destination.adapter.identity !== "composer/lekalo-planner-adapter" || destination.adapter.version !== "1.4.2") fail("f12b-happy-fields", JSON.stringify(destination.adapter));
      if (!destination.adapter.evidenceDigest) fail("f12b-happy-digest", JSON.stringify(destination.adapter));
      if (!validate.adapterSnapshot(snapshot)) fail("f12b-happy-schema", validate.adapterSnapshot.errors);
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }
}

// F12c — lock reconciliation is honest: a canonical lock that
// corroborates nothing selectable reports `unreconciled`, never `known`
// — a lock minted on this very fixture pins no selected profile.
{
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    execFileSync(binary, ["lock"], { cwd: temp, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
    const run = tryRun(["--json", "portability", "--evidence", "reports/portability-evidence.json"], temp);
    if (run.code !== 0) fail("f12c-exit", String(run.code));
    const report = JSON.parse(run.stdout).portability;
    if (report.inputs.lockDigest.state !== "unreconciled") {
      fail("f12c-vacuous-known", JSON.stringify(report.inputs.lockDigest));
    }
    if (!report.inputs.lockDigest.value) fail("f12c-digest", JSON.stringify(report.inputs.lockDigest));
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

// 10d. Round-4 fix probes: destination-exact bridge admission, proved
//      preconditions, target-scoped gate custody, gate-coverage denial,
//      and emit-side schema conformance on adversarial-but-legal inputs.
//      Every emitted report — valid or denied — must validate against
//      the shipped report schema (the post-emission assertion).
{
  const foreignOn = (symbol, target, alias) => ({ target, kind: "foreign", symbol: alias });
  const withForeignBinding = (temp, symbol, target, alias) => {
    const implPath = join(temp, "reports/implementation.json");
    const impl = JSON.parse(readFileSync(implPath, "utf8"));
    impl.contracts.find((c) => c.symbol === symbol).targets.push(foreignOn(symbol, target, alias));
    writeFileSync(implPath, JSON.stringify(impl, null, 2) + "\n");
    withEvidence(temp, (manifest) => {
      manifest.families.find((f) => f.family === "implementation").digest = `sha256:${sha256(readFileSync(implPath))}`;
    });
  };
  const behavioralRow = (report, subject, destination) => report.subjects
    .find((s) => s.subject === subject)
    .rows.find((r) => r.destination === destination && r.dimension === "behavioral");

  // R4-1a — the full PHP precondition list never credits a Go mapping:
  // `go-service` is unkeyed on the `runtime.async` bridge, and a
  // destination without a keyed set can never carry the bridge's credit.
  {
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      withForeignBinding(temp, "planner.schedule_calculate", "go-service", "planner.ScheduleCalculate");
      withEvidence(temp, (manifest) => {
        manifest.mappings.push({
          id: "map.go-php-preconditions",
          requirement: "behavior.hook",
          capability: "runtime.async",
          targetId: "go-service",
          subject: "planner.schedule_calculate",
          coverage: "full",
          conditions: ["replacement-binding-declared", "php-84", "laravel-11"],
          definitionPins: [{ id: "runtime.async", version: "0.2.16" }],
        });
      });
      const run = tryRun(["--json", "portability", "--to", "go-service", "--symbol", "planner.schedule_calculate", "--evidence", "reports/portability-evidence.json"], temp);
      if (run.code !== 0) fail("r4-1a-exit", String(run.code));
      const report = JSON.parse(run.stdout).portability;
      if (!validate.report(report)) fail("r4-1a-schema", validate.report.errors);
      const row = behavioralRow(report, "command:planner.schedule_calculate", "go-service");
      if (row.classification !== "unknown" || row.reason !== "mapping-unqualified" || !row.blocking) {
        fail("r4-1a-fabricated-green", JSON.stringify(row));
      }
      const denied = tryRun(["--json", "portability", "--to", "go-service", "--symbol", "planner.schedule_calculate", "--evidence", "reports/portability-evidence.json", "--check"], temp);
      if (denied.code !== 3) fail("r4-1a-check", String(denied.code));
      if (!validate.report(JSON.parse(denied.stdout).payload.portability)) fail("r4-1a-denied-schema", "denied payload");
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }

  // R4-1b — the mirror direction: the Go vocabulary never credits a PHP
  // mapping through the go-keyed `storage.returning` bridge.
  {
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      withEvidence(temp, (manifest) => {
        manifest.mappings.push({
          id: "map.php-go-preconditions",
          requirement: "behavior.hook",
          capability: "storage.returning",
          targetId: "php-laravel",
          subject: "planner.schedule_calculate",
          coverage: "full",
          conditions: ["replacement-binding-declared", "go-122"],
          definitionPins: [{ id: "storage.returning", version: "0.2.16" }],
        });
      });
      const run = tryRun(["--json", "portability", "--to", "php-laravel", "--symbol", "planner.schedule_calculate", "--evidence", "reports/portability-evidence.json"], temp);
      if (run.code !== 0) fail("r4-1b-exit", String(run.code));
      const report = JSON.parse(run.stdout).portability;
      if (!validate.report(report)) fail("r4-1b-schema", validate.report.errors);
      const row = behavioralRow(report, "command:planner.schedule_calculate", "php-laravel");
      if (row.classification === "supported-with-mapping" || !row.blocking) {
        fail("r4-1b-mirror-credit", JSON.stringify(row));
      }
      if (row.reason !== "mapping-unqualified") fail("r4-1b-reason", row.reason);
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }

  // R4-1c — a destination keyed nowhere in any bridge never carries
  // mapping credit: a fourth configured target (reusing the laravel
  // profile pins under its own id) is unkeyed on every bridge.
  {
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      const config = JSON.parse(readFileSync(join(temp, "config/portability-targets.json"), "utf8"));
      const fourth = structuredClone(config.targets.find((t) => t.targetId === "php-laravel"));
      fourth.targetId = "ruby-rails";
      config.targets.push(fourth);
      writeFileSync(join(temp, "config/portability-targets.json"), JSON.stringify(config, null, 2) + "\n");
      withForeignBinding(temp, "planner.count_focused", "ruby-rails", "planner.CountFocusedRuby");
      withEvidence(temp, (manifest) => {
        manifest.mappings.push({
          id: "map.ruby-nowhere-keyed",
          requirement: "behavior.hook",
          capability: "runtime.async",
          targetId: "ruby-rails",
          subject: "planner.count_focused",
          coverage: "full",
          conditions: ["replacement-binding-declared", "php-84", "laravel-11"],
          definitionPins: [{ id: "runtime.async", version: "0.2.16" }],
        });
      });
      const run = tryRun(["--json", "portability", "--to", "ruby-rails", "--symbol", "planner.count_focused", "--evidence", "reports/portability-evidence.json"], temp);
      if (run.code !== 0) fail("r4-1c-exit", String(run.code));
      const report = JSON.parse(run.stdout).portability;
      if (!validate.report(report)) fail("r4-1c-schema", validate.report.errors);
      const row = behavioralRow(report, "query:planner.count_focused", "ruby-rails");
      if (row.classification !== "unknown" || row.reason !== "mapping-unqualified" || !row.blocking) {
        fail("r4-1c-nowhere-keyed-credit", JSON.stringify(row));
      }
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }

  // R4-1d — declared-but-unproved operational conditions never upgrade a
  // row: the reviewed transaction vocabulary without a corroboring
  // isolation offering stays an explicit unknown; the corroborated
  // control reads the declared capability.
  {
    const transactionRequirement = {
      subject: "command:planner.focus_task",
      requirement: "storage.transactions",
      conditions: ["same-database-engine-family", "declared-isolation-level-at-least-required"],
      definitionPins: [{ id: "storage.transactions", version: "0.4.0" }],
    };
    const isolationRequirement = {
      subject: "command:planner.focus_task",
      policy: { kind: "isolation", value: "repeatable-read" },
    };
    const operationalRows = (report) => report.subjects
      .find((s) => s.subject === "command:planner.focus_task")
      .rows.filter((r) => r.dimension === "operational");
    const unproved = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, unproved, { recursive: true });
      withEvidence(unproved, (manifest) => {
        manifest.operationalRequirements.push(structuredClone(transactionRequirement));
        manifest.storageRequired.push(structuredClone(isolationRequirement));
      });
      const run = tryRun(["--json", "portability", "--to", "php-laravel", "--symbol", "planner.focus_task", "--evidence", "reports/portability-evidence.json"], unproved);
      if (run.code !== 0) fail("r4-1d-exit", String(run.code));
      const report = JSON.parse(run.stdout).portability;
      if (!validate.report(report)) fail("r4-1d-schema", validate.report.errors);
      const rows = operationalRows(report);
      const gap = rows.find((r) => r.evidenceRefs.includes("storage.transactions"));
      if (!gap || gap.classification !== "unknown" || gap.reason !== "precondition-unproved" || !gap.blocking) {
        fail("r4-1d-unproved-portable", JSON.stringify(rows));
      }
      const denied = tryRun(["--json", "portability", "--to", "php-laravel", "--symbol", "planner.focus_task", "--evidence", "reports/portability-evidence.json", "--check"], unproved);
      if (denied.code !== 3) fail("r4-1d-check", String(denied.code));
    } finally {
      rmSync(unproved, { recursive: true, force: true });
    }
    const proved = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, proved, { recursive: true });
      withEvidence(proved, (manifest) => {
        manifest.operationalRequirements.push(structuredClone(transactionRequirement));
        manifest.storageRequired.push(structuredClone(isolationRequirement));
        manifest.storageOffered.push({ targetId: "php-laravel", policy: { kind: "isolation", value: "repeatable-read" } });
      });
      const run = tryRun(["--json", "portability", "--to", "php-laravel", "--symbol", "planner.focus_task", "--evidence", "reports/portability-evidence.json"], proved);
      if (run.code !== 0) fail("r4-1d-proof-exit", String(run.code));
      const report = JSON.parse(run.stdout).portability;
      if (!validate.report(report)) fail("r4-1d-proof-schema", validate.report.errors);
      const rows = operationalRows(report);
      const credited = rows.find((r) => r.evidenceRefs.includes("storage.transactions"));
      if (!credited || credited.classification !== "portable" || credited.reason !== "capability-declared") {
        fail("r4-1d-proof-row", JSON.stringify(rows));
      }
    } finally {
      rmSync(proved, { recursive: true, force: true });
    }
  }

  // R4-2 — gate custody is (gate, destination) scoped: a qualified php
  // receipt never executes a gate in a go-only assessment, while the
  // same-shape go receipt does.
  {
    const gateCustody = (temp, target) => {
      const manifest = JSON.parse(readFileSync(join(temp, "reports/portability-evidence.json"), "utf8"));
      const targets = JSON.parse(readFileSync(join(temp, "config/portability-targets.json"), "utf8"));
      const pin = targets.targets.find((t) => t.targetId === target).profileRef;
      const receipt = {
        schemaVersion: "lekalo/portability-receipt/v0.8.0",
        identity: "dev.lekalo.portability-receipt@0.8.0",
        kind: "gate",
        subject: "planner.native-scenario-gate",
        targetId: target,
        result: "pass",
        projectRef: manifest.projectRef,
        profileRef: { id: pin.id, version: pin.version, resolvedDigest: pin.resolvedDigest },
      };
      const bytes = Buffer.from(JSON.stringify(receipt, null, 2) + "\n");
      writeFileSync(join(temp, "reports/receipts/gate.json"), bytes);
      manifest.gates[0].state = "executed";
      manifest.gates[0].targetId = target;
      manifest.gates[0].receiptRef = "reports/receipts/gate.json";
      manifest.gates[0].receiptDigest = `sha256:${sha256(bytes)}`;
      writeFileSync(join(temp, "reports/portability-evidence.json"), JSON.stringify(manifest, null, 2) + "\n");
    };
    const foreign = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, foreign, { recursive: true });
      gateCustody(foreign, "php-laravel");
      const run = tryRun(["--json", "portability", "--to", "go-service", "--symbol", "planner.focus_task", "--evidence", "reports/portability-evidence.json"], foreign);
      if (run.code !== 0) fail("r4-2-exit", String(run.code));
      const report = JSON.parse(run.stdout).portability;
      if (!validate.report(report)) fail("r4-2-schema", validate.report.errors);
      const gate = report.affectedGates.find((g) => g.gate === "planner.native-scenario-gate");
      if (!gate) fail("r4-2-gate-missing", JSON.stringify(report.affectedGates));
      if (gate.state !== "declared") fail("r4-2-cross-target-upgrade", JSON.stringify(gate));
    } finally {
      rmSync(foreign, { recursive: true, force: true });
    }
    const own = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, own, { recursive: true });
      gateCustody(own, "go-service");
      const run = tryRun(["--json", "portability", "--to", "go-service", "--symbol", "planner.focus_task", "--evidence", "reports/portability-evidence.json"], own);
      if (run.code !== 0) fail("r4-2-own-exit", String(run.code));
      const report = JSON.parse(run.stdout).portability;
      const gate = report.affectedGates.find((g) => g.gate === "planner.native-scenario-gate");
      if (!gate || gate.state !== "executed") fail("r4-2-own-executed", JSON.stringify(gate));
    } finally {
      rmSync(own, { recursive: true, force: true });
    }
  }

  // R4-3 — missing gate coverage is derived from the scoped affected
  // set: it diagnostics, plans collect-evidence work, marks the surface
  // incomplete, and denies the strict gate.
  {
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      withEvidence(temp, (manifest) => {
        manifest.gates = [];
      });
      const run = tryRun(["--json", "portability", "--to", "php-laravel", "--symbol", "planner.focus_task", "--evidence", "reports/portability-evidence.json"], temp);
      if (run.code !== 0) fail("r4-3-exit", String(run.code));
      const report = JSON.parse(run.stdout).portability;
      if (!validate.report(report)) fail("r4-3-schema", validate.report.errors);
      if (!report.diagnostics.some((d) => d.detail === "gate-coverage-unknown")) {
        fail("r4-3-silent", JSON.stringify(report.diagnostics));
      }
      const coverageRow = report.migrationSurface.rows.find((row) => row.reason === "gate-coverage-unknown");
      if (!coverageRow || coverageRow.action !== "collect-evidence" || !coverageRow.evidenceNeeded.includes("gate-record")) {
        fail("r4-3-planning", JSON.stringify(report.migrationSurface.rows));
      }
      if (report.migrationSurface.complete) fail("r4-3-complete", "coverage gap must mark the surface incomplete");
      const denied = tryRun(["--json", "portability", "--to", "php-laravel", "--symbol", "planner.focus_task", "--evidence", "reports/portability-evidence.json", "--check"], temp);
      if (denied.code !== 3) fail("r4-3-check", String(denied.code));
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }

  // R4-4 — emit-side bounds on adversarial-but-legal inputs: every
  // emitted list is capped at its schema maximum with a named clipping
  // diagnostic, planning ids stay inside the published semanticId
  // bound, and the typed-id budget refuses over-budget identities
  // instead of emitting them.
  {
    // 34 admissible mappings on one obligation -> evidenceRefs clipped
    // to 32 with the named diagnostic.
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      withEvidence(temp, (manifest) => {
        for (let i = 0; i < 33; i += 1) {
          manifest.mappings.push({
            id: `map.reading-go-${i}`,
            requirement: "behavior.hook",
            capability: "storage.returning",
            targetId: "go-service",
            subject: "planner.count_focused",
            coverage: "full",
            conditions: ["replacement-binding-declared", "go-122"],
            definitionPins: [{ id: "storage.returning", version: "0.2.16" }],
          });
        }
      });
      const run = tryRun(["--json", "portability", "--to", "go-service", "--symbol", "planner.count_focused", "--evidence", "reports/portability-evidence.json"], temp);
      if (run.code !== 0) fail("r4-4-refs-exit", String(run.code));
      const report = JSON.parse(run.stdout).portability;
      if (!validate.report(report)) fail("r4-4-refs-schema", validate.report.errors);
      const row = behavioralRow(report, "query:planner.count_focused", "go-service");
      if (row.evidenceRefs.length > 32) fail("r4-4-refs-bound", String(row.evidenceRefs.length));
      if (!report.diagnostics.some((d) => d.detail === "evidence-refs-clipped")) {
        fail("r4-4-refs-silent", JSON.stringify(report.diagnostics));
      }
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }
  {
    // A legal 128-character target id and a legal 122-character
    // operational requirement keep every planning id inside the bound.
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      const config = JSON.parse(readFileSync(join(temp, "config/portability-targets.json"), "utf8"));
      const alias = structuredClone(config.targets.find((t) => t.targetId === "go-service"));
      alias.targetId = "t".repeat(127) + "x";
      config.targets.push(alias);
      writeFileSync(join(temp, "config/portability-targets.json"), JSON.stringify(config, null, 2) + "\n");
      withEvidence(temp, (manifest) => {
        manifest.operationalRequirements.push({
          subject: "command:planner.focus_task",
          requirement: `${"a".repeat(120)}.b`,
          conditions: [],
          definitionPins: [],
        });
      });
      const run = tryRun(["--json", "portability", "--to", "t".repeat(127) + "x", "--symbol", "planner.focus_task", "--evidence", "reports/portability-evidence.json", "--check"], temp);
      if (run.code !== 3) fail("r4-4-ids-exit", String(run.code));
      const report = JSON.parse(run.stdout).payload.portability;
      if (!validate.report(report)) fail("r4-4-ids-schema", validate.report.errors);
      for (const row of report.migrationSurface.rows) {
        if (row.id.length > 192) fail("r4-4-ids-bound", `${row.id.length}: ${row.id}`);
      }
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }
  {
    // A 70-dependency subject clips requirements to 64 with the named
    // diagnostic (the model change invalidates the evidence pins, so
    // this probe runs without a manifest).
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      const entities = join(temp, "lekalo/modules/planner/entities.yaml");
      const scalars = [];
      const fields = [];
      for (let i = 0; i < 70; i += 1) {
        const name = `r4dep${String(i).padStart(2, "0")}`;
        scalars.push(`  - id: planner.${name}\n    kind: scalar\n    version: 1\n    description: "R4 dependency ${i}"\n    base: string`);
        fields.push(`      - name: f${i}\n        type: "planner.${name}"\n        required: true`);
      }
      writeFileSync(entities, readFileSync(entities, "utf8") + "\n" + scalars.join("\n") + "\n");
      const commands = join(temp, "lekalo/modules/planner/commands.yaml");
      writeFileSync(commands, readFileSync(commands, "utf8") + `\n  - id: planner.r4_bulk_dependent\n    kind: command\n    version: 1\n    description: "Wide dependency command"\n    input:\n${fields.join("\n")}\n    effects:\n      - "planner.create_task"\n`);
      const run = tryRun(["--json", "portability", "--to", "go-service", "--symbol", "planner.r4_bulk_dependent"], temp);
      if (run.code !== 0) fail("r4-4-deps-exit", `${run.code}: ${run.stderr.slice(0, 200)}`);
      const report = JSON.parse(run.stdout).portability;
      if (!validate.report(report)) fail("r4-4-deps-schema", validate.report.errors);
      const subject = report.subjects.find((s) => s.subject === "command:planner.r4_bulk_dependent");
      if (!subject) fail("r4-4-deps-subject", "missing");
      if (subject.requirements.length > 64) fail("r4-4-deps-bound", String(subject.requirements.length));
      if (!report.diagnostics.some((d) => d.detail === "requirements-clipped")) {
        fail("r4-4-deps-silent", JSON.stringify(report.diagnostics));
      }
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }
  {
    // A legal-but-over-budget 191-character model id (typed id 198) is
    // an explicit refusal, never a schema-invalid emission; a 127-char
    // id still assesses.
    const segment = "a".repeat(63);
    for (const [label, id, expectRefusal] of [
      ["over-budget", `${segment}.${segment}.${segment}`, true],
      ["in-budget", `${segment}.${"b".repeat(63)}`, false],
    ]) {
      const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
      try {
        cpSync(project, temp, { recursive: true });
        const entities = join(temp, "lekalo/modules/planner/entities.yaml");
        writeFileSync(entities, readFileSync(entities, "utf8") + `\n  - id: ${id}\n    kind: scalar\n    version: 1\n    description: "R4 long identity"\n    base: string\n`);
        const run = tryRun(["--json", "portability", "--to", "go-service", "--symbol", id], temp);
        if (expectRefusal) {
          if (run.code !== 1) fail("r4-4-id-refusal-exit", `${label}: ${run.code}`);
          if (!(run.stderr + run.stdout).includes('"subject-id-bound"')) fail("r4-4-id-refusal-reason", run.stderr.slice(0, 200));
        } else {
          if (run.code !== 0) fail("r4-4-id-ok-exit", `${label}: ${run.code}`);
          const report = JSON.parse(run.stdout).portability;
          if (!validate.report(report)) fail("r4-4-id-ok-schema", validate.report.errors);
          if (!report.subjects.some((s) => s.subject === `scalar:${id}`)) fail("r4-4-id-ok-subject", label);
        }
      } finally {
        rmSync(temp, { recursive: true, force: true });
      }
    }
  }
}

// 10e. Round-5 fix probes: the typed-verbatim strict-gate verdict under
//      a crowded diagnostics channel, the disclosed `diagnostics-bound`
//      marker, the maximum-legal-length emit bounds for storage
//      references and the targets selector, and the receipt provenance
//      grammar at admission.
{
  // R5-1 — a schema-legal crowd cannot green the strict gate: thousands
  // of unqualified scenario claims fill the ordinary diagnostics region
  // while no gate record covers the affected scenarios. The verdict is
  // the typed `migrationSurface.complete` flag, the coverage indication
  // is pinned inside the reserved tail, and the bound is disclosed.
  {
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      withEvidence(temp, (manifest) => {
        manifest.gates = [];
        const digest = manifest.scenarios[0].receiptDigest;
        for (let i = 0; i < 4200; i += 1) {
          manifest.scenarios.push({
            scenario: `planner.crowd-${i}`,
            targetId: "php-laravel",
            state: "recorded-pass",
            receiptRef: "reports/receipts/focus-flow.json",
            receiptDigest: digest,
          });
        }
      });
      const args = ["--json", "portability", "--to", "php-laravel", "--symbol", "planner.focus_task", "--evidence", "reports/portability-evidence.json"];
      const control = tryRun(args, temp);
      if (control.code !== 0) fail("r5-1-advisory-exit", String(control.code));
      const controlReport = JSON.parse(control.stdout).portability;
      if (!validate.report(controlReport)) fail("r5-1-advisory-schema", validate.report.errors);
      const denied = tryRun([...args, "--check"], temp);
      if (denied.code !== 3) fail("r5-1-crowd-green", `crowded --check must deny, got ${denied.code}`);
      const report = JSON.parse(denied.stdout).payload.portability;
      if (!validate.report(report)) fail("r5-1-denied-schema", validate.report.errors);
      if (report.migrationSurface.complete) fail("r5-1-complete", "the coverage gap must mark the surface incomplete");
      if (report.diagnostics.length > 4096) fail("r5-1-bound", String(report.diagnostics.length));
      const unqualified = report.diagnostics.filter((d) => d.detail === "receipt-unqualified").length;
      if (unqualified !== 4088) fail("r5-1-crowd-fill", `expected the ordinary region full (4088), got ${unqualified}`);
      const last = report.diagnostics[report.diagnostics.length - 1];
      if (!last || last.detail !== "diagnostics-bound") fail("r5-1-marker", JSON.stringify(last));
      if (!report.diagnostics.some((d) => d.detail === "gate-coverage-unknown")) {
        fail("r5-1-pin", "the coverage gap must stay pinned above the crowd");
      }
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }

  // R5-4 — the storage-reference emit-composition bound: a 252-character
  // reference (the exact legal maximum) emits a 324-character verified
  // member — inside the published bound by construction, and the report
  // stays schema-valid.
  {
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      const ref = ["s".repeat(63), "s".repeat(63), "s".repeat(63), `t${"t".repeat(54)}.json`].join("/");
      if (ref.length !== 252) fail("r5-4-ref-length", String(ref.length));
      let dir = temp;
      for (const segment of ref.split("/").slice(0, -1)) {
        dir = join(dir, segment);
        mkdirSync(dir, { recursive: true });
      }
      copyFileSync(join(root, "tests/fixtures/storage-engine-profile/valid/mysql-8.0.json"), join(dir, "t".repeat(55) + ".json"));
      const config = JSON.parse(readFileSync(join(temp, "config/portability-targets.json"), "utf8"));
      config.targets.find((t) => t.targetId === "php-laravel").storageProfileRefs = [ref];
      writeFileSync(join(temp, "config/portability-targets.json"), JSON.stringify(config, null, 2) + "\n");
      const run = tryRun(["--json", "portability", "--to", "php-laravel", "--symbol", "planner.focus_task", "--evidence", "reports/portability-evidence.json"], temp);
      if (run.code !== 0) fail("r5-4-exit", `${run.code}: ${run.stderr.slice(0, 200)}`);
      const report = JSON.parse(run.stdout).portability;
      if (!validate.report(report)) fail("r5-4-schema", validate.report.errors);
      const emitted = report.destinations.find((d) => d.targetId === "php-laravel").storageProfileRefs[0];
      // Exact bound: 252 + len("#sha256:") + 64 = 324.
      if (!emitted || emitted.length !== 324 || !/#sha256:[0-9a-f]{64}$/.test(emitted)) {
        fail("r5-4-emit-bound", JSON.stringify(emitted));
      }
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }

  // R5-5 — the targets-selector echo bound: the exact 256-character
  // maximum still assesses and emits a schema-valid report; one
  // character more is an explicit admission refusal.
  {
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      const atBound = ["d".repeat(60), "d".repeat(60), "d".repeat(60), "d".repeat(60), "targets.json"].join("/");
      const over = ["d".repeat(60), "d".repeat(60), "d".repeat(60), "d".repeat(61), "targets.json"].join("/");
      if (atBound.length !== 256 || over.length !== 257) fail("r5-5-length", `${atBound.length}/${over.length}`);
      for (const relative of [atBound, over]) {
        let dir = temp;
        for (const segment of relative.split("/").slice(0, -1)) {
          dir = join(dir, segment);
          mkdirSync(dir, { recursive: true });
        }
        copyFileSync(join(temp, "config/portability-targets.json"), join(dir, "targets.json"));
      }
      const emitted = tryRun(["--json", "portability", "--targets", atBound, "--to", "php-laravel", "--symbol", "planner.focus_task"], temp);
      if (emitted.code !== 0) fail("r5-5-emit-exit", `${emitted.code}: ${emitted.stderr.slice(0, 200)}`);
      const report = JSON.parse(emitted.stdout).portability;
      if (!validate.report(report)) fail("r5-5-emit-schema", validate.report.errors);
      const echoed = report.inputs.targetsConfig.path;
      if (echoed !== atBound || echoed.length !== 256) fail("r5-5-echo", JSON.stringify(echoed));
      const refused = tryRun(["--json", "portability", "--targets", over], temp);
      if (refused.code !== 1) fail("r5-5-refuse-exit", String(refused.code));
      if (!(refused.stderr + refused.stdout).includes('"targets-path-bound"')) fail("r5-5-refuse-reason", refused.stderr.slice(0, 200));
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }

  // R5-6 — the receipt provenance grammar at admission: a referenced
  // receipt with malformed provenance members refuses at decode (the
  // schema-invalid bytes never qualify or demote silently).
  {
    const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
    try {
      cpSync(project, temp, { recursive: true });
      const malformations = [
        (receipt) => { receipt.profileRef.resolvedDigest = "not-a-digest"; },
        (receipt) => { receipt.projectRef.projectId = "INVALID"; },
        (receipt) => { receipt.projectRef.modelVersion = "m".repeat(33); },
        (receipt) => { receipt.projectRef.irDigest = "sha256:zz"; },
      ];
      for (const [index, malform] of malformations.entries()) {
        cpSync(project, temp, { recursive: true, force: true });
        const receipt = JSON.parse(readFileSync(join(temp, "reports/receipts/focus-flow.json"), "utf8"));
        malform(receipt);
        const bytes = Buffer.from(JSON.stringify(receipt, null, 2) + "\n");
        writeFileSync(join(temp, "reports/receipts/bad-grammar.json"), bytes);
        withEvidence(temp, (manifest) => {
          manifest.scenarios[0].receiptRef = "reports/receipts/bad-grammar.json";
          manifest.scenarios[0].receiptDigest = `sha256:${sha256(bytes)}`;
        });
        const refused = tryRun(["--json", "portability", "--to", "php-laravel", "--symbol", "planner.focus_task", "--evidence", "reports/portability-evidence.json"], temp);
        if (refused.code !== 1) fail(`r5-6-exit-${index}`, String(refused.code));
        if (!(refused.stderr + refused.stdout).includes('"receipt-shape"')) fail(`r5-6-reason-${index}`, refused.stderr.slice(0, 200));
      }
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }
}

// 10f. Round-6 fix probe: the coverage-gap verdict of the migration
//      surface is computed from the untruncated domain join, so a legal
//      model with more than 10,000 affected scenarios cannot green the
//      strict gate through a gap the emission clip drops.
{
  // R6-1 — 10,003 affected scenarios for the symbol scope, all covered
  // by declared gates except one uncovered scenario that sorts past the
  // 10,000-join emission bound. The fully covered variant greens --check
  // (control); the clipped-away gap must deny (exit 3) with its pinned
  // indication and its collect-evidence planning row — never a false
  // green on a surface the report itself marks complete only because
  // the clip hid the gap.
  const temp = mkdtempSync(join(tmpdir(), "lekalo-portability-"));
  try {
    cpSync(project, temp, { recursive: true });
    const big = (args) => {
      try {
        return { code: 0, stdout: execFileSync(binary, args, { cwd: temp, encoding: "utf8", maxBuffer: 500e6, stdio: ["ignore", "pipe", "pipe"] }) };
      } catch (error) {
        return { code: error.status ?? 1, stdout: String(error.stdout ?? ""), stderr: String(error.stderr ?? "") };
      }
    };
    // Crowd scenarios split across two module documents (the loader's
    // per-document node bound): 5,000 in planner, 5,000 in a new crowd
    // module importing planner; every one covers planner.focus_task.
    const def = (id) => `  - id: ${id}\n    kind: scenario\n    version: 1\n    description: "Crowd coverage"\n    summary: "Crowd scenario covering planner.focus_task."\n    covers:\n      - "planner.focus_task"\n`;
    let plannerYaml = "";
    for (let i = 0; i < 5000; i += 1) plannerYaml += def(`planner.crowdp${String(i).padStart(5, "0")}`);
    appendFileSync(join(temp, "lekalo/modules/planner/scenarios.yaml"), plannerYaml);
    mkdirSync(join(temp, "lekalo/modules/crowd"), { recursive: true });
    writeFileSync(join(temp, "lekalo/modules/crowd/module.yaml"), "schema_version: \"0.2.16\"\ndefinitions:\n  - id: crowd\n    kind: module\n    version: 1\n    description: \"Crowd flood module\"\n    imports:\n      - planner\n");
    let crowdYaml = "schema_version: \"0.2.16\"\ndefinitions:\n";
    for (let i = 0; i < 5000; i += 1) crowdYaml += def(`crowd.crowdn${String(i).padStart(5, "0")}`);
    writeFileSync(join(temp, "lekalo/modules/crowd/scenarios.yaml"), crowdYaml);
    // The live Model/IR pins of the mutated model, derived through the
    // production loader — the same canonical bytes the custody pins
    // bind — then re-pinned into the evidence manifest, the receipts,
    // and the implementation family artifact.
    const repin = () => {
      const member = (out, key) => {
        let sub = out.slice(out.indexOf(`"${key}":`) + key.length + 3).trimEnd();
        if (sub.endsWith("}")) sub = sub.slice(0, -1);
        return sub;
      };
      const modelDigest = `sha256:${sha256(Buffer.from(member(big(["--no-cache", "--json", "load"]).stdout, "model"), "utf8"))}`;
      const irDigest = `sha256:${sha256(Buffer.from(member(big(["--no-cache", "--json", "load", "--ir"]).stdout, "ir"), "utf8"))}`;
      const evidencePath = join(temp, "reports/portability-evidence.json");
      const evidence = JSON.parse(readFileSync(evidencePath, "utf8"));
      evidence.projectRef.irDigest = irDigest;
      const implPath = join(temp, "reports/implementation.json");
      const impl = JSON.parse(readFileSync(implPath, "utf8"));
      if (impl.projectRef) impl.projectRef.irDigest = irDigest;
      impl.modelRef.digest = modelDigest;
      impl.irRef.digest = irDigest;
      writeFileSync(implPath, Buffer.from(JSON.stringify(impl, null, 2) + "\n", "utf8"));
      for (const record of evidence.scenarios) {
        if (!record.receiptRef) continue;
        const receiptPath = join(temp, record.receiptRef);
        const receipt = JSON.parse(readFileSync(receiptPath, "utf8"));
        receipt.projectRef.irDigest = irDigest;
        const bytes = Buffer.from(JSON.stringify(receipt, null, 2) + "\n", "utf8");
        writeFileSync(receiptPath, bytes);
        record.receiptDigest = `sha256:${sha256(bytes)}`;
      }
      const family = evidence.families.find((f) => f.family === "implementation");
      family.digest = `sha256:${sha256(readFileSync(implPath))}`;
      return evidence;
    };
    const coveredNames = [];
    for (let i = 0; i < 5000; i += 1) coveredNames.push(`crowd.crowdn${String(i).padStart(5, "0")}`);
    for (let i = 0; i < 5000; i += 1) coveredNames.push(`planner.crowdp${String(i).padStart(5, "0")}`);
    const args = ["--no-cache", "--json", "portability", "--evidence", "reports/portability-evidence.json", "--symbol", "planner.focus_task", "--to", "php-laravel", "--check"];

    // The control: every affected scenario covered — the clipped join is
    // honest presentation and the strict gate greens.
    let evidence = repin();
    evidence.gates.push(...coveredNames.reduce((gates, _, index) => {
      if (index % 250 === 0) {
        gates.push({ gate: `planner.crowd-gate-${String(index / 250).padStart(2, "0")}`, scenarios: coveredNames.slice(index, index + 250), state: "declared", targetId: null, receiptRef: null, receiptDigest: null });
      }
      return gates;
    }, []));
    writeFileSync(join(temp, "reports/portability-evidence.json"), Buffer.from(JSON.stringify(evidence, null, 2) + "\n", "utf8"));
    const control = big(args);
    if (control.code !== 0) fail("r6-1-control-exit", `${control.code}: ${control.stderr.slice(0, 300)}`);
    const controlReport = JSON.parse(control.stdout).portability;
    if (!validate.report(controlReport)) fail("r6-1-control-schema", validate.report.errors);
    if (!controlReport.migrationSurface.complete) fail("r6-1-control-complete", "a fully covered clipped join stays complete");

    // The probe: one uncovered scenario sorts past the emitted bound —
    // the covering gates stay exactly as the control wrote them.
    appendFileSync(join(temp, "lekalo/modules/planner/scenarios.yaml"), def("planner.zzzuncovered"));
    evidence = repin();
    writeFileSync(join(temp, "reports/portability-evidence.json"), Buffer.from(JSON.stringify(evidence, null, 2) + "\n", "utf8"));
    const denied = big(args);
    if (denied.code !== 3) fail("r6-1-clipped-green", `a clipped-away gap must deny --check, got ${denied.code}`);
    const envelope = JSON.parse(denied.stdout);
    const report = envelope.payload.portability;
    if (!validate.report(report)) fail("r6-1-denied-schema", validate.report.errors);
    if (report.migrationSurface.complete) fail("r6-1-complete", "the clipped-away gap must mark the surface incomplete");
    if (report.affectedScenarios.length !== 10000) fail("r6-1-emit-bound", String(report.affectedScenarios.length));
    if (report.migrationSurface.affectedScenarios !== 10002) fail("r6-1-domain-count", String(report.migrationSurface.affectedScenarios));
    if (report.affectedScenarios.some((s) => s.scenario === "planner.zzzuncovered")) fail("r6-1-emitted-gap", "the uncovered scenario must sort past the emitted prefix");
    if (!report.diagnostics.some((d) => d.detail === "affected-joins-clipped")) fail("r6-1-clip-marker", "missing");
    if (!report.diagnostics.some((d) => d.detail === "gate-coverage-unknown" && d.subject === "planner.zzzuncovered")) fail("r6-1-pin", "the gap stays pinned");
    const collect = report.migrationSurface.rows.find((row) => row.subject === "scenario:planner.zzzuncovered");
    if (!collect || collect.reason !== "gate-coverage-unknown" || collect.action !== "collect-evidence") {
      fail("r6-1-collect-row", JSON.stringify(collect).slice(0, 200));
    }
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

// 11. The strict --check denial: exit 3, status denied, report preserved
//      and schema-valid: the denied envelope's embedded report must pass
//      the published report schema exactly like the valid one.
const denied = JSON.parse(deniedRun);
if (denied.status !== "denied") fail("check-status", denied.status);
if (!validate.report(denied.payload.portability)) fail("denied-report-schema", validate.report.errors);
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
