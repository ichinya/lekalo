/**
 * #47 scenario mapper suite: pure Scenario IR → test-AST vectors, run in
 * process with Node built-ins only. The honesty rules are asserted
 * literally: every feature without a port surface, runner capability, or
 * resolvable operation lands in `unsupported[]` with its diagnostic, and
 * an unsupported assertion can never compile to a pass.
 */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

import {
  CONCURRENCY_METADATA_KEY,
  DEFAULT_RUNNER,
  IR_REF_MISMATCH,
  OPERATION_UNRESOLVED,
  PORT_MISSING,
  PORT_SHAPE,
  RUNNER_UNKNOWN,
  RUNNER_REGISTRY,
  SCENARIO_IDENTITY,
  SCENARIO_TESTS_DIR,
  UNSUPPORTED_CAPABILITY,
  mapScenario,
} from "../src/scenario-map.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..", "..", "..");

function load(path) {
  return JSON.parse(readFileSync(join(repoRoot, path), "utf8"));
}

const irMinimal = load("tests/fixtures/adapter-conformance/inputs/ir-minimal.json");
const irDigest = "sha256:" + createHash("sha256").update(
  readFileSync(join(repoRoot, "tests/fixtures/adapter-conformance/inputs/ir-minimal.json")),
).digest("hex");

const portDoc = load("tests/fixtures/orchestration/project/lekalo/test-port.json");

function fullPort() {
  return {
    port: {
      path: portDoc.port.path,
      exports: {
        invoke: true, state: true, fixtures: true, actor: true, clock: true,
        ids: true, emissions: true, effects: true, authorize: true,
        contractCheck: true, fixtureDigest: true, reset: true,
      },
    },
  };
}

function invokePort() {
  return { port: { path: "port.mjs", exports: { invoke: true } } };
}

/** The minimal happy-path scenario fixture, with its irRef bound to the IR. */
function happyScenario() {
  const scenario = load("tests/fixtures/scenario/valid/minimal-invoke-result.json");
  scenario.irRef.digest = irDigest;
  return scenario;
}

const idempotentScenario = () => {
  const scenario = load("tests/fixtures/scenario/valid/idempotency-replay.json");
  scenario.irRef.digest = irDigest;
  return scenario;
};

function map(scenario, options = {}) {
  return mapScenario({
    scenario,
    ir: options.ir === undefined ? irMinimal : options.ir,
    irDigest: options.irDigest === undefined ? irDigest : options.irDigest,
    port: options.port === undefined ? fullPort() : options.port,
    portPresent: options.portPresent ?? true,
    profileCapabilities: options.profileCapabilities ?? null,
  });
}

test("the mapper contract constants are pinned", () => {
  assert.equal(SCENARIO_IDENTITY, "dev.lekalo.scenario-ir@0.2.16");
  assert.equal(SCENARIO_TESTS_DIR, "src/generated/node-typescript/scenario-tests");
  assert.equal(DEFAULT_RUNNER, "node:test");
  assert.deepEqual(RUNNER_REGISTRY["node:test"].capabilities, [
    "testing.clock",
    "testing.coverage",
    "testing.event-capture",
    "testing.fixtures",
    "testing.parallel",
  ]);
});

test("the happy path maps to a resolved command invocation with no findings", () => {
  const outcome = map(happyScenario());
  assert.equal(outcome.state, "mapped");
  assert.deepEqual(outcome.findings, []);
  const model = outcome.scenarios[0];
  assert.equal(model.id, "planner.scenario.minimal");
  assert.equal(model.projectId, "planner");
  assert.equal(model.runner.id, "node:test");
  const invoke = model.when[0];
  assert.deepEqual(invoke.operation, { id: "planner.focus_task", kind: "command", ref: "planner.command.focus_task" });
  assert.deepEqual(
    invoke.input.map((entry) => entry.field),
    ["task_id", "user_id"],
  );
  assert.equal(invoke.unsupported, null);
  assert.equal(model.unsupported.length, 0);
  assert.equal(model.given.length, 0);
  assert.equal(model.then[0].kind, "result");
  assert.equal(model.then[0].port, null);
  assert.equal(model.then[0].unsupported, null);
  assert.deepEqual(model.binding, {
    mode: "generated", backend: "none", test: null, capabilityDigest: null,
  });
});

