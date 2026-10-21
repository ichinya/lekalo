#!/usr/bin/env node
// Real CLI and independently validated Model/IR/Change successor contracts.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
assert.equal(require("ajv/package.json").version, "8.20.0");
const Ajv = require("ajv/dist/2020").default;
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const bin = process.env.LEKALO_BIN ?? join(root, "target/debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
assert(existsSync(bin), "build the checkout CLI first");
const ajv = new Ajv({ strict: true, allErrors: true });
const schemas = Object.fromEntries(["model", "ir", "change-request", "change-plan", "change-receipt"].map(name => [name, ajv.compile(JSON.parse(readFileSync(join(root, `contracts/${name}.schema.v0.8.0.json`), "utf8")))]));
const stage = mkdtempSync(join(tmpdir(), "lekalo-change-defaults-"));
let calls = 0;
function validate(name, doc) { assert(schemas[name](doc), `${name}: ${JSON.stringify(schemas[name].errors)}`); }
function run(args, expected = 0) {
  calls++;
  const result = spawnSync(bin, [...args, "--project", ".", "--json"], { cwd: stage, encoding: "utf8", timeout: 30000, env: { ...process.env, LEKALO_PROJECT: "" } });
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
  const entities = join(stage, "lekalo/modules/tasks/entities.yaml");
  const args = ["change", "entity-field:add", "--entity", "tasks.task", "--name", "priority", "--type", "tasks.priority", "--default", "normal"];
  const legacy = readFileSync(entities);
  run(args, 4);
  assert(readFileSync(entities).equals(legacy));
  for (const file of ["lekalo/project.yaml", "lekalo/modules/tasks/module.yaml", "lekalo/modules/tasks/entities.yaml"]) {
    const path = join(stage, file);
    writeFileSync(path, readFileSync(path, "utf8").replaceAll("0.2.16", "0.8.0").replace(/\r?\n/g, "\r\n"));
  }
  const before = readFileSync(entities);
  const plan = run(args);
  assert(readFileSync(entities).equals(before));
  assert(!existsSync(join(stage, "lekalo/change-history.json")));
  const applyArgs = [...args, "--apply", "--idempotency-key", "default-field", "--expect-plan", plan.planId];
  const applied = run(applyArgs);
  assert.equal(applied.state, "committed");
  assert.equal(run(applyArgs).state, "replayed");
  assert(readFileSync(entities, "utf8").includes('default: "normal"'));
  assert(!/(?<!\r)\n/.test(readFileSync(entities, "utf8")), "default insertion retains CRLF");
  const ir = run(["load", "--ir"]).ir;
  validate("ir", ir);
  assert.equal(ir.definitions.find(d => d.id === "tasks.task").fields.find(f => f.name === "priority").default, "normal");
  const declaration = { name: "save", input: [{ name: "title", type: { ref: "tasks.text" }, default: "before" }] };
  request([{ operation: "definition:add", kind: "command", module: "tasks", definition: declaration }], ["--apply", "--idempotency-key", "default-command"]);
  const signature = value => [{ operation: "operation-input:set", operationTarget: "tasks.save", definition: { input: [{ name: "title", type: { ref: "tasks.text" }, default: value }] } }];
  // A large default cannot escape the closed 512-character diff summary.
  const large = "x".repeat(8192);
  const changed = request(signature(large));
  const delta = changed.semanticDiff.changes.find(change => change.kind === "field.default-changed");
  assert(delta, "a default-only change must survive the Change plan projection");
  assert.match(delta.changeId, /^sha256:[a-f0-9]{64}$/);
  const committed = request(signature("after"), ["--apply", "--idempotency-key", "default-signature"]);
  assert.equal(committed.state, "committed");
  assert.equal(request(signature("after")).equal, true);
  run(["validate"]);
  const finalBytes = readFileSync(entities);
  request([{ operation: "entity-field:add", entity: "tasks.task", field: { name: "invalid", type: { ref: "tasks.priority" }, default: "unknown" } }], [], 1);
  assert(readFileSync(entities).equals(finalBytes));
  // Schema parity over the owner bounds, including null versus absence.
  const model = { schema_version: "0.8.0", definitions: [{ id: "tasks.saved", kind: "event", version: 1, payload: [{ name: "title", type: { optional: { ref: "tasks.text" } }, default: null }] }] };
  validate("model", model);
  for (const invalid of [1.5, [[[[1]]]], "x".repeat(8193), Array(65).fill(0)]) {
    const bad = structuredClone(model);
    bad.definitions[0].payload[0].default = invalid;
    assert.equal(schemas.model(bad), false);
  }
  console.log(JSON.stringify({ ok: true, gate: "change-defaults", calls, schemas: Object.keys(schemas).length }));
} finally {
  assert(stage.startsWith(resolve(tmpdir()) + "/") || stage.startsWith(resolve(tmpdir()) + "\\"));
  rmSync(stage, { recursive: true, force: true });
}
