#!/usr/bin/env node
// Issue #87 end-to-end classification surface: the CLI binary over the
// committed fixtures. Proves the classification validate/inspect and
// dataflow report surfaces, the strict/deny exits, subject resolution
// failures, the credential declassification seal, and the non-disclosure
// byte-scan: no fixture artifact ever carries the sentinel secret value.

import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const binary = process.env.LEKALO_BIN ?? join(root, "target", "debug", "lekalo.exe");

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

const run = (args, cwd) => {
  try {
    return {
      code: 0,
      stdout: execFileSync(binary, args, { cwd, encoding: "utf8" }),
    };
  } catch (error) {
    return {
      code: error.status ?? 1,
      stdout: error.stdout ?? "",
      stderr: error.stderr ?? "",
    };
  }
};

// 1. The binary exists (cargo build must run first).
let binaryOk = false;
try {
  binaryOk = statSync(binary).isFile();
} catch {
  binaryOk = false;
}
if (!binaryOk) fail("binary-missing", `run: cargo build -p lekalo-cli (${binary})`);

const VALID = join(root, "tests/fixtures/classification/valid/planner");
const DECLASSIFIED = join(root, "tests/fixtures/classification/valid/declassified-export");
const UNKNOWN = join(root, "tests/fixtures/classification/invalid/unknown-subject");
const SEALED = join(root, "tests/fixtures/classification/invalid/credential-declassified");
const UNCLOSED = join(root, "tests/fixtures/classification/invalid/unclosed-policy");
const attachment = (fixture) => join(fixture, "lekalo/classification.json");
const policy = (fixture) => join(fixture, "lekalo/classification-policy.json");

// 2. The valid fixture validates, inspects, and reports a pass verdict.
{
  const inspect = run(
    ["classification", "inspect", "--attachment", attachment(VALID), "--policy", policy(VALID), "--json"],
    VALID,
  );
  if (inspect.code !== 0) fail("inspect-valid-exit", inspect);
  const document = JSON.parse(inspect.stdout);
  const personal = document.subjects.filter((row) => row.kind === "personal");
  if (personal.length !== 2) fail("inspect-personal-count", personal);
  const validate = run(
    ["classification", "validate", "--attachment", attachment(VALID), "--policy", policy(VALID)],
    VALID,
  );
  if (validate.code !== 0) fail("validate-valid-exit", validate);
  const report = run(
    ["dataflow", "report", "--attachment", attachment(VALID), "--policy", policy(VALID), "--json"],
    VALID,
  );
  if (report.code !== 0) fail("report-valid-exit", report);
  const parsed = JSON.parse(report.stdout).report;
  if (parsed.verdict !== "pass") fail("report-verdict", parsed.verdict);
  if (parsed.schemaVersion !== "lekalo/data-flow-report/v0.4.0") {
    fail("report-identity", parsed.schemaVersion);
  }
  // The report carries metadata only: no source text, no physical paths.
  for (const banned of ["entities.yaml", "target/debug", "C:\\\\Users", "/Users/"]) {
    if (report.stdout.includes(banned)) fail("report-carries-path", banned);
  }
}

// 3. The declassified export: personal -> derived through the reviewed
// grant stays valid and the grant subject resolves to `derived`.
{
  const inspect = run(
    [
      "classification",
      "inspect",
      "--attachment",
      attachment(DECLASSIFIED),
      "--policy",
      policy(DECLASSIFIED),
      "--json",
    ],
    DECLASSIFIED,
  );
  if (inspect.code !== 0) fail("declassify-inspect-exit", inspect);
  const document = JSON.parse(inspect.stdout);
  const row = document.subjects.find((entry) => entry.subject === "notify.user");
  if (!row || row.kind !== "derived") fail("declassify-not-lowered", row);
  const validate = run(
    [
      "classification",
      "validate",
      "--attachment",
      attachment(DECLASSIFIED),
      "--policy",
      policy(DECLASSIFIED),
    ],
    DECLASSIFIED,
  );
  if (validate.code !== 0) fail("declassify-validate-exit", validate);
}

// 4. Unknown subjects fail closed (invalid, exit 1).
{
  const outcome = run(
    ["classification", "validate", "--attachment", attachment(UNKNOWN), "--policy", policy(UNKNOWN)],
    UNKNOWN,
  );
  if (outcome.code !== 1) fail("unknown-subject-exit", outcome);
  if (!outcome.stderr.includes("classification.unknown-subject")) {
    fail("unknown-subject-rule", outcome.stderr);
  }
}

// 5. The credential seal: a downward grant on a credential subject is a
// structured invalid set, never a silent pass.
{
  const outcome = run(
    ["classification", "validate", "--attachment", attachment(SEALED), "--policy", policy(SEALED)],
    SEALED,
  );
  if (outcome.code !== 1) fail("credential-seal-exit", outcome);
  if (!outcome.stderr.includes("classification.invalid-declassification")) {
    fail("credential-seal-rule", outcome.stderr);
  }
}

// 6. Policy coverage: an unclassified-resolvable kind without a policy
// row is a kind-rule-missing finding (invalid, exit 1).
{
  const outcome = run(
    ["classification", "validate", "--attachment", attachment(UNCLOSED), "--policy", policy(UNCLOSED)],
    UNCLOSED,
  );
  if (outcome.code !== 1) fail("unclosed-policy-exit", outcome);
  if (!outcome.stderr.includes("classification.kind-rule-missing")) {
    fail("unclosed-policy-rule", outcome.stderr);
  }
}

// 7. Non-disclosure byte-scan: the sentinel secret value never appears
// in any committed classification fixture artifact or in any CLI output
// over them. Classification metadata flows; values never do.
const SENTINEL = "LEKALO-SENTINEL-SECRET-9f2c";
{
  const fixtures = join(root, "tests/fixtures/classification");
  const walk = (dir) => {
    const found = [];
    for (const entry of readdirSync(dir)) {
      const path = join(dir, entry);
      if (statSync(path).isDirectory()) found.push(...walk(path));
      else found.push(path);
    }
    return found;
  };
  for (const path of walk(fixtures)) {
    const bytes = readFileSync(path, "utf8");
    if (bytes.includes(SENTINEL)) fail("sentinel-in-fixture", path);
  }
  for (const fixture of [VALID, DECLASSIFIED, UNKNOWN, SEALED, UNCLOSED]) {
    for (const command of [
      ["classification", "validate", "--attachment", attachment(fixture), "--policy", policy(fixture)],
      ["classification", "inspect", "--attachment", attachment(fixture), "--policy", policy(fixture)],
      ["dataflow", "report", "--attachment", attachment(fixture), "--policy", policy(fixture), "--json"],
    ]) {
      const outcome = run(command, fixture);
      const output = `${outcome.stdout}${outcome.stderr}`;
      if (output.includes(SENTINEL)) fail("sentinel-in-output", { fixture, command });
    }
  }
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  fixtures: {
    valid: 1,
    declassified: 1,
    invalid: 3,
  },
  sentinelScanned: true,
}, null, 2)}\n`);
