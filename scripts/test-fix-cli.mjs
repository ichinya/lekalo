#!/usr/bin/env node
// Issue #74 live gate. Rollback fault injection stays in the in-crate golden
// test: the production CLI intentionally exposes no failure-injection switch.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";

const require = createRequire(import.meta.url);
const Ajv2020 = require("ajv/dist/2020").default;
assert.equal(require("ajv/package.json").version, "8.20.0");
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const binary = join(root, "target/debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
assert(existsSync(binary), "cargo build -p lekalo-cli first");
const read = path => JSON.parse(readFileSync(join(root, path), "utf8"));
const ajv = new Ajv2020({ strict: true, allErrors: true });
for (const name of ["change-request", "change-plan", "change-receipt", "fix-recipe", "fix-result"]) ajv.addSchema(read(`contracts/${name}.schema.v0.8.0.json`));
ajv.addSchema(read("contracts/diagnostic.schema.v0.2.16.json"), "https://dev.lekalo/diagnostic.schema.v0.2.16.json");
const validate = ajv.getSchema("https://dev.lekalo/fix-result.schema.v0.8.0.json");
function run(args, expected = 0) {
  const child = spawnSync(binary, [...args, "--json"], { cwd: root, encoding: "utf8", timeout: 30000, maxBuffer: 4 * 1024 * 1024 });
  assert.equal(child.status, expected, `${args.join(" ")}\n${child.error ?? ""}\n${child.stdout}\n${child.stderr}`);
  const output = expected === 1 || expected === 5 ? child.stderr : child.stdout;
  assert.equal(expected === 1 || expected === 5 ? child.stdout : child.stderr, "", "status-owned stream");
  return JSON.parse(output);
}
const valid = value => assert(validate(value), JSON.stringify(validate.errors));
function snapshot(path) {
  return Object.fromEntries(readdirSync(path, { withFileTypes: true }).sort((a,b) => a.name < b.name ? -1 : 1).flatMap(entry => {
    const next = join(path, entry.name);
    return entry.isDirectory() ? Object.entries(snapshot(next)).map(([key, value]) => [`${entry.name}/${key}`, value]) : [[entry.name, readFileSync(next).toString("hex")]];
  }));
}
mkdirSync(join(root, "target"), { recursive: true });
const sandbox = mkdtempSync(join(root, "target/fix-cli-"));
const project = relative(root, sandbox).replaceAll("\\", "/");
cpSync(join(root, "tests/fixtures/fix/project"), sandbox, { recursive: true });
let checks = 0;
try {
  const list = run(["fix", "--list"]); valid(list);
  assert.equal(list.state, "listed"); assert.equal(list.recipes[0].preconditions.sourceFingerprint, null); checks++;
  const before = snapshot(join(sandbox, "lekalo"));
  const preview = run(["fix", "--safe", "--dry-run", "--project", project]); valid(preview);
  assert.deepEqual(preview, read("tests/fixtures/fix/golden/safe-preview.json"));
  assert.deepEqual(snapshot(join(sandbox, "lekalo")), before); assert(!existsSync(join(sandbox, ".lekalo"))); checks++;
  const selected = run(["fix", "LEK-FORMAT-001", "--dry-run", "--project", project]);
  assert.deepEqual(selected, preview); checks++;
  const human = spawnSync(binary, ["fix", "--safe", "--project", project], { cwd: root, encoding: "utf8", timeout: 30000 });
  assert.equal(human.status, 0); assert.match(human.stdout, /semantic equal: true/); assert.match(human.stdout, /--- lekalo\//); assert.match(human.stdout, /--apply --expect-plan/); checks++;
  const noConsent = run(["fix", "--safe", "--apply", "--project", project], 3);
  assert.deepEqual(noConsent.reasonCodes, ["fix.confirmation-required"]); assert.deepEqual(snapshot(join(sandbox, "lekalo")), before); checks++;
  const conflict = run(["fix", "--safe", "--apply", "--expect-plan", `sha256:${"0".repeat(64)}`, "--project", project], 3);
  assert.deepEqual(conflict.reasonCodes, ["fix.conflict"]); assert.deepEqual(snapshot(join(sandbox, "lekalo")), before); checks++;
  const recipePath = join(sandbox, "bound-recipe.json");
  writeFileSync(recipePath, JSON.stringify(preview.recipes[0]));
  const originalProject = readFileSync(join(sandbox, "lekalo/project.yaml"));
  writeFileSync(join(sandbox, "lekalo/project.yaml"), Buffer.concat([originalProject, Buffer.from("\n")]));
  const drifted = snapshot(join(sandbox, "lekalo"));
  const stale = run(["fix", "--recipe", recipePath, "--apply", "--expect-plan", preview.planId, "--project", project], 3);
  assert.deepEqual(stale.reasonCodes, ["fix.precondition-failed"]); assert.deepEqual(snapshot(join(sandbox, "lekalo")), drifted); assert(!existsSync(join(sandbox, "lekalo/change-history.json"))); checks++;
  writeFileSync(join(sandbox, "lekalo/project.yaml"), originalProject);
  const applied = run(["fix", "--safe", "--apply", "--expect-plan", preview.planId, "--project", project]); valid(applied);
  assert.equal(applied.state, "committed"); assert.equal(applied.postValidation, "passed"); assert.equal(applied.dryRun, false);
  const historyPath = join(sandbox, "lekalo/change-history.json");
  const pinPath = join(sandbox, ".lekalo/change-history-pin.json");
  assert.equal(JSON.parse(readFileSync(historyPath)).records.length, 1); assert(existsSync(pinPath)); checks++;
  const after = snapshot(join(sandbox, "lekalo")); const pin = readFileSync(pinPath);
  const replay = run(["fix", "--safe", "--apply", "--expect-plan", preview.planId, "--project", project]); valid(replay);
  assert.equal(replay.state, "replayed"); assert.equal(replay.receipt.replayed, true); assert.deepEqual(snapshot(join(sandbox, "lekalo")), after); assert.deepEqual(readFileSync(pinPath), pin); checks++;
  const noOp = run(["fix", "--safe", "--project", project]); valid(noOp);
  assert.equal(noOp.plan.fileEdits.length, 0); assert.equal(noOp.plan.semanticDiff.equal, true); assert.deepEqual(snapshot(join(sandbox, "lekalo")), after); checks++;
  // A safe executor must seal the same policy/acknowledgement that replay
  // compares, even though those options cannot authorize a semantic delta.
  const policyPath = join(sandbox, "policy.json");
  const policyBytes = JSON.stringify({ schemaVersion: "lekalo/change-policy/v0.8.0", identity: "dev.lekalo.change-policy@0.8.0", allowBreakingClasses: [] });
  writeFileSync(policyPath, policyBytes);
  const changedPolicyPath = join(sandbox, "changed-policy.json");
  writeFileSync(changedPolicyPath, `${policyBytes}\n`);
  const policyDigest = `sha256:${createHash("sha256").update(policyBytes).digest("hex")}`;
  for (const [name, selection, flags] of [
    ["policy", "--safe", ["--policy", policyPath]],
    ["acknowledgement", "LEK-FORMAT-001", ["--allow-breaking"]],
    ["policy-and-acknowledgement", "LEK-FORMAT-001", ["--policy", policyPath, "--allow-breaking"]],
  ]) {
    const optionRoot = join(sandbox, name);
    cpSync(join(root, "tests/fixtures/fix/project"), optionRoot, { recursive: true });
    const optionProject = relative(root, optionRoot).replaceAll("\\", "/");
    const baseArgs = ["fix", selection, ...flags, "--project", optionProject];
    const optionPreview = run(baseArgs); valid(optionPreview);
    const applyArgs = [...baseArgs, "--apply", "--expect-plan", optionPreview.planId];
    const optionApplied = run(applyArgs); valid(optionApplied);
    assert.equal(optionApplied.state, "committed");
    const optionAfter = snapshot(optionRoot);
    const optionReplay = run(applyArgs); valid(optionReplay);
    assert.equal(optionReplay.state, "replayed"); assert.equal(optionReplay.receipt.replayed, true);
    assert.deepEqual(snapshot(optionRoot), optionAfter, "same-command replay preserves sources, ledger and custody");
    assert.equal(optionApplied.plan.policyDigest, flags.includes("--policy") ? policyDigest : null);
    assert.equal(optionApplied.receipt.policy.policyDigest, optionApplied.plan.policyDigest);
    assert.equal(optionApplied.receipt.policy.allowBreaking, flags.includes("--allow-breaking"));
    for (const changedFlags of [
      [],
      ["--policy", changedPolicyPath, ...flags.filter(flag => flag === "--allow-breaking")],
      [...(flags.includes("--policy") ? ["--policy", policyPath] : []), ...(flags.includes("--allow-breaking") ? [] : ["--allow-breaking"])],
    ]) {
      const changed = run(["fix", "LEK-FORMAT-001", ...changedFlags, "--project", optionProject, "--apply", "--expect-plan", optionPreview.planId], 3);
      assert.deepEqual(changed.reasonCodes, ["fix.conflict"]);
      assert.deepEqual(snapshot(optionRoot), optionAfter, "different option intent cannot replay or mutate");
    }
    checks++;
  }
  // Unknown policy members and acknowledgements undeclared by this class
  // refuse before sealing AND before ledger-first replay, without writes.
  const refusalRoot = join(sandbox, "option-refusals");
  cpSync(join(root, "tests/fixtures/fix/project"), refusalRoot, { recursive: true });
  const refusalProject = relative(root, refusalRoot).replaceAll("\\", "/");
  const refusalPreview = run(["fix", "--safe", "--project", refusalProject]);
  const invalidPolicyPath = join(sandbox, "invalid-policy.json");
  writeFileSync(invalidPolicyPath, JSON.stringify({ ...JSON.parse(policyBytes), force: true }));
  for (const phase of ["before-commit", "after-commit"]) {
    const refusedBefore = snapshot(refusalRoot);
    for (const [flags, expected, reason] of [
      [["--allow-unsafe"], 3, "fix.class-denied"],
      [["--policy", invalidPolicyPath], 1, "change.request-invalid"],
    ]) {
      for (const mode of [[], ["--apply", "--expect-plan", refusalPreview.planId]]) {
        const refused = run(["fix", "LEK-FORMAT-001", ...flags, ...mode, "--project", refusalProject], expected);
        assert.deepEqual(refused.reasonCodes, [reason], phase);
        assert.deepEqual(snapshot(refusalRoot), refusedBefore, phase);
      }
      checks++;
    }
    if (phase === "before-commit") {
      assert(!existsSync(join(refusalRoot, ".lekalo")));
      assert.equal(run(["fix", "--safe", "--apply", "--expect-plan", refusalPreview.planId, "--project", refusalProject]).state, "committed");
    }
  }
  const unknown = run(["fix", "LEK-REF-014", "--dry-run", "--project", project], 4);
  assert.deepEqual(unknown.reasonCodes, ["fix.recipe-unavailable"]); checks++;
  const manual = structuredClone(preview.recipes[0]);
  Object.assign(manual, { id: "adapter.node-typescript.manual-resolution", owner: "adapter.node-typescript", target: "node-typescript", class: "manual-only", operation: { kind: "manual-only" }, scope: { operations: [], subjects: [] }, postValidation: "manual-only" });
  const manualPath = join(sandbox, "manual.json"); writeFileSync(manualPath, JSON.stringify(manual));
  const manualPreview = run(["fix", "--recipe", manualPath, "--project", "missing-project"]); valid(manualPreview); assert.equal(manualPreview.state, "manual-only");
  const manualApply = run(["fix", "--recipe", manualPath, "--apply", "--allow-unsafe", "--allow-breaking", "--project", "missing-project"], 3);
  assert.deepEqual(manualApply.reasonCodes, ["fix.manual-only"]); valid(manualApply.payload); assert.deepEqual(snapshot(join(sandbox, "lekalo")), after); checks++;
  const semantic = structuredClone(manual);
  Object.assign(semantic, { id: "adapter.node-typescript.add-field", class: "unsafe", diagnosticCodes: ["LEK-SEM-009"], operation: { kind: "model-change", request: { schemaVersion: "lekalo/change-request/v0.8.0", operations: [{ operation: "entity-field:add", entity: "tasks.task", field: { name: "extra", type: { ref: "tasks.text" } } }] } }, scope: { operations: ["entity-field:add"], subjects: ["tasks.task"] }, postValidation: "validate" });
  const semanticPath = join(sandbox, "semantic.json");
  for (const risk of ["unsafe", "breaking"]) {
    semantic.class = risk; writeFileSync(semanticPath, JSON.stringify(semantic));
    const denied = run(["fix", "--recipe", semanticPath, "--apply", "--expect-plan", preview.planId, "--project", project], 3);
    assert.deepEqual(denied.reasonCodes, ["fix.class-denied"]); checks++;
  }
  semantic.class = "unsafe"; semantic.scope.subjects = ["tasks.someone-else"]; writeFileSync(semanticPath, JSON.stringify(semantic));
  const outOfScope = run(["fix", "--recipe", semanticPath, "--dry-run", "--project", project], 3);
  assert.deepEqual(outOfScope.reasonCodes, ["fix.scope-denied"]); assert.deepEqual(snapshot(join(sandbox, "lekalo")), after); checks++;
  // A real adapter-owned semantic recipe repairs the originating validator
  // error, through only its declared operation, with explicit breaking consent.
  const repairRoot = join(sandbox, "adapter-project");
  cpSync(join(root, "tests/fixtures/fix/project"), repairRoot, { recursive: true });
  const repairProject = relative(root, repairRoot).replaceAll("\\", "/");
  const entityPath = join(repairRoot, "lekalo/modules/tasks/entities.yaml");
  writeFileSync(entityPath, readFileSync(entityPath, "utf8").replace("      - title", "      - extra"));
  const sha = bytes => `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
  // Recipe fingerprints cover Model documents; Change also seals audit and
  // optional registry metadata, including an absent registry's empty digest.
  const pins = preview.plan.inputs.filter(pin => !["lekalo/change-history.json", "lekalo/error-registry.json"].includes(pin.path)).map(pin => [pin.path, sha(readFileSync(join(repairRoot, pin.path)))]).sort((a,b) => a[0] < b[0] ? -1 : 1);
  semantic.scope.subjects = ["tasks.task"]; semantic.class = "breaking";
  semantic.preconditions.sourceFingerprint = sha(JSON.stringify(pins));
  semantic.explanation = "Declare the missing entity identity field through its exact scoped semantic operation.";
  writeFileSync(semanticPath, JSON.stringify(semantic));
  const repairPreview = run(["fix", "--recipe", semanticPath, "--allow-breaking", "--project", repairProject]); valid(repairPreview);
  assert.equal(repairPreview.plan.semanticDiff.equal, false); checks++;
  const repair = run(["fix", "--recipe", semanticPath, "--allow-breaking", "--apply", "--expect-plan", repairPreview.planId, "--project", repairProject]); valid(repair);
  assert.equal(repair.state, "committed"); assert.equal(repair.receipt.operations[0].operation, "entity-field:add"); assert.equal(run(["validate", "--project", repairProject]).status, "valid"); checks++;
  const repairedSources = snapshot(join(repairRoot, "lekalo"));
  const repairReplay = run(["fix", "--recipe", semanticPath, "--allow-breaking", "--apply", "--expect-plan", repairPreview.planId, "--project", repairProject]); valid(repairReplay);
  assert.equal(repairReplay.state, "replayed"); assert.deepEqual(snapshot(join(repairRoot, "lekalo")), repairedSources); checks++;
  semantic.version = "2.0.0"; writeFileSync(semanticPath, JSON.stringify(semantic));
  const changedVersion = run(["fix", "--recipe", semanticPath, "--allow-breaking", "--apply", "--expect-plan", repairPreview.planId, "--project", repairProject], 3);
  assert.deepEqual(changedVersion.reasonCodes, ["fix.conflict"]); assert.deepEqual(snapshot(join(repairRoot, "lekalo")), repairedSources); checks++;
  console.log(JSON.stringify({ ok: true, gate: "test-fix-cli", checks, safe: true, stale: true, conflict: true, replay: true, optionReplay: true, optionConflicts: true, optionAdmission: true, audit: true, riskGates: true, adapterScope: true, adapterRepair: true, recipeVersion: true }));
} finally { rmSync(sandbox, { recursive: true, force: true }); }
