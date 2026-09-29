#!/usr/bin/env node
// Issue #60 acceptance harness: the PHP Laravel routes generator proven
// end to end through the real one-shot adapter exchange and through the
// real Laravel fixture kernel — the same confined protocol path
// production uses.
//
// Stages:
//   1. Managed custody: the composed types + operations + routes run
//      plans the full inventory, applies it, and every emitted file
//      matches the plan digests byte for byte; verify is clean.
//   2. Byte stability: two clean roots with identical inputs emit
//      identical files, sidecars, and plan ids.
//   3. The generated surface: exactly one public action per thin
//      controller, no business constructs, the typed request bindings,
//      and the classmap; the emitted classes syntax-check and load
//      under `php -n`.
//   4. The explicit error-to-HTTP table: the emitted map carries the
//      declared statuses, categories, codes, and category defaults.
//   5. Checked custody: absent evidence vetoes with a typed finding;
//      conforming evidence passes without writes; stale evidence is a
//      typed finding.
//   6. Scope immunity: the plan never writes outside the generated
//      roots — the manual routes of the app are never touched.
//   7. Code + OpenAPI sync: a tampered emitted projection is a typed
//      drift finding.
//   8. Runtime: the planner fixture serves the Today and focus
//      endpoints through the generated routes and thin controllers over
//      the real HTTP kernel — happy path, missing task, conflict,
//      validation refusal, unauthenticated denial, policy denial, the
//      declared infrastructure failure, and the untouched manual route.
//
// Dependency-free (node:*, no npm packages); run from the repo root.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const adapterPath = join(repoRoot, "adapters", "php-laravel", "adapter.php");
const inputs = join(repoRoot, "tests", "fixtures", "php-laravel", "routes", "inputs");
const fixtureRoot = join(repoRoot, "tests", "fixtures", "php-laravel", "planner");
const php = process.env.LEKALO_PHP ?? "php";
const sha256 = (bytes) => "sha256:" + createHash("sha256").update(bytes).digest("hex");
const ZEROS = "sha256:" + "0".repeat(64);
const EVIDENCE_PATH = ".lekalo/cache/ir/planner.json";

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

