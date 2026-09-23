/**
 * Pure Scenario IR → test-AST mapping for the scenario-test compiler
 * (issue #47, plan S3). Mirrors the zod-map/zod-emit split of issue #45.
 *
 * Inputs are one parsed Scenario IR document (`dev.lekalo.scenario-ir@0.2.16`),
 * one parsed compiled project IR document (`dev.lekalo.ir@0.2.16`), one
 * parsed project test-port declaration (`lekalo/test-port.json`), and the
 * profile's negotiated capability snapshot. The output is a closed test
 * AST plus typed findings. No filesystem, clock, environment, process, or
 * network access happens here, so the same inputs always map to the same
 * AST — the determinism contract the byte-stable emitter depends on.
 *
 * Honesty rules (plan §2, §12):
 * - Every scenario feature without a port surface, runner capability, or
 *   resolvable target lands in `unsupported[]` with its diagnostic — never
 *   silently dropped, never approximated, never a pass.
 * - Operation references resolve to `command` or `query` from the compiled
 *   IR, never from the name; anything else is a `scenario.operation-unresolved`
 *   compile-time finding.
 * - Concurrency race scenarios (metadata `testing.concurrency`) map to an
 *   explicit whole-scenario unsupported outcome: a serial execution never
 *   satisfies a race fixture.
 * - `unsupported` assertion kinds compile to recorded unsupported rows and
 *   can never report pass.
 */

import { canonicalJson } from "./zod-emit.mjs";

/** The exact accepted Scenario IR contract identity. */
export const SCENARIO_IDENTITY = "dev.lekalo.scenario-ir@0.2.16";

/** The exact accepted compiled IR contract identity. */
export const IR_IDENTITY = "dev.lekalo.ir@0.2.16";

/** The generated scenario-test home under the generated root. */
export const SCENARIO_TESTS_DIR = "src/generated/node-typescript/scenario-tests";

/** The project test-port declaration path (issue #47, plan S2). */
export const PORT_DOC_PATH = "lekalo/test-port.json";

/** The compile-time finding code: an operation ref resolves to neither kind. */
export const OPERATION_UNRESOLVED = "scenario.operation-unresolved";

/** The compile-time finding code: the irRef digest disagrees with the evidence. */
export const IR_REF_MISMATCH = "scenario.ir-ref-mismatch";

/** The compile-time finding code: the port document is absent. */
export const PORT_MISSING = "scenario.port-missing";

/** The compile-time finding code: the port module shape is unusable. */
export const PORT_SHAPE = "scenario.port-shape";

/** The compile-time finding code: the binding names an unknown runner. */
export const RUNNER_UNKNOWN = "scenario.runner-unknown";

/** The compile-time finding code: a capability gap compiles to unsupported. */
export const UNSUPPORTED_CAPABILITY = "scenario.unsupported-capability";

/** The irRef cross-check finding bounds. */
export const MAX_SCENARIOS_PER_DOCUMENT = 1;

/** IR bounds mirrored from the core (`scenario/version.rs`), plan §2. */
export const LIMITS = Object.freeze({
  maxGivenSteps: 256,
  maxWhenSteps: 256,
  maxThenSteps: 512,
  maxTotalSteps: 1024,
  maxRefsPerStep: 128,
  maxBindings: 32,
  maxTypedDepth: 32,
  maxTypedItems: 4096,
  maxScalarCodepoints: 4096,
});

/**
 * The closed runner registry (plan §3): profile-declared runner ids map to
 * the syntax and capability facts the compiler needs. The capabilities
 * mirror the `node-native` testing component's provides (issue #47 S1);
 * `testing.concurrency` is deliberately absent — the serial `node:test`
 * harness has no deterministic race scheduler, so race cases compile to
 * explicit unsupported outcomes.
 */
export const RUNNER_REGISTRY = Object.freeze({
  "node:test": Object.freeze({
    capabilities: Object.freeze([
      "testing.clock",
      "testing.coverage",
      "testing.event-capture",
      "testing.fixtures",
      "testing.parallel",
    ]),
    concurrency: false,
    eventCapture: "partial",
    reporter: "reporter.mjs",
    syntax: "node-test",
  }),
});

