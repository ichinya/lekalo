#!/usr/bin/env node
/**
 * Issue #118 CI gate: run the brownfield pilot harness end to end
 * against the committed synthetic fixture and assert the full
 * observed-mode path — adopt dry-run purity, the scan (wire refused
 * for recorded reasons, in-process fallback completing), the recorded
 * and confirmed use-case binding, the inspect/impact/context
 * projections, the controlled-change staleness gate, and the closed
 * metrics schema (top-level members, per-step detail allowlists, leak
 * probes over both emitted artifacts). Dependency-free; run from the
 * repo root. The lekalo binary is built when missing (CI builds it in
 * a prior step anyway).
 */
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const harness = join(repoRoot, "scripts", "pilot-brownfield-ts.mjs");
const fixture = join(repoRoot, "tests", "fixtures", "pilot", "brownfield-ts");
const exe = process.platform === "win32" ? "lekalo.exe" : "lekalo";
const binary = join(repoRoot, "target", "debug", exe);

// Prefer building (the committed gate proves the build too); other
// gates assume a prior CI build step, this one does not.
if (!existsSync(binary)) {
  const build = spawnSync("cargo", ["build", "-p", "lekalo-cli"], {
    cwd: repoRoot, encoding: "utf8", timeout: 900_000, stdio: "inherit",
  });
  if (build.status !== 0) {
    process.stderr.write(
      `${JSON.stringify({ ok: false, gate: "pilot-brownfield-ts", reason: "build-failed" })}\n`,
    );
    process.exit(1);
  }
}

const outDir = resolve(join(tmpdir(), `lekalo-pilot-brownfield-ts-gate-${process.pid}`));
const run = spawnSync(process.execPath, [
  harness,
  "--consumer", fixture,
  "--bind", "packages/api/src/routes/tasks.ts:submitTask:task.submit",
  "--disposition", "public-fixture",
  "--out", outDir,
], { cwd: repoRoot, encoding: "utf8", timeout: 900_000, maxBuffer: 64 * 1024 * 1024 });

// The closed per-step detail member sets: a step's detail entry may
// never carry a member outside its allowlist.
const DETAIL_ALLOWLIST = {
  copy: ["files", "skippedDirEntries"],
  "in-place": ["accepted", "note"],
  "adopt-dry-run": ["plannedWrites", "purity"],
  adopt: ["projectIdDigest", "created"],
  profile: ["readRoots", "exclusions", "packages"],
  "scan-wire": ["used", "variant", "manifestedRefusal", "localDevRefusal", "symbols", "endpoints"],
  "scan-fallback": [
    "used", "note", "coldMs", "warmMs", "warmByteIdentical", "scannedSymbols",
    "scanEntries", "uncertaintyRows", "diagnostics", "endpoints", "unboundRoutes",
    "testBindings", "carrierlessTests", "frameworkProviders",
  ],
  modules: ["modules", "note"],
  "observe-update": ["symbols", "endpoints", "inferred", "explicit", "note"],
  "bind-use-case": ["state", "binding"],
  "confirm-handler-binding": ["confirmed", "ambiguousProposals", "note"],
  "attach-native-tests": ["attached", "source"],
  "promote-use-case": ["symbols"],
  inspect: ["binding", "state", "promoted", "nativeTests", "endpoints", "completeness"],
  impact: ["dependents", "recordedSymbols", "stale", "unknown", "completeness"],
  context: [
    "budget", "estimatedTokens", "fits", "includedSections", "excludedSections",
    "gaps", "complete", "markdownBytes",
  ],
  baseline: ["symbols"],
  "bindings-list": [
    "bindings", "endpoints", "tests", "explicit", "confirmed", "inferred",
    "current", "stale", "unknown",
  ],
  "status-doctor": ["statusOk", "doctorVerdict"],
  "controlled-change": ["strategy", "staleDiagnostics", "checkFailed", "cleanAfterRevert"],
  report: ["reportBytes", "metricsBytes"],
};

