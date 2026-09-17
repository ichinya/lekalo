// Regenerate the golden plan through the kernel read view + bundle.
import path from "node:path";
import fs from "node:fs";
import os from "node:os";
import { pathToFileURL } from "node:url";

const repo = "C:/Users/User/orca/workspaces/lekalo/m3-issue-48";
const adapter = await import(
  pathToFileURL(repo + "/adapters/node-typescript/adapter.mjs").href
);
const kernel = adapter.__lekaloKernel;
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "plan-gen-"));
const project = path.join(tmp, "project");
fs.cpSync(
  path.join(repo, "tests/fixtures/node-native-gates/pnpm-monorepo"),
  project,
  { recursive: true },
);
const profile = kernel.validateResolvedProjectProfile({
  id: "standalone",
  mode: "observed",
  target: "node-typescript",
  readRoots: [
    { kind: "tree", path: "packages/planner" },
    { kind: "tree", path: "packages/api" },
    { kind: "tree", path: "packages/cli" },
    { kind: "file", path: "package.json" },
    { kind: "file", path: "pnpm-workspace.yaml" },
  ],
  exclusions: [],
  provenance: { origin: "declared", revision: "gen", disposition: "public-fixture" },
});
const roots = [
  { kind: "tree", path: "packages/planner", scope: "packages/planner/**" },
  { kind: "tree", path: "packages/api", scope: "packages/api/**" },
  { kind: "tree", path: "packages/cli", scope: "packages/cli/**" },
  { kind: "file", path: "package.json", scope: "package.json" },
  { kind: "file", path: "pnpm-workspace.yaml", scope: "pnpm-workspace.yaml" },
];
const readView = kernel.createReadView(project, roots, profile);
readView.permittedProjectRoot = project;
const dirs = adapter.__lekaloNativeGate.listInventoryDirectories(readView);
const inv = adapter.__lekaloWorkspace.buildWorkspaceInventory({
  readView,
  permittedProjectRoot: project,
  directories: dirs,
});
const policy = JSON.parse(
  fs.readFileSync(
    repo + "/tests/fixtures/node-native-gates/protocol/policy.golden.json",
    "utf8",
  ),
);
const toolCatalog = [
  {
    id: "fixture-node",
    name: "node",
    version: "unknown",
    artifact_digest: "sha256:" + "c".repeat(64),
    entry_digest: "sha256:" + "c".repeat(64),
    platform: "windows",
    provenance: "fixture-catalog",
  },
];
const plan = adapter.__lekaloNativePlan.buildNativePlan({
  inventory: inv,
  changes: {
    files: [
      {
        path: "packages/planner/src/plan.ts",
        change: "modified",
        after_digest: "sha256:" + "1".repeat(64),
      },
    ],
    symbols: [],
  },
  policy,
  toolCatalog,
  toolCatalogDigest: "sha256:" + "9".repeat(64),
  profileRef: "standalone",
  profileDigest: "sha256:" + "f".repeat(64),
  adapterIdentity: {
    id: "lekalo-target-node-typescript",
    version: "0.3.2",
    digest: "sha256:" + "f".repeat(64),
  },
  scanRef: { id: "scan", version: "0.3.2", digest: "sha256:" + "2".repeat(64) },
  observedRef: { id: "observed", version: "0.3.2", digest: "sha256:" + "3".repeat(64) },
  inputManifestDigest: "sha256:" + "5".repeat(64),
  capabilitySnapshotDigest: "sha256:" + "7".repeat(64),
});
fs.writeFileSync(
  repo + "/tests/fixtures/node-native-gates/protocol/plan.golden.json",
  JSON.stringify(plan, null, 2) + "\n",
);
fs.writeFileSync(
  repo + "/tests/fixtures/node-native-gates/protocol/digest-vector.json",
  JSON.stringify({ plan, digest: plan.plan_digest }, null, 2) + "\n",
);
console.log("plan regenerated:", plan.plan_digest);
console.log(
  "commands:",
  plan.commands.length,
  "| root pkg:",
  plan.workspace.packages.some((p) => p.root === "."),
);
fs.rmSync(tmp, { recursive: true, force: true });