/** The default runner used when a scenario declares no native binding. */
export const DEFAULT_RUNNER = "node:test";

/**
 * Binding capabilities the port's `invoke` surface itself provides when the
 * dispatch enforces idempotency dedup: they are resolvable for bindings even
 * though no testing component provides them.
 */
const PORT_PROVIDED_CAPABILITIES = Object.freeze([
  "idempotency.durable_key",
  "idempotency.replay",
]);

/** The scenario metadata key whose presence marks a concurrency race case. */
export const CONCURRENCY_METADATA_KEY = "testing.concurrency";

/** The closed given-step precondition kinds. */
const PRECONDITION_KINDS = Object.freeze([
  "state", "fixture", "actor", "clock", "id_source",
]);

/** The closed assertion kinds (plan §2.4). */
const ASSERTION_KINDS = Object.freeze([
  "result", "error", "entity_state", "emitted", "forbidden_effect",
  "authorization", "idempotency", "contract_match", "deterministic_fixture",
  "unsupported",
]);

/** The closed typed-value wire kinds (`scenario/value.rs`). */
const VALUE_KINDS = Object.freeze([
  "null", "boolean", "integer", "string", "decimal", "date", "datetime",
  "uuid", "uri", "list", "object",
]);

/** The closed reference kinds (`scenario/reference.rs`). */
const REF_KINDS = Object.freeze([
  "symbol", "operation", "entity", "field", "event", "job", "effect",
  "error", "requirement", "fixture", "actor", "clock", "id-source",
  "step-output", "given-value",
]);

/**
 * Map one scenario document plus its joined context into the test AST.
 *
 * `input` is `{ scenario, ir, port, portPresent, profileCapabilities }`:
 * `scenario` is the parsed Scenario IR document, `ir` the parsed compiled
 * project IR evidence, `port` the parsed test-port declaration (null when
 * `portPresent` is false), and `profileCapabilities` the negotiated
 * `[{id, support}]` snapshot from the request envelope (null when the
 * launch carries no resolution). Returns the closed mapper outcome.
 */
export function mapScenario(input) {
  const scenario = input.scenario;
  const context = {
    findings: [],
    scenarioId: typeof scenario?.scenarioId === "string" ? scenario.scenarioId : undefined,
    operationIndex: operationIndex(input.ir),
  };
  const shapeRefusal = checkScenarioShape(scenario);
  if (shapeRefusal) {
    return { state: "refused", refusal: shapeRefusal, scenarios: [], findings: [] };
  }
  const irRefOk = input.ir !== null
    && input.ir.contract === IR_IDENTITY
    && scenario.irRef !== undefined
    && scenario.irRef.digest === input.irDigest;
  if (!irRefOk) {
    context.findings.push({
      code: IR_REF_MISMATCH,
      symbol: scenario.scenarioId,
      detail: "ir-ref-digest",
    });
  }
  const runner = resolveRunner(scenario, context);
  const portSurface = resolvePortSurface(input, context);
  const unsupported = [];
  collectConcurrencyUnsupported(scenario, input, runner, unsupported);
  collectCapabilityGaps(scenario, input, runner, unsupported);
  const model = {
    id: scenario.scenarioId,
    version: scenario.scenarioVersion,
    summary: scenario.summary,
    projectId: scenario.projectId,
    irDigest: scenario.irRef?.digest ?? null,
    runner,
    binding: bindingModel(scenario),
    tags: scenario.tags ?? [],
    unsupported,
    given: mapGiven(scenario.given ?? [], portSurface),
    when: mapWhen(scenario.when ?? [], context, portSurface),
    then: mapThen(scenario.then ?? [], context, portSurface),
  };
  return {
    state: "mapped",
    scenarios: [model],
    findings: context.findings,
  };
}

// ---------------------------------------------------------------------------
// Closed-shape decoding (defense in depth: the core validates first; the
// adapter refuses malformed wires before any mapping happens).
// ---------------------------------------------------------------------------

