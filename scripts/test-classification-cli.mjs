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
const binary = process.env.LEKALO_BIN ??
  join(root, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");

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
const EXPIRED = join(root, "tests/fixtures/classification/invalid/expired-public-grant");
const CROSSING = join(root, "tests/fixtures/classification/invalid/tenant-crossing");
const SINK = join(root, "tests/fixtures/classification/invalid/secret-in-sink");
const SEALED = join(root, "tests/fixtures/classification/invalid/credential-declassified");
const UNCLOSED = join(root, "tests/fixtures/classification/invalid/unclosed-policy");
const ENDPOINT = join(root, "tests/fixtures/classification/invalid/public-endpoint-private-field");
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

// 6b. The representative tenant-crossing fixture (acceptance): a
// tenant-scoped subject whose tenant relation cannot be derived is
// treated as crossing — the flow is a dataflow.tenant-crossing finding
// and the report verdict is denied (exit 3).
{
  const outcome = run(
    ["dataflow", "report", "--attachment", attachment(CROSSING), "--policy", policy(CROSSING)],
    CROSSING,
  );
  if (outcome.code !== 3) fail("tenant-crossing-exit", outcome);
  const deniedOutput = outcome.stdout + outcome.stderr;
  if (!deniedOutput.includes("dataflow.tenant-crossing")) {
    fail("tenant-crossing-rule", deniedOutput);
  }
}

// 6c. The secret-in-sink fixture (hard rule): a credential-kind subject
// is above the unconditional diagnostics/trace ceilings wherever it
// flows — classification.sink-ceiling-exceeded, verdict denied.
{
  const outcome = run(
    ["dataflow", "report", "--attachment", attachment(SINK), "--policy", policy(SINK)],
    SINK,
  );
  if (outcome.code !== 3) fail("secret-in-sink-exit", outcome);
  const sinkOutput = outcome.stdout + outcome.stderr;
  if (!sinkOutput.includes("classification.sink-ceiling-exceeded")) {
    fail("secret-in-sink-rule", sinkOutput);
  }
}

// 6d. The public-endpoint acceptance case (plan §8): the same project
// through an `authenticated` binding stays valid; the identical
// `public` binding exposes the internal task entity —
// dataflow.exposed-private-field, verdict denied (exit 3).
{
  const authenticated = run(
    [
      "dataflow",
      "report",
      "--attachment",
      attachment(ENDPOINT),
      "--policy",
      policy(ENDPOINT),
      "--endpoint",
      "planner.api_focus:authenticated",
      "--json",
    ],
    ENDPOINT,
  );
  if (authenticated.code !== 0) fail("endpoint-authenticated-exit", authenticated);
  if (JSON.parse(authenticated.stdout).report.verdict !== "pass") {
    fail("endpoint-authenticated-verdict", authenticated.stdout);
  }
  const publicRun = run(
    [
      "dataflow",
      "report",
      "--attachment",
      attachment(ENDPOINT),
      "--policy",
      policy(ENDPOINT),
      "--endpoint",
      "planner.api_focus:public",
      "--json",
    ],
    ENDPOINT,
  );
  if (publicRun.code !== 3) fail("endpoint-public-exit", publicRun);
  const publicOutput = publicRun.stdout + publicRun.stderr;
  if (!publicOutput.includes("dataflow.exposed-private-field")) {
    fail("endpoint-public-rule", publicOutput);
  }
  // The denied envelope carries the full report as its payload:
  // status "denied", the report rows, the mirrored diagnostics, and
  // the derived reason codes (a denial never hides the evidence
  // behind it).
  const deniedDoc = JSON.parse(publicRun.stdout);
  if (deniedDoc.status !== "denied") fail("endpoint-public-status", deniedDoc.status);
  const exposed = (deniedDoc.payload?.report?.findings ?? []).filter(
    (finding) => finding.ruleId === "dataflow.exposed-private-field",
  );
  if (exposed.length !== 1) fail("endpoint-public-finding", deniedDoc.payload);
  if (!(deniedDoc.diagnostics ?? []).some(
    (diagnostic) => diagnostic.id === "dataflow.exposed-private-field",
  )) {
    fail("endpoint-public-diagnostic", deniedDoc.diagnostics);
  }
  if (!deniedDoc.reasonCodes.includes("dataflow.exposed-private-field")) {
    fail("endpoint-public-reason-codes", deniedDoc.reasonCodes);
  }

  // 6d''. The inspect projection (review r3, F-2): a dead (expired)
  // grant never lowers the displayed kind — the same shared predicate
  // every grant consumer uses.
  {
    const inspect = run(
      [
        "classification",
        "inspect",
        "--attachment",
        attachment(EXPIRED),
        "--policy",
        policy(EXPIRED),
        "--json",
      ],
      EXPIRED,
    );
    if (inspect.code !== 0) fail("expired-inspect-exit", inspect);
    const subject = JSON.parse(inspect.stdout).subjects.find(
      (row) => row.subject === "notify.user",
    );
    if (!subject || subject.kind !== "personal") {
      fail("expired-inspect-kind", subject);
    }
  }

  // 6e. The validate-pipeline review (F-2 fix): the broken attachments
  // invalidate in the strict profile; the valid fixture stays green.
  // In the default profile structured refusals stay invalid while
  // finding-level issues are recorded without failing — but never
  // silently: the rule id is visible in every profile.
  for (const [fixture, rule, strictCode, defaultCode] of [
    [UNKNOWN, "classification.unknown-subject", 1, 1],
    [SEALED, "classification.invalid-declassification", 1, 1],
    [UNCLOSED, "classification.kind-rule-missing", 1, 0],
    [EXPIRED, "classification.expired-declassification", 1, 0],
    [VALID, null, 3, 0],
  ]) {
    const strictRun = run(["validate", "--strict", "--json"], fixture);
    if (strictRun.code !== strictCode) {
      fail("validate-review-exit", { fixture, strictCode, ...strictRun });
    }
    if (rule !== null && !strictRun.stdout.includes(rule) && !strictRun.stderr.includes(rule)) {
      fail("validate-review-rule", { fixture, rule, ...strictRun });
    }
    const defaultRun = run(["validate", "--json"], fixture);
    if (defaultRun.code !== defaultCode) {
      fail("validate-review-default-exit", { fixture, defaultCode, ...defaultRun });
    }
    if (rule !== null && !defaultRun.stdout.includes(rule) && !defaultRun.stderr.includes(rule)) {
      fail("validate-review-recorded", { fixture, rule });
    }
  }
}

// 6f. The as-of boundary (review r4, F-1): expiry is a lexicographic
// compare, so malformed `--as-of`/`LEKALO_AS_OF` must refuse at the
// CLI (usage, exit 1) on every classification surface — malformed
// input denies, never passes. A bare `YYYY-MM-DD` is normalized to
// midnight UTC; the expired grants (2020) are live before their
// expiry, so 2019 validates clean.
{
  const asOfArgs = [
    "--attachment",
    attachment(EXPIRED),
    "--policy",
    policy(EXPIRED),
  ];
  for (const bad of [
    "!",
    "",
    "2026-13-01",
    "2026-01-01T99:00:00Z",
    "garbage",
    // r5 F-1: exactly 20 bytes with a multi-byte char spanning the
    // time-slice offsets — must refuse as usage, never panic.
    "2026-01-01T€xxxxxZ",
    "2026-01-é",
  ]) {
    for (const command of [
      ["classification", "validate", ...asOfArgs, "--as-of", bad],
      ["dataflow", "report", ...asOfArgs, "--as-of", bad],
      ["classification", "inspect", ...asOfArgs, "--as-of", bad],
    ]) {
      const outcome = run(command, EXPIRED);
      if (outcome.code !== 1) fail("as-of-not-usage", { bad, command, ...outcome });
    }
  }
  const envProbe = (env, command) => {
    const previous = process.env.LEKALO_AS_OF;
    process.env.LEKALO_AS_OF = env;
    try {
      return run(command, EXPIRED);
    } finally {
      if (previous === undefined) delete process.env.LEKALO_AS_OF;
      else process.env.LEKALO_AS_OF = previous;
    }
  };
  const envBad = envProbe("garbage", ["classification", "validate", ...asOfArgs]);
  if (envBad.code !== 1) fail("as-of-env-not-usage", { ...envBad, env: "LEKALO_AS_OF=garbage" });
  const normalized = envProbe("2019-01-01", ["classification", "validate", ...asOfArgs]);
  if (normalized.code !== 0) fail("as-of-bare-date-normalization", normalized);
  const liveFlag = run(
    ["classification", "validate", ...asOfArgs, "--as-of", "2019-01-01T00:00:00Z"],
    EXPIRED,
  );
  if (liveFlag.code !== 0) fail("as-of-wire-shape", liveFlag);
}

// 6g. The suppression regression (review r4, F-5): the fixture now
// classifies `planner.focus_task` `personal`, so its expired
// personal→public grant GENUINELY applies to the exposed subject —
// default as-of must deny with both findings (fold-in AND
// non-suppression), while the same surface before expiry must pass
// (a live grant suppresses). Proves both halves the r2/r3 majors
// rode on.
{
  const endpointArgs = [
    "dataflow",
    "report",
    "--attachment",
    attachment(EXPIRED),
    "--policy",
    policy(EXPIRED),
    "--endpoint",
    "planner.api_focus:public",
    "--json",
  ];
  const expiredRun = run(endpointArgs, EXPIRED);
  if (expiredRun.code !== 3) fail("suppression-expired-exit", expiredRun);
  const expiredDoc = JSON.parse(expiredRun.stdout);
  const findings = (expiredDoc.payload?.report?.findings ?? []).map(
    (finding) => `${finding.ruleId}:${finding.subject}`,
  );
  if (!findings.includes("dataflow.exposed-private-field:planner.focus_task")) {
    fail("suppression-expired-exposure", findings);
  }
  if (
    !findings.includes(
      "classification.expired-declassification:planner.focus_task",
    )
  ) {
    fail("suppression-expired-fold", findings);
  }
  const liveRun = run([...endpointArgs, "--as-of", "2019-01-01T00:00:00Z"], EXPIRED);
  if (liveRun.code !== 0) fail("suppression-live-exit", liveRun);
  if (JSON.parse(liveRun.stdout).report.verdict !== "pass") {
    fail("suppression-live-verdict", liveRun.stdout);
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
  for (const fixture of [VALID, DECLASSIFIED, UNKNOWN, SEALED, UNCLOSED, CROSSING, SINK, ENDPOINT, EXPIRED]) {
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
    invalid: 7,
  },
  sentinelScanned: true,
}, null, 2)}\n`);
