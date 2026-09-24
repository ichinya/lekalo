#!/usr/bin/env node
// Runtime privacy CLI gate (issue #119, plan S7).
//
// Exercises the `lekalo privacy` surface end to end against the
// fail-closed runtime: the evaluator protocol, the export pipeline
// (deny, transform-required redaction with dry-run, allowed
// publication, the residual-leak refusal, and the consent deny), and
// the read-only redaction diff contract. Fail-closed everywhere: no
// secret, raw prompt, or matched leak value may reach any written
// byte or report.

import assert from "node:assert/strict";
import { mkdir as mkdirDir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, relative as pathRelative } from "node:path";
import { buildLekaloBinary, runLekalo } from "./privacy-runtime-helpers.mjs";

const binary = buildLekaloBinary();
const temp = await mkdtemp(join(tmpdir(), "lekalo-privacy-runtime-"));
const project = join(temp, "project");
await mkdirDir(join(project, ".lekalo"), { recursive: true });

const SECRET = "AKIAABCDEFGHIJKLMNOP";
let cases = 0;

async function write(name, document) {
  const path = join(project, name);
  await writeFile(path, typeof document === "string" ? document : JSON.stringify(document));
  return path;
}

function parseOrThrow(run) {
  assert.equal(run.error, undefined, "the CLI must spawn");
  assert.equal(run.signal, null, "the CLI must not crash");
  return JSON.parse(run.stdout);
}

// The project path policy refuses absolute selections, so every CLI
// invocation runs from the temp directory with temp-relative
// spellings.
function runLekaloInTemp(args) {
  return runLekalo(binary, args, { cwd: temp });
}

function relative(absolutePath) {
  return pathRelative(temp, absolutePath);
}

// ---------------------------------------------------------------------------
// The evaluator protocol.
// ---------------------------------------------------------------------------

const allowed = JSON.parse(await readFile(new URL("../tests/fixtures/privacy/allowed.json", import.meta.url), "utf8"))[0].decision;
const decisionPath = join(temp, "decision.json");
await writeFile(decisionPath, JSON.stringify(allowed));
{
  const run = runLekaloInTemp(["privacy", "evaluate", "--decision", decisionPath]);
  const output = parseOrThrow(run);
  assert.equal(run.status, 0, `allow exits 0: ${run.stderr}`);
  assert.equal(output.decision, "allow");
  assert.equal(output.reasonCodes[0], "policy.allow");
  assert.equal(output.effectiveRefs.policyRef.version, "0.3.2");
  cases += 1;
}

// ---------------------------------------------------------------------------
// Export: transform-required with dry-run and apply; the secret never
// reaches a written byte or the decision record.
// ---------------------------------------------------------------------------

const summaryPath = await write("summary.json", {
  artifactKind: "generated.summary",
  payload: `summary containing ${SECRET} inside`,
  class: ["public"],
});
const consentPath = await write("consent.json", {
  contractId: "dev.lekalo.privacy-authorizing-evidence",
  version: "0.2.16",
  digest: "sha256:cba51a4d9ae21a8d6ad7ebcb98f63410d918b0308ebcbdce1a165e084f52957e",
  evidenceKind: "export-transfer-consent",
  purpose: "authorize-export-transfer-or-storage",
  outcome: "granted",
  evidenceId: `evidence-sha256:${"9".repeat(64)}`,
  verificationState: "verified",
  freshnessState: "current",
});

{
  const run = runLekaloInTemp([
    "privacy", "export", relative(summaryPath),
    "--destination", "publish",
    "--consent", relative(consentPath),
    "--dry-run",
    "--project", "project",
  ]);
  const summary = parseOrThrow(run);
  assert.equal(run.status, 0, `dry-run exits 0: ${run.stderr}`);
  assert.equal(summary.status, "ready");
  assert.equal(summary.decision.decision, "transform-required");
  assert.ok(summary.payload.includes("redacted-content"), "the candidate is the content stub");
  assert.ok(!summary.payload.includes("AKIA"), "the dry-run payload carries no secret");
  assert.equal(summary.written, false, "dry-run writes nothing");
  cases += 1;
}

{
  const run = runLekaloInTemp([
    "privacy", "export", relative(summaryPath),
    "--destination", "publish",
    "--consent", relative(consentPath),
    "--project", "project",
  ]);
  const summary = parseOrThrow(run);
  assert.equal(run.status, 0, `apply exits 0: ${run.stderr}`);
  assert.equal(summary.written, true);
  cases += 1;

  const exportBytes = await readFile(join(project, ".lekalo/privacy/exports/summary.json"), "utf8");
  assert.ok(!exportBytes.includes("AKIA"), "the written export carries no secret");
  const recordBytes = await readFile(
    join(project, ".lekalo/privacy/decisions/export/summary.json"),
    "utf8",
  );
  assert.ok(!recordBytes.includes("AKIA"), "the decision record carries no secret");
  assert.ok(recordBytes.includes("transform-required"));
}

// ---------------------------------------------------------------------------
// Export: the deny path (raw prompt publication), nothing written.
// ---------------------------------------------------------------------------

{
  const promptPath = await write("prompt.json", {
    artifactKind: "ai.prompt",
    payload: "the raw user prompt",
    class: ["confidential"],
  });
  const run = runLekaloInTemp([
    "privacy", "export", relative(promptPath),
    "--destination", "publish",
    "--project", "project",
  ]);
  const output = parseOrThrow(run);
  assert.equal(run.status, 3, "deny exits 3");
  assert.equal(output.decision, "deny");
  assert.equal(output.reasonCodes[0], "disposition.forbidden-dominates");
  cases += 1;
}

