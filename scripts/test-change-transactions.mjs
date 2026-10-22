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
// Ajv 8.20.0 is provisioned outside the checkout like the other gates.

import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, openSync, closeSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
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
if (ajvVersion !== "8.20.0") {
  process.stderr.write(`ajv ${ajvVersion} is not the pinned 8.20.0 contract gate\n`);
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

// Strict duplicate-key refusal (finding 8; made real in r6, omp F3):
// JSON.parse silently collapses duplicates, so post-parse key-set checks
// are tautological. Duplicates are detected over the RAW text with a
// per-object scanner that pairs keys to their enclosing object braces.
function assertDupFree(text, where) {
  // Token-walk the raw JSON: each '{' opens an object whose keys are the
  // string literals immediately followed by ':'. A repeated key inside
  // one object depth is a duplicate — the same property the Rust
  // `unique_json` ingress refuses.
  let depth = 0;
  const scopes = new Map();
  let i = 0;
  const n = text.length;
  while (i < n) {
    const ch = text[i];
    if (ch === '"') {
      // Scan the full string literal.
      let j = i + 1;
      while (j < n) {
        if (text[j] === '\\') { j += 2; continue; }
        if (text[j] === '"') break;
        j += 1;
      }
      const literal = text.slice(i + 1, j);
      // A key is a string followed only by whitespace and ':'.
      let k = j + 1;
      while (k < n && /\s/.test(text[k])) k += 1;
      if (text[k] === ':') {
        if (!scopes.has(depth)) scopes.set(depth, new Set());
        const seen = scopes.get(depth);
        if (seen.has(literal)) fail("duplicate-keys", { where, key: literal });
        seen.add(literal);
      }
      i = j + 1;
      continue;
    }
    if (ch === '{' || ch === '[') {
      depth += 1;
      scopes.set(depth, new Set());
    } else if (ch === '}' || ch === ']') {
      scopes.delete(depth);
      depth -= 1;
    }
    i += 1;
  }
  let parsed;
  try {
    parsed = JSON.parse(text);
  } catch (error) {
    fail("parse-failed", { where, error: error.message });
  }
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
const writeJournal = (id, files, extras = {}, provenance = true) => {
  const journalDir = join(sandbox, ".lekalo/cache/change", id);
  mkdirSync(journalDir, { recursive: true });
  const journal = {
    schemaVersion: "lekalo/change-journal/1",
    transaction: id,
    planId: "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    state: "replacing",
    files,
    history: false,
    ...extras,
  };
  writeFileSync(join(journalDir, "journal.json"), `${JSON.stringify(journal)}\n`);
  if (provenance) {
    // Round 5: the durable provenance record (merged by transaction) a
    // legitimate recovery verifies the journal against.
    const { schemaVersion: _version, state, ...pins } = journal;
    const registryPath = join(sandbox, ".lekalo/pending-transactions.json");
    let records = [];
    if (existsSync(registryPath)) {
      const existing = JSON.parse(readFileSync(registryPath, "utf8"));
      records = existing.records ?? [];
    }
    records = records.filter((record) => record.transaction !== id);
    records.push({ ...pins, state });
    writeFileSync(
      registryPath,
      `${JSON.stringify({ schemaVersion: "lekalo/change-pending/1", records })}\n`,
    );
  }
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

// 4. A stale writer lock does not brick recovery (round 1 finding 3):
//    the OS-held lock proves the owner is dead and the artifact is taken
//    over; a planted metadata file never substitutes for liveness.
//    (r6, devin F2 + omp F2): the writer lock moved to the shared
//    `.lekalo/cache/writer.lock` leaf; it is persistent — never unlinked
//    — so recovery consumes the journal, and the stale artifact bytes
//    are simply adopted-and-ignored (no holder metadata is written).
freshSandbox("stale");
const staleCache = join(sandbox, ".lekalo/cache/change");
const staleSharedLeaf = join(sandbox, ".lekalo/cache/writer.lock");
mkdirSync(staleCache, { recursive: true });
writeJournal("change-gatest01", []);
writeFileSync(staleSharedLeaf, "held-by-a-dead-process\n");
writeFileSync(
  join(staleCache, "writer.lock.json"),
  `${JSON.stringify({ pid: 999999999, boot: "not-this-boot" })}\n`,
);
const stale = run(["change", "recover", "--project", ".", "--transaction", "change-gatest01", "--json"]);
if (stale.code !== 0) fail("stale-lock-bricked-recovery", { code: stale.code, json: stale.json });
if (!existsSync(staleSharedLeaf)) fail("shared-lock-leaf-unlinked");
if (existsSync(join(sandbox, ".lekalo/cache/change/change-gatest01/journal.json"))) {
  fail("stale-journal-not-consumed");
}
const unblocked = run(["load", "--project", ".", "--json"]);
if (unblocked.code !== 0) fail("stale-barrier-remains", unblocked.json);

// 4b. An untouched staging transaction (a real interruption during
//     staging, round 2 finding 4) recovers without touching any source
//     byte: the transaction aborts and the barrier lifts.
freshSandbox("staging");
const stagingEntities = join(sandbox, "lekalo/modules/tasks/entities.yaml");
const stagingBefore = readFileSync(stagingEntities);
writeJournal("change-gatestg1", [
  {
    path: "lekalo/modules/tasks/entities.yaml",
    beforeDigest: zeros,
    afterDigest:
      "sha256:1111111111111111111111111111111111111111111111111111111111111111",
    creation: false,
  },
], { state: "staging" });
const staging = run(["change", "recover", "--project", ".", "--transaction", "change-gatestg1", "--json"]);
if (staging.code !== 0) fail("staging-recovery-failed", { code: staging.code, json: staging.json });
if (staging.json?.state !== "rolled-back") fail("staging-state", staging.json);
if (staging.json?.files?.length !== 0) fail("staging-files", staging.json);
if (!readFileSync(stagingEntities).equals(stagingBefore)) fail("staging-target-touched");
if (existsSync(join(sandbox, ".lekalo/cache/change/change-gatestg1"))) {
  fail("staging-journal-kept");
}

// 4c. A forged creation entry for an unrelated file is refused (round 2
//     finding 3): deletion authority stays with the admitted history
//     home, and the barrier is retained.
freshSandbox("forged-creation");
const sentinel = join(sandbox, "UNRELATED-SENTINEL.txt");
writeFileSync(sentinel, "UNRELATED\n");
writeJournal("change-gateforg1", [
  {
    path: "UNRELATED-SENTINEL.txt",
    beforeDigest: zeros,
    afterDigest:
      "sha256:2222222222222222222222222222222222222222222222222222222222222222",
    creation: true,
  },
]);
const forgedCreation = run(["change", "recover", "--project", ".", "--transaction", "change-gateforg1", "--json"]);
if (forgedCreation.code === 0) fail("forged-creation-accepted", forgedCreation.json);
if (!existsSync(sentinel)) fail("forged-creation-deleted", sentinel);
if (!existsSync(join(sandbox, ".lekalo/cache/change/change-gateforg1/journal.json"))) {
  fail("forged-creation-barrier-lifted");
}

// 4d. Round 5, finding 1: a fully self-consistent forged journal (real
//     backup bytes, pins matching the live project) has ZERO recovery
//     authority without the durable provenance record under `.lekalo` —
//     the overwrite exploit restores nothing and the delete exploit
//     removes nothing; both keep the barrier with the journal intact.
//     A journal planted via clone/checkout of the cache tree alone can
//     never mint authority.
freshSandbox("provenance");
const provEntities = join(sandbox, "lekalo/modules/tasks/entities.yaml");
const provOriginal = readFileSync(provEntities);
const provAttack = Buffer.concat([provOriginal, Buffer.from("# ATTACKER-CONTROLLED LINE\n")]);
const provDigest = (bytes) => `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
const overwriteDir = writeJournal(
  "change-gateprov1",
  [
    {
      path: "lekalo/modules/tasks/entities.yaml",
      beforeDigest: provDigest(provAttack),
      afterDigest: provDigest(provOriginal),
      creation: false,
    },
  ],
  {},
  false,
);
mkdirSync(join(overwriteDir, "backups"), { recursive: true });
writeFileSync(join(overwriteDir, "backups/0"), provAttack);
const overwrite = run(["change", "recover", "--project", ".", "--transaction", "change-gateprov1", "--json"]);
if (overwrite.code === 0) fail("forged-overwrite-accepted", overwrite.json);
if (!overwrite.json?.reasonCodes?.includes("change.commit-failed")) {
  fail("forged-overwrite-rule", { code: overwrite.code, json: overwrite.json });
}
if (!readFileSync(provEntities).equals(provOriginal)) fail("forged-overwrite-mutated");
if (!existsSync(join(overwriteDir, "journal.json"))) fail("forged-overwrite-barrier-lifted");
const deleteDir = writeJournal(
  "change-gateprov2",
  [
    {
      path: "lekalo/modules/tasks/entities.yaml",
      beforeDigest: zeros,
      afterDigest: provDigest(provOriginal),
      creation: true,
    },
  ],
  {},
  false,
);
const forgedDelete = run(["change", "recover", "--project", ".", "--transaction", "change-gateprov2", "--json"]);
if (forgedDelete.code === 0) fail("forged-delete-accepted", forgedDelete.json);
if (!existsSync(provEntities)) fail("forged-delete-removed-target");
if (!existsSync(join(deleteDir, "journal.json"))) fail("forged-delete-barrier-lifted");
const provBlocked = run(["load", "--project", ".", "--json"]);
if (provBlocked.code !== 3) fail("provenance-barrier-not-blocking", provBlocked.json);

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

// 7. An interrupted transaction (real journal with a verified backup,
//    durable `replacing` state) recovers to originals and reports
//    rolled-back — never committed (round 2 findings 4 and 5).
const interruptedDir = join(sandbox, ".lekalo/cache/change/change-gateflow1");
mkdirSync(join(interruptedDir, "backups"), { recursive: true });
const entitiesNow = readFileSync(join(sandbox, "lekalo/modules/tasks/entities.yaml"));
writeFileSync(join(interruptedDir, "backups/0"), entitiesNow);
const interruptedFiles = [
  {
    path: "lekalo/modules/tasks/entities.yaml",
    beforeDigest: apply.json.files[0].afterDigest,
    afterDigest: apply.json.files[0].beforeDigest,
    creation: false,
  },
];
writeFileSync(
  join(interruptedDir, "journal.json"),
  `${JSON.stringify({
    schemaVersion: "lekalo/change-journal/1",
    transaction: "change-gateflow1",
    planId: apply.json.planId,
    state: "replacing",
    files: interruptedFiles,
    history: false,
  })}\n`,
);
// Round 5: the matching durable provenance record.
writeFileSync(
  join(sandbox, ".lekalo/pending-transactions.json"),
  `${JSON.stringify({
    schemaVersion: "lekalo/change-pending/1",
    records: [
      {
        transaction: "change-gateflow1",
        planId: apply.json.planId,
        state: "replacing",
        files: interruptedFiles,
        history: false,
      },
    ],
  })}\n`,
);
const blocked = run(["load", "--project", ".", "--json"]);
if (blocked.code !== 3) fail("flow-barrier-exit", blocked.json);
// Round 6, finding 3: while the journal and its verified backup are the
// only recovery authority, `cache clear --yes` must refuse outright and
// remove nothing — the custody tree stays and the barrier stays.
const clearDuringTx = run(["cache", "clear", "--yes", "--json"]);
if (clearDuringTx.code !== 3) fail("clear-during-transaction-exit", clearDuringTx.json);
if (!clearDuringTx.json?.reasonCodes?.includes("change.recovery-required")) {
  fail("clear-during-transaction-rule", clearDuringTx.json?.reasonCodes);
}
if (!existsSync(join(interruptedDir, "journal.json"))) fail("clear-consumed-journal");
if (!existsSync(join(interruptedDir, "backups/0"))) fail("clear-consumed-backup");
const stillBlocked = run(["load", "--project", ".", "--json"]);
if (stillBlocked.code !== 3) fail("clear-lifted-barrier", stillBlocked.json);
// While the replacing journal is pending, the ledger is gated: a recorded
// receipt must not read as committed (round 2 finding 9).
const gatedReceipt = run([
  "change",
  "receipt",
  "--project",
  ".",
  "--idempotency-key",
  "gate-flow-v1",
  "--json",
]);
if (gatedReceipt.code !== 3) fail("receipt-not-gated", gatedReceipt.json);
if (!gatedReceipt.json?.reasonCodes?.includes("change.recovery-required")) {
  fail("receipt-gate-rule", gatedReceipt.json?.reasonCodes);
}
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
// With no live transaction the clear succeeds again, and it still never
// touches the change custody home (round 6, finding 3: exemption is the
// second line of defense; recovery residue is not disposable cache).
mkdirSync(join(sandbox, ".lekalo/cache/change/leftover-tx/backups"), { recursive: true });
writeFileSync(
  join(sandbox, ".lekalo/cache/change/leftover-tx/backups/0"),
  "recovered-transaction residue\n",
);
const clearAfterRecover = run(["cache", "clear", "--yes", "--json"]);
if (clearAfterRecover.code !== 0) fail("clear-after-recovery-exit", clearAfterRecover.json);
if (!existsSync(join(sandbox, ".lekalo/cache/change/leftover-tx/backups/0"))) {
  fail("clear-removed-change-home");
}

// 4c. Shared writer serialization (r6, findings devin F2 + omp F2): a
//     sibling service's writer refuses while the change journal awaits
//     recovery — the registered busy rule (exit 3, denied) with the
//     change recovery rule for the change family's own journal. A live
//     holder of the shared `.lekalo/cache/writer.lock` also denies the
//     change writer itself (cross-service exclusion is real, not just
//     the reader barrier).
freshSandbox("serialize");
writeJournal("change-gateser01", []);
const migrateWhileBlocked = run(["migrate", "--project", ".", "--to", "0.7.0", "--json"]);
if (migrateWhileBlocked.code === 0) fail("migrate-ignored-change-journal", migrateWhileBlocked.json);
const formatWhileBlocked = run(["format", "--project", ".", "--json"]);
if (formatWhileBlocked.code === 0) fail("format-ignored-change-journal", formatWhileBlocked.json);
const serializeRequest = join(sandbox, "serialize-request.json");
writeFileSync(
  serializeRequest,
  `${JSON.stringify(readJson("tests/fixtures/change/golden/request-add-field.json"))}\n`,
);
const changeWhileBlocked = run([
  "change", "request", "--project", ".",
  "--request", serializeRequest,
  "--apply", "--idempotency-key", "gate-serialize-01",
  "--json",
]);
if (changeWhileBlocked.code !== 3) fail("change-journal-barrier-exit", changeWhileBlocked.json);
if (!changeWhileBlocked.json?.reasonCodes?.includes("change.recovery-required")) {
  fail("change-journal-barrier-rule", changeWhileBlocked.json?.reasonCodes);
}
const sourceBefore = readFileSync(join(sandbox, "lekalo/modules/tasks/entities.yaml"));
writeJournal("change-gateser02", [], {}, false);
// The shared lock's leaf persists; recovery of the second planted journal
// (no provenance) still refuses, and the sources stay untouched.
const recoverSecond = run(["change", "recover", "--project", ".", "--transaction", "change-gateser02", "--json"]);
if (recoverSecond.code === 0) fail("provenance-less-journal-recovered", recoverSecond.json);
if (!readFileSync(join(sandbox, "lekalo/modules/tasks/entities.yaml")).equals(sourceBefore)) {
  fail("serialize-sources-mutated");
}

// 4d. Adoption journals join the shared-writer sibling barrier (r8,
//     devin F1/F3 + omp F3): before r8 the `.lekalo/import/adoption/
//     recovery` journals were in NO barrier set — a stranded
//     installation serialized against nothing. A planted stranded
//     journal (no receipt, no recovery progress) now refuses every
//     non-adoption mutating writer under the shared lock; each family
//     projects onto its own registered rule. Model readers stay open by
//     contract (see the loader's Phase 2c note), and the adoption
//     recovery view — the stranded-state diagnostic — stays reachable
//     through its root-only resolution.
freshSandbox("adoptbarrier");
const strandedPlan = `sha256:${"a".repeat(32)}${"b".repeat(32)}`;
const adoptionHome = join(sandbox, ".lekalo/import/adoption/recovery", "a".repeat(32), "b".repeat(32));
mkdirSync(adoptionHome, { recursive: true });
writeFileSync(
  join(adoptionHome, "journal.json"),
  JSON.stringify({
    schema_version: "lekalo/adoption-journal/v0.8.0",
    plan: strandedPlan,
    phase: "apply",
    steps: [],
  }, null, 2),
);
const serializeRequest2 = join(sandbox, "adoptbarrier-request.json");
writeFileSync(
  serializeRequest2,
  `${JSON.stringify(readJson("tests/fixtures/change/golden/request-add-field.json"))}\n`,
);
const changeBlockedByAdoption = run([
  "change", "request", "--project", ".",
  "--request", serializeRequest2,
  "--apply", "--idempotency-key", "gate-adoptbar-01",
  "--json",
]);
if (changeBlockedByAdoption.code !== 3) fail("change-ignored-adoption-journal", changeBlockedByAdoption.json);
if (!changeBlockedByAdoption.json?.reasonCodes?.includes("change.precondition-failed")) {
  fail("change-adoption-barrier-rule", changeBlockedByAdoption.json?.reasonCodes);
}
// The migration-rollback entry is one of the r8-serialized writers: it
// joins the shared lock after its load and refuses with the serialized
// rule while the adoption journal is stranded.
const migrateRollbackBlockedByAdoption = run([
  "migrate", "--project", ".", "--rollback", "e".repeat(64), "--json",
]);
if (
  migrateRollbackBlockedByAdoption.code !== 1 ||
  !migrateRollbackBlockedByAdoption.json?.reasonCodes?.includes("versioning.migration-in-progress")
) {
  fail("migrate-rollback-adoption-barrier", migrateRollbackBlockedByAdoption.json);
}
// The formatter joins the shared lock before its own load, so the
// stranded journal refuses it under the sibling barrier — projected,
// like every format writer refusal, onto the family's own denied-
// allowed `format.locked` rule (exit 3).
const formatBlockedByAdoption = run(["format", "--project", ".", "--json"]);
if (
  formatBlockedByAdoption.code !== 3 ||
  !formatBlockedByAdoption.json?.reasonCodes?.includes("format.locked")
) {
  fail("format-adoption-barrier", formatBlockedByAdoption.json);
}
// The stranded-state diagnostic stays reachable (root-only resolution):
// the view answers for the planted journal instead of being blocked by
// the reader barrier its own resolution must pass.
const adoptionView = run(["adoption", "recovery", "--digest", strandedPlan, "--project", ".", "--json"]);
if (adoptionView.code !== 0) fail("adoption-view-blocked", adoptionView.json ?? adoptionView.stderr);
// Resolving the journal (deleting it stands in for a completed adoption
// recovery) restores every sibling writer.
rmSync(join(adoptionHome, "journal.json"), { force: true });
const changeAfterAdoptionRecovery = run([
  "change", "request", "--project", ".",
  "--request", serializeRequest2,
  "--apply", "--idempotency-key", "gate-adoptbar-02",
  "--json",
]);
if (changeAfterAdoptionRecovery.code !== 0) fail("change-blocked-after-adoption-recovery", changeAfterAdoptionRecovery.json);

// 4e. The mutating adoption and migration-rollback entries join the
//     shared mutation lock (r8, devin F1/F3 + omp F3), and the adoption
//     busy rule is the family's own denied-allowed rule (r8, devin F4 +
//     omp F4) — never a denied envelope carrying an invalid-only rule.
//     A live holder of the shared leaf blocks `adoption recovery
//     --restore` and `migrate --rollback` exactly like the change
//     writer. The OS-held-lock leg runs where an open handle really
//     excludes (Windows share modes); elsewhere it skips loudly like
//     the junction-custody flow, and the Rust suites pin exclusivity.
let heldLockFlow = "shared-writer-held-live (skipped: POSIX handle sharing does not exclude flock)";
if (process.platform === "win32") {
  heldLockFlow = "shared-writer-held-live";
  freshSandbox("heldlock");
  const lockLeaf = join(sandbox, ".lekalo/cache/writer.lock");
  mkdirSync(dirname(lockLeaf), { recursive: true });
  writeFileSync(lockLeaf, "");
  const handle = openSync(lockLeaf, "r");
  const adoptionUnderLock = run([
    "adoption", "recovery", "--digest", `sha256:${"c".repeat(64)}`,
    "--restore", "--project", ".", "--json",
  ]);
  if (adoptionUnderLock.code !== 3 || !adoptionUnderLock.json?.reasonCodes?.includes("adoption.writer-locked")) {
    fail("adoption-restore-under-held-lock", adoptionUnderLock.json ?? adoptionUnderLock.stderr);
  }
  const migrateRollbackUnderLock = run([
    "migrate", "--project", ".", "--rollback", "d".repeat(64), "--json",
  ]);
  if (
    migrateRollbackUnderLock.code !== 1 ||
    !migrateRollbackUnderLock.json?.reasonCodes?.includes("versioning.migration-in-progress")
  ) {
    fail("migrate-rollback-under-held-lock", migrateRollbackUnderLock.json ?? migrateRollbackUnderLock.stderr);
  }
  closeSync(handle);
  // The lock was the refusal cause: without the holder the same restore
  // reaches its own journal-missing refusal instead.
  const adoptionAfterRelease = run([
    "adoption", "recovery", "--digest", `sha256:${"c".repeat(64)}`,
    "--restore", "--project", ".", "--json",
  ]);
  if (adoptionAfterRelease.code === 3 && adoptionAfterRelease.json?.reasonCodes?.includes("adoption.writer-locked")) {
    fail("adoption-restore-still-locked-after-release", adoptionAfterRelease.json);
  }
}

// r10 F1/F2: the public chain/tip can be recomputed by a ledger editor.
// Only an actual local transaction may publish independent replay custody.
const canonical = (value) => Array.isArray(value) ? value.map(canonical)
  : value && typeof value === "object" ? Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])])) : value;
