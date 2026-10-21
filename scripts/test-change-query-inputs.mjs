#!/usr/bin/env node
// Query signatures, graph/inspect projections and closed successor schemas.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import { basename, dirname, join, resolve } from "node:path";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";

const require = createRequire(import.meta.url);
assert.equal(require("ajv/package.json").version, "8.17.1");
const Ajv = require("ajv/dist/2020").default;
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const bin = process.env.LEKALO_BIN ?? join(root, "target/debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
assert(existsSync(bin), "build the checkout CLI first");
const ajv = new Ajv({ strict: true, allErrors: true });
const schemas = Object.fromEntries(["model", "ir", "graph", "inspect", "change-request", "change-plan", "change-receipt"].map(name => [name, ajv.compile(JSON.parse(readFileSync(join(root, `contracts/${name}.schema.v0.8.0.json`), "utf8")))]));
const stage = mkdtempSync(join(tmpdir(), "lekalo-change-query-input-"));
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
function request(operations, flags = [], expected = 0) {
  const doc = { schemaVersion: "lekalo/change-request/v0.8.0", operations };
  validate("change-request", doc);
  writeFileSync(join(stage, "request.json"), JSON.stringify(doc));
  return run(["change", "request", "--request", "request.json", ...flags], expected);
}
try {
  cpSync(join(root, "tests/fixtures/change/field-vertical"), stage, { recursive: true, filter: path => basename(path) !== ".lekalo" });
  const queries = join(stage, "lekalo/modules/tasks/queries.yaml");
  const entities = join(stage, "lekalo/modules/tasks/entities.yaml");
  writeFileSync(queries, 'schema_version: "0.2.16"\r\ndefinitions:\r\n  - id: tasks.list\r\n    kind: query\r\n    version: 1\r\n    reads:\r\n      - tasks.task\r\n    returns: tasks.task # keep output note');
  const input = [{ name: "search", type: { ref: "tasks.text" }, default: "tasks.text" }];
  const args = ["change", "operation-input:set", "--operation", "tasks.list", "--input-json", JSON.stringify(input)];
  const beforeLegacy = readFileSync(queries);
  run(args, 4);
  const declaration = { operation: "definition:add", kind: "query", module: "tasks", definition: { name: "find", input, reads: ["tasks.task"], returns: { ref: "tasks.task" } } };
  request([declaration], [], 4);
  assert(readFileSync(queries).equals(beforeLegacy));
  for (const file of ["lekalo/project.yaml", "lekalo/modules/tasks/module.yaml", "lekalo/modules/tasks/entities.yaml", "lekalo/modules/tasks/queries.yaml"]) {
    const path = join(stage, file);
    writeFileSync(path, readFileSync(path, "utf8").replaceAll("0.2.16", "0.8.0").replace(/\r?\n/g, "\r\n"));
  }
  const before = readFileSync(queries);
  const untouched = readFileSync(entities);
  const plan = run(args);
  assert(plan.semanticDiff.changes.some(c => c.kind === "signature.input-added"));
  assert(readFileSync(queries).equals(before));
  assert(!existsSync(join(stage, ".lekalo")), "preview cannot create runtime state");
  assert.equal(request([{ operation: "operation-input:set", operationTarget: "tasks.list", definition: { input } }]).planId, plan.planId);
  const apply = [...args, "--apply", "--idempotency-key", "query-input-first", "--expect-plan", plan.planId];
  assert.equal(run(apply).state, "committed");
  assert.equal(run(apply).state, "replayed");
  assert(readFileSync(entities).equals(untouched));
  const source = readFileSync(queries, "utf8");
  assert(source.includes("returns: tasks.task # keep output note\r\n"));
  assert(!source.endsWith("\n"));
  assert(!source.replaceAll("\r\n", "").includes("\n"));
  const ir = run(["load", "--ir"]).ir;
  validate("ir", ir);
  assert.deepEqual(ir.definitions.find(d => d.id === "tasks.list").input, input);
  const graph = run(["graph", "export"]).graph;
  validate("graph", graph);
  assert(graph.edges.some(e => e.from.endsWith(":tasks.list") && e.to.endsWith(":tasks.text") && e.relation === "accepts" && e.provenance.referenceRole === "query-input"));
  const inspection = run(["inspect", "tasks.list"]).inspect;
  validate("inspect", inspection);
  assert(inspection.contract.input.some(f => f.name === "search"));
  assert.equal(run(args).fileEdits.length, 0);
  run(["change", "operation-input:set", "--operation", "tasks.list", "--input-json", JSON.stringify([{ name: "bad", type: { ref: "tasks.text" }, default: true }])], 1);
  assert.equal(readFileSync(queries, "utf8"), source);
  const changed = [{ name: "search", type: { ref: "tasks.text" }, default: "changed" }];
  const changedPlan = request([{ operation: "operation-input:set", operationTarget: "tasks.list", definition: { input: changed } }]);
  assert(changedPlan.semanticDiff.changes.some(c => c.kind === "field.default-changed"));
  request([declaration], ["--apply", "--idempotency-key", "query-input-declare"]);
  run(["validate"]);
  writeFileSync(queries, readFileSync(queries, "utf8")
    .replace('        type: "tasks.text"', '        type: "tasks.text" # keep type note')
    .replace('        default: "tasks.text"', '        default: "tasks.text"\r\n        description: \'old description\' # keep description note'));
  const metadataArgs = ["change", "operation-input:set", "--operation", "tasks.list", "--input-json", JSON.stringify([{ ...input[0], description: "can't" }])];
  const metadata = run(metadataArgs);
  assert.equal(metadata.semanticDiff.equal, true);
  assert.equal(run([...metadataArgs, "--apply", "--idempotency-key", "query-input-description", "--expect-plan", metadata.planId]).state, "committed");
  assert(readFileSync(queries, "utf8").includes("description: 'can''t' # keep description note\r\n"));
  assert(readFileSync(queries, "utf8").includes('type: "tasks.text" # keep type note\r\n'));
  const clear = ["change", "operation-input:set", "--operation", "tasks.find", "--input-json", "[]", "--apply", "--idempotency-key", "query-input-clear"];
  const stable = readFileSync(queries);
  run(clear, 3);
  assert(readFileSync(queries).equals(stable));
  run([...clear, "--allow-breaking"], 3);
  assert(readFileSync(queries).equals(stable));
  writeFileSync(join(stage, "policy.json"), JSON.stringify({ schemaVersion: "lekalo/change-policy/v0.8.0", identity: "dev.lekalo.change-policy@0.8.0", allowBreakingClasses: ["data-loss-risk", "source-breaking"], evidence: { migrationPlanDigest: "sha256:" + "a".repeat(64) } }));
  assert.equal(run([...clear, "--allow-breaking", "--policy", "policy.json"]).state, "committed");
  const cleared = run(["load", "--ir"]).ir;
  validate("ir", cleared);
  assert(!Object.hasOwn(cleared.definitions.find(d => d.id === "tasks.find"), "input"));
  assert.equal(cleared.definitions.find(d => d.id === "tasks.list").input.length, 1);
  validate("model", { schema_version: "0.8.0", definitions: [{ kind: "query", id: "tasks.list", version: 1, reads: ["tasks.task"], input }] });
  assert(!existsSync(join(stage, "apps")), "signature changes do not execute generation");
  console.log(JSON.stringify({ ok: true, gate: "change-query-inputs", calls, schemas: Object.keys(schemas).length }));
} finally {
  assert(stage.startsWith(resolve(tmpdir()) + "/") || stage.startsWith(resolve(tmpdir()) + "\\"));
  rmSync(stage, { recursive: true, force: true });
}
