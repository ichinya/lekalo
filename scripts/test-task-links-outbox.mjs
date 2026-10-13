#!/usr/bin/env node
// Issue #52 outbox gate: the deterministic delivery state machine is
// executed through the Rust battery (lease fencing, bounded retry,
// dead-letter, cancel-on-revoke, crash recovery) and independently
// re-asserted here: the declared outbox vocabulary is the closed
// nine-state machine, retries keep their idempotency keys, recipient
// accounting stays partial while anything is unresolved, and the
// network call is declared outside every local transaction.

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
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

// 2a. Execution-owner regressions, distinct from declaration-only file joins.
{
  const executed = spawnSync("cargo", ["test", "--locked", "-p", "lekalo-core", "--test", "task_links_execution"], { cwd: root, encoding: "utf8", timeout: 600000 });
  if (executed.error || executed.status !== 0) fail("execution-owner-regressions", String(executed.error ?? (executed.stdout + executed.stderr).slice(-1200)));
  const show = operation => spawnSync(resolvedBinary, ["--no-cache", "--json", "effects", "show", operation,
    "--project", "tests/fixtures/task-external-links/project",
    "--task-links-attachment", "tests/fixtures/task-external-links/contracts/integration.json",
    "--transactions", "tests/fixtures/task-external-links/contracts/transaction-concurrency.json",
    "--extended-effects", "tests/fixtures/task-external-links/contracts/extended-effects.json"], { cwd: root, encoding: "utf8", timeout: 120000 });
  const local = show("tasklinks.request_action"), partial = show("tasklinks.dispatch_outbox");
  for (const run of [local, partial]) if (run.error || run.status !== 0) fail("executed-effect-owner", String(run.error ?? run.stderr));
  if (local.status === 0 && partial.status === 0) {
    const localEffects = JSON.parse(local.stdout).effects, dispatch = JSON.parse(partial.stdout).effects;
    const tx = localEffects.transactions[0];
    if (tx?.basis !== "owner-admitted-execution" || tx.members.length !== 4) fail("executed-local-atomic-members", tx);
    const canonical = localEffects.edges.filter(edge => edge.confidence === "canonical");
    if (canonical.length !== 4 || canonical.some(edge => edge.transactionGroup !== tx.groupId)) fail("canonical-transaction-owner", canonical);
    if (dispatch.edges.filter(edge => edge.kind === "external-call").some(edge => edge.confidence !== "verified" || edge.transactionGroup !== null)) fail("executed-network-boundary", dispatch);
    const record = dispatch.partialFailures[0];
    if (record?.basis !== "owner-admitted-execution" || JSON.stringify(record.retryEligible) !== JSON.stringify(["tasklinks.provider/mock-secondary"]) || !record.recipients.find(row => row.result === "delivered")?.receiptDigest) fail("executed-partial-trace", record);
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

  // The public effect graph carries the extended surface: the committed
  // detected-evidence envelope joins THIS compilation (its source
  // revision is the live IR digest, recomputed by the owning regen
  // script) and the external provider write, the durable outbox worker
  // job, the receipt event, and the explicit local transaction groups
  // become visible public edges. The network call stays outside the
  // transaction: the group ids are declared facts, not simulations.
  const detectedEnvelope = "tests/fixtures/task-external-links/contracts/detected-effects.json";
  const detected = (operation) => {
    const run = spawnSync(
      resolvedBinary,
      ["--no-cache", "--json", "effects", "show", operation,
       "--project", "tests/fixtures/task-external-links/project",
       "--evidence", detectedEnvelope],
      { cwd: root, encoding: "utf8", timeout: 120000 },
    );
    if (run.error || run.status !== 0) {
      fail("detected-show", operation + ": " + String(run.error ?? run.stderr).slice(0, 200));
      return null;
    }
    return JSON.parse(run.stdout).effects;
  };
  {
    const effects = detected("tasklinks.dispatch_outbox");
    if (effects) {
      const byKind = (kind) => effects.edges.filter((edge) => edge.kind === kind);
      if (!byKind("external-call").some((edge) => edge.resource.id === "tasklinks.provider/github-issues")) {
        fail("detected-external-write", "dispatch_outbox external provider write missing");
      }
      if (!byKind("enqueue-job").some((edge) => edge.resource.id === "tasklinks.queue/outbox")) {
        fail("detected-worker-job", "dispatch_outbox durable outbox worker job missing");
      }
      // Network calls sit OUTSIDE local atomic groups: no external
      // call may carry a transaction group (the contract excludes
      // them), and the local dispatch group is a TYPED declaration
      // over the durable enqueue only — never a name over the call.
      if (effects.edges.some((edge) => edge.kind === "external-call" && edge.transactionGroup !== null)) {
        fail("detected-network-in-group", "an external call was enclosed in a transaction group");
      }
      const tx = effects.transactions.find((row) => row.groupId === "tasklinks/tx-intent-dispatch");
      if (!tx) {
        fail("detected-atomic-group", "the dispatch local transaction is not declared");
      } else if (
        JSON.stringify(tx.members) !== JSON.stringify([
          { kind: "enqueue-job", operation: "tasklinks.dispatch_outbox", subject: "job:tasklinks.queue/outbox" },
        ])
      ) {
        fail("detected-atomic-group-members", JSON.stringify(tx.members));
      }
      // Caller-asserted trust never mints verified confidence: the
      // envelope ASSERTS verified, the join caps it at extracted (an
      // envelope operand is adapter-extracted evidence, never an
      // owner confirmation).
      if (!effects.edges.every((edge) => edge.confidence !== "verified" && edge.confidence !== "current")) {
        fail("detected-trust-clamp", "a caller-asserted envelope minted owner-level confidence");
      }
      if (!byKind("external-call").some((edge) => edge.confidence === "extracted")) {
        fail("detected-clamped-confidence", "the detected edges must render as extracted");
      }
      // The typed partial-failure trace: a two-recipient dispatch with
      // per-recipient results and failed-only retry, realizing the
      // declared partial-delivery case of the same fixture family.
      const record = effects.partialFailures[0];
      if (record && record.basis !== "envelope-declaration") {
        fail("detected-partial-basis", "file-origin traces are not verified execution");
      }
      if (!record) {
        fail("detected-partial-failure", "the typed partial-failure trace is missing");
      } else {
        const declared = read("tests/fixtures/task-external-links/contracts/extended-effects.json");
        const declaredCase = declared.partialFailureCases.find((row) => row.caseId === record.caseRef);
        if (!declaredCase) {
          fail("detected-partial-failure-case", `case ${record.caseRef} is not a declared case`);
        } else if (declaredCase.scenarioRef.scenarioId !== "tasklinks.scenario.partial_delivery") {
          fail("detected-partial-failure-scenario", declaredCase.scenarioRef.scenarioId);
        }
        const delivered = record.recipients.filter((row) => row.result === "delivered");
        const failed = record.recipients.filter((row) => row.result === "failed");
        if (record.recipients.length !== 2 || delivered.length !== 1 || failed.length !== 1) {
          fail("detected-partial-failure-recipients", JSON.stringify(record.recipients));
        }
        if (delivered[0]?.receiptDigest === null || delivered[0]?.errorClass !== null) {
          fail("detected-partial-failure-receipt", JSON.stringify(delivered[0]));
        }
        if (failed[0]?.errorClass === null || failed[0]?.receiptDigest !== null) {
          fail("detected-partial-failure-error", JSON.stringify(failed[0]));
        }
        if (
          JSON.stringify(record.retryEligible) !== JSON.stringify([failed[0]?.provider])
          || record.retryEligible.includes(delivered[0]?.provider)
        ) {
          fail("detected-partial-failure-retry", JSON.stringify(record.retryEligible));
        }
      }
    }
  }
  {
    const effects = detected("tasklinks.request_action");
    if (effects) {
      // The typed extracted group names two real local writes of this
      // compilation. It is a caller declaration, not proof of atomic
      // execution; authoritative admission is exercised separately above.
      const grouped = effects.edges.filter((edge) => edge.transactionGroup === "tasklinks/tx-intent-local");
      const groupedKinds = grouped.map((edge) => edge.kind + ":" + edge.resource.id).sort();
      if (
        JSON.stringify(groupedKinds)
        !== JSON.stringify(["create:tasklinks.outbox_item", "update:planner.task"])
      ) {
        fail("detected-local-group", "the request_action atomic pair must share one group: " + JSON.stringify(groupedKinds));
      }
      const tx = effects.transactions.find((row) => row.groupId === "tasklinks/tx-intent-local");
      if (tx && tx.basis !== "envelope-declaration") {
        fail("detected-transaction-basis", "file-origin groups are not verified atomicity");
      }
      if (!tx) {
        fail("detected-local-declaration", "the request_action local transaction is not declared");
      } else {
        const members = tx.members
          .map((member) => member.kind + ":" + member.subject)
          .sort();
        if (
          JSON.stringify(members) !== JSON.stringify([
            "create:canonical:tasklinks.outbox_item",
            "update:canonical:planner.task",
          ])
        ) {
          fail("detected-local-declaration-members", JSON.stringify(tx.members));
        }
      }
    }
  }
  {
    const effects = detected("tasklinks.ack_delivery");
    if (effects) {
      // The receipt reconciliation call is a public edge that sits
      // OUTSIDE the local ack transaction (network calls are never
      // atomic-group members), and the durable receipt event carries
      // the typed local ack group.
      const reconcile = effects.edges.find(
        (edge) => edge.kind === "external-call"
          && edge.resource.id === "tasklinks.provider/github-issues",
      );
      if (!reconcile) fail("detected-partial-failure", "the receipt reconciliation edge is missing");
      if (reconcile && reconcile.transactionGroup !== null) {
        fail("detected-reconcile-in-group", "the reconciliation network call is enclosed in a transaction group");
      }
      if (!effects.edges.some((edge) => edge.kind === "emit-event" && edge.resource.id === "tasklinks.delivery_recorded" && edge.transactionGroup === "tasklinks/tx-intent-ack")) {
        fail("detected-receipt-event", "the durable delivery receipt event is missing");
      }
      if (!effects.transactions.some((row) => row.groupId === "tasklinks/tx-intent-ack")) {
        fail("detected-ack-declaration", "the ack local transaction is not declared");
      }
    }
  }
  // A stale envelope (source revision not the live IR digest) refuses
  // closed: the join is pin-checked, never projected blindly.
  {
    const run = spawnSync(
      resolvedBinary,
      ["--no-cache", "--json", "effects", "show", "tasklinks.dispatch_outbox",
       "--project", "tests/fixtures/task-external-links/project",
       "--evidence", "tests/fixtures/task-external-links/invalid/detected-effects-stale.json"],
      { cwd: root, encoding: "utf8", timeout: 120000 },
    );
    if (run.error || run.status === 0 || !(run.stdout + run.stderr).includes("effects-stale-pin")) {
      fail("detected-stale-pin", "a stale envelope must refuse with effects-stale-pin");
    }
  }
  // Typed semantics refuse their forgeries: a network call inside a
  // transaction group, an undeclared group string, a transaction
  // declaration that does not match its grouped entries, and a
  // partial-failure trace whose retry set re-targets a delivered
  // recipient are each one typed refusal — meaningful-looking names
  // never stand in for the semantics.
  {
    const work = mkdtempSync(join(tmpdir(), "lekalo-detected-"));
    try {
      const base = JSON.parse(readFileSync(join(root, detectedEnvelope), "utf8"));
      const refuse = (name, mutate, expected) => {
        const mutated = JSON.parse(JSON.stringify(base));
        mutate(mutated);
        const path = join(work, `${name}.json`);
        if (name !== "tampered-digest") {
          delete mutated.evidenceDigest;
          const sort = (value) => Array.isArray(value) ? value.map(sort)
            : value && typeof value === "object" ? Object.fromEntries(
              Object.keys(value).sort().map((key) => [key, sort(value[key])])) : value;
          mutated.evidenceDigest = "sha256:" + createHash("sha256")
            .update(JSON.stringify(sort(mutated))).digest("hex");
        }
        writeFileSync(path, JSON.stringify(mutated));
        const run = spawnSync(
          resolvedBinary,
          ["--no-cache", "--json", "effects", "show", "tasklinks.dispatch_outbox",
           "--project", "tests/fixtures/task-external-links/project",
           "--evidence", path],
          { cwd: root, encoding: "utf8", timeout: 120000 },
        );
        if (run.error || run.status === 0 || !(run.stdout + run.stderr).includes(expected)) {
          fail(`detected-${name}`, `expected refusal ${expected}`);
        }
      };
      refuse("tampered-digest", (doc) => {
        doc.entries[0].subject = "external-service:arbitrary/recipient";
      }, "evidence-digest-mismatch");
      refuse("omitted-failed-retry", (doc) => {
        doc.entries[0].partialFailure.retryEligible = [];
      }, "partial-failure-retry");
      refuse("all-recipients-failed", (doc) => {
        const recipient = doc.entries[0].partialFailure.recipients[0];
        recipient.result = "failed";
        delete recipient.receiptDigest;
        recipient.errorClass = "timeout";
      }, "partial-failure-shape");
      refuse("invented-local-write", (doc) => {
        doc.transactions[1].members[0].subject = "canonical:tasklinks.conflict";
        doc.entries[2].subject = "canonical:tasklinks.conflict";
      }, "group-member-undeclared");
      refuse("duplicate-local-member", (doc) => {
        doc.transactions[1].members.push(structuredClone(doc.transactions[1].members[0]));
      }, "group-member-mismatch");
      refuse("network-in-group", (doc) => {
        doc.entries[1].transactionGroup = "tasklinks/tx-intent-dispatch";
        doc.entries[0].transactionGroup = "tasklinks/tx-intent-dispatch";
        doc.transactions[0].members.push({
          operation: "tasklinks.dispatch_outbox",
          kind: "external-call",
          subject: "external-service:tasklinks.provider/github-issues",
        });
      }, "network-in-group");
      refuse("group-undeclared", (doc) => {
        delete doc.entries[1].transactionGroup;
        doc.transactions = [];
        doc.entries = doc.entries.map((entry) => {
          if (entry.kind === "enqueue-job") entry.transactionGroup = "tasklinks/tx-undeclared";
          if (entry.transactionGroup && entry.transactionGroup !== "tasklinks/tx-undeclared") {
            delete entry.transactionGroup;
          }
          return entry;
        });
      }, "group-undeclared");
      refuse("group-member-mismatch", (doc) => {
        doc.transactions[0].members = [{
          operation: "tasklinks.dispatch_outbox",
          kind: "enqueue-job",
          subject: "job:tasklinks.queue/elsewhere",
        }];
      }, "group-member-mismatch");
      refuse("partial-failure-retry", (doc) => {
        doc.entries[0].partialFailure.retryEligible = [
          "tasklinks.provider/github-issues",
          "tasklinks.provider/mock-secondary",
        ];
      }, "partial-failure-retry");
      refuse("partial-failure-shape", (doc) => {
        doc.entries[0].partialFailure.recipients = [doc.entries[0].partialFailure.recipients[0]];
      }, "partial-failure-shape");
    } finally {
      rmSync(work, { recursive: true, force: true });
    }
  }
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
