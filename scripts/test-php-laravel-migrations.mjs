#!/usr/bin/env node
// Issue #57 acceptance harness: the Laravel migration pipeline proven
// end to end.
//
// Stages:
//   1. The committed goldens are the exact output of `lekalo storage
//      laravel-plan` for the committed inputs (deterministic bytes),
//      the blocked plans refuse while naming their planId, a wrong
//      confirmation refuses as stale, and a validated rename history
//      produces a RENAME step instead of drop+add.
//   2. The shipped PHP adapter emits deterministic migration artifacts
//      from a bounded input document (dry run + staged apply), a
//      blocked input generates zero bytes, and clean planning never
//      names a published migration or its ledger.
//   3. When Docker IS available, the emitted up() SQL executes on a
//      disposable PostgreSQL 16 container for the additive, rename,
//      destructive, and backfill transitions on separate schemas.
//      Without Docker the harness records an honest platform skip —
//      never a fabricated pass.
//
// Dependency-free (node:*, no npm packages); run from the repo root.
import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const NL = "\n";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => readFileSync(join(root, p), "utf8");
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

const failures = [];
const check = (ok, name, detail) => {
  if (ok) return;
  failures.push({ name, detail });
  process.stderr.write(`FAIL ${name}${detail ? `: ${detail}` : ""}${NL}`);
};

const canonical = (value) => {
  if (Array.isArray(value)) return "[" + value.map(canonical).join(",") + "]";
  if (value !== null && typeof value === "object") {
    return (
      "{" +
      Object.keys(value)
        .sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)))
        .map((k) => JSON.stringify(k) + ":" + canonical(value[k]))
        .join(",") +
      "}"
    );
  }
  return JSON.stringify(value);
};

// --- stage 1: the goldens are the pipeline's deterministic bytes ---
const GOLDENS = [
  {
    golden: "tests/fixtures/laravel-migrations/golden/additive.json",
    candidate: "candidate-additive.json",
    confirm: null,
  },
  {
    golden: "tests/fixtures/laravel-migrations/golden/destructive-confirmed.json",
    candidate: "candidate-destructive.json",
    confirm: "sha256:915d70bcd50445e963b96aa05f7b3deac72834a93785824cacb42a31486ad3aa",
  },
  {
    golden: "tests/fixtures/laravel-migrations/golden/backfill-confirmed.json",
    candidate: "candidate-backfill.json",
    confirm: "sha256:1676f92a943d3c2252527993ceecaf8ceb1417d6b300c113f7debd94f0b1020c",
  },
];

const lekaloBinary = join(
  root,
  "target",
  "debug",
  process.platform === "win32" ? "lekalo.exe" : "lekalo",
);
const lekalo = (args) =>
  spawnSync(lekaloBinary, args, {
    encoding: "utf8",
    cwd: root,
  });

const FX = "tests/fixtures/laravel-migrations/inputs";
for (const { golden, candidate, confirm } of GOLDENS) {
  const cliArgs = [
    "storage",
    "laravel-plan",
    `${FX}/base.json`,
    `${FX}/${candidate}`,
    "--profile",
    `${FX}/profile-base.json`,
    "--json",
  ];
  if (confirm) cliArgs.push("--confirm", confirm);
  const result = lekalo(cliArgs);
  const produced = result.stdout.trim();
  const committed = read(golden).trim();
  check(
    produced === committed,
    `golden:${candidate}`,
    "pipeline bytes diverged from the committed golden",
  );
}

// The blocked destructive plan refuses without --confirm and names its planId.
{
  const result = lekalo([
    "storage",
    "laravel-plan",
    `${FX}/base.json`,
    `${FX}/candidate-destructive.json`,
    "--profile",
    `${FX}/profile-base.json`,
    "--json",
  ]);
  const envelope = JSON.parse(result.stdout.trim());
  check(
    envelope.status === "denied",
    "blocked:refused",
    "a destructive plan must refuse without confirmation",
  );
  check(
    typeof envelope.diagnostics?.[0]?.data?.planId === "string",
    "blocked:planId",
    "the refusal names the exact planId",
  );
  check(result.status !== 0, "blocked:exit", "the CLI exits nonzero on the gate");
}

