/**
 * Deterministic TypeScript emitter for the scenario-test compiler
 * (issue #47, plan S4). Input is the pure test AST of `scenario-map.mjs`;
 * output is one TypeScript test file per scenario plus the runner-neutral
 * `testkit.ts`, the run-record reporter `reporter.mjs`, and the port
 * shim `port.ts`, each test paired with one canonical `.map.json`
 * sidecar.
 *
 * Byte stability is the contract (issue #45 conventions):
 * - fixed header comment (adapter id/version, contract identities, the
 *   `sha256:` digest of the exact scenario document bytes — no
 *   timestamps, no host paths, no host data);
 * - sorted imports, deterministic scenario/step ordering, 2-space
 *   indent, LF endings, no trailing whitespace, exactly one final
 *   newline;
 * - string literals emitted via JSON.stringify (no escaping drift);
 * - sidecars are canonical JSON (keys sorted by unsigned UTF-8 byte
 *   order, compact) with the same final-newline rule.
 *
 * Evidence honesty (plan §7): every assertion block records exactly one
 * outcome row (`pass | fail | unsupported | infrastructure`), unsupported
 * rows can never become passes — a test with any unsupported row and no
 * failure ends in `t.skip(...)`, which the runner reports as skipped —
 * and the reporter persists the rows into the durable run record.
 */

import { canonicalJson } from "./zod-emit.mjs";

/** The emitting adapter identity; kept in lockstep with the kernel. */
export const ADAPTER_ID = "lekalo-target-node-typescript";

/** The sidecar micro-contract token of the scenario test maps. */
export const MAP_CONTRACT = "lekalo/scenario-test-map/v0.4.0";

/** The run-record contract family the reporter writes (plan S9). */
export const RUN_RECORD_IDENTITY = "dev.lekalo.scenario-run@0.4.0";
export const RUN_RECORD_SCHEMA_VERSION = "lekalo/scenario-run/v0.4.0";

/** The import-home directory of every emitted file. */
export const SCENARIO_DIR = "src/generated/node-typescript/scenario-tests";

/** The run-record ingest home (an adjudicated `.lekalo/import` home). */
export const RUN_RECORD_DIR = ".lekalo/import/scenario-runs";

/** Reserved emitted module names; a scenario module may never collide. */
export const RESERVED_MODULES = Object.freeze(["testkit", "port", "reporter"]);

/**
 * Emit every generated file of one mapped scenario document.
 *
 * `input` is `{ models, inputDigest, adapterVersion, portModulePath,
 * profile, startedBy }`: `models` is the mapper output array, and
 * `portModulePath` is the project-relative port module path from the
 * test-port declaration. Returns sorted `{path, text, map}` records; the
 * `map` member is non-null only on sidecars.
 */
export function emitScenarioTests(input) {
  const context = {
    inputDigest: input.inputDigest,
    adapterVersion: input.adapterVersion,
    portModulePath: input.portModulePath,
    profile: input.profile ?? null,
    startedBy: input.startedBy ?? "lekalo-scenario-harness",
  };
  const files = [
    file(`${SCENARIO_DIR}/testkit.ts`, testkitText(context)),
    file(`${SCENARIO_DIR}/reporter.mjs`, reporterText(context)),
    file(`${SCENARIO_DIR}/port.ts`, portText(context)),
  ];
  for (const model of orderedModels(input.models)) {
    const module = moduleOf(model.id);
    if (RESERVED_MODULES.includes(module)) {
      throw new TypeError(`scenario module collides with a reserved emitted file: ${module}`);
    }
    const testFile = emitTest(model, context);
    files.push(file(`${SCENARIO_DIR}/${module}/${model.id}.test.ts`, testFile.text));
    files.push(
      file(
        `${SCENARIO_DIR}/${module}/${model.id}.map.json`,
        `${canonicalJson(testFile.map)}\n`,
        testFile.map,
      ),
    );
  }
  files.sort((left, right) => (left.path < right.path ? -1 : left.path > right.path ? 1 : 0));
  return files;
}

function file(path, text, map = null) {
  return { path, text, map };
}

