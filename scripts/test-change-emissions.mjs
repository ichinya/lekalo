#!/usr/bin/env node
// Direct command emissions are event contracts, independently of entity CRUD.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import { basename, dirname, join, resolve } from "node:path";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";

const require = createRequire(import.meta.url);
assert.equal(require("ajv/package.json").version, "8.20.0");
const Ajv = require("ajv/dist/2020").default;
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const bin = process.env.LEKALO_BIN ?? join(root, "target/debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
assert(existsSync(bin), "build the checkout CLI first");
const ajv = new Ajv({ strict: true, allErrors: true });
const schemas = Object.fromEntries(["model", "ir", "graph", "inspect", "change-request", "change-plan", "change-receipt"].map(name => [name, ajv.compile(JSON.parse(readFileSync(join(root, `contracts/${name}.schema.v0.8.0.json`), "utf8")))]));
const effectSchema = JSON.parse(readFileSync(join(root, "contracts/effect-graph.schema.v0.8.0.json"), "utf8"));
ajv.addSchema(effectSchema);
const effect = ajv.getSchema(`${effectSchema.$id}#/$defs/effect`);
const legacyModel = ajv.compile(JSON.parse(readFileSync(join(root, "contracts/model.schema.v0.2.16.json"), "utf8")));
const stage = mkdtempSync(join(tmpdir(), "lekalo-change-emissions-"));
let calls = 0;
function validate(name, doc) { assert(schemas[name](doc), `${name}: ${JSON.stringify(schemas[name].errors?.slice(-5))}`); }
function run(args, expected = 0) {
  calls++;
  const result = spawnSync(bin, [...args, "--project", ".", "--no-cache", "--json"], { cwd: stage, encoding: "utf8", timeout: 30000, env: { ...process.env, LEKALO_PROJECT: "" } });
  assert.equal(result.status, expected, result.stderr || result.stdout);
  const doc = JSON.parse(result.stdout.trim() || result.stderr.trim());
  if (expected === 0 && args[0] === "change") validate(doc.schemaVersion.includes("change-plan") ? "change-plan" : "change-receipt", doc);
  return doc;
}
try {
  cpSync(join(root, "tests/fixtures/change/field-vertical"), stage, { recursive: true, filter: path => basename(path) !== ".lekalo" });
  const commands = join(stage, "lekalo/modules/tasks/commands.yaml");
  const entities = join(stage, "lekalo/modules/tasks/entities.yaml");
  const events = join(stage, "lekalo/modules/tasks/events.yaml");
  writeFileSync(commands, 'schema_version: "0.2.16"\r\ndefinitions:\r\n  - id: tasks.complete\r\n    kind: command\r\n    version: 1 # keep command note');
  writeFileSync(events, 'schema_version: "0.2.16"\ndefinitions:\n  - id: tasks.completed\n    kind: event\n    version: 1\n');
  const add = ["change", "command-effect:add", "--command", "tasks.complete", "--emit", "tasks.completed"];
  const legacy = readFileSync(commands);
  run(add, 4);
  assert(readFileSync(commands).equals(legacy));
  for (const file of ["lekalo/project.yaml", "lekalo/modules/tasks/module.yaml", "lekalo/modules/tasks/entities.yaml", "lekalo/modules/tasks/commands.yaml", "lekalo/modules/tasks/events.yaml"]) {
    const path = join(stage, file);
    writeFileSync(path, readFileSync(path, "utf8").replaceAll("0.2.16", "0.8.0"));
  }
  const before = readFileSync(commands);
  const untouched = readFileSync(entities);
  writeFileSync(join(stage, "policy.json"), JSON.stringify({ schemaVersion: "lekalo/change-policy/v0.8.0", identity: "dev.lekalo.change-policy@0.8.0", allowBreakingClasses: ["data-loss-risk", "source-breaking"], evidence: { migrationPlanDigest: "sha256:" + "a".repeat(64) } }));
  const review = ["--allow-breaking", "--policy", "policy.json"];
  const plan = run([...add, ...review]);
  assert(plan.semanticDiff.changes.some(c => c.kind === "signature.effects-changed"));
  assert(readFileSync(commands).equals(before));
  assert(!existsSync(join(stage, ".lekalo")), "preview cannot create runtime state");
  const request = { schemaVersion: "lekalo/change-request/v0.8.0", operations: [{ operation: "command-effect:add", command: "tasks.complete", emit: "tasks.completed" }] };
  validate("change-request", request);
  writeFileSync(join(stage, "request.json"), JSON.stringify(request));
  assert.equal(run(["change", "request", "--request", "request.json", ...review]).planId, plan.planId);
  const apply = [...add, ...review, "--apply", "--idempotency-key", "emission-add", "--expect-plan", plan.planId];
  assert.equal(run(apply).state, "committed");
  assert.equal(run(apply).state, "replayed");
  assert(readFileSync(entities).equals(untouched));
  const source = readFileSync(commands, "utf8");
  assert(source.startsWith(before.toString("utf8")));
  assert(!source.endsWith("\n"));
  assert(!source.replaceAll("\r\n", "").includes("\n"));
  const ir = run(["load", "--ir"]).ir;
  validate("ir", ir);
  const command = ir.definitions.find(d => d.id === "tasks.complete");
  assert.deepEqual(command.emits, ["tasks.completed"]);
  assert(!command.effects?.length, "pure emission creates no CRUD contract");
  const graph = run(["graph", "export"]).graph;
  validate("graph", graph);
  assert(graph.edges.some(e => e.from.endsWith(":tasks.complete") && e.to.endsWith(":tasks.completed") && e.relation === "emits" && e.provenance.referenceRole === "command-emits"));
  const inspection = run(["inspect", "tasks.complete"]).inspect;
  validate("inspect", inspection);
  assert.deepEqual(inspection.contract.emits, ["tasks.completed"]);
  const shown = run(["effects", "show", "tasks.complete"]).effects;
  assert.equal(shown.identity, "dev.lekalo.effects@0.8.0");
  assert.equal(shown.edges.length, 1);
  assert(effect(shown.edges[0]), JSON.stringify(effect.errors));
  assert.equal(shown.edges[0].kind, "emit-event");
  assert.equal(shown.edges[0].provenance.role, "command-emits");
  assert.equal(shown.edges[0].action, null);
  const remove = ["change", "command-effect:remove", "--command", "tasks.complete", "--emit", "tasks.completed", ...review, "--apply", "--idempotency-key", "emission-remove"];
  assert.equal(run(remove).state, "committed");
  assert.equal(run(remove).state, "replayed");
  assert(readFileSync(commands).equals(before), "last emission removal restores exact authored source");
  run(["validate"]);
  assert.equal(run(["effects", "show", "tasks.complete"]).effects.edges.length, 0);
  const chained = { ...request, operations: [
    { operation: "definition:add", kind: "command", module: "tasks", definition: { name: "notify" } },
    { operation: "command-effect:add", command: "tasks.notify", emit: "tasks.completed" },
  ] };
  validate("change-request", chained);
  writeFileSync(join(stage, "request.json"), JSON.stringify(chained));
  assert.equal(run(["change", "request", "--request", "request.json", ...review, "--apply", "--idempotency-key", "emission-declare-and-bind"]).state, "committed");
  assert.equal(run(["effects", "show", "tasks.notify"]).effects.edges[0].kind, "emit-event");
  const authored = { schema_version: "0.8.0", definitions: [{ kind: "command", id: "tasks.complete", version: 1, emits: ["tasks.completed"] }] };
  validate("model", authored);
  assert.equal(legacyModel({ ...authored, schema_version: "0.2.16" }), false);
  for (const operation of [{ operation: "command-effect:add", command: "tasks.complete" }, { ...request.operations[0], effect: "tasks.task" }]) {
    assert.equal(schemas["change-request"]({ ...request, operations: [operation] }), false);
  }
  assert(!existsSync(join(stage, "apps")), "emission declaration never executes an emitter");
  console.log(JSON.stringify({ ok: true, gate: "change-emissions", calls, schemas: Object.keys(schemas).length + 1 }));
} finally {
  assert(stage.startsWith(resolve(tmpdir()) + "/") || stage.startsWith(resolve(tmpdir()) + "\\"));
  rmSync(stage, { recursive: true, force: true });
}