function rechain(document) {
  let previous = provDigest("");
  for (const record of document.records) {
    const projection = structuredClone(record.receipt);
    delete projection.history.recordDigest;
    previous = provDigest(`${previous}\x1f${document.project}\x1f${record.intentDigest}\x1f${JSON.stringify(canonical(projection))}`);
    record.receipt.history.recordDigest = previous;
  }
  document.tip = { sequence: document.records.length, recordDigest: previous };
}
function applyField(name, key) {
  return run(["change", "entity-field:add", "--project", ".", "--entity", "tasks.task",
    "--name", name, "--type", "tasks.priority", "--apply", "--idempotency-key", key, "--json"]);
}
freshSandbox("custody-donor");
const donorCommit = applyField("priority", "r10-custody");
if (donorCommit.code !== 0) fail("r10-donor-commit", donorCommit.json);
const genuine = JSON.parse(readFileSync(join(sandbox, "lekalo/change-history.json"), "utf8"));
const nextCommit = applyField("escalation", "r10-successor");
if (nextCommit.code !== 0) fail("r10-successor-commit", nextCommit.json);
const withSuccessor = JSON.parse(readFileSync(join(sandbox, "lekalo/change-history.json"), "utf8"));
// Plain tail deletion changes no record hash, but MUST fail both receipt
// lookups rather than silently reporting committed/unknown.
const truncated = structuredClone(withSuccessor);
truncated.records.pop();
truncated.nextSequence = 2;
writeFileSync(join(sandbox, "lekalo/change-history.json"), JSON.stringify(truncated));
for (const key of ["r10-custody", "r10-successor"]) {
  const result = run(["change", "receipt", "--project", ".", "--idempotency-key", key, "--json"]);
  if (result.code !== 1 || !result.json?.reasonCodes?.includes("change.history-invalid")) fail("r10-tail-truncation", result);
}
rmSync(sandbox, { recursive: true, force: true });
for (const attack of ["current-digest", "different-file", "empty-files", "fake-no-op", "later-successor"]) {
  freshSandbox(`custody-${attack}`);
  const source = join(sandbox, "lekalo/modules/tasks/entities.yaml");
  const before = readFileSync(source);
  const document = structuredClone(attack === "later-successor" ? withSuccessor : genuine);
  const record = document.records[0];
  if (attack === "current-digest") {
    record.receipt.files[0].afterDigest = provDigest(before);
    record.receipt.files[0].beforeDigest = provDigest("invented preimage");
  } else if (attack === "different-file") {
    record.files = ["lekalo/project.yaml"];
    record.receipt.files[0].path = "lekalo/project.yaml";
    record.receipt.files[0].afterDigest = provDigest(readFileSync(join(sandbox, "lekalo/project.yaml")));
  } else if (attack === "later-successor") {
    document.records[1].receipt.files[0].afterDigest = provDigest(before);
  } else {
    record.files = [];
    record.receipt.files = [];
    if (attack === "fake-no-op") record.receipt.state = "no-op";
  }
  rechain(document);
  if (attack !== "empty-files" && !historyValidate(document)) fail("r10-forgery-shape", historyValidate.errors);
  writeFileSync(join(sandbox, "lekalo/change-history.json"), JSON.stringify(document));
  const result = applyField("priority", "r10-custody");
  if (result.code !== 1 || !result.json?.reasonCodes?.includes("change.history-invalid")) fail(`r10-forged-${attack}`, result);
  if (!readFileSync(source).equals(before)) fail("r10-forgery-mutated-sources", attack);
  rmSync(sandbox, { recursive: true, force: true });
}