/** Scenario files ordered by id byte order — deterministic emission. */
function orderedModels(models) {
  return [...models].sort((left, right) => (left.id < right.id ? -1 : left.id > right.id ? 1 : 0));
}

/** The emission module of one scenario: the id prefix before the first dot. */
export function moduleOf(scenarioId) {
  const cut = scenarioId.indexOf(".");
  return cut <= 0 ? scenarioId : scenarioId.slice(0, cut);
}

/** The safe TypeScript identifier of one scenario or step id. */
export function identifierOf(id) {
  const sanitized = String(id).replace(/[^a-zA-Z0-9_]/g, "_");
  return /^[0-9]/.test(sanitized) ? `_${sanitized}` : sanitized;
}

/**
 * One comment-safe single-line projection of free wire text (issue #47
 * fix F-1): every line terminator and control character collapses, so a
 * core-valid `summary` can never close a generated comment and inject
 * live code into the emitted test. Bounded like every echoed wire
 * string; the full text stays in the sidecar metadata.
 */
export function commentSafe(text) {
  return String(text ?? "")
    .replace(/\r\n|[\r\n\u0085\u2028\u2029]|\p{Cc}/gu, " ")
    .replace(/\s+/g, " "
    )
    .trim()
    .slice(0, 200);
}

// ---------------------------------------------------------------------------
// Shared emitted helpers.
// ---------------------------------------------------------------------------

function docHeader(context, note) {
  return [
    `// Generated by ${ADAPTER_ID}@${context.adapterVersion} (scenario-test-compiler).`,
    `// From dev.lekalo.scenario-ir@0.2.16 input ${context.inputDigest}${note}.`,
    `// Do not edit: regenerate with \`lekalo generate\`. The runner is`,
    `// profile-declared and the binding pins it per scenario.`,
  ];
}

function testkitText(context) {
  // The testkit is written in the JS-strict subset of TypeScript (JSDoc
  // types only): directly executable under Node type stripping and
  // typechecked as part of the release gate.
  return `// Generated by ${ADAPTER_ID}@${context.adapterVersion} (scenario-test-compiler).
// Shared runner-neutral helpers; content depends only on the adapter
// version, so this file is itself a determinism probe. Generated file —
// do not edit.

/**
 * Canonical typed equality over the closed Scenario IR value domain:
 * dates, datetimes, uuids, uris, and decimals compare exactly as their
 * canonical strings; integers compare numerically across number and
 * bigint spellings; objects compare field-by-field in any key order.
 *
 * @param actual {unknown}
 * @param expected {unknown}
 * @returns {boolean}
 */
export function typedEqual(actual, expected) {
  if (actual === expected) return true;
  if (typeof actual === "bigint" || typeof expected === "bigint") {
    try {
      return BigInt(/** @type {any} */ (actual)) === BigInt(/** @type {any} */ (expected));
    } catch {
      return false;
    }
  }
  if (actual === null || expected === null) return false;
  if (typeof actual !== "object" || typeof expected !== "object") return false;
  if (Array.isArray(actual) || Array.isArray(expected)) {
    if (!Array.isArray(actual) || !Array.isArray(expected)) return false;
    return (
      actual.length === expected.length
      && actual.every((item, index) => typedEqual(item, expected[index]))
    );
  }
  const leftKeys = Object.keys(actual).sort();
  const rightKeys = Object.keys(expected).sort();
  if (leftKeys.length !== rightKeys.length) return false;
  if (!leftKeys.every((key, index) => key === rightKeys[index])) return false;
  return leftKeys.every((key) =>
    typedEqual(
      /** @type {any} */ (actual)[key],
      /** @type {any} */ (expected)[key],
    ));
}

/**
 * The public subset of one error object: the id plus the declared
 * payload fields only — a generated test never asserts private error
 * internals.
 *
 * @param error {{ id?: string, fields?: Record<string, unknown> | null }}
 * @param fields {Record<string, unknown>}
 * @returns {boolean}
 */
export function errorFieldsMatch(error, fields) {
  if (error === null || typeof error !== "object") return false;
  for (const [key, expected] of Object.entries(fields)) {
    const carried = /** @type {any} */ (error).fields?.[key];
    if (!typedEqual(carried, expected)) return false;
  }
  return true;
}

/**
 * One bounded, control-cleaned failure detail: the run record carries no
 * absolute paths, no host data, and never more than one short line.
 *
 * @param value {unknown}
 * @returns {string}
 */
export function boundedDetail(value) {
  return String(value ?? "unknown")
    .replace(/[^a-zA-Z0-9._:/() -]+/g, "?")
    .trim()
    .slice(0, 200);
}
`;
}