// ---------------------------------------------------------------------------
// Export: residual leaks never silently ship. A hygiene-class leak
// (a URL) ships pseudonymized; a secret-class leak refuses.
// ---------------------------------------------------------------------------

{
  const leakyPath = await write("leaky.json", {
    artifactKind: "fixture",
    payload: "see https://private.example/acme",
    class: ["public"],
    synthetic: true,
  });
  const run = runLekaloInTemp([
    "privacy", "export", relative(leakyPath),
    "--destination", "publish",
    "--project", "project",
  ]);
  const summary = parseOrThrow(run);
  assert.equal(run.status, 0, "the hygiene-class leak ships pseudonymized");
  assert.ok(summary.payload.includes("<redacted:url>"), summary.payload);
  assert.ok(!summary.payload.includes("https://"), summary.payload);
  cases += 1;

  const secretPath = await write("secret-leak.json", {
    artifactKind: "fixture",
    payload: `token ghp_${"abcdefghijklmnopqrstuvwxyz0123456789"}abcd`,
    class: ["public"],
    synthetic: true,
  });
  const secretRun = runLekaloInTemp([
    "privacy", "export", relative(secretPath),
    "--destination", "publish",
    "--project", "project",
  ]);
  const refusal = parseOrThrow(secretRun);
  assert.equal(secretRun.status, 3, "the leak refusal exits 3");
  assert.equal(refusal.status, "refused-leaks");
  assert.ok(refusal.reasonCodes.includes("leak.secret-token"), JSON.stringify(refusal.reasonCodes));
  assert.ok(!JSON.stringify(refusal).includes("ghp_"), "no leak value in the report");
  cases += 1;
}

// ---------------------------------------------------------------------------
// Export: an allowed synthetic fixture publishes as-is.
// ---------------------------------------------------------------------------

{
  const fixturePath = await write("fixture.json", {
    artifactKind: "fixture",
    payload: "synthetic fixture text",
    class: ["public"],
    synthetic: true,
  });
  const run = runLekaloInTemp([
    "privacy", "export", relative(fixturePath),
    "--destination", "publish",
    "--project", "project",
  ]);
  const summary = parseOrThrow(run);
  assert.equal(run.status, 0, "the allow exits 0");
  assert.equal(summary.decision.decision, "allow");
  assert.equal(summary.payload, "synthetic fixture text");
  assert.equal(summary.written, true);
  cases += 1;
}

// ---------------------------------------------------------------------------
// Export: a shareable transfer without consent denies.
// ---------------------------------------------------------------------------

{
  const run = runLekaloInTemp([
    "privacy", "export", relative(summaryPath),
    "--destination", "transfer-tenant",
    "--project", "project",
  ]);
  const output = parseOrThrow(run);
  assert.equal(run.status, 3, "the consent deny exits 3");
  assert.equal(output.decision, "deny");
  assert.equal(output.reasonCodes[0], "provenance.consent-required");
  cases += 1;
}

// ---------------------------------------------------------------------------
// Export: a missing class refuses before evaluation, and an unknown
// destination is a malformed invocation.
// ---------------------------------------------------------------------------

{
  const classless = await write("classless.json", {
    artifactKind: "generated.summary",
    payload: "text",
  });
  const run = runLekaloInTemp([
    "privacy", "export", relative(classless),
    "--destination", "workspace",
    "--project", "project",
  ]);
  assert.equal(run.status, 3, "the class refusal exits 3");
  assert.ok(run.stdout.includes("privacy.class-missing"), run.stdout);
  cases += 1;

  const unknown = runLekaloInTemp([
    "privacy", "export", summaryPath,
    "--destination", "elsewhere",
    "--project", "project",
  ]);
  assert.equal(unknown.status, 1, "unknown destination exits 1");
  assert.ok(unknown.stderr.includes("privacy.destination-unknown"), unknown.stderr);
  cases += 1;
}

// ---------------------------------------------------------------------------
// Redact: the read-only diff contract.
// ---------------------------------------------------------------------------

{
  const payloadPath = await write(
    "leak.txt",
    "mail bob@corp.example token ghp_abcdefghijklmnopqrstuvwxyz0123456789abcd",
  );
  const run = runLekaloInTemp(["privacy", "redact", "--payload", relative(payloadPath), "--dry-run"]);
  const report = parseOrThrow(run);
  assert.equal(run.status, 0, "redact exits 0");
  assert.equal(report.status, "ready");
  const kinds = report.diff.map((finding) => finding.kind);
  assert.ok(kinds.includes("secret-token"), JSON.stringify(kinds));
  assert.ok(kinds.includes("email"), JSON.stringify(kinds));
  assert.ok(!report.redacted.includes("bob@corp.example"));
  assert.ok(!report.redacted.includes("ghp_"));
  assert.ok(report.redacted.includes("<redacted:secret>"));
  assert.ok(report.redacted.includes("<redacted:pii>"));
  cases += 1;
}

// The reserved privacy homes carry exactly the pipeline outputs.
const exported = await readdir(join(project, ".lekalo/privacy/exports"));
const recorded = await readdir(join(project, ".lekalo/privacy/decisions/export"));
assert.deepEqual(exported.sort(), ["fixture.json", "leaky.json", "summary.json"]);
assert.deepEqual(recorded.sort(), ["fixture.json", "leaky.json", "summary.json"]);

await rm(temp, { recursive: true, force: true });
console.log(JSON.stringify({ ok: true, cases, binary: "target/[debug|release]/lekalo" }));