// r10 F3: exact crash state after the first of TWO recovery restorations.
// A prefix with every PRESENT row restored must not unblock a sibling.
freshSandbox("partial-adoption-recovery");
const partialPlan = `sha256:${"c".repeat(32)}${"d".repeat(32)}`;
const partialHomeRelative = `.lekalo/import/adoption/recovery/${"c".repeat(32)}/${"d".repeat(32)}`;
const partialHome = join(sandbox, partialHomeRelative);
mkdirSync(join(partialHome, "snapshots"), { recursive: true });
const restorationRows = ["one.txt", "two.txt"].map((path) => {
  const snapshot = `${partialHomeRelative}/snapshots/${createHash("sha256").update(path).digest("hex").slice(0, 32)}.bin`;
  writeFileSync(join(sandbox, snapshot), "");
  return { path, state: "written", snapshot, action: "create", before: null, after: provDigest("installed") };
});
writeFileSync(join(sandbox, "two.txt"), "installed"); // first path already restored (removed)
const originalJournal = { schema_version: "lekalo/adoption-journal/v0.8.0", plan: partialPlan, phase: "apply", steps: restorationRows };
writeFileSync(join(partialHome, "journal.json"), JSON.stringify(originalJournal));
writeFileSync(join(partialHome, "recovery.json"), JSON.stringify({
  ...originalJournal, phase: "recovery", steps: [{ ...restorationRows[0], action: undefined, state: "restored" }],
}));
const receiptHome = join(sandbox, ".lekalo/import/adoption/receipts", "c".repeat(32));
mkdirSync(receiptHome, { recursive: true });
const receiptLeaf = join(receiptHome, `${"d".repeat(32)}.json`);
for (const malformed of ["", "{}", '{"outcome":"completed"}']) {
  writeFileSync(receiptLeaf, malformed);
  const refused = applyField("priority", "r10-partial-adoption");
  if (refused.code !== 3 || !refused.json?.reasonCodes?.includes("change.precondition-failed")) fail("r10-partial-adoption-unblocked", refused);
  if (!existsSync(join(sandbox, "two.txt"))) fail("r10-second-path-not-preserved");
}
rmSync(receiptLeaf);
const restored = run(["adoption", "recovery", "--project", ".", "--digest", partialPlan, "--restore", "--json"]);
if (restored.code !== 0) fail("r10-adoption-resume", restored);
if (existsSync(join(sandbox, "two.txt"))) fail("r10-adoption-incomplete");
const afterRecovery = applyField("priority", "r10-partial-adoption");
if (afterRecovery.code !== 0) fail("r10-complete-adoption-still-blocked", afterRecovery);
// The live cache-clear command must preserve lock-leaf identity/custody.
const cleared = run(["cache", "clear", "--project", ".", "--yes", "--json"]);
if (cleared.code !== 0 || !existsSync(join(sandbox, ".lekalo/cache/writer.lock"))) fail("r10-cache-clear-unlinked-writer-lock", cleared);
if (!existsSync(join(sandbox, ".lekalo/change-history-pin.json"))) fail("r10-cache-clear-lost-commit-custody");
const replayAfterClear = applyField("priority", "r10-partial-adoption");
if (replayAfterClear.code !== 0 || replayAfterClear.json?.state !== "replayed") fail("r10-replay-after-clear", replayAfterClear);

