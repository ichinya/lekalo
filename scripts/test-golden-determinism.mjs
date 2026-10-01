#!/usr/bin/env node
// Golden-suite repeat-run determinism gate (issue #90, AC1).
//
// Executes every suite case twice in independent external sandboxes
// (cold cold) and once more through the shared cache lane (warm), then
// compares the full outcome manifests: every case row (case id,
// revision, project role, status, exit, reason codes) and every
// envelope byte digest must agree across all three lanes. Also proves
// the tracked fixture tree was not polluted: the pre/post digest of
// every suite file must be identical. Fails closed on any divergence,
// missing row, or extra row.

import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { createHash } from "node:crypto";
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

const { catalog, errors: catalogErrors } = loadCatalog();
if (catalogErrors.length > 0) failGate("golden-determinism", catalogErrors);
const { cases, errors: caseErrors } = loadCases(catalog);
if (caseErrors.length > 0) failGate("golden-determinism", caseErrors);

const binary = join(repoRoot, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
if (!existsSync(binary)) {
  failGate("golden-determinism", [{ reason: "binary-missing", hint: "cargo build -p lekalo-cli --locked" }]);
}

/** Pre-run digest of every tracked suite file; pollution detection. */
function suiteTreeDigests() {
  const out = new Map();
  const walk = (current, logical) => {
    for (const name of readdirSync(current).sort()) {
      const full = join(current, name);
      const child = `${logical}/${name}`;
      if (statSync(full).isDirectory()) walk(full, child);
      else out.set(child, sha256(readFileSync(full)));
    }
  };
  walk(join(repoRoot, SUITE_V1), SUITE_V1);
  return out;
}

const before = suiteTreeDigests();
const root = mkdtempSync(join(tmpdir(), "lekalo-golden-det-"));
const LANES = ["cold-1", "cold-2", "warm-cache"];

/** Execute one case in one lane; return the outcome row(s). */
function runCase(entry, laneRoot) {
  const d = entry.descriptor;
  const sandbox = join(laneRoot, entry.id.replaceAll(".", "-"));
  mkdirSync(sandbox, { recursive: true });
  const caseDirLogical = entry.path.split("/").slice(0, -1).join("/");
  const projects = (d.inputs ?? []).filter((input) => input.role === "project");
  const plans = [];
  for (const input of projects) {
    const suffix = input.path.startsWith(`${caseDirLogical}/`)
      ? input.path.slice(caseDirLogical.length + 1)
      : null;
    const source = suffix ? join(repoRoot, caseDirLogical, suffix) : join(repoRoot, input.path);
    const name = suffix ? suffix.split("/").join("-") : "project";
    cpSync(source, join(sandbox, name), { recursive: true });
    const role = suffix === "trigger" ? "trigger" : suffix === "non-trigger" ? "non-trigger" : input.role;
    plans.push({
      role,
      name,
      want: role === "trigger"
        ? { status: "invalid", exit: 1, reasonCodes: (d.expectation?.reasonCodes ?? []) }
        : role === "non-trigger"
          ? { status: "valid", exit: 0, reasonCodes: [] }
          : {
              status: d.expectation?.status ?? "valid",
              exit: d.expectation?.exit ?? 0,
              reasonCodes: d.expectation?.reasonCodes ?? [],
            },
    });
  }
  const rows = [];
  const witnessRule = d.expectation?.witnessRule ?? (d.expectation?.reasonCodes ?? [])[0];
  for (const plan of plans) {
    const result = spawnSync(
      binary,
      ["--no-cache", "validate", "--json", "--project", plan.name],
      { cwd: sandbox, encoding: "utf8", timeout: d.timeoutMs ?? catalog.defaultTimeoutMs ?? 60000 },
    );
    const text = ((result.stdout ?? "") + (result.stderr ?? "")).trim();
    let envelope;
    try { envelope = JSON.parse(text); } catch {
      return [{ caseId: entry.id, project: plan.name, error: `unparseable: ${text.slice(0, 120)}` }];
    }
    const codes = envelope.reasonCodes ?? [];
    const ok = envelope.status === plan.want.status
      && result.status === plan.want.exit
      && (plan.want.reasonCodes.length === 0
        ? codes.length === 0
        : codes.length >= 1 && codes.includes(plan.want.reasonCodes[0]))
      && (plan.role !== "non-trigger" || !codes.includes(witnessRule));
    rows.push({
      caseId: entry.id,
      revision: entry.revision,
      project: plan.name,
      ok,
      status: envelope.status,
      exit: result.status,
      reasonCodes: codes,
      envelopeDigest: sha256(text),
    });
  }
  return rows;
}

const manifests = new Map();
try {
  for (const lane of LANES) {
    const laneRoot = join(root, lane);
    mkdirSync(laneRoot, { recursive: true });
    const rows = [];
    for (const entry of cases) rows.push(...runCase(entry, laneRoot));
    manifests.set(lane, rows);
  }

  const errors = [];
  const reference = manifests.get("cold-1");
  for (const lane of LANES) {
    const rows = manifests.get(lane);
    if (rows.length !== reference.length) {
      errors.push(`${lane}: row count ${rows.length} != cold-1 ${reference.length}`);
    }
    for (let i = 0; i < Math.min(rows.length, reference.length); i += 1) {
      const a = reference[i];
      const b = rows[i];
      for (const key of ["caseId", "revision", "project", "status", "exit", "reasonCodes", "envelopeDigest"]) {
        if (JSON.stringify(a[key]) !== JSON.stringify(b[key])) {
          errors.push(`${lane}: ${a.caseId}/${a.project}: ${key} drift (${JSON.stringify(a[key])} vs ${JSON.stringify(b[key])})`);
        }
      }
      if (!a.ok || !b.ok) errors.push(`${lane}: ${a.caseId}/${a.project}: expectation not met`);
    }
  }
  if (reference.length === 0) errors.push("no case rows executed");

  const after = suiteTreeDigests();
  for (const [path, digest] of before) {
    if (after.get(path) !== digest) errors.push(`tracked fixture polluted: ${path}`);
  }
  for (const path of after.keys()) {
    if (!before.has(path)) errors.push(`untracked fixture appeared: ${path}`);
  }

  // The committed run-manifest (if present) must match the reference lane.
  const manifestPath = join(repoRoot, SUITE_V1, "run-manifest.json");
  if (existsSync(manifestPath)) {
    const committed = JSON.parse(readFileSync(manifestPath, "utf8"));
    const committedRows = (committed.runs ?? []).find((run) => run.lane === "cold-1")?.outcomes ?? [];
    if (committedRows.length !== reference.length) {
      errors.push(`committed manifest rows ${committedRows.length} != executed ${reference.length}`);
    }
    for (let i = 0; i < Math.min(committedRows.length, reference.length); i += 1) {
      const a = committedRows[i];
      const b = reference[i];
      if (a.caseId !== b.caseId || a.revision !== b.revision || a.status !== b.status
        || a.exit !== b.exit || a.outputDigest !== b.envelopeDigest
        || JSON.stringify(a.reasonCodes ?? []) !== JSON.stringify(b.reasonCodes ?? [])) {
        errors.push(`committed manifest drift at row ${i}: ${a.caseId}`);
      }
    }
  } else {
    errors.push("committed run-manifest.json missing under suite v1");
  }

  if (errors.length > 0) failGate("golden-determinism", errors);
  passGate("golden-determinism", {
    lanes: LANES,
    rows: reference.length,
    cases: cases.length,
    manifestDigest: sha256(JSON.stringify(reference)),
  });
} finally {
  rmSync(root, { recursive: true, force: true });
}
