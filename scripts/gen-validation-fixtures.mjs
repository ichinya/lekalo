// Issue #12 fixture generator: builds the validation fixture matrix under
// tests/fixtures/validation. Run once with `node scripts/gen-validation-fixtures.mjs`;
// the output files are committed and never regenerated in CI.

import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "tests", "fixtures", "validation");

const SCHEMA = "0.1.0";

const doc = (definitions) => `${JSON.stringify({ schema_version: SCHEMA, definitions }, null, 2)}\n`;

// The clean base project: one planner module exercising every reference
// site with resolvable, kind-correct targets. `effectsRef` lets a case
// point the command's effects list at a wrong or unknown symbol.
function base({ visibility = {}, portability = {}, effectsRef = "planner.effect_create" } = {}) {
  const common = (id) => {
    const extra = {};
    if (visibility[id] === "module") extra.visibility = "module";
    if (visibility[id] === "project") extra.visibility = "project";
    if (portability[id]) extra.portability = portability[id];
    return extra;
  };
  return {
    "lekalo/project.yaml": doc([{ id: "planner", kind: "project", version: 1, description: "Validation base project" }]),
    "lekalo/modules/planner/module.yaml": doc([{ id: "planner", kind: "module", version: 1, description: "Base module" }]),
    "lekalo/modules/planner/entities.yaml": doc([
      { id: "planner.task_id", kind: "scalar", version: 1, description: "Task identifier", base: "uuid", ...common("planner.task_id") },
      { id: "planner.text", kind: "scalar", version: 1, description: "Text", base: "string", ...common("planner.text") },
      { id: "planner.task", kind: "entity", version: 1, description: "Task", fields: [
        { name: "task_id", type: { ref: "planner.task_id" }, required: true },
        { name: "title", type: { ref: "planner.text" }, required: true },
      ], identity: ["task_id"] },
    ]),
    "lekalo/modules/planner/commands.yaml": doc([
      { id: "planner.effect_create", kind: "effect", version: 1, description: "Create effect", operation: "create", entity: "planner.task", emits: ["planner.task_created"] },
      { id: "planner.create_task", kind: "command", version: 1, description: "Create task", input: [
        { name: "title", type: { ref: "planner.text" }, required: true },
      ], effects: [effectsRef] },
    ]),
    "lekalo/modules/planner/events.yaml": doc([
      { id: "planner.task_created", kind: "event", version: 1, description: "Created event", payload: [
        { name: "task_id", type: { ref: "planner.task_id" }, required: true },
      ] },
    ]),
    "lekalo/modules/planner/queries.yaml": doc([
      { id: "planner.get_task", kind: "query", version: 1, description: "Get task", ...common("planner.get_task"), reads: ["planner.task"], returns: { ref: "planner.task" } },
    ]),
    "lekalo/modules/planner/policies.yaml": doc([
      { id: "planner.allow_create", kind: "policy", version: 1, description: "Allow create", applies_to: ["planner.create_task"], decision: "allow" },
    ]),
    "lekalo/modules/planner/scenarios.yaml": doc([
      { id: "planner.scenario_create", kind: "scenario", version: 1, description: "Create scenario", summary: "Create one task.", covers: ["planner.create_task"] },
    ]),
    "lekalo/modules/planner/bindings.yaml": doc([
      { id: "planner.endpoint_create", kind: "endpoint", version: 1, description: "Create endpoint", invokes: "planner.create_task", method: "POST", path: "/tasks" },
    ]),
  };
}

const swap = (files, key, from, to) => {
  const body = files[key];
  if (!body.includes(from)) throw new Error(`fixture mutation missed: ${from} not in ${key}`);
  files[key] = body.replace(from, to);
  return files;
};

const expect = (rule) => `${JSON.stringify({ outcome: "invalid", reasonCodes: [rule] }, null, 2)}\n`;

// Write one fixture: `files` maps project-relative POSIX paths to bytes.
function fixture(name, files) {
  const dir = join(out, name);
  rmSync(dir, { recursive: true, force: true });
  for (const [relative, body] of Object.entries(files)) {
    const target = join(dir, relative);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, body);
  }
}