rmSync(sandbox, { recursive: true, force: true });

// r11 F2: real no-op history, not a schema-only fixture. Requested rows
// survive both a wholly unchanged signature and a mixed ordered batch.
freshSandbox("noop-audit");
const commandPath = join(sandbox, "lekalo/modules/tasks/commands.yaml");
const commandBytes = 'schema_version: "0.2.16"\ndefinitions:\n  - id: tasks.act\n    kind: command\n    version: 1\n    input:\n      - name: title\n        type: tasks.text\n';
writeFileSync(commandPath, commandBytes);
const noop = { operation: "operation-input:set", operationTarget: "tasks.act",
  definition: { input: [{ name: "title", type: { ref: "tasks.text" } }] } };
function applyBatch(operations, key) {
  writeFileSync(join(sandbox, "batch.json"), JSON.stringify({ schemaVersion: "lekalo/change-request/v0.8.0", operations }));
  return run(["change", "request", "--project", ".", "--request", "batch.json", "--apply", "--idempotency-key", key, "--json"]);
}
const noopCommit = applyBatch([noop], "noop-key");
if (noopCommit.code !== 0 || noopCommit.json?.state !== "no-op" || noopCommit.json.operations.length !== 1) fail("r11-noop-row", noopCommit);
if (!receiptValidate(noopCommit.json)) fail("r11-noop-receipt-schema", receiptValidate.errors);
const noopHistory = JSON.parse(readFileSync(join(sandbox, "lekalo/change-history.json"), "utf8"));
if (!historyValidate(noopHistory)) fail("r11-noop-history-schema", historyValidate.errors);
if (noopHistory.records[0].operations.length !== 1 || noopHistory.records[0].receipt.operations.length !== 1) fail("r12-fresh-noop-empty-audit", noopHistory);
const noopReplay = applyBatch([noop], "noop-key");
if (noopReplay.code !== 0 || noopReplay.json?.state !== "replayed" || noopReplay.json.operations.length !== 1 ||
    !receiptValidate(noopReplay.json)) fail("r12-fresh-noop-replay", noopReplay);
