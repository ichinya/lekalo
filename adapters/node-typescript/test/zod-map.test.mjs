/**
 * #45 zod mapper suite: pure IR → schema-AST vectors, run in-process with
 * Node built-ins only. The optional/nullable orthogonality matrix (issue
 * acceptance criterion 3) is asserted literally: `required` governs key
 * presence, the `optional` wrapper governs value nullability, and all four
 * combinations map to distinct zod compositions.
 */
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  DEFAULT_POLICY,
  IR_IDENTITY,
  SCHEMA_KINDS,
  UNSUPPORTED,
  ZOD_DIR,
  mapProject,
  pascal,
} from "../src/zod-map.mjs";

/** One minimal well-formed field. */
function field(name, type, required = true) {
  return required ? { name, type, required: true } : { name, type };
}

const ref = (id) => ({ ref: id });
const list = (inner) => ({ list: inner });
const optional = (inner) => ({ optional: inner });

function project(definitions) {
  return { contract: IR_IDENTITY, definitions, modelVersion: "0.2.16" };
}

// ---------------------------------------------------------------------------
// Naming.
// ---------------------------------------------------------------------------

test("pascal converts snake segments deterministically", () => {
  assert.equal(pascal("planner"), "Planner");
  assert.equal(pascal("task_id"), "TaskId");
  assert.equal(pascal("focus_task"), "FocusTask");
  assert.equal(pascal("a1_b2"), "A1B2");
});

test("export names follow the uniform <Module><Name><Suffix> rule", () => {
  const { modules } = mapProject(
    project([
      { id: "planner.task", kind: "entity", fields: [], identity: [] },
      {
        id: "planner.focus_task",
        kind: "command",
        input: [],
      },
      { id: "planner.count", kind: "query", returns: ref("planner.task") },
      { id: "planner.task_focused", kind: "event", payload: [] },
      { id: "planner.state", kind: "enum", values: [{ value: "on" }] },
      { id: "planner.title", kind: "scalar", base: "string" },
    ]),
  );
  const names = modules[0].declarations.map((declaration) => declaration.exportName);
  assert.deepEqual(names, [
    "PlannerCountResultSchema",
    "PlannerFocusTaskInputSchema",
    "PlannerStateSchema",
    "PlannerTaskSchema",
    "PlannerTaskFocusedPayloadSchema",
    "PlannerTitleSchema",
  ]);
});

// ---------------------------------------------------------------------------
// Kind coverage and skipping.
// ---------------------------------------------------------------------------

test("only schema-bearing kinds produce declarations", () => {
  assert.deepEqual(SCHEMA_KINDS, [
    "scalar",
    "enum",
    "value-object",
    "entity",
    "command",
    "query",
    "event",
  ]);
  const { modules } = mapProject(
    project([
      { id: "planner.task", kind: "entity", fields: [], identity: [] },
      { id: "planner.act", kind: "effect", operation: "create", entity: "planner.task" },
      { id: "planner.api", kind: "endpoint", method: "POST", path: "/t" },
      { id: "planner.gate", kind: "policy", decision: "deny" },
      { id: "planner.flow", kind: "scenario" },
      { id: "planner.bind", kind: "target-binding", target: "node-typescript" },
    ]),
  );
  assert.equal(modules.length, 1);
  assert.equal(modules[0].declarations.length, 1);
  assert.equal(modules[0].declarations[0].semanticId, "planner.task");
});

// ---------------------------------------------------------------------------
// Optional vs nullable orthogonality (AC-3): the four-case matrix.
// ---------------------------------------------------------------------------

test("the four presence × nullability combinations stay distinct", () => {
  const { modules } = mapProject(
    project([
      { id: "planner.text", kind: "scalar", base: "string" },
      {
        id: "planner.matrix",
        kind: "value-object",
        fields: [
          // required + plain type → `field: T` (present, non-null).
          field("plain", ref("planner.text")),
          // required + optional wrapper → `field: T.nullable()` (present,
          // may be null). The nullable axis is carried as a structural
          // envelope around the inner expression so the two axes never
          // collapse into one wrapper.
          {
            name: "nullable",
            type: optional(ref("planner.text")),
            required: true,
          },
          // absent required + plain type → `field: T.optional()` (absent
          // key allowed).
          field("optionalKey", ref("planner.text"), false),
          // absent required + optional wrapper → `T.nullable().optional()`.
          field("optionalNullable", optional(ref("planner.text")), false),
        ],
      },
    ]),
  );
  const matrix = modules[0].declarations.find(
    (declaration) => declaration.semanticId === "planner.matrix",
  );
  const byName = new Map(matrix.fields.map((field) => [field.name, field]));
  // Plain: presence, no wrappers.
  assert.equal(byName.get("plain").required, true);
  assert.deepEqual(byName.get("plain").expr, {
    k: "ref",
    name: "PlannerTextSchema",
    module: "planner",
  });
  // Nullable: required (no key wrapper), nullability tracked for the
  // emitter by the declaration's own type envelope.
  assert.equal(byName.get("nullable").required, true);
  assert.deepEqual(byName.get("nullable").expr, {
    k: "nullable",
    inner: { k: "ref", name: "PlannerTextSchema", module: "planner" },
  });
  // Optional key: `.optional()` wrapper, no nullability.
  assert.equal(byName.get("optionalKey").required, false);
  assert.deepEqual(byName.get("optionalKey").expr, {
    k: "optional",
    inner: { k: "ref", name: "PlannerTextSchema", module: "planner" },
  });
  // Both axes: `.nullable()` inside `.optional()`.
  assert.deepEqual(byName.get("optionalNullable").expr, {
    k: "optional",
    inner: {
      k: "nullable",
      inner: { k: "ref", name: "PlannerTextSchema", module: "planner" },
    },
  });
});

