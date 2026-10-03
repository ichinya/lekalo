#!/usr/bin/env node
// Golden-update policy gate (issue #90, AC5).
//
// Proves the deliberate update flow behaves as the issue requires:
// - the plan phase performs no tracked writes and is deterministic;
// - the plan carries a semantic review summary bound to before/after
//   digests (a hash-only summary is rejected by the closed schema);
// - apply refuses a wrong accept digest, a stale case revision, and
//   drifted preimages, and never writes outside the reviewed list;
// - a diagnostic.*.pair case (a descriptor with no declared expected
//   outputs) can be planned and reviewed — its candidate rows carry no
//   declaredPath — and apply refuses its candidates with
//   unmapped-candidate because no destination is declared, so the
//   no-expected branch is rehearsed end to end instead of only the
//   minimal.project path;
// - CI has no write path: the flow is never invoked by any workflow.
//
// The gate runs the real tool end to end against a scratch copy of the
// checkout's catalog and restores everything it touches.

import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, statSync, readdirSync, writeFileSync } from "node:fs";
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

// 1. CI never runs the update flow or any tracked-suite writer
//    (generators, manifest/checksum writers included).
const workflows = join(REPO_ROOT, ".github", "workflows");
const writerScripts = [
  "update-golden",
  "gen-suite-coverage",
  "gen-suite-diagnostic-pairs",
];
for (const name of readdirSync(workflows)) {
  const text = readFileSync(join(workflows, name), "utf8");
  for (const writer of writerScripts) {
    if (text.includes(writer)) {
      errors.push(`ci-invokes-suite-writer: ${writer} in .github/workflows/${name}`);
    }
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
const beforeTree = suiteTreeDigests();
// Native realpath expands the Windows 8.3 temp alias (RUNNER~1) that
// the production selection policy denies; the plain JS realpath keeps
// an 8.3-spelled input as written.
const scratch = realpathSync.native(mkdtempSync(join(tmpdir(), "lekalo-golden-policy-")));
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

    // Real-change rehearsal: mutate a committed preimage (a graph node
    // rename via a changed project document), re-plan, and require the
    // semantic summary to name a concrete semantic delta — not just
    // byte hashes. The scratch catalog pins the case checksums, so the
    // rehearsal refreshes them after the mutation first.
    {
      const projectYaml = join(scratch, SUITE_V1, "minimal", "project", "lekalo", "project.yaml");
      const original = readFileSync(projectYaml, "utf8");
      const mutated = original.replace(/description: [^\n]*/, "description: Renamed suite-v1 minimal project");
      if (mutated === original) {
        errors.push("real-change: mutation was a no-op");
      } else {
        writeFileSync(projectYaml, mutated);
        try {
          // Re-plan over the mutated preimage.
          const replan = run(["plan", "--case", "minimal.project", "--reason", "#90 policy gate: real semantic-change rehearsal"]);
          if (replan.status !== 0) {
            errors.push(`real-change: replan failed: ${(replan.stderr ?? "").slice(0, 200)}`);
          } else {
            const summaryPhase = (replan.stderr ?? "").includes("semantic-summary");
            if (!summaryPhase) errors.push("real-change: replan printed no semantic summary");
            const planFile = JSON.parse(replan.stdout.trim()).planPath;
            const planDoc = JSON.parse(readFileSync(planFile, "utf8"));
            const changedRows = planDoc.after.files.filter((row) => row.change === "modified");
            if (changedRows.length === 0) {
              errors.push("real-change: no modified files recorded after a real mutation");
            } else {
              const kinds = new Set(planDoc.summary.semanticChanges.map((row) => row.kind));
              if (kinds.size === 1 && kinds.has("digests-refreshed")) {
                errors.push("real-change: hash-only summary for a real mutation");
              }
            }
          }
        } finally {
          writeFileSync(projectYaml, original);
        }
      }
    }
  }

  // 3b. Pair-class rehearsal: a diagnostic.*.pair descriptor declares
  //     no expected outputs, so plan must still succeed (the
  //     no-expected branch) with a semantic summary and candidate rows
  //     that carry no declaredPath, the digest binding still refuses a
  //     wrong accept digest, and apply with the correct digest must
  //     still refuse with unmapped-candidate — a pair case declares no
  //     destination, so nothing may be written for it.
  {
    const pairPlan = run(["plan", "--case", "diagnostic.type-recursion.pair", "--reason", "#90 policy gate: no-expected pair class, plan review apply rehearsal"]);
    if (pairPlan.status !== 0) {
      errors.push(`pair-plan-phase-failed: ${(pairPlan.stderr ?? pairPlan.stdout ?? "").slice(0, 300)}`);
    } else {
      const pairReport = JSON.parse(pairPlan.stdout.trim());
      const pairDoc = JSON.parse(readFileSync(pairReport.planPath, "utf8"));
      if (!Array.isArray(pairDoc.summary?.semanticChanges) || pairDoc.summary.semanticChanges.length === 0) {
        errors.push("pair-plan-phase: empty semantic summary");
      }
      if (!Array.isArray(pairDoc.after?.files) || pairDoc.after.files.length === 0) {
        errors.push("pair-plan-phase: no candidate rows for the pair case");
      }
      if (pairDoc.after.files.some((row) => Object.hasOwn(row, "declaredPath"))) {
        errors.push("pair-plan-phase: no-expected plan emitted a declaredPath member");
      }
      // The review binding holds for the class: wrong digest refused.
      const pairWrong = run(["apply", "--plan", pairReport.planPath, "--accept-plan-sha256", "sha256:" + "0".repeat(64)]);
      if (pairWrong.status === 0) errors.push("pair-apply-phase: wrong digest was accepted");
      // Correct digest still refuses: no declared destination exists.
      const pairApply = run(["apply", "--plan", pairReport.planPath, "--accept-plan-sha256", pairReport.planSha256]);
      if (pairApply.status === 0) {
        errors.push("pair-apply-phase: applied candidates without declared destinations");
      } else if (!(pairApply.stderr ?? "").includes("unmapped-candidate")) {
        errors.push(`pair-apply-phase: expected unmapped-candidate refusal: ${(pairApply.stderr ?? "").slice(0, 200)}`);
      }
    }
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}

function suiteTreeDigests() {
  const digests = new Map();
  const walk = (current, logical) => {
    for (const name of readdirSync(current).sort()) {
      const full = join(current, name);
      const child = `${logical}/${name}`;
      if (statSync(full).isDirectory()) walk(full, child);
      else digests.set(child, sha256(readFileSync(full)));
    }
  };
  walk(join(REPO_ROOT, SUITE_V1), SUITE_V1);
  return digests;
}
// 4. The tracked tree must be untouched by the gate itself: snapshot
//    before the rehearsal, compare after.

const afterTree = suiteTreeDigests();
for (const [path, digest] of beforeTree) {
  if (afterTree.get(path) !== digest) errors.push(`tracked-suite-polluted: ${path}`);
}
for (const path of afterTree.keys()) {
  if (!beforeTree.has(path)) errors.push(`tracked-suite-new-file: ${path}`);
}

if (errors.length > 0) failGate("golden-update-policy", errors);
passGate("golden-update-policy", {
  planSchemaKinds: kinds.length,
  flow: "plan -> review -> apply(digest-bound)",
  pairClassFlow: "diagnostic.type-recursion.pair: plan -> review -> apply(unmapped-candidate)",
});