const cases = {
  // Positive: the untouched base validates clean.
  "valid/base": base(),

  // Negative: one semantic defect per fixture; the defect is the ONLY
  // difference from the base, so exactly one rule fires per fixture.
  "invalid/type-ref-unresolved": expect("semantic.type-ref-unresolved") && (() => {
    const files = base();
    swap(files, "lekalo/modules/planner/entities.yaml", '"ref": "planner.text"', '"ref": "planner.ghost"');
    files["expect.json"] = expect("semantic.type-ref-unresolved");
    return files;
  })(),
  "invalid/type-ref-kind-mismatch": (() => {
    const files = base();
    swap(files, "lekalo/modules/planner/entities.yaml", '"ref": "planner.text"', '"ref": "planner.create_task"');
    files["expect.json"] = expect("semantic.type-ref-kind-mismatch");
    return files;
  })(),
  "invalid/command-effect-unresolved": (() => {
    const files = base({ effectsRef: "planner.ghost_effect" });
    files["expect.json"] = expect("semantic.command-effect-unresolved");
    return files;
  })(),
  "invalid/command-effect-kind-mismatch": (() => {
    const files = base({ effectsRef: "planner.get_task" });
    files["expect.json"] = expect("semantic.command-effect-kind-mismatch");
    return files;
  })(),
  "invalid/query-read-unresolved": (() => {
    const files = base();
    swap(files, "lekalo/modules/planner/queries.yaml", '"planner.task"', '"planner.ghost"');
    files["expect.json"] = expect("semantic.query-read-unresolved");
    return files;
  })(),
  "invalid/query-read-kind-mismatch": (() => {
    const files = base();
    swap(files, "lekalo/modules/planner/queries.yaml", '"planner.task"', '"planner.create_task"');
    files["expect.json"] = expect("semantic.query-read-kind-mismatch");
    return files;
  })(),
  "invalid/policy-operation-unresolved": (() => {
    const files = base();
    swap(files, "lekalo/modules/planner/policies.yaml", '"planner.create_task"', '"planner.ghost"');
    files["expect.json"] = expect("semantic.policy-operation-unresolved");
    return files;
  })(),
  "invalid/policy-operation-kind-mismatch": (() => {
    const files = base();
    swap(files, "lekalo/modules/planner/policies.yaml", '"planner.create_task"', '"planner.task_created"');
    files["expect.json"] = expect("semantic.policy-operation-kind-mismatch");
    return files;
  })(),
  "invalid/effect-resource-unresolved": (() => {
    const files = base();
    swap(files, "lekalo/modules/planner/commands.yaml", '"entity": "planner.task"', '"entity": "planner.ghost"');
    files["expect.json"] = expect("semantic.effect-resource-unresolved");
    return files;
  })(),
  "invalid/effect-resource-kind-mismatch": (() => {
    const files = base();
    swap(files, "lekalo/modules/planner/commands.yaml", '"entity": "planner.task"', '"entity": "planner.text"');
    files["expect.json"] = expect("semantic.effect-resource-kind-mismatch");
    return files;
  })(),
  "invalid/effect-emits-unresolved": (() => {
    const files = base();
    swap(files, "lekalo/modules/planner/commands.yaml", '"planner.task_created"', '"planner.ghost_event"');
    files["expect.json"] = expect("semantic.effect-emits-unresolved");
    return files;
  })(),
  "invalid/effect-emits-kind-mismatch": (() => {
    const files = base();
    swap(files, "lekalo/modules/planner/commands.yaml", '"planner.task_created"', '"planner.create_task"');
    files["expect.json"] = expect("semantic.effect-emits-kind-mismatch");
    return files;
  })(),
  "invalid/endpoint-operation-unresolved": (() => {
    const files = base();
    swap(files, "lekalo/modules/planner/bindings.yaml", '"planner.create_task"', '"planner.ghost"');
    files["expect.json"] = expect("semantic.endpoint-operation-unresolved");
    return files;
  })(),
  "invalid/endpoint-operation-kind-mismatch": (() => {
    const files = base();
    swap(files, "lekalo/modules/planner/bindings.yaml", '"planner.create_task"', '"planner.effect_create"');
    files["expect.json"] = expect("semantic.endpoint-operation-kind-mismatch");
    return files;
  })(),
  "invalid/scenario-operation-unresolved": (() => {
    const files = base();
    swap(files, "lekalo/modules/planner/scenarios.yaml", '"planner.create_task"', '"planner.ghost"');
    files["expect.json"] = expect("semantic.scenario-operation-unresolved");
    return files;
  })(),
  "invalid/entity-identity-field-missing": (() => {
    const files = base();
    swap(files, "lekalo/modules/planner/entities.yaml", '"task_id"', '"ghost_field"');
    files["expect.json"] = expect("semantic.entity-identity-field-missing");
    return files;
  })(),
  "invalid/type-recursion": (() => {
    const files = base();
    files["lekalo/modules/planner/entities.yaml"] = doc([
      { id: "planner.loop_a", kind: "entity", version: 1, description: "Loop A", fields: [
        { name: "other", type: { ref: "planner.loop_b" }, required: true },
      ], identity: ["other"] },
      { id: "planner.loop_b", kind: "entity", version: 1, description: "Loop B", fields: [
        { name: "back", type: { ref: "planner.loop_a" }, required: true },
      ], identity: ["back"] },
      { id: "planner.task_id", kind: "scalar", version: 1, description: "Task identifier", base: "uuid" },
      { id: "planner.text", kind: "scalar", version: 1, description: "Text", base: "string" },
      { id: "planner.task", kind: "entity", version: 1, description: "Task", fields: [
        { name: "task_id", type: { ref: "planner.task_id" }, required: true },
        { name: "title", type: { ref: "planner.text" }, required: true },
      ], identity: ["task_id"] },
    ]);
    files["expect.json"] = expect("semantic.type-recursion");
    return files;
  })(),
};

