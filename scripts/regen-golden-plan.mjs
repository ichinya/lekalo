// Regenerate the golden plan through the shipped pipeline: the bundled
// adapter's real planner extension (listInventoryDirectories →
// buildWorkspaceInventory → buildNativePlan), with the tool catalog
// derived by the extension itself and all digests computed — no
// hardcoded placeholder values. Path-resilient (works from any cwd).
import path from "node:path";
import fs from "node:fs";
import os from "node:os";
import { pathToFileURL } from "node:url";
import { fileURLToPath } from "node:url";

const repo = path.resolve(fileURLToPath(import.meta.url), "..", "..");
const adapterPath = path.join(repo, "adapters/node-typescript/adapter.mjs");
const adapter = await import(pathToFileURL(adapterPath).href);
const kernelNs = adapter.__lekaloKernel;
const nativeGate = adapter.__lekaloNativeGate;
const workspace = adapter.__lekaloWorkspace;
const nativePlan = adapter.__lekaloNativePlan;
const launchPolicy = adapter.__lekaloLaunchPolicy;

if (!nativeGate || !workspace || !nativePlan || !launchPolicy) {
  process.stderr.write(
    "regen-golden-plan: the adapter bundle does not export the planner " +
      "symbols; rebuild with `node adapters/node-typescript/build.mjs`\n",
  );
  process.exit(1);
}

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "plan-gen-"));
const project = path.join(tmp, "project");
fs.cpSync(
  path.join(repo, "tests/fixtures/node-native-gates/pnpm-monorepo"),
  project,
  { recursive: true },
);

const profile = kernelNs.validateResolvedProjectProfile({
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
const readView = kernelNs.createReadView(project, roots, profile);
readView.permittedProjectRoot = project;

// The real pipeline: the same declared-pattern discovery the extension
// runs (readDeclaredPatterns → listInventoryDirectories →
// buildWorkspaceInventory), and the plan from buildNativePlan with the
// extension's real tool catalog and the production canonical catalog
// digest — identical inputs to planNativeOperation, not approximations.
const inclusionPatterns = nativeGate.readDeclaredPatterns(readView);
const discovery = {};
const dirs = nativeGate.listInventoryDirectories(readView, inclusionPatterns, discovery);
const inv = workspace.buildWorkspaceInventory({
  readView,
  directories: dirs,
  discoveryTruncated: discovery.truncated === true,
});
const toolCatalog = nativeGate.buildToolCatalog(launchPolicy);
const toolCatalogDigest = nativeGate.computeToolCatalogDigest(toolCatalog);

const plan = nativePlan.buildNativePlan({
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
  policy: launchPolicy,
  toolCatalog,
  toolCatalogDigest,
  profileRef: "standalone",
  profileDigest: "sha256:" + "f".repeat(64),
  adapterIdentity: adapter.__lekaloKernelAdapterIdentity ?? {
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
  path.join(repo, "tests/fixtures/node-native-gates/protocol/plan.golden.json"),
  JSON.stringify(plan, null, 2) + "\n",
);
fs.writeFileSync(
  path.join(repo, "tests/fixtures/node-native-gates/protocol/digest-vector.json"),
  JSON.stringify({ plan, digest: plan.plan_digest }, null, 2) + "\n",
);
console.log("plan regenerated:", plan.plan_digest);
console.log(
  "commands:",
  plan.commands.length,
  "| root pkg:",
  plan.workspace.packages.some((p) => p.root === "."),
  "| tools[0].name:",
  plan.tools[0]?.name,
);
fs.rmSync(tmp, { recursive: true, force: true });
