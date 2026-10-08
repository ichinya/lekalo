#!/usr/bin/env node
// Issue #52 outbox gate: the deterministic delivery state machine is
// executed through the Rust battery (lease fencing, bounded retry,
// dead-letter, cancel-on-revoke, crash recovery) and independently
// re-asserted here: the declared outbox vocabulary is the closed
// nine-state machine, retries keep their idempotency keys, recipient
// accounting stays partial while anything is unresolved, and the
// network call is declared outside every local transaction.

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const gate = "task-links-outbox";
const failures = [];
const fail = (reason, detail) => failures.push({ reason, detail });
const read = (relative) => JSON.parse(readFileSync(join(root, relative), "utf8"));
const resolvedBinary = [
  join(root, "target/debug/lekalo"),
  join(root, "target/debug/lekalo.exe"),
].find((candidate) => existsSync(candidate));

// 1. The attachment's delivery rules carry the closed nine-state machine
//    and the hard boundaries.
const EXPECTED_STATES = [
  "pending",
  "leased",
  "accepted_pending_ack",
  "delivered",
  "retry_wait",
  "conflicted",
  "cancelled",
  "dead_letter",
  "unknown_delivery",
];
{
  const attachment = read("tests/fixtures/task-external-links/contracts/integration.json");
  const rules = attachment.deliveryRules;
  if (JSON.stringify(rules.outboxStates) !== JSON.stringify(EXPECTED_STATES)) {
    fail("outbox-states", JSON.stringify(rules.outboxStates));
  }
  if (rules.networkInsideTransaction !== false) {
    fail("network-inside-transaction", "the network call never joins a local transaction");
  }
  if (rules.externalDeleteImplicit !== false) {
    fail("implicit-external-delete", "no implicit external delete exists");
  }
  if (rules.maxAttempts < 1) fail("max-attempts", String(rules.maxAttempts));
  if (rules.leaseMs < 1) fail("lease-ms", String(rules.leaseMs));
  if (!["report-unknown-reconcile", "receipt-reconciliation"].includes(rules.uncertaintyPolicy)) {
    fail("uncertainty-policy", rules.uncertaintyPolicy);
  }
  // Every operation binding declares a bounded retry guarantee.
  for (const binding of attachment.operationBindings) {
    if (binding.retryGuarantee === "none-declared") {
      fail("binding-retry", binding.operation);
    }
    if (binding.timeoutMs < 1 || binding.timeoutMs > 600000) {
      fail("binding-timeout", binding.operation);
    }
  }
}

// 2. The Rust engine battery: outbox/lease/retry/crash legs.
const names = [
  "scenario_partial_delivery_keeps_receipts_and_partial_status",
  "scenario_stale_conflict_and_bounded_retry_dead_letter",
  "bounded_retries_exhaust_into_dead_letter_keeping_the_key",
  "scenario_detach_history_cancels_pending_and_never_deletes",
  "scenario_retry_echo_crash_preserves_key_and_uncertainty",
];
const rust = spawnSync(
  "cargo",
  ["test", "--locked", "-p", "lekalo-core", "--test", "task_links", "--", ...names],
  { cwd: root, encoding: "utf8", timeout: 600000 },
);
if (rust.error) {
  fail("cargo-unavailable", String(rust.error));
} else if (rust.status !== 0) {
  fail("engine-outbox-failed", (rust.stdout + rust.stderr).slice(0, 800));
} else {
  for (const name of names) {
    if (!rust.stdout.includes(`test ${name} ... ok`)) {
      fail("engine-leg-missing", name);
    }
  }
}

// 2b. The declared effect graph carries the atomic intent group: the
//     canonical local write and the durable outbox commit are declared
//     edges of the SAME command, and the dispatch/ack commands carry
//     the durable outbox write (external writes and partial failure
//     are declared in the extended-effects family, validated against
//     the same compilation by the core battery).
{
  const intentGroup = (operation, kinds) => {
    const run = spawnSync(
      resolvedBinary,
      ["--no-cache", "--json", "effects", "show", operation, "--project", "tests/fixtures/task-external-links/project"],
      { cwd: root, encoding: "utf8", timeout: 120000 },
    );
    if (run.error || run.status !== 0) {
      fail("effects-show", operation + ": " + String(run.error ?? run.stderr).slice(0, 200));
      return;
    }
    const envelope = JSON.parse(run.stdout);
    const edges = envelope.effects.edges.map((edge) => edge.kind + ":" + edge.resource.id);
    for (const expected of kinds) {
      if (!edges.includes(expected)) {
        fail("intent-group-edge", operation + " missing " + expected + " in " + JSON.stringify(edges));
      }
    }
    if (envelope.effects.complete !== true) {
      fail("intent-group-complete", operation);
    }
  };
  intentGroup("tasklinks.request_action", ["create:tasklinks.outbox_item", "update:planner.task"]);
  intentGroup("tasklinks.dispatch_outbox", ["create:tasklinks.outbox_item"]);
  intentGroup("tasklinks.ack_delivery", ["create:tasklinks.outbox_item"]);
  // The extended-effects declaration joins the same compilation: the
  // external write, the outbox worker job, the durable receipt event,
  // and the partial-failure case are declared and admitted.
  const rust = spawnSync(
    "cargo",
    ["test", "--locked", "-p", "lekalo-core", "--test", "task_links", "extended_effects_declare"],
    { cwd: root, encoding: "utf8", timeout: 600000 },
  );
  if (rust.error || rust.status !== 0) {
    fail("extended-effects", String(rust.error ?? rust.stdout + rust.stderr).slice(0, 300));
  }
}

// 3. Independent state-machine invariants over the scenario docs.
{
  const partial = read("tests/fixtures/task-external-links/scenarios/partial_delivery.json");
  const retryEligible = partial.expects.find((row) => row.kind === "retry-eligible");
  if (!retryEligible || retryEligible.only !== true) {
    fail("partial-accounting", "only the failed recipient is retry-eligible");
  }
  const echo = read("tests/fixtures/task-external-links/scenarios/retry_echo_crash.json");
  if (!echo.expects.some((row) => row.kind === "retry-keeps-key" && row.value === true)) {
    fail("retry-key", "retries keep the same key");
  }
  if (!echo.expects.some((row) => row.kind === "unknown-delivery-or-reconcile")) {
    fail("uncertainty", "acceptance-before-ack must reconcile or report unknown");
  }
  const detach = read("tests/fixtures/task-external-links/scenarios/detach_history.json");
  if (!detach.expects.some((row) => row.kind === "pending-cancelled" && row.value === true)) {
    fail("cancel-on-detach", "pending dispatch is cancelled");
  }
}

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate, failures }, null, 2)}\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({ ok: true, gate, legs: names.length })}\n`);