test("the idempotency replay scenario preserves the replay metadata", () => {
  const outcome = map(idempotentScenario());
  assert.equal(outcome.state, "mapped");
  assert.deepEqual(outcome.findings, []);
  const model = outcome.scenarios[0];
  const [focus, replay] = model.when;
  assert.equal(focus.ctx.idempotencyKey.value, "user-1");
  assert.deepEqual(replay.replay, { of: "focus", expect: "no-duplicate" });
  const idempotency = model.then[0];
  assert.equal(idempotency.kind, "idempotency");
  assert.deepEqual(idempotency.payload, {
    replay: "focus",
    equivalence: "identical",
    duplicates: "none",
  });
  assert.equal(idempotency.unsupported, null);
});

test("given preconditions map onto their declared port surfaces", () => {
  const scenario = happyScenario();
  scenario.given = [
    {
      stepId: "seed",
      precondition: {
        kind: "state",
        entity: "planner.task",
        selector: [{ field: "task_id", equals: { type: "string", value: "task-1" } }],
        fields: { user_id: { type: "string", value: "user-1" } },
      },
    },
    { stepId: "clock", precondition: { kind: "clock", at: { type: "datetime", value: "2026-01-02T03:04:05Z" } } },
    { stepId: "who", precondition: { kind: "actor", actor: "planner/member" } },
  ];
  const outcome = map(scenario);
  const [seed, clock, who] = outcome.scenarios[0].given;
  assert.equal(seed.port, "state");
  assert.equal(seed.unsupported, null);
  assert.deepEqual(seed.payload.fields, [["user_id", { type: "string", value: "user-1" }]]);
  assert.equal(clock.port, "clock");
  assert.equal(clock.payload.at, "2026-01-02T03:04:05Z");
  assert.equal(who.port, "actor");
});

test("port surfaces that are absent compile to explicit unsupported rows", () => {
  const scenario = happyScenario();
  scenario.given = [
    {
      stepId: "seed",
      precondition: {
        kind: "state",
        entity: "planner.task",
        selector: [{ field: "task_id", equals: { type: "string", value: "task-1" } }],
        fields: { user_id: { type: "string", value: "user-1" } },
      },
    },
  ];
  scenario.then = [
    {
      stepId: "state",
      observes: "run",
      assertion: {
        kind: "entity_state",
        entity: "planner.task",
        selector: [{ field: "task_id", equals: { type: "string", value: "task-1" } }],
        expect: "exists",
        fields: {},
      },
    },
    {
      stepId: "emitted",
      observes: "run",
      assertion: { kind: "emitted", target: { kind: "event", id: "planner.task_focused" } },
    },
    {
      stepId: "declared",
      observes: "run",
      assertion: { kind: "unsupported", capability: "testing.concurrency", note: "race" },
    },
  ];
  const outcome = map(scenario, { port: invokePort() });
  assert.deepEqual(outcome.findings, []);
  const model = outcome.scenarios[0];
  assert.equal(model.given[0].port, null);
  assert.equal(model.given[0].unsupported.capability, "testing.state");
  assert.equal(model.then[0].unsupported.capability, "testing.state");
  assert.equal(model.then[1].unsupported.capability, "testing.emissions");
  // The declared unsupported expectation carries its own capability ref.
  assert.equal(model.then[2].unsupported.capability, "testing.concurrency");
  assert.equal(model.then[2].unsupported.reason, "declared-unsupported");
  // Three unsupported rows exist; none of them is a pass.
  assert.equal(model.unsupported.length, 0, "step-level rows stay on their steps");
  const invoke = model.when[0];
  assert.deepEqual(invoke.operation, { id: "planner.focus_task", kind: "command", ref: "planner.command.focus_task" });
});

