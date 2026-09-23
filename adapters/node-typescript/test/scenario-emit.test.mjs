/**
 * #47 scenario emitter suite: byte-stable TypeScript emission over the
 * mapped test AST. The determinism contract is asserted literally
 * (repeat emission is byte-identical, no timestamps/paths), the emitted
 * test files execute against an in-memory port under the vendored
 * toolchain (`node --test` with type stripping), and the generated
 * TypeScript typechecks under the pinned compiler.
 */
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdtempSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

import {
  ADAPTER_ID,
  commentSafe,
  MAP_CONTRACT,
  RESERVED_MODULES,
  RUN_RECORD_DIR,
  RUN_RECORD_IDENTITY,
  RUN_RECORD_SCHEMA_VERSION,
  SCENARIO_DIR,
  emitScenarioTests,
  identifierOf,
  literalOf,
  moduleOf,
} from "../src/scenario-emit.mjs";
import { mapScenario } from "../src/scenario-map.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..", "..", "..");
const ADAPTER_VERSION = "0.4.0-test";

function load(path) {
  return JSON.parse(readFileSync(join(repoRoot, path), "utf8"));
}

const irMinimal = load("tests/fixtures/adapter-conformance/inputs/ir-minimal.json");
const irDigest = "sha256:" + createHash("sha256").update(
  readFileSync(join(repoRoot, "tests/fixtures/adapter-conformance/inputs/ir-minimal.json")),
).digest("hex");

function happyScenario() {
  const scenario = load("tests/fixtures/scenario/valid/minimal-invoke-result.json");
  scenario.irRef.digest = irDigest;
  return JSON.parse(JSON.stringify(scenario));
}

function idempotentScenario() {
  const scenario = load("tests/fixtures/scenario/valid/idempotency-replay.json");
  scenario.irRef.digest = irDigest;
  return JSON.parse(JSON.stringify(scenario));
}

/**
 * An inline seeded happy path: the repo minimal fixture has no seed, but
 * the fixture port answers task_missing for an unseeded task — execution
 * vectors must be behaviorally honest, so they seed before focusing.
 */
function seededScenario() {
  return {
    schemaVersion: "lekalo/scenario-ir/v0.2.16",
    identity: "dev.lekalo.scenario-ir@0.2.16",
    projectId: "planner",
    scenarioId: "planner.scenario.focus_seed",
    scenarioVersion: "0.2.16",
    summary: "Seed one task and focus it.",
    irRef: { digest: irDigest, identity: "dev.lekalo.ir@0.2.16" },
    modelRef: { modelVersion: "0.2.16", digest: "sha256:" + "2".repeat(64) },
    given: [{
      stepId: "seed",
      precondition: {
        kind: "state",
        entity: "planner.task",
        selector: [{ field: "task_id", equals: { type: "string", value: "task-1" } }],
        fields: { user_id: { type: "string", value: "user-1" } },
      },
    }],
    when: [{
      stepId: "run",
      action: {
        kind: "invoke",
        operation: "planner.command.focus_task",
        input: {
          task_id: { type: "string", value: "task-1" },
          user_id: { type: "string", value: "user-1" },
        },
      },
    }],
    then: [{ stepId: "output", observes: "run", assertion: { kind: "result", valueType: "planner.task" } }],
    bindings: [],
    tags: [],
    metadata: {},
  };
}

const fullPort = {
  port: {
    path: "src/testing/port.mjs",
    exports: {
      invoke: true, state: true, fixtures: true, actor: true, clock: true,
      ids: true, emissions: true, effects: true, authorize: true,
      contractCheck: true, fixtureDigest: true, reset: true,
    },
  },
};

function map(scenario, port = fullPort) {
  return mapScenario({
    scenario,
    ir: irMinimal,
    irDigest,
    port,
    portPresent: true,
    profileCapabilities: null,
  });
}

function emit(models, overrides = {}) {
  return emitScenarioTests({
    models,
    inputDigest: "sha256:" + "1".repeat(64),
    adapterVersion: ADAPTER_VERSION,
    portModulePath: "src/testing/port.mjs",
    ...overrides,
  });
}

const canonicalByteOrder = (left, right) => (left < right ? -1 : left > right ? 1 : 0);

