/**
 * #47 checked-binding suite (plan S8): the `lekalo:<id>` title
 * convention rides the scanner's observed `t` slot, and the scenario
 * verify joins native checked bindings against the observed index —
 * reporting missing, ambiguous, and stale bindings as typed findings,
 * never silently resolving them.
 */
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  BINDING_AMBIGUOUS,
  BINDING_MISSING,
  BINDING_MISMATCH,
  OBSERVED_INDEX_PATH,
  joinCheckedBindings,
} from "../src/scenario-gen.mjs";
import { lekaloTestIdsByModule } from "../src/scanner.mjs";

const FINGERPRINT = "sha256:" + "a".repeat(64);
const STALE = "sha256:" + "b".repeat(64);

function binding(test, extra = {}) {
  return {
    backend: "native",
    runner: "node:test",
    runnerVersion: "24.0.0",
    capabilities: [],
    capabilityDigest: "sha256:" + "0".repeat(64),
    test,
    mode: "checked",
    ...extra,
  };
}

function indexDocument(records) {
  return {
    schema_version: "lekalo/observed-index/v0.2.16",
    test_bindings: records,
  };
}

test("a checked binding joins a matching scanned test with no findings", () => {
  const scenario = {
    bindings: [binding("planner.scenario.focus_happy", { evidenceDigest: FINGERPRINT })],
  };
  const index = indexDocument([
    {
      id: "src/tests/focus.test.ts#lekalo:planner.scenario.focus_happy",
      symbol: "planner.focus_task",
      path: "src/tests/focus.test.ts",
      fingerprint: FINGERPRINT,
      state: "current",
    },
  ]);
  assert.deepEqual(joinCheckedBindings(scenario, index), []);
});

test("a binding with no scanned test is missing", () => {
  const scenario = { bindings: [binding("planner.scenario.mystery")] };
  const index = indexDocument([
    {
      id: "src/tests/focus.test.ts#lekalo:planner.scenario.focus_happy",
      symbol: "planner.focus_task",
      path: "src/tests/focus.test.ts",
      fingerprint: FINGERPRINT,
    },
  ]);
  const findings = joinCheckedBindings(scenario, index);
  assert.equal(findings.length, 1);
  assert.equal(findings[0].code, BINDING_MISSING);
  assert.equal(findings[0].symbol, "planner.scenario.mystery");
});

test("an id claimed by two tests is ambiguous, never silently resolved", () => {
  const scenario = { bindings: [binding("planner.scenario.focus_happy")] };
  const index = indexDocument([
    {
      id: "src/tests/a.test.ts#lekalo:planner.scenario.focus_happy",
      symbol: "planner.focus_task",
      path: "src/tests/a.test.ts",
      fingerprint: FINGERPRINT,
    },
    {
      id: "src/tests/b.test.ts#lekalo:planner.scenario.focus_happy",
      symbol: "planner.focus_task",
      path: "src/tests/b.test.ts",
      fingerprint: STALE,
    },
  ]);
  const findings = joinCheckedBindings(scenario, index);
  assert.equal(findings[0].code, BINDING_AMBIGUOUS);
  assert.equal(findings[0].detail, "claimed-by-2-tests");
});

test("one test claiming several scenario identities is ambiguous", () => {
  const scenario = { bindings: [binding("planner.scenario.focus_happy")] };
  const index = indexDocument([
    {
      id: "src/tests/both.test.ts#lekalo:planner.scenario.focus_happy,lekalo:planner.scenario.focus_error",
      symbol: "planner.focus_task",
      path: "src/tests/both.test.ts",
      fingerprint: FINGERPRINT,
    },
  ]);
  const findings = joinCheckedBindings(scenario, index);
  assert.equal(findings[0].code, BINDING_AMBIGUOUS);
  assert.equal(findings[0].detail, "test-claims-several-ids");
});

test("a stale evidence digest is a mismatch, never a silent rewrite", () => {
  const scenario = {
    bindings: [binding("planner.scenario.focus_happy", { evidenceDigest: STALE })],
  };
  const index = indexDocument([
    {
      id: "src/tests/focus.test.ts#lekalo:planner.scenario.focus_happy",
      symbol: "planner.focus_task",
      path: "src/tests/focus.test.ts",
      fingerprint: FINGERPRINT,
    },
  ]);
  const findings = joinCheckedBindings(scenario, index);
  assert.equal(findings.length, 1);
  assert.equal(findings[0].code, BINDING_MISMATCH);
  assert.equal(findings[0].detail, "stale-evidence-digest");
});

test("generated and fake-reference bindings never join", () => {
  const scenario = {
    bindings: [
      binding("planner.scenario.focus_happy", { mode: "generated" }),
      {
        backend: "fake-reference",
        runner: "node:test",
        runnerVersion: "24.0.0",
        capabilities: [],
        capabilityDigest: "sha256:" + "0".repeat(64),
        test: "planner.scenario.focus_happy",
        mode: "checked",
      },
    ],
  };
  assert.deepEqual(joinCheckedBindings(scenario, indexDocument([])), []);
});

test("an absent observed index is legal absence: the join says nothing", () => {
  const scenario = { bindings: [binding("planner.scenario.focus_happy")] };
  assert.deepEqual(joinCheckedBindings(scenario, null), []);
  assert.deepEqual(joinCheckedBindings(scenario, {}), []);
});

test("the scanner groups lekalo test titles per module with bounds", () => {
  const grouped = lekaloTestIdsByModule([
    { path: "src/tests/focus.test.ts", name: "lekalo:planner.scenario.focus_happy" },
    { path: "src/tests/focus.test.ts", name: "lekalo:planner.scenario.focus_error" },
    { path: "src/tests/focus.test.ts", name: "lekalo:planner.scenario.focus_happy" },
    { path: "src/tests/other.test.ts", name: "not-a-lekalo-title" },
    { path: "src/tests/edge.test.ts", name: "lekalo:" },
  ]);
  assert.deepEqual(grouped.get("src/tests/focus.test.ts"), [
    "planner.scenario.focus_error",
    "planner.scenario.focus_happy",
  ]);
  assert.equal(grouped.has("src/tests/other.test.ts"), false);
  assert.equal(grouped.has("src/tests/edge.test.ts"), false);
});

test("the observed index path pin holds", () => {
  assert.equal(OBSERVED_INDEX_PATH, ".lekalo/import/observed/index.json");
});