const mixed = [noop, { operation: "entity-field:add", entity: "tasks.task", field: { name: "mixed", type: { ref: "tasks.text" } } }, noop];
const mixedCommit = applyBatch(mixed, "mixed-key");
if (mixedCommit.code !== 0 || mixedCommit.json?.state !== "committed") fail("r11-mixed-commit", mixedCommit);
const mixedHistory = JSON.parse(readFileSync(join(sandbox, "lekalo/change-history.json"), "utf8"));
if (!historyValidate(mixedHistory)) fail("r11-mixed-history-schema", historyValidate.errors);
if (JSON.stringify(mixedCommit.json.operations.map((row) => row.operation)) !== JSON.stringify(mixed.map((row) => row.operation)) ||
    JSON.stringify(mixedHistory.records[1].operations) !== JSON.stringify(mixed.map((row) => row.operation))) fail("r11-mixed-order", mixedCommit.json);
if (readFileSync(commandPath, "utf8") !== commandBytes) fail("r11-noop-source-churn");
rmSync(sandbox, { recursive: true, force: true });

// r12 F1/F2: genuine r10 output, retained verbatim rather than synthesized
// from a modern no-op. Empty operation arrays must never acquire custody
// or contaminate a successful append, even with a matching legacy/local pin.
function canonicalSnapshot(home) {
  return readdirSync(home, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))
    .flatMap((entry) => entry.isDirectory() ? canonicalSnapshot(join(home, entry.name))
      : [[join(home, entry.name), readFileSync(join(home, entry.name)).toString("hex")]]);
}
const legacyBytes = readFileSync(join(root, "tests/fixtures/change/legacy/r10-no-op-history.json"));
if (provDigest(legacyBytes) !== "sha256:7642c43decb40d5275b4727ab8defb76bafb3e71eda404c5608cbb411d4786a9") fail("r12-r10-fixture-byte-pin");
const legacyHistory = JSON.parse(legacyBytes);
if (historyValidate(legacyHistory) ||
    !["/records/0/operations", "/records/0/receipt/operations"].every((path) =>
      historyValidate.errors.some((error) => error.instancePath === path && error.keyword === "minItems"))) fail("r12-r10-fixture-must-be-invalid", historyValidate.errors);
