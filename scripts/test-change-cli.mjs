#!/usr/bin/env node
// Issue #73 live change gate: the real CLI binary over a disposable copy
// of the committed field-vertical fixture. Pins the full observable flow —
// deterministic preview (golden plan bytes, nothing written), durable apply
// (golden receipt and history bytes), ledger-first replay, receipt lookup,
// the breaking-policy denial, the status-owned exits, and the recovery
// barrier — and validates every wire document against the closed schemas.
//
// Requires `cargo build -p lekalo-cli` first (CI builds before this gate).
// Ajv 8.17.1 is provisioned outside the checkout like the other gates.

import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
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
  process.stderr.write(`ajv provisioning failed: ${error?.message ?? error}\n`);
  process.exit(1);
}
if (ajvVersion !== "8.17.1") {
  process.stderr.write(`ajv ${ajvVersion} is not the pinned 8.17.1 contract gate\n`);
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const binary =
  process.env.LEKALO_BIN ??
  join(root, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

// 1. The binary exists (cargo build must run first).
let binaryOk = false;
try {
  binaryOk = statSync(binary).isFile();
} catch {
  binaryOk = false;
}
if (!binaryOk) fail("binary-missing", "run `cargo build -p lekalo-cli` first");

// 2. The schemas for the wire validation.
function readJson(relative) {
  return JSON.parse(readFileSync(resolve(root, relative), "utf8"));
}
const ajv = new Ajv2020({ strict: true, allErrors: true });
const planValidate = ajv.compile(readJson("contracts/change-plan.schema.v0.8.0.json"));
const receiptValidate = ajv.compile(readJson("contracts/change-receipt.schema.v0.8.0.json"));
const historyValidate = ajv.compile(readJson("contracts/change-history.schema.v0.8.0.json"));
const requestValidate = ajv.compile(readJson("contracts/change-request.schema.v0.8.0.json"));

// 3. The disposable sandbox: one fresh fixture copy per run.
const sandbox = join(tmpdir(), `lekalo-change-gate-${process.pid}`);
rmSync(sandbox, { recursive: true, force: true });
mkdirSync(sandbox, { recursive: true });
cpSync(join(root, "tests/fixtures/change/field-vertical"), sandbox, { recursive: true });

let step = 0;
function run(args) {
  step += 1;
  const cleanEnv = { ...process.env };
  delete cleanEnv.LEKALO_PROJECT;
  // The binary resolves the selection against its own invocation directory,
  // so every command runs from inside the sandbox (the run-history gate
  // pattern).
  const result = spawnSync(binary, args, { cwd: sandbox, encoding: "utf8", env: cleanEnv });
  const stdout = result.stdout ?? "";
  const stderr = result.stderr ?? "";
  const stream = stdout.trim().length > 0 ? stdout : stderr;
  let json = null;
  try {
    json = JSON.parse(stream.trim());
  } catch {
    json = null;
  }
  return { code: result.status, stdout, stderr, json };
}

const projectArgs = ["--project", "."];
const entities = join(sandbox, "lekalo/modules/tasks/entities.yaml");
const addFieldRequest = readJson("tests/fixtures/change/golden/request-add-field.json");
const goldenPlan = readJson("tests/fixtures/change/golden/plan-preview.json");
const goldenReceipt = readJson("tests/fixtures/change/golden/receipt-committed.json");
const goldenHistory = readJson("tests/fixtures/change/golden/change-history.json");
if (!requestValidate(addFieldRequest)) fail("gate-request-invalid", requestValidate.errors);
const requestFile = join(sandbox, "gate-request.json");
writeFileSync(requestFile, `${JSON.stringify(addFieldRequest, null, 2)}\n`);
const writeJson = (name, body) => {
  const path = join(sandbox, name);
  writeFileSync(path, `${JSON.stringify(body)}\n`);
  return path;
};

// 4. Deterministic preview: golden plan bytes, zero writes.
const entitiesBefore = readFileSync(entities);
const preview = run([
  "change",
  "request",
  ...projectArgs,
  "--request",
  requestFile,
  "--dry-run",
  "--json",
]);
if (preview.code !== 0) fail("preview-exit", { code: preview.code, stderr: preview.stderr });
if (preview.json?.status !== "valid") fail("preview-status", preview.json);
for (const member of [
  "planId",
  "requestDigest",
  "catalogDigest",
  "policyDigest",
  "fileEdits",
  "fileDiff",
  "semanticDiff",
  "impact",
  "generation",
  "breakingClasses",
  "complete",
]) {
  if (JSON.stringify(preview.json[member]) !== JSON.stringify(goldenPlan[member])) {
    fail("preview-golden-drift", { member });
  }
}
if (!planValidate(preview.json)) fail("preview-schema", planValidate.errors);
if (existsSync(join(sandbox, "lekalo/change-history.json"))) fail("preview-wrote-history");
if (existsSync(join(sandbox, ".lekalo"))) fail("preview-wrote-runtime");
if (!readFileSync(entities).equals(entitiesBefore)) fail("preview-mutated-source");

// 5. The deterministic apply: golden receipt bytes and history bytes.
const apply = run([
  "change",
  "request",
  ...projectArgs,
  "--request",
  requestFile,
  "--apply",
  "--idempotency-key",
  "task-priority-v1",
  "--json",
]);
if (apply.code !== 0) fail("apply-exit", { code: apply.code, stderr: apply.stderr });
if (apply.json?.state !== "committed") fail("apply-state", apply.json);
for (const member of [
  "state",
  "requestDigest",
  "keyDigest",
  "operations",
  "files",
  "history",
  "transaction",
]) {
  if (JSON.stringify(apply.json[member]) !== JSON.stringify(goldenReceipt[member])) {
    fail("apply-golden-drift", { member });
  }
}
if (!receiptValidate(apply.json)) fail("apply-schema", receiptValidate.errors);
const committedHistory = JSON.parse(readFileSync(join(sandbox, "lekalo/change-history.json"), "utf8"));
if (!historyValidate(committedHistory)) fail("history-schema", historyValidate.errors);
if (JSON.stringify(committedHistory) !== JSON.stringify(goldenHistory)) {
  fail("history-golden-drift");
}
const entitiesAfter = readFileSync(entities, "utf8");
if (!entitiesAfter.includes('- name: "priority"')) fail("apply-field-missing");

// 6. The ledger-first replay returns the original receipt without mutating.
const replay = run([
  "change",
  "request",
  ...projectArgs,
  "--request",
  requestFile,
  "--apply",
  "--idempotency-key",
  "task-priority-v1",
  "--json",
]);
if (replay.code !== 0) fail("replay-exit", { code: replay.code, stderr: replay.stderr });
if (replay.json?.state !== "replayed") fail("replay-state", replay.json);
if (replay.json?.planId !== apply.json.planId) fail("replay-plan-id");
const replayHistory = JSON.parse(readFileSync(join(sandbox, "lekalo/change-history.json"), "utf8"));
if (replayHistory.records.length !== 1) fail("replay-duplicated-history");

// 7. A different intent under the same key is a conflict (denied, exit 3).
const conflictRequest = writeJson("gate-conflict.json", {
  schemaVersion: "lekalo/change-request/v0.8.0",
  operations: [
    {
      operation: "enum-case:add",
      enum: "tasks.priority",
      case: { name: "blocked" },
    },
  ],
});
const conflict = run([
  "change",
  "request",
  ...projectArgs,
  "--request",
  conflictRequest,
  "--apply",
  "--idempotency-key",
  "task-priority-v1",
  "--json",
]);
if (conflict.code !== 3) fail("conflict-exit", { code: conflict.code, json: conflict.json });
if (!conflict.json?.reasonCodes?.includes("change.idempotency-conflict")) {
  fail("conflict-rule", conflict.json?.reasonCodes);
}

// 8. The receipt surface finds the committed record by key.
const receipt = run([
  "change",
  "receipt",
  ...projectArgs,
  "--idempotency-key",
  "task-priority-v1",
  "--json",
]);
if (receipt.code !== 0) fail("receipt-exit", { code: receipt.code });
if (receipt.json?.state !== "committed") fail("receipt-state", receipt.json);
if (receipt.json?.planId !== apply.json.planId) fail("receipt-plan-id");

// 9. The breaking removal denies without acknowledgement AND under the
//    bare flag (round 2 finding 15: data-loss classes need reviewed
//    evidence); a reviewed policy with migration evidence authorizes it.
const removalRequest = writeJson("gate-removal.json", {
  schemaVersion: "lekalo/change-request/v0.8.0",
  operations: [{ operation: "entity-field:remove", entity: "tasks.task", name: "priority" }],
});
const denied = run([
  "change",
  "request",
  ...projectArgs,
  "--request",
  removalRequest,
  "--apply",
  "--idempotency-key",
  "gate-remove-v1",
  "--json",
]);
if (denied.code !== 3) fail("breaking-exit", { code: denied.code });
if (!denied.json?.reasonCodes?.includes("change.compatibility-denied")) {
  fail("breaking-rule", denied.json?.reasonCodes);
}
const flagOnly = run([
  "change",
  "request",
  ...projectArgs,
  "--request",
  removalRequest,
  "--apply",
  "--idempotency-key",
  "gate-remove-v2",
  "--allow-breaking",
  "--json",
]);
if (flagOnly.code !== 3) fail("breaking-flag-bypassed-evidence", { code: flagOnly.code });
const evidenceDigest =
  "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const evidencePolicy = writeJson("gate-evidence-policy.json", {
  schemaVersion: "lekalo/change-policy/v0.8.0",
  identity: "dev.lekalo.change-policy@0.8.0",
  allowBreakingClasses: ["data-loss-risk", "source-breaking"],
  evidence: { migrationPlanDigest: evidenceDigest },
});
const allowed = run([
  "change",
  "request",
  ...projectArgs,
  "--request",
  removalRequest,
  "--apply",
  "--idempotency-key",
  "gate-remove-v1",
  "--policy",
  evidencePolicy,
  "--json",
]);
if (allowed.code !== 0) fail("breaking-allowed-exit", { code: allowed.code });
if (allowed.json?.state !== "committed") fail("breaking-allowed-state", allowed.json);
if (allowed.json?.policy?.evidence !== evidenceDigest) {
  fail("breaking-allowed-evidence-recorded", allowed.json?.policy);
}
if (readFileSync(entities, "utf8").includes('- name: "priority"')) {
  fail("breaking-allowed-not-applied");
}

