/**
 * #45 zod policy + findings suite: the closed target-document policy
 * grammar and the unsupported-construct finding classification against
 * the committed fixture golden (acceptance criterion 4).
 */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { test } from "node:test";

import { DEFAULT_POLICY } from "../src/zod-map.mjs";
import { mapProject } from "../src/zod-map.mjs";
import {
  MAX_POLICY_BYTES,
  POLICY_PATH,
  resolvePolicy,
} from "../src/zod-policy.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const FIXTURE_DIR = join(repoRoot, "tests", "fixtures", "ir", "zod-unsupported");

// ---------------------------------------------------------------------------
// Policy resolution.
// ---------------------------------------------------------------------------

test("the policy path is the documented target document", () => {
  assert.equal(POLICY_PATH, "lekalo/targets/node-typescript.yaml");
  assert.equal(MAX_POLICY_BYTES, 16 * 1024);
});

test("an absent document resolves the documented defaults", () => {
  const resolved = resolvePolicy(null);
  assert.deepEqual(resolved.policy, DEFAULT_POLICY);
  assert.equal(resolved.source, "defaults");
  assert.equal(resolved.refusal, undefined);
});

test("a well-formed document overrides exactly the declared keys", () => {
  const resolved = resolvePolicy(
    [
      "# generation policy",
      "zod:",
      "  date: date-native",
      "",
    ].join("\n"),
  );
  assert.deepEqual(resolved.policy, { date: "date-native", unknownKeys: "strict" });
  assert.equal(resolved.source, "document");
});

test("both keys may be overridden", () => {
  const resolved = resolvePolicy("zod:\n  date: date-native\n  unknown-keys: strip\n");
  assert.deepEqual(resolved.policy, { date: "date-native", unknownKeys: "strip" });
});

test("malformed documents refuse with bounded reasons, never fall back", () => {
  const refusals = [
    ["", "zod:\n  date: weekly\n", "date-value"],
    ["", "zod:\n  unknown-keys: keep\n", "unknown-keys-value"],
    ["", "zod:\n  date: date-native\n  ghost: 1\n", "unknown-key"],
    ["", "zod:\n  date: date-native\n  date: date-string\n", "duplicate-key"],
    ["", "zod:\n  date: date-native\n    nested: 1\n", "key-shape"],
    ["", "zod:\n\tdate: date-native\n", "tab-indentation"],
    ["", "zod: 1\n", "top-level-key"],
    ["", "ghost:\n  date: date-native\n", "unknown-section"],
    ["", "  date: date-native\n", "orphan-key"],
    ["", "zod:\n  date: date-native\nzod:\n  date: date-string\n", "duplicate-section"],
  ];
  for (const [, text, expected] of refusals) {
    const resolved = resolvePolicy(text);
    assert.equal(resolved.refusal, expected, text);
    assert.equal(resolved.policy, undefined);
  }
});

test("non-text input and overbound documents refuse", () => {
  assert.equal(resolvePolicy(42).refusal, "not-text");
  assert.equal(resolvePolicy(`# ${"x".repeat(20 * 1024)}\n`).refusal, "overbound");
});

// ---------------------------------------------------------------------------
// Unsupported-construct findings against the committed fixture (AC-4).
// ---------------------------------------------------------------------------

test("the unsupported fixture classifies exactly the golden findings", () => {
  const irBytes = readFileSync(join(FIXTURE_DIR, "ir.json"));
  const ir = JSON.parse(irBytes.toString("utf8"));
  const { modules, findings } = mapProject(ir);
  const golden = JSON.parse(
    readFileSync(join(FIXTURE_DIR, "findings.golden.json"), "utf8"),
  );
  assert.deepEqual(findings, golden);
  // The only clean definition (the empty entity) still maps.
  assert.equal(modules.length, 1);
  assert.deepEqual(
    modules[0].declarations.map((declaration) => declaration.semanticId),
    ["planner.task"],
  );
});

test("findings sort deterministically by path, code, detail", () => {
  // Covered by the fixture ordering: count < ghosted < report.
  const golden = JSON.parse(
    readFileSync(join(FIXTURE_DIR, "findings.golden.json"), "utf8"),
  );
  const details = golden.map((finding) => finding.detail);
  assert.deepEqual([...details].sort(), details);
});