/** The staged evidence of one disposable root, with the real digests. */
function newRoot(tag, { withEvidence = true } = {}) {
  const root = join(mkdtempSync(join(tmpdir(), "lekalo-routes-")), tag);
  for (const home of [".lekalo/cache/ir", ".lekalo/cache/transport", ".lekalo/cache/openapi", ".lekalo/import/observed", "lekalo/types", "lekalo/operations", "lekalo/routes"]) {
    mkdirSync(join(root, home), { recursive: true });
  }
  const irBytes = JSON.stringify(JSON.parse(readFileSync(join(inputs, "ir/planner.ir.json"), "utf8")));
  const irDigest = sha256(irBytes);
  writeFileSync(join(root, ".lekalo/cache/ir/planner.json"), irBytes);
  const transportBytes = readFileSync(join(inputs, "transport.json"), "utf8").replace(/\n$/, "");
  const transportDoc = JSON.parse(transportBytes);
  assert.equal(transportDoc.irRef.digest, irDigest, "the committed transport fixture pins the committed IR");
  const transportDigest = sha256(transportBytes);
  writeFileSync(join(root, ".lekalo/cache/transport/planner.json"), transportBytes);
  writeFileSync(join(root, "lekalo/transport.yaml"), transportBytes);
  const openapiBytes = readFileSync(join(inputs, "openapi/planner.openapi.json"), "utf8").replace(/\n$/, "");
  const openapiDoc = JSON.parse(openapiBytes);
  assert.equal(openapiDoc["x-lekalo-provenance"].irRef.digest, irDigest, "the committed OpenAPI golden pins the committed IR");
  assert.equal(openapiDoc["x-lekalo-provenance"].transportRef.digest, transportDigest, "the committed OpenAPI golden pins the committed transport");
  writeFileSync(join(root, ".lekalo/cache/openapi/planner.json"), openapiBytes);
  const typesBytes = readFileSync(join(inputs, "planner.types.json"), "utf8").replace(ZEROS, irDigest);
  const typesDigest = sha256(typesBytes);
  writeFileSync(join(root, "lekalo/types/planner.types.json"), typesBytes);
  const operationsBytes = readFileSync(join(inputs, "planner.operations.json"), "utf8")
    .replace(`"irDigest": "${ZEROS}"`, `"irDigest": "${irDigest}"`)
    .replace(`"typesInputDigest": "${ZEROS}"`, `"typesInputDigest": "${typesDigest}"`);
  const operationsDigest = sha256(operationsBytes);
  writeFileSync(join(root, "lekalo/operations/planner.operations.json"), operationsBytes);
  const routesBytes = readFileSync(join(inputs, "planner.routes.json"), "utf8")
    .replace(`"irDigest": "${ZEROS}"`, `"irDigest": "${irDigest}"`)
    .replace(`"transportDigest": "${ZEROS}"`, `"transportDigest": "${transportDigest}"`)
    .replace(`"typesInputDigest": "${ZEROS}"`, `"typesInputDigest": "${typesDigest}"`)
    .replace(`"operationsInputDigest": "${ZEROS}"`, `"operationsInputDigest": "${operationsDigest}"`);
  writeFileSync(join(root, "lekalo/routes/planner.routes.json"), routesBytes);
  if (withEvidence) {
    // The scanner evidence of the checked record: the fixture's own
    // maintained controller is the existing route.
    const controllerBytes = readFileSync(join(fixtureRoot, "app/Http/Controllers/TaskFocusController.php"));
    mkdirSync(join(root, "app/Http/Controllers"), { recursive: true });
    writeFileSync(join(root, "app/Http/Controllers/TaskFocusController.php"), controllerBytes);
    const evidence = {
      schemaVersion: "lekalo/php-routes-evidence/v0.4.0",
      identity: "dev.lekalo.php-routes-evidence@0.4.0",
      projectId: "planner",
      irDigest,
      routesInputDigest: sha256(routesBytes),
      producer: { tool: "artisan-route-list", version: "1.0.0" },
      sources: [{ path: "app/http/controllers/taskfocuscontroller.php", digest: sha256(controllerBytes) }],
      routes: [{
        method: "POST",
        uri: "/api/tasks/{task_id}/focus",
        name: "tasks.focus",
        action: "App\\Http\\Controllers\\TaskFocusController@focus",
        path: "app/Http/Controllers/TaskFocusController.php",
        digest: sha256(controllerBytes),
        middleware: ["fixture.auth"],
      }],
    };
    writeFileSync(join(root, ".lekalo/import/observed/routes-evidence.json"), JSON.stringify(evidence, null, 1) + "\n");
  }
  return { root, irDigest };
}

const routesRequest = (overrides = {}) => ({
  protocol: "lekalo.target/v1",
  protocol_version: "0.3.2",
  operation: "generate",
  request_id: "req-" + "a".repeat(64),
  project_root: ".",
  target: "php-laravel",
  ir_path: EVIDENCE_PATH,
  dry_run: true,
  ...overrides,
});

const ROUTES_ROOT = ".lekalo/generated/php-laravel/routes";

