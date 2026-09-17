/**
 * Issue #48 native contract validator tests: the closed shapes, enums,
 * and bounds of the four 0.3.2 native gate contracts, plus the digest
 * recomputation invariant. The JSON Schema side of the same documents is
 * exercised by scripts/test-native-gate-contracts.mjs (pinned Ajv).
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  recomputePlanDigest,
  RUN_SCHEMA_VERSION,
  validateNativePlan,
} from "../src/native-contract.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const fixtureRoot = join(repoRoot, "tests/fixtures/node-native-gates");

test("the committed plan golden validates against the closed validator", () => {
  const plan = JSON.parse(readFileSync(join(fixtureRoot, "protocol/plan.golden.json"), "utf8"));
  assert.equal(validateNativePlan(plan), true);
  assert.equal(recomputePlanDigest(plan), plan.plan_digest);
});

test("validators refuse unknown members, bad ids, and digest drift", () => {
  const plan = JSON.parse(readFileSync(join(fixtureRoot, "protocol/plan.golden.json"), "utf8"));

  // Unknown top-level member.
  const extra = JSON.parse(JSON.stringify(plan));
  extra.extra_member = true;
  assert.throws(() => validateNativePlan(extra), TypeError);

  // Wrong kind.
  const kind = JSON.parse(JSON.stringify(plan));
  kind.kind = "native-plan-v2";
  assert.throws(() => validateNativePlan(kind), TypeError);

  // Unknown gate kind on a command.
  const gate = JSON.parse(JSON.stringify(plan));
  gate.commands[0].gate = "deploy";
  assert.throws(() => validateNativePlan(gate), TypeError);

  // Digest drift: tampered content without recomputing the digest.
  const drift = JSON.parse(JSON.stringify(plan));
  drift.input_manifest_digest = "sha256:" + "9".repeat(64);
  assert.throws(() => validateNativePlan(drift), TypeError, "the recorded digest must match the content");

  // Absolute cwd is refused.
  const abs = JSON.parse(JSON.stringify(plan));
  abs.commands[0].cwd = "C:/Users/User/evil";
  assert.throws(() => validateNativePlan(abs), TypeError);

  // Unknown package id in affected.
  const unknown = JSON.parse(JSON.stringify(plan));
  unknown.affected[0].package_id = "packages/ghost=@fixture/ghost";
  assert.throws(() => validateNativePlan(unknown), TypeError);
});

test("run receipt schema version is pinned", () => {
  assert.equal(RUN_SCHEMA_VERSION, "lekalo/native-gate-run/v0.3.2");
});

test("the run result golden: never-executed commands carry unknown valueState", () => {
  const run = JSON.parse(readFileSync(join(fixtureRoot, "protocol/run-result.golden.json"), "utf8"));
  const blocked = run.commands.find((command) => command.command_id === "not-run-command");
  assert.equal(blocked.exit.state, "unknown", "a never-executed command never reports exit 0");
  assert.equal(blocked.duration_ms.state, "unknown", "a never-executed command never reports duration 0");
  assert.equal(blocked.output_ref.state, "withheld", "raw output stays withheld in portable evidence");
});