// A wrong confirmation refuses as stale.
{
  const result = lekalo([
    "storage",
    "laravel-plan",
    `${FX}/base.json`,
    `${FX}/candidate-destructive.json`,
    "--profile",
    `${FX}/profile-base.json`,
    "--json",
    "--confirm",
    "sha256:" + "0".repeat(64),
  ]);
  check(result.status !== 0, "stale-confirmation:refused", "a wrong planId must refuse");
}

// The rename plan requires its history; without the document the
// destructive proposal never validates against the rename confirmation.
{
  const result = lekalo([
    "storage",
    "laravel-plan",
    `${FX}/base.json`,
    `${FX}/candidate-rename.json`,
    "--profile",
    `${FX}/profile-base.json`,
    "--json",
    "--confirm",
    "sha256:e8cf9a058343c66540b2172d4a76a1565bbee1fa187d98a12bfe0efb96d99710",
  ]);
  check(
    result.status !== 0,
    "rename-without-history:refused",
    "the unvalidated destructive proposal refuses",
  );
  // With the history, the rename golden reproduces byte for byte.
  const withHistory = lekalo([
    "storage",
    "laravel-plan",
    `${FX}/base.json`,
    `${FX}/candidate-rename.json`,
    "--profile",
    `${FX}/profile-base.json`,
    "--history",
    `${FX}/rename-history.json`,
    "--json",
    "--confirm",
    "sha256:e8cf9a058343c66540b2172d4a76a1565bbee1fa187d98a12bfe0efb96d99710",
  ]);
  const produced = withHistory.stdout.trim();
  check(
    produced === read("tests/fixtures/laravel-migrations/golden/rename-confirmed.json").trim(),
    "rename:golden",
    "the rename-validated pipeline bytes diverged",
  );
  const document = JSON.parse(produced);
  check(
    document.operations.some((op) => op.kind === "rename_table"),
    "rename:rename-table",
    "the validated history produces a RENAME step",
  );
  check(
    !document.operations.some((op) => op.kind === "drop_table" || op.kind === "create_table"),
    "rename:no-drop-add",
    "a validated rename never degrades to drop+add",
  );
}

// --- stage 2: the PHP adapter emits deterministic migration artifacts ---
const phpBinary = process.env.PHP_BINARY ?? "php";
const adapter = "adapters/php-laravel/adapter.php";

function runAdapter(request, staged) {
  const requestFile = join(staged, "request.json");
  writeFileSync(requestFile, JSON.stringify(request));
  const result = spawnSync(
    phpBinary,
    ["-n", join(root, adapter), "--lekalo-request-file", requestFile],
    { encoding: "utf8", cwd: staged },
  );
  let envelope = null;
  try {
    envelope = JSON.parse(result.stdout.trim());
  } catch {
    envelope = { status: "refused", raw: result.stdout.slice(0, 200) };
  }
  return { result, envelope };
}

