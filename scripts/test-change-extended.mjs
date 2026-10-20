#!/usr/bin/env node
// Exercise the additional M8 semantic verbs through the real CLI and the
// closed wire schemas. Fixtures and every mutation remain in a temporary copy.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, readdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const require = createRequire(import.meta.url);
assert.equal(require("ajv/package.json").version, "8.17.1");
const Ajv = require("ajv/dist/2020").default;
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const bin = process.env.LEKALO_BIN ?? join(root, "target/debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
const ajv = new Ajv({ strict: true, allErrors: true });
const schemas = Object.fromEntries(["plan", "receipt", "history"].map(name => [name, ajv.compile(JSON.parse(readFileSync(join(root, `contracts/change-${name}.schema.v0.8.0.json`), "utf8")))]));
const stage = mkdtempSync(join(tmpdir(), "lekalo-change-extended-"));
let calls = 0;
function run(args, expected = 0) {
  calls++;
  const result = spawnSync(bin, [...args, "--project", ".", "--json"], { cwd: stage, encoding: "utf8", timeout: 30000, env: { ...process.env, LEKALO_PROJECT: "" } });
  assert.equal(result.status, expected, result.stderr || result.stdout);
  const doc = JSON.parse(result.stdout.trim() || result.stderr.trim());
  if (expected === 0) {
    const validate = schemas[doc.schemaVersion.includes("change-plan") ? "plan" : "receipt"];
    assert(validate(doc), JSON.stringify(validate.errors));
  }
  return doc;
}
try {
  cpSync(join(root, "tests/fixtures/change/field-vertical"), stage, { recursive: true });
  const entities = join(stage, "lekalo/modules/tasks/entities.yaml");
  const queries = join(stage, "lekalo/modules/tasks/queries.yaml");
  writeFileSync(entities, readFileSync(entities, "utf8") + "  - id: tasks.note\n    kind: entity\n    version: 1\n    fields:\n      - name: title\n        type: tasks.text\n        required: true\n    identity:\n      - title\n");
  writeFileSync(queries, 'schema_version: "0.2.16"\ndefinitions:\n  - id: tasks.list_tasks\n    kind: query\n    version: 1\n    reads:\n      - tasks.task\n');
  const sugar = ["change", "requirement:link", "--symbol", "tasks.task", "--requirement", "TASK-REQ-001"];
  const before = readFileSync(entities);
  const preview = run([...sugar, "--dry-run"]);
  assert(readFileSync(entities).equals(before));
  writeFileSync(join(stage, "request.json"), JSON.stringify({ schemaVersion: "lekalo/change-request/v0.8.0", operations: [{ operation: "requirement:link", symbol: "tasks.task", requirement: "TASK-REQ-001" }] }));
  assert.equal(run(["change", "request", "--request", "request.json", "--dry-run"]).planId, preview.planId);
  assert.equal(run([...sugar, "--apply", "--idempotency-key", "req-link"]).state, "committed");
  assert.equal(run([...sugar, "--apply", "--idempotency-key", "req-link"]).state, "replayed");
  run(["change", "requirement:unlink", "--symbol", "tasks.task", "--requirement", "TASK-REQ-001", "--apply", "--idempotency-key", "req-unlink"]);
  assert(readFileSync(entities).equals(before), "link/unlink preserves exact source bytes");
  for (const [verb, key] of [["add", "read-add"], ["remove", "read-remove"]]) run(["change", `query-effect:${verb}`, "--query", "tasks.list_tasks", "--effect", "tasks.note", "--apply", "--idempotency-key", key, "--allow-breaking"]);
  const queryBefore = readFileSync(queries);
  run(["change", "query-effect:remove", "--query", "tasks.list_tasks", "--effect", "tasks.task", "--apply", "--idempotency-key", "invalid-last-read", "--allow-breaking"], 1);
  assert(readFileSync(queries).equals(queryBefore), "invalid candidate never writes");
  writeFileSync(join(stage, "policy.json"), JSON.stringify({ schemaVersion: "lekalo/change-policy/v0.8.0", identity: "dev.lekalo.change-policy@0.8.0", allowBreakingClasses: ["data-loss-risk", "source-breaking"], evidence: { migrationPlanDigest: "sha256:" + "a".repeat(64) } }));
  const migration = ["change", "migration:apply", "--step", "model.entity-field-rename.v0.8.0", "--parameters-json", JSON.stringify({ entity: "tasks.task", name: "title", to: "label" }), "--policy", "policy.json", "--allow-breaking"];
  const plan = run([...migration, "--dry-run"]);
  assert.equal(plan.operations[0].detail, "model.entity-field-rename.v0.8.0");
  const applied = run([...migration, "--apply", "--idempotency-key", "rename-step"]);
  assert.equal(applied.operations[0].operation, "migration:apply");
  assert.equal(applied.operations[0].detail, plan.operations[0].detail);
  assert.equal(run([...migration, "--apply", "--idempotency-key", "rename-step"]).state, "replayed");
  const after = readFileSync(entities);
  run(["change", "migration:apply", "--step", "unknown.recipe", "--dry-run"], 4);
  assert(readFileSync(entities).equals(after));
  const moduleBefore = readdirSync(join(stage, "lekalo/modules/tasks")).map(name => [name, readFileSync(join(stage, "lekalo/modules/tasks", name)).toString("hex")]);
  const move = ["change", "module:move", "--module", "tasks", "--directory", "task_sources"];
  const movePlan = run([...move, "--dry-run"]);
  assert.equal(movePlan.fileEdits.length, moduleBefore.length * 2);
  run([...move, "--apply", "--idempotency-key", "physical-move"]);
  assert.equal(existsSync(join(stage, "lekalo/modules/tasks")), false);
  assert.deepEqual(readdirSync(join(stage, "lekalo/modules/task_sources")).map(name => [name, readFileSync(join(stage, "lekalo/modules/task_sources", name)).toString("hex")]), moduleBefore);
  assert.equal(run([...move, "--apply", "--idempotency-key", "physical-move"]).state, "replayed");
  assert.equal(run([...migration, "--apply", "--idempotency-key", "rename-step"]).state, "replayed", "a semantic edit remains replayable after its physical source home moves");
  const history = JSON.parse(readFileSync(join(stage, "lekalo/change-history.json"), "utf8"));
  assert(schemas.history(history), JSON.stringify(schemas.history.errors));
  assert.equal(history.records.length, 6, "replays and refused operations add no history rows");
  console.log(JSON.stringify({ ok: true, gate: "change-extended", calls, committed: 6, verbs: 6 }));
} finally { rmSync(stage, { recursive: true, force: true }); }