function reporterText(context) {
  // Plain JavaScript: the reporter writes the durable run record without
  // depending on type stripping. It never embeds absolute paths in the
  // record content — the project root is derived from this file's own
  // fixed location under the generated root, never from the cwd.
  return `// Generated by ${ADAPTER_ID}@${context.adapterVersion} (scenario-test-compiler).
// The durable run-record writer (issue #47 plan S7/S9): one
// ${RUN_RECORD_SCHEMA_VERSION} document per scenario run, written into the
// adjudicated ingest home ${RUN_RECORD_DIR}/. Generated file — do not edit.
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

// The recorder receives the IMPORTING TEST FILE's import.meta.url, so
// the root is one level per SCENARIO_DIR segment up from <module>/<file>.
const GENERATED_ROOT_DEPTH = 5;

/** Canonical compact JSON: keys sorted at every nesting level. */
function canonical(value) {
  if (value === null) return "null";
  if (Array.isArray(value)) return "[" + value.map(canonical).join(",") + "]";
  if (typeof value === "object") {
    const keys = Object.keys(value).sort();
    return "{" + keys.map((key) => JSON.stringify(key) + ":" + canonical(value[key])).join(",") + "}";
  }
  return JSON.stringify(value);
}

/**
 * The project root derived from this file's fixed emitted location —
 * never from the current working directory, never from host config.
 */
export function projectRootOf(importMetaUrl) {
  const here = dirname(fileURLToPath(importMetaUrl));
  let root = here;
  for (let index = 0; index < GENERATED_ROOT_DEPTH; index += 1) {
    root = dirname(root);
  }
  return root;
}

/**
 * Create one run recorder for one scenario test. Spec members: scenario
 * {id, version, irDigest, symbols, operations}, runner {id, version},
 * profile {id, version, digest} or null, test {id}, bindingMode,
 * startedBy. The test fingerprint is computed over the exact bytes of
 * the importing test file at flush time.
 */
export function createRunRecorder(spec, importMetaUrl) {
  const assertions = [];
  return {
    /**
     * Record exactly one assertion outcome row. Outcomes are closed:
     * pass | fail | unsupported | infrastructure | degraded. An
     * unsupported row can never become a pass.
     */
    record(row) {
      const outcome = String(row.outcome);
      if (!["pass", "fail", "unsupported", "infrastructure", "degraded"].includes(outcome)) {
        throw new TypeError("closed outcome vocabulary violation: " + outcome);
      }
      const entry = {
        step_id: row.step_id === null ? null : String(row.step_id),
        observes: row.observes === null ? null : String(row.observes),
        kind: String(row.kind),
        outcome,
      };
      if (row.detail !== undefined && row.detail !== null) {
        entry.detail = String(row.detail).slice(0, 200);
      }
      assertions.push(entry);
    },
    /** Whether any row is unsupported (such a run is never a pass). */
    hasUnsupported() {
      return assertions.some((row) => row.outcome === "unsupported");
    },
    /** Whether any row failed on an assertion or on infrastructure. */
    hasBlockingFailure() {
      return assertions.some(
        (row) => row.outcome === "fail" || row.outcome === "infrastructure",
      );
    },
    /** Persist the run record into the ingest home; returns its path. */
    flush() {
      const testFilePath = fileURLToPath(importMetaUrl);
      const root = projectRootOf(importMetaUrl);
      const fingerprint = "sha256:"
        + createHash("sha256").update(readFileSync(testFilePath)).digest("hex");
      const document = {
        schema_version: "${RUN_RECORD_SCHEMA_VERSION}",
        identity: "${RUN_RECORD_IDENTITY}",
        scenario: {
          id: String(spec.scenario.id),
          version: String(spec.scenario.version),
          ir_digest: String(spec.scenario.irDigest),
          symbols: spec.scenario.symbols ?? [],
          operations: spec.scenario.operations ?? [],
        },
        runner: { id: String(spec.runner.id), version: String(spec.runner.version) },
        profile: spec.profile
          ? {
              id: String(spec.profile.id),
              version: String(spec.profile.version),
              digest: String(spec.profile.digest),
            }
          : null,
        test: {
          id: String(spec.test.id),
          path: relative(root, testFilePath).split("\\\\").join("/"),
          fingerprint,
        },
        binding_mode: String(spec.bindingMode),
        started_by: String(spec.startedBy),
        assertions,
      };
      const target = join(root, "${RUN_RECORD_DIR}", String(spec.scenario.id) + ".json");
      mkdirSync(dirname(target), { recursive: true });
      writeFileSync(target, canonical(document) + "\\n");
      return target;
    },
  };
}
`;
}

