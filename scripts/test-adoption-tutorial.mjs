#!/usr/bin/env node
// Issue #136 tutorial replay gate (AC7): the incremental-adoption
// walkthrough is executable. Every ```sh block of
// docs/tutorial-incremental-adoption.md is extracted verbatim and run in
// order against a freshly built binary — staging, the read-only
// analysis, the selective confirmation, the conformed legacy command,
// the verified refactor, the authored contracted feature, the hybrid
// join, and the recovery surface — with independent assertions between
// the steps. The blocks are never copied here: this gate runs the
// document itself, so any doc drift breaks CI. Requires a POSIX shell
// (bash) and node on PATH; `lekalo` resolves to the freshly built
// binary through PATH precedence. Runs after `cargo build --workspace
// --locked` in the same job.
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};
const BINARY = join(root, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
if (!existsSync(BINARY)) fail("binary-missing", "cargo build --workspace --locked first");

const DOC = join(root, "docs", "tutorial-incremental-adoption.md");
const doc = readFileSync(DOC, "utf8");
const blocks = [];
for (const match of doc.matchAll(/```sh\n([\s\S]*?)```/g)) {
  const lines = match[1].split("\n").filter((line) => line.trim() !== "");
  if (lines.length > 0) blocks.push(lines);
}
if (blocks.length < 8) fail("blocks-missing", { extracted: blocks.length });
const stage = blocks[0].join("\n");
if (!stage.includes("rm -rf .adoption-tutorial")) {
  fail("stage-block-missing", "the first block must stage .adoption-tutorial");
}
const recoveryIndex = blocks.findIndex((lines) => lines.some((line) => line.includes("adoption recovery")));
if (recoveryIndex < 0) fail("recovery-block-missing");
const cleanupIndex = blocks.findIndex((lines) => lines.some((line) => line.startsWith("rm -rf .adoption-tutorial")) && lines.some((line) => line.startsWith("cd ..")));
if (cleanupIndex < 0 || cleanupIndex !== blocks.length - 1) fail("cleanup-block-missing");
rmSync(join(root, ".adoption-tutorial"), { recursive: true, force: true });

const binDir = dirname(BINARY);
const env = { ...process.env, PATH: `${binDir}${process.platform === "win32" ? ";" : ":"}${process.env.PATH}` };
let cwd = root;
let blockSeq = 0;
const scriptDir = join(root, "target", "adoption-tutorial-scripts");
rmSync(scriptDir, { recursive: true, force: true });
mkdirSync(scriptDir, { recursive: true });
const runBlocks = (from, upto) => {
  for (let index = from; index < upto; index += 1) {
    let current = [];
    const segments = [];
    for (const line of blocks[index]) {
      const cd = /^cd (.+)$/.exec(line.trim());
      if (cd) {
        if (current.length > 0) segments.push({ cwd, lines: current });
        current = [];
        cwd = resolve(cwd, cd[1]);
        continue;
      }
      current.push(line);
    }
    if (current.length > 0) segments.push({ cwd, lines: current });
    for (const segment of segments) {
      // The exact document lines go to a script file: no shell or argv
      // re-quoting layer sits between the doc and the shell, and the
      // segment's working directory travels as an explicit cd — MSYS
      // bash does not reliably honor a spawned cwd once directories
      // were recreated under it. The recovery-deferral segment
      // intentionally ends exit-3 (the documented rollback-surface
      // refusal).
      const expectsRefusal = segment.lines.some((line) => line.includes("--restore"));
      blockSeq += 1;
      const scriptPath = join(scriptDir, `block-${blockSeq}.sh`);
      const cdLine = `cd '${segment.cwd.replace(/\\/g, "/")}' || exit 70`;
      writeFileSync(scriptPath, `${cdLine}\n${segment.lines.join("\n")}`);
      const result = spawnSync("bash", [scriptPath], {
        cwd: segment.cwd,
        encoding: "utf8",
        maxBuffer: 64 * 1024 * 1024,
        env,
      });
      const expected = expectsRefusal ? 3 : 0;
      if (result.status !== expected) {
        fail("tutorial-command", {
          block: index,
          script: segment.lines.join("\n").slice(0, 160),
          status: result.status,
          expected,
          stderr: (result.stderr || "").slice(0, 600),
          stdout: (result.stdout || "").slice(0, 300),
        });
      }
    }
  }
};

const readArtifact = (name) => {
  const path = join(root, ".adoption-tutorial", name);
  if (!existsSync(path)) fail("artifact-missing", name);
  return JSON.parse(readFileSync(path, "utf8"));
};

try {
  // Everything through the recovery surface, then the assertions.
  runBlocks(0, recoveryIndex + 1);

  const confirmation = readArtifact("confirmation-receipt.json");
  if (confirmation.outcome !== "completed") fail("confirmation-outcome", confirmation.outcome);
  const contractCheck = readArtifact("contract-check.json");
  if (contractCheck.status !== "valid" || contractCheck.conformant !== 1) fail("legacy-check", contractCheck);
  const execution = readArtifact("execution.json");
  if (execution.conclusion !== "preserved" || execution.oracle.status !== "passed") fail("execution", execution.conclusion);
  if (!execution.custody || !execution.custody.run) fail("execution-custody", execution.custody);
  const apply = readArtifact("apply-receipt.json");
  if (apply.outcome !== "completed") fail("apply-outcome", apply.outcome);
  const featureCheck = readArtifact("feature-check.json");
  if (featureCheck.status !== "valid" || featureCheck.conformant !== 2) fail("feature-check", featureCheck);
  const coverage = readArtifact("coverage.json");
  if (coverage.counts.contracted_legacy < 1 || coverage.counts.contracted_new < 1) {
    fail("three-modes", coverage.counts);
  }
  const auditRow = coverage.rows.find((row) => row.symbol === "taskboard.audit_event");
  if (!auditRow || auditRow.effective !== "contracted-new" || !auditRow.verification) {
    fail("executed-provenance-join", auditRow);
  }
  const status = readArtifact("status.json");
  if (status.panels.some((panel) => panel.state === "current" && panel.freshness !== "fresh")) {
    fail("fresh-panels", status.panels);
  }
  const trace = readArtifact("trace.json");
  if (!Array.isArray(trace.verification) || trace.verification.length === 0) fail("trace-join", trace.verification);
  const recoveryView = readArtifact("recovery-view.json");
  if (recoveryView.plan !== apply.plan || recoveryView.intact !== true) fail("recovery-view", recoveryView);
  const recoveryRefusal = readArtifact("recovery-refusal.json");
  if (!recoveryRefusal.diagnostics.some((item) => item.code === "LEK-ADOPT-009")) {
    fail("recovery-defers-to-rollback", recoveryRefusal.diagnostics);
  }

  // The documented cleanup.
  runBlocks(cleanupIndex, cleanupIndex + 1);
  if (existsSync(join(root, ".adoption-tutorial"))) fail("cleanup-leftover");
} finally {
  rmSync(join(root, ".adoption-tutorial"), { recursive: true, force: true });
}
process.stdout.write(`${JSON.stringify({ ok: true, gate: "adoption-tutorial", blocks: blocks.length })}\n`);
