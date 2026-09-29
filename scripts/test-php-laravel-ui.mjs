#!/usr/bin/env node
// Issue #53 planning-screen gate: the UI/API projection of the
// planning screen stays checked, not hand-written.
//
// Stages:
//   1. Checked join: the committed TypeScript/Go client of the planner
//      HTTP API is byte-identical to a fresh render of the committed
//      client-SDK evidence through the real node-typescript adapter
//      kernel (dry-run plan, apply, byte equality).
//   2. Client ↔ OpenAPI sync: every client operation, method, path,
//      success status, and declared error code is the same projection
//      the committed #46 OpenAPI document publishes.
//   3. UI projection traceability: every DTO field, bucket, action
//      gate, optimistic rollback row, state trigger, and requirement
//      link in ui-projection.json resolves to a declared API/semantic
//      symbol (IR, transport, client contract, requirements
//      attachment).
//   4. Screen typecheck: the maintained Vue screen compiles
//      (@vue/compiler-sfc) and typechecks strict (pinned TypeScript)
//      against the generated client.
//   5. Browser-E2E bindings: the scenario-IR documents of the screen
//      are canonical, schema-valid, pin the committed IR/Model
//      digests, resolve their scenario ids as Model scenario symbols,
//      and agree with the ui-projection e2e block.
//
// Dependency-free (node:*) apart from the NODE_PATH-provisioned
// pinned TypeScript, @vue/compiler-sfc, and Ajv — exactly how CI
// provisions them.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { createRequire } from "node:module";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const inputs = join(repoRoot, "tests", "fixtures", "php-laravel", "routes", "inputs");
const modelHome = join(repoRoot, "tests", "fixtures", "php-laravel", "routes", "model");
const uiRoot = join(repoRoot, "tests", "fixtures", "php-laravel", "routes", "ui");
const require_ = createRequire(import.meta.url);

let passed = 0;
async function step(name, fn) {
  try {
    await fn();
    passed += 1;
    console.log(`ok - ${name}`);
  } catch (error) {
    console.error(`not ok - ${name}`);
    console.error(error instanceof Error ? error.stack : String(error));
    process.exit(1);
  }
}

function sortedCompact(value) {
  if (Array.isArray(value)) return value.map(sortedCompact);
  if (value !== null && typeof value === "object") {
    const out = {};
    for (const key of Object.keys(value).sort()) out[key] = sortedCompact(value[key]);
    return out;
  }
  return value;
}

const sha256 = (bytes) => "sha256:" + createHash("sha256").update(bytes).digest("hex");

const clientContract = JSON.parse(readFileSync(join(inputs, "client-sdk", "planner.json"), "utf8"));
const transport = JSON.parse(readFileSync(join(inputs, "transport.json"), "utf8"));
const openapi = JSON.parse(readFileSync(join(inputs, "openapi", "planner.openapi.json"), "utf8"));
const ir = JSON.parse(readFileSync(join(inputs, "ir", "planner.ir.json"), "utf8"));
const projection = JSON.parse(readFileSync(join(uiRoot, "ui-projection.json"), "utf8"));
const attachment = JSON.parse(readFileSync(join(modelHome, "requirements.attachment.json"), "utf8"));

const irDefinitions = new Map(ir.definitions.map((definition) => [definition.id, definition]));
const irDigest = sha256(JSON.stringify(JSON.parse(readFileSync(join(inputs, "ir", "planner.ir.json"), "utf8"))));
const modelDigest = transport.modelRef.digest;
const clientMethodOf = (endpoint) => {
  // The effective operation id is the client's method-name binding;
  // the evidence carries it, so the gate never re-derives spelling.
  const operation = clientContract.operations.find((candidate) => candidate.endpoint === endpoint);
  return operation ? operation.operationId : "(unbound)";
};