/**
 * The closed SemanticId grammar mirrored from the core
 * (`scenario/id.rs::SemanticId`): two or three dot-separated lowercase
 * segments (`[a-z][a-z0-9_]*`, ≤63 each), total ≤191 bytes, and the
 * first segment never the reserved `lekalo`/`dev`. Enforced at the
 * emission trust boundary (issue #47 fix F-1) — raw ids interpolate
 * into the emitted test name and emitted path.
 */
export function isSemanticId(text) {
  if (typeof text !== "string" || text.length === 0 || text.length > 191) {
    return false;
  }
  const segments = text.split(".");
  if (segments.length < 2 || segments.length > 3) {
    return false;
  }
  return segments.every((segment, index) => {
    if (segment.length === 0 || segment.length > 63) return false;
    if (!/^[a-z]/.test(segment)) return false;
    if (!/^[a-z0-9_]*$/.test(segment)) return false;
    if (index === 0 && (segment === "lekalo" || segment === "dev")) return false;
    return true;
  });
}

/** The closed single-segment step-id grammar (`scenario/id.rs::StepId`). */
export function isStepId(text) {
  return (
    typeof text === "string"
    && text.length > 0
    && text.length <= 64
    && /^[a-z][a-z0-9_]*$/.test(text)
  );
}

function checkScenarioShape(scenario) {
  if (scenario === null || typeof scenario !== "object" || Array.isArray(scenario)) {
    return "scenario-shape";
  }
  if (scenario.schemaVersion !== "lekalo/scenario-ir/v0.2.16"
    || scenario.identity !== SCENARIO_IDENTITY) {
    return "scenario-identity";
  }
  for (const key of [
    "projectId", "scenarioId", "scenarioVersion", "summary",
    "irRef", "modelRef", "given", "when", "then", "bindings", "tags", "metadata",
  ]) {
    if (!(key in scenario)) {
      return "scenario-missing-field";
    }
  }
  if (typeof scenario.scenarioId !== "string"
    || !isSemanticId(scenario.scenarioId)) {
    return "scenario-id";
  }
  if (!Array.isArray(scenario.given) || !Array.isArray(scenario.when)
    || !Array.isArray(scenario.then)) {
    return "scenario-steps-shape";
  }
  if (scenario.when.length === 0 || scenario.then.length === 0) {
    return "scenario-empty";
  }
  if (scenario.given.length > LIMITS.maxGivenSteps
    || scenario.when.length > LIMITS.maxWhenSteps
    || scenario.then.length > LIMITS.maxThenSteps
    || scenario.given.length + scenario.when.length + scenario.then.length
      > LIMITS.maxTotalSteps) {
    return "scenario-steps-bound";
  }
  if (!Array.isArray(scenario.bindings) || scenario.bindings.length > LIMITS.maxBindings) {
    return "scenario-bindings-bound";
  }
  const stepIds = new Set();
  for (const role of [scenario.given, scenario.when, scenario.then]) {
    for (const step of role) {
      if (step === null || typeof step !== "object"
        || typeof step.stepId !== "string" || !isStepId(step.stepId)) {
        return "scenario-step-id";
      }
      if (stepIds.has(step.stepId)) {
        return "scenario-duplicate-step-id";
      }
      stepIds.add(step.stepId);
    }
  }
  return null;
}

/** The wire-shape check for one typed value or reference leaf. */
function checkLeaf(leaf, depth) {
  if (leaf === null || typeof leaf !== "object" || Array.isArray(leaf)) {
    return "leaf-shape";
  }
  if (depth > LIMITS.maxTypedDepth) {
    return "leaf-depth";
  }
  if (typeof leaf.$ref === "string") {
    return REF_KINDS.includes(leaf.$ref) ? null : "leaf-ref-kind";
  }
  if (typeof leaf.type !== "string" || !VALUE_KINDS.includes(leaf.type)) {
    return "leaf-value-kind";
  }
  if (leaf.type === "list") {
    if (!Array.isArray(leaf.value) || leaf.value.length > LIMITS.maxTypedItems) {
      return "leaf-items";
    }
    for (const item of leaf.value) {
      const problem = checkLeaf(item, depth + 1);
      if (problem) return problem;
    }
    return null;
  }
  if (leaf.type === "object") {
    if (leaf.value === null || typeof leaf.value !== "object" || Array.isArray(leaf.value)) {
      return "leaf-object";
    }
    const keys = Object.keys(leaf.value);
    if (keys.length > LIMITS.maxTypedItems) {
      return "leaf-items";
    }
    for (const key of keys) {
      const problem = checkLeaf(leaf.value[key], depth + 1);
      if (problem) return problem;
    }
    return null;
  }
  if (!("value" in leaf)) {
    return "leaf-value";
  }
  if (typeof leaf.value === "string" && leaf.value.length > LIMITS.maxScalarCodepoints) {
    return "leaf-scalar";
  }
  return null;
}