test("the optional wrapper composes through lists", () => {
  const { modules } = mapProject(
    project([
      { id: "planner.tag", kind: "scalar", base: "string" },
      {
        id: "planner.holder",
        kind: "value-object",
        fields: [
          field("tags", list(optional(ref("planner.tag")))),
        ],
      },
    ]),
  );
  const holder = modules[0].declarations[0];
  assert.deepEqual(holder.fields[0].expr, {
    k: "array",
    item: { k: "nullable", inner: { k: "ref", name: "PlannerTagSchema", module: "planner" } },
  });
});

test("depth-four wrappers compose mechanically", () => {
  const { modules } = mapProject(
    project([
      { id: "planner.tag", kind: "scalar", base: "string" },
      {
        id: "planner.holder",
        kind: "value-object",
        fields: [
          field("deep", optional(list(optional(ref("planner.tag")))), false),
        ],
      },
    ]),
  );
  const holder = modules[0].declarations[0];
  assert.deepEqual(holder.fields[0].expr, {
    k: "optional",
    inner: {
      k: "nullable",
      inner: {
        k: "array",
        item: {
          k: "nullable",
          inner: { k: "ref", name: "PlannerTagSchema", module: "planner" },
        },
      },
    },
  });
});

// ---------------------------------------------------------------------------
// Branded identity scalars.
// ---------------------------------------------------------------------------

test("identity member scalars are branded with their semantic id", () => {
  const { modules } = mapProject(
    project([
      { id: "planner.task_id", kind: "scalar", base: "uuid" },
      { id: "planner.text", kind: "scalar", base: "string" },
      {
        id: "planner.task",
        kind: "entity",
        identity: ["task_id"],
        fields: [field("task_id", ref("planner.task_id")), field("title", ref("planner.text"))],
      },
    ]),
  );
  const declarations = new Map(
    modules[0].declarations.map((declaration) => [declaration.semanticId, declaration]),
  );
  assert.equal(declarations.get("planner.task_id").branded, true);
  assert.equal(declarations.get("planner.text").branded, false);
  assert.deepEqual(declarations.get("planner.task_id").expr, {
    k: "brand",
    inner: { k: "uuid" },
    brand: "planner.task_id",
  });
});

// ---------------------------------------------------------------------------
// Policies.
// ---------------------------------------------------------------------------

test("the default date policy maps to the date-string helper node", () => {
  const { modules } = mapProject(project([{ id: "planner.due", kind: "scalar", base: "date" }]));
  assert.deepEqual(modules[0].declarations[0].expr, { k: "dateString" });
});

test("the date-native policy maps date scalars to z.date()", () => {
  const { modules } = mapProject(
    project([{ id: "planner.due", kind: "scalar", base: "date" }]),
    { ...DEFAULT_POLICY, date: "date-native" },
  );
  assert.deepEqual(modules[0].declarations[0].expr, { k: "dateNative" });
});

test("the strip policy relaxes strict objects", () => {
  const { modules } = mapProject(
    project([
      {
        id: "planner.task",
        kind: "entity",
        identity: [],
        fields: [],
      },
    ]),
    { ...DEFAULT_POLICY, unknownKeys: "strip" },
  );
  assert.equal(modules[0].declarations[0].strict, false);
});

// ---------------------------------------------------------------------------
// Unsupported-construct classification (AC-4).
// ---------------------------------------------------------------------------

test("unknown scalar bases classify as unsupported findings", () => {
  const { modules, findings } = mapProject(
    project([{ id: "planner.count", kind: "scalar", base: "integer" }]),
  );
  assert.equal(modules.length, 0);
  assert.deepEqual(findings, [
    {
      path: `${ZOD_DIR}/planner.ts`,
      code: UNSUPPORTED,
      detail: "symbol:planner.count",
    },
  ]);
});

