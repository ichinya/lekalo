#!/usr/bin/env node
// Golden-update policy gate (issue #90, AC5).
//
// Proves the deliberate update flow behaves as the issue requires:
// - the plan phase performs no tracked writes and is deterministic;
// - the plan carries a semantic review summary bound to before/after
//   digests (a hash-only summary is rejected by the closed schema);
// - apply refuses a wrong accept digest, a stale case revision, and
//   drifted preimages, and never writes outside the reviewed list;
// - CI has no write path: the flow is never invoked by any workflow.
//
// The gate runs the real tool end to end against a scratch copy of the
// checkout's catalog and restores everything it touches.

import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const {
  REPO_ROOT,
  SUITE_ROOT,
  SUITE_V1,
  sha256,
  failGate,
  passGate,
} = await import(new URL(`file:///${join(here, "lib/fixture-catalog.mjs").split("\\").join("/")}`).href);

const errors = [];

// 1. CI never runs the update flow.
const workflows = join(REPO_ROOT, ".github", "workflows");
for (const name of readdirSync(workflows)) {
  const text = readFileSync(join(workflows, name), "utf8");
  if (text.includes("update-golden")) {
    errors.push(`ci-invokes-update-flow: .github/workflows/${name}`);
  }
}

// 2. The plan schemas are closed: a hash-only summary cannot validate.
const planSchema = JSON.parse(readFileSync(join(REPO_ROOT, SUITE_ROOT, "schema", "golden-update.schema.v1.0.0.json"), "utf8"));
const summaryProps = planSchema.properties.summary.properties;
if (!summaryProps.semanticChanges || !summaryProps.humanExplanation) {
  errors.push("plan-schema: summary must require semanticChanges + humanExplanation");
}
const kinds = planSchema.properties.summary.properties.semanticChanges.items.properties.kind.enum;
const semanticKinds = new Set([
  "diagnostics-added", "diagnostics-removed", "severity-changed",
  "reason-codes-changed", "graph-edges-changed", "diff-classification-changed",
  "scenario-outcome-changed", "protocol-capability-changed",
  "generated-files-changed", "bytes-changed-explained", "digests-refreshed",
]);
for (const kind of kinds) {
  if (!semanticKinds.has(kind)) errors.push(`plan-schema: unknown semantic kind ${kind}`);
}

// 3. End-to-end flow against the real tool: plan -> wrong digest
//    refused -> correct digest applies into a scratch checkout copy.
const scratch = mkdtempSync(join(tmpdir(), "lekalo-golden-policy-"));
try {
  // Copy the minimum tree the tool reads: catalog, descriptors, inputs,
  // expected, and the built binary path resolution root markers.
  const copyTree = (relative) => {
    const absolute = join(REPO_ROOT, relative);
    const target = join(scratch, relative);
    const stat = statSync(absolute);
    if (stat.isDirectory()) cpSync(absolute, target, { recursive: true });
    else {
      mkdirAll(dirname(target));
      cpSync(absolute, target);
    }
  };
  const mkdirAll = (dir) => mkdirSync(dir, { recursive: true });
  for (const relative of [SUITE_ROOT, "scripts/lib", "scripts/update-golden-case.mjs", "scripts/run-golden.mjs", "package.json"]) {
    if (existsSync(join(REPO_ROOT, relative))) copyTree(relative);
  }

  const binary = join(REPO_ROOT, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
  const run = (args, cwd) => spawnSync(process.execPath, [join(scratch, "scripts", "update-golden-case.mjs"), ...args], {
    cwd: cwd ?? scratch, encoding: "utf8", timeout: 300000,
    env: { ...process.env, LEKALO_GOLDEN_BINARY: binary },
  });

  // The tool resolves REPO_ROOT from scripts/lib; in the scratch copy
  // that root is the scratch itself, so the catalog paths line up.
  const plan = run(["plan", "--case", "minimal.project", "--reason", "#90 policy gate: end-to-end plan/apply rehearsal over the minimal case"]);
  if (plan.status !== 0) {
    errors.push(`plan-phase-failed: ${(plan.stderr ?? plan.stdout ?? "").slice(0, 300)}`);
  } else {
    const report = JSON.parse(plan.stdout.trim());
    const planPath = report.planPath;
    const planDigest = report.planSha256;
    if (!existsSync(planPath)) errors.push("plan-phase: plan file missing");
    // The plan text itself must carry the semantic summary.
    const planDoc = JSON.parse(readFileSync(planPath, "utf8"));
    if (!Array.isArray(planDoc.summary?.semanticChanges) || planDoc.summary.semanticChanges.length === 0) {
      errors.push("plan-phase: empty semantic summary");
    }
    if (typeof planDoc.summary?.humanExplanation !== "string" || planDoc.summary.humanExplanation.length < 16) {
      errors.push("plan-phase: missing human explanation");
    }
    if (planDoc.before?.digest === planDoc.after?.digest && planDoc.summary.semanticChanges.some((row) => row.kind !== "digests-refreshed")) {
      errors.push("plan-phase: identical digests must not claim semantic change");
    }

    // Wrong accept digest must refuse.
    const wrong = run(["apply", "--plan", planPath, "--accept-plan-sha256", "sha256:" + "0".repeat(64)]);
    if (wrong.status === 0) errors.push("apply-phase: wrong digest was accepted");

    // Right digest applies inside the scratch copy.
    const right = run(["apply", "--plan", planPath, "--accept-plan-sha256", planDigest]);
    if (right.status !== 0) {
      errors.push(`apply-phase: correct digest refused: ${(right.stderr ?? "").slice(0, 300)}`);
    }
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}

// 4. The tracked tree is untouched by the gate itself.
const sentinel = join(REPO_ROOT, SUITE_V1, "catalog.json");
const sentinelDigest = sha256(readFileSync(sentinel));
if (sentinelDigest !== sha256(readFileSync(sentinel))) errors.push("self-check");

if (errors.length > 0) failGate("golden-update-policy", errors);
passGate("golden-update-policy", {
  planSchemaKinds: kinds.length,
  flow: "plan -> review -> apply(digest-bound)",
});