// ---------------------------------------------------------------------------
// Resolution: operations, runners, port surfaces, capabilities.
// ---------------------------------------------------------------------------

/** The command/query index of the compiled IR evidence. */
function operationIndex(ir) {
  const index = new Map();
  if (ir === null || typeof ir !== "object" || !Array.isArray(ir.definitions)) {
    return index;
  }
  for (const definition of ir.definitions) {
    if (definition === null || typeof definition !== "object") continue;
    if (definition.kind === "command" || definition.kind === "query") {
      index.set(definition.id, definition.kind);
    }
  }
  return index;
}

/**
 * Resolve one scenario operation reference against the compiled IR. The
 * exact definition id wins; otherwise the kind-qualified wire spelling
 * (`<module>.command.<name>` / `<module>.query.<name>`) resolves when the
 * base id without the kind segment is declared with exactly that kind.
 * The resolved id is always the canonical IR identity; the original
 * reference rides along as `ref` when it differed. Anything else is
 * unresolved — never a guessed call kind.
 */
export function resolveOperation(index, id) {
  const exact = index.get(id);
  if (exact) return { id, kind: exact };
  const segments = id.split(".");
  if (segments.length >= 3) {
    const kind = segments[segments.length - 2];
    if (kind === "command" || kind === "query") {
      const base = [...segments.slice(0, -2), segments[segments.length - 1]].join(".");
      if (index.get(base) === kind) return { id: base, kind, ref: id };
    }
  }
  return null;
}

/** The resolved runner entry, or an unknown-runner finding with the default. */
function resolveRunner(scenario, context) {
  const nativeBinding = (scenario.bindings ?? []).find(
    (binding) => binding !== null && typeof binding === "object" && binding.backend === "native",
  );
  const runnerId = typeof nativeBinding?.runner === "string"
    ? nativeBinding.runner
    : DEFAULT_RUNNER;
  const entry = RUNNER_REGISTRY[runnerId];
  if (!entry) {
    context.findings.push({
      code: RUNNER_UNKNOWN,
      symbol: scenario.scenarioId,
      detail: boundToken(runnerId),
    });
    return { id: DEFAULT_RUNNER, ...RUNNER_REGISTRY[DEFAULT_RUNNER] };
  }
  return {
    id: runnerId,
    ...(typeof nativeBinding?.runnerVersion === "string"
      ? { declaredVersion: nativeBinding.runnerVersion }
      : {}),
    ...entry,
  };
}

/**
 * The port surface join: every closed port flag the project declares. A
 * missing declaration is a compile-time finding; generation then refuses
 * (the caller decides) and every port-backed feature maps to unsupported.
 */
function resolvePortSurface(input, context) {
  const port = input.port;
  if (!input.portPresent || port === null) {
    context.findings.push({ code: PORT_MISSING, detail: "declaration-absent" });
    return emptySurface();
  }
  const exports = port.port && typeof port.port === "object" ? port.port.exports : null;
  if (exports === null || typeof exports !== "object" || exports.invoke !== true) {
    context.findings.push({ code: PORT_SHAPE, detail: "exports-shape" });
    return emptySurface();
  }
  const surface = {};
  for (const flag of [
    "invoke", "state", "fixtures", "actor", "clock", "ids",
    "emissions", "effects", "authorize", "contractCheck", "fixtureDigest", "reset",
  ]) {
    surface[flag] = exports[flag] === true;
  }
  return surface;
}

