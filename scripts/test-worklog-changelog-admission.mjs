#!/usr/bin/env node
// Issue #51 r6: production-boundary regressions for backend eligibility,
// closed corpus admission, and the real journal/application transitions.
// Disposable copies only; no network, installation, or real provider calls.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fixture = "tests/fixtures/pilot/worklog-changelog";
const binary = process.env.LEKALO_BIN ?? resolve(root, existsSync(join(root, "target/debug/lekalo.exe")) ? "target/debug/lekalo.exe" : "target/debug/lekalo");
const php = (process.env.LEKALO_PHP ?? "php").split(";").find((candidate) =>
  spawnSync(candidate, ["-v"], { encoding: "utf8" }).status === 0);
assert.ok(php, "native interpreter required");
const require = createRequire(import.meta.url);
const { default: Ajv2020 } = require("ajv/dist/2020");
assert.equal(require("ajv/package.json").version, "8.17.1");
const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateScenario = ajv.compile(JSON.parse(readFileSync(join(root, "contracts/scenario-ir.schema.v0.2.16.json"), "utf8")));
const execute = (command, args, cwd = root, env = process.env) => {
  const result = spawnSync(command, args, { cwd, env, encoding: "utf8", timeout: 120_000, maxBuffer: 64 * 1024 * 1024 });
  assert.ifError(result.error);
  return result;
};
const json = (path) => JSON.parse(readFileSync(path, "utf8"));
const sha = (bytes) => "sha256:" + createHash("sha256").update(bytes).digest("hex");
const cases = [];
const temp = mkdtempSync(join(tmpdir(), "lekalo-admission-regression-"));
try {
  const native = execute(php, [join(root, "tests/pilot/worklog-changelog-journal.php")]);
  assert.equal(native.status, 0, native.stderr);
  cases.push(...JSON.parse(native.stdout).cases);

  const honest = execute(php, [join(root, fixture, "php/run.php")]);
  assert.equal(honest.status, 0, honest.stderr);
  const lane = JSON.parse(honest.stdout);
  const caseRoot = join(temp, "fixture");
  cpSync(join(root, fixture), caseRoot, { recursive: true, filter: (path) => !path.replaceAll("\\", "/").includes("/.lekalo") });
  const report = json(join(caseRoot, "evidence/evidence.json"));
  Object.assign(report.targets.find((row) => row.target === "php"), {
    backend: lane.backend, execution: "executed", capabilities: lane.capabilities,
    assertions: lane.assertions.map(({ scenario, status }) => ({ scenario, status })), blocking: [],
  });
  const reportPath = join(caseRoot, "evidence/live.json");
  writeFileSync(reportPath, JSON.stringify(report));
  const admit = () => {
    const result = execute(binary, ["--json", "--no-cache", "pilot", "admit", reportPath,
      join(caseRoot, "attachments/pilot.json"), "--profile", "strict", "--project", "model"], caseRoot);
    assert.equal(result.status, 4, result.stderr + result.stdout);
    const envelope = JSON.parse(result.stdout);
    const receipt = envelope.pilot ?? envelope.payload?.pilot;
    assert.ok(receipt, "unsupported receipt retained");
    return { ...receipt.targets.find((row) => row.target === "php"), blockers: receipt.blockers };
  };
  assert.equal(admit().verdict, "pass", "honest reviewed backend control");
  cases.push("F2-reviewed-backend-control");

  // F5: compare exact frozen schema refusal with standalone production
  // admission. No separate contracts gate is needed to refuse the corpus.
  const scenarioPath = join(caseRoot, "scenarios/worklog.scenario.completion_without_worklog.scenario.json");
  const originalBytes = readFileSync(scenarioPath);
  const original = JSON.parse(originalBytes);
  for (const mutation of ["missing-when", "missing-then", "empty-when", "empty-then", "version", "rogue", "ref-version", "ref-shape"]) {
    const document = structuredClone(original);
    switch (mutation) {
      case "missing-when": delete document.when; break;
      case "missing-then": delete document.then; break;
      case "empty-when": document.when = []; break;
      case "empty-then": document.then = []; break;
      case "version": document.schemaVersion = "lekalo/scenario-ir/v9.9.9"; break;
      case "rogue": document.rogue = true; break;
      case "ref-version": document.modelRef.modelVersion = "9.9.9"; break;
      case "ref-shape": document.irRef.rogue = true; break;
    }
    assert.equal(validateScenario(document), false, "frozen schema: " + mutation);
    writeFileSync(scenarioPath, JSON.stringify(document));
    const target = admit();
    assert.equal(target.verdict, "unsupported", mutation);
    assert.ok(target.blockers.includes("php:execution:battery-scenario-invalid"), JSON.stringify(target));
    cases.push("F5-" + mutation);
  }
  writeFileSync(scenarioPath, originalBytes);
  assert.equal(admit().verdict, "pass", "corpus restored control");

  // F2: this printer has no stores, operations, renderer or transport.
  // It can answer the public battery and even reproduce the full gate's
  // standalone facts, but it is not the engine's reviewed implementation.
  const captured = Buffer.from(JSON.stringify(lane)).toString("base64");
  const fabricator = `<?php
$lane = json_decode(base64_decode('${captured}'), true);
file_put_contents(__DIR__ . '/printer-executed', 'yes');
$lane['backend']['digest'] = 'sha256:' . hash_file('sha256', __FILE__);
if ($argc > 1) {
    $raw = file_get_contents($argv[1]);
    $battery = json_decode($raw, true);
    $lane['battery'] = ['digest' => 'sha256:' . hash('sha256', $raw), 'nonce' => $battery['nonce']];
    foreach ($battery['corpus'] as $doc) {
        $lane['arrangements'][$doc['scenarioId']] = ['given' => $doc['given'], 'verified' => true];
        foreach ($lane['assertions'] as &$row) {
            if ($row['scenario'] !== $doc['scenarioId']) continue;
            $row['invocations'] = [];
            foreach ($doc['when'] as $step) {
                $input = [];
                foreach ($step['action']['input'] as $field => $cell) $input[$field] = $cell['value'] ?? null;
                $row['invocations'][] = ['operation' => $step['action']['operation'], 'input' => $input];
            }
        }
        unset($row);
    }
    $lane['probe'] = ['kind' => 'entity_state', 'entity' => 'changelog.entry', 'rows' => [[
        'entry_id' => $battery['probe']['entryId'], 'draft_body' => $battery['probe']['body'], 'record_revision' => 1,
    ]]];
}
echo json_encode($lane), PHP_EOL;
`;
  const lanePath = join(caseRoot, "php/run.php");
  writeFileSync(lanePath, fabricator);
  report.targets.find((row) => row.target === "php").backend.digest = sha(Buffer.from(fabricator));
  writeFileSync(reportPath, JSON.stringify(report));
  const refused = admit();
  assert.equal(refused.verdict, "unsupported");
  assert.ok(refused.blockers.includes("php:backend:provenance"), JSON.stringify(refused));
  assert.ok(refused.blockers.includes("php:execution:backend-untrusted"), JSON.stringify(refused));
  assert.equal(existsSync(join(caseRoot, "php/printer-executed")), false, "untrusted program not launched by admission");
  cases.push("F2-battery-aware-fabricator-admission");

  // Ensure this is an adaptive printer, not just the already-refused
  // unchanged-capture control: it mechanically answers a fresh battery.
  const nonce = "fresh-regression-nonce";
  const battery = { nonce, corpus: [{ scenarioId: original.scenarioId, given: original.given, when: original.when, then: original.then }],
    probe: { entryId: "battery-probe-" + nonce, body: "battery probe body " + nonce } };
  const batteryPath = join(temp, "battery.json");
  writeFileSync(batteryPath, JSON.stringify(battery));
  const fabricated = execute(php, [lanePath, batteryPath]);
  assert.equal(fabricated.status, 0, fabricated.stderr);
  const answered = JSON.parse(fabricated.stdout);
  assert.equal(answered.battery.nonce, nonce);
  assert.equal(answered.battery.digest, sha(readFileSync(batteryPath)));
  assert.equal(answered.probe.rows[0].draft_body, battery.probe.body);
  cases.push("F2-adaptive-printer-control");

  // The complete shipped strict gate must now refuse this same printer,
  // after its standalone facts pass all the gate's earlier checks.
  const gateRoot = join(temp, "gate");
  mkdirSync(join(gateRoot, "scripts"), { recursive: true });
  mkdirSync(join(gateRoot, "contracts"), { recursive: true });
  cpSync(caseRoot, join(gateRoot, fixture), { recursive: true });
  cpSync(join(root, "scripts/test-worklog-changelog-pilot.mjs"), join(gateRoot, "scripts/test-worklog-changelog-pilot.mjs"));
  cpSync(join(root, "contracts/worklog-changelog-evidence.schema.v0.8.0.json"), join(gateRoot, "contracts/worklog-changelog-evidence.schema.v0.8.0.json"));
  // createRequire in a disposable copy still resolves the exact pinned
  // validator via the harness's external dependency directory.
  const moduleRoot = dirname(dirname(require.resolve("ajv/package.json")));
  const gate = execute(process.execPath, [join(gateRoot, "scripts/test-worklog-changelog-pilot.mjs"),
    "--target", "php", "--profile", "strict", "--case", "all"], gateRoot,
    { ...process.env, LEKALO_BIN: binary, LEKALO_PHP: php, NODE_PATH: moduleRoot });
  assert.equal(gate.status, 1, gate.stderr);
  assert.match(gate.stderr, /required-evidence-unsupported/);
  assert.match(gate.stderr, /"receiptVerdict": "unsupported"/);
  cases.push("F2-battery-aware-fabricator-full-strict-gate");
  process.stdout.write(JSON.stringify({ ok: true, gate: "test-worklog-changelog-admission", cases }) + "\n");
} finally {
  rmSync(temp, { recursive: true, force: true });
}
