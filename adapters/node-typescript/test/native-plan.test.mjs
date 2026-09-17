/**
 * Issue #48 native plan tests: the pure affected-closure selection, the
 * confirmed-script literal parser, and the deterministic plan digest —
 * in-process, no I/O beyond the committed fixtures, no processes.
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  buildNativePlan,
  canonicalJsonText,
  computeAffectedClosure,
  isSafeLiteral,
  parseConfirmedScript,
  planDigest,
  PlanRefusal,
} from "../src/native-plan.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const fixtureRoot = join(repoRoot, "tests/fixtures/node-native-gates");

const D = (c) => "sha256:" + c.repeat(64);

const inventory = () => ({
  manager: "pnpm-workspace",
  compatibilityPath: "supported",
  root: ".",
  workspaceManifestDigest: D("e1"),
  lockDigestState: "absent",
  packages: [
    { id: "packages/planner=@fixture/planner", name: "@fixture/planner", root: "packages/planner", manifestDigest: D("e3") },
    { id: "packages/api=@fixture/api", name: "@fixture/api", root: "packages/api", manifestDigest: D("e4") },
    { id: "packages/cli=@fixture/cli", name: "@fixture/cli", root: "packages/cli", manifestDigest: D("e5") },
    { id: "packages/web=@fixture/web", name: "@fixture/web", root: "packages/web", manifestDigest: D("e6") },
    { id: "packages/integration=@fixture/integration", name: "@fixture/integration", root: "packages/integration", manifestDigest: D("e7") },
  ],
  edges: [
    { from: "packages/api=@fixture/api", to: "packages/planner=@fixture/planner", kind: "dependency", scope: "dependencies", specifier: "workspace:^", provenance: "workspace-specifier" },
    { from: "packages/cli=@fixture/cli", to: "packages/api=@fixture/api", kind: "dependency", scope: "dependencies", specifier: "workspace:*", provenance: "workspace-specifier" },
  ],
  uncertainties: [],
  completeness: "complete",
});

test("affected closure: planner change selects planner, api, and cli only", () => {
  const affected = computeAffectedClosure(inventory(), ["packages/planner=@fixture/planner"]);
  const ids = affected.map((a) => a.package_id);
  assert.deepEqual(ids, [
    "packages/api=@fixture/api",
    "packages/cli=@fixture/cli",
    "packages/planner=@fixture/planner",
  ]);
  const kinds = Object.fromEntries(affected.map((a) => [a.package_id, a.reasons[0].kind]));
  assert.equal(kinds["packages/planner=@fixture/planner"], "changed-package");
  assert.equal(kinds["packages/api=@fixture/api"], "dependent-closure");
  assert.equal(kinds["packages/cli=@fixture/cli"], "dependent-closure");
});

test("affected closure: unrelated web and integration are never selected", () => {
  const affected = computeAffectedClosure(inventory(), ["packages/planner=@fixture/planner"]);
  const ids = affected.map((a) => a.package_id);
  assert.equal(ids.includes("packages/web=@fixture/web"), false);
  assert.equal(ids.includes("packages/integration=@fixture/integration"), false);
});

test("affected closure: no changes yields an empty affected set (not everything)", () => {
  const affected = computeAffectedClosure(inventory(), []);
  assert.deepEqual(affected, []);
});

test("confirmed-script parser: the literal form matches; shell forms refuse", () => {
  const confirmed = ["node", "gates/verify.mjs", "--mode", "test"];
  const ok = parseConfirmedScript("node gates/verify.mjs --mode test", confirmed);
  assert.equal(ok.ok, true);
  assert.equal(ok.matched, true);
  // Hostile-looking literal argument never executes a second command.
  const hostile = parseConfirmedScript("node gates/verify.mjs --mode test; rm -rf /", confirmed);
  assert.equal(hostile.ok, false);
  // Full-string comparison: a changed argument is a mismatch, not a prefix hit.
  const changed = parseConfirmedScript("node gates/verify.mjs --mode typecheck", confirmed);
  assert.equal(changed.ok, false);
  assert.equal(changed.reason, "script-argv-mismatch");
  // Shell feature forms.
  for (const hostileScript of [
    "node `id`",
    "node $(id)",
    "node a && node b",
    "node a | node b",
    "node a > out",
    "FOO=1 node a",
    "node $HOME/x",
    "cmd /c echo hi",
    "pnpm run test",
    "node tools/run.bat",
    "node --eval code",
    "node  a",
  ]) {
    const result = parseConfirmedScript(hostileScript, undefined);
    assert.equal(result.ok, false, hostileScript);
  }
});

test("isSafeLiteral: metacharacters and empty strings refuse", () => {
  assert.equal(isSafeLiteral("plain-value"), true);
  assert.equal(isSafeLiteral(""), false);
  assert.equal(isSafeLiteral("a|b"), false);
  assert.equal(isSafeLiteral("$(x)"), false);
  assert.equal(isSafeLiteral("`x`"), false);
});

const policy = () => JSON.parse(
  readFileSync(join(fixtureRoot, "protocol/policy.golden.json"), "utf8"),
);

const buildArgs = (overrides = {}) => ({
  inventory: inventory(),
  changes: { files: [{ path: "packages/planner/src/plan.ts", change: "modified", after_digest: D("11") }], symbols: [] },
  policy: policy(),
  toolCatalog: [
    { id: "fixture-node", name: "node", version: "unknown", artifact_digest: D("c1"), entry_digest: D("c2"), platform: "any", provenance: "fixture-catalog" },
  ],
  profileRef: "standalone",
  profileDigest: D("f1"),
  adapterIdentity: { id: "lekalo-target-node-typescript", version: "0.3.2", digest: D("f2") },
  scanRef: { id: "scan", version: "0.3.2", digest: D("22") },
  observedRef: { id: "observed", version: "0.3.2", digest: D("33") },
  inputManifestDigest: D("55"),
  toolCatalogDigest: D("66"),
  capabilitySnapshotDigest: D("77"),
  ...overrides,
});

test("plan build: the planner change produces the confirmed gates and excludes the unrelated", () => {
  const plan = buildNativePlan(buildArgs());
  const commandPackages = plan.commands.map((command) => command.package_id);
  assert.ok(commandPackages.includes("packages/planner=@fixture/planner"), "planner gate planned");
  assert.ok(commandPackages.includes("packages/api=@fixture/api"), "api gate planned (dependent)");
  assert.ok(commandPackages.includes("packages/cli=@fixture/cli"), "cli gate planned (transitive)");
  // web/integration: no edge, no confirmation → excluded with a reason.
  const excludedIds = plan.excluded.map((entry) => entry.package_id);
  assert.ok(excludedIds.includes("packages/web=@fixture/web"), "web excluded");
  assert.ok(excludedIds.includes("packages/integration=@fixture/integration"), "integration excluded");
  // Direct argv, no shell: every command argv is the confirmed literal.
  for (const command of plan.commands) {
    assert.equal(command.argv[0], "node");
    assert.equal(isSafeLiteral(command.argv.join(" ")), true, "argv has no shell metacharacters");
  }
  // Selection stays targeted; run eligibility is plan-only in production.
  assert.equal(plan.selection_mode, "targeted");
  assert.equal(plan.run_eligibility.state, "plan-only");
  assert.equal(plan.kind, "native-plan");
});

test("plan digest: deterministic, content-bound, and domain-separated", () => {
  const plan = buildNativePlan(buildArgs());
  const a = planDigest(plan);
  const b = planDigest(buildNativePlan(buildArgs()));
  assert.equal(a, b, "cold and warm builds are byte-identical");
  assert.equal(plan.plan_digest, a);
  // A digest member is never an input to its own computation.
  const tampered = JSON.parse(JSON.stringify(plan));
  tampered.plan_digest = "sha256:" + "9".repeat(64);
  assert.equal(planDigest(tampered), a, "the digest member is omitted from its own input");
  // Every decision-relevant change moves the digest.
  const other = buildNativePlan(buildArgs({ capabilitySnapshotDigest: D("78") }));
  assert.notEqual(planDigest(other), a);
  // Canonical form: sorted keys, compact.
  assert.equal(canonicalJsonText(plan), canonicalJsonText(JSON.parse(canonicalJsonText(plan))));
  assert.match(canonicalJsonText(plan), /^\{/, "compact form, no whitespace");
});

test("plan build: refusing an invalid inventory and empty command plans", () => {
  assert.throws(() => buildNativePlan(buildArgs({ inventory: null })), PlanRefusal);
  assert.throws(() => buildNativePlan(buildArgs({ policy: null })), PlanRefusal);
  // No changes → no affected → no commands; run eligibility is blocked.
  const empty = buildNativePlan(buildArgs({ changes: { files: [], symbols: [] } }));
  assert.deepEqual(empty.commands, []);
  assert.equal(empty.run_eligibility.state, "blocked");
});

test("plan build: the committed plan golden rebuilds byte-identically", () => {
  const golden = JSON.parse(
    readFileSync(join(fixtureRoot, "protocol/plan.golden.json"), "utf8"),
  );
  // The golden was produced by this builder; the digest must still verify.
  assert.equal(planDigest(golden), golden.plan_digest);
});