function portText(context) {
  // The relative ESM path from the generated scenario-test home to the
  // project-declared port module. The generated tests import the port
  // exclusively through this shim, so no test file ever knows where the
  // port lives (plan §4). The shim is scenario-independent: its content
  // depends only on the adapter version and the declared port path, so
  // per-scenario generations never drift against each other.
  const depth = SCENARIO_DIR.split("/").length; // src + generated + node-typescript + scenario-tests
  const ups = Array.from({ length: depth }, () => "..").join("/");
  const specifier = `${ups}/${context.portModulePath}`;
  return `// Generated by ${ADAPTER_ID}@${context.adapterVersion} (scenario-test-compiler).
// The project test-port binding shim; content depends only on the
// adapter version and the declared port path, so this file is itself a
// determinism probe. Generated file — do not edit.

// @ts-expect-error the project port module is plain JavaScript by contract
export { port, resetPort } from "${specifier}";
`;
}

// ---------------------------------------------------------------------------
// Per-scenario test rendering.
// ---------------------------------------------------------------------------

function emitTest(model, context) {
  const testSymbol = `lekalo_${identifierOf(model.id)}`;
  const segments = [];
  let cursor = 0;
  const push = (text, stepId = null) => {
    segments.push({ text, start: cursor, stepId });
    cursor += byteLength(text);
  };
  const runnerVersion = model.runner.declaredVersion ?? null;
  push(`${docHeader(context, "").join("\n")}
//
// Scenario ${model.id} @${model.version}: ${commentSafe(model.summary)}
// Runner ${model.runner.id}; binding ${model.binding.mode}; generated by
// the scenario-test compiler (issue #47).

import assert from "node:assert/strict";
import { test } from "node:test";
import { port, resetPort } from "../port.ts";
import {
  boundedDetail,
  errorFieldsMatch,
  typedEqual,
} from "../testkit.ts";
import { createRunRecorder } from "../reporter.mjs";

export const ${testSymbol} = { scenario: ${JSON.stringify(model.id)} };

`);
  const blockStart = cursor;
  push(`test("lekalo:${model.id}", async (t) => {
  const recorder = createRunRecorder({
    scenario: {
      id: ${JSON.stringify(model.id)},
      version: ${JSON.stringify(model.version)},
      irDigest: ${JSON.stringify(model.irDigest ?? null)},
      symbols: ${JSON.stringify(model.symbols ?? [])},
      operations: ${JSON.stringify(operationsOf(model))},
    },
    runner: {
      id: ${JSON.stringify(model.runner.id)},
      version: ${runnerVersion === null ? "process.versions.node" : JSON.stringify(runnerVersion)},
    },
    profile: ${JSON.stringify(context.profile)},
    test: { id: ${JSON.stringify(model.binding.test ?? model.id)} },
    bindingMode: ${JSON.stringify(model.binding.mode)},
    startedBy: ${JSON.stringify(context.startedBy)},
  }, import.meta.url);
  try {
    await resetPort();
`);
  for (const group of renderBody(model)) {
    push(group.lines.join("\n") + "\n", group.stepId);
  }
  push(`    recorder.flush();
    if (recorder.hasUnsupported()) {
      // Unsupported rows never become passes: the runner reports this
      // test as skipped, and the record carries the exact rows.
      t.skip("scenario.unsupported-capability");
    }
  } catch (error) {
    recorder.flush();
    throw error;
  }
});
`);
  const text = segments.map((segment) => segment.text).join("");
  return {
    text,
    map: {
      contract: MAP_CONTRACT,
      adapter: { id: ADAPTER_ID, version: context.adapterVersion },
      owner: model.id,
      fields: { "": model.id },
      declarations: [
        { id: `scenario:${model.id}`, export: testSymbol, start: blockStart, end: byteLength(text) },
        ...segments
          .filter((segment) => segment.stepId !== null)
          .map((segment) => ({
            id: `then:${segment.stepId}`,
            export: testSymbol,
            start: segment.start,
            end: segment.start + byteLength(segment.text),
          })),
      ],
    },
  };
}

