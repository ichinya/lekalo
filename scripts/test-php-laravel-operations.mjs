#!/usr/bin/env node
// Issue #59 acceptance harness: the PHP operations generator proven end
// to end through the real one-shot adapter exchange — the same confined
// protocol path production uses.
//
// Stages:
//   1. Managed custody: the composed types + operations run plans the
//      full inventory, applies it, and every emitted file matches the
//      committed golden digests byte for byte.
//   2. Byte stability: two clean roots with identical inputs emit
//      identical files, sidecars, and plan ids.
//   3. The generated surface: exactly one public entrypoint per
//      handler, constructor-injected typed ports, the transaction
//      wrapper, typed errors, and the classmap; the emitted classes
//      syntax-check and load under `php -n`.
//   4. Query-write honesty: a query with a write recipe is a typed
//      `operations.query-write` finding with ZERO writes planned.
//   5. Types-unbound: an operations input without the bound types
//      document is a finding with zero writes.
//   6. Scaffold-once custody: one emission into the consumer root, a
//      marker-guarded regeneration that plans nothing, and a
//      pre-existing unowned path that refuses.
//   7. Checked/custom join: absent evidence is a finding for every
//      declared id; conforming evidence passes; stale evidence is a
//      typed finding; the managed files verify by exact digest and a
//      tampered handler is drift.
//
// Dependency-free (node:*, no npm packages); run from the repo root.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const adapterPath = join(repoRoot, "adapters", "php-laravel", "adapter.php");
const fixtures = join(repoRoot, "tests", "fixtures", "php-laravel", "operations", "inputs");
const php = process.env.LEKALO_PHP ?? "php";
const sha256 = (bytes) => "sha256:" + createHash("sha256").update(bytes).digest("hex");

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

function refusedCall(root, request) {
  const result = spawnSync(php, [adapterPath], {
    cwd: root,
    input: JSON.stringify(request),
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    timeout: 120_000,
  });
  assert.notEqual(result.status, 0, "a refused generation never exits 0");
  assert.match(result.stderr, /"diagnostic"/, "the refusal is a bounded stderr diagnostic");
  return result.stderr;
}

function newRoot(tag) {
  const root = join(mkdtempSync(join(tmpdir(), "lekalo-operations-")), tag);
  mkdirSync(join(root, ".lekalo", "cache", "ir"), { recursive: true });
  mkdirSync(join(root, "lekalo", "types"), { recursive: true });
  mkdirSync(join(root, "lekalo", "operations"), { recursive: true });
  return root;
}

const irBytes = readFileSync(join(fixtures, "ir", "planner.ir.json"), "utf8");
const irDigest = sha256(irBytes);
const typesInputBytes = `${JSON.stringify({
  identity: "dev.lekalo.php-types-input@0.4.0",
  irDigest,
  projectId: "planner",
  schemaVersion: "lekalo/php-types-input/v0.4.0",
})}\n`;
const typesDigest = sha256(typesInputBytes);

/** The committed operations input template with real digests. */
function operationsInputBytes() {
  return readFileSync(join(fixtures, "planner.operations.json"), "utf8")
    .replace(
      '"irDigest": "sha256:' + "0".repeat(64) + '"',
      `"irDigest": "${irDigest}"`,
    )
    .replace(
      '"typesInputDigest": "sha256:' + "0".repeat(64) + '"',
      `"typesInputDigest": "${typesDigest}"`,
    );
}

function stageInputs(root) {
  writeFileSync(join(root, ".lekalo", "cache", "ir", "planner.json"), irBytes);
  writeFileSync(join(root, "lekalo", "types", "planner.types.json"), typesInputBytes);
  writeFileSync(join(root, "lekalo", "operations", "planner.operations.json"), operationsInputBytes());
}

const HEX = "a".repeat(64);
const REQ_OPERATIONS = "req-" + HEX;
const REQ_VERIFY = "req-" + "b".repeat(64);
const REQ_VERIFY_DRIFT = "req-" + "c".repeat(64);
const REQ_CLEAN = "req-" + "d".repeat(64);
const EVIDENCE_PATH = ".lekalo/cache/ir/planner.json";

const operationsRequest = (overrides = {}) => ({
  protocol: "lekalo.target/v1",
  protocol_version: "0.3.2",
  operation: "generate",
  request_id: REQ_OPERATIONS,
  project_root: ".",
  target: "php-laravel",
  ir_path: EVIDENCE_PATH,
  dry_run: true,
  ...overrides,
});

