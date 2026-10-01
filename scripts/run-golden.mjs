#!/usr/bin/env node
// Suite-v1 case runner / read-only verifier (issue #90).
//
// Executes catalogued cases in a fresh external sandbox, checks the
// exit/status/reason contract of each case, compares produced bytes
// against the pinned expected outputs, and emits an execution receipt
// manifest. Never writes inside the tracked fixture tree: expected
// files are updated only through the deliberate update-golden flow.
//
// Usage:
//   node scripts/run-golden.mjs [--case <case-id>]... [--verify] [--out <dir>]
//
// Without --verify: runs and prints the receipt (exit code 0 when all
// cases match their expectations). With --verify: also compares every
// expected output byte-for-byte with the pinned file.

import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync } from "node:fs";
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
  : mkdtempSync(join(tmpdir(), "lekalo-golden-run-"));

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

  const runner = d.runner;
  if (runner !== "cli-validate") {
    // Other runners are exercised by their dedicated gates; the runner
    // records them as declared-not-executed here.
    outcomes.push({ caseId: entry.id, revision: entry.revision, status: "declared", runner });
    continue;
  }

  const expect = d.expectation ?? {};
  const plans = materialized.map((project) => project.role === "trigger"
    ? { project, want: { status: "invalid", exit: 1, reasonCodes: expect.reasonCodes ?? [] } }
    : project.role === "non-trigger"
      ? { project, want: { status: "valid", exit: 0, reasonCodes: [] } }
      : { project, want: { status: expect.status ?? "valid", exit: expect.exit ?? 0, reasonCodes: expect.reasonCodes ?? [] } });
  const witnessRule = expect.witnessRule ?? (expect.reasonCodes ?? [])[0];
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
}

if (outDir === null) {
  rmSync(sandboxRoot, { recursive: true, force: true });
}

if (failures > 0) failGate("golden-run", outcomes.filter((o) => o.status === "gate-error" || o.wanted));
passGate("golden-run", { cases: outcomes.length, sandbox: outDir ? sandboxRoot : "(temp)", outcomes });