test("emission layout, ordering, and the byte-stability contract", () => {
  const outcome = {
    scenarios: [...map(happyScenario()).scenarios, ...map(idempotentScenario()).scenarios],
  };
  const files = emit(outcome.scenarios);
  const paths = files.map((entry) => entry.path);
  assert.deepEqual(paths, [...paths].sort(canonicalByteOrder));
  assert.deepEqual(paths, [
    "src/generated/node-typescript/scenario-tests/planner/planner.scenario.idempotent_focus.map.json",
    "src/generated/node-typescript/scenario-tests/planner/planner.scenario.idempotent_focus.test.ts",
    "src/generated/node-typescript/scenario-tests/planner/planner.scenario.minimal.map.json",
    "src/generated/node-typescript/scenario-tests/planner/planner.scenario.minimal.test.ts",
    "src/generated/node-typescript/scenario-tests/port.ts",
    "src/generated/node-typescript/scenario-tests/reporter.mjs",
    "src/generated/node-typescript/scenario-tests/testkit.ts",
  ]);
  for (const entry of files) {
    assert.equal(entry.text.endsWith("\n"), true, `${entry.path} single final newline`);
    assert.equal(entry.text.includes("\r"), false, `${entry.path} LF only`);
    assert.equal(/[ \t]+\n/.test(entry.text), false, `${entry.path} no trailing whitespace`);
    if (!entry.path.endsWith(".map.json")) {
      assert.equal(entry.text.includes("C:"), false, `${entry.path} no host paths`);
    }
  }
  const testFile = files.find((entry) => entry.path.endsWith("minimal.test.ts"));
  assert.match(testFile.text, /^\/\/ Generated by lekalo-target-node-typescript@/);
  assert.match(testFile.text, /input sha256:1{64}/);
  assert.match(testFile.text, /test\("lekalo:planner\.scenario\.minimal"/);
});

test("repeat emission is byte-identical and independent of host state", () => {
  const first = emit(map([happyScenario()]).scenarios).map((entry) => entry.text);
  const second = emit(map([happyScenario()]).scenarios).map((entry) => entry.text);
  assert.deepEqual(first, second);
});

test("sidecars carry the declaration ranges of every then-step", () => {
  const files = emit(map(idempotentScenario()).scenarios);
  const sidecar = files.find((entry) => entry.path.endsWith(".map.json"));
  const mapDocument = JSON.parse(sidecar.text);
  assert.equal(mapDocument.contract, MAP_CONTRACT);
  assert.equal(mapDocument.adapter.id, ADAPTER_ID);
  assert.equal(mapDocument.owner, "planner.scenario.idempotent_focus");
  const ids = mapDocument.declarations.map((declaration) => declaration.id);
  assert.ok(ids.includes("scenario:planner.scenario.idempotent_focus"));
  assert.ok(ids.includes("then:no_duplicates"));
  for (const declaration of mapDocument.declarations) {
    assert.ok(declaration.start < declaration.end, "half-open ranges");
    const testFile = files.find((entry) => entry.path === sidecar.path.replace(/\.map\.json$/, ".test.ts"));
    assert.ok(declaration.end <= Buffer.byteLength(testFile.text), "ranges cover the test file, not the sidecar");
  }
  // The sidecar is canonical: byte-sorted keys, compact.
  const reparsed = JSON.parse(sidecar.text);
  const canonicalOf = (value) => {
    if (value === null) return "null";
    if (Array.isArray(value)) return `[${value.map(canonicalOf).join(",")}]`;
    if (typeof value === "object") {
      return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalOf(value[key])}`).join(",")}}`;
    }
    return JSON.stringify(value);
  };
  assert.equal(sidecar.text.trimEnd(), canonicalOf(reparsed));
});

test("typed literal rendering covers the closed value domain", async () => {
  const { emitValue } = await import("../src/scenario-emit.mjs");
  assert.equal(emitValue(literalOf({ type: "null", value: null })), "null");
  assert.equal(emitValue(literalOf({ type: "boolean", value: true })), "true");
  assert.equal(emitValue(literalOf({ type: "integer", value: 3 })), "3");
  assert.equal(emitValue(literalOf({ type: "integer", value: "123456789012345678901234567890" })), "123456789012345678901234567890n");
  assert.equal(emitValue(literalOf({ type: "decimal", value: "12.5" })), '"12.5"');
  assert.equal(emitValue(literalOf({ type: "datetime", value: "2026-01-02T03:04:05Z" })), '"2026-01-02T03:04:05Z"');
  assert.equal(emitValue(literalOf({ type: "list", value: [{ type: "integer", value: 1 }] })), "[1]");
  assert.equal(
    emitValue(literalOf({ type: "object", value: { b: { type: "string", value: "y" }, a: { type: "null" } } })),
    '{ "b": "y", "a": null }',
  );
});