for (const pinVersion of [null, 1, 2]) {
  freshSandbox(`r10-noop-upgrade-${pinVersion ?? "checkout"}`);
  writeFileSync(join(sandbox, "lekalo/modules/tasks/commands.yaml"), commandBytes);
  writeFileSync(join(sandbox, "lekalo/change-history.json"), legacyBytes);
  const legacyPinPath = join(sandbox, ".lekalo/change-history-pin.json");
  if (pinVersion !== null) {
    mkdirSync(dirname(legacyPinPath), { recursive: true });
    writeFileSync(legacyPinPath, JSON.stringify({ schemaVersion: `lekalo/change-history-pin/${pinVersion}`,
      project: "tasks", recordCount: 1, tipDigest: legacyHistory.tip.recordDigest, historyDigest: provDigest(legacyBytes),
      ...(pinVersion === 2 ? { firstLocalSequence: 2 } : {}) }));
  }
  const canonicalBefore = JSON.stringify(canonicalSnapshot(join(sandbox, "lekalo")));
  const pinBefore = existsSync(legacyPinPath) ? readFileSync(legacyPinPath) : null;
  const assertUnchanged = () => {
    if (JSON.stringify(canonicalSnapshot(join(sandbox, "lekalo"))) !== canonicalBefore ||
        (pinBefore === null ? existsSync(legacyPinPath) : !readFileSync(legacyPinPath).equals(pinBefore))) fail("r12-upgrade-mutated-originals", pinVersion);
    if (existsSync(join(sandbox, ".lekalo/pending-transactions.json")) || existsSync(join(sandbox, ".lekalo/cache/change"))) fail("r12-upgrade-started-transaction", pinVersion);
  };
  const assertRefused = (result) => {
    if (result.code !== 1 || result.json?.status !== "invalid" ||
        !result.json?.reasonCodes?.includes("change.history-invalid") ||
        result.json?.diagnostics?.[0]?.data?.detail !== "history-schema") fail("r12-upgrade-refusal", { pinVersion, result });
    assertUnchanged();
  };
  assertRefused(run(["change", "custody-init", "--project", ".", "--expect-history", provDigest(legacyBytes), "--json"]));
  assertRefused(applyField("priority", "after-r10-upgrade"));
  assertRefused(applyBatch([noop], "after-r10-noop"));
  assertRefused(applyBatch([noop], "base-noop"));
  assertRefused(run(["change", "receipt", "--project", ".", "--idempotency-key", "base-noop", "--json"]));
  const readOnly = run(["load", "--project", ".", "--json"]);
  if (readOnly.code !== 0) fail("r12-upgrade-blocked-model-reader", readOnly);
  assertUnchanged();
  rmSync(sandbox, { recursive: true, force: true });
}