function operationsOf(model) {
  const operations = [];
  for (const step of model.when) {
    if (step.operation && !operations.includes(step.operation.id)) {
      operations.push(step.operation.id);
    }
  }
  return operations.sort();
}

/** The test body as tagged segments: given, when, then, in scenario order. */
function renderBody(model) {
  const groups = [];
  const wholeScenarioUnsupported = model.unsupported.length > 0;
  const stepVars = new Map();
  const clockIsos = new Map();
  if (wholeScenarioUnsupported) {
    // Concurrency race cases and binding-capability gaps: record the
    // declared rows and execute nothing (a serial run would lie).
    const lines = [];
    for (const entry of model.unsupported) {
      lines.push(unsupportedRow(null, null, "scenario", `${entry.capability}: ${entry.reason}`));
    }
    for (const step of model.then) {
      lines.push(unsupportedRow(step.stepId, step.observes, step.kind, "scenario-unsupported"));
    }
    groups.push({ lines, stepId: null });
    return groups;
  }
  for (const step of model.given) {
    if (step.unsupported) {
      groups.push({
        lines: [unsupportedRow(step.stepId, null, `given:${step.kind}`, `${step.unsupported.capability}: ${step.unsupported.reason}`)],
        stepId: null,
      });
      continue;
    }
    groups.push({
      lines: [`    // given ${step.stepId} (${step.kind})`, ...renderGiven(step, stepVars, clockIsos)],
      stepId: null,
    });
  }
  for (const step of model.when) {
    if (step.unsupported) {
      groups.push({
        lines: [unsupportedRow(step.stepId, null, "when", `${step.unsupported.capability}: ${step.unsupported.reason}`)],
        stepId: null,
      });
      continue;
    }
    groups.push({
      lines: [`    // when ${step.stepId} (${step.operation.kind} ${step.operation.id})`, ...renderWhen(step, stepVars)],
      stepId: null,
    });
  }
  for (const step of model.then) {
    groups.push({ lines: renderThen(step, model, stepVars, clockIsos), stepId: step.stepId });
  }
  return groups;
}

function unsupportedRow(stepId, observes, kind, detail) {
  return `    recorder.record({ step_id: ${JSON.stringify(stepId)}, observes: ${JSON.stringify(observes)},`
    + ` kind: ${JSON.stringify(kind)}, outcome: "unsupported", detail: boundedDetail(${JSON.stringify(detail)}) });`;
}

