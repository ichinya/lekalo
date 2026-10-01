#!/usr/bin/env node
// Deliberate golden update flow, phase 1: plan (issue #90, AC5).
//
// Produces a reviewed update plan for one case without touching any
// tracked file: the producer runs twice in fresh sandboxes, both
// executions must agree byte-for-byte, and the plan binds every
// changed/unchanged expected file's before/after digest plus a semantic
// review summary. `apply` (update-golden-apply.mjs) refuses to write
// unless presented with exactly this plan's sha256 digest, so a review
// is mandatory between generation and application. CI never invokes
// either phase.
//
// Usage:
//   node scripts/update-golden-case.mjs plan --case <case-id> --reason "<issue + rationale>" [--out <dir>]
//   node scripts/update-golden-case.mjs apply --plan <plan-file> --accept-plan-sha256 <digest>

import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const {
  SUITE_V1,
  loadCatalog,
  loadCases,
  sha256,
  failGate,
} = await import(new URL(`file:///${join(here, "lib/fixture-catalog.mjs").split("\\").join("/")}`).href);

const argv = process.argv.slice(2);
const mode = argv[0];

if (mode === "plan") {
  const caseIndex = argv.indexOf("--case");
  const reasonIndex = argv.indexOf("--reason");
  const outIndex = argv.indexOf("--out");
  const caseId = caseIndex >= 0 ? argv[caseIndex + 1] : null;
  const reason = reasonIndex >= 0 ? argv[reasonIndex + 1] : null;
  const outDir = outIndex >= 0 ? resolve(argv[outIndex + 1]) : null;
  if (!caseId || !reason || reason.length < 16) {
    process.stderr.write("usage: update-golden-case.mjs plan --case <id> --reason \"<issue + rationale>\" [--out <dir>]\n");
    process.exit(2);
  }

  const { catalog, errors } = loadCatalog();
  if (errors.length > 0) failGate("golden-update-plan", errors);
  const { cases } = loadCases(catalog);
  const entry = cases.find((row) => row.id === caseId);
  if (!entry) failGate("golden-update-plan", [{ reason: "unknown-case", caseId }]);

  const binary = process.env.LEKALO_GOLDEN_BINARY
    ?? join(repoRoot, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
  if (!existsSync(binary)) failGate("golden-update-plan", [{ reason: "binary-missing" }]);

  const sandboxBase = outDir ?? mkdtempSync(join(tmpdir(), "lekalo-golden-plan-"));
  mkdirSync(sandboxBase, { recursive: true });

  // Role producers: each declared expected role names its own runner.
  // Cases without declared expected files (status-contract pairs)
  // produce one validate envelope per project role.
  const RUNNER_ROLE_ARGS = new Map([
    ["load-envelope", ["load", "--json", "--project"]],
    ["ir-envelope", ["load", "--ir", "--spans", "--json", "--project"]],
    ["validate-strict-envelope", ["validate", "--strict", "--json", "--project"]],
    ["validate-envelope", ["validate", "--json", "--project"]],
    ["graph-envelope", ["--json", "graph", "export", "--project"]],
  ]);
  const runOnce = (lane) => {
    const laneRoot = join(sandboxBase, lane);
    mkdirSync(laneRoot, { recursive: true });
    const caseDirLogical = entry.path.split("/").slice(0, -1).join("/");
    const declared = entry.descriptor.expected ?? [];
    const rows = [];
    const projects = (entry.descriptor.inputs ?? []).filter((row) => row.role === "project");
    const firstProject = (laneRootName) => {
      const input = projects[0];
      const suffix = input.path.startsWith(`${caseDirLogical}/`)
        ? input.path.slice(caseDirLogical.length + 1)
        : null;
      const source = suffix ? join(repoRoot, caseDirLogical, suffix) : join(repoRoot, input.path);
      const name = suffix ? suffix.split("/").join("-") : "project";
      const project = join(laneRootName, name);
      cpSync(source, project, { recursive: true });
      return name;
    };
    if (declared.length > 0) {
      const name = firstProject(laneRoot);
      for (const output of declared) {
        const args = RUNNER_ROLE_ARGS.get(output.role);
        if (!args) {
          failGate("golden-update-plan", [{ reason: "unknown-role-runner", role: output.role, caseId }]);
        }
        const result = spawnSync(binary, ["--no-cache", ...args, name], {
          cwd: laneRoot, encoding: "utf8", timeout: entry.descriptor.timeoutMs ?? 60000,
        });
        rows.push({ project: output.role, exit: result.status, envelope: (result.stdout ?? "") });
      }
    } else {
      for (const input of projects) {
        const suffix = input.path.startsWith(`${caseDirLogical}/`)
          ? input.path.slice(caseDirLogical.length + 1)
          : null;
        const source = suffix ? join(repoRoot, caseDirLogical, suffix) : join(repoRoot, input.path);
        const name = suffix ? suffix.split("/").join("-") : "project";
        const project = join(laneRoot, name);
        cpSync(source, project, { recursive: true });
        const result = spawnSync(binary, ["--no-cache", "validate", "--json", "--project", name], {
          cwd: laneRoot, encoding: "utf8", timeout: entry.descriptor.timeoutMs ?? 60000,
        });
        rows.push({ project: name, exit: result.status, envelope: (result.stdout ?? "") });
      }
    }
    return rows;
  };

  const cold1 = runOnce("cold-1");
  const cold2 = runOnce("cold-2");
  const a = JSON.stringify(cold1);
  const b = JSON.stringify(cold2);
  if (a !== b) {
    failGate("golden-update-plan", [{ reason: "producer-not-deterministic", caseId, detail: "two plan executions disagree" }]);
  }

  // The candidate outputs (one envelope per project role).
  const declared = entry.descriptor.expected ?? [];
  const candidateFiles = cold1.map((row, index) => ({
    path: declared.length > 0 ? `expected/${declared[index].role}` : `expected/${row.project}.envelope.json`,
    declaredPath: declared.length > 0 ? declared[index].path : null,
    digest: sha256(row.envelope),
    bytes: row.envelope,
  }));

  // Before digests of every declared expected output (absent => added).
  const beforeFiles = (entry.descriptor.expected ?? []).map((output) => ({
    path: output.path,
    digest: sha256(readFileSync(join(repoRoot, output.path))),
  }));
  const beforeMap = new Map(beforeFiles.map((row) => [row.path, row.digest]));

  const semanticChanges = [];
  // Compare candidate envelopes against the declared expected roles in
  // order: diagnostics pairs pin one envelope expectation per role.
  if (declared.length === candidateFiles.length) {
    for (let i = 0; i < declared.length; i += 1) {
      const oldDigest = sha256(readFileSync(join(repoRoot, declared[i].path)));
      const newDigest = candidateFiles[i].digest;
      if (oldDigest !== newDigest) {
        let semantic = "bytes-changed-explained";
        try {
          const oldDoc = JSON.parse(readFileSync(join(repoRoot, declared[i].path), "utf8").replace(/^[^{]*/, ""));
          const newDoc = JSON.parse(candidateFiles[i].bytes);
          const oldCodes = oldDoc.reasonCodes ?? [];
          const newCodes = newDoc.reasonCodes ?? [];
          if (JSON.stringify(oldCodes) !== JSON.stringify(newCodes)) semantic = "reason-codes-changed";
          else if ((oldDoc.diagnostics ?? []).length !== (newDoc.diagnostics ?? []).length) semantic = "diagnostics-added";
        } catch { /* envelope is not the tracked wire form */ }
        semanticChanges.push({ kind: semantic, detail: `${declared[i].path}: ${oldDigest.slice(0, 19)}… -> ${newDigest.slice(0, 19)}…` });
      }
    }
  } else {
    semanticChanges.push({ kind: "digests-refreshed", detail: `${candidateFiles.length} candidate envelope(s) for case ${caseId}` });
  }
  if (semanticChanges.length === 0) {
    semanticChanges.push({ kind: "digests-refreshed", detail: "no byte changes; plan re-confirms current goldens" });
  }

  const plan = {
    planId: `golden-plan-${caseId.replaceAll(".", "-")}`,
    fixtureSchema: "dev.lekalo.fixture@1.0.0",
    createdBy: { script: "scripts/update-golden-case.mjs" },
    reason,
    caseId,
    caseRevision: entry.revision,
    producer: { runner: entry.descriptor.runner },
    before: { digest: sha256(a), files: beforeFiles },
    after: {
      digest: sha256(b),
      files: candidateFiles.map((row) => ({
        path: row.path,
        declaredPath: row.declaredPath,
        digest: row.digest,
        change: "modified",
      })),
    },
    summary: {
      semanticChanges,
      humanExplanation: reason,
    },
  };
  const planText = `${JSON.stringify(plan, null, 2)}\n`;
  const planPath = join(sandboxBase, `${plan.planId}.json`);
  writeFileSync(planPath, planText);
  // Candidate bytes are written beside the plan for human review.
  for (const row of candidateFiles) {
    const target = join(sandboxBase, row.path);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, `${row.bytes}\n`);
  }
  const planDigest = sha256(planText);
  process.stdout.write(`${JSON.stringify({
    ok: true,
    phase: "plan",
    caseId,
    planPath,
    planSha256: planDigest,
    note: "review the plan and the candidate files, then apply with --accept-plan-sha256",
  }, null, 2)}\n`);
  process.exit(0);
}

if (mode === "apply") {
  const planIndex = argv.indexOf("--plan");
  const acceptIndex = argv.indexOf("--accept-plan-sha256");
  const planPath = planIndex >= 0 ? resolve(argv[planIndex + 1]) : null;
  const accepted = acceptIndex >= 0 ? argv[acceptIndex + 1] : null;
  if (!planPath || !accepted || !/^sha256:[0-9a-f]{64}$/.test(accepted)) {
    process.stderr.write("usage: update-golden-case.mjs apply --plan <file> --accept-plan-sha256 sha256:<hex>\n");
    process.exit(2);
  }
  const plan = JSON.parse(readFileSync(planPath, "utf8"));
  const planBytes = readFileSync(planPath);
  const actualDigest = sha256(planBytes);
  if (actualDigest !== accepted) {
    failGate("golden-update-apply", [{ reason: "plan-digest-mismatch", actual: actualDigest, accepted }]);
  }
  if (!plan.caseId || !plan.after?.files) failGate("golden-update-apply", [{ reason: "plan-shape" }]);

  // The plan must still describe the current preimage (no concurrent drift).
  const { catalog, errors } = loadCatalog();
  if (errors.length > 0) failGate("golden-update-apply", errors);
  const { cases } = loadCases(catalog);
  const entry = cases.find((row) => row.id === plan.caseId);
  if (!entry) failGate("golden-update-apply", [{ reason: "unknown-case", caseId: plan.caseId }]);
  if (entry.revision !== plan.caseRevision) {
    failGate("golden-update-apply", [{ reason: "stale-plan", detail: `case revision moved: ${entry.revision} != ${plan.caseRevision}` }]);
  }
  for (const file of plan.before.files) {
    let current;
    try { current = sha256(readFileSync(join(repoRoot, file.path))); } catch {
      failGate("golden-update-apply", [{ reason: "preimage-missing", path: file.path }]);
    }
    if (current !== file.digest) {
      failGate("golden-update-apply", [{ reason: "preimage-drift", path: file.path }]);
    }
  }

  // Write exactly the listed candidate files, mapped positionally onto
  // the case's declared expected outputs (the same order the plan
  // phase used). A candidate without a declared preimage is refused:
  // new expected files go through a descriptor revision first.
  const declared = entry.descriptor.expected ?? [];
  const planDir = dirname(planPath);
  if (declared.length > 0 && declared.length !== plan.after.files.length) {
    failGate("golden-update-apply", [{ reason: "candidate-count-mismatch", declared: declared.length, candidates: plan.after.files.length }]);
  }
  let applied = 0;
  for (let i = 0; i < plan.after.files.length; i += 1) {
    const file = plan.after.files[i];
    const declaredPath = file.declaredPath ?? declared[i]?.path;
    if (!declaredPath) {
      failGate("golden-update-apply", [{ reason: "unmapped-candidate", path: file.path }]);
    }
    const bytes = readFileSync(join(planDir, file.path));
    writeFileSync(join(repoRoot, declaredPath), bytes);
    applied += 1;
  }
  process.stdout.write(`${JSON.stringify({ ok: true, phase: "apply", caseId: plan.caseId, applied }, null, 2)}\n`);
  process.exit(0);
}

process.stderr.write("usage: update-golden-case.mjs <plan|apply> ...\n");
process.exit(2);
