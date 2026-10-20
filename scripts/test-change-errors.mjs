#!/usr/bin/env node
// Owner revisions, policy, closed requests, transactional publication and replay.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const require = createRequire(import.meta.url);
assert.equal(require("ajv/package.json").version, "8.17.1");
const Ajv = require("ajv/dist/2020").default;
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const bin = process.env.LEKALO_BIN ?? join(root, "target/debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
assert(existsSync(bin), "build the checkout CLI before the live gate");
const ajv = new Ajv({ strict: true, allErrors: true });
const validators = Object.fromEntries(["request", "plan", "receipt", "history"].map(name => [name, ajv.compile(JSON.parse(readFileSync(join(root, `contracts/change-${name}.schema.v0.8.0.json`), "utf8")))]));
const stage = mkdtempSync(join(tmpdir(), "lekalo-change-errors-"));
const sha = bytes => "sha256:" + createHash("sha256").update(bytes).digest("hex");
const home = join(stage, "lekalo/error-registry.json");
const historyPath = join(stage, "lekalo/change-history.json");
let calls = 0;
function run(args, expected = 0) {
  calls++;
  const result = spawnSync(bin, [...args, "--project", ".", "--json"], { cwd: stage, encoding: "utf8", timeout: 30000, env: { ...process.env, LEKALO_PROJECT: "" } });
  assert.equal(result.status, expected, result.stderr || result.stdout);
  const doc = JSON.parse(result.stdout.trim() || result.stderr.trim());
  if (expected === 0 && args[0] === "change") {
    const validate = validators[doc.schemaVersion.includes("change-plan") ? "plan" : "receipt"];
    assert(validate(doc), JSON.stringify(validate.errors));
  }
  return doc;
}
function request(operationTarget, definition, flags = [], expected = 0) {
  const doc = { schemaVersion: "lekalo/change-request/v0.8.0", operations: [{ operation: "operation-error:set", operationTarget, definition }] };
  if (expected === 0) assert(validators.request(doc), JSON.stringify(validators.request.errors));
  writeFileSync(join(stage, "request.json"), JSON.stringify(doc));
  return run(["change", "request", "--request", "request.json", ...flags], expected);
}
try {
  cpSync(join(root, "tests/fixtures/model-v1/valid-planner"), stage, { recursive: true });
  const before = readFileSync(join(root, "contracts/error-registry.v0.2.16.json"));
  const target = "planner.focus_task";
  const errors = JSON.parse(before).bindings.find(binding => binding.operation === target).errors.filter(id => id !== "planner.focus_conflict");
  const definition = { errors, registryDigest: sha(before) };
  request(target, definition, ["--allow-breaking"], 3);
  assert(!existsSync(home), "absence never installs the global planner seed");
  writeFileSync(home, before);
  request(target, definition, [], 3);
  request(target, { ...definition, registryDigest: "sha256:" + "0".repeat(64) }, ["--allow-breaking"], 3);
  for (const replacement of [{ errors: [] }, { errors: [errors[0], errors[0]] }, { path: "outside.json" }]) request(target, { ...definition, ...replacement }, [], 1);
  request(target, { ...definition, errors: ["planner.undeclared"] }, ["--allow-breaking"], 1);
  assert(readFileSync(home).equals(before));
  assert(!existsSync(historyPath));
  const sugar = ["change", "operation-error:set", "--operation", target, "--definition-json", JSON.stringify(definition), "--allow-breaking"];
  const preview = run([...sugar, "--dry-run"]);
  assert.equal(request(target, { ...definition, errors: [...errors].reverse() }, ["--allow-breaking"]).planId, preview.planId);
  assert.deepEqual(preview.semanticDiff.classification, ["source-breaking"]);
  assert.equal(preview.semanticDiff.changes[0].kind, "error.union-member-removed");
  assert.equal(preview.inputs.find(pin => pin.path === "lekalo/error-registry.json").digest, sha(before));
  const receipt = run([...sugar, "--apply", "--idempotency-key", "error-first", "--expect-plan", preview.planId]);
  assert.equal(receipt.state, "committed");
  assert.deepEqual(receipt.files.map(file => file.path), ["lekalo/error-registry.json"]);
  const after = readFileSync(home);
  assert.deepEqual(JSON.parse(after).bindings.find(binding => binding.operation === target).errors, errors);
  run([...sugar, "--apply", "--idempotency-key", "error-first"], 3);
  assert.equal(run([...sugar, "--apply", "--idempotency-key", "error-first", "--expect-plan", preview.planId]).state, "replayed");
  const current = { errors, registryDigest: sha(after) };
  assert.equal(request(target, current).semanticDiff.equal, true);
  assert.equal(request(target, current, ["--apply", "--idempotency-key", "error-noop"]).state, "no-op");
  request(target, definition, ["--allow-breaking"], 3);
  const queryBefore = JSON.parse(after).bindings.find(binding => binding.operation === "planner.list_tasks");
  request("planner.list_tasks", { errors: ["planner.store_unavailable"], registryDigest: sha(after) }, ["--allow-breaking", "--apply", "--idempotency-key", "query-error"]);
  assert.deepEqual(JSON.parse(readFileSync(home)).bindings.find(binding => binding.operation === "planner.list_tasks").output, queryBefore.output);
  const queriesPath = join(stage, "lekalo/modules/planner/queries.yaml");
  const queries = JSON.parse(readFileSync(queriesPath));
  queries.definitions.push({ id: "planner.error_probe", kind: "query", version: 1, reads: ["planner.task"], returns: { list: { ref: "planner.task" } } });
  writeFileSync(queriesPath, JSON.stringify(queries));
  const newBinding = { errors: ["planner.store_unavailable"], registryDigest: sha(readFileSync(home)) };
  const additive = request("planner.error_probe", newBinding);
  assert.deepEqual(additive.semanticDiff.classification, ["additive"]);
  assert.equal(additive.semanticDiff.changes[0].kind, "error.binding-added");
  request("planner.error_probe", newBinding, ["--apply", "--idempotency-key", "new-binding"]);
  assert.equal(run(["validate"]).status, "valid");
  assert(JSON.stringify(run(["context-budget", "--symbol", "planner.error_probe", "--budget", "100000"])).includes("error-contracts-unrepresentable"));
  assert.equal(run([...sugar, "--apply", "--idempotency-key", "error-first", "--expect-plan", preview.planId]).state, "replayed");
  const history = JSON.parse(readFileSync(historyPath));
  assert(validators.history(history), JSON.stringify(validators.history.errors));
  assert.equal(history.records.length, 4);
  console.log(JSON.stringify({ ok: true, gate: "change-errors", calls, committed: 3, noOp: 1, replay: true, ownerOutputPreserved: true, localConsumers: 2 }));
} finally { rmSync(stage, { recursive: true, force: true }); }