function emptySurface() {
  const surface = {};
  for (const flag of [
    "invoke", "state", "fixtures", "actor", "clock", "ids",
    "emissions", "effects", "authorize", "contractCheck", "fixtureDigest", "reset",
  ]) {
    surface[flag] = false;
  }
  return surface;
}

/**
 * Concurrency race cases: a scenario that declares the concurrency
 * metadata compiles to one whole-scenario unsupported row (plan §2.8).
 * A serial execution never satisfies a race fixture, and the negotiated
 * profile never carries `testing.concurrency` on a serial runner.
 */
function collectConcurrencyUnsupported(scenario, input, runner, unsupported) {
  const metadata = scenario.metadata;
  if (metadata === null || typeof metadata !== "object") return;
  if (!(CONCURRENCY_METADATA_KEY in metadata)) return;
  if (runner.capabilities.includes("testing.concurrency")) return;
  unsupported.push({
    step: null,
    capability: "testing.concurrency",
    reason: "scenario-requires-concurrency",
    detail: boundToken(String(metadata[CONCURRENCY_METADATA_KEY])),
  });
}

/**
 * The binding capability join: every declared binding capability must be
 * resolvable from the runner registry entry, the port dispatch surface,
 * or the negotiated profile snapshot; each gap is one unsupported row.
 */
function collectCapabilityGaps(scenario, input, runner, unsupported) {
  const profile = new Set(
    (input.profileCapabilities ?? [])
      .filter((entry) => entry && entry.support !== undefined && entry.support !== "unsupported")
      .map((entry) => entry.id),
  );
  for (const binding of scenario.bindings ?? []) {
    if (binding === null || typeof binding !== "object") continue;
    if (binding.backend !== "native") continue; // fake-reference stays with #107
    for (const capability of Array.isArray(binding.capabilities) ? binding.capabilities : []) {
      if (typeof capability !== "string") continue;
      if (runner.capabilities.includes(capability)) continue;
      if (PORT_PROVIDED_CAPABILITIES.includes(capability)) continue;
      if (profile.size > 0 && !profile.has(capability)) {
        unsupported.push({
          step: null,
          capability,
          reason: "binding-capability-gap",
          detail: "profile-snapshot",
        });
        continue;
      }
      if (profile.size === 0) {
        unsupported.push({
          step: null,
          capability,
          reason: "binding-capability-gap",
          detail: "runner-registry",
        });
      }
    }
  }
}

/** The binding metadata of the generated test (native binding only). */
function bindingModel(scenario) {
  const nativeBinding = (scenario.bindings ?? []).find(
    (binding) => binding !== null && typeof binding === "object" && binding.backend === "native",
  );
  if (!nativeBinding) {
    return { mode: "generated", backend: "none", test: null, capabilityDigest: null };
  }
  return {
    mode: typeof nativeBinding.mode === "string" ? nativeBinding.mode : "generated",
    backend: "native",
    test: typeof nativeBinding.test === "string" ? nativeBinding.test : null,
    capabilityDigest: typeof nativeBinding.capabilityDigest === "string"
      ? nativeBinding.capabilityDigest
      : null,
  };
}

// ---------------------------------------------------------------------------
// Step mapping: given / when / then, each port-joined.
// ---------------------------------------------------------------------------

function mapGiven(given, portSurface) {
  const surfaceOf = {
    state: "state",
    fixture: "fixtures",
    actor: "actor",
    clock: "clock",
    id_source: "ids",
  };
  return given.map((step) => {
    const precondition = step.precondition ?? {};
    const kind = precondition.kind;
    const mapped = {
      stepId: step.stepId,
      kind: PRECONDITION_KINDS.includes(kind) ? kind : "unknown",
      port: null,
      unsupported: null,
    };
    const surface = surfaceOf[kind];
    if (surface === undefined) {
      mapped.unsupported = { capability: `scenario.precondition.${kind ?? "unknown"}`, reason: "precondition-kind-unknown" };
      return mapped;
    }
    if (!portSurface[surface]) {
      mapped.unsupported = {
        capability: `testing.${surface === "ids" ? "ids" : surface}`,
        reason: "port-surface-absent",
        detail: surface,
      };
      return mapped;
    }
    mapped.port = surface;
    mapped.payload = preconditionPayload(precondition);
    return mapped;
  });
}