test("refs to non-schema-bearing definitions classify as unsupported", () => {
  const { modules, findings } = mapProject(
    project([
      { id: "planner.task", kind: "entity", identity: [], fields: [] },
      {
        id: "planner.act",
        kind: "effect",
        operation: "create",
        entity: "planner.task",
      },
      {
        id: "planner.wrap",
        kind: "value-object",
        fields: [field("effect", ref("planner.act"))],
      },
    ]),
  );
  assert.equal(modules.length, 1);
  assert.equal(modules[0].declarations.length, 1, "only the entity mapped");
  assert.equal(findings.length, 1);
  assert.equal(findings[0].detail, "symbol:planner.wrap");
});

test("unknown type shapes and missing returns classify as unsupported", () => {
  const one = mapProject(
    project([
      { id: "planner.text", kind: "scalar", base: "string" },
      {
        id: "planner.weird",
        kind: "value-object",
        fields: [field("x", { map: { key: "planner.text", value: "planner.text" } })],
      },
    ]),
  );
  assert.equal(one.findings.length, 1);
  assert.equal(one.findings[0].detail, "symbol:planner.weird");

  const two = mapProject(
    project([{ id: "planner.q", kind: "query" }]),
  );
  assert.equal(two.findings.length, 1);
  assert.equal(two.findings[0].detail, "symbol:planner.q");
});

test("missing ref targets classify as unsupported, not crashes", () => {
  const { findings } = mapProject(
    project([
      {
        id: "planner.task",
        kind: "entity",
        identity: [],
        fields: [field("ghost", ref("planner.ghost"))],
      },
    ]),
  );
  assert.equal(findings.length, 1);
  assert.equal(findings[0].detail, "symbol:planner.task");
});

// ---------------------------------------------------------------------------
// Cross-module imports and sidecar field paths.
// ---------------------------------------------------------------------------

test("cross-module refs produce sorted import entries", () => {
  const { modules } = mapProject(
    project([
      { id: "alpha.text", kind: "scalar", base: "string" },
      { id: "alpha.task", kind: "entity", identity: [], fields: [] },
      {
        id: "beta.report",
        kind: "value-object",
        fields: [
          field("text", ref("alpha.text")),
          field("task", ref("alpha.task")),
          field("local", ref("beta.local")),
        ],
      },
      { id: "beta.local", kind: "scalar", base: "string" },
    ]),
  );
  const beta = modules.find((module) => module.id === "beta");
  assert.deepEqual(beta.imports, [
    { module: "alpha", names: ["AlphaTaskSchema", "AlphaTextSchema"] },
  ]);
  const alpha = modules.find((module) => module.id === "alpha");
  assert.deepEqual(alpha.imports, []);
});

test("sidecar field paths flatten through same-module object refs", () => {
  const { modules } = mapProject(
    project([
      { id: "planner.tag", kind: "scalar", base: "string" },
      {
        id: "planner.window",
        kind: "value-object",
        fields: [field("from", ref("planner.tag"))],
      },
      {
        id: "planner.task",
        kind: "entity",
        identity: [],
        fields: [
          field("due", ref("planner.tag")),
          field("window", list(ref("planner.window"))),
        ],
      },
    ]),
  );
  const planner = modules.find((module) => module.id === "planner");
  // Object roots overwrite each other on the shared "" key; the last
  // declaration in semantic-id order wins deterministically, and every
  // flattened path still resolves to its owning symbol.
  assert.deepEqual(planner.fields, {
    PlannerTagSchema: "planner.tag",
    PlannerTaskSchema: "planner.task",
    PlannerWindowSchema: "planner.window",
    "": "planner.window",
    due: "planner.task",
    window: "planner.task",
    "window.0.from": "planner.window",
    from: "planner.window",
  });
});

test("module records sort by id and declarations by semantic id", () => {
  const { modules } = mapProject(
    project([
      { id: "zeta.task", kind: "entity", identity: [], fields: [] },
      { id: "alpha.state", kind: "enum", values: [{ value: "b" }, { value: "a" }] },
      { id: "alpha.text", kind: "scalar", base: "string" },
    ]),
  );
  assert.deepEqual(modules.map((module) => module.id), ["alpha", "zeta"]);
  assert.deepEqual(
    modules[0].declarations.map((declaration) => declaration.semanticId),
    ["alpha.state", "alpha.text"],
  );
});

test("enum declared order is preserved (order is semantic)", () => {
  const { modules } = mapProject(
    project([
      {
        id: "planner.state",
        kind: "enum",
        values: [{ value: "backlog" }, { value: "focused" }, { value: "done" }],
      },
    ]),
  );
  assert.deepEqual(modules[0].declarations[0].values, ["backlog", "focused", "done"]);
});

test("mapping the same IR twice is structurally identical", () => {
  const ir = project([
    { id: "planner.task_id", kind: "scalar", base: "uuid" },
    {
      id: "planner.task",
      kind: "entity",
      identity: ["task_id"],
      fields: [field("task_id", ref("planner.task_id")), field("due", optional(ref("planner.text")), false)],
    },
    { id: "planner.text", kind: "scalar", base: "string" },
  ]);
  assert.deepEqual(mapProject(ir), mapProject(ir));
});
