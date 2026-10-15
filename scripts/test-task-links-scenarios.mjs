#!/usr/bin/env node
// Issue #52 scenario gate: all ten stable scenario ids are declared,
// executed through the deterministic Rust engine battery, and
// independently re-asserted here (field/effect/idempotency invariants
// computed from the committed docs, never from the implementation's
// own summaries). No hidden network: the engine is pure and this gate
// spawns only cargo and node.

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const gate = "task-links-scenarios";
const failures = [];
const fail = (reason, detail) => failures.push({ reason, detail });
const read = (relative) => JSON.parse(readFileSync(join(root, relative), "utf8"));

const SCENARIOS = [
  "two_providers",
  "duplicate_import",
  "identity_collision",
  "attributed_inbound",
  "decision_matrix",
  "partial_delivery",
  "stale_conflict",
  "detach_history",
  "retry_echo_crash",
  "secret_canary",
];

// 1. Every scenario doc exists with the verbatim issue fragment and
//    mock origin.
for (const slug of SCENARIOS) {
  const path = `tests/fixtures/task-external-links/scenarios/${slug}.json`;
  if (!existsSync(join(root, path))) {
    fail("doc-missing", slug);
    continue;
  }
  const document = read(path);
  if (document.scenarioId !== `tasklinks.scenario.${slug}`) fail("id-drift", slug);
  if (typeof document.issueVerbatim !== "string" || document.issueVerbatim.length < 5) {
    fail("verbatim-missing", slug);
  }
  if (!Array.isArray(document.expects) || document.expects.length === 0) {
    fail("expects-missing", slug);
  }
}

// 2. The Rust engine battery executes every scenario family; cargo
//    missing is a failure, not a skip.
const rust = spawnSync(
  "cargo",
  ["test", "--locked", "-p", "lekalo-core", "--test", "task_links", "scenario_"],
  { cwd: root, encoding: "utf8", timeout: 600000 },
);
if (rust.error) {
  fail("cargo-unavailable", String(rust.error));
} else if (rust.status !== 0) {
  fail("engine-battery-failed", (rust.stdout + rust.stderr).slice(0, 800));
} else {
  const ran = (rust.stdout.match(/test scenario_\w+ \.\.\. ok/g) ?? []).length;
  if (ran < 10) fail("engine-scenario-count", String(ran));
}

// 3. Independent invariant assertions, computed here from the docs.
{
  const matrix = read("tests/fixtures/task-external-links/scenarios/decision_matrix.json");
  for (const row of matrix.matrix) {
    const admitted = row.outcome === "allowed";
    if (admitted !== row.intent) fail("matrix-intent", JSON.stringify(row));
    if ((row.authorization === "denied") === admitted && row.authorization === "denied") {
      fail("matrix-denied-admitted", JSON.stringify(row));
    }
    if (row.capability === "unknown" && admitted) fail("matrix-unknown-admitted", row);
    if (row.capability === "unsupported" && admitted) fail("matrix-unsupported-admitted", row);
  }
  const collision = read("tests/fixtures/task-external-links/scenarios/identity_collision.json");
  const refused = collision.steps.find((step) => step.expectError);
  if (!refused || refused.expectError !== "tasklinks.error.identity_already_bound") {
    fail("collision-error-missing", "identity_already_bound");
  }
  const duplicate = read("tests/fixtures/task-external-links/scenarios/duplicate_import.json");
  if (duplicate.expects.some((row) => row.kind === "link-count" && row.value !== 1)) {
    fail("duplicate-link-count", "must stay one link");
  }
  const partial = read("tests/fixtures/task-external-links/scenarios/partial_delivery.json");
  if (!partial.expects.some((row) => row.kind === "delivery-status" && row.value === "partial")) {
    fail("partial-status", "must stay partial");
  }
  const detach = read("tests/fixtures/task-external-links/scenarios/detach_history.json");
  for (const kind of ["history-retained", "external-object-retained", "no-external-delete"]) {
    if (!detach.expects.some((row) => row.kind === kind && row.value === true)) {
      fail("detach-expect", kind);
    }
  }
  const echo = read("tests/fixtures/task-external-links/scenarios/retry_echo_crash.json");
  for (const kind of ["retry-keeps-key", "no-new-intent-on-echo", "unknown-delivery-or-reconcile"]) {
    if (!echo.expects.some((row) => row.kind === kind && row.value === true)) {
      fail("echo-expect", kind);
    }
  }
  const canary = read("tests/fixtures/task-external-links/scenarios/secret_canary.json");
  const surfaces = canary.expects.find((row) => row.kind === "canary-absent")?.surfaces ?? [];
  for (const surface of ["payload", "evidence", "error", "stdout", "stderr", "logs", "exports"]) {
    if (!surfaces.includes(surface)) fail("canary-surface", surface);
  }
  // Attribution keys keep provider/installation/event/revision distinct.
  const inbound = read("tests/fixtures/task-external-links/scenarios/attributed_inbound.json");
  const keys = inbound.expects.find((row) => row.kind === "attribution-keys")?.value ?? [];
  for (const key of ["provider", "installation", "event-identity", "revision"]) {
    if (!keys.includes(key)) fail("attribution-key", key);
  }
}

// 4. The committed evidence instance is mock-origin and matches the
//    ten-scenario catalog pin (one evidence doc per run receipt).
{
  const evidence = read("tests/fixtures/task-external-links/evidence/mock-run.json");
  if (evidence.origin !== "mock") fail("evidence-origin", evidence.origin);
  if (evidence.clock.policy !== "injected") fail("evidence-clock", evidence.clock.policy);
  const stale = read("tests/fixtures/task-external-links/invalid/evidence-stale.json");
  if (stale.attachmentRevision === evidence.attachmentRevision) {
    fail("stale-evidence-not-stale", "the negative control must differ");
  }
}

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate, failures }, null, 2)}\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({ ok: true, gate, scenarios: SCENARIOS.length })}\n`);