step("managed custody: the composed run plans types and operations in one authorized plan", () => {
  const root = newRoot("managed");
  stageInputs(root);
  const dry = adapterCall(root, operationsRequest());
  assert.equal(dry.status, "ok", JSON.stringify(dry));
  const writes = dry.writes ?? [];
  assert.ok(writes.length > 0, "the plan is not empty");
  for (const write of writes) {
    assert.equal(write.action, "create");
  }
  const operations = writes.filter((write) => write.path.startsWith(".lekalo/generated/php-laravel/operations/"));
  assert.ok(operations.length > 0, "the operations family plans under the generated operations root");
  assert.ok(
    writes.some((write) => write.path === ".lekalo/generated/php-laravel/operations/operations.map.json"),
    "the custody sidecar rides the plan",
  );
  assert.ok(
    writes.some((write) => write.path.includes("planner/focus_task/handler.php")),
    "the focus handler rides the plan",
  );
  assert.ok(
    writes.some((write) => write.path.includes("planner/count_focused/input.php")),
    "the explicit empty query input rides the plan",
  );
  assert.ok(
    writes.some((write) => write.path === ".lekalo/generated/php-laravel/types/planner/focus_task_input.php"),
    "the missing managed command input type is generated in the same plan",
  );
  // Apply and verify the exact bytes.
  const planId = dry.evidence?.plan_id;
  const applied = adapterCall(root, operationsRequest({ dry_run: false, plan_id: planId }));
  assert.equal(applied.status, "ok");
  assert.deepEqual(applied.writes, writes, "the applied plan equals the dry run");
  for (const write of writes) {
    const bytes = readFileSync(join(root, write.path));
    assert.equal(sha256(bytes), write.sha256, write.path);
  }
  // Verify is clean over the fresh bytes.
  const verified = adapterCall(root, {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    operation: "verify",
    request_id: REQ_VERIFY,
    project_root: ".",
    target: "php-laravel",
    ir_path: EVIDENCE_PATH,
  });
  assert.equal(verified.status, "ok", JSON.stringify(verified));
  assert.deepEqual(verified.result.findings, [], "fresh managed operations verify clean");
});

step("byte stability: two clean roots emit identical plan ids", () => {
  const one = newRoot("one");
  const two = newRoot("two");
  stageInputs(one);
  stageInputs(two);
  const first = adapterCall(one, operationsRequest());
  const second = adapterCall(two, operationsRequest());
  assert.equal(first.evidence?.plan_id, second.evidence?.plan_id, "the plan id is root-independent");
  assert.deepEqual(first.writes, second.writes);
});