test("an absent port declaration is a compile-time finding", () => {
  const outcome = map(happyScenario(), { port: null, portPresent: false });
  assert.equal(outcome.state, "mapped");
  assert.equal(outcome.findings.length, 1);
  assert.equal(outcome.findings[0].code, PORT_MISSING);
  const model = outcome.scenarios[0];
  assert.equal(model.when[0].unsupported.reason, "port-surface-absent");
});

test("a malformed port declaration is a shape finding", () => {
  const outcome = map(happyScenario(), {
    port: { port: { path: "port.mjs", exports: {} } },
  });
  assert.equal(outcome.findings[0].code, PORT_SHAPE);
});

test("operation references resolve from the compiled IR, never the name", () => {
  const scenario = happyScenario();
  scenario.when[0].action.operation = "planner.query.count_focused";
  const query = map(scenario);
  assert.equal(query.scenarios[0].when[0].operation.kind, "query");
  assert.deepEqual(query.findings, []);

  const unresolved = map(happyScenario(), {
    ir: { contract: "dev.lekalo.ir@0.2.16", definitions: [] },
  });
  assert.equal(unresolved.scenarios[0].when[0].operation, null);
  assert.equal(unresolved.scenarios[0].when[0].unsupported.reason, "operation-unresolved");
  assert.equal(unresolved.findings[0].code, OPERATION_UNRESOLVED);
  assert.equal(unresolved.findings[0].detail, "planner.command.focus_task");

  // A kind-qualified segment that disagrees with the declared kind is
  // unresolved, never silently coerced.
  const lying = map(happyScenario(), {
    ir: {
      contract: "dev.lekalo.ir@0.2.16",
      definitions: [{ id: "planner.focus_task", kind: "query" }],
    },
  });
  assert.equal(lying.scenarios[0].when[0].operation, null);
  assert.equal(lying.findings[0].code, OPERATION_UNRESOLVED);
});

test("an irRef digest mismatch is a compile-time finding", () => {
  const scenario = happyScenario();
  scenario.irRef.digest = "sha256:" + "0".repeat(64);
  const outcome = map(scenario);
  assert.equal(outcome.findings[0].code, IR_REF_MISMATCH);
});

test("malformed scenario wires are refusals, never findings", () => {
  assert.equal(map(null).state, "refused");
  assert.equal(map({}).state, "refused");
  const wrongIdentity = happyScenario();
  wrongIdentity.identity = "dev.lekalo.scenario-ir@0.2.15";
  assert.equal(map(wrongIdentity).refusal, "scenario-identity");
  const empty = happyScenario();
  empty.when = [];
  assert.equal(map(empty).refusal, "scenario-empty");
  const duplicate = happyScenario();
  duplicate.then[0].stepId = duplicate.when[0].stepId;
  assert.equal(map(duplicate).refusal, "scenario-duplicate-step-id");
  const overbound = happyScenario();
  overbound.given = Array.from({ length: 257 }, (_, index) => ({
    stepId: `s${index}`,
    precondition: { kind: "clock", at: { type: "datetime", value: "2026-01-02T03:04:05Z" } },
  }));
  assert.equal(map(overbound).refusal, "scenario-steps-bound");
});

test("concurrency race metadata compiles to a whole-scenario unsupported row", () => {
  const scenario = happyScenario();
  scenario.metadata = { [CONCURRENCY_METADATA_KEY]: "schedule" };
  const outcome = map(scenario);
  assert.deepEqual(outcome.findings, []);
  const model = outcome.scenarios[0];
  assert.equal(model.unsupported.length, 1);
  assert.equal(model.unsupported[0].capability, "testing.concurrency");
  assert.equal(model.unsupported[0].reason, "scenario-requires-concurrency");
});

