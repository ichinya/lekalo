#!/usr/bin/env node
// CLI / JSON request parity, wire-value semantics and closed successor schemas.
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
const schemas = Object.fromEntries(["model", "ir", "change-request", "change-plan", "change-receipt"].map(name => [name, ajv.compile(JSON.parse(readFileSync(join(root, `contracts/${name}.schema.v0.8.0.json`), "utf8")))]));
const stage = mkdtempSync(join(tmpdir(), "lekalo-change-enum-wire-"));
let calls = 0;
function validate(name, doc) { assert(schemas[name](doc), `${name}: ${JSON.stringify(schemas[name].errors?.slice(-5))}`); }
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
  const args = ["change", "enum-case:add", "--enum", "tasks.priority", "--name", "urgent", "--value-json", JSON.stringify("URGENT")];
  const legacy = readFileSync(entities);
  run(args, 4);
  run(["change", "enum-case:change", "--enum", "tasks.priority", "--name", "normal", "--value-json", "null"], 4);
  assert(readFileSync(entities).equals(legacy));
  for (const file of ["lekalo/project.yaml", "lekalo/modules/tasks/module.yaml", "lekalo/modules/tasks/entities.yaml"]) {
    const path = join(stage, file);
    writeFileSync(path, readFileSync(path, "utf8").replaceAll("0.2.16", "0.8.0").replace(/\r?\n/g, "\r\n"));
  }
  const before = readFileSync(entities);
  const plan = run(args);
  assert(readFileSync(entities).equals(before));
  assert(!existsSync(join(stage, ".lekalo")), "a wire preview never creates runtime state");
  assert.equal(request([{ operation: "enum-case:add", enum: "tasks.priority", case: { name: "urgent", value: "URGENT" } }]).planId, plan.planId);
  const apply = [...args, "--apply", "--idempotency-key", "wire-first", "--expect-plan", plan.planId];
  assert.equal(run(apply).state, "committed");
  assert.equal(run(apply).state, "replayed");
  writeFileSync(entities, readFileSync(entities, "utf8").replace('wire_value: "URGENT"', 'wire_value: "URGENT" # keep wire note'));
  const change = ["change", "enum-case:change", "--enum", "tasks.priority", "--name", "urgent", "--value-json", JSON.stringify("NEXT"), "--description", "Explicit encoding"];
  const beforeDenied = readFileSync(entities);
  run([...change, "--apply", "--idempotency-key", "wire-denied", "--allow-breaking"], 3);
  assert(readFileSync(entities).equals(beforeDenied));
  writeFileSync(join(stage, "policy.json"), JSON.stringify({ schemaVersion: "lekalo/change-policy/v0.8.0", identity: "dev.lekalo.change-policy@0.8.0", allowBreakingClasses: ["data-loss-risk", "source-breaking"], evidence: { migrationPlanDigest: "sha256:" + "a".repeat(64) } }));
  const approved = [...change, "--policy", "policy.json", "--allow-breaking"];
  const preview = run(approved);
  assert(preview.semanticDiff.changes.some(c => c.kind === "type.member-wire-changed"));
  assert.equal(run([...approved, "--apply", "--idempotency-key", "wire-approved", "--expect-plan", preview.planId]).state, "committed");
  assert(readFileSync(entities, "utf8").includes('wire_value: "NEXT" # keep wire note\r\n'));
  assert.equal(run(change).equal, true);
  const structured = { ref: "tasks.text", items: [1, true, null] };
  request([
    { operation: "enum-case:add", enum: "tasks.priority", case: { name: "object_wire", value: structured } },
    { operation: "enum-case:add", enum: "tasks.priority", case: { name: "numeric_wire", value: 17 } },
    { operation: "enum-case:add", enum: "tasks.priority", case: { name: "null_wire", value: null } },
  ], ["--apply", "--idempotency-key", "wire-kinds"]);
  const ir = run(["load", "--ir"]).ir;
  validate("ir", ir);
  const values = ir.definitions.find(d => d.id === "tasks.priority").values;
  assert.equal(values.find(v => v.value === "urgent").wireValue, "NEXT");
  assert.deepEqual(values.find(v => v.value === "object_wire").wireValue, structured);
  assert.equal(values.find(v => v.value === "numeric_wire").wireValue, 17);
  assert(Object.hasOwn(values.find(v => v.value === "null_wire"), "wireValue"));
  assert.equal(values.find(v => v.value === "null_wire").wireValue, null);
  run(["change", "symbol:move", "--symbol", "tasks.text", "--to-module", "tasks", "--to-name", "label", "--allow-breaking", "--apply", "--idempotency-key", "wire-data-rename"]);
  const moved = run(["load", "--ir"]).ir;
  validate("ir", moved);
  assert.deepEqual(moved.definitions.find(d => d.id === "tasks.priority").values.find(v => v.value === "object_wire").wireValue, structured, "symbol renames cannot rewrite opaque wire data");
  run(["validate"]);
  const stable = readFileSync(entities);
  request([{ operation: "enum-case:add", enum: "tasks.priority", case: { name: "duplicate", value: "NEXT" } }], [], 1);
  run(["change", "enum-case:add", "--enum", "tasks.priority", "--name", "invalid_float", "--value-json", "1.5"], 1);
  assert(readFileSync(entities).equals(stable));
  assert(!existsSync(join(stage, "apps")), "enum changes do not generate target implementations");
  validate("model", { schema_version: "0.8.0", definitions: [{ kind: "enum", id: "tasks.priority", version: 1, values: [{ value: "object_wire", wire_value: structured }, { value: "null_wire", wire_value: null }] }] });
  console.log(JSON.stringify({ ok: true, gate: "change-enum-wire", calls, schemas: Object.keys(schemas).length }));
} finally {
  assert(stage.startsWith(resolve(tmpdir()) + "/") || stage.startsWith(resolve(tmpdir()) + "\\"));
  rmSync(stage, { recursive: true, force: true });
}