test("whole-scenario unsupported compiles to rows and a skip, never a pass", () => {
  const scenario = happyScenario();
  scenario.metadata = { "testing.concurrency": "schedule" };
  const files = emit(map(scenario).scenarios);
  const testFile = files.find((entry) => entry.path.endsWith(".test.ts"));
  assert.match(testFile.text, /outcome: "unsupported"/);
  assert.match(testFile.text, /t\.skip\("scenario\.unsupported-capability"\)/);
  assert.doesNotMatch(testFile.text, /port\.invoke/);
});

test("reserved module collisions are refused", () => {
  const scenario = happyScenario();
  scenario.scenarioId = "testkit.scenario.mystery";
  assert.throws(() => emit(map(scenario).scenarios), /reserved emitted file/);
  assert.ok(RESERVED_MODULES.includes("port"));
});

test("generated tests execute green under node:test against the fixture port", () => {
  const dir = mkdtempSync(join(tmpdir(), "lekalo-scenario-"));
  try {
    materializeFixtureProject(dir);
    const result = spawnSync(process.execPath, ["--test", ...generatedTestFiles(dir)], {
      cwd: join(dir, SCENARIO_DIR),
      encoding: "utf8",
      timeout: 120000,
      env: spawnedEnv(),
    });
    const output = `${result.stdout}${result.stderr}`;
    assert.equal(
      result.status,
      0,
      `generated suite must pass:\n${output.slice(0, 4000)}`,
    );
    assert.match(output, /pass \d+/);
    // The run records landed in the ingest home.
    for (const id of ["planner.scenario.focus_seed", "planner.scenario.idempotent_focus"]) {
      const recordPath = join(dir, RUN_RECORD_DIR, `${id}.json`);
      assert.ok(existsSync(recordPath), `run record for ${id}`);
      const record = JSON.parse(readFileSync(recordPath, "utf8"));
      assert.equal(record.schema_version, RUN_RECORD_SCHEMA_VERSION);
      assert.equal(record.identity, RUN_RECORD_IDENTITY);
      assert.equal(record.scenario.ir_digest, irDigest);
      assert.ok(record.assertions.length >= 1);
      for (const row of record.assertions) {
        assert.ok(
          ["pass", "fail", "unsupported", "infrastructure", "degraded"].includes(row.outcome),
          `closed outcome vocabulary: ${row.outcome}`,
        );
      }
      assert.match(record.test.path, /\.test\.ts$/);
      assert.match(record.test.fingerprint, /^sha256:[0-9a-f]{64}$/);
      assert.doesNotMatch(JSON.stringify(record), /[/\\]Users[/\\]|[/\\]tmp[/\\]|C:\\\\Users/);
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("reruns are isolated: the port reset clears state between runs", () => {
  const dir = mkdtempSync(join(tmpdir(), "lekalo-scenario-"));
  try {
    materializeFixtureProject(dir);
    const args = ["--test", ...generatedTestFiles(dir)];
    const run = (what) => {
      const result = spawnSync(process.execPath, args, {
        cwd: join(dir, SCENARIO_DIR),
        encoding: "utf8",
        timeout: 120000,
        env: spawnedEnv(),
      });
      assert.equal(result.status, 0, `${what} must pass: ${result.stdout}${result.stderr}`);
      return readFileSync(join(dir, RUN_RECORD_DIR, "planner.scenario.focus_seed.json"), "utf8");
    };
    const first = run("first run");
    const second = run("second run");
    // A rerun sees a fresh port (reset at test start): the same rows
    // come out pass again, and the record is byte-identical.
    assert.equal(first, second, "the record is deterministic over identical runs");
    const record = JSON.parse(first);
    assert.equal(
      record.assertions.every((row) => row.outcome === "pass"),
      true,
      "every row passes on both runs",
    );
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("the concurrency scenario skips and records unsupported, never a pass", () => {
  const dir = mkdtempSync(join(tmpdir(), "lekalo-scenario-"));
  try {
    materializeFixtureProject(dir);
    const scenario = happyScenario();
    scenario.scenarioId = "planner.scenario.concurrent_focus";
    scenario.metadata = { "testing.concurrency": "schedule" };
    const outcome = map(scenario);
    const files = emit(outcome.scenarios);
    for (const entry of files) {
      const target = join(dir, entry.path);
      mkdirSync(dirname(target), { recursive: true });
      writeFileSync(target, entry.text);
    }
    const result = spawnSync(process.execPath, ["--test", ...generatedTestFiles(dir)], {
      cwd: join(dir, SCENARIO_DIR),
      encoding: "utf8",
      timeout: 120000,
      env: spawnedEnv(),
    });
    const output = `${result.stdout}${result.stderr}`;
    // Skipped is not a failure and never a pass.
    assert.match(output, /skipped 1/);
    const record = JSON.parse(
      readFileSync(join(dir, RUN_RECORD_DIR, "planner.scenario.concurrent_focus.json"), "utf8"),
    );
    assert.equal(
      record.assertions.every((row) => row.outcome === "unsupported"),
      true,
      "every row is unsupported",
    );
    assert.equal(
      record.assertions.some((row) => row.outcome === "pass"),
      false,
      "unsupported never reports pass",
    );
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

/**
 * The spawned `node --test` run must not inherit the outer runner's
 * test context (a node:test grandchild would refuse to run files).
 */
function spawnedEnv() {
  const env = { ...process.env };
  delete env.NODE_TEST_CONTEXT;
  delete env.NODE_TEST_RUNNER;
  return env;
}

/**
 * The generated test files of one materialized root, as relative POSIX
 * paths under the generated scenario-test home — the deterministic
 * spawn set (never directory globs, whose resolution differs per host).
 */
function generatedTestFiles(dir) {
  const home = join(dir, SCENARIO_DIR);
  const found = [];
  const walk = (current) => {
    for (const entry of readdirSync(current)) {
      const full = join(current, entry);
      if (statSync(full).isDirectory()) walk(full);
      else if (entry.endsWith(".test.ts")) found.push(full.slice(home.length + 1).split("\\").join("/"));
    }
  };
  walk(home);
  return found.sort();
}

/**
 * Materialize the fixture project subset the generated tests need: the
 * in-memory planner port (S5), the generated files, and a package.json
 * pinning ESM. The port module is copied from the committed fixture.
 */
function materializeFixtureProject(dir) {
  mkdirSync(join(dir, "src", "testing"), { recursive: true });
  cpSync(
    join(repoRoot, "tests", "fixtures", "orchestration", "project", "src", "testing", "port.mjs"),
    join(dir, "src", "testing", "port.mjs"),
  );
  writeFileSync(join(dir, "package.json"), JSON.stringify({ type: "module", private: true }));
  const happy = map(seededScenario());
  const idempotent = map(idempotentScenario());
  for (const entry of emit([...happy.scenarios, ...idempotent.scenarios])) {
    const target = join(dir, entry.path);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, entry.text);
  }
}

test("generated files typecheck under the pinned typescript", () => {
  // The exact compiler pin the adapter bundles, resolved from the
  // adapter's build-time node_modules — never ambient resolution.
  const require = createRequire(join(repoRoot, "adapters", "node-typescript", "package.json"));
  const ts = require("typescript/lib/typescript.js");
  assert.equal(ts.version, "5.9.3", "compiler pin drift");
  const dir = mkdtempSync(join(tmpdir(), "lekalo-scenario-ts-"));
  try {
    materializeFixtureProject(dir);
    // Ambient declarations so the generated tests typecheck without
    // @types/node: only the two node builtins the tests import.
    writeFileSync(join(dir, "ambient.d.ts"), `declare module "node:test" {
  export function test(name: string, body: (t: { skip(reason?: string): void }) => void | Promise<void>): void;
}
declare module "node:assert/strict" {
  const assert: {
    equal(actual: unknown, expected: unknown, message?: string): void;
    ok(value: unknown, message?: string): void;
    fail(message?: string): never;
    AssertionError: new (message?: string) => Error;
  };
  export = assert;
}
declare var process: { versions: { node: string } };
declare var ImportMeta: { url: string };
`);
    const rootNames = [];
    const walk = (current) => {
      for (const entry of readdirSync(current)) {
        const full = join(current, entry);
        if (statSync(full).isDirectory()) walk(full);
        else if (/\.(ts|mjs)$/.test(entry)) rootNames.push(full);
      }
    };
    walk(join(dir, SCENARIO_DIR));
    const program = ts.createProgram(
      [...rootNames, join(dir, "ambient.d.ts")],
      {
        noEmit: true,
        strict: true,
        // Documented carve-outs (mirrors issue #45 review r2 F-4): the
        // emitted port.ts shim imports the plain-JavaScript project port
        // (its own @ts-expect-error covers that edge), so noImplicitAny
        // is relaxed, and catch blocks treat thrown values as opaque
        // (the emitted code reads only a bounded message off them);
        // type stripping requires the explicit .ts import extensions,
        // allowed under bundler resolution.
        noImplicitAny: false,
        useUnknownInCatchVariables: false,
        allowImportingTsExtensions: true,
        verbatimModuleSyntax: false,
        target: ts.ScriptTarget.ES2022,
        module: ts.ModuleKind.ESNext,
        moduleResolution: ts.ModuleResolutionKind.Bundler,
        allowJs: true,
        checkJs: false,
        skipLibCheck: true,
        types: [],
      },
      ts.createCompilerHost({}, true),
    );
    const diagnostics = ts.getPreEmitDiagnostics(program).filter(
      (diagnostic) => !String(diagnostic.messageText).includes("@ts-expect-error"),
    );
    const rendered = diagnostics
      .map((diagnostic) => ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"))
      .join("\n");
    assert.equal(diagnostics.length, 0, `typecheck diagnostics:\n${rendered}`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("module naming and identifier sanitization stay deterministic", () => {
  assert.equal(moduleOf("planner.scenario.minimal"), "planner");
  assert.equal(moduleOf("solo"), "solo");
  assert.equal(identifierOf("planner.scenario.a-1"), "planner_scenario_a_1");
  assert.equal(identifierOf("9lives"), "_9lives");
});

test('the summary is comment-safe: newlines cannot inject code (F-1)', () => {
  const scenario = happyScenario();
  scenario.summary = 'innocent\nthrow new Error("injected") // tail';
  const files = emit(map(scenario).scenarios);
  const testFile = files.find((entry) => entry.path.endsWith('minimal.test.ts'));
  const commentLines = testFile.text.split('\n').filter((line) => line.includes('innocent'));
  assert.equal(commentLines.length, 1, 'the summary stays on one comment line');
  // Every occurrence of the hostile payload stays inside a comment line:
  // collapsed, never emitted as live TypeScript.
  for (const line of testFile.text.split('\n')) {
    if (line.includes('throw new Error')) {
      assert.match(line, /^\s*\/\//, 'injected text stays commented: ' + line);
    }
  }
  assert.equal(commentSafe('a\r\nb\u0000c\n'), 'a b c');
});

test('observes on a consumed given step resolves the given binding and runs (F-2)', () => {
  // The core data-flow permits then.observes to name a consumed given
  // precondition; the emitted test must not reference an undeclared
  // step_* variable (review reproduced a ReferenceError).
  const scenario = seededScenario();
  scenario.then = [{
    stepId: 'state',
    observes: 'seed',
    assertion: {
      kind: 'entity_state',
      entity: 'planner.task',
      where: [{ field: 'task_id', equals: { type: 'string', value: 'task-1' } }],
      expect: { presence: 'exists' },
      fields: { user_id: { value: { type: 'string', value: 'user-1' } } },
    },
  }];
  const files = emit(map(scenario).scenarios);
  const testFile = files.find((entry) => entry.path.endsWith('.test.ts'));
  assert.match(testFile.text, /given_seed/, 'the given binding exists');
  assert.doesNotMatch(testFile.text, /\bstep_seed\b/, 'no phantom step_ variable');
  // The entity_state check queries the port, so the test executes green.
  const dir = mkdtempSync(join(tmpdir(), 'lekalo-scenario-f2-'));
  try {
    materializeFixtureProject(dir);
    const result = spawnSync(process.execPath, ['--test', ...generatedTestFiles(dir)], {
      cwd: join(dir, SCENARIO_DIR),
      encoding: 'utf8',
      timeout: 120000,
      env: spawnedEnv(),
    });
    assert.equal(result.status, 0, `${result.stdout}${result.stderr}`.slice(0, 2000));
    const record = JSON.parse(
      readFileSync(join(dir, RUN_RECORD_DIR, 'planner.scenario.focus_seed.json'), 'utf8'),
    );
    assert.equal(
      record.assertions.every((row) => row.outcome === 'pass'),
      true,
    );
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('assertion semantics are enforced, never approximated (F-3)', () => {
  // presence "exists" compiles to >= 1, not == 1 (multi-row entities).
  const existsScenario = happyScenario();
  existsScenario.then = [{
    stepId: 'state',
    observes: 'run',
    assertion: {
      kind: 'entity_state',
      entity: 'planner.task',
      where: [{ field: 'task_id', equals: { type: 'string', value: 'task-1' } }],
      expect: { presence: 'exists' },
      fields: {
        focused_at: { match: 'datetime' },
        task_id: { match: 'uuid' },
        score: { match: 'decimal' },
        link: { match: 'uri' },
      },
    },
  }];
  const files = emit(map(existsScenario).scenarios);
  const testFile = files.find((entry) => entry.path.endsWith('.test.ts'));
  assert.match(testFile.text, /stateRows\.length >= 1, "entity-exists"/);
  assert.doesNotMatch(testFile.text, /"entity-count"/, 'exists is not == 1');
  // Typed matchers compile to real canonical-form checks.
  assert.match(testFile.text, /\[0-9a-f]\{8\}-\[0-9a-f]\{4\}/, 'uuid matcher');
  assert.match(testFile.text, /entity-match:task_id:uuid/);
  assert.match(testFile.text, /entity-match:focused_at:datetime/);
  assert.match(testFile.text, /entity-match:score:decimal/);
  assert.match(testFile.text, /entity-match:link:uri/);
  assert.doesNotMatch(testFile.text, /entity-match:[a-z_]+"/, 'no matcher weakens to bare non-null');

  // forbidden_effect resource scope lands in unsupported, not a weak filter.
  const resourceScenario = happyScenario();
  resourceScenario.then = [{
    stepId: 'no_resource',
    observes: 'run',
    assertion: { kind: 'forbidden_effect', effect: 'planner.create_task', scope: 'resource' },
  }];
  const resourceFiles = emit(map(resourceScenario).scenarios);
  const resourceTest = resourceFiles.find((entry) => entry.path.endsWith('.test.ts'));
  assert.match(resourceTest.text, /outcome: "unsupported"/);
  assert.match(resourceTest.text, /scope-unimplemented/);

  // equivalence "equivalent" lands in unsupported, never strict equality.
  const equivalentScenario = happyScenario();
  equivalentScenario.then = [{
    stepId: 'no_duplicates',
    observes: 'run',
    assertion: { kind: 'idempotency', replay: 'run', equivalence: 'equivalent', duplicates: 'none' },
  }];
  const equivalentOutcome = map(equivalentScenario);
  assert.equal(equivalentOutcome.scenarios[0].then[0].unsupported.reason, 'equivalence-unimplemented');
  const equivalentFiles = emit(equivalentOutcome.scenarios);
  const equivalentTest = equivalentFiles.find((entry) => entry.path.endsWith('.test.ts'));
  assert.match(equivalentTest.text, /outcome: "unsupported"/);

  // An unknown match kind is unsupported at map time, never weakened.
  const unknownMatch = happyScenario();
  unknownMatch.then = [{
    stepId: 'state',
    observes: 'run',
    assertion: {
      kind: 'entity_state',
      entity: 'planner.task',
      where: [{ field: 'task_id', equals: { type: 'string', value: 'task-1' } }],
      expect: { presence: 'exists' },
      fields: { mystery: { match: 'regex' } },
    },
  }];
  assert.equal(map(unknownMatch).scenarios[0].then[0].unsupported.capability, 'scenario.match-kind');

  // error.contract emits the port contractCheck call when supported.
  const contractScenario = happyScenario();
  contractScenario.then = [{
    stepId: 'typed',
    observes: 'run',
    assertion: {
      kind: 'error',
      error: 'planner.error.task_missing',
      payload: { task_id: { value: { type: 'string', value: 'task-1' } } },
      contract: 'planner/error-contract',
    },
  }];
  const contractFiles = emit(map(contractScenario).scenarios);
  const contractTest = contractFiles.find((entry) => entry.path.endsWith('.test.ts'));
  assert.match(contractTest.text, /port\.contractCheck\("planner\/error-contract"/);
  assert.match(contractTest.text, /"error-contract"/);
});

test('checked bindings emit no self-claiming test (F-4)', () => {
  const scenario = happyScenario();
  scenario.bindings = [{
    backend: "native",
    runner: "node:test",
    runner_version: "24.0.0",
    capabilities: [],
    capability_digest: "sha256:" + "0".repeat(64),
    test: "planner.scenario.minimal",
    mode: "checked",
  }];
  const files = emit(map(scenario).scenarios);
  const paths = files.map((entry) => entry.path);
  // The support files still emit; the scenario identity does not.
  assert.equal(paths.length, 3, JSON.stringify(paths));
  for (const path of paths) {
    assert.doesNotMatch(path, /minimal/, 'no emitted artifact claims the checked id');
  }
  assert.ok(!files.some((entry) => entry.text.includes('lekalo:planner.scenario.minimal')),
    'no emitted file claims the checked lekalo: title');
});