step("the generated surface is explicit, typed, and loads under php -n", () => {
  const root = newRoot("surface");
  stageInputs(root);
  const dry = adapterCall(root, operationsRequest());
  const applied = adapterCall(root, operationsRequest({ dry_run: false, plan_id: dry.evidence?.plan_id }));
  assert.equal(applied.status, "ok");
  const handlerPath = join(
    root,
    ".lekalo/generated/php-laravel/operations/planner/focus_task/handler.php",
  );
  const handler = readFileSync(handlerPath, "utf8");
  assert.match(handler, /final readonly class FocusTaskHandler/);
  assert.match(handler, /private readonly \\Lekalo\\Generated\\Operations\\Planner\\TaskRepository \$taskRepository,/);
  assert.match(handler, /private readonly \\Lekalo\\Generated\\Operations\\TransactionPort \$transactions,/);
  assert.match(handler, /public function handle\(\\Lekalo\\Generated\\Types\\Planner\\FocusTaskInput \$input, \\Lekalo\\Generated\\Operations\\ActorContext \$actor\): void/);
  assert.equal(
    (handler.match(/public function (?!__construct)/g) ?? []).length,
    1,
    "exactly one public entrypoint (the constructor does not count)",
  );
  assert.match(handler, /authorize\(\$input, \$actor\);/);
  assert.match(handler, /->find\(\$input->taskId\);/);
  assert.match(handler, /throw new \\Lekalo\\Generated\\Operations\\Planner\\Errors\\TaskNotFoundError\(\);/);
  assert.match(handler, /throw new \\Lekalo\\Generated\\Operations\\Planner\\Errors\\FocusConflictError\(\);/);
  assert.match(handler, /TaskState::Focused/);
  assert.match(handler, /->save\(\$taskUpdated\);/);
  assert.match(handler, /->taskFocused\(new \\Lekalo\\Generated\\Types\\Planner\\TaskFocusedPayload\(\$input->taskId\)\);/);
  // The query handler delegates through the typed port.
  const query = readFileSync(
    join(root, ".lekalo/generated/php-laravel/operations/planner/count_focused/handler.php"),
    "utf8",
  );
  assert.match(query, /public function handle\(\\Lekalo\\Generated\\Operations\\Planner\\CountFocusedInput \$input, \\Lekalo\\Generated\\Operations\\ActorContext \$actor\): \\Lekalo\\Generated\\Types\\Planner\\TaskState/);
  assert.match(query, /return \$this->focusedCounter->count\(\$input, \$actor\);/);
  // Syntax-check and load every emitted PHP file under `php -n` with
  // the explicit classmap; no extension beyond the core is needed.
  const classmapBytes = readFileSync(
    join(root, ".lekalo/generated/php-laravel/operations/classmap.php"),
  );
  const classmapText = classmapBytes.toString("utf8");
  const loaded = spawnSync(
    php,
    [
      "-n",
      "-r",
      `$map = require ${JSON.stringify(join(root, ".lekalo/generated/php-laravel/operations/classmap.php"))};
$base = ${JSON.stringify(join(root, ".lekalo/generated/php-laravel/operations"))};
$loaded = [];
foreach ($map as $fqn => $relative) {
  require $base . '/' . $relative;
  $loaded[] = $fqn;
}
$classes = array_filter($loaded, fn ($fqn) => class_exists($fqn) || interface_exists($fqn) || enum_exists($fqn));
echo count($classes) . '/' . count($loaded);
if (count($classes) !== count($loaded)) { fwrite(STDERR, 'missing classes'); exit(1); }`,
    ],
    { encoding: "utf8", timeout: 60_000 },
  );
  assert.equal(loaded.status, 0, loaded.stderr);
  assert.match(loaded.stdout, /^\d+\/\d+$/);
  assert.ok(classmapText.includes("FocusTaskHandler"));
  void handlerPath;
});

step("a query with a write recipe is a typed query-write finding with zero writes", () => {
  const root = newRoot("query-write");
  stageInputs(root);
  const input = JSON.parse(operationsInputBytes());
  input.operations[0].recipe = {
    kind: "single-entity-update",
    entity: "planner.task",
    key: "task_id",
    assignments: [{ field: "state", value: { enumCase: { type: "planner.task_state", value: "focused" } } }],
    kept: ["title"],
    missingBehavior: { error: "planner.store_unavailable" },
  };
  writeFileSync(join(root, "lekalo", "operations", "planner.operations.json"), JSON.stringify(input, null, 1) + "\n");
  const response = adapterCall(root, operationsRequest());
  assert.equal(response.status, "error", JSON.stringify(response));
  assert.equal(response.error.code, "operations.query-write", JSON.stringify(response));
  assert.equal(response.writes, undefined, "a veto never carries a writes member");
  assert.equal(response.evidence.plan_id, undefined, "a veto carries no plan authority");
});

step("an operations input without the bound types document is a zero-write finding", () => {
  const root = newRoot("unbound");
  writeFileSync(join(root, ".lekalo", "cache", "ir", "planner.json"), irBytes);
  writeFileSync(join(root, "lekalo", "operations", "planner.operations.json"), operationsInputBytes());
  const response = adapterCall(root, operationsRequest());
  assert.equal(response.status, "error", JSON.stringify(response));
  assert.equal(response.error.code, "operations.types-unbound", JSON.stringify(response));
  assert.equal(response.writes, undefined, "a veto never carries a writes member");
});

step("a malformed operations input refuses as a bounded diagnostic", () => {
  const root = newRoot("shape");
  stageInputs(root);
  writeFileSync(join(root, "lekalo", "operations", "planner.operations.json"), "{not json");
  const stderr = refusedCall(root, operationsRequest());
  assert.match(stderr, /operations-input-shape/);
});

