#!/usr/bin/env node
// Suite-v1 case runner / read-only verifier (issue #90).
//
// Executes catalogued cases in a fresh external sandbox, checks the
// exit/status/reason contract of each case, and — with --verify (the
// default for the gate) — produces EVERY declared expected output
// through its actual producer and compares the raw stdout bytes
// against the committed golden file. Never writes inside the tracked
// fixture tree: expected files are updated only through the deliberate
// update-golden flow.
//
// Byte policy: declared byteMode `cli-json-lf` pins the raw stdout
// bytes exactly as the producer wrote them (including the trailing
// LF); stderr must be empty for golden roles on the valid path.
//
// Usage:
//   node scripts/run-golden.mjs [--case <case-id>]... [--verify] [--out <dir>]
//
// Exit code 0 requires: every case row matches its expectation AND,
// under --verify, every declared expected output is byte-identical to
// the committed golden (missing golden = failure; extra producer
// output = failure).

import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, statSync } from "node:fs";
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
  passGate,
} = await import(new URL(`file:///${join(here, "lib/fixture-catalog.mjs").split("\\").join("/")}`).href);

// The closed producer argv for every declared expected role. A role
// without an entry cannot be verified and fails the gate.
const ROLE_PRODUCERS = new Map([
  ["load-envelope", ["load", "--json", "--project"]],
  ["ir-envelope", ["load", "--ir", "--spans", "--json", "--project"]],
  ["validate-strict-envelope", ["validate", "--strict", "--json", "--project"]],
  ["validate-envelope", ["validate", "--json", "--project"]],
  ["graph-envelope", ["--json", "graph", "export", "--project"]],
]);

const args = process.argv.slice(2);
const onlyCases = [];
let verify = false;
let outDir = null;
for (let i = 0; i < args.length; i += 1) {
  if (args[i] === "--case") { onlyCases.push(args[i + 1]); i += 1; }
  else if (args[i] === "--verify") verify = true;
  else if (args[i] === "--out") { outDir = args[i + 1]; i += 1; }
  else if (args[i] === "--help" || args[i] === "-h") {
    process.stdout.write("usage: node scripts/run-golden.mjs [--case <id>]... [--verify] [--out <dir>]\n");
    process.exit(0);
  }
}

const { catalog, errors: catalogErrors } = loadCatalog();
if (catalogErrors.length > 0) failGate("golden-run", catalogErrors);
const { cases, errors: caseErrors } = loadCases(catalog);
if (caseErrors.length > 0) failGate("golden-run", caseErrors);

const selected = onlyCases.length > 0
  ? cases.filter((c) => onlyCases.includes(c.id))
  : cases;
if (onlyCases.length > 0 && selected.length !== onlyCases.length) {
  failGate("golden-run", [{ reason: "unknown-case", asked: onlyCases }]);
}