function renderGiven(step, stepVars, clockIsos) {
  const variable = `given_${identifierOf(step.stepId)}`;
  stepVars.set(step.stepId, variable);
  const payload = step.payload ?? {};
  switch (step.kind) {
    case "state":
      return [
        `    const ${variable} = await port.state.seed(${JSON.stringify(payload.entity)},`,
        `      ${emitValue(selectorObject(payload.selector, stepVars))},`,
        `      ${emitValue(objectLiteral(payload.fields, stepVars))});`,
      ];
    case "fixture":
      return [
        `    await port.fixtures.load(${JSON.stringify(payload.fixture)},`,
        `      { version: ${JSON.stringify(payload.version)}, capabilities: ${JSON.stringify(payload.capabilities ?? [])} });`,
      ];
    case "actor":
      return payload.scope === null
        ? [`    const ${variable} = port.actor(${JSON.stringify(payload.actor)});`]
        : [`    const ${variable} = port.actor(${JSON.stringify(payload.actor)}, ${JSON.stringify(payload.scope)});`];
    case "clock":
      clockIsos.set(step.stepId, payload.at);
      return [`    port.clock.freeze(${JSON.stringify(payload.at)});`];
    case "id_source":
      return [
        `    port.ids.seed({ algorithm: ${JSON.stringify(payload.algorithm)}, seed: ${JSON.stringify(payload.seed)} });`,
      ];
    default:
      return [`    // unknown precondition kind ${step.kind}; nothing to establish`];
  }
}

function selectorObject(selector, stepVars) {
  const object = {};
  for (const term of selector ?? []) {
    object[term.field] = literalOf(term.equals, stepVars);
  }
  return object;
}

function objectLiteral(fieldEntries, stepVars) {
  const object = {};
  const entries = Array.isArray(fieldEntries)
    ? fieldEntries
    : Object.entries(fieldEntries ?? {});
  for (const [field, leaf] of entries) {
    object[field] = literalOf(leaf, stepVars);
  }
  return object;
}

function renderWhen(step, stepVars) {
  const variable = `step_${identifierOf(step.stepId)}`;
  stepVars.set(step.stepId, variable);
  const input = {};
  for (const entry of step.input) {
    input[entry.field] = literalOf(entry.leaf, stepVars);
  }
  const ctx = {};
  if (step.ctx.actor !== undefined) {
    const actorStep = step.ctx.actor.id;
    const bound = stepVars.get(actorStep);
    ctx.actor = bound ? { __stepVar: bound } : actorStep;
  }
  if (step.ctx.clock !== undefined) {
    ctx.clock = clockIsos.get(step.ctx.clock.id) ?? null;
  }
  if (step.ctx.idempotencyKey !== undefined) {
    ctx.idempotencyKey = literalOf(step.ctx.idempotencyKey, stepVars);
  }
  return [
    `    let ${variable};`,
    `    try {`,
    `      ${variable} = await port.invoke(${JSON.stringify(step.operation.id)},`,
    `        ${emitValue(input)},`,
    `        ${emitValue(ctx)});`,
    `    } catch (error) {`,
    `      recorder.record({ step_id: ${JSON.stringify(step.stepId)}, observes: null, kind: "when",`
      + ` outcome: "infrastructure", detail: boundedDetail(error?.message) });`,
    `      throw error;`,
    `    }`,
  ];
}

function renderThen(step, model, stepVars, clockIsos) {
  const observed = `step_${identifierOf(step.observes)}`;
  const meta = `step_id: ${JSON.stringify(step.stepId)}, observes: ${JSON.stringify(step.observes)}, kind: ${JSON.stringify(step.kind)}`;
  if (step.unsupported) {
    return [unsupportedRow(step.stepId, step.observes, step.kind, `${step.unsupported.capability}: ${step.unsupported.reason}`)];
  }
  const checks = renderChecks(step, model, stepVars, clockIsos, observed);
  return [
    `    // then ${step.stepId}: ${step.kind} over ${step.observes}`,
    `    try {`,
    ...checks.map((line) => `      ${line}`),
    `      recorder.record({ ${meta}, outcome: "pass" });`,
    `    } catch (error) {`,
    `      recorder.record({ ${meta}, outcome: error instanceof assert.AssertionError ? "fail" : "infrastructure",`
      + ` detail: boundedDetail(error?.message) });`,
    `      throw error;`,
    `    }`,
  ];
}