function preconditionPayload(precondition) {
  switch (precondition.kind) {
    case "state":
      return {
        entity: precondition.entity,
        selector: (precondition.selector ?? []).map((term) => ({
          field: term.field,
          equals: term.equals,
        })),
        fields: Object.entries(precondition.fields ?? {}),
      };
    case "fixture":
      return {
        fixture: precondition.fixture,
        version: precondition.version,
        capabilities: precondition.capabilities ?? [],
      };
    case "actor":
      return { actor: precondition.actor, scope: precondition.scope ?? null };
    case "clock":
      return { at: precondition.at?.value ?? null };
    case "id_source":
      return { seed: precondition.seed, algorithm: precondition.algorithm };
    default:
      return {};
  }
}

function mapWhen(when, context, portSurface) {
  return when.map((step) => {
    const action = step.action ?? {};
    const mapped = {
      stepId: step.stepId,
      kind: "invoke",
      operation: null,
      input: [],
      ctx: {},
      replay: null,
      unsupported: null,
    };
    if (action.kind !== "invoke") {
      mapped.unsupported = { capability: "scenario.action", reason: "action-kind-unknown" };
      return mapped;
    }
    const operationId = action.operation;
    const operation = resolveOperation(context.operationIndex, operationId);
    if (!operation) {
      context.findings.push({
        code: OPERATION_UNRESOLVED,
        symbol: context.scenarioId ?? undefined,
        detail: boundToken(operationId),
      });
      mapped.unsupported = {
        capability: "scenario.operation",
        reason: "operation-unresolved",
        detail: boundToken(operationId),
      };
      return mapped;
    }
    mapped.operation = operation;
    mapped.input = Object.entries(action.input ?? {}).map(([field, leaf]) => ({
      field,
      leaf,
      leafProblem: checkLeaf(leaf, 0),
    }));
    if (action.actor !== undefined) mapped.ctx.actor = action.actor;
    if (action.clock !== undefined) mapped.ctx.clock = action.clock;
    if (action.idempotencyKey !== undefined) {
      mapped.ctx.idempotencyKey = action.idempotencyKey;
      const problem = checkLeaf(action.idempotencyKey, 0);
      if (problem) mapped.unsupported = { capability: "scenario.value", reason: problem };
    }
    if (!portSurface.invoke) {
      mapped.unsupported = { capability: "testing.fixtures", reason: "port-surface-absent", detail: "invoke" };
    }
    if (step.replay !== null && typeof step.replay === "object") {
      mapped.replay = {
        of: step.replay.of,
        expect: step.replay.expect,
      };
    }
    return mapped;
  });
}