try {
  const metrics = JSON.parse(readFileSync(join(outDir, "metrics.json"), "utf8"));
  const report = readFileSync(join(outDir, "report.md"), "utf8");
  const step = (name) => metrics.steps.find((entry) => entry.step === name);

  // The whole harness flow answers green.
  assert.equal(run.status, 0, `harness exited ${run.status}:\n${run.stdout?.slice(-2000)}${run.stderr?.slice(-2000)}`);

  // Adopt dry-run purity held, and adoption fixed the project.
  assert.equal(step("adopt-dry-run").purity, "intact");
  assert.equal(step("adopt").created >= 1, true);

  // The scan completed through the fallback with an honest wire record.
  assert.equal(step("scan-wire").used, false, "the manifested wire scan must refuse for the monorepo fixture");
  assert.match(String(step("scan-wire").manifestedRefusal), /^adapter\.manifest-mismatch\//);
  const fallback = step("scan-fallback");
  assert.equal(fallback.used, true);
  assert.equal(fallback.warmByteIdentical, true);
  assert.ok(fallback.scannedSymbols >= 6, "the fixture scan saw the workspace symbols");
  assert.ok(fallback.endpoints >= 1, "the POST /tasks route resolved to an indexed handler");
  assert.ok(fallback.scanEntries >= 6, "the scan document carried the exported surface");

  // The use case is recorded, confirmed, promoted, and projectable.
  assert.equal(step("bind-use-case").binding, "explicit");
  assert.equal(step("confirm-handler-binding").confirmed, 1);
  assert.equal(step("promote-use-case").symbols, 1);
  assert.equal(step("inspect").promoted, true);
  assert.equal(step("impact").recordedSymbols, fallback.scanEntries);
  assert.equal(step("context").fits, true);

  // The controlled change fired staleness and the revert cleaned it.
  const mutation = step("controlled-change");
  assert.equal(mutation.staleDiagnostics >= 1, true);
  assert.equal(mutation.checkFailed, true);
  assert.equal(mutation.cleanAfterRevert, true);

  // The metrics schema is closed: exactly the declared top-level
  // members, no paths, no consumer identifiers.
  assert.deepEqual(
    Object.keys(metrics).sort(),
    ["consumer", "disposition", "mode", "projections", "schema", "steps", "totals"],
  );
  assert.match(metrics.consumer.pathDigest, /^sha256:[0-9a-f]{64}$/);
  assert.equal(metrics.consumer.label, "consumer");
  const flattened = JSON.stringify(metrics) + report;
  for (const leak of ["fixtures", "packages/api", "submitTask", "tasks.ts", "consumer-copy", ":\\"].map((needle) => needle.toLowerCase())) {
    assert.equal(flattened.toLowerCase().includes(leak), false, `artifact leak: ${leak}`);
  }

  // Every step record carries only its schema members: the fixed
  // record spine plus the step's allowlisted detail members (the
  // harness spreads details flat; `error` rides failures only).
  for (const entry of metrics.steps) {
    for (const member of Object.keys(entry)) {
      assert.ok(
        ["step", "status", "required", "durationMs", "error"].includes(member)
        || DETAIL_ALLOWLIST[entry.step]?.includes(member),
        `step ${entry.step} carries an unexpected record member "${member}"`,
      );
    }
    assert.ok(["ok", "failed"].includes(entry.status));
    const allowlist = DETAIL_ALLOWLIST[entry.step];
    assert.ok(Array.isArray(allowlist), `no detail allowlist declared for step ${entry.step}`);
  }
  assert.deepEqual(
    metrics.steps.filter((entry) => entry.step !== "copy" && entry.step !== "in-place").map((entry) => entry.step),
    Object.keys(DETAIL_ALLOWLIST).filter((name) => name !== "copy" && name !== "in-place"),
    "the recorded step sequence must match the closed allowlist",
  );
  assert.equal(
    metrics.steps.filter((entry) => entry.step === "copy" || entry.step === "in-place").length,
    1,
    "exactly one copy-discipline step may be recorded",
  );
} catch (error) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, gate: "pilot-brownfield-ts", reason: String(error?.message ?? error).slice(0, 400) })}\n`,
  );
  rmSync(outDir, { recursive: true, force: true });
  process.exit(1);
}
rmSync(outDir, { recursive: true, force: true });

process.stdout.write(`${JSON.stringify({ ok: true, gate: "pilot-brownfield-ts" })}\n`);