// 10. The status-owned exits for malformed shapes.
const unsupportedRequest = writeJson("gate-unsupported.json", {
  schemaVersion: "lekalo/change-request/v0.8.0",
  operations: [
    {
      operation: "definition:add",
      kind: "command",
      module: "tasks",
      definition: { id: "tasks.complete" },
    },
  ],
});
const unsupported = run([
  "change",
  "request",
  ...projectArgs,
  "--request",
  unsupportedRequest,
  "--dry-run",
  "--json",
]);
if (unsupported.code !== 4) fail("unsupported-exit", { code: unsupported.code });
if (!unsupported.json?.reasonCodes?.includes("change.operation-unsupported")) {
  fail("unsupported-rule", unsupported.json?.reasonCodes);
}
const wrongVersion = writeJson("gate-version.json", {
  schemaVersion: "lekalo/change-request/v0.6.4",
  operations: [],
});
const versionRefused = run([
  "change",
  "request",
  ...projectArgs,
  "--request",
  wrongVersion,
  "--dry-run",
  "--json",
]);
if (versionRefused.code !== 5) fail("version-exit", { code: versionRefused.code });

// 11. The recovery barrier blocks readers until explicit recovery.
const journalDir = join(sandbox, ".lekalo/cache/change/change-gatecafe-1");
mkdirSync(journalDir, { recursive: true });
writeFileSync(
  join(journalDir, "journal.json"),
  `${JSON.stringify({
    transaction: "change-gatecafe-1",
    planId: "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    state: "staging",
    files: [],
    history: false,
  })}\n`,
);
const blocked = run(["load", ...projectArgs, "--json"]);
// The registered rule is admitted for the denied class (LEK-CHANGE-010,
// denied 3): the reader barrier denies instead of invalidating.
if (blocked.code !== 3) fail("barrier-exit", { code: blocked.code });
if (!blocked.json?.reasonCodes?.includes("change.recovery-required")) {
  fail("barrier-rule", blocked.json?.reasonCodes);
}
const recovered = run([
  "change",
  "recover",
  ...projectArgs,
  "--transaction",
  "change-gatecafe-1",
  "--json",
]);
if (recovered.code !== 0) fail("recover-exit", { code: recovered.code });
const unblocked = run(["load", ...projectArgs, "--json"]);
if (unblocked.code !== 0) fail("unblocked-exit", { code: unblocked.code });
if (unblocked.json?.status !== "valid") fail("unblocked-status", unblocked.json);

rmSync(sandbox, { recursive: true, force: true });
process.stdout.write(`${JSON.stringify({
  ok: true,
  gate: "test-change-cli.mjs",
  steps: step,
  flows: [
    "preview-golden",
    "apply-golden",
    "replay",
    "conflict",
    "receipt",
    "breaking",
    "exits",
    "recovery",
  ],
}, null, 2)}\n`);