// Also reject a schema-invalid later row in an otherwise valid imported
// prefix: foreign members, malformed discriminators, bounds and row types.
for (const mutation of ["foreign-member", "foreign-family", "malformed-operation", "missing-status", "over-count", "non-object"]) {
  freshSandbox(`operation-schema-${mutation}`);
  const document = structuredClone(mixedHistory);
  const record = document.records[1];
  const row = record.receipt.operations[0];
  if (mutation === "foreign-member") row.foreign = true;
  if (mutation === "foreign-family") row.subjectFamily = "foreign";
  if (mutation === "malformed-operation") { row.operation = "foreign"; record.operations[0] = "foreign"; }
  if (mutation === "missing-status") delete row.status;
  if (mutation === "over-count") {
    record.operations = Array(101).fill(row.operation);
    record.receipt.operations = Array.from({ length: 101 }, () => structuredClone(row));
  }
  if (mutation === "non-object") record.receipt.operations[0] = row.operation;
  rechain(document);
  if (historyValidate(document)) fail("r12-malformed-prefix-control", mutation);
  const bytes = Buffer.from(JSON.stringify(document));
  writeFileSync(join(sandbox, "lekalo/change-history.json"), bytes);
  const sourceBefore = JSON.stringify(canonicalSnapshot(join(sandbox, "lekalo")));
  for (const result of [
    run(["change", "custody-init", "--project", ".", "--expect-history", provDigest(bytes), "--json"]),
    applyField("priority", "schema-invalid-prefix"),
  ]) {
    if (result.code !== 1 || !result.json?.reasonCodes?.includes("change.history-invalid")) fail("r12-malformed-prefix-admitted", { mutation, result });
    if (JSON.stringify(canonicalSnapshot(join(sandbox, "lekalo"))) !== sourceBefore ||
        existsSync(join(sandbox, ".lekalo/change-history-pin.json")) || existsSync(join(sandbox, ".lekalo/pending-transactions.json")) ||
        existsSync(join(sandbox, ".lekalo/cache/change"))) fail("r12-malformed-prefix-mutated", mutation);
  }
  rmSync(sandbox, { recursive: true, force: true });
}

// r11 F1: a valid near-4-MiB history followed by a REAL twenty-field
// append must refuse before any source/history/pin/pending publication.
// The synthetic boundary prefix is deliberately given trusted local
// custody; this is a capacity test, never a ledger-only forgery claim.
freshSandbox("history-capacity");
writeFileSync(join(sandbox, "lekalo/modules/tasks/commands.yaml"), commandBytes);
const historyPath = join(sandbox, "lekalo/change-history.json");
const pinPath = join(sandbox, ".lekalo/change-history-pin.json");
const maxBytes = 4 * 1024 * 1024;
const boundary = { ...structuredClone(noopHistory), records: [], nextSequence: 1 };
let estimatedBytes = Buffer.byteLength(JSON.stringify(canonical(boundary), null, 2));
for (let index = 1; index <= 4096; index += 1) {
  const record = structuredClone(noopHistory.records[0]);
  record.sequence = index;
  record.keyDigest = provDigest(`boundary-${index}`);
  record.receipt.keyDigest = record.keyDigest;
  record.receipt.history.sequence = index;
  record.receipt.transaction.id = `change-aabbccdd-${index}`;
  // Extra indentation for a record inside the root records array.
  const row = JSON.stringify(canonical(record), null, 2);
  const rowBytes = Buffer.byteLength(row) + row.split("\n").length * 4 + 2;
  if (estimatedBytes + rowBytes > maxBytes - 1024) break;
  estimatedBytes += rowBytes;
  boundary.records.push(record);
}
boundary.nextSequence = boundary.records.length + 1;
rechain(boundary);
const boundaryBytes = Buffer.from(JSON.stringify(canonical(boundary), null, 2));
if (boundaryBytes.length > maxBytes || maxBytes - boundaryBytes.length > 4096) fail("r11-cap-fixture-not-near-bound", boundaryBytes.length);
if (!historyValidate(boundary)) fail("r11-cap-fixture-schema", historyValidate.errors);
mkdirSync(join(sandbox, ".lekalo"), { recursive: true });
writeFileSync(historyPath, boundaryBytes);
writeFileSync(pinPath, JSON.stringify({ schemaVersion: "lekalo/change-history-pin/1", project: boundary.project,
  recordCount: boundary.records.length, tipDigest: boundary.tip.recordDigest, historyDigest: provDigest(boundaryBytes) }));
const firstBefore = run(["change", "receipt", "--project", ".", "--idempotency-key", "boundary-1", "--json"]);
if (firstBefore.code !== 0) fail("r11-cap-fixture-reader", firstBefore);
const sourcePath = join(sandbox, "lekalo/modules/tasks/entities.yaml");
const sourcesBefore = readFileSync(sourcePath);
const pinBefore = readFileSync(pinPath);
const capacityBatch = Array.from({ length: 20 }, (_, index) => ({ operation: "entity-field:add", entity: "tasks.task",
  field: { name: `capacity_${index}`, type: { ref: "tasks.text" } } }));
const capacityResult = applyBatch(capacityBatch, "capacity-crossing");
if (capacityResult.code !== 3 || !capacityResult.json?.reasonCodes?.includes("change.precondition-failed") ||
    capacityResult.json?.diagnostics?.[0]?.data?.detail !== "history-byte-limit") fail("r11-capacity-refusal", capacityResult);
if (!readFileSync(sourcePath).equals(sourcesBefore) || !readFileSync(historyPath).equals(boundaryBytes) ||
    !readFileSync(pinPath).equals(pinBefore)) fail("r11-capacity-mutated-originals");
