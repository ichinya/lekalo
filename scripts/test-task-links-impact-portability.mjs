#!/usr/bin/env node
// Issue #52 impact/portability gate: a provider-contract change pair
// (exactly one upstream pin moves) is joined through the live CLI:
// every core row stays byte-identical while the pinned adapter rows
// move, the named consumers witness the change, and strict admission
// keeps refusing the un-evidenced target guarantees. Requires the
// built binary (fail-closed), like every binary-dependent lane.

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const gate = "task-links-impact-portability";
const failures = [];
const fail = (reason, detail) => failures.push({ reason, detail });
const read = (relative) => JSON.parse(readFileSync(join(root, relative), "utf8"));

const binaryCandidates = [
  join(root, "target/debug/lekalo"),
  join(root, "target/debug/lekalo.exe"),
];
const binary = binaryCandidates.find((candidate) => existsSync(candidate));
if (!binary) {
  fail("binary-missing", "build the workspace first: cargo build --workspace --locked");
} else {
  const run = (args) => {
    const result = spawnSync(binary, ["--no-cache", ...args], {
      cwd: root,
      encoding: "utf8",
      timeout: 120000,
    });
    if (result.error) {
      fail("spawn-error", String(result.error));
    }
    return result;
  };

  const project = "tests/fixtures/task-external-links/project";
  const attachmentPath = "tests/fixtures/task-external-links/contracts/integration.json";
  const fromProfile = "tests/fixtures/task-external-links/profiles/laravel.json";
  const toProfile = "tests/fixtures/task-external-links/profiles/node.json";
  const pair = read("tests/fixtures/task-external-links/impact/provider-change-pair.json");

  // 1. The pair mutates exactly the declared members.
  const original = readFileSync(join(root, attachmentPath), "utf8");
  if (!original.includes(pair.base.upstreamPin)) fail("pair-base-missing", pair.base.upstreamPin);
  let changed = original.replaceAll(pair.base.upstreamPin, pair.changed.upstreamPin);
  changed = changed.replaceAll(pair.baseRevisionToken ?? pair.base.attachmentRevision ?? original.match(/"attachmentRevision": "([^"]*)"/)[1], pair.changedRevision);
  if (changed === original) fail("pair-no-op", "the mutation changed nothing");
  if (!changed.includes(pair.changedRevision)) fail("pair-revision-missing", pair.changedRevision);

  // 2. Both sides run through portability (permissive: gaps visible,
  //    exit 0) and the core rows are byte-identical while the pinned
  //    adapter rows move.
  const work = mkdtempSync(join(tmpdir(), "lekalo-tasklinks-"));
  try {
    const changedPath = join(work, "changed-attachment.json");
    writeFileSync(changedPath, changed);
    const evidencePath = "tests/fixtures/task-external-links/evidence/mock-run.json";
    // The changed attachment is a same-shape provider-only mutation, so
    // its mock evidence is the base receipt re-pinned to the changed
    // revision (the canonical revision recomputation mirrors the core).
    const REVISION_MEMBERS = [
      "schemaVersion", "identity", "projectId", "modelRef", "irRef",
      "ownerContractPins", "entities", "relations", "uniqueConstraints",
      "providerContracts", "linkCapabilityRequirements", "fieldPolicies",
      "actionPolicies", "operationBindings", "deliveryRules", "contributions",
    ];
    const sortDeep = (value) => {
      if (Array.isArray(value)) return value.map(sortDeep);
      if (value && typeof value === "object") {
        const out = {};
        for (const key of Object.keys(value).sort()) out[key] = sortDeep(value[key]);
        return out;
      }
      return value;
    };
    const canonicalRevision = (document) => {
      const revision = {};
      for (const member of REVISION_MEMBERS) revision[member] = document[member];
      return "sha256:" + createHash("sha256")
        .update(JSON.stringify(sortDeep(revision)))
        .digest("hex");
    };
    const changedEvidence = join(work, "changed-evidence.json");
    {
      const baseEvidence = read(attachmentPath.replace("contracts/integration.json", "evidence/mock-run.json"));
      baseEvidence.attachmentRevision = pair.changedRevision;
      writeFileSync(changedEvidence, JSON.stringify(baseEvidence, null, 2));
    }
    const portability = (attachment, evidenceDoc) =>
      run([
        "--json",
        "task-links",
        "portability",
        attachment,
        "--project",
        project,
        "--from-profile",
        fromProfile,
        "--to-profile",
        toProfile,
        ...(evidenceDoc ? ["--evidence", evidenceDoc] : []),
        "--profile",
        "permissive",
      ]);
    const base = portability(attachmentPath);
    const candidate = portability(changedPath);
    if (process.env.TL_DEBUG) process.stderr.write(`DEBUG base: status=${base.status} code=${base.status} err=${base.error} stdout0=${String(base.stdout).slice(0,80)} stderr0=${String(base.stderr).slice(0,120)}
`);
    if (base.status !== 0) fail("base-exit", String(base.status) + String(base.stderr).slice(0, 200));
    if (candidate.status !== 0) {
      fail("candidate-exit", String(candidate.status) + candidate.stderr.slice(0, 200));
    }
    if (base.status === 0 && candidate.status === 0) {
      const baseReport = JSON.parse(base.stdout).taskLinks;
      const candidateReport = JSON.parse(candidate.stdout).taskLinks;
      if (JSON.stringify(baseReport.coreSemantics) !== JSON.stringify(candidateReport.coreSemantics)) {
        fail("core-rows-moved", "a provider-only change must preserve every core row");
      }
      if (JSON.stringify(baseReport.providerAdapters) === JSON.stringify(candidateReport.providerAdapters)) {
        fail("adapter-rows-frozen", "the adapter rows must move with the pin");
      }
      const movedPins = candidateReport.providerAdapters.filter(
        (row) => row.pin === pair.changed.upstreamPin,
      ).length;
      if (movedPins === 0) fail("adapter-pin-not-carried", pair.changed.upstreamPin);
      // The changed attachment carries its own declared revision.
      if (candidateReport.attachmentRevision !== pair.changedRevision) {
        fail("revision-not-carried", "the changed revision must ride the report");
      }
    }

    // 3. Validate the changed attachment: schema-valid, semantic checks
    //    still pass (the pin is a declaration, not a semantic break).
    const changedValidate = run([
      "--json",
      "task-links",
      "validate",
      changedPath,
      "--project",
      project,
      "--profile",
      "strict",
    ]);
    if (changedValidate.status !== 0) {
      fail("changed-validate-exit", changedValidate.stderr.slice(0, 300));
    }

    // 4b. The reverse impact of the changed provider contract is
    //     DERIVED from the attachment and the compilation (never a
    //     prewritten list): the same consumer set derives on both
    //     sides of a pin-only change (core preserved), the set names
    //     the operation/policy/command/effect/scenario witnesses, and
    //     an unknown contract refuses.
    const derivedImpact = (attachmentArg) => {
      const impact = run([
        "--json",
        "task-links",
        "impact",
        pair.changed.contractId,
        "--project",
        project,
        "--attachment",
        attachmentArg,
      ]);
      if (impact.status !== 0) {
        fail("impact-derive-exit", String(impact.status) + impact.stderr.slice(0, 200));
        return null;
      }
      return JSON.parse(impact.stdout).taskLinks;
    };
    {
      const baseImpact = derivedImpact(attachmentPath);
      const changedImpact = derivedImpact(changedPath);
      if (baseImpact && changedImpact) {
        if (JSON.stringify(baseImpact.consumers) !== JSON.stringify(changedImpact.consumers)) {
          fail("impact-derivation-moved", "a pin-only change must derive an identical consumer set");
        }
        const symbols = changedImpact.consumers.map((consumer) => consumer.symbol);
        for (const expected of [
          "tasklinks.request_action",
          "tasklinks.policy.ap.request_action",
          "tasklinks.effect.write_outbox",
          "tasklinks.scenario.two_providers",
        ]) {
          if (!symbols.includes(expected)) {
            fail("impact-witness-missing", expected + " not derived from " + JSON.stringify(symbols));
          }
        }
        const roles = new Set(changedImpact.consumers.map((consumer) => consumer.role));
        for (const role of ["binding", "action-policy", "command", "effect", "scenario"]) {
          if (!roles.has(role)) fail("impact-role-missing", role);
        }
        if (changedImpact.upstreamPin !== pair.changed.upstreamPin) {
          fail("impact-pin", changedImpact.upstreamPin);
        }
      }
      const ghost = run([
        "--json",
        "task-links",
        "impact",
        "tasklinks.provider_contract.ghost",
        "--project",
        project,
        "--attachment",
        attachmentPath,
      ]);
      if (ghost.status !== 1) fail("impact-ghost-exit", String(ghost.status));
    }

    // 5. Model-surface consumer witnesses via the ordinary inspect
    //    command; attachment-level consumers were already joined by the
    //    validate/portability legs above.
    for (const consumer of [...pair.consumers, ...pair.attachmentLevelConsumers].filter(
      (consumer) => !consumer.symbol.startsWith("tasklinks.cap.")
        && !consumer.symbol.startsWith("tasklinks.policy.")
        && consumer.symbol !== "tasklinks.provider_contract.primary",
    )) {
      const witness = run([
        "--json",
        "inspect",
        consumer.symbol,
        "--project",
        project,
        "--include",
        "bindings,scenarios",
      ]);
      if (witness.status !== 0) {
        fail("consumer-witness", `${consumer.symbol}: ${witness.stderr.slice(0, 200)}`);
        continue;
      }
      const envelope = JSON.parse(witness.stdout);
      if (envelope.status !== "valid") fail("consumer-status", consumer.symbol);
      if (!JSON.stringify(envelope).includes(consumer.symbol)) {
        fail("consumer-absent", consumer.symbol);
      }
    }

    // 5. Strict portability keeps refusing the un-evidenced target
    //    guarantees for both pair members (unknown stays unknown; a
    //    changed pin never fabricates native evidence).
    for (const [label, attachment] of [
      ["base", attachmentPath],
      ["changed", changedPath],
    ]) {
      const strict = run([
        "--json",
        "task-links",
        "portability",
        attachment,
        "--project",
        project,
        "--from-profile",
        fromProfile,
        "--to-profile",
        toProfile,
        "--profile",
        "strict",
      ]);
      if (strict.status !== 1) fail(`strict-refusal-${label}`, String(strict.status));
      else {
        const envelope = JSON.parse(strict.stderr);
        if (!envelope.reasonCodes.includes("tasklink.required-guarantee-unsupported")) {
          fail(`strict-rule-${label}`, JSON.stringify(envelope.reasonCodes));
        }
      }
    }
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate, failures }, null, 2)}\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({ ok: true, gate })}\n`);