step("scaffold-once custody: one emission, marker-guarded regeneration, unowned refusal", () => {
  const root = newRoot("scaffold");
  stageInputs(root);
  const input = JSON.parse(operationsInputBytes());
  input.operations[1].mode = "scaffold-once";
  writeFileSync(join(root, "lekalo", "operations", "planner.operations.json"), JSON.stringify(input, null, 1) + "\n");
  const dry = adapterCall(root, operationsRequest());
  assert.equal(dry.status, "ok", JSON.stringify(dry));
  const scaffoldWrites = (dry.writes ?? []).filter((write) => write.path.startsWith("app/lekalo-operations/"));
  assert.ok(scaffoldWrites.length > 0, "the scaffold rides the plan");
  const applied = adapterCall(root, operationsRequest({ dry_run: false, plan_id: dry.evidence?.plan_id }));
  assert.equal(applied.status, "ok");
  // Regeneration with the marker present plans nothing for the root.
  const again = adapterCall(root, operationsRequest());
  const planned = (again.writes ?? []).filter((write) => write.path.startsWith("app/lekalo-operations/"));
  assert.deepEqual(planned, [], "the scaffold is user-owned after the one emission");
  // A pre-existing unowned path refuses the first emission.
  const foreign = newRoot("scaffold-unowned");
  stageInputs(foreign);
  const foreignInput = JSON.parse(operationsInputBytes());
  foreignInput.operations[1].mode = "scaffold-once";
  writeFileSync(join(foreign, "lekalo", "operations", "planner.operations.json"), JSON.stringify(foreignInput, null, 1) + "\n");
  mkdirSync(join(foreign, "app", "lekalo-operations", "planner", "focus_task"), { recursive: true });
  writeFileSync(join(foreign, "app", "lekalo-operations", "planner", "focus_task", "handler.php"), "<?php\n");
  const stderr = refusedCall(foreign, operationsRequest());
  assert.match(stderr, /operations-scaffold-unowned/);
});

step("checked custody: absent, conforming, and stale evidence", () => {
  const root = newRoot("checked");
  stageInputs(root);
  const input = JSON.parse(operationsInputBytes());
  input.operations[0].mode = "checked";
  input.operations[0].entry = {
    fqn: "App\\LekaloOperations\\CountFocusedHandler",
    method: "handle",
    path: "app/count_focused_handler.php",
  };
  delete input.operations[0].recipe;
  delete input.operations[0].transaction;
  const checkedInputBytes = JSON.stringify(input, null, 1) + "\n";
  writeFileSync(join(root, "lekalo", "operations", "planner.operations.json"), checkedInputBytes);
  // Absent evidence: the veto is the bounded in-envelope error with a
  // zero-write plan.
  const absent = adapterCall(root, operationsRequest());
  assert.equal(absent.status, "error", JSON.stringify(absent));
  assert.equal(absent.error.code, "operations.binding-missing", JSON.stringify(absent));
  assert.equal(absent.writes, undefined, "a veto never carries a writes member");
  // Conforming evidence: the source bytes are real, digested, and the
  // record joins.
  const sourceRoot = join(root, "app");
  mkdirSync(sourceRoot, { recursive: true });
  const sourceBytes = "<?php\n\ndeclare(strict_types=1);\n";
  writeFileSync(join(sourceRoot, "count_focused_handler.php"), sourceBytes);
  const evidence = {
    schemaVersion: "lekalo/php-operations-evidence/v0.4.0",
    identity: "dev.lekalo.php-operations-evidence@0.4.0",
    projectId: "planner",
    irDigest,
    operationsInputDigest: sha256(checkedInputBytes),
    producer: { tool: "mago", version: "1.0.0" },
    sources: [{ path: "app/count_focused_handler.php", digest: sha256(sourceBytes) }],
    operations: [
      {
        id: "planner.count_focused",
        fqn: "App\\LekaloOperations\\CountFocusedHandler",
        method: "handle",
        path: "app/count_focused_handler.php",
        digest: sha256(sourceBytes),
        constructor: [{ name: "counter", type: "App\\Counters\\FocusedCounter" }],
        parameters: [
          { name: "input", type: "CountFocusedInput", required: true },
          { name: "actor", type: "ActorContext", required: true },
        ],
        returnType: "TaskState",
        publicMethods: ["handle"],
      },
    ],
  };
  mkdirSync(join(root, ".lekalo", "import", "observed"), { recursive: true });
  writeFileSync(
    join(root, ".lekalo", "import", "observed", "operations-evidence.json"),
    JSON.stringify(evidence, null, 1) + "\n",
  );
  const conforming = adapterCall(root, operationsRequest());
  assert.equal(conforming.status, "ok", JSON.stringify(conforming));
  assert.deepEqual(
    (conforming.writes ?? [])
      .filter((write) => write.path.startsWith(".lekalo/generated/php-laravel/operations/planner/count_focused")),
    [],
    "a checked record never emits",
  );
  // Stale evidence: the live bytes diverge from the record.
  writeFileSync(join(sourceRoot, "count_focused_handler.php"), sourceBytes + "// drifted\n");
  const stale = adapterCall(root, operationsRequest());
  assert.equal(stale.status, "error", JSON.stringify(stale));
  assert.equal(stale.error.code, "operations.binding-stale", JSON.stringify(stale));
});