if (existsSync(join(sandbox, ".lekalo/pending-transactions.json")) || existsSync(join(sandbox, ".lekalo/cache/change"))) fail("r11-capacity-started-transaction");
const firstAfter = run(["change", "receipt", "--project", ".", "--idempotency-key", "boundary-1", "--json"]);
if (firstAfter.code !== 0) fail("r11-capacity-stranded-history", firstAfter);
const boundaryReplay = applyBatch([noop], "boundary-1");
if (boundaryReplay.code !== 0 || boundaryReplay.json?.state !== "replayed") fail("r11-capacity-stranded-replay", boundaryReplay);
rmSync(sandbox, { recursive: true, force: true });

// r11 F3: ordinary canonical-only checkouts may explicitly start a local
// epoch, but not even a later valid append may authorize imported rows.
const epochDonor = freshSandbox("epoch-donor");
if (applyField("priority", "epoch-old").code !== 0 || applyField("escalation", "epoch-second").code !== 0) fail("r11-epoch-donor");
freshSandbox("epoch-checkout");
cpSync(join(epochDonor, "lekalo"), join(sandbox, "lekalo"), { recursive: true });
const importedBytes = readFileSync(join(sandbox, "lekalo/change-history.json"));
const importedSources = readFileSync(join(sandbox, "lekalo/modules/tasks/entities.yaml"));
const noInit = applyField("local_one", "epoch-local");
if (noInit.code !== 1) fail("r11-implicit-prefix-trust", noInit);
const staleInit = run(["change", "custody-init", "--project", ".", "--expect-history", provDigest("stale"), "--json"]);
if (staleInit.code !== 3 || existsSync(join(sandbox, ".lekalo/change-history-pin.json"))) fail("r11-epoch-stale-pin", staleInit);
const initialized = run(["change", "custody-init", "--project", ".", "--expect-history", provDigest(importedBytes), "--json"]);
if (initialized.code !== 0 || !receiptValidate(initialized.json)) fail("r11-epoch-init", { initialized, errors: receiptValidate.errors });
if (!readFileSync(join(sandbox, "lekalo/change-history.json")).equals(importedBytes) ||
    !readFileSync(join(sandbox, "lekalo/modules/tasks/entities.yaml")).equals(importedSources)) fail("r11-epoch-init-mutated-canonical");
for (const [name, key] of [["local_one", "epoch-local"], ["local_two", "epoch-next"]]) {
  const local = applyField(name, key);
  if (local.code !== 0 || local.json?.state !== "committed") fail("r11-epoch-local-commit", local);
  const replay = applyField(name, key);
  if (replay.code !== 0 || replay.json?.state !== "replayed") fail("r11-epoch-local-replay", replay);
  const old = applyField("priority", "epoch-old");
  if (old.code !== 1 || old.json?.diagnostics?.[0]?.data?.detail !== "commit-custody-epoch") fail("r11-epoch-prefix-laundered", old);
  const pin = JSON.parse(readFileSync(join(sandbox, ".lekalo/change-history-pin.json"), "utf8"));
  if (pin.firstLocalSequence !== 3) fail("r11-epoch-boundary-reset", pin);
}
const epochPin = readFileSync(join(sandbox, ".lekalo/change-history-pin.json"));
const reinit = run(["change", "custody-init", "--project", ".", "--expect-history", provDigest(readFileSync(join(sandbox, "lekalo/change-history.json"))), "--json"]);
if (reinit.code !== 3 || !readFileSync(join(sandbox, ".lekalo/change-history-pin.json")).equals(epochPin)) fail("r11-epoch-rebind", reinit);
const auditOld = run(["change", "receipt", "--project", ".", "--idempotency-key", "epoch-old", "--json"]);
if (auditOld.code !== 0 || auditOld.json?.state !== "committed") fail("r11-epoch-audit-lookup", auditOld);
rmSync(sandbox, { recursive: true, force: true });
rmSync(epochDonor, { recursive: true, force: true });

// Even an initialized, fully rechained endpoint forgery stays audit-only
// after an unrelated legitimate local commit (the old graft bypass).
freshSandbox("epoch-graft");
const graftSource = join(sandbox, "lekalo/modules/tasks/entities.yaml");
const graft = structuredClone(genuine);
graft.records[0].receipt.files[0].afterDigest = provDigest(readFileSync(graftSource));
graft.records[0].receipt.files[0].beforeDigest = provDigest("invented preimage");
rechain(graft);
const graftBytes = Buffer.from(JSON.stringify(graft));
writeFileSync(join(sandbox, "lekalo/change-history.json"), graftBytes);
const graftInit = run(["change", "custody-init", "--project", ".", "--expect-history", provDigest(graftBytes), "--json"]);
if (graftInit.code !== 0) fail("r11-graft-epoch-init", graftInit);
if (applyField("unrelated", "graft-local").code !== 0) fail("r11-graft-local-commit");
const graftBeforeReplay = readFileSync(graftSource);
const graftReplay = applyField("priority", "r10-custody");
if (graftReplay.code !== 1 || graftReplay.json?.diagnostics?.[0]?.data?.detail !== "commit-custody-epoch") fail("r11-graft-epoch-laundered", graftReplay);
if (!readFileSync(graftSource).equals(graftBeforeReplay)) fail("r11-graft-epoch-replay-mutated");
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
    "staging-interrupt-aborts-cleanly",
    "forged-creation-refused-barrier-kept",
    "forged-journal-provenance-refused",
    "forged-ledger-refused",
    "dup-free-receipts",
    "pending-receipt-gated",
    "rollback-recovery-schema-valid",
    "cache-clear-custody",
    "shared-writer-serialization",
    "adoption-journal-barriers",
    "recomputed-ledger-custody-refusals",
    "two-record-tail-truncation",
    "two-file-partial-adoption-recovery",
    "cache-clear-preserves-writer-lock-and-custody",
    "live-noop-and-mixed-batch-history-schema",
    "genuine-r10-noop-upgrade-refuses-with-originals-and-typed-diagnostic",
    "schema-invalid-later-operation-arrays-never-adopted-or-republished",
    "near-cap-history-real-append-refuses-with-originals",
    "explicit-checkout-epoch-new-work-without-prefix-replay",
    "initialized-graft-never-laundered-by-local-append",
    heldLockFlow,
  ],
}, null, 2)}\n`);
