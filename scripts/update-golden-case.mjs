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
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync } from "node:fs";
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
  assertRepoPath,
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

  const sandboxBase = outDir ? resolve(outDir) : realpathSync.native(mkdtempSync(join(tmpdir(), "lekalo-golden-plan-")));
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
  // Field-wise semantic diff of every changed golden: status changes,
  // reason-code additions/removals, diagnostic count/identity/severity
  // deltas, then a byte-level fallback. Hashes appear only as
  // anchors, never as the review surface.
  const describeEnvelopeDelta = (beforeText, afterText) => {
    const rows = [];
    let beforeDoc;
    let afterDoc;
    try {
      beforeDoc = JSON.parse(beforeText.trim());
      afterDoc = JSON.parse(afterText.trim());
    } catch {
      rows.push({ kind: "bytes-changed-explained", detail: "non-JSON golden bytes changed" });
      return rows;
    }
    if (beforeDoc.status !== afterDoc.status) {
      rows.push({ kind: "bytes-changed-explained", detail: `status: ${beforeDoc.status} -> ${afterDoc.status}` });
    }
    const beforeCodes = beforeDoc.reasonCodes ?? [];
    const afterCodes = afterDoc.reasonCodes ?? [];
    const added = afterCodes.filter((code) => !beforeCodes.includes(code));
    const removed = beforeCodes.filter((code) => !afterCodes.includes(code));
    if (added.length > 0) {
      rows.push({ kind: "reason-codes-changed", detail: `added reason codes: ${added.join(", ")}` });
    }
    if (removed.length > 0) {
      rows.push({ kind: "reason-codes-changed", detail: `removed reason codes: ${removed.join(", ")}` });
    }
    const beforeDiag = beforeDoc.diagnostics ?? [];
    const afterDiag = afterDoc.diagnostics ?? [];
    if (beforeDiag.length !== afterDiag.length) {
      rows.push({
        kind: afterDiag.length > beforeDiag.length ? "diagnostics-added" : "diagnostics-removed",
        detail: `diagnostic count: ${beforeDiag.length} -> ${afterDiag.length}`,
      });
    }
    const beforeIds = new Set(beforeDiag.map((row) => `${row.id}:${row.severity}`));
    for (const diag of afterDiag) {
      const key = `${diag.id}:${diag.severity}`;
      if (!beforeIds.has(key)) {
        if (beforeDiag.some((row) => row.id === diag.id) && beforeDiag.find((row) => row.id === diag.id)?.severity !== diag.severity) {
          rows.push({ kind: "severity-changed", detail: `${diag.id}: ${beforeDiag.find((row) => row.id === diag.id)?.severity} -> ${diag.severity}` });
        }
      }
    }
    if (rows.length === 0) {
      rows.push({ kind: "bytes-changed-explained", detail: "envelope bytes changed with identical status/reason/diagnostic surface (message, data, or framing)" });
    }
    return rows;
  };
  if (declared.length === candidateFiles.length) {
    for (let i = 0; i < declared.length; i += 1) {
      const declaredPath = declared[i].path;
      const beforeText = readFileSync(join(repoRoot, declaredPath), "utf8");
      const newDigest = candidateFiles[i].digest;
      const oldDigest = sha256(Buffer.from(beforeText, "utf8"));
      if (oldDigest !== newDigest) {
        for (const row of describeEnvelopeDelta(beforeText, candidateFiles[i].bytes)) {
          semanticChanges.push({ kind: row.kind, detail: `${declaredPath}: ${row.detail}` });
        }
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
    // before.digest binds the CURRENT tracked golden bytes (the
    // preimages apply will replace); after.digest binds the exact
    // candidate bytes apply will publish. Both are plain sha256 over
    // the ordered digest list, so a reviewer can recompute either.
    before: {
      digest: sha256(beforeFiles.map((row) => row.digest).join("\n")),
      files: beforeFiles,
    },
    after: {
      digest: sha256(candidateFiles.map((row) => row.digest).join("\n")),
      // Only byte-changed files are listed for publication; unchanged
      // goldens are not rewritten (the closed schema's change enum is
      // added|modified|removed — no no-op rows).
      files: candidateFiles
        .filter((row) => beforeMap.get(row.declaredPath ?? row.path) !== row.digest)
        .map((row) => ({
          path: row.path,
          declaredPath: row.declaredPath,
          digest: row.digest,
          change: beforeMap.has(row.declaredPath ?? row.path) ? "modified" : "added",
        })),
    },
    summary: {
      semanticChanges,
      humanExplanation: reason,
    },
  };
  // The semantic summary is printed for the reviewing maintainer
  // (hashes alone are never the review surface); stdout keeps the
  // single-JSON protocol the policy gate parses.
  process.stderr.write(`${JSON.stringify({
    phase: "semantic-summary",
    caseId,
    semanticSummary: semanticChanges,
    changedFiles: plan.after.files.filter((row) => row.change === "modified").map((row) => row.declaredPath ?? row.path),
  }, null, 2)}\n`);
  const planText = `${JSON.stringify(plan, null, 2)}\n`;
  // The produced plan must satisfy the closed v1.0.0 schema.
  {
    const { validateGoldenUpdatePlan } = await import(new URL(`file:///${join(here, "lib/golden-schema-validation.mjs").split("\\").join("/")}`).href);
    const violations = validateGoldenUpdatePlan(plan);
    if (violations.length > 0) {
      failGate("golden-update-plan", [{ reason: "plan-schema", errors: violations.slice(0, 6) }]);
    }
  }
  const planPath = join(sandboxBase, `${plan.planId}.json`);
  writeFileSync(planPath, planText);
  // Candidate bytes are written beside the plan for human review: the
  // exact reviewed bytes (the digest in the plan covers exactly these,
  // with no extra newline added on write).
  for (const row of candidateFiles) {
    const target = join(sandboxBase, row.path);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, row.bytes);
  }
  const planDigest = sha256(planText);
  process.stdout.write(`${JSON.stringify({
    ok: true,
    phase: "plan-written",
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
  {
    const { validateGoldenUpdatePlan } = await import(new URL(`file:///${join(here, "lib/golden-schema-validation.mjs").split("\\").join("/")}`).href);
    const violations = validateGoldenUpdatePlan(plan);
    if (violations.length > 0) {
      failGate("golden-update-apply", [{ reason: "plan-schema", errors: violations.slice(0, 6) }]);
    }
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

  // Write exactly the listed candidate files. Preflight everything
  // before any write: candidate bytes must hash to the reviewed plan
  // digest and each destination must be the descriptor-owned declared
  // path (path-policy checked; an accepted plan cannot redirect
  // writes outside the case's declared expected outputs).
  const declared = entry.descriptor.expected ?? [];
  const planDir = dirname(planPath);
  const declaredPaths = new Set(declared.map((row) => row.path));
  const preflight = [];
  for (const file of plan.after.files) {
    const declaredPath = file.declaredPath;
    if (!declaredPath) {
      failGate("golden-update-apply", [{ reason: "unmapped-candidate", path: file.path }]);
    }
    // Destination must be a declared expected output of this case —
    // an accepted plan cannot redirect writes elsewhere.
    if (!declaredPaths.has(declaredPath)) {
      failGate("golden-update-apply", [{ reason: "destination-not-declared", declaredPath, caseId: plan.caseId }]);
    }
    try {
      assertRepoPath(declaredPath, `apply ${plan.caseId}`);
    } catch (error) {
      failGate("golden-update-apply", [{ reason: "destination-path-policy", detail: error.message }]);
    }
    // Candidate bytes must match the reviewed digest exactly; a
    // post-review tamper of the candidate file is refused here.
    const candidatePath = join(planDir, file.path);
    let bytes;
    try {
      bytes = readFileSync(candidatePath);
    } catch {
      failGate("golden-update-apply", [{ reason: "candidate-missing", path: file.path }]);
    }
    const candidateDigest = sha256(bytes);
    if (candidateDigest !== file.digest) {
      failGate("golden-update-apply", [{ reason: "candidate-tampered", path: file.path, plan: file.digest, actual: candidateDigest }]);
    }
    preflight.push({ declaredPath, bytes });
  }
  // All reads succeeded; publish atomically in one pass, then refresh
  // the case's checksum sidecar so the reviewed write is self-contained
  // (the catalog gate verifies the sidecar against the tracked bytes,
  // so a stale sidecar would fail the gate immediately after apply).
  let applied = 0;
  for (const { declaredPath, bytes } of preflight) {
    writeFileSync(join(repoRoot, declaredPath), bytes);
    applied += 1;
  }
  {
    const caseDir = entry.path.split("/").slice(0, -1).join("/");
    const { readdirSync: caseReaddir, statSync: caseStat } = await import("node:fs");
    const files = [];
    const walkCase = (current, logical) => {
      for (const child of caseReaddir(current).sort()) {
        const full = join(current, child);
        const childLogical = `${logical}/${child}`;
        if (caseStat(full).isDirectory()) walkCase(full, childLogical);
        else files.push({ path: childLogical, sha256: sha256(readFileSync(full)) });
      }
    };
    walkCase(join(repoRoot, caseDir), caseDir);
    const sidecar = {
      caseId: entry.id,
      revision: entry.revision,
      algorithm: "sha256",
      files,
    };
    writeFileSync(
      join(repoRoot, SUITE_V1, "checksums", `${entry.id}.json`),
      `${JSON.stringify(sidecar, null, 2)}\n`,
    );
  }
  process.stdout.write(`${JSON.stringify({ ok: true, phase: "apply", caseId: plan.caseId, applied, checksumsRefreshed: true }, null, 2)}\n`);
  process.exit(0);
}

process.stderr.write("usage: update-golden-case.mjs <plan|apply> ...\n");
process.exit(2);