// ---------------------------------------------------------------------------
// 1. Checked join: regenerate through the adapter kernel, compare bytes.
// ---------------------------------------------------------------------------
async function checkedJoin() {
  const root = mkdtempSync(join(tmpdir(), "lekalo-ui-gate-"));
  try {
    for (const home of [".lekalo/cache/client-sdk", "src"]) {
      mkdirSync(join(root, home), { recursive: true });
    }
    copyFileSync(
      join(inputs, "client-sdk", "planner.json"),
      join(root, ".lekalo/cache/client-sdk/planner.json"),
    );
    const { createKernel, validateResolvedProjectProfile } = await import(
      pathToFileURL(join(repoRoot, "adapters/node-typescript/src/kernel.mjs")).href
    );
    const { descriptor: generationComposite } = await import(
      pathToFileURL(join(repoRoot, "adapters/node-typescript/src/generation-composite.mjs")).href
    );
    const profile = validateResolvedProjectProfile({
      id: "standalone",
      mode: "observed",
      target: "node-typescript",
      readRoots: [
        { kind: "tree", path: ".lekalo/cache/client-sdk" },
        { kind: "tree", path: "src" },
      ],
      exclusions: [],
      provenance: { origin: "declared", revision: "test", disposition: "public-fixture" },
    });
    const kernel = createKernel({
      resolvedProjectProfile: profile,
      extensionRegistry: [generationComposite],
    });
    const base = {
      protocol: "lekalo.target/v1",
      protocol_version: "0.3.2",
      operation: "generate",
      project_root: ".",
      target: "node-typescript",
      profile: "standalone",
      ir_path: ".lekalo/cache/client-sdk/planner.json",
    };
    const dry = kernel.dispatch(
      { ...base, request_id: "req-ui-gate-dry", dry_run: true },
      { permittedProjectRoot: root },
    ).response;
    assert.equal(dry.status, "ok", JSON.stringify(dry).slice(0, 600));
    const applied = kernel.dispatch(
      { ...base, request_id: "req-ui-gate-apply", dry_run: false, plan_id: dry.evidence.plan_id },
      { permittedProjectRoot: root },
    ).response;
    assert.equal(applied.status, "ok", JSON.stringify(applied).slice(0, 600));
    const emittedRoot = join(root, "src/generated/node-typescript/clients");
    // The TypeScript module, the compatibility sidecar, and the
    // ownership map compare byte for byte. The Go derivation compares
    // after the deterministic gofmt normalization — the committed
    // fixture is the gofmt-clean form of the same render (the #72
    // vue-consumer convention); gofmt is deterministic, so a wire
    // change still moves the normalized bytes and drift stays a
    // finding, never a silent pass.
    for (const name of ["planner.client.ts", "planner.compatibility.json", "planner.map.json"]) {
      const fresh = readFileSync(join(emittedRoot, name), "utf8");
      const committed = readFileSync(join(uiRoot, "generated", name), "utf8");
      assert.equal(fresh, committed, `the committed generated client is the fresh render: ${name}`);
    }
    const freshGo = readFileSync(join(emittedRoot, "planner.client.go"));
    writeFileSync(join(root, "fresh-normalized.go"), freshGo);
    const formatted = spawnSync("gofmt", ["-w", join(root, "fresh-normalized.go")], { encoding: "utf8" });
    if (formatted.error || formatted.status !== 0) {
      throw new Error(`gofmt unavailable or failed: ${String(formatted.stderr ?? formatted.error?.message ?? "").slice(0, 200)}`);
    }
    const freshGoNormalized = readFileSync(join(root, "fresh-normalized.go"), "utf8");
    const committedGo = readFileSync(join(uiRoot, "generated", "planner.client.go"), "utf8");
    assert.equal(freshGoNormalized, committedGo, "the committed Go client is the gofmt-normalized fresh render");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

// ---------------------------------------------------------------------------
// 2. Client ↔ OpenAPI sync.
// ---------------------------------------------------------------------------
function clientOpenapiSync() {
  const byId = new Map(clientContract.operations.map((operation) => [operation.operationId, operation]));
  const seen = new Set();
  for (const binding of transport.endpoints) {
    const operation = byId.get(clientMethodOf(binding.endpoint));
    assert.ok(operation, `the client carries ${binding.endpoint} as ${clientMethodOf(binding.endpoint)}`);
    seen.add(operation.operationId);
    // Method and path are single-sourced in the Model endpoint symbol;
    // the canonical transport never restates them.
    const endpointSymbol = irDefinitions.get(binding.endpoint);
    assert.ok(endpointSymbol && endpointSymbol.kind === "endpoint", `the IR declares ${binding.endpoint}`);
    const item = openapi.paths[endpointSymbol.path];
    assert.ok(item, `openapi publishes ${endpointSymbol.path}`);
    const spec = item[endpointSymbol.method.toLowerCase()];
    assert.ok(spec, `openapi publishes ${endpointSymbol.method} ${endpointSymbol.path}`);
    assert.equal(operation.method, endpointSymbol.method, `the client keeps the method of ${binding.endpoint}`);
    assert.equal(operation.path, endpointSymbol.path, `the client keeps the path of ${binding.endpoint}`);
    assert.equal(spec.summary, binding.summary, `the summary of ${binding.endpoint} is one projection`);
    const responses = spec.responses ?? {};
    assert.ok(responses[String(binding.success.status)], `openapi declares the success status of ${binding.endpoint}`);
    for (const declared of binding.errors ?? []) {
      const errorRow = operation.errors.find((row) => row.error === declared.error);
      assert.ok(errorRow, `the client union carries ${declared.error} for ${binding.endpoint}`);
      assert.equal(errorRow.status, declared.status, `${declared.error} keeps its status on ${binding.endpoint}`);
      assert.ok(responses[String(declared.status)], `openapi declares ${declared.status} for ${binding.endpoint}`);
    }
  }
  assert.equal(seen.size, clientContract.operations.length, "every client operation maps one transport endpoint");
}

// ---------------------------------------------------------------------------
// 3. UI projection traceability.
// ---------------------------------------------------------------------------
function projectionTraceability() {
  const operations = new Map(clientContract.operations.map((operation) => [operation.operationId, operation]));
  const types = new Map(clientContract.types.map((type) => [type.typeId, type]));
  const transportByEndpoint = new Map(transport.endpoints.map((binding) => [binding.endpoint, binding]));
  const attachmentRequirements = new Set(attachment.references.map((reference) => reference.requirement));

  for (const requirement of projection.requirements) {
    assert.ok(attachmentRequirements.has(requirement), `attachment binds ${requirement}`);
  }

  for (const [dtoName, dto] of Object.entries(projection.dto)) {
    const type = types.get(dto.of);
    assert.ok(type, `${dtoName} projects a declared type`);
    for (const field of dto.fields) {
      if (!field.source.startsWith("planner.")) continue;
      const wire = type.fields.find((candidate) => candidate.name === field.name);
      assert.ok(wire, `${dtoName}.${field.name} exists on ${dto.of}`);
      assert.equal(field.typeRef, wire.typeRef, `${dtoName}.${field.name} keeps its declared type`);
      // The source is one semantic symbol plus one declared member.
      const lastDot = field.source.lastIndexOf(".");
      const symbol = irDefinitions.get(field.source.slice(0, lastDot));
      assert.ok(symbol, `${field.source} resolves to a semantic symbol`);
      const member = field.source.slice(lastDot + 1);
      if (symbol.kind === "entity") {
        assert.ok(
          (symbol.fields ?? []).some((candidate) => candidate.name === member),
          `${field.source} names a declared entity field`,
        );
      }
    }
  }

  for (const bucket of projection.buckets) {
    const query = irDefinitions.get(bucket.query);
    assert.ok(query && query.kind === "query", `bucket ${bucket.id} names a query`);
    const endpoint = irDefinitions.get(bucket.endpoint);
    assert.ok(endpoint && endpoint.kind === "endpoint", `bucket ${bucket.id} names an endpoint`);
    assert.equal(endpoint.invokes, bucket.query, `bucket ${bucket.id} endpoint invokes its query`);
    assert.ok(transportByEndpoint.has(bucket.endpoint), `bucket ${bucket.id} rides the transport`);
    assert.ok(operations.has(bucket.clientMethod), `bucket ${bucket.id} names the client method`);
    assert.ok(types.has(bucket.itemType), `bucket ${bucket.id} item type is declared`);
  }

  for (const action of projection.actions) {
    const endpointBinding = transportByEndpoint.get(action.endpoint);
    assert.ok(endpointBinding, `action ${action.id} names a transport endpoint`);
    const command = irDefinitions.get(action.command);
    assert.ok(command && command.kind === "command", `action ${action.id} names a command`);
    assert.equal(
      irDefinitions.get(action.endpoint)?.invokes,
      action.command,
      `action ${action.id} endpoint invokes its command`,
    );
    const operation = operations.get(action.clientMethod);
    assert.ok(operation, `action ${action.id} names the client method`);
    assert.equal(operation.endpoint, action.endpoint, `action ${action.id} client method targets its endpoint`);
    if (action.policy) {
      const policy = irDefinitions.get(action.policy);
      assert.ok(policy && policy.kind === "policy", `action ${action.id} names a policy`);
      assert.ok(
        (policy.applies_to ?? []).includes(action.command),
        `the policy of ${action.id} applies to its command`,
      );
    }
    if (action.idempotency?.required) {
      assert.deepEqual(
        endpointBinding.idempotency,
        { header: action.idempotency.header, required: true },
        `action ${action.id} carries the declared idempotency key`,
      );
    }
    for (const requirement of action.availability?.requires ?? []) {
      if (requirement.field) {
        const type = types.get(requirement.of);
        assert.ok(type, `availability of ${action.id} names a declared type`);
        assert.ok(
          type.fields.some((field) => field.name === requirement.field),
          `availability of ${action.id} names a declared field (${requirement.field})`,
        );
      }
    }
    const declaredErrors = new Map(operation.errors.map((row) => [row.error, row]));
    const outcomeRows = [
      ...(action.outcomes?.errors ?? []),
      ...(action.optimistic?.rollback ?? []),
    ];
    for (const row of outcomeRows) {
      const declared = declaredErrors.get(row.error);
      assert.ok(declared, `${row.error} is declared for ${action.id}`);
      assert.equal(declared.status, row.status, `${row.error} keeps its declared status on ${action.id}`);
    }
    for (const row of endpointBinding.errors ?? []) {
      assert.ok(declaredErrors.has(row.error), `the transport error ${row.error} rides the client union of ${action.id}`);
    }
  }

  for (const state of projection.states) {
    for (const trigger of state.triggers) {
      if (trigger.endpoint) {
        assert.ok(transportByEndpoint.has(trigger.endpoint), `state ${state.id} names a transport endpoint`);
      }
      if (trigger.error) {
        const declared = clientContract.operations.some((operation) =>
          operation.errors.some((row) => row.error === trigger.error));
        assert.ok(declared, `state ${state.id} names a declared error (${trigger.error})`);
      }
    }
  }
}

// ---------------------------------------------------------------------------
// 5. Browser-E2E scenario bindings.
// ---------------------------------------------------------------------------
function e2eBindings() {
  let Ajv2020;
  ({ default: Ajv2020 } = require_("ajv/dist/2020"));
  const schema = JSON.parse(readFileSync(join(repoRoot, "contracts", "scenario-ir.schema.v0.2.16.json"), "utf8"));
  const validate = new Ajv2020({ strict: false, allErrors: true }).compile(schema);
  const scenarioDir = join(modelHome, "lekalo", "scenarios");
  assert.ok(existsSync(scenarioDir), "the screen scenario corpus exists");
  const scenarioIds = new Set();
  for (const name of readdirSync(scenarioDir).filter((candidate) => candidate.endsWith(".scenario.json"))) {
    const raw = readFileSync(join(scenarioDir, name), "utf8");
    const document = JSON.parse(raw);
    assert.ok(validate(document), `${name}: ${JSON.stringify(validate.errors ?? []).slice(0, 300)}`);
    assert.equal(
      JSON.stringify(sortedCompact(document)) + "\n",
      raw,
      `${name} is canonically ordered`,
    );
    assert.equal(document.irRef.digest, irDigest, `${name} pins the committed IR`);
    assert.equal(document.modelRef.digest, modelDigest, `${name} pins the committed Model`);
    const symbol = irDefinitions.get(document.scenarioId);
    assert.ok(symbol && symbol.kind === "scenario", `${name} names a Model scenario`);
    assert.ok(
      symbol.covers.every((covered) => irDefinitions.has(covered)),
      `${name} covers declared symbols`,
    );
    for (const binding of document.bindings) {
      assert.equal(binding.backend, "native", `${name} binds a native runner`);
      assert.equal(binding.runner, projection.e2e.runner, `${name} binds the declared runner`);
    }
    assert.ok(projection.e2e.scenarios.includes(document.scenarioId), `${name} rides the projection e2e block`);
    for (const [key, value] of Object.entries(document.metadata)) {
      if (key === "ui.actions") {
        for (const action of String(value).split(",")) {
          assert.ok(
            projection.actions.some((candidate) => candidate.id === action),
            `${name} metadata names a declared action: ${action}`,
          );
        }
      }
    }
    scenarioIds.add(document.scenarioId);
  }
  for (const expected of projection.e2e.scenarios) {
    assert.ok(scenarioIds.has(expected), `the scenario corpus carries ${expected}`);
  }
}

// ---------------------------------------------------------------------------
// 4. Screen typecheck + SFC compile (pinned tooling via NODE_PATH).
// ---------------------------------------------------------------------------
async function screenTypecheck() {
  let compiler;
  let ts;
  try {
    compiler = require_("@vue/compiler-sfc");
    ts = require_("typescript");
  } catch {
    throw new Error(
      "provision typescript@5.9.3 + @vue/compiler-sfc@3.4.38 through NODE_PATH (CI does the same)",
    );
  }
  const sfcPath = join(uiRoot, "src", "PlannerBoard.vue");
  const parsed = compiler.parse(readFileSync(sfcPath, "utf8"), { filename: sfcPath });
  assert.deepEqual(parsed.errors, [], "the screen SFC parses");
  const compiled = compiler.compileScript(parsed.descriptor, { id: "fixture-planner-board" });
  const virtualPath = join(uiRoot, "src", "PlannerBoard.vue.ts");
  writeFileSync(virtualPath, compiled.content, "utf8");
  try {
    const configFile = join(uiRoot, "tsconfig.json");
    const parsedConfig = ts.getParsedCommandLineOfConfigFile(
      configFile,
      { noEmit: true },
      { ...ts.sys, getCurrentDirectory: () => uiRoot },
    );
    assert.ok(parsedConfig, "the screen tsconfig parses");
    const program = ts.createProgram(parsedConfig.fileNames, parsedConfig.options);
    const diagnostics = ts.getPreEmitDiagnostics(program).map((diagnostic) => ({
      where: diagnostic.file ? diagnostic.file.fileName.slice(uiRoot.length + 1) : "(compiler)",
      message: ts.flattenDiagnosticMessageText(diagnostic.messageText, " ").slice(0, 200),
    }));
    assert.deepEqual(diagnostics, [], "the screen typechecks strict against the generated client");
  } finally {
    rmSync(virtualPath, { force: true });
  }
}

// ---------------------------------------------------------------------------
// 6. The Go backend compiles with the pinned toolchain and is
// gofmt-clean — the second derivation must compile, not just the
// TypeScript one (the read-only convention of the #72 runtime gate).
// ---------------------------------------------------------------------------
function goClientCompiles() {
  const goFile = join(uiRoot, "generated", "planner.client.go");
  assert.ok(existsSync(goFile), "the generated Go client is committed");
  const gofmt = spawnSync("gofmt", ["-l", goFile], { encoding: "utf8" });
  if (gofmt.error || gofmt.status !== 0) {
    throw new Error(`gofmt unavailable or failed: ${String(gofmt.stderr ?? gofmt.error?.message ?? "").slice(0, 200)}`);
  }
  assert.equal((gofmt.stdout ?? "").trim(), "", "the committed Go client is gofmt-clean");
  const build = spawnSync("go", ["build", "./..."], {
    cwd: join(uiRoot, "generated"),
    stdio: "pipe",
    encoding: "utf8",
  });
  if (build.error || build.status !== 0) {
    throw new Error(`go build failed: ${String(build.stderr ?? build.error?.message ?? "").slice(0, 300)}`);
  }
}

// ---------------------------------------------------------------------------
await step("checked join: the committed client is the fresh adapter render of the committed evidence", checkedJoin);
await step("client ↔ OpenAPI sync: one projection of method, path, status, and error identity", clientOpenapiSync);
await step("ui projection traceability: every field, action, and state resolves to a declared symbol", projectionTraceability);
await step("browser-e2e bindings: canonical scenario documents pin the committed join", e2eBindings);
await step("screen typecheck: the maintained Vue screen compiles and typechecks strict", screenTypecheck);
await step("go client compile: the second derivation builds with the pinned toolchain", goClientCompiles);
process.stdout.write(`${JSON.stringify({ ok: true, passed })}\n`);
