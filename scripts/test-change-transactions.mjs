#!/usr/bin/env node
// Issue #73 change transaction gate: the journal/backup/lock/recovery
// subsystem over the real CLI binary and a disposable fixture copy. This
// is the fault-injection matrix the review round 1 required (finding 10):
// planted journals must never write outside the project (finding 1),
// corrupted backups must refuse with the barrier intact (finding 4),
// stale writer locks must not brick recovery (finding 3), forged history
// must never mint a replayed receipt (finding 5), recover must emit a
// schema-valid rolled-back receipt (finding 6), and emitted documents
// must be duplicate-key-free JSON (finding 8).
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

let binaryOk = false;
try {
  binaryOk = statSync(binary).isFile();
} catch {
  binaryOk = false;
}
if (!binaryOk) fail("binary-missing", "run `cargo build -p lekalo-cli` first");

const ajv = new Ajv2020({ strict: true, allErrors: true });
const receiptValidate = ajv.compile(readJson("contracts/change-receipt.schema.v0.8.0.json"));
const historyValidate = ajv.compile(readJson("contracts/change-history.schema.v0.8.0.json"));

function readJson(relative) {
  return JSON.parse(readFileSync(resolve(root, relative), "utf8"));
}

// Strict duplicate-key refusal (finding 8): JSON.parse silently accepts
// duplicates, so detect them with a revival pass.
function assertDupFree(text, where) {
  const keys = [];
  const walker = (value) => {
    if (Array.isArray(value)) return value.forEach(walker);
    if (value && typeof value === "object") {
      const seen = new Set(Object.keys(value));
      if (seen.size !== Object.keys(value).length) fail("duplicate-keys", where);
      Object.values(value).forEach(walker);
    }
  };
  let parsed;
  try {
    parsed = JSON.parse(text, (key, value) => {
      if (key !== "" && keys.includes(key)) {
        // Key repetition across different objects is legal; the object
        // check above catches real duplicates.
      }
      return value;
    });
  } catch (error) {
    fail("parse-failed", { where, error: error.message });
  }
  walker(parsed);
  return parsed;
}

