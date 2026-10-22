#!/usr/bin/env node
// Closed binding payloads, exact evidence, transactional publication and replay.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const require = createRequire(import.meta.url);
assert.equal(require("ajv/package.json").version, "8.20.0");
const Ajv = require("ajv/dist/2020").default;
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const bin = process.env.LEKALO_BIN ?? join(root, "target/debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
assert(existsSync(bin), "build the checkout CLI before the live gate");
const ajv = new Ajv({ strict: true, allErrors: true });
const validators = Object.fromEntries(["request", "plan", "receipt", "history"].map(name => [name, ajv.compile(JSON.parse(readFileSync(join(root, `contracts/change-${name}.schema.v0.8.0.json`), "utf8")))]));
const stage = mkdtempSync(join(tmpdir(), "lekalo-change-bindings-"));
const sha = bytes => "sha256:" + createHash("sha256").update(bytes).digest("hex");
let calls = 0;
function run(cwd, args, expected = 0) {
  calls++;
  const result = spawnSync(bin, [...args, "--project", ".", "--json"], { cwd, encoding: "utf8", timeout: 30000, env: { ...process.env, LEKALO_PROJECT: "" } });
  assert.equal(result.status, expected, result.stderr || result.stdout);
  const doc = JSON.parse(result.stdout.trim() || result.stderr.trim());
  if (expected === 0 && args[0] === "change") {
    const validate = validators[doc.schemaVersion.includes("change-plan") ? "plan" : "receipt"];
    assert(validate(doc), JSON.stringify(validate.errors));
  }
  return doc;
}
function request(cwd, symbol, definition, expected = 0, flags = []) {
  const doc = { schemaVersion: "lekalo/change-request/v0.8.0", operations: [{ operation: "binding:add", symbol, definition }] };
  if (expected === 0) assert(validators.request(doc), JSON.stringify(validators.request.errors));
  writeFileSync(join(cwd, "request.json"), JSON.stringify(doc));
  return run(cwd, ["change", "request", "--request", "request.json", ...flags], expected);
}
try {
  const canonical = join(stage, "canonical");
  cpSync(join(root, "tests/fixtures/change/field-vertical"), canonical, { recursive: true, filter: path => !/[\\/]\.lekalo$/.test(path) });
  mkdirSync(join(canonical, "lekalo/targets"), { recursive: true });
  writeFileSync(join(canonical, "lekalo/targets/node-typescript.yaml"), "target: node-typescript\n");
  const symbol = "tasks.binding_node";
  const definition = { kind: "canonical-target-binding", target: "node-typescript", description: "Explicit binding" };
  const sugar = ["change", "binding:add", "--symbol", symbol, "--definition-json", JSON.stringify(definition)];
  const preview = run(canonical, [...sugar, "--dry-run"]);
  assert.equal(request(canonical, symbol, definition).planId, preview.planId);
  const home = join(canonical, "lekalo/modules/tasks/bindings.yaml");
  assert(!existsSync(home));
  request(canonical, symbol, { ...definition, target: "unknown" }, 3);
  request(canonical, symbol, { ...definition, path: "src/anything.ts" }, 1);
  run(canonical, [...sugar, "--apply", "--idempotency-key", "canonical"]);
  assert(existsSync(home));
  assert.equal(run(canonical, [...sugar, "--apply", "--idempotency-key", "canonical"]).state, "replayed");

  const observed = join(stage, "observed");
  cpSync(join(root, "tests/fixtures/bindings/app"), observed, { recursive: true });
  const scan = JSON.parse(readFileSync(join(root, "tests/fixtures/bindings/scans/app-ambiguous.scan.v1_1_0.json"), "utf8"));
  for (const symbol of scan.symbols) {
    if (symbol.location && existsSync(join(observed, symbol.location.path))) symbol.fingerprint = sha(readFileSync(join(observed, symbol.location.path)));
    for (const candidate of symbol.candidates ?? []) if (existsSync(join(observed, candidate.path))) candidate.fingerprint = sha(readFileSync(join(observed, candidate.path)));
  }
  writeFileSync(join(observed, "scan.json"), JSON.stringify(scan));
  run(observed, ["observe", "update", "--scan", "scan.json"]);
  const proposals = run(observed, ["bindings", "propose"]).proposals;
  const offer = proposals.find(row => row.symbol === "taskboard.focus_task");
  assert(offer.ambiguous);
  const indexPath = join(observed, ".lekalo/import/observed/index.json");
  const historyPath = join(observed, "lekalo/change-history.json");
  const before = readFileSync(indexPath);
  const original = JSON.parse(before);
  const payload = { kind: "observed-confirmation", proposal: offer.proposal, candidate: "src/tasks.ts#focusTask", indexDigest: sha(before) };
  const plan = request(observed, offer.symbol, payload);
  assert(readFileSync(indexPath).equals(before));
  request(observed, offer.symbol, { ...payload, candidate: "unrecorded-native" }, 3);
  request(observed, offer.symbol, { ...payload, indexDigest: "sha256:" + "0".repeat(64) }, 3);
  request(observed, offer.symbol, { ...payload, candidate: null }, 1);
  const nativePath = join(observed, "src/tasks.ts");
  const native = readFileSync(nativePath);
  writeFileSync(nativePath, Buffer.concat([native, Buffer.from("// later edit\n")]));
  request(observed, offer.symbol, payload, 3, ["--apply", "--idempotency-key", "confirm"]);
  assert(!existsSync(historyPath));
  assert(readFileSync(indexPath).equals(before));
  writeFileSync(nativePath, native);
  const flags = ["--apply", "--idempotency-key", "confirm", "--expect-plan", plan.planId];
  const committed = request(observed, offer.symbol, payload, 0, flags);
  assert.equal(committed.files.length, 1);
  assert.equal(committed.files[0].path, ".lekalo/import/observed/index.json");
  const confirmed = JSON.parse(readFileSync(indexPath));
  const record = confirmed.symbols.find(row => row.id === offer.symbol);
  assert.equal(record.status, "confirmed");
  assert.deepEqual(record.provenance, original.symbols.find(row => row.id === offer.symbol).provenance);
  assert.equal(record.stable_key, payload.candidate);
  assert(readFileSync(nativePath).equals(native));
  assert.equal(request(observed, offer.symbol, payload, 0, flags).state, "replayed");
  const second = run(observed, ["bindings", "propose"]).proposals.find(row => row.symbol === "taskboard.task");
  const secondPayload = { kind: "observed-confirmation", proposal: second.proposal, candidate: second.candidates[0].native, indexDigest: sha(readFileSync(indexPath)) };
  request(observed, second.symbol, secondPayload, 0, ["--apply", "--idempotency-key", "confirm-second"]);
  assert.equal(request(observed, offer.symbol, payload, 0, flags).state, "replayed", "later committed confirmations retain the old chain endpoint proof");
  const history = JSON.parse(readFileSync(historyPath));
  assert(validators.history(history), JSON.stringify(validators.history.errors));
  assert.equal(history.records.length, 2);
  console.log(JSON.stringify({ ok: true, gate: "change-bindings", calls, committed: 3, ambiguousExplicitSelection: true }));
} finally { rmSync(stage, { recursive: true, force: true }); }