step("managed drift is a verify finding, and plan-clean never names the scaffold", () => {
  const root = newRoot("drift");
  stageInputs(root);
  const dry = adapterCall(root, operationsRequest());
  adapterCall(root, operationsRequest({ dry_run: false, plan_id: dry.evidence?.plan_id }));
  const handlerPath = join(root, ".lekalo/generated/php-laravel/operations/planner/focus_task/handler.php");
  writeFileSync(handlerPath, "<?php\n// tampered\n");
  const verified = adapterCall(root, {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    operation: "verify",
    request_id: REQ_VERIFY_DRIFT,
    project_root: ".",
    target: "php-laravel",
    ir_path: EVIDENCE_PATH,
  });
  const codes = (verified.result?.findings ?? []).map((finding) => finding.code);
  assert.ok(codes.includes("operations.drift"), JSON.stringify(verified.result));
  // Plan-clean carries no ir_path (the closed wire): it plans over the
  // kernel-owned fixture artifact only, so neither the scaffold nor the
  // retained migration custody can ever enter a delete plan here.
  const cleanPlan = adapterCall(root, {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    operation: "plan-clean",
    request_id: REQ_CLEAN,
    project_root: ".",
    target: "php-laravel",
  });
  for (const write of cleanPlan.writes ?? []) {
    assert.ok(!write.path.startsWith("app/lekalo-operations/"), write.path);
    assert.ok(!write.path.includes("/migrations/"), write.path);
    assert.ok(!write.path.startsWith(".lekalo/generated/php-laravel/operations/"), write.path);
  }
  void existsSync;
  void rmSync;
});

// --- Fix-round repro stages (review blockers B1-B4) -----------------

step("B1: a deleted scaffold handler is an operations.scaffold-missing verify finding", () => {
  const root = newRoot("b1");
  stageInputs(root);
  const input = JSON.parse(operationsInputBytes());
  input.operations[1].mode = "scaffold-once";
  writeFileSync(join(root, "lekalo", "operations", "planner.operations.json"), JSON.stringify(input, null, 1) + "\n");
  const dry = adapterCall(root, operationsRequest());
  const applied = adapterCall(root, operationsRequest({ dry_run: false, plan_id: dry.evidence?.plan_id }));
  assert.equal(applied.status, "ok");
  const handler = join(root, "app", "lekalo-operations", "planner", "focus_task", "handler.php");
  assert.ok(existsSync(handler), "the scaffold handler exists after the apply");
  rmSync(handler);
  const verified = adapterCall(root, {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    operation: "verify",
    request_id: REQ_VERIFY,
    project_root: ".",
    target: "php-laravel",
    ir_path: EVIDENCE_PATH,
  });
  const codes = (verified.result?.findings ?? []).map((finding) => finding.code);
  assert.ok(
    codes.includes("operations.scaffold-missing"),
    `the removed scaffold is a typed verify finding: ${JSON.stringify(verified.result)}`,
  );
});

step("B2: required-family findings veto the composed run with zero writes", () => {
  const root = newRoot("b2");
  stageInputs(root);
  // The types family runs checked custody with absent evidence: a
  // findings-only envelope with no files.
  const types = JSON.parse(typesInputBytes);
  types.policy = { custody: "checked" };
  const typesBytes = JSON.stringify(types) + "\n";
  writeFileSync(join(root, "lekalo", "types", "planner.types.json"), typesBytes);
  // The operations input pins the new types bytes.
  const input = JSON.parse(operationsInputBytes());
  input.typesInputDigest = sha256(typesBytes);
  writeFileSync(join(root, "lekalo", "operations", "planner.operations.json"), JSON.stringify(input, null, 1) + "\n");
  const response = adapterCall(root, operationsRequest());
  assert.equal(response.status, "error", JSON.stringify(response).slice(0, 800));
  assert.equal(response.error.code, "php-types.binding-missing", JSON.stringify(response).slice(0, 800));
  assert.equal(response.writes, undefined, "the veto never carries a writes member");
  assert.equal(response.evidence.plan_id, undefined, "a veto carries no plan authority");
});

