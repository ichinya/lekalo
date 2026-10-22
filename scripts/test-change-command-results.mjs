#!/usr/bin/env node
// Command results must survive authored source, typed changes and read projections.
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
const legacyModel = ajv.compile(JSON.parse(readFileSync(join(root, "contracts/model.schema.v0.2.16.json"), "utf8")));
const stage = mkdtempSync(join(tmpdir(), "lekalo-change-command-results-"));
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
  const commands = join(stage, "lekalo/modules/tasks/commands.yaml");
  const entities = join(stage, "lekalo/modules/tasks/entities.yaml");
  writeFileSync(commands, 'schema_version: "0.2.16"\r\ndefinitions:\r\n  - id: tasks.complete\r\n    kind: command\r\n    version: 1 # keep command note');
  const args = ["change", "operation-output:set", "--operation", "tasks.complete", "--type", "tasks.text?"];
  const declaration = { operation: "definition:add", kind: "command", module: "tasks", definition: { name: "create", returns: { ref: "tasks.task" } } };
  const beforeLegacy = readFileSync(commands);
  run(args, 4);
  request([declaration], [], 4);
  assert(readFileSync(commands).equals(beforeLegacy));
  for (const file of ["lekalo/project.yaml", "lekalo/modules/tasks/module.yaml", "lekalo/modules/tasks/entities.yaml", "lekalo/modules/tasks/commands.yaml"]) {
    const path = join(stage, file);
    writeFileSync(path, readFileSync(path, "utf8").replaceAll("0.2.16", "0.8.0").replace(/\r?\n/g, "\r\n"));
  }
  const before = readFileSync(commands);
  const untouched = readFileSync(entities);
  writeFileSync(join(stage, "policy.json"), JSON.stringify({ schemaVersion: "lekalo/change-policy/v0.8.0", identity: "dev.lekalo.change-policy@0.8.0", allowBreakingClasses: ["data-loss-risk", "source-breaking"], evidence: { migrationPlanDigest: "sha256:" + "a".repeat(64) } }));
  const review = ["--allow-breaking", "--policy", "policy.json"];
  const plan = run([...args, ...review]);
  assert(plan.semanticDiff.changes.some(c => c.kind === "signature.output-changed"));
  assert(readFileSync(commands).equals(before));
  assert(!existsSync(join(stage, ".lekalo")), "preview cannot create runtime state");
  assert.equal(request([{ operation: "operation-output:set", operationTarget: "tasks.complete", type: { optional: { ref: "tasks.text" } } }], review).planId, plan.planId);
  const apply = [...args, ...review, "--apply", "--idempotency-key", "command-result-first", "--expect-plan", plan.planId];
  assert.equal(run(apply).state, "committed");
  assert.equal(run(apply).state, "replayed");
  assert(readFileSync(entities).equals(untouched));
  const source = readFileSync(commands, "utf8");
  assert(source.startsWith(before.toString("utf8")));
  assert(source.includes("# keep command note\r\n    returns:"));
  assert(!source.endsWith("\n"));
  assert(!source.replaceAll("\r\n", "").includes("\n"));
  const ir = run(["load", "--ir"]).ir;
  validate("ir", ir);
  assert.deepEqual(ir.definitions.find(d => d.id === "tasks.complete").returns, { optional: { ref: "tasks.text" } });
  const graph = run(["graph", "export"]).graph;
  validate("graph", graph);
  assert(graph.edges.some(e => e.from.endsWith(":tasks.complete") && e.to.endsWith(":tasks.text") && e.relation === "returns" && e.provenance.referenceRole === "command-returns"));
  const inspection = run(["inspect", "tasks.complete"]).inspect;
  validate("inspect", inspection);
  assert(JSON.stringify(inspection.contract.output).includes("tasks.text"));
  assert.equal(run([...args, ...review]).fileEdits.length, 0);
  run(["change", "operation-output:set", "--operation", "tasks.complete", "--type", "tasks.missing"], 1);
  assert.equal(readFileSync(commands, "utf8"), source);
  request([declaration], ["--apply", "--idempotency-key", "command-result-declare"]);
  run(["validate"]);
  const authored = { schema_version: "0.8.0", definitions: [{ kind: "command", id: "tasks.complete", version: 1, returns: { optional: { ref: "tasks.text" } } }] };
  validate("model", authored);
  assert.equal(legacyModel({ ...authored, schema_version: "0.2.16" }), false);
  assert(!existsSync(join(stage, "apps")), "signature changes do not execute generation");
  console.log(JSON.stringify({ ok: true, gate: "change-command-results", calls, schemas: Object.keys(schemas).length }));
} finally {
  assert(stage.startsWith(resolve(tmpdir()) + "/") || stage.startsWith(resolve(tmpdir()) + "\\"));
  rmSync(stage, { recursive: true, force: true });
}
