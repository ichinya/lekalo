#!/usr/bin/env node
// Issue #52 contract gate: the task-links family under exact Ajv
// 8.17.1 (provisioned outside the checkout like every other schema
// lane), the diagnostic-registry v0.8.0 union successor, the live CLI
// validate/inspect/portability surface, and the full trigger/control
// matrix of the eight LEK-TASKLINK rules.
//
// Schema-only mode (--schema-only) skips every binary-dependent leg so
// the Node contract lane can run before any Rust build; the full mode
// (default) fails closed when the built binary is missing.

import { createRequire } from "node:module";
import { readFileSync, existsSync, statSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  Ajv2020 = require("ajv/dist/2020").default;
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(
    `${JSON.stringify({
      ok: false,
      gate: "task-links-contracts",
      reason: "ajv-unavailable",
      detail: `Ajv 8.17.1 is not reachable through NODE_PATH / LEKALO_AJV_NODE_PATH: ${error?.message ?? error}`,
    })}\n`,
  );
  process.exit(1);
}
if (ajvVersion !== "8.17.1") {
  process.stderr.write(
    `${JSON.stringify({
      ok: false,
      gate: "task-links-contracts",
      reason: "ajv-version",
      detail: `exact Ajv 8.17.1 is required, found ${ajvVersion}`,
    })}\n`,
  );
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
let triggerPairCount = 0;
const schemaOnly = process.argv.includes("--schema-only");
const gate = "task-links-contracts";
const failures = [];
const fail = (reason, detail) => failures.push({ reason, detail });

const read = (relative) => JSON.parse(readFileSync(join(root, relative), "utf8"));
const readText = (relative) => readFileSync(join(root, relative), "utf8");

const ajv = new Ajv2020({ strict: true, allErrors: true });

// 1. The six contract artifacts are valid closed Draft 2020-12
//    documents, and every fixture instance validates or is rejected
//    with the expected shape.
const schemas = {
  integration: "contracts/task-links-integration.schema.v0.8.0.json",
  evidence: "contracts/task-links-evidence.schema.v0.8.0.json",
  report: "contracts/task-links-report.schema.v0.8.0.json",
  portability: "contracts/task-links-portability.schema.v0.8.0.json",
};
const compiled = {};
for (const [name, path] of Object.entries(schemas)) {
  try {
    compiled[name] = ajv.compile(read(path));
  } catch (error) {
    fail("schema-compile", `${path}: ${error.message}`);
  }
}
{
  const registrySchema = ajv.compile(read("contracts/diagnostic-registry.schema.v0.8.0.json"));
  if (!registrySchema(read("contracts/diagnostic-registry.v0.8.0.json"))) {
    fail("registry-instance", ajv.errorsText(registrySchema.errors));
  }
}

// 2. Registry union invariants: 0.8.0 = 0.6.5 + exactly the eight new
//    tasklink entries, sorted, unique codes, exact categories.
{
  const predecessor = read("contracts/diagnostic-registry.v0.6.5.json");
  const successor = read("contracts/diagnostic-registry.v0.8.0.json");
  const predecessorIds = new Map(predecessor.entries.map((entry) => [entry.id, entry]));
  const successorIds = new Map(successor.entries.map((entry) => [entry.id, entry]));
  for (const [id, entry] of predecessorIds) {
    if (JSON.stringify(successorIds.get(id)) !== JSON.stringify(entry)) {
      fail("predecessor-drift", id);
    }
  }
  const added = successor.entries.filter((entry) => !predecessorIds.has(entry.id));
  const expected = [
    ["tasklink.conflict-resolution-invalid", "LEK-TASKLINK-005", "semantic"],
    ["tasklink.decision-incomplete", "LEK-TASKLINK-002", "security"],
    ["tasklink.delivery-contract-invalid", "LEK-TASKLINK-004", "semantic"],
    ["tasklink.evidence-invalid", "LEK-TASKLINK-008", "compatibility"],
    ["tasklink.fanout-not-admitted", "LEK-TASKLINK-003", "security"],
    ["tasklink.history-or-ownership-violation", "LEK-TASKLINK-006", "semantic"],
    ["tasklink.relation-identity-invalid", "LEK-TASKLINK-001", "model"],
    ["tasklink.required-guarantee-unsupported", "LEK-TASKLINK-007", "adapter"],
  ];
  if (added.length !== expected.length) fail("added-count", added.length);
  const ids = successor.entries.map((entry) => entry.id);
  if (JSON.stringify(ids) !== JSON.stringify([...ids].sort())) fail("entries-not-sorted");
  for (const [id, code, category] of expected) {
    const entry = successorIds.get(id);
    if (!entry) {
      fail("missing-rule", id);
      continue;
    }
    if (entry.code !== code) fail("rule-code", `${id}: ${entry.code}`);
    if (entry.category !== category) fail("rule-category", id);
    if (entry.default_severity !== "error") fail("rule-severity", id);
    if (entry.message_id !== entry.id) fail("rule-message-id", id);
    if (entry.lifecycle !== "active") fail("rule-lifecycle", id);
  }
}

// 3. The valid attachment and evidence instances satisfy their schemas.
{
  const integration = read("tests/fixtures/task-external-links/contracts/integration.json");
  if (!compiled.integration(integration)) {
    fail("valid-attachment-rejected", ajv.errorsText(compiled.integration.errors));
  }
  const evidence = read("tests/fixtures/task-external-links/evidence/mock-run.json");
  if (!compiled.evidence(evidence)) {
    fail("valid-evidence-rejected", ajv.errorsText(compiled.evidence.errors));
  }
}

// 4. Shape-level negatives are rejected by the schemas: unknown
//    members, wrong discriminator/identity, malformed digests. The
//    committed invalid/ documents are semantic-level mutations that
//    stay schema-valid and reach their registered rules through the
//    live CLI legs below (the two-layer refusal is intentional).
{
  const integration = read("tests/fixtures/task-external-links/contracts/integration.json");
  const shapeCases = [
    ["unknown-top-field", { ...integration, phantom: true }],
    ["wrong-identity", { ...integration, identity: "dev.lekalo.task-links-integration@0.7.0" }],
    ["wrong-discriminator", { ...integration, schemaVersion: "lekalo/task-links-integration/v0.7.0" }],
    ["bad-revision-digest", { ...integration, attachmentRevision: "sha256:xyz" }],
    ["open-relation", (() => {
      const clone = JSON.parse(JSON.stringify(integration));
      clone.relations[0].onDelete = "cascade-external-delete";
      return clone;
    })()],
    ["atomic-network", (() => {
      const clone = JSON.parse(JSON.stringify(integration));
      clone.deliveryRules.networkInsideTransaction = true;
      return clone;
    })()],
    ["implicit-delete", (() => {
      const clone = JSON.parse(JSON.stringify(integration));
      clone.deliveryRules.externalDeleteImplicit = true;
      return clone;
    })()],
  ];
  for (const [name, document] of shapeCases) {
    if (compiled.integration(document)) {
      fail("shape-negative-accepted", name);
    }
  }
  const evidence = read("tests/fixtures/task-external-links/evidence/mock-run.json");
  if (!compiled.evidence({ ...evidence, origin: "live-provider" })) {
    // sanity: the closed origin enum rejects an unadmitted origin
  } else {
    fail("shape-negative-accepted", "evidence-origin");
  }
}

// 5. Duplicate JSON keys are rejected in every fixture JSON of the
//    family (the repo convention; serde would silently take the last).
function walkValues(value, visit) {
  visit(value);
  if (Array.isArray(value)) {
    for (const item of value) walkValues(item, visit);
  } else if (value && typeof value === "object") {
    for (const item of Object.values(value)) walkValues(item, visit);
  }
}
function hasDuplicateKeys(text) {
  // Cheap structural detector: keys inside one object literal at any
  // depth. Parsed-object comparison cannot see duplicates, so scan the
  // raw text with a tiny state machine.
  let depth = 0;
  let inString = false;
  let escaped = false;
  let state = "expect-key";
  let lastKey = "";
  const stack = [];
  for (let i = 0; i < text.length; i += 1) {
    const char = text[i];
    if (inString) {
      if (escaped) escaped = false;
      else if (char === "\\") escaped = true;
      else if (char === '"') {
        inString = false;
        if (state === "in-key") {
          if (stack[stack.length - 1] === lastKey) return true;
          stack[stack.length - 1] = lastKey;
          state = "after-key";
        }
      } else if (state === "in-key") lastKey += char;
      continue;
    }
    if (char === '"') {
      inString = true;
      if (state === "expect-key" || state === "expect-value") {
        if (state === "expect-key") state = "in-key";
        lastKey = "";
      }
      continue;
    }
    if (char === "{") {
      depth += 1;
      stack.push(null);
      state = "expect-key";
    } else if (char === "}") {
      depth -= 1;
      stack.pop();
      state = depth === 0 ? "done" : "expect-key";
    } else if (char === "[") {
      depth += 1;
      stack.push(Symbol("array"));
      state = "expect-value";
    } else if (char === "]") {
      depth -= 1;
      stack.pop();
      state = depth === 0 ? "done" : "expect-value";
    } else if (char === ":") {
      state = "expect-value";
    } else if (char === ",") {
      if (state === "after-key" || state === "expect-key") state = "expect-key";
      else state = "expect-value";
    }
  }
  return false;
}
const fixtureRoot = join(root, "tests/fixtures/task-external-links");
const jsonFiles = [];
const walkFiles = (current) => {
  for (const name of readdirSorted(current)) {
    const full = join(current, name);
    if (statSync(full).isDirectory()) walkFiles(full);
    else if (name.endsWith(".json")) jsonFiles.push(full);
  }
};
function readdirSorted(current) {
  return require("node:fs").readdirSync(current).sort();
}
walkFiles(fixtureRoot);
for (const file of jsonFiles) {
  if (hasDuplicateKeys(readFileSync(file, "utf8"))) {
    fail("duplicate-json-keys", file.slice(root.length + 1));
  }
}

// 6. The ten scenario docs name the ten verbatim scenario ids and the
//    evidence instance is mock-origin and pin-bound.
{
  const expectedScenarios = [
    "two_providers",
    "duplicate_import",
    "identity_collision",
    "attributed_inbound",
    "decision_matrix",
    "partial_delivery",
    "stale_conflict",
    "detach_history",
    "retry_echo_crash",
    "secret_canary",
  ];
  for (const slug of expectedScenarios) {
    const path = `tests/fixtures/task-external-links/scenarios/${slug}.json`;
    if (!existsSync(join(root, path))) {
      fail("scenario-doc-missing", slug);
      continue;
    }
    const document = read(path);
    if (document.scenarioId !== `tasklinks.scenario.${slug}`) {
      fail("scenario-id-drift", slug);
    }
    if (document.origin !== "mock") fail("scenario-origin", slug);
    if (!document.expects || document.expects.length === 0) {
      fail("scenario-expects-empty", slug);
    }
  }
}

// 7. Live CLI legs (skipped in --schema-only mode).
const binary = join(root, "target/debug/lekalo.exe");
const binaryCandidates = [
  join(root, "target/debug/lekalo"),
  join(root, "target/debug/lekalo.exe"),
];
const resolvedBinary = binaryCandidates.find((candidate) => existsSync(candidate));
const project = "tests/fixtures/task-external-links/project";
const attachment = "tests/fixtures/task-external-links/contracts/integration.json";
const evidence = "tests/fixtures/task-external-links/evidence/mock-run.json";
const profileLaravel = "tests/fixtures/task-external-links/profiles/laravel.json";
const profileNode = "tests/fixtures/task-external-links/profiles/node.json";

function runCli(args) {
  const result = spawnSync(resolvedBinary, ["--no-cache", ...args], {
    cwd: root,
    encoding: "utf8",
    timeout: 120000,
  });
  return {
    code: result.status,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
  };
}

if (!schemaOnly) {
  if (!resolvedBinary) {
    fail("binary-missing", "build the workspace first: cargo build --workspace --locked");
  } else {
    // validate strict: valid, exit 0, exact counts.
    {
      const run = runCli([
        "--json",
        "task-links",
        "validate",
        attachment,
        "--project",
        project,
        "--profile",
        "strict",
      ]);
      if (run.code !== 0) {
        fail("validate-strict-exit", String(run.code) + run.stderr.slice(0, 300));
      } else {
        const envelope = JSON.parse(run.stdout);
        if (envelope.status !== "valid") fail("validate-strict-status", envelope.status);
        if (envelope.taskLinks.checks.relationsChecked !== 2) {
          fail("validate-strict-counts", JSON.stringify(envelope.taskLinks.checks));
        }
        if (
          envelope.taskLinks.schemaVersion !== "lekalo/task-links-report/v0.8.0" ||
          envelope.taskLinks.identity !== "dev.lekalo.task-links-report@0.8.0"
        ) {
          fail("report-identity", "report pins");
        }
      }
    }
    // inspect admits evidence and shows the typed view.
    {
      const run = runCli([
        "--json",
        "task-links",
        "inspect",
        "tasklinks.task_external_link",
        "--project",
        project,
        "--attachment",
        attachment,
        "--evidence",
        evidence,
      ]);
      if (run.code !== 0) fail("inspect-exit", String(run.code));
      else {
        const envelope = JSON.parse(run.stdout);
        const relations = envelope.taskLinks.relations;
        if (!relations.some((row) => row.id === "tasklinks.rel.task_links" && row.cardinality === "0..N")) {
          fail("inspect-relations", "missing typed relation");
        }
        if (envelope.taskLinks.evidence?.origin !== "mock") {
          fail("inspect-evidence", JSON.stringify(envelope.taskLinks.evidence));
        }
      }
    }
    // The trigger matrix: every pair reaches exactly its rule.
    const matrix = read("tests/fixtures/task-external-links/invalid/expect.json");
    triggerPairCount = matrix.pairs.length;
    for (const pair of matrix.pairs) {
      if (pair.command === "inspect") {
        // Evidence-side triggers: the mutated evidence document must
        // reach its rule through inspect with the valid attachment.
        const trigger = runCli([
          "--json",
          "task-links",
          "inspect",
          "tasklinks.task_external_link",
          "--project",
          project,
          "--attachment",
          attachment,
          "--evidence",
          `tests/fixtures/task-external-links/${pair.trigger}`,
        ]);
        if (trigger.code !== 1) {
          fail("trigger-exit", `${pair.rule}: ${trigger.code}`);
          continue;
        }
        const envelope = JSON.parse(trigger.stderr);
        if (envelope.reasonCodes[0] !== pair.rule) {
          fail("trigger-rule", `${pair.rule}: ${JSON.stringify(envelope.reasonCodes)}`);
        }
        const diagnostic = envelope.diagnostics[0];
        if (diagnostic.data.detail !== pair.detailTag) {
          fail("trigger-detail", `${pair.rule}: ${diagnostic.data.detail}`);
        }
        continue;
      }
      if (pair.command === "validate") {
        const trigger = runCli([
          "--json",
          "task-links",
          "validate",
          `tests/fixtures/task-external-links/${pair.trigger}`,
          "--project",
          project,
          "--profile",
          "strict",
        ]);
        if (trigger.code !== 1) {
          fail("trigger-exit", `${pair.rule}: ${trigger.code}`);
          continue;
        }
        const envelope = JSON.parse(trigger.stderr);
        if (envelope.status !== "invalid") fail("trigger-status", pair.rule);
        if (envelope.reasonCodes[0] !== pair.rule) {
          fail("trigger-rule", `${pair.rule}: ${JSON.stringify(envelope.reasonCodes)}`);
        }
        const diagnostic = envelope.diagnostics[0];
        if (diagnostic.data.detail !== pair.detailTag) {
          fail("trigger-detail", `${pair.rule}: ${diagnostic.data.detail}`);
        }
        if (diagnostic.code !== pair.code) fail("trigger-code", pair.rule);
        if (diagnostic.registry_version !== "0.8.0") {
          fail("trigger-registry-pin", pair.rule);
        }
        // The control: the unmutated attachment stays valid.
        const control = runCli([
          "--json",
          "task-links",
          "validate",
          attachment,
          "--project",
          project,
          "--profile",
          "strict",
        ]);
        if (control.code !== 0) fail("control-exit", pair.rule);
      }
    }
    // 007 polarity through portability.
    {
      const strict = runCli([
        "--json",
        "task-links",
        "portability",
        attachment,
        "--project",
        project,
        "--from-profile",
        profileLaravel,
        "--to-profile",
        profileNode,
        "--profile",
        "strict",
      ]);
      if (strict.code !== 1) fail("portability-strict-exit", String(strict.code));
      else {
        const envelope = JSON.parse(strict.stderr);
        if (!envelope.reasonCodes.includes("tasklink.required-guarantee-unsupported")) {
          fail("portability-strict-rule", JSON.stringify(envelope.reasonCodes));
        }
      }
      const permissive = runCli([
        "--json",
        "task-links",
        "portability",
        attachment,
        "--project",
        project,
        "--from-profile",
        profileLaravel,
        "--to-profile",
        profileNode,
        "--evidence",
        evidence,
        "--profile",
        "permissive",
      ]);
      if (permissive.code !== 0) fail("portability-permissive-exit", String(permissive.code));
      else {
        const envelope = JSON.parse(permissive.stdout);
        if (!envelope.reasonCodes.includes("tasklink.required-guarantee-unsupported")) {
          fail("portability-permissive-gap", "the gap must stay visible");
        }
        if (envelope.status !== "valid") fail("portability-permissive-status", envelope.status);
      }
    }
    // 008 polarity through inspect with stale evidence.
    {
      const stale = runCli([
        "--json",
        "task-links",
        "inspect",
        "tasklinks.task_external_link",
        "--project",
        project,
        "--attachment",
        attachment,
        "--evidence",
        "tests/fixtures/task-external-links/invalid/evidence-stale.json",
      ]);
      if (stale.code !== 1) fail("evidence-trigger-exit", String(stale.code));
      else {
        const envelope = JSON.parse(stale.stderr);
        if (envelope.reasonCodes[0] !== "tasklink.evidence-invalid") {
          fail("evidence-trigger-rule", JSON.stringify(envelope.reasonCodes));
        }
      }
    }
    // Contribution switches are consumed: a disabled inspect
    // contribution refuses inspect; a disabled impact contribution
    // refuses portability; both refusals carry the registered rule.
    {
      const inspectOff = runCli([
        "--json",
        "task-links",
        "inspect",
        "tasklinks.task_external_link",
        "--project",
        project,
        "--attachment",
        "tests/fixtures/task-external-links/invalid/contrib-inspect-off.json",
      ]);
      if (inspectOff.code !== 1) fail("contribution-inspect-exit", String(inspectOff.code));
      else {
        const envelope = JSON.parse(inspectOff.stderr);
        if (envelope.diagnostics[0].data.detail !== "contribution-disabled") {
          fail("contribution-inspect-detail", JSON.stringify(envelope.diagnostics[0].data));
        }
      }
      const impactOff = runCli([
        "--json",
        "task-links",
        "portability",
        "tests/fixtures/task-external-links/invalid/contrib-impact-off.json",
        "--project",
        project,
        "--from-profile",
        profileLaravel,
        "--to-profile",
        profileNode,
        "--profile",
        "permissive",
      ]);
      if (impactOff.code !== 1) fail("contribution-impact-exit", String(impactOff.code));
      else {
        const envelope = JSON.parse(impactOff.stderr);
        if (envelope.diagnostics[0].data.detail !== "contribution-disabled") {
          fail("contribution-impact-detail", JSON.stringify(envelope.diagnostics[0].data));
        }
      }
    }
    // Portability output satisfies its schema end to end.
    {
      const run = runCli([
        "--json",
        "task-links",
        "portability",
        attachment,
        "--project",
        project,
        "--from-profile",
        profileLaravel,
        "--to-profile",
        profileNode,
        "--evidence",
        evidence,
        "--profile",
        "permissive",
      ]);
      if (run.code === 0) {
        const envelope = JSON.parse(run.stdout);
        if (!compiled.portability(envelope.taskLinks)) {
          fail("portability-schema", ajv.errorsText(compiled.portability.errors));
        }
      }
    }
    // The validate report satisfies its schema end to end.
    {
      const run = runCli([
        "--json",
        "task-links",
        "validate",
        attachment,
        "--project",
        project,
        "--profile",
        "strict",
      ]);
      if (run.code === 0) {
        const envelope = JSON.parse(run.stdout);
        if (!compiled.report(envelope.taskLinks)) {
          fail("report-schema", ajv.errorsText(compiled.report.errors));
        }
      }
    }
  }
}

if (failures.length > 0) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, gate, failures }, null, 2)}\n`,
  );
  process.exit(1);
}
process.stdout.write(
  `${JSON.stringify({
    ok: true,
    gate,
    schemaOnly,
    schemasValidated: Object.keys(schemas).length + 1,
    jsonFilesScanned: jsonFiles.length,
    triggerPairs: schemaOnly ? 0 : triggerPairCount,
  })}\n`,
);