for (const [name, files] of Object.entries(cases)) fixture(name, files);

// Warnings: the portability rule fires at warning severity under strict and
// is downgraded to info under the default profile.
{
  const files = base({ portability: { "planner.text": "target-specific", "planner.get_task": "portable" } });
  swap(files, "lekalo/modules/planner/queries.yaml", '"ref": "planner.task"', '"ref": "planner.text"');
  fixture("warning/portable-target-reference", files);
}

// Visibility: two modules; the planner query reads an audit entity that is
// module-private, and the audit query returns it project-visibly.
{
  const boundary = {
    "lekalo/project.yaml": doc([{ id: "planner", kind: "project", version: 1, description: "Visibility project" }]),
    "lekalo/modules/planner/module.yaml": doc([{ id: "planner", kind: "module", version: 1, description: "Planner", imports: ["audit"] }]),
    "lekalo/modules/planner/entities.yaml": doc([
      { id: "planner.task_id", kind: "scalar", version: 1, description: "Task identifier", base: "uuid" },
      { id: "planner.task", kind: "entity", version: 1, description: "Task", fields: [
        { name: "task_id", type: { ref: "planner.task_id" }, required: true },
      ], identity: ["task_id"] },
    ]),
    "lekalo/modules/planner/queries.yaml": doc([
      { id: "planner.get_log", kind: "query", version: 1, description: "Read audit log", reads: ["audit.log"] },
    ]),
    "lekalo/modules/audit/module.yaml": doc([{ id: "audit", kind: "module", version: 1, description: "Audit" }]),
    "lekalo/modules/audit/entities.yaml": doc([
      { id: "audit.log", kind: "entity", version: 1, description: "Audit log", visibility: "module", fields: [
        { name: "line", type: { ref: "audit.entry" }, required: true },
      ], identity: ["line"] },
      { id: "audit.entry", kind: "scalar", version: 1, description: "Log line", base: "string" },
    ]),
    "expect.json": expect("semantic.visibility-boundary-violation"),
  };
  fixture("invalid/visibility-boundary", boundary);

  const leak = structuredClone(boundary);
  delete leak["lekalo/modules/planner/queries.yaml"];
  delete leak["expect.json"];
  leak["lekalo/modules/planner/queries.yaml"] = doc([
    { id: "planner.get_task", kind: "query", version: 1, description: "Get task", reads: ["planner.task"], returns: { ref: "planner.task" } },
  ]);
  leak["lekalo/modules/audit/queries.yaml"] = doc([
    { id: "audit.get_log", kind: "query", version: 1, description: "Expose log", reads: ["audit.log"], returns: { ref: "audit.log" } },
  ]);
  leak["expect.json"] = expect("semantic.public-output-private-type");
  fixture("invalid/public-output-private", leak);
}

console.log("validation fixtures written to", out);
