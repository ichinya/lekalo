#!/usr/bin/env node
// Real CLI + closed wire schemas; no generated/native implementation writes.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const require = createRequire(import.meta.url);
assert.equal(require("ajv/package.json").version, "8.17.1");
const Ajv = require("ajv/dist/2020").default;
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const bin = process.env.LEKALO_BIN ?? join(root, "target/debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
assert(existsSync(bin), "build the checkout CLI first");
const ajv = new Ajv({ strict: true, allErrors: true });
const schemas = Object.fromEntries(["request", "plan", "receipt"].map(name => [name, ajv.compile(JSON.parse(readFileSync(join(root, `contracts/change-${name}.schema.v0.8.0.json`), "utf8")))]));
const stage = mkdtempSync(join(tmpdir(), "lekalo-change-symbols-"));
let calls = 0;
function run(args, expected = 0) {
  calls++;
  const result = spawnSync(bin, [...args, "--project", ".", "--json"], { cwd: stage, encoding: "utf8", timeout: 30000, env: { ...process.env, LEKALO_PROJECT: "" } });
  assert.equal(result.status, expected, result.stderr || result.stdout);
  const doc = JSON.parse(result.stdout.trim() || result.stderr.trim());
  if (expected === 0 && args[0] === "change") {
    const validate = schemas[doc.schemaVersion.includes("change-plan") ? "plan" : "receipt"];
    assert(validate(doc), JSON.stringify(validate.errors));
  }
  return doc;
}
function sugar(symbol, module, name) {
  return ["change", "symbol:move", "--symbol", symbol, "--to-module", module, ...(name ? ["--to-name", name] : []), "--allow-breaking"];
}
function request(operations, flags = [], expected = 0) {
  const doc = { schemaVersion: "lekalo/change-request/v0.8.0", operations };
  if (expected === 0) assert(schemas.request(doc), JSON.stringify(schemas.request.errors));
  writeFileSync(join(stage, "request.json"), JSON.stringify(doc));
  return run(["change", "request", "--request", "request.json", ...flags], expected);
}
try {
  cpSync(join(root, "tests/fixtures/change/field-vertical"), stage, { recursive: true, filter: path => !/[\\/]\.lekalo$/.test(path) });
  const entities = join(stage, "lekalo/modules/tasks/entities.yaml");
  writeFileSync(entities, readFileSync(entities, "utf8").replace(/\r?\n/g, "\r\n")
    .replace("base: string\r\n", "base: string   \r\n")
    .replace("- title\r\n", "- title   \r\n"));
  const original = readFileSync(entities);
  const op = { operation: "symbol:move", symbol: "tasks.text", toModule: "tasks", toName: "label" };
  const args = sugar(op.symbol, op.toModule, op.toName);
  const preview = run(args);
  assert(readFileSync(entities).equals(original));
  assert(!existsSync(join(stage, "lekalo/change-history.json")));
  assert.equal(request([op], ["--allow-breaking"]).planId, preview.planId);
  request([{ ...op, toName: null }], [], 1);
  request([{ ...op, toModule: "tasks.other" }], [], 1);
  request([{ ...op, file: "elsewhere.yaml" }], [], 1);
  request([op, op], ["--allow-breaking"], 4);
  writeFileSync(join(stage, "module-api.json"), "");
  run(args, 4);
  rmSync(join(stage, "module-api.json"));
  const committed = run([...args, "--apply", "--idempotency-key", "symbol-first", "--expect-plan", preview.planId]);
  assert.equal(committed.state, "committed");
  assert.deepEqual(committed.files.map(file => file.path), ["lekalo/modules/tasks/entities.yaml", "lekalo/project.yaml"]);
  assert.match(readFileSync(entities, "utf8"), /type: "tasks.label"/);
  assert(readFileSync(entities, "utf8").includes("base: string   \r\n    renamed_from:"), "member insertion preserves the previous line's trailing spaces");
  assert(!readFileSync(entities, "utf8").replaceAll("\r\n", "").includes("\n"), "declaration insertion preserves CRLF");
  run([...sugar("tasks.label", "tasks", "title_text"), "--apply", "--idempotency-key", "symbol-second"]);
  assert.equal(run([...args, "--apply", "--idempotency-key", "symbol-first", "--expect-plan", preview.planId]).state, "replayed");
  run(sugar("tasks.title_text", "tasks", "text"), 1);
  const destination = join(stage, "lekalo/modules/physical-other");
  mkdirSync(destination);
  writeFileSync(join(destination, "module.yaml"), 'schema_version: "0.2.16"\ndefinitions:\n  - id: other\n    kind: module\n    version: 1\n');
  run([...sugar("tasks.task", "other", "item"), "--apply", "--idempotency-key", "symbol-cross"]);
  const moved = readFileSync(join(destination, "entities.yaml"), "utf8");
  assert.match(moved, /id: other.item/);
  assert.match(moved, /type: "tasks.title_text"/);
  assert(moved.includes("- title   \r\n"), "a relocated declaration keeps its internal trailing spaces");
  assert(!readFileSync(entities, "utf8").includes("id: tasks.task"));
  run(["validate"]);
  run(sugar("other.item", "tasks"), 4); // last definition needs retirement protocol
  console.log(JSON.stringify({ ok: true, calls, commits: 3, replay: true, schemas: 3 }));
} finally {
  rmSync(stage, { recursive: true, force: true });
}