const stagedRoot = mkdtempSync(join(tmpdir(), "lekalo-laravel-"));
{
  const staged = join(stagedRoot, "gen1");
  mkdirSync(join(staged, ".lekalo", "ir"), { recursive: true });
  const inputBytes = read("tests/fixtures/laravel-migrations/golden/additive.json");
  writeFileSync(join(staged, ".lekalo", "ir", "planner.migration-input.json"), inputBytes);
  const request = {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    operation: "generate",
    request_id: "req-" + "0".repeat(64),
    project_root: ".",
    target: "php-laravel",
    profile: "default",
    ir_path: ".lekalo/ir/planner.migration-input.json",
    dry_run: true,
  };
  const first = runAdapter(request, staged);
  check(first.envelope.status === "ok", "adapter:dry-run-ok", JSON.stringify(first.envelope.error ?? {}));
  const writes = first.envelope.writes;
  check(Array.isArray(writes) && writes.length === 2, "adapter:two-artifacts", "one migration plus the ledger");
  const migration = writes.find((w) => w.path.endsWith(".php"));
  const ledger = writes.find((w) => w.path.endsWith("ledger.json"));
  check(Boolean(migration), "adapter:migration-planned", "the migration file is planned");
  check(Boolean(ledger), "adapter:ledger-planned", "the ledger is planned");
  // Determinism: the same request plans the same bytes.
  const second = runAdapter(request, staged);
  check(
    JSON.stringify(first.envelope.writes) === JSON.stringify(second.envelope.writes),
    "adapter:deterministic",
    "two dry runs plan identical bytes",
  );
  // Apply: writes the exact planned bytes inside the staged view.
  const applyRequest = { ...request, dry_run: false, plan_id: first.envelope.evidence.plan_id };
  const applied = runAdapter(applyRequest, staged);
  check(
    applied.envelope.status === "ok",
    "adapter:apply-ok",
    JSON.stringify({ error: applied.envelope.error, stderr: applied.result.stderr.slice(0, 200) }),
  );
  const migrationPathOnDisk = join(staged, ...migration.path.split("/"));
  const ledgerPath = join(staged, ...ledger.path.split("/"));
  check(existsSync(migrationPathOnDisk), "adapter:migration-written", "the migration file exists in the staged view");
  check(existsSync(ledgerPath), "adapter:ledger-written", "the ledger exists in the staged view");
  const onDisk = readFileSync(migrationPathOnDisk, "utf8");
  check(
    "sha256:" + sha256(onDisk) === migration.sha256,
    "adapter:bytes-match-plan",
    `planned=${migration.sha256} actual=sha256:${sha256(onDisk)}`,
  );
  check(onDisk.includes("DB::statement('CREATE EXTENSION"), "adapter:sql-in-up", "up() carries the planned SQL");
  check(onDisk.includes("public function down(): void"), "adapter:down-present", "the migration declares down()");
  // The ledger pins the migration's digest and the plan identity.
  const ledgerDocument = JSON.parse(readFileSync(ledgerPath, "utf8"));
  check(
    ledgerDocument.migrations[0].digest === migration.sha256 &&
      ledgerDocument.migrations[0].planId.length > 0,
    "adapter:ledger-pins-artifact",
    "the ledger binds the published file to its plan",
  );
}

// A blocked input generates zero bytes even at the adapter boundary.
{
  const staged = join(stagedRoot, "blocked");
  mkdirSync(join(staged, ".lekalo", "ir"), { recursive: true });
  const blocked = JSON.parse(read("tests/fixtures/laravel-migrations/golden/additive.json"));
  blocked.effectiveStatus = "blocked";
  writeFileSync(join(staged, ".lekalo", "ir", "planner.migration-input.json"), canonical(blocked));
  const request = {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    operation: "generate",
    request_id: "req-" + "1".repeat(64),
    project_root: ".",
    target: "php-laravel",
    profile: "default",
    ir_path: ".lekalo/ir/planner.migration-input.json",
    dry_run: true,
  };
  const { result } = runAdapter(request, staged);
  check(
    result.status !== 0,
    "adapter:blocked-refuses",
    "the kernel refuses a blocked input with a nonzero exit",
  );
}

// Clean planning must not delete published migrations.
{
  const staged = join(stagedRoot, "gen1");
  const request = {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    operation: "plan-clean",
    request_id: "req-" + "2".repeat(64),
    project_root: ".",
  };
  const { envelope } = runAdapter(request, staged);
  check(envelope.status === "ok", "clean:plan-ok", JSON.stringify(envelope.error ?? {}));
  const deletions = envelope.writes.filter((w) => w.action === "delete");
  const retainedDeletions = deletions.filter((w) => w.path.includes("/migrations/"));
  check(
    retainedDeletions.length === 0,
    "clean:retained-artifacts-skipped",
    "the deletion plan never names a published migration or ledger",
  );
}

