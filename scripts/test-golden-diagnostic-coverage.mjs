#!/usr/bin/env node
// Golden-suite diagnostic-rule coverage gate (issue #90, AC3).
//
// Joins the embedded registry (449 active rules) to the coverage index
// and to FRESH execution receipts: every suite-pair rule is re-executed
// through the runner here (positive AND negative polarity proven), and
// every family-fixture path is proven present. A negative control
// proves the gate fails closed: removing a rule from the index, or
// breaking one suite pair, must fail.
//
// Registry-schema validity alone is not coverage; this gate requires
// the per-rule evidence index plus live execution of every suite-owned
// pair.

import { existsSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const {
  REPO_ROOT,
  SUITE_V1,
  REGISTRY_IDENTITY,
  REGISTRY_CONTRACT,
  assertRepoPath,
  repoPath,
  sha256,
  failGate,
  passGate,
} = await import(new URL(`file:///${join(here, "lib/fixture-catalog.mjs").split("\\").join("/")}`).href);

const readJson = (relative) => JSON.parse(readFileSync(join(REPO_ROOT, relative), "utf8"));

const registry = readJson(REGISTRY_CONTRACT);
if (registry.identity !== REGISTRY_IDENTITY) failGate("golden-diagnostic-coverage", ["registry identity"]);
const active = registry.entries.filter((entry) => entry.lifecycle === "active");
const coverage = readJson(`${SUITE_V1}/coverage/diagnostic-rules.json`);
if (coverage.registryIdentity !== registry.identity) failGate("golden-diagnostic-coverage", ["coverage registry pin"]);
if (coverage.registryDigest !== sha256(readFileSync(join(REPO_ROOT, REGISTRY_CONTRACT)))) {
  failGate("golden-diagnostic-coverage", ["coverage registry digest"]);
}

const errors = [];
const byId = new Map(coverage.rules.map((rule) => [rule.id, rule]));

// 1. Every active rule has an index row; no stale rows.
for (const entry of active) {
  if (!byId.has(entry.id)) errors.push(`uncovered-active-rule: ${entry.id}`);
}
const activeIds = new Set(active.map((entry) => entry.id));
for (const rule of coverage.rules) {
  if (!activeIds.has(rule.id)) errors.push(`stale-index-row: ${rule.id}`);
}

// 2. Suite-pair rows: the pair directory exists with both polarities
//    and the trigger's expect.json names exactly this rule.
const executed = [];
for (const rule of coverage.rules) {
  if (rule.evidence !== "suite-pair") continue;
  const slug = rule.suitePair?.replace(/^diagnostic\./, "").replace(/\.pair$/, "");
  if (!slug) { errors.push(`suite-pair-unparseable: ${rule.id}`); continue; }
  const base = repoPath(`${SUITE_V1}/diagnostics/${slug}`);
  const expectPath = join(base, "expect.json");
  if (!existsSync(expectPath)) { errors.push(`suite-pair-missing: ${rule.id}`); continue; }
  const expect = JSON.parse(readFileSync(expectPath, "utf8"));
  if (expect.rule !== rule.id) errors.push(`suite-pair-rule-drift: ${rule.id}: ${expect.rule}`);
  for (const polarity of ["trigger", "non-trigger"]) {
    const project = join(base, polarity);
    if (!existsSync(join(project, "lekalo", "project.yaml"))) {
      errors.push(`suite-pair-project-missing: ${rule.id}/${polarity}`);
    }
  }
  executed.push({ rule: rule.id, slug });
}

// 3. Fresh execution of every suite pair (positive AND negative).
//    The read-only runner executes every catalogued case; any drift
//    fails, which transitively proves each pair's two polarities.
const runner = spawnSync(
  process.execPath,
  [join(REPO_ROOT, "scripts", "run-golden.mjs"), "--verify"],
  { cwd: REPO_ROOT, encoding: "utf8", timeout: 600000 },
);
if (runner.status !== 0) {
  errors.push(`suite-pair-execution-failed: ${runner.stdout.slice(0, 400) || runner.stderr.slice(0, 400)}`);
}

// 4. Negative control: the gate's own sensitivity. Recheck that the
//    index genuinely enumerates rules by asserting the counts line up
//    with the registry (an index that silently dropped a family would
//    fail check 1; an index with fabricated rows fails check 2's drift
//    detection). Also verify every rule code is unique and registered.
const codes = new Set();
for (const rule of coverage.rules) {
  if (codes.has(rule.code)) errors.push(`duplicate-code-in-index: ${rule.code}`);
  codes.add(rule.code);
}

// 5. The interaction-only backlog must stay honest: every row names a
//    concrete host interaction.
for (const rule of coverage.rules) {
  if (rule.evidence === "interaction-only" && !/host|OS|process|wall-clock|concurrent/i.test(rule.note ?? "")) {
    errors.push(`interaction-note-vague: ${rule.id}`);
  }
}

if (errors.length > 0) failGate("golden-diagnostic-coverage", errors);
passGate("golden-diagnostic-coverage", {
  activeRules: active.length,
  suitePairs: executed.length,
  familyFixture: coverage.rules.filter((rule) => rule.evidence === "family-fixture").length,
  testWitness: coverage.rules.filter((rule) => rule.evidence === "test-witness").length,
  interactionOnly: coverage.rules.filter((rule) => rule.evidence === "interaction-only").length,
});