function renderChecks(step, model, stepVars, clockIsos, observed) {
  const payload = step.payload ?? {};
  switch (step.kind) {
    case "result": {
      const checks = [
        `assert.equal(${observed}.ok, true, boundedDetail(${observed}?.error?.id ?? "invoke-failed"));`,
      ];
      if (payload.value !== undefined) {
        checks.push(
          `assert.ok(typedEqual(${observed}.value, ${emitValue(literalOf(payload.value, stepVars))}), "result-value");`,
        );
      }
      return checks;
    }
    case "error":
      return [
        `assert.equal(${observed}.ok, false, "expected a typed error");`,
        `assert.equal(${observed}.error?.id, ${JSON.stringify(payload.error)}, "error-id");`,
        ...(payload.payload ?? [])
          .filter((entry) => !entry.leafProblem)
          .map((entry) =>
            `assert.ok(errorFieldsMatch(${observed}.error, { ${JSON.stringify(entry.field)}:`
              + ` ${emitValue(literalOf(entry.leaf, stepVars))} }), "error-fields");`),
      ];
    case "entity_state": {
      const selector = selectorObject(payload.where, stepVars);
      const exactFields = {};
      const matchFields = [];
      for (const [field, expectation] of Object.entries(payload.fields ?? {})) {
        if (expectation !== null && typeof expectation === "object" && "match" in expectation) {
          matchFields.push(field);
          continue;
        }
        exactFields[field] = literalOf(expectation?.value ?? expectation, stepVars);
      }
      const selectorText = emitValue(selector);
      const fieldsText = emitValue(exactFields);
      return [
        `const stateRows = await port.state.query(${JSON.stringify(payload.entity)}, ${selectorText});`,
        `assert.equal(stateRows.length, ${JSON.stringify(expectCount(payload.expect))}, "entity-count");`,
        ...(Object.keys(exactFields).length > 0
          ? [
              `assert.ok(stateRows.every((row) => typedEqual(`,
              `  Object.fromEntries(${JSON.stringify(Object.keys(exactFields))}.map((key) => [key, row[key]])),`,
              `  ${fieldsText})), "entity-fields");`,
            ]
          : []),
        ...matchFields.map((field) =>
          `assert.ok(stateRows.every((row) => row[${JSON.stringify(field)}] !== null), "entity-match:${field}");`),
      ];
    }
    case "emitted": {
      const target = payload.target ?? {};
      const count = payload.count ? emitCount(payload.count) : null;
      return [
        `const emissions = port.emissions().filter((entry) => entry.id === ${JSON.stringify(target.id)} && entry.kind === ${JSON.stringify(target.kind)});`,
        ...(count === null
          ? [`assert.ok(emissions.length >= 1, "emitted-at-least-one");`]
          : [`assert.ok(emissions.length ${count.operator} ${count.value}, "emitted-count");`]),
      ];
    }
    case "forbidden_effect":
      return [
        `const matching = port.effects().filter((entry) => entry.effect === ${JSON.stringify(payload.effect)}` +
          (payload.field ? ` && entry.field === ${JSON.stringify(payload.field)}` : ``) + `);`,
        `assert.equal(matching.length, 0, "forbidden-effect");`,
      ];
    case "authorization":
      return [
        `const decision = await port.authorize(${JSON.stringify(payload.actor?.id ?? null)},`
          + ` ${JSON.stringify(payload.policy)}, ${JSON.stringify(observesOperation(model, step))});`,
        `assert.equal(decision, ${JSON.stringify(payload.outcome)}, "authorization-outcome");`,
      ];
    case "idempotency": {
      const original = `step_${identifierOf(payload.replay ?? "")}`;
      const originalStep = model.when.find((candidate) => candidate.stepId === payload.replay);
      const checks = [
        `assert.ok(typedEqual(${observed}, ${original}), "replay-equivalence");`,
      ];
      if (payload.duplicates === "none" && originalStep?.operation) {
        checks.push(
          `assert.ok(port.emissions().filter((entry) => entry.operation === ${JSON.stringify(originalStep.operation.id)}).length <= 1,`,
          `  "duplicates-none");`,
        );
      }
      return checks;
    }
    case "contract_match":
      return [
        `const projection = ${JSON.stringify(payload.projection ?? [])};`,
        `const actual = ${observed}?.value ?? null;`,
        `assert.equal(await port.contractCheck(${JSON.stringify(payload.contract)}, projection, actual), true, "contract-match");`,
      ];
    case "deterministic_fixture": {
      const fixtureStep = model.given.find((candidate) => candidate.kind === "fixture");
      if (!fixtureStep) {
        return [`assert.fail("deterministic_fixture without a fixture precondition");`];
      }
      return [
        `assert.equal(await port.fixtureDigest(${JSON.stringify(fixtureStep.payload?.fixture ?? null)}),`
          + ` ${JSON.stringify(payload.digest)}, "fixture-digest");`,
      ];
    }
    default:
      return [`assert.fail(${JSON.stringify(`unrenderable assertion kind ${step.kind}`)});`];
  }
}