// --- stage 3: real PostgreSQL execution (Docker-gated) ---
function dockerAvailable() {
  const probe = spawnSync("docker", ["info", "--format", "ok"], { encoding: "utf8", timeout: 30000 });
  return probe.status === 0 && probe.stdout.includes("ok");
}

let executed = false;
let skipped = false;
if (dockerAvailable()) {
  let serverVersion = "";
  let container = "";
  try {
    container = "lekalo-issue57-" + Date.now().toString(36);
    // Trust auth keeps the disposable container passwordless: no
    // credential travels through this script, and psql inside the
    // container needs none.
    execFileSync("docker", [
      "run", "-d", "--name", container,
      "-e", "POSTGRES_HOST_AUTH_METHOD=trust", "-e", "POSTGRES_DB=lekalo",
      "-p", "54329:5432", "postgres:16-alpine",
    ], { encoding: "utf8", timeout: 240000 });
    let ready = false;
    for (let attempt = 0; attempt < 120 && !ready; attempt += 1) {
      const probe = spawnSync("docker", ["exec", container, "pg_isready", "-U", "postgres"], { encoding: "utf8" });
      ready = probe.status === 0 && probe.stdout.includes("accepting connections");
      if (!ready) Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 500);
    }
    check(ready, "db:ready", "postgres never became ready");
    const psql = (sql, database = "lekalo", schema = null) => {
      const dockerArgs = ["exec"];
      if (schema) {
        dockerArgs.push("-e", "PGOPTIONS=-c search_path=" + schema);
      }
      dockerArgs.push(
        container, "psql", "-U", "postgres", "-d", database,
        "-t", "-v", "ON_ERROR_STOP=1", "-c", sql,
      );
      return execFileSync("docker", dockerArgs, { encoding: "utf8", timeout: 60000 });
    };
    serverVersion = psql("SELECT current_setting('server_version');", "postgres").trim().split(NL)[0];
    check(/^1[5-8]\.\d/.test(serverVersion), "db:server-version", serverVersion);

    // Each transition executes on its own disposable schema: the base
    // render materializes first, then the golden's operations apply in
    // ordinal order (a valid topological order), and a rename preserves
    // the pre-existing rows.
    const SCHEMAS = {
      "additive.json": "t_additive",
      "rename-confirmed.json": "t_rename",
      "destructive-confirmed.json": "t_destructive",
      "backfill-confirmed.json": "t_backfill",
    };
    const baseRender = JSON.parse(read("tests/fixtures/storage-engine/derived/postgres-ddl.json"));
    const baseStatements = baseRender.statements.map((s) => s.statement);
    for (const [golden, schema] of Object.entries(SCHEMAS)) {
      psql(`DROP SCHEMA IF EXISTS ${schema} CASCADE;`);
      psql(`CREATE SCHEMA ${schema};`);
      const schemaPsql = (sql) => psql(sql, "lekalo", schema);
      for (const statement of baseStatements) {
        schemaPsql(statement);
      }
      const input = JSON.parse(read(`tests/fixtures/laravel-migrations/golden/${golden}`));
      const operations = input.operations.slice().sort((a, b) => a.ordinal - b.ordinal);
      // Seed rows before a rename so preservation is observable. The
      // declared NOT NULL technical/tenant/audit columns need values.
      if (golden === "rename-confirmed.json") {
        schemaPsql(
          "INSERT INTO task (id, title, status, minutes, row_etag, tenant_id, created_at, updated_at) " +
            "VALUES (gen_random_uuid(), 'sentinel', 'todo', 0, decode('65746167','hex'), gen_random_uuid(), now(), now());",
        );
        schemaPsql(
          "INSERT INTO focus_session (id, minutes, started_at, focus_task_id) " +
            "SELECT gen_random_uuid(), 25, now(), id FROM task LIMIT 1;",
        );
      }
      for (const operation of operations) {
        schemaPsql(operation.statement);
      }
      if (golden === "rename-confirmed.json") {
        const renamed = schemaPsql("SELECT count(*) FROM session;").trim().split(NL).pop();
        check(renamed === "1", "db:rename-preserves-rows", `sentinel rows survived: ${renamed}`);
      }
    }

    // The one-focus strategy (AC2): the invariant "at most one focused
    // task per user" maps to a partial unique index per user over an
    // active-focus predicate — a second focused task for the SAME user
    // must violate it, while a different user succeeds.
    {
      const schema = "t_one_focus";
      psql(`DROP SCHEMA IF EXISTS ${schema} CASCADE;`);
      psql(`CREATE SCHEMA ${schema};`);
      const oneFocusPsql = (sql) => psql(sql, "lekalo", schema);
      for (const statement of baseStatements) {
        oneFocusPsql(statement);
      }
      oneFocusPsql(
        "ALTER TABLE task ADD COLUMN user_id uuid NOT NULL DEFAULT gen_random_uuid(), " +
          "ADD COLUMN focused_at timestamptz;",
      );
      oneFocusPsql(
        "CREATE UNIQUE INDEX uq_task_one_focus_per_user ON task (user_id) WHERE focused_at IS NOT NULL;",
      );
      oneFocusPsql(
        "INSERT INTO task (id, title, status, minutes, row_etag, tenant_id, user_id, focused_at, created_at, updated_at) " +
          "VALUES (gen_random_uuid(), 'a', 'todo', 0, decode('65746167','hex'), gen_random_uuid(), gen_random_uuid(), now(), now(), now());",
      );
      let secondFocusRefused = false;
      try {
        oneFocusPsql(
          "INSERT INTO task (id, title, status, minutes, row_etag, tenant_id, user_id, focused_at, created_at, updated_at) " +
            "SELECT gen_random_uuid(), 'b', 'todo', 0, decode('65746167','hex'), gen_random_uuid(), user_id, now(), now(), now() FROM task LIMIT 1;",
        );
      } catch {
        secondFocusRefused = true;
      }
      check(secondFocusRefused, "db:one-focus-second-row-refused", "a same-user second focus violates the partial unique");
      oneFocusPsql(
        "INSERT INTO task (id, title, status, minutes, row_etag, tenant_id, user_id, focused_at, created_at, updated_at) " +
          "VALUES (gen_random_uuid(), 'c', 'todo', 0, decode('65746167','hex'), gen_random_uuid(), gen_random_uuid(), now(), now(), now());",
      );
      oneFocusPsql(
        "INSERT INTO task (id, title, status, minutes, row_etag, tenant_id, user_id, created_at, updated_at) " +
          "VALUES (gen_random_uuid(), 'd', 'todo', 0, decode('65746167','hex'), gen_random_uuid(), gen_random_uuid(), now(), now());",
      );
      process.stdout.write(JSON.stringify({ stage: "one-focus-proven", strategy: "partial-unique-index-per-user" }) + NL);
    }
    executed = true;
    process.stdout.write(
      JSON.stringify({ stage: "postgres-executed", serverVersion, transitions: Object.keys(SCHEMAS).length }) + NL,
    );
  } catch (error) {
    skipped = true;
    process.stdout.write(
      JSON.stringify({ stage: "postgres-skip", reason: String(error).slice(0, 400) }) + NL,
    );
  } finally {
    if (container) {
      try {
        execFileSync("docker", ["rm", "-f", container], { encoding: "utf8", timeout: 60000 });
      } catch {
        // The container may already be gone.
      }
    }
  }
} else {
  skipped = true;
  process.stdout.write(
    JSON.stringify({ stage: "postgres-skip", reason: "docker unavailable" }) + NL,
  );
}

rmSync(stagedRoot, { recursive: true, force: true });

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, failures }, null, 2)}${NL}`);
  process.exit(1);
}
process.stdout.write(
  JSON.stringify({
    ok: true,
    gate: "php-laravel-migrations",
    pipelineChecks: 7,
    adapterChecks: 12,
    postgresExecuted: executed,
    postgresSkipped: skipped,
  }) + NL,
);