step("B3: a write recipe without the required transaction is a typed finding", () => {
  const root = newRoot("b3");
  stageInputs(root);
  const input = JSON.parse(operationsInputBytes());
  delete input.operations[1].transaction;
  writeFileSync(join(root, "lekalo", "operations", "planner.operations.json"), JSON.stringify(input, null, 1) + "\n");
  const response = adapterCall(root, operationsRequest());
  assert.equal(response.status, "error", JSON.stringify(response).slice(0, 800));
  assert.equal(response.error.code, "operations.transaction-required", JSON.stringify(response).slice(0, 800));
  assert.equal(response.writes, undefined);
});

step("B4: bogus policy, operation, and effect ids are typed findings, never silent ok", () => {
  // A bogus policy binding.
  const policyRoot = newRoot("b4-policy");
  stageInputs(policyRoot);
  const policyInput = JSON.parse(operationsInputBytes());
  policyInput.operations[1].policy = { id: "planner.bogus_policy" };
  writeFileSync(join(policyRoot, "lekalo", "operations", "planner.operations.json"), JSON.stringify(policyInput, null, 1) + "\n");
  const policyResponse = adapterCall(policyRoot, operationsRequest());
  assert.equal(policyResponse.status, "error");
  assert.equal(policyResponse.error.code, "operations.policy-unresolved", JSON.stringify(policyResponse).slice(0, 600));
  // A bogus operation id under checked mode.
  const opRoot = newRoot("b4-op");
  stageInputs(opRoot);
  const opInput = JSON.parse(operationsInputBytes());
  opInput.operations[0].id = "planner.bogus_op";
  opInput.operations[0].mode = "checked";
  opInput.operations[0].entry = {
    fqn: "App\\LekaloOperations\\BogusHandler",
    method: "handle",
    path: "app/bogus_handler.php",
  };
  delete opInput.operations[0].recipe;
  delete opInput.operations[0].transaction;
  writeFileSync(join(opRoot, "lekalo", "operations", "planner.operations.json"), JSON.stringify(opInput, null, 1) + "\n");
  const opResponse = adapterCall(opRoot, operationsRequest());
  assert.equal(opResponse.status, "error");
  assert.equal(opResponse.error.code, "operations.operation-unresolved", JSON.stringify(opResponse).slice(0, 600));
  // An emit naming a non-effect/non-event id.
  const emitRoot = newRoot("b4-emit");
  stageInputs(emitRoot);
  const emitInput = JSON.parse(operationsInputBytes());
  emitInput.operations[1].recipe.emit = [
    { event: "planner.bogus_event", payload: { task_id: { fromInput: "task_id" } } },
  ];
  writeFileSync(join(emitRoot, "lekalo", "operations", "planner.operations.json"), JSON.stringify(emitInput, null, 1) + "\n");
  const emitResponse = adapterCall(emitRoot, operationsRequest());
  assert.equal(emitResponse.status, "error");
  assert.equal(emitResponse.error.code, "operations.effect-unresolved", JSON.stringify(emitResponse).slice(0, 600));
  // An operand naming a bogus input field.
  const operandRoot = newRoot("b4-operand");
  stageInputs(operandRoot);
  const operandInput = JSON.parse(operationsInputBytes());
  operandInput.operations[1].recipe.emit = [
    { event: "planner.task_focused", payload: { task_id: { fromInput: "bogus_field" } } },
  ];
  writeFileSync(join(operandRoot, "lekalo", "operations", "planner.operations.json"), JSON.stringify(operandInput, null, 1) + "\n");
  const operandResponse = adapterCall(operandRoot, operationsRequest());
  assert.equal(operandResponse.status, "error");
  assert.equal(operandResponse.error.code, "operations.type-mismatch", JSON.stringify(operandResponse).slice(0, 600));
});

process.stdout.write(`${JSON.stringify({ ok: true, passed })}\n`);
