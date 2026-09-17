/**
 * Issue #48 end-to-end native-gate extension tests: the shipped
 * planNativeOperation path over the committed pnpm-monorepo fixture —
 * declared-pattern member discovery (non-vocabulary names, the R3-1
 * regression), confirmation verification, and the golden-plan pipeline
 * being byte-exact reproducible through the production helpers (R3-3).
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  createReadView,
  validateResolvedProjectProfile,
} from "../src/kernel.mjs";
import {
  buildToolCatalog,
  computeToolCatalogDigest,
  listInventoryDirectories,
  planNativeOperation,
  readDeclaredPatterns,
  setAdapterIdentity,
  setLaunchPolicy,
} from "../src/native-gate-extension.mjs";
import { buildWorkspaceInventory } from "../src/workspace.mjs";
import { buildNativePlan } from "../src/native-plan.mjs";
import launchPolicy from "../src/native-policy.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const project = join(repoRoot, "tests/fixtures/node-native-gates/pnpm-monorepo");
const D = (c) => "sha256:" + c.repeat(64);

const READ_ROOTS = [
  { kind: "tree", path: "packages/planner" },
  { kind: "tree", path: "packages/api" },
  { kind: "tree", path: "packages/cli" },
  { kind: "file", path: "package.json" },
  { kind: "file", path: "pnpm-workspace.yaml" },
];

function fixtureReadView() {
  const profile = validateResolvedProjectProfile({
    id: "standalone",
    mode: "observed",
    target: "node-typescript",
    readRoots: READ_ROOTS.map(({ kind, path }) => ({ kind, path })),
    exclusions: [],
    provenance: { origin: "declared", revision: "test", disposition: "public-fixture" },
  });
  const roots = READ_ROOTS.map(({ kind, path }) => ({
    kind,
    path,
    scope: kind === "tree" ? `${path}/**` : path,
  }));
  const readView = createReadView(project, roots, profile);
  readView.permittedProjectRoot = project;
  return { profile, readView };
}

setAdapterIdentity({ id: "lekalo-target-node-typescript", version: "0.3.2", digest: D("f") });
setLaunchPolicy(launchPolicy);

test("planNativeOperation discovers non-vocabulary members via declared roots", () => {
  const { profile, readView } = fixtureReadView();
  const result = planNativeOperation(
    {
      request: {
        native_request: {
          changes: {
            files: [{
              path: "packages/planner/src/plan.ts",
              change: "modified",
              after_digest: D("1"),
            }],
            symbols: [],
          },
          scan_ref: { id: "scan", version: "0.3.2", digest: D("2") },
          execution_policy_ref: { id: "policy", version: "0.3.2", digest: D("4") },
          input_manifest_digest: D("5"),
          tool_catalog_digest: D("6"),
          capability_snapshot_digest: D("7"),
        },
      },
      profile,
      readView,
    },
    launchPolicy,
  );
  assert.equal(result.state, "complete", JSON.stringify(result.diagnostics));
  const plan = result.data.native_plan;
  const memberRoots = plan.workspace.packages.map((pkg) => pkg.root).sort();
  // planner/api/cli are NOT in the bounded CHILD_VOCABULARY — only the
  // declared tree roots can enumerate them (the R3-1 regression set).
  assert.deepEqual(memberRoots, [".", "packages/api", "packages/cli", "packages/planner"]);
  assert.equal(plan.commands.length, 3, "one confirmed command per member package");
  assert.notEqual(plan.run_eligibility?.state, "blocked");
});

test("declared-pattern discovery enumerates the same members the roots grant", () => {
  const { readView } = fixtureReadView();
  const patterns = readDeclaredPatterns(readView);
  assert.deepEqual(patterns, ["packages/*"]);
  const dirs = listInventoryDirectories(readView, patterns);
  for (const member of ["packages/planner", "packages/api", "packages/cli"]) {
    assert.ok(dirs.includes(member), `${member} must be a discovery candidate`);
  }
  const inventory = buildWorkspaceInventory({ readView, directories: dirs });
  assert.equal(inventory.completeness, "complete");
  assert.equal(inventory.packages.length, 4);
});

test("zero-member discovery records a pattern-partial uncertainty", () => {
  const profile = validateResolvedProjectProfile({
    id: "standalone",
    mode: "observed",
    target: "node-typescript",
    readRoots: [{ kind: "file", path: "package.json" }, { kind: "file", path: "pnpm-workspace.yaml" }],
    exclusions: [],
    provenance: { origin: "declared", revision: "test", disposition: "public-fixture" },
  });
  const readView = createReadView(project, [
    { kind: "file", path: "package.json", scope: "package.json" },
    { kind: "file", path: "pnpm-workspace.yaml", scope: "pnpm-workspace.yaml" },
  ], profile);
  readView.permittedProjectRoot = project;
  const dirs = listInventoryDirectories(readView, readDeclaredPatterns(readView));
  const inventory = buildWorkspaceInventory({ readView, directories: dirs });
  assert.equal(inventory.completeness, "incomplete");
  assert.ok(
    inventory.uncertainties.some((entry) =>
      entry.kind === "pattern-partial"
        && entry.detail.includes("matched no in-scope directories")),
    "an unmatched declared pattern must degrade completeness",
  );
});

test("the production pipeline reproduces the committed golden plan byte-exact", () => {
  const { readView } = fixtureReadView();
  // The exact regen-golden-plan.mjs pipeline: production helpers only.
  const inclusionPatterns = readDeclaredPatterns(readView);
  const discovery = {};
  const dirs = listInventoryDirectories(readView, inclusionPatterns, discovery);
  const inventory = buildWorkspaceInventory({
    readView,
    directories: dirs,
    discoveryTruncated: discovery.truncated === true,
  });
  const toolCatalog = buildToolCatalog(launchPolicy);
  const toolCatalogDigest = computeToolCatalogDigest(toolCatalog);
  const plan = buildNativePlan({
    inventory,
    changes: {
      files: [{
        path: "packages/planner/src/plan.ts",
        change: "modified",
        after_digest: D("1"),
      }],
      symbols: [],
    },
    policy: launchPolicy,
    toolCatalog,
    toolCatalogDigest,
    profileRef: "standalone",
    profileDigest: D("f"),
    adapterIdentity: { id: "lekalo-target-node-typescript", version: "0.3.2", digest: D("f") },
    scanRef: { id: "scan", version: "0.3.2", digest: D("2") },
    observedRef: { id: "observed", version: "0.3.2", digest: D("3") },
    inputManifestDigest: D("5"),
    capabilitySnapshotDigest: D("7"),
  });
  const golden = JSON.parse(
    readFileSync(join(repoRoot, "tests/fixtures/node-native-gates/protocol/plan.golden.json"), "utf8"),
  );
  assert.deepEqual(plan, golden, "the golden must be the production pipeline output");
});