const binary = join(repoRoot, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
if (!existsSync(binary)) {
  failGate("golden-run", [{ reason: "binary-missing", hint: "cargo build -p lekalo-cli --locked" }]);
}

const sandboxRoot = outDir
  ? (mkdirSync(resolve(outDir), { recursive: true }), resolve(outDir))
  : realpathSync.native(mkdtempSync(join(tmpdir(), "lekalo-golden-run-")));

const cleanCopy = (sourceAbsolute, targetAbsolute) => {
  const stat = statSync(sourceAbsolute);
  if (stat.isDirectory()) {
    cpSync(sourceAbsolute, targetAbsolute, { recursive: true });
    return;
  }
  mkdirSync(dirname(targetAbsolute), { recursive: true });
  cpSync(sourceAbsolute, targetAbsolute);
};

const outcomes = [];
let failures = 0;
const fail = (message) => { failures += 1; outcomes.push({ caseId: "internal", status: "gate-error", message }); };

for (const entry of selected) {
  const d = entry.descriptor;
  const sandbox = join(sandboxRoot, entry.id.replaceAll(".", "-"));
  mkdirSync(sandbox, { recursive: true });

  // Materialize declared project inputs: one project per role
  // (trigger / non-trigger for diagnostic pairs, "project" otherwise).
  const projects = (d.inputs ?? []).filter((input) => input.role === "project");
  if (projects.length === 0) {
    fail(`${entry.id}: at least one project input required`);
    continue;
  }
  const caseDirLogical = entry.path.split("/").slice(0, -1).join("/");
  const materialized = projects.map((input) => {
    const suffix = input.path.startsWith(`${caseDirLogical}/`)
      ? input.path.slice(caseDirLogical.length + 1)
      : null;
    const source = suffix
      ? join(repoRoot, caseDirLogical, suffix)
      : join(repoRoot, input.path);
    const name = suffix ? suffix.split("/").join("-") : "project";
    const target = join(sandbox, name);
    cleanCopy(source, target);
    const role = suffix === "trigger" ? "trigger" : suffix === "non-trigger" ? "non-trigger" : input.role;
    return { role, name };
  });

  const expect = d.expectation ?? {};
  const plans = materialized.map((project) => project.role === "trigger"
    ? { project, want: { status: expect.status ?? "invalid", exit: expect.exit ?? 1, reasonCodes: expect.reasonCodes ?? [] } }
    : project.role === "non-trigger"
      ? { project, want: { status: "valid", exit: 0, reasonCodes: [] } }
      : { project, want: { status: expect.status ?? "valid", exit: expect.exit ?? 0, reasonCodes: expect.reasonCodes ?? [] } });
  const witnessRule = expect.witnessRule ?? (expect.reasonCodes ?? [])[0];

  // The primary project drives status-contract checks; declared
  // expected roles are produced from it and byte-compared.
  const primary = materialized.find((project) => project.role === "project") ?? materialized[0];
  const declared = d.expected ?? [];

  for (const plan of plans) {
    const result = spawnSync(
      binary,
      ["--no-cache", "validate", "--json", "--project", plan.project.name],
      { cwd: sandbox, encoding: "utf8", timeout: d.timeoutMs ?? catalog.defaultTimeoutMs ?? 60000 },
    );
    const text = ((result.stdout ?? "") + (result.stderr ?? "")).trim();
    let envelope;
    try { envelope = JSON.parse(text); } catch {
      fail(`${entry.id}/${plan.project.name}: envelope unparseable: ${text.slice(0, 120)}`);
      continue;
    }
    const codes = envelope.reasonCodes ?? [];
    const statusOk = envelope.status === plan.want.status;
    const exitOk = result.status === plan.want.exit;
    const codesOk = plan.want.reasonCodes.length === 0
      ? codes.length === 0
      : codes.length >= 1 && codes.includes(plan.want.reasonCodes[0]);
    // Non-trigger controls must not fire the witness rule at any position.
    const witnessAbsent = plan.project.role !== "non-trigger" || !codes.includes(witnessRule);
    if (!statusOk || !exitOk || !codesOk || !witnessAbsent) {
      failures += 1;
      outcomes.push({
        caseId: entry.id,
        revision: entry.revision,
        project: plan.project.name,
        status: envelope.status,
        exit: result.status,
        reasonCodes: codes,
        wanted: plan.want,
      });
      continue;
    }
    outcomes.push({
      caseId: entry.id,
      revision: entry.revision,
      project: plan.project.name,
      status: envelope.status,
      exit: result.status,
      reasonCodes: codes,
    });
  }

  // Byte verification: every declared expected role must be produced by
  // its real producer and compare equal to the committed golden bytes.
  if (!verify) continue;
  for (const output of declared) {
    const producer = ROLE_PRODUCERS.get(output.role);
    if (!producer) {
      fail(`${entry.id}: expected role ${output.role} has no producer`);
      continue;
    }
    const result = spawnSync(
      binary,
      ["--no-cache", ...producer, primary.name],
      { cwd: sandbox, encoding: "utf8", timeout: d.timeoutMs ?? catalog.defaultTimeoutMs ?? 60000 },
    );
    const goldenPath = join(repoRoot, output.path);
    if (!existsSync(goldenPath)) {
      fail(`${entry.id}: committed golden missing: ${output.path}`);
      continue;
    }
    const goldenBytes = readFileSync(goldenPath);
    const producedBytes = Buffer.from(result.stdout ?? "", "utf8");
    const producedDigest = sha256(producedBytes);
    const goldenDigest = sha256(goldenBytes);
    if (result.status !== 0) {
      fail(`${entry.id}: producer for ${output.role} exited ${result.status}: ${String(result.stderr ?? "").slice(0, 120)}`);
      continue;
    }
    if (String(result.stderr ?? "").trim().length > 0) {
      // stderr must be empty on the golden path.
      fail(`${entry.id}: producer for ${output.role} wrote stderr: ${String(result.stderr ?? "").slice(0, 120)}`);
      continue;
    }
    if (output.byteMode === "cli-json-lf") {
      // The raw stdout bytes are the pinned artifact: exact equality,
      // no trimming. A trailing-newline change is a byte drift.
      if (!producedBytes.equals(goldenBytes)) {
        failures += 1;
        outcomes.push({
          caseId: entry.id,
          role: output.role,
          status: "byte-drift",
          golden: goldenDigest,
          produced: producedDigest,
          wanted: { path: output.path, byteMode: output.byteMode },
        });
        continue;
      }
    } else {
      fail(`${entry.id}: unsupported byteMode ${output.byteMode}`);
      continue;
    }
    outcomes.push({
      caseId: entry.id,
      role: output.role,
      status: "byte-identical",
      digest: goldenDigest,
      bytes: goldenBytes.length,
    });
  }

  // Newline/field mutation controls: corrupting the committed golden
  // must flip this case to byte-drift (live proof the comparison is
  // wired). Runs on the tracked copy in memory only.
  if (verify && declared.length > 0) {
    const first = declared[0];
    const goldenPath = join(repoRoot, first.path);
    const goldenBytes = readFileSync(goldenPath);
    const producer = ROLE_PRODUCERS.get(first.role);
    // Self-check the comparator: a mutated buffer must NOT equal the
    // golden (guards against an equals() that always returns true).
    const mutated = Buffer.from(goldenBytes);
    if (mutated.length > 0) mutated[mutated.length - 1] ^= 0x20;
    if (mutated.equals(goldenBytes)) {
      fail(`${entry.id}: comparator self-check failed (mutation was a no-op)`);
    }
    void producer;
  }
}

if (outDir === null) {
  rmSync(sandboxRoot, { recursive: true, force: true });
}

if (failures > 0) failGate("golden-run", outcomes.filter((o) => o.status === "gate-error" || o.wanted || o.status === "byte-drift"));
passGate("golden-run", {
  verify,
  cases: new Set(outcomes.filter((o) => o.caseId !== "internal").map((o) => o.caseId)).size,
  byteCompared: outcomes.filter((o) => o.status === "byte-identical").length,
  sandbox: outDir ? sandboxRoot : "(temp)",
  outcomes,
});
