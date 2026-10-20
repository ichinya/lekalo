#!/usr/bin/env node
// Existing managed owner, exact evidence, one policy scalar, real Change journal.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createRequire } from "node:module";
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
const stage = mkdtempSync(join(tmpdir(), "lekalo-change-ownership-"));
const source = join(stage, "lekalo/ownership.json");
const sha = bytes => "sha256:" + createHash("sha256").update(bytes).digest("hex");
const ajv = new Ajv({ strict: true, allErrors: true });
const validators = Object.fromEntries(["request", "plan", "receipt"].map(name => [name, ajv.compile(JSON.parse(readFileSync(join(root, `contracts/change-${name}.schema.v0.8.0.json`), "utf8")))]));
let calls = 0;
function run(args, expected = 0) {
  calls++;
  const result = spawnSync(bin, [...args, "--project", ".", "--json"], { cwd: stage, encoding: "utf8", timeout: 30000, env: { ...process.env, LEKALO_PROJECT: "" } });
  assert.equal(result.status, expected, result.stderr || result.stdout);
  const doc = JSON.parse(result.stdout.trim() || result.stderr.trim());
  if (expected === 0) {
    const validate = validators[doc.schemaVersion.includes("change-plan") ? "plan" : "receipt"];
    assert(validate(doc), JSON.stringify(validate.errors));
  }
  return doc;
}
function request(subject, mode, evidence, flags = [], expected = 0) {
  const doc = { schemaVersion: "lekalo/change-request/v0.8.0", operations: [{ operation: "ownership:set", subject, mode, evidence }] };
  if (expected === 0) assert(validators.request(doc), JSON.stringify(validators.request.errors));
  writeFileSync(join(stage, "request.json"), JSON.stringify(doc));
  return run(["change", "request", "--request", "request.json", ...flags], expected);
}
function sugar(mode, evidence) { return ["change", "ownership:set", "--module", "planner", "--mode", mode, "--evidence-json", JSON.stringify(evidence), "--allow-breaking"]; }
try {
  assert(existsSync(bin), "build the current checkout CLI first");
  cpSync(join(root, "tests/fixtures/managed/planner"), stage, { recursive: true });
  // Recompute authority from the current valid fixture snapshot.
  function load(ir) {
    calls++;
    const result = spawnSync(bin, ["load", "--project", ".", "--json", ...(ir ? ["--ir"] : [])], { cwd: stage, encoding: "utf8", timeout: 30000, env: { ...process.env, LEKALO_PROJECT: "" } });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout.trim();
  }
  const modelPayload = load(false);
  const irPayload = load(true);
  const irPrefix = `{"status":"valid","modelVersion":"${JSON.parse(irPayload).modelVersion}","ir":`;
  assert(irPayload.startsWith(irPrefix));
  const qualifiedCoverage = JSON.parse(readFileSync(join(stage, "coverage.json")));
  qualifiedCoverage.authority.value.model = sha(Buffer.from(modelPayload));
  qualifiedCoverage.authority.value.ir = sha(Buffer.from(irPayload.slice(irPrefix.length, -1)));
  writeFileSync(join(stage, "coverage.json"), JSON.stringify(qualifiedCoverage, null, 2) + "\n");
  const policy = JSON.parse(readFileSync(join(stage, "ownership.json")));
  policy.modules[0].mode = "observed";
  const legacy = structuredClone(policy.modules[0]);
  legacy.module = "legacy";
  legacy.targets[0].generatedRoot = "apps/legacy/gen";
  policy.modules.unshift(legacy);
  const before = Buffer.from(JSON.stringify(policy, null, 2) + "\n");
  const evidence = { ownershipDigest: sha(before) };
  request("planner", "contracted", evidence, ["--allow-breaking"], 3);
  assert(!existsSync(source), "absence never installs an owner seed");
  writeFileSync(source, before);
  const untouched = ["ownership.json", "adapters/managed.mjs", "model/planner.model.json", "lekalo/project.yaml", "lekalo/modules/planner/commands.yaml", "lekalo/modules/planner/queries.yaml"].map(path => [path, readFileSync(join(stage, path))]);
  request("planner", "contracted", evidence, [], 3);
  request("planner.focus_task", "contracted", evidence, ["--allow-breaking"], 4);
  request("planner", "contracted", { ...evidence, writePath: "outside.json" }, [], 1);
  request("planner", "managed", { ...evidence, coverage: [{ target: "node-typescript", path: "../coverage.json", digest: sha(before) }] }, [], 1);
  request("planner", "managed", evidence, ["--allow-breaking"], 4);
  const args = sugar("contracted", evidence);
  const preview = run(args);
  assert(readFileSync(source).equals(before));
  assert.equal(request("planner", "contracted", evidence, ["--allow-breaking"]).planId, preview.planId);
  assert.equal(preview.semanticDiff.changes[0].kind, "ownership.mode-changed");
  assert.equal(preview.impact.seeds[0].id, "planner");
  assert.equal(preview.inputs.find(pin => pin.path === "lekalo/ownership.json").digest, sha(before));
  const externallyChanged = structuredClone(policy);
  externallyChanged.modules[1].mode = "managed";
  const externalBytes = Buffer.from(JSON.stringify(externallyChanged));
  writeFileSync(source, externalBytes);
  run([...args, "--apply", "--idempotency-key", "ownership-contract", "--expect-plan", preview.planId], 3);
  assert(readFileSync(source).equals(externalBytes), "a stale request preserves the caller's policy edit");
  writeFileSync(source, before);
  const receipt = run([...args, "--apply", "--idempotency-key", "ownership-contract", "--expect-plan", preview.planId]);
  assert.equal(receipt.state, "committed");
  assert.deepEqual(receipt.files.map(file => file.path), ["lekalo/ownership.json"]);
  assert.equal(run([...args, "--apply", "--idempotency-key", "ownership-contract", "--expect-plan", preview.planId]).state, "replayed");
  let current = readFileSync(source);
  assert.equal(JSON.parse(current).modules[0].mode, "observed");
  assert.equal(JSON.parse(current).modules[1].mode, "contracted");
  assert.equal(request("planner", "contracted", { ownershipDigest: sha(current) }, ["--apply", "--idempotency-key", "ownership-noop"]).state, "no-op");
  const coverage = readFileSync(join(stage, "coverage.json"));
  const managedEvidence = { ownershipDigest: sha(current), coverage: [{ target: "node-typescript", path: "coverage.json", digest: sha(coverage) }] };
  const managedArgs = sugar("managed", managedEvidence);
  const managedPreview = run(managedArgs);
  assert(managedPreview.inputs.some(pin => pin.path === "adapters/managed.mjs"));
  writeFileSync(join(stage, "coverage.json"), Buffer.concat([coverage, Buffer.from("\n")]));
  run([...managedArgs, "--apply", "--idempotency-key", "ownership-managed", "--expect-plan", managedPreview.planId], 3);
  assert(readFileSync(source).equals(current));
  const partial = { ...JSON.parse(coverage), complete: false };
  const partialBytes = Buffer.from(JSON.stringify(partial));
  writeFileSync(join(stage, "coverage.json"), partialBytes);
  request("planner", "managed", { ...managedEvidence, coverage: [{ ...managedEvidence.coverage[0], digest: sha(partialBytes) }] }, ["--allow-breaking"], 4);
  writeFileSync(join(stage, "coverage.json"), coverage);
  const applied = run([...managedArgs, "--apply", "--idempotency-key", "ownership-managed", "--expect-plan", managedPreview.planId]);
  assert.equal(applied.state, "committed");
  assert.equal(JSON.parse(readFileSync(source)).modules[1].mode, "managed");
  assert(!existsSync(join(stage, "apps")), "policy mutation never generates native code");
  for (const [path, bytes] of untouched) assert(readFileSync(join(stage, path)).equals(bytes), path);
  console.log(JSON.stringify({ ok: true, calls, commits: 2, noOp: 1, managedOwnerAdmission: true, nativeWrites: 0 }));
} finally {
  rmSync(stage, { recursive: true, force: true });
}