function observesOperation(model, step) {
  const observed = model.when.find((candidate) => candidate.stepId === step.observes);
  return observed?.operation?.id ?? null;
}

function expectCount(expect) {
  if (expect !== null && typeof expect === "object") {
    if ("count" in expect) return expect.count;
    if (expect.presence === "missing") return 0;
    if (expect.presence === "exists") return 1;
  }
  if (typeof expect === "number") return expect;
  return expect;
}

function emitCount(count) {
  if (count !== null && typeof count === "object") {
    if (count.exactly !== undefined) return { operator: "===", value: count.exactly };
    if (count.atLeast !== undefined) return { operator: ">=", value: count.atLeast };
  }
  return { operator: ">=", value: 1 };
}

// ---------------------------------------------------------------------------
// Literal rendering of closed typed values and references.
// ---------------------------------------------------------------------------

/**
 * Render one typed leaf (value or reference) into its emitted argument.
 * `step-output` and `given-value` references become the emitted const
 * bindings of their steps; every other reference kind compiles to its
 * identity string (a port-call argument, never guessed code).
 */
export function literalOf(leaf, stepVars = new Map()) {
  if (leaf === null || typeof leaf !== "object") return null;
  if (typeof leaf.$ref === "string") {
    if ((leaf.$ref === "step-output" || leaf.$ref === "given-value") && typeof leaf.id === "string") {
      const bound = stepVars.get(leaf.id);
      if (bound) return { __stepVar: bound };
    }
    return `$ref:${leaf.$ref}:${leaf.id ?? null}`;
  }
  switch (leaf.type) {
    case "null":
      return null;
    case "boolean":
    case "string":
    case "decimal":
    case "date":
    case "datetime":
    case "uuid":
    case "uri":
      return leaf.value;
    case "integer": {
      // Integers inside the safe range stay number literals; anything
      // wider becomes a BigInt literal at emission (plan §2.5).
      const numeric = typeof leaf.value === "string" ? Number(leaf.value) : leaf.value;
      return Number.isSafeInteger(numeric) ? numeric : { __bigint: String(leaf.value) };
    }
    case "list":
      return (leaf.value ?? []).map((item) => literalOf(item, stepVars));
    case "object": {
      const object = {};
      for (const [key, value] of Object.entries(leaf.value ?? {})) {
        object[key] = literalOf(value, stepVars);
      }
      return object;
    }
    default:
      throw new TypeError(`unrenderable leaf kind ${leaf.type}`);
  }
}

/**
 * Render one literalOf output into its exact emitted JavaScript text:
 * step-variable markers become the raw const bindings, wide integers
 * become BigInt literals, everything else is JSON-stringified (no
 * escaping drift).
 */
export function emitValue(value) {
  if (value === null) return "null";
  if (Array.isArray(value)) return `[${value.map(emitValue).join(", ")}]`;
  if (typeof value === "object") {
    if (value.__stepVar !== undefined) return value.__stepVar;
    if (value.__bigint !== undefined) return `${value.__bigint}n`;
    const members = Object.entries(value)
      .map(([key, member]) => `${JSON.stringify(key)}: ${emitValue(member)}`)
      .join(", ");
      return `{ ${members} }`;
  }
  return JSON.stringify(value);
}

function byteLength(text) {
  return Buffer.byteLength(text, "utf8");
}
