#!/usr/bin/env node
// Query-model successor signatures must exactly join the authored Model input.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
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
const schema = ajv.compile(JSON.parse(readFileSync(join(root, "contracts/query-model.schema.v0.8.0.json"), "utf8")));
const stage = mkdtempSync(join(tmpdir(), "lekalo-query-input-bindings-"));
const sha = bytes => "sha256:" + createHash("sha256").update(bytes).digest("hex");
let calls = 0;
function run(args, expected = 0, detail) {
  calls++;
  const result = spawnSync(bin, [...args, "--project", ".", "--no-cache", "--json"], { cwd: stage, encoding: "utf8", timeout: 30000, env: { ...process.env, LEKALO_PROJECT: "" } });
  assert.equal(result.status, expected, result.stderr || result.stdout);
  const wire = (result.stdout || result.stderr).trimEnd();
  const doc = JSON.parse(wire);
  if (detail) assert(wire.includes(detail), wire);
  return { wire, doc };
}
function validate(doc, expected = 0, detail) {
  writeFileSync(join(stage, "queries.json"), JSON.stringify(doc));
  return run(["query-model", "validate", "queries.json"], expected, detail);
}
try {
  cpSync(join(root, "tests/fixtures/change/field-vertical"), stage, { recursive: true, filter: path => basename(path) !== ".lekalo" });
  for (const file of ["lekalo/project.yaml", "lekalo/modules/tasks/module.yaml", "lekalo/modules/tasks/entities.yaml"]) {
    const path = join(stage, file);
    writeFileSync(path, readFileSync(path, "utf8").replaceAll("0.2.16", "0.8.0"));
  }
  const queries = join(stage, "lekalo/modules/tasks/queries.yaml");
  writeFileSync(queries, 'schema_version: "0.8.0"\ndefinitions:\n  - id: tasks.find\n    kind: query\n    version: 1\n    input:\n      - name: title\n        type: tasks.text\n        required: true\n        default: sample\n      - name: tag\n        type: tasks.text?\n        default: null\n    reads:\n      - tasks.task\n    returns: tasks.task\n');
  const before = readFileSync(queries);
  const model = run(["load"]);
  const ir = run(["load", "--ir"]);
  // The fixture must round-trip exactly before JavaScript hashes any nested
  // canonical payload: numeric precision or key-order loss is a hard failure.
  assert.equal(model.wire, JSON.stringify(model.doc));
  assert.equal(ir.wire, JSON.stringify(ir.doc));
  const attachment = {
    schemaVersion: "lekalo/query-model/v0.8.0", identity: "dev.lekalo.query-model@0.8.0",
    attachmentRevision: "1.0.0", projectId: ir.doc.ir.project.id,
    modelRef: { modelVersion: "0.8.0", digest: sha(model.wire) },
    irRef: { identity: "dev.lekalo.ir@0.8.0", digest: sha(JSON.stringify(ir.doc.ir)) },
    tenancy: [], queries: [{ query: "tasks.find", source: "tasks.task", cardinality: "one", consistency: "strong", parameters: [
      { name: "title", type: "tasks.text", required: true, default: "sample" },
      { name: "tag", type: "tasks.text?", required: false, default: null },
    ] }],
  };
  assert(schema(attachment), JSON.stringify(schema.errors));
  assert.equal(validate(attachment).doc.queryModel.queryCount, 1);
  for (const [member, value, detail] of [
    ["name", "other", "query-input-members"],
    ["type", "tasks.bool", "query-input-type"],
    ["required", false, "query-input-required"],
    ["default", "different", "query-input-default"],
  ]) {
    const invalid = structuredClone(attachment);
    invalid.queries[0].parameters[0][member] = value;
    assert(schema(invalid), JSON.stringify(schema.errors));
    validate(invalid, 1, detail);
  }
  const absent = structuredClone(attachment);
  delete absent.queries[0].parameters[1].default;
  assert(schema(absent), JSON.stringify(schema.errors));
  validate(absent, 1, "query-input-default");
  const missing = structuredClone(attachment);
  delete missing.queries[0].parameters[0].required;
  assert.equal(schema(missing), false);
  validate(missing, 1, "parameter-required");
  const mixed = structuredClone(attachment);
  mixed.modelRef.modelVersion = "0.2.16";
  assert.equal(schema(mixed), false);
  validate(mixed, 1, "model-version");
  const stale = structuredClone(attachment);
  stale.irRef.digest = "sha256:" + "0".repeat(64);
  assert(schema(stale), JSON.stringify(schema.errors));
  validate(stale, 1, "ir-digest");
  const duplicate = structuredClone(attachment);
  duplicate.queries[0].parameters.push(duplicate.queries[0].parameters[0]);
  validate(duplicate, 1, "parameter-duplicate");
  const oversized = structuredClone(attachment);
  oversized.queries[0].parameters[0].default = "x".repeat(8193);
  assert.equal(schema(oversized), false);
  validate(oversized, 1, "parameter-default");
  assert(readFileSync(queries).equals(before));
  assert(!existsSync(join(stage, ".lekalo")), "read-only resolution must not create runtime state");
  assert(!existsSync(join(stage, "apps")), "signature resolution does not execute generation");
  const fields = Array.from({ length: 256 }, (_, index) => `p${index}`);
  writeFileSync(queries, 'schema_version: "0.8.0"\ndefinitions:\n  - id: tasks.find\n    kind: query\n    version: 1\n    input:\n'
    + fields.map(name => `      - name: ${name}\n        type: tasks.text\n`).join("")
    + '    reads:\n      - tasks.task\n    returns: tasks.task\n');
  const wideModel = run(["load"]);
  const wideIr = run(["load", "--ir"]);
  assert.equal(wideModel.wire, JSON.stringify(wideModel.doc));
  assert.equal(wideIr.wire, JSON.stringify(wideIr.doc));
  const wide = structuredClone(attachment);
  wide.modelRef.digest = sha(wideModel.wire);
  wide.irRef.digest = sha(JSON.stringify(wideIr.doc.ir));
  wide.queries[0].parameters = fields.map(name => ({ name, type: "tasks.text", required: false }));
  assert(schema(wide), JSON.stringify(schema.errors));
  assert.equal(validate(wide).doc.queryModel.queryCount, 1);
  wide.queries[0].parameters.push({ name: "overflow", type: "tasks.text", required: false });
  assert.equal(schema(wide), false);
  validate(wide, 1, "parameter-bound");
  console.log(JSON.stringify({ ok: true, gate: "query-input-bindings", calls, schemas: 1, exactJoin: ["name", "type", "required", "default"] }));
} finally {
  assert(stage.startsWith(resolve(tmpdir()) + "/") || stage.startsWith(resolve(tmpdir()) + "\\"));
  rmSync(stage, { recursive: true, force: true });
}