test("unknown runners are findings; binding capability gaps are unsupported rows", () => {
  const unknown = happyScenario();
  unknown.bindings = [{
    backend: "native", runner: "jest", runner_version: "29.0.0",
    capabilities: [], capability_digest: "sha256:" + "0".repeat(64), test: "planner.scenario.minimal",
  }];
  const unknownOutcome = map(unknown);
  assert.equal(unknownOutcome.findings[0].code, RUNNER_UNKNOWN);
  assert.equal(unknownOutcome.scenarios[0].runner.id, DEFAULT_RUNNER);

  const gap = happyScenario();
  gap.bindings = [{
    backend: "native", runner: "node:test", runner_version: "24.0.0",
    capabilities: ["testing.concurrency"], capability_digest: "sha256:" + "0".repeat(64),
    test: "planner.scenario.minimal",
  }];
  const gapOutcome = map(gap);
  assert.deepEqual(gapOutcome.findings, []);
  const row = gapOutcome.scenarios[0].unsupported.find(
    (entry) => entry.reason === "binding-capability-gap",
  );
  assert.equal(row.capability, "testing.concurrency");

  const replay = idempotentScenario();
  replay.bindings = [{
    backend: "native", runner: "node:test", runner_version: "24.0.0",
    capabilities: ["idempotency.replay"], capability_digest: "sha256:" + "0".repeat(64),
    test: "planner.scenario.idempotent_focus", mode: "generated",
  }];
  const replayOutcome = map(replay);
  assert.deepEqual(replayOutcome.findings, []);
  assert.equal(replayOutcome.scenarios[0].unsupported.length, 0);
  assert.equal(replayOutcome.scenarios[0].binding.mode, "generated");
  assert.equal(replayOutcome.scenarios[0].binding.test, "planner.scenario.idempotent_focus");
});

test("fake-reference bindings stay untouched and produce no join", () => {
  const scenario = happyScenario();
  scenario.bindings = [{
    backend: "fake-reference", runner: "node:test", runner_version: "24.0.0",
    capabilities: ["testing.concurrency"], capability_digest: "sha256:" + "0".repeat(64),
    test: "planner.scenario.minimal",
  }];
  const outcome = map(scenario);
  assert.deepEqual(outcome.findings, []);
  assert.equal(outcome.scenarios[0].unsupported.length, 0);
  assert.equal(outcome.scenarios[0].binding.backend, "none");
});

test("the mapping is deterministic over identical inputs", () => {
  const first = JSON.stringify(map(happyScenario()));
  const second = JSON.stringify(map(happyScenario()));
  assert.equal(first, second);
});

test("typed leaves outside the closed set or bounds are unsupported, not crashes", () => {
  const scenario = happyScenario();
  scenario.when[0].action.input.task_id = { type: "float", value: 1.5 };
  const outcome = map(scenario);
  const entry = outcome.scenarios[0].when[0].input[0];
  assert.equal(entry.leafProblem, "leaf-value-kind");
});

test("deep leaves beyond the typed depth bound are refused to leaf-depth", () => {
  const scenario = happyScenario();
  let leaf = { type: "string", value: "x" };
  for (let index = 0; index < 40; index += 1) {
    leaf = { type: "list", value: [leaf] };
  }
  scenario.when[0].action.input.task_id = leaf;
  const outcome = map(scenario);
  assert.equal(outcome.scenarios[0].when[0].input[0].leafProblem, "leaf-depth");
});

test("unsupported findings carry the registered code spellings", () => {
  assert.equal(UNSUPPORTED_CAPABILITY, "scenario.unsupported-capability");
  assert.equal(PORT_MISSING, "scenario.port-missing");
  assert.equal(PORT_SHAPE, "scenario.port-shape");
  assert.equal(OPERATION_UNRESOLVED, "scenario.operation-unresolved");
  assert.equal(RUNNER_UNKNOWN, "scenario.runner-unknown");
  assert.equal(IR_REF_MISMATCH, "scenario.ir-ref-mismatch");
});