step("managed custody: the composed run plans types, operations, and routes in one authorized plan", () => {
  const { root } = newRoot("managed");
  const dry = adapterCall(root, routesRequest());
  assert.equal(dry.status, "ok", JSON.stringify(dry).slice(0, 1200));
  const writes = dry.writes ?? [];
  assert.ok(writes.length > 0, "the plan is not empty");
  for (const write of writes) {
    assert.equal(write.action, "create");
  }
  const routeWrites = writes.filter((write) => write.path.startsWith(ROUTES_ROOT + "/"));
  assert.ok(routeWrites.length > 0, "the routes family plans under the generated routes root");
  for (const path of [
    ROUTES_ROOT + "/routes.php",
    ROUTES_ROOT + "/routes.map.json",
    ROUTES_ROOT + "/classmap.php",
    ROUTES_ROOT + "/openapi.json",
    ROUTES_ROOT + "/http-envelope.php",
    ROUTES_ROOT + "/error-http-map.php",
    ROUTES_ROOT + "/planner/endpoint_focus_task_by_id_controller.php",
    ROUTES_ROOT + "/planner/endpoint_focus_task_by_id_request.php",
    ROUTES_ROOT + "/planner/endpoint_today_controller.php",
  ]) {
    assert.ok(writes.some((write) => write.path === path), `the plan covers ${path}`);
  }
  assert.ok(
    !writes.some((write) => write.path === "routes/api.php"),
    "the plan never touches the manual routes file",
  );
  // Apply and verify the exact bytes.
  const planId = dry.evidence?.plan_id;
  const applied = adapterCall(root, routesRequest({ dry_run: false, plan_id: planId }));
  assert.equal(applied.status, "ok");
  assert.deepEqual(applied.writes, writes, "the applied plan equals the dry run");
  for (const write of writes) {
    const bytes = readFileSync(join(root, write.path));
    assert.equal(sha256(bytes), write.sha256, write.path);
  }
  // The generated routes file registers the managed routes only.
  const registrations = readFileSync(join(root, ROUTES_ROOT, "routes.php"), "utf8");
  assert.match(registrations, /Route::post\('\/tasks\/\{task_id\}\/focus'/);
  assert.match(registrations, /Route::get\('\/today'/);
  assert.match(registrations, /->middleware\(\['fixture\.auth'\]\)/);
  assert.ok(!registrations.includes("/api/tasks"), "the manual route surface never enters the generated file");
  // Verify is clean over the fresh bytes.
  const verified = adapterCall(root, {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    operation: "verify",
    request_id: "req-" + "b".repeat(64),
    project_root: ".",
    target: "php-laravel",
    ir_path: EVIDENCE_PATH,
  });
  assert.equal(verified.status, "ok", JSON.stringify(verified));
  assert.deepEqual(verified.result.findings, [], "fresh managed routes verify clean");
  rmSync(root, { recursive: true, force: true });
});

step("byte stability: two clean roots emit identical plan ids and writes", () => {
  const one = newRoot("one");
  const two = newRoot("two");
  const first = adapterCall(one.root, routesRequest());
  const second = adapterCall(two.root, routesRequest());
  assert.equal(first.evidence?.plan_id, second.evidence?.plan_id, "the plan id is root-independent");
  assert.deepEqual(first.writes, second.writes);
  rmSync(one.root, { recursive: true, force: true });
  rmSync(two.root, { recursive: true, force: true });
});

step("the generated surface is thin, typed, and loads under php -n", () => {
  const { root } = newRoot("surface");
  const dry = adapterCall(root, routesRequest());
  adapterCall(root, routesRequest({ dry_run: false, plan_id: dry.evidence?.plan_id }));
  const controller = readFileSync(join(root, ROUTES_ROOT, "planner/endpoint_focus_task_by_id_controller.php"), "utf8");
  // Exactly one public action (the constructor does not count).
  const publicMethods = controller.match(/public function (?!__construct)/g) ?? [];
  assert.equal(publicMethods.length, 1, "exactly one public action per thin controller");
  // The wrapper only decodes, delegates, and encodes: no business
  // constructs anywhere in the generated routes tree.
  const controllerText = controller;
  assert.match(controllerText, /final readonly class EndpointFocusTaskByIdController/);
  assert.match(controllerText, /private \\Lekalo\\Generated\\Operations\\Planner\\FocusTaskHandler \$handler,/);
  assert.match(controllerText, /ErrorHttpMap::validation\('planner\.endpoint_focus_task_by_id', \$failure->fields\)/);
  assert.match(controllerText, /ErrorHttpMap::domain\('planner\.endpoint_focus_task_by_id', 'planner\.task_not_found'/);
  assert.match(controllerText, /HttpEnvelope::actor\(\$request, true\)/);
  for (const banned of ["::query(", "DB::", "Event::", "->save(", "new Query", "Eloquent"]) {
    assert.ok(!controllerText.includes(banned), `the thin controller never contains ${banned}`);
  }
  // The typed request binding decodes the declared surface.
  const request = readFileSync(join(root, ROUTES_ROOT, "planner/endpoint_focus_task_by_id_request.php"), "utf8");
  assert.match(request, /FocusTaskInputCodec::decode\(\$wire\)/);
  assert.match(request, /\$route->parameter\('task_id'\)/);
  assert.match(request, /RequestValidationError\(\['task_id'\]\)/);
  assert.match(request, /header\('Idempotency-Key'\) === null/);
  // Every emitted PHP class loads under `php -n` through the classmap.
  for (const family of ["routes", "operations", "types"]) {
    const classmapPath = join(root, ".lekalo/generated/php-laravel", family, "classmap.php");
    const loaded = spawnSync(php, [
      "-n",
      "-r",
      `$map = require ${JSON.stringify(classmapPath)};
$base = ${JSON.stringify(join(root, ".lekalo/generated/php-laravel", family))};
$loaded = [];
foreach ($map as $fqn => $relative) {
  $path = (string) $relative;
  if ($path !== '' && $path[0] !== '/' && !preg_match('/^[A-Za-z]:/', $path)) { $path = $base . '/' . $path; }
  require $path;
  $loaded[] = $fqn;
}
$classes = array_filter($loaded, fn ($fqn) => class_exists($fqn) || interface_exists($fqn) || enum_exists($fqn));
echo count($classes) . '/' . count($loaded);
if (count($classes) !== count($loaded)) { fwrite(STDERR, 'missing classes'); exit(1); }`,
    ], { encoding: "utf8", timeout: 60_000 });
    assert.equal(loaded.status, 0, loaded.stderr + String.fromCharCode(10) + loaded.stdout);
    assert.match(loaded.stdout, /^\d+\/\d+$/);
  }
  rmSync(root, { recursive: true, force: true });
});

step("the emitted error-to-HTTP table is the declared projection", () => {
  const { root } = newRoot("table");
  const dry = adapterCall(root, routesRequest());
  adapterCall(root, routesRequest({ dry_run: false, plan_id: dry.evidence?.plan_id }));
  const map = readFileSync(join(root, ROUTES_ROOT, "error-http-map.php"), "utf8");
  const declared = [
    ["'planner.task_not_found' => ['status' => 404, 'category' => 'not-found', 'code' => 'LEK-ERR-005']"],
    ["'planner.focus_conflict' => ['status' => 409, 'category' => 'conflict', 'code' => 'LEK-ERR-001']"],
    ["'planner.focus_denied' => ['status' => 403, 'category' => 'auth', 'code' => 'LEK-ERR-002']"],
    ["'planner.input_invalid' => ['status' => 400, 'category' => 'validation', 'code' => 'LEK-ERR-003']"],
    ["'planner.store_unavailable' => ['status' => 503, 'category' => 'infrastructure', 'code' => 'LEK-ERR-004']"],
  ];
  for (const [row] of declared) {
    assert.ok(map.includes(row), `the table carries ${row}`);
  }
  // The category defaults are the declared six.
  for (const member of ["'validation' => 400", "'auth' => 403", "'conflict' => 409", "'not-found' => 404", "'domain' => 422", "'infrastructure' => 500"]) {
    assert.ok(map.includes(member), `the defaults carry ${member}`);
  }
  // The today route declares exactly its own error.
  assert.ok(map.includes("'planner.endpoint_today'"), "the today route owns a table");
  // The emitted openapi.json is the staged projection, verbatim.
  const emitted = readFileSync(join(root, ROUTES_ROOT, "openapi.json"), "utf8").replace(/\n$/, "");
  const staged = readFileSync(join(root, ".lekalo/cache/openapi/planner.json"), "utf8").replace(/\n$/, "");
  assert.equal(emitted, staged, "the emitted projection is the staged evidence, byte for byte");
  rmSync(root, { recursive: true, force: true });
});

step("checked custody: absent evidence vetoes, conforming evidence passes, stale evidence is a finding", () => {
  // Absent evidence: the veto is the bounded in-envelope error with a
  // zero-write plan.
  const absent = newRoot("absent", { withEvidence: false });
  const absentResponse = adapterCall(absent.root, routesRequest());
  assert.equal(absentResponse.status, "error", JSON.stringify(absentResponse));
  assert.equal(absentResponse.error.code, "routes.binding-missing", JSON.stringify(absentResponse));
  assert.equal(absentResponse.writes, undefined, "a veto never carries a writes member");
  rmSync(absent.root, { recursive: true, force: true });

  // Conforming evidence: the plan publishes and the checked record
  // never emits.
  const { root } = newRoot("checked");
  const dry = adapterCall(root, routesRequest());
  assert.equal(dry.status, "ok", JSON.stringify(dry).slice(0, 1200));
  const checkedWrites = (dry.writes ?? []).filter((write) => write.path.includes("TaskFocus"));
  assert.deepEqual(checkedWrites, [], "a checked record never emits");
  adapterCall(root, routesRequest({ dry_run: false, plan_id: dry.evidence?.plan_id }));

  // Stale evidence: the live bytes diverge from the record.
  const controllerPath = join(root, "app/Http/Controllers/TaskFocusController.php");
  writeFileSync(controllerPath, readFileSync(controllerPath, "utf8") + "// drifted\n");
  const stale = adapterCall(root, routesRequest());
  assert.equal(stale.status, "error", JSON.stringify(stale));
  assert.equal(stale.error.code, "routes.binding-stale", JSON.stringify(stale));
  assert.equal(stale.writes, undefined);
  rmSync(root, { recursive: true, force: true });
});

step("scope immunity: the plan never writes outside the generated roots", () => {
  const { root } = newRoot("scope");
  const dry = adapterCall(root, routesRequest());
  for (const write of dry.writes ?? []) {
    assert.ok(
      write.path.startsWith(".lekalo/generated/php-laravel/"),
      `the plan stays inside the generated tree: ${write.path}`,
    );
    assert.ok(!write.path.startsWith("routes/"), write.path);
    assert.ok(!write.path.startsWith("app/"), write.path);
  }
  rmSync(root, { recursive: true, force: true });
});

step("code + OpenAPI sync: a tampered projection is a typed drift finding", () => {
  const { root } = newRoot("drift");
  const dry = adapterCall(root, routesRequest());
  adapterCall(root, routesRequest({ dry_run: false, plan_id: dry.evidence?.plan_id }));
  const projectionPath = join(root, ROUTES_ROOT, "openapi.json");
  writeFileSync(projectionPath, readFileSync(projectionPath, "utf8").replace('"planner"', '"tampered"'));
  const verified = adapterCall(root, {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    operation: "verify",
    request_id: "req-" + "c".repeat(64),
    project_root: ".",
    target: "php-laravel",
    ir_path: EVIDENCE_PATH,
  });
  assert.equal(verified.status, "ok", JSON.stringify(verified));
  const codes = (verified.result?.findings ?? []).map((finding) => finding.code);
  assert.ok(codes.includes("routes.drift"), JSON.stringify(verified.result));
  const drifted = (verified.result?.findings ?? []).filter((finding) => finding.path.endsWith("openapi.json"));
  assert.ok(drifted.length > 0, "the drifted projection is named");
  rmSync(root, { recursive: true, force: true });
});

/** Locate the provisioned vendor tree (a runtime-stage precondition). */
function ensureVendor() {
  if (!existsSync(join(fixtureRoot, "vendor", "autoload.php"))) {
    process.stderr.write(
      "provisioned vendor tree missing at " + join(fixtureRoot, "vendor", "autoload.php") +
      "; run the operator bootstrap (composer install --no-interaction --prefer-dist --no-scripts) " +
      "outside verification, then re-run this harness\n",
    );
    process.exit(1);
  }
}

step("runtime: the planner fixture serves the Today and focus endpoints through the generated surface", () => {
  ensureVendor();
  // Materialize the fixture (with its provisioned vendor tree) into a
  // disposable runtime root and generate into it.
  const materialRoot = join(mkdtempSync(join(tmpdir(), "lekalo-routes-runtime-")), "fixture");
  cpSync(fixtureRoot, materialRoot, { recursive: true });
  const staged = newRoot("runtime");
  for (const home of [".lekalo/cache/ir", ".lekalo/cache/transport", ".lekalo/cache/openapi", ".lekalo/import/observed", "lekalo/types", "lekalo/operations", "lekalo/routes", "app/Http/Controllers"]) {
    mkdirSync(join(materialRoot, home), { recursive: true });
  }
  for (const relative of [
    ".lekalo/cache/ir/planner.json",
    ".lekalo/cache/transport/planner.json",
    "lekalo/transport.yaml",
    ".lekalo/cache/openapi/planner.json",
    "lekalo/types/planner.types.json",
    "lekalo/operations/planner.operations.json",
    "lekalo/routes/planner.routes.json",
    ".lekalo/import/observed/routes-evidence.json",
    "app/Http/Controllers/TaskFocusController.php",
  ]) {
    cpSync(join(staged.root, relative), join(materialRoot, relative));
  }
  rmSync(staged.root, { recursive: true, force: true });
  const dry = adapterCall(materialRoot, routesRequest());
  assert.equal(dry.status, "ok", JSON.stringify(dry).slice(0, 1200));
  const applied = adapterCall(materialRoot, routesRequest({ dry_run: false, plan_id: dry.evidence?.plan_id }));
  assert.equal(applied.status, "ok");

  // Drive the real kernel through the generated surface and the
  // untouched manual route.
  const driverPath = join(repoRoot, "tests", "fixtures", "php-laravel", "routes", "runtime", "driver.php");
  const taskId = "3f2e5f3a-9f4e-4d1e-b35a-2f6d0a7c1b45";
  const specs = [
    // The today board over a fresh table.
    { method: "GET", uri: "/today" },
    // Unauthenticated focus: the declared auth denial.
    { method: "POST", uri: `/tasks/${taskId}/focus`, body: { task_id: taskId } },
    // Authenticated happy path.
    {
      method: "POST",
      uri: `/tasks/${taskId}/focus`,
      body: { task_id: taskId },
      headers: { "X-Fixture-User": "user-1", "Idempotency-Key": "idem-1" },
    },
    // The today board reflects the focus.
    { method: "GET", uri: "/today" },
    // The conflict: an already-focused task refuses with the declared
    // conflict envelope.
    {
      method: "POST",
      uri: `/tasks/${taskId}/focus`,
      body: { task_id: taskId },
      headers: { "X-Fixture-User": "user-1", "Idempotency-Key": "idem-2" },
    },
    // Missing task: the declared not-found envelope.
    {
      method: "POST",
      uri: "/tasks/9f2e5f3a-9f4e-4d1e-b35a-2f6d0a7c1b99/focus",
      body: { task_id: "9f2e5f3a-9f4e-4d1e-b35a-2f6d0a7c1b99" },
      headers: { "X-Fixture-User": "user-1", "Idempotency-Key": "idem-3" },
    },
    // Validation: a malformed body refuses with the declared
    // validation envelope.
    {
      method: "POST",
      uri: `/tasks/${taskId}/focus`,
      body: {},
      headers: { "X-Fixture-User": "user-1", "Idempotency-Key": "idem-4" },
    },
    // Policy: a bulk actor is denied with the declared auth error.
    {
      method: "POST",
      uri: `/tasks/8f2e5f3a-9f4e-4d1e-b35a-2f6d0a7c1b88/focus`,
      body: { task_id: "8f2e5f3a-9f4e-4d1e-b35a-2f6d0a7c1b88" },
      headers: { "X-Fixture-User": "user-2", "X-Fixture-Mode": "bulk", "Idempotency-Key": "idem-5" },
    },
    // The declared infrastructure failure maps through the table.
    { method: "GET", uri: "/today", headers: { "X-Fixture-Fail": "store" } },
    // The manual route outside the managed scope still works, untouched.
    {
      method: "POST",
      uri: `/api/tasks/${taskId}/focus`,
      body: { focused_at: "2026-01-01T00:00:00Z" },
    },
  ];
  const run = spawnSync(php, [driverPath, JSON.stringify(specs)], {
    cwd: materialRoot,
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
    timeout: 120_000,
  });
  assert.equal(run.status, 0, run.stderr.slice(0, 3000));
  const outcomes = JSON.parse(run.stdout);
  assert.equal(outcomes.length, specs.length);

  const byUri = (uri, method, n = 0) => outcomes.filter((o) => o.spec.uri === uri && o.spec.method === method)[n];
  const errorOf = (outcome) => outcome.body?.error ?? {};

  // Today over a fresh table: the declared success projection.
  const freshToday = outcomes[0];
  assert.equal(freshToday.status, 200, JSON.stringify(freshToday));
  assert.equal(freshToday.body, "backlog");

  // Unauthenticated: the declared auth denial, never an anonymous pass.
  const unauthorized = outcomes[1];
  assert.equal(unauthorized.status, 403, JSON.stringify(unauthorized));
  assert.equal(errorOf(unauthorized).id, "planner.focus_denied");
  assert.equal(errorOf(unauthorized).code, "LEK-ERR-002");
  assert.equal(errorOf(unauthorized).category, "auth");

  // The happy path: the declared 202 with an empty body.
  const focus = outcomes[2];
  assert.equal(focus.status, 202, JSON.stringify(focus));

  // The today board reflects the focus through the maintained port.
  const focusedToday = outcomes[3];
  assert.equal(focusedToday.status, 200, JSON.stringify(focusedToday));
  assert.equal(focusedToday.body, "focused");

  // The conflict: the declared conflict envelope.
  const conflict = outcomes[4];
  assert.equal(conflict.status, 409, JSON.stringify(conflict));
  assert.equal(errorOf(conflict).id, "planner.focus_conflict");
  assert.equal(errorOf(conflict).code, "LEK-ERR-001");
  assert.equal(errorOf(conflict).category, "conflict");
  assert.equal(errorOf(conflict).payload.task_id, taskId);

  // The missing task: the declared not-found envelope.
  const missing = outcomes[5];
  assert.equal(missing.status, 404, JSON.stringify(missing));
  assert.equal(errorOf(missing).id, "planner.task_not_found");
  assert.equal(errorOf(missing).code, "LEK-ERR-005");
  assert.equal(errorOf(missing).category, "not-found");
  assert.equal(errorOf(missing).payload.task_id, "9f2e5f3a-9f4e-4d1e-b35a-2f6d0a7c1b99");

  // The malformed body: the declared validation envelope.
  const invalid = outcomes[6];
  assert.equal(invalid.status, 400, JSON.stringify(invalid));
  assert.equal(errorOf(invalid).id, "planner.input_invalid");
  assert.equal(errorOf(invalid).code, "LEK-ERR-003");
  assert.equal(errorOf(invalid).category, "validation");
  assert.equal(errorOf(invalid).payload.field, "task_id");

  // The bulk actor: the policy denial through the typed port.
  const denied = outcomes[7];
  assert.equal(denied.status, 403, JSON.stringify(denied));
  assert.equal(errorOf(denied).id, "planner.focus_denied");

  // The declared infrastructure failure: 503 with its envelope.
  const unavailable = outcomes[8];
  assert.equal(unavailable.status, 503, JSON.stringify(unavailable));
  assert.equal(errorOf(unavailable).id, "planner.store_unavailable");
  assert.equal(errorOf(unavailable).code, "LEK-ERR-004");
  assert.equal(errorOf(unavailable).category, "infrastructure");

  // The manual route: untouched, still serving its own contract.
  const manual = outcomes[9];
  assert.equal(manual.status, 200, JSON.stringify(manual));
  assert.equal(manual.body?.ok, true);
  assert.equal(manual.body?.value?.focused, true);

  // The emitted projection still names the manual route's declared
  // contract — the checked record joined it.
  const projection = JSON.parse(readFileSync(join(materialRoot, ROUTES_ROOT, "openapi.json"), "utf8"));
  assert.ok(projection.paths["/api/tasks/{task_id}/focus"], "the checked route rides the projection");
  assert.ok(projection.paths["/today"], "the today route rides the projection");

  rmSync(materialRoot, { recursive: true, force: true });
});

process.stdout.write(`${JSON.stringify({ ok: true, passed })}\n`);
