#!/usr/bin/env node
// Issue #121 end-to-end run-history gate: the real CLI binary over a
// disposable local project. Proves the fully offline flow (an empty
// environment: no account, provider, proxy, or telemetry variables of
// any kind), the produced record validating against the committed
// versioned schema, the separate assertion custody, retention with
// dependent invalidation, recovery, the fail-closed refusals without
// value echo, and the absent export surface.
//
// Requires `cargo build -p lekalo-cli` first (CI builds before this
// gate). Ajv 8.17.1 is provisioned outside the checkout like the other
// contract gates.

import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, statSync } from "node:fs";
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
    `ajv provisioning failed: ${error && error.message ? error.message : error}\n`,
  );
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
if (!binaryOk) fail("binary-missing", `run: cargo build -p lekalo-cli (${binary})`);

// The empty offline environment: only what the process needs to start.
const offlineEnv = { PATH: process.env.PATH ?? "" };
if (process.platform === "win32") {
  offlineEnv.SystemRoot = process.env.SystemRoot ?? "";
}

const runOffline = (args, cwd, input) => {
  const result = spawnSync(binary, args, {
    cwd,
    input,
    encoding: "utf8",
    env: offlineEnv,
  });
  return {
    code: result.status ?? 1,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
  };
};

const caseId = `${process.pid}-${Date.now()}`;
const project = join(tmpdir(), `lekalo-history-cli-${caseId}`);
mkdirSync(project, { recursive: true});
try {
  // 2. Init creates the governed home with its generated protection.
  let result = runOffline(["--json", "history", "init", "--project", "."], project);
  if (result.code !== 0) fail("init-failed", result);
  if (!existsSync(join(project, ".lekalo", "history", ".gitignore"))) {
    fail("ignore-protection-missing", join(project, ".lekalo/history"));
  }
  if (readFileSync(join(project, ".lekalo/history/.gitignore"), "utf8") !== "*\n") {
    fail("ignore-protection-wrong", "expected generated '*'");
  }

  // 3. A scope token is a random opaque local token.
  result = runOffline(["--json", "history", "scope", "create", "--project", "."], project);
  if (result.code !== 0) fail("scope-failed", result);
  const scope = JSON.parse(result.stdout).result.tenantScopeId;
  if (!/^[0-9a-f]{32}$/.test(scope)) fail("scope-token-shape", scope);

  // 4. The offline record flow: empty environment, no account, no
  // network configuration, and the observation enters only via stdin.
  const observation = {
    schema_version: "lekalo/run-observation/v0.4.0",
    identity: "dev.lekalo.run-observation@0.4.0",
    pilot: { mode: "brownfield", scopeState: "observed" },
    operation: { kind: "verify", affectedSemanticIds: ["planner.focus_task"] },
    timestamp: "2026-09-30T12:00:00Z",
    provenance: {
      git: {},
      model: {
        revision: { state: "known", value: "planner-model" },
        digest: {
          state: "known",
          value: "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        },
        irDigest: {
          state: "known",
          value: "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        },
      },
      lock: {},
      core: { version: { state: "known", value: "0.4.0" } },
      adapters: [],
      profile: {},
      harness: { id: { state: "known", value: "pilot-harness" } },
    },
    metrics: {
      durationMs: { state: "known", value: 900 },
      tokens: {
        input: { state: "known", value: 10 },
        output: { state: "known", value: 5 },
        reasoning: { state: "unknown" },
        total: { state: "known", value: 15 },
      },
      cost: {},
      context: {},
    },
    measurementSources: [
      {
        field: "metrics.durationMs",
        sourceKind: "core",
        sourceId: "lekalo-core",
        sourceVersion: { state: "known", value: "0.4.0" },
      },
    ],
    testGateSummaries: [],
    diagnostics: [],
    assertions: {
      rows: [
        {
          assertionId: "focus-task-returns-planned-order",
          subjectSemanticId: "planner.focus_task",
          kind: "behavior",
          outcome: "pass",
          evidenceRef: null,
        },
      ],
    },
    repeatParentRunId: null,
    status: { outcome: "pass", coverageState: "complete" },
    dataSensitivity: "internal",
  };
  result = runOffline(
    ["--json", "history", "record", "--input", "-", "--scope", scope, "--project", "."],
    project,
    JSON.stringify(observation),
  );
  if (result.code !== 0) fail("record-failed", result);
  const receipt = JSON.parse(result.stdout).result;
  if (!/^[0-9a-f]{32}$/.test(receipt.runId)) fail("run-id-shape", receipt.runId);
  if (receipt.status !== "pass") fail("record-status", receipt);

  // 5. The produced record validates against the committed schema and
  // the missing metrics stayed unknown.
  const recordSchema = JSON.parse(
    readFileSync(join(root, "contracts/run-record.schema.v0.4.0.json"), "utf8"),
  );
  const ajv = new Ajv2020({ strict: true, allErrors: true });
  const validateRecord = ajv.compile(recordSchema);
  result = runOffline(
    ["--json", "history", "show", receipt.runId, "--scope", scope, "--project", "."],
    project,
  );
  if (result.code !== 0) fail("show-failed", result);
  const show = JSON.parse(result.stdout).result;
  if (!validateRecord(show.record)) fail("record-schema", validateRecord.errors);
  if (show.record.metrics.cost.amount.state !== "unknown") {
    fail("missing-cost-not-unknown", show.record.metrics.cost);
  }
  // Assertions are stored separately and digest-bound.
  if (!show.assertions || show.assertions.runId !== receipt.runId) {
    fail("assertions-not-separate", show);
  }
  if (show.record.assertionsRef.setId !== show.assertions.setId) {
    fail("assertion-binding", show.record.assertionsRef);
  }

  // 6. Retention with the record bound pruned and dependents
  // invalidated in one transaction.
  result = runOffline(
    [
      "--json", "history", "dependents", "register", "claim.lift",
      "--kind", "claim", "--run", receipt.runId, "--scope", scope, "--project", ".",
    ],
    project,
  );
  if (result.code !== 0) fail("register-failed", result);
  result = runOffline(
    ["--json", "history", "delete", receipt.runId, "--scope", scope, "--dry-run", "--project", "."],
    project,
  );
  if (result.code !== 0) fail("delete-dry-failed", result);
  const dry = JSON.parse(result.stdout).result;
  if (dry.applied !== false || JSON.stringify(dry.invalidatedDependents) !== '["claim.lift"]') {
    fail("delete-dry-report", dry);
  }
  result = runOffline(
    ["--json", "history", "delete", receipt.runId, "--scope", scope, "--apply", "--project", "."],
    project,
  );
  if (result.code !== 0) fail("delete-apply-failed", result);
  result = runOffline(
    ["--json", "history", "dependents", "resolve", "claim.lift", "--scope", scope, "--project", "."],
    project,
  );
  if (result.code !== 1 || !`${result.stdout}${result.stderr}`.includes("history.dependent-invalidated")) {
    fail("dependent-not-invalidated", result);
  }

  // 7. Recovery verifies the surviving store.
  result = runOffline(["--json", "history", "recover", "--project", "."], project);
  if (result.code !== 0) fail("recover-failed", result);
  const recovered = JSON.parse(result.stdout).result;
  if (recovered.runCount !== 0) fail("recover-count", recovered);

  // 8. The hostile observation refuses without echoing the value.
  const hostile = JSON.parse(JSON.stringify(observation));
  hostile.provenance.harness.id = { state: "known", value: "C:/Users/someone/prompt.txt" };
  const scope2 = JSON.parse(
    runOffline(["--json", "history", "scope", "create", "--project", "."], project).stdout,
  ).result.tenantScopeId;
  result = runOffline(
    ["--json", "history", "record", "--input", "-", "--scope", scope2, "--project", "."],
    project,
    JSON.stringify(hostile),
  );
  if (result.code !== 1) fail("hostile-exit", result);
  const hostileText = `${result.stdout}${result.stderr}`;
  if (!hostileText.includes("history.unsafe-field")) fail("hostile-code", result);
  if (hostileText.includes("C:/Users") || hostileText.includes("prompt.txt")) {
    fail("hostile-echo", result);
  }

  // 9. No export surface exists anywhere in the family.
  result = runOffline(["history", "export", "--project", "."], project);
  if (result.code === 0) fail("export-exists", result);
  result = runOffline(["history", "--help"], project);
  const helpText = `${result.stdout}${result.stderr}`;
  for (const forbidden of ["export", "upload", "publish", "aggregate"]) {
    if (new RegExp(`^\\s+${forbidden}\\b`, "m").test(helpText)) {
      fail("export-surface-in-help", forbidden);
    }
  }
} finally {
  rmSync(project, { recursive: true, force: true });
}

process.stdout.write(`${JSON.stringify({ ok: true, binary, offline: true })}\n`);