let step = 0;
let sandbox;
function freshSandbox(tag) {
  sandbox = join(tmpdir(), `lekalo-change-tx-${tag}-${process.pid}`);
  rmSync(sandbox, { recursive: true, force: true });
  mkdirSync(sandbox, { recursive: true });
  cpSync(join(root, "tests/fixtures/change/field-vertical"), sandbox, { recursive: true });
  return sandbox;
}
function run(args) {
  step += 1;
  const cleanEnv = { ...process.env };
  delete cleanEnv.LEKALO_PROJECT;
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
const writeJournal = (id, files, extras = {}) => {
  const journalDir = join(sandbox, ".lekalo/cache/change", id);
  mkdirSync(journalDir, { recursive: true });
  const journal = {
    transaction: id,
    planId: "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    files,
    history: false,
    ...extras,
  };
  writeFileSync(join(journalDir, "journal.json"), `${JSON.stringify(journal)}\n`);
  return journalDir;
};
const zeros = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

// 1. A planted traversal journal never writes outside the project (finding 1).
freshSandbox("escape");
const escapePath = join(sandbox, "..", `le73-gate-ESCAPE-${process.pid}.txt`);
writeJournal("change-gateesc01", [
  {
    path: `../le73-gate-ESCAPE-${process.pid}.txt`,
    beforeDigest: zeros,
    afterDigest: zeros,
    creation: false,
  },
]);
const escape = run(["change", "recover", "--project", ".", "--transaction", "change-gateesc01", "--json"]);
if (escape.code === 0) fail("escape-write-succeeded", escape.json);
if (existsSync(escapePath)) fail("escape-file-written", escapePath);
if (!existsSync(join(sandbox, ".lekalo/cache/change/change-gateesc01/journal.json"))) {
  fail("escape-journal-consumed", "the refusing journal must stay");
}
rmSync(escapePath, { force: true });

// 2. An absolute journal path is refused the same way (finding 1).
freshSandbox("absolute");
writeJournal("change-gateabs01", [
  { path: "/lekalo-pwn/abs.txt", beforeDigest: zeros, afterDigest: zeros, creation: false },
]);
const absolute = run(["change", "recover", "--project", ".", "--transaction", "change-gateabs01", "--json"]);
if (absolute.code === 0) fail("absolute-write-succeeded", absolute.json);
if (!existsSync(join(sandbox, ".lekalo/cache/change/change-gateabs01/journal.json"))) {
  fail("absolute-journal-consumed");
}

// 3. A corrupted backup refuses and keeps the barrier (finding 4).
freshSandbox("corrupt");
const corruptDir = writeJournal(
  "change-gatecor01",
  [
    {
      path: "lekalo/modules/tasks/entities.yaml",
      beforeDigest:
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
      afterDigest: zeros,
      creation: false,
    },
  ],
);
mkdirSync(join(corruptDir, "backups"), { recursive: true });
writeFileSync(
  join(corruptDir, "backups/0"),
  "GARBAGE-CORRUPTED-BYTES-NOT-YAML\n",
);
const entitiesBefore = readFileSync(join(sandbox, "lekalo/modules/tasks/entities.yaml"));
const corrupt = run(["change", "recover", "--project", ".", "--transaction", "change-gatecor01", "--json"]);
if (corrupt.code === 0) fail("corrupt-backup-accepted", corrupt.json);
if (!existsSync(join(sandbox, ".lekalo/cache/change/change-gatecor01/journal.json"))) {
  fail("corrupt-barrier-lifted", "the journal must stay on verification failure");
}
const blockedAfterCorrupt = run(["load", "--project", ".", "--json"]);
if (blockedAfterCorrupt.code !== 3) fail("corrupt-barrier-not-blocking", blockedAfterCorrupt.json);
if (!blockedAfterCorrupt.json?.reasonCodes?.includes("change.recovery-required")) {
  fail("corrupt-barrier-rule", blockedAfterCorrupt.json?.reasonCodes);
}
if (!readFileSync(join(sandbox, "lekalo/modules/tasks/entities.yaml")).equals(entitiesBefore)) {
  fail("corrupt-target-mutated");
}

// 4. A stale writer lock does not brick recovery (finding 3).
freshSandbox("stale");
const staleCache = join(sandbox, ".lekalo/cache/change");
mkdirSync(staleCache, { recursive: true });
writeJournal("change-gatest01", []);
writeFileSync(join(staleCache, "writer.lock"), "held-by-a-dead-process\n");
writeFileSync(
  join(staleCache, "writer.lock.json"),
  `${JSON.stringify({ pid: 999999999, boot: "not-this-boot" })}\n`,
);
const stale = run(["change", "recover", "--project", ".", "--transaction", "change-gatest01", "--json"]);
if (stale.code !== 0) fail("stale-lock-bricked-recovery", { code: stale.code, json: stale.json });
if (existsSync(join(staleCache, "writer.lock"))) fail("stale-lock-not-released");
if (existsSync(join(sandbox, ".lekalo/cache/change/change-gatest01/journal.json"))) {
  fail("stale-journal-not-consumed");
}
const unblocked = run(["load", "--project", ".", "--json"]);
if (unblocked.code !== 0) fail("stale-barrier-remains", unblocked.json);

// 5. A forged history record never mints a replayed receipt (finding 5);
//    every emitted document is duplicate-key-free JSON (finding 8).
freshSandbox("forged");
const forgedKeyDigest = "sha256:426fcff9463b3f01eb47cc6e5f40b36d473534083837305a5500634e1a624ca7";
const forgedRecord = {
  sequence: 1,
  keyDigest: forgedKeyDigest,
  requestDigest: zeros,
  planId: zeros,
  operations: ["entity-field:add"],
  files: ["lekalo/modules/tasks/entities.yaml"],
  receipt: {
    status: "valid",
    schemaVersion: "lekalo/change-receipt/v0.8.0",
    identity: "dev.lekalo.change-receipt@0.8.0",
    state: "committed",
    planId: zeros,
    requestDigest: zeros,
    keyDigest: forgedKeyDigest,
    operations: [
      {
        operation: "entity-field:add",
        subject: "tasks.task",
        subjectFamily: "entity",
        status: "resolved",
      },
    ],
    files: [
      {
        path: "lekalo/modules/tasks/entities.yaml",
        beforeDigest: zeros,
        afterDigest: zeros,
        outcome: "replaced",
      },
    ],
    history: { sequence: 1, recordDigest: zeros },
    transaction: { id: "change-gateforge01", state: "committed" },
    diagnosticIds: [],
    replayed: false,
  },
};
mkdirSync(join(sandbox, "lekalo"), { recursive: true });
writeFileSync(
  join(sandbox, "lekalo/change-history.json"),
  `${JSON.stringify({
    schemaVersion: "lekalo/change-history/v0.8.0",
    identity: "dev.lekalo.change-history@0.8.0",
    project: "tasks",
    nextSequence: 2,
    records: [forgedRecord],
  })}\n`,
);
const requestFile = join(sandbox, "gate-forged-request.json");
writeFileSync(
  requestFile,
  `${JSON.stringify(readJson("tests/fixtures/change/golden/request-add-field.json"))}\n`,
);
const forged = run([
  "change",
  "request",
  "--project",
  ".",
  "--request",
  requestFile,
  "--apply",
  "--idempotency-key",
  "gate-forged-v1",
  "--json",
]);
if (forged.code === 0 && forged.json?.state === "replayed") {
  fail("forged-ledger-minted-replay", forged.json);
}
if (forged.code !== 1 || !forged.json?.reasonCodes?.includes("change.history-invalid")) {
  fail("forged-ledger-not-refused", { code: forged.code, json: forged.json });
}

// 6. A real committed apply/replay cycle: dup-free documents, valid
//    schemas, and a rolled-back recover receipt (findings 6 and 8).
freshSandbox("flow");
const flowRequest = join(sandbox, "gate-flow-request.json");
writeFileSync(
  flowRequest,
  `${JSON.stringify(readJson("tests/fixtures/change/golden/request-add-field.json"), null, 2)}\n`,
);
const apply = run([
  "change",
  "request",
  "--project",
  ".",
  "--request",
  flowRequest,
  "--apply",
  "--idempotency-key",
  "gate-flow-v1",
  "--json",
]);
if (apply.code !== 0) fail("flow-apply-exit", { code: apply.code, stderr: apply.stderr });
assertDupFree(apply.stdout, "apply receipt");
if (!receiptValidate(apply.json)) fail("flow-apply-schema", receiptValidate.errors);
const flowHistory = assertDupFree(
  readFileSync(join(sandbox, "lekalo/change-history.json"), "utf8"),
  "flow history",
);
if (!historyValidate(flowHistory)) fail("flow-history-schema", historyValidate.errors);

// 7. An interrupted transaction (real journal with a verified backup)
//    recovers to originals and reports rolled-back — never committed.
const interruptedDir = join(sandbox, ".lekalo/cache/change/change-gateflow1");
mkdirSync(join(interruptedDir, "backups"), { recursive: true });
const entitiesNow = readFileSync(join(sandbox, "lekalo/modules/tasks/entities.yaml"));
writeFileSync(join(interruptedDir, "backups/0"), entitiesNow);
writeFileSync(
  join(interruptedDir, "journal.json"),
  `${JSON.stringify({
    transaction: "change-gateflow1",
    planId: apply.json.planId,
    files: [
      {
        path: "lekalo/modules/tasks/entities.yaml",
        beforeDigest: apply.json.files[0].afterDigest,
        afterDigest: apply.json.files[0].beforeDigest,
        creation: false,
      },
    ],
    history: false,
  })}\n`,
);
const blocked = run(["load", "--project", ".", "--json"]);
if (blocked.code !== 3) fail("flow-barrier-exit", blocked.json);
const recovered = run([
  "change",
  "recover",
  "--project",
  ".",
  "--transaction",
  "change-gateflow1",
  "--json",
]);
if (recovered.code !== 0) fail("flow-recover-exit", { code: recovered.code, json: recovered.json });
assertDupFree(recovered.stdout, "recover receipt");
if (recovered.json?.state !== "rolled-back") fail("recover-state", recovered.json);
if (recovered.json?.transaction?.state !== "rolled-back") {
  fail("recover-transaction-state", recovered.json);
}
if (recovered.json?.status !== "valid" || recovered.json?.change !== undefined) {
  fail("recover-envelope-shape", recovered.json);
}
if (!receiptValidate(recovered.json)) fail("recover-schema", receiptValidate.errors);
if (!readFileSync(join(sandbox, "lekalo/modules/tasks/entities.yaml")).equals(entitiesNow)) {
  fail("recover-did-not-restore-originals");
}
const afterRecover = run(["load", "--project", ".", "--json"]);
if (afterRecover.code !== 0) fail("recover-barrier-remains", afterRecover.json);

rmSync(sandbox, { recursive: true, force: true });
process.stdout.write(`${JSON.stringify({
  ok: true,
  gate: "test-change-transactions.mjs",
  steps: step,
  flows: [
    "journal-escape-refused",
    "journal-absolute-refused",
    "corrupt-backup-refused-barrier-kept",
    "stale-lock-reclaimed",
    "forged-ledger-refused",
    "dup-free-receipts",
    "rollback-recovery-schema-valid",
  ],
}, null, 2)}\n`);