function mapThen(then, context, portSurface) {
  const surfaceOf = {
    entity_state: "state",
    emitted: "emissions",
    forbidden_effect: "effects",
    authorization: "authorize",
    contract_match: "contractCheck",
    deterministic_fixture: "fixtureDigest",
  };
  return then.map((step) => {
    const assertion = step.assertion ?? {};
    const kind = assertion.kind;
    const mapped = {
      stepId: step.stepId,
      observes: step.observes,
      kind: ASSERTION_KINDS.includes(kind) ? kind : "unknown",
      port: null,
      unsupported: null,
      payload: {},
    };
    if (!ASSERTION_KINDS.includes(kind)) {
      mapped.unsupported = { capability: "scenario.assertion", reason: "assertion-kind-unknown" };
      return mapped;
    }
    if (kind === "unsupported") {
      // The explicit unsupported expectation: always a recorded
      // unsupported row carrying the capability ref — never a pass,
      // never a skip-as-pass (plan §2.4).
      mapped.unsupported = {
        capability: assertion.capability ?? "scenario.capability",
        reason: "declared-unsupported",
        detail: assertion.note === undefined ? null : boundToken(String(assertion.note)),
      };
      return mapped;
    }
    if (kind === "result") {
      mapped.payload.valueType = assertion.valueType;
      if (assertion.value !== undefined) {
        mapped.payload.value = assertion.value;
        const problem = checkLeaf(assertion.value, 0);
        if (problem) mapped.unsupported = { capability: "scenario.value", reason: problem };
      }
      return mapped;
    }
    if (kind === "error") {
      mapped.payload.error = assertion.error;
      mapped.payload.payload = Object.entries(assertion.payload ?? {}).map(([field, leaf]) => ({
        field,
        leaf,
        leafProblem: checkLeaf(leaf, 0),
      }));
      mapped.payload.contract = assertion.contract ?? null;
      if (mapped.payload.contract !== null && !portSurface.contractCheck) {
        // The contract half needs the port contract check; the typed
        // error identity and public-field subset stay supported.
        mapped.unsupported = {
          capability: "scenario.contract-check",
          reason: "port-surface-absent",
          detail: "contractCheck",
        };
      }
      return mapped;
    }
    if (kind === "idempotency") {
      // The weaker semantic-equivalence relation has no evaluator in v1;
      // emitting strict equality would over-constrain the replay
      // (review F-3) — an explicit unsupported row, never a proxy.
      if (assertion.equivalence === "equivalent") {
        mapped.unsupported = {
          capability: "scenario.equivalence-equivalent",
          reason: "equivalence-unimplemented",
          detail: "equivalent",
        };
        return mapped;
      }
      mapped.payload.replay = assertion.replay;
      mapped.payload.equivalence = assertion.equivalence;
      mapped.payload.duplicates = assertion.duplicates ?? null;
      return mapped;
    }
    const surface = surfaceOf[kind];
    if (surface !== undefined && !portSurface[surface]) {
      mapped.unsupported = {
        capability: `testing.${surface}`,
        reason: "port-surface-absent",
        detail: surface,
      };
      return mapped;
    }
    // Review F-3: semantics the emitted subset cannot express land in
    // unsupported[], never an approximation.
    if (kind === "forbidden_effect" && assertion.scope === "resource") {
      // No resource ledger surface exists on the closed port contract.
      mapped.unsupported = {
        capability: "scenario.forbidden-scope-resource",
        reason: "scope-unimplemented",
        detail: "resource",
      };
      return mapped;
    }
    if (kind === "entity_state") {
      // A matcher outside the closed vocabulary the emitter can check is
      // unsupported, never silently weakened (review F-3).
      const known = ["datetime", "uuid", "uri", "decimal", "non-null"];
      const unknown = Object.entries(assertion.fields ?? {})
        .filter(([, expectation]) => expectation !== null && typeof expectation === "object"
          && "match" in expectation && !known.includes(expectation.match))
        .map(([field, expectation]) => `${field}:${expectation.match}`);
      if (unknown.length > 0) {
        mapped.unsupported = {
          capability: "scenario.match-kind",
          reason: "match-kind-unimplemented",
          detail: boundToken(unknown.join(",")),
        };
        return mapped;
      }
    }
    mapped.port = surface ?? null;
    mapped.payload = { ...assertion };
    delete mapped.payload.kind;
    for (const [key, value] of Object.entries(mapped.payload)) {
      if (value !== null && typeof value === "object" && !Array.isArray(value)
        && ("$ref" in value || "type" in value)) {
        const problem = checkLeaf(value, 0);
        if (problem) mapped.unsupported = { capability: "scenario.value", reason: problem };
      }
    }
    return mapped;
  });
}

/** Bounded, control-cleaned detail token (no raw attacker text). */
function boundToken(text) {
  return String(text ?? "unknown")
    .replace(/[^a-zA-Z0-9._:/-]+/g, "?")
    .slice(0, 128);
}

/**
 * The canonical AST digest input: the mapper outcome serialized with the
 * shared canonical JSON writer — used by the emitter header and tests.
 */
export function astDigestInput(model) {
  return canonicalJson(model);
}
