#!/usr/bin/env node
// Issue #51 binary-driven impact gate.
//
// It resolves a freshly built `lekalo` binary, runs the exact graph
// reverse closure and bounded impact query from the contract, compares
// both against the committed consumer oracle, and checks the additive /
// breaking mutation probes remain declared. The default impact profile
// supplies the complete consumer set; strict profile may honestly deny
// on missing runtime gates without making the consumer closure vanish.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const binary = process.env.LEKALO_BIN ?? resolve(root, "target/debug/lekalo.exe");
const project = "tests/fixtures/pilot/worklog-changelog/model";
const oracle = JSON.parse(
  readFileSync(resolve(root, "tests/fixtures/pilot/worklog-changelog/evidence/consumer-oracle.json"), "utf8"),
);
const run = (args) => JSON.parse(execFileSync(binary, args, { cwd: root, encoding: "utf8" }).trim());
const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

const graph = run([
  "--json",
  "--no-cache",
  "graph",
  "callers",
  "changelog.public_entry_dto",
  "--transitive",
  "--project",
  project,
]);
if (graph.status !== "valid") fail("graph-denied", graph);
const expectedGraph = new Set(
  oracle.expectedConsumers
    .filter((consumer) => ["query", "endpoint", "scenario"].includes(consumer.kind))
    .map((consumer) => {
      if (consumer.kind === "query") return `operation:${consumer.consumer}`;
      if (consumer.kind === "endpoint") return `endpoint:${consumer.consumer}`;
      return `scenario:${consumer.consumer}`;
    }),
);
const actualGraph = new Set(
  graph.graph.callers
    .map((edge) => edge.from)
    .filter((subject) => expectedGraph.has(subject)),
);
for (const subject of expectedGraph) {
  if (!actualGraph.has(subject)) fail("consumer-missing", subject);
}

const impact = run([
  "--json",
  "--no-cache",
  "impact",
  "changelog.public_entry_dto",
  "--depth",
  "256",
  "--project",
  project,
]);
if (impact.status !== "valid") fail("impact-denied", impact);
const transitive = impact.impact.transitive;
if (!transitive.complete || transitive.omitted !== 0) {
  fail("impact-incomplete", {
    complete: transitive.complete,
    omitted: transitive.omitted,
  });
}
const actualImpact = new Set(transitive.items.map((item) => item.subject));
for (const consumer of oracle.expectedConsumers.filter((row) => ["endpoint", "scenario"].includes(row.kind))) {
  const subject = `${consumer.kind === "endpoint" ? "endpoint" : "scenario"}:${consumer.consumer}`;
  if (!actualImpact.has(subject)) fail("impact-consumer-missing", subject);
}

// The oracle's mutation probes are closed and must carry both an
// additive non-breaking and a breaking removal witness; the exact
// consumer set for the removal is compared against the binary closure.
if (oracle.mutationProbes.length !== 2) fail("mutation-probe-count", oracle.mutationProbes.length);
const additive = oracle.mutationProbes.find((probe) => !probe.breaking);
const breaking = oracle.mutationProbes.find((probe) => probe.breaking);
if (!additive || additive.expectedDelta.length !== 0) fail("additive-probe-drift");
if (!breaking || breaking.expectedDelta.length === 0) fail("breaking-probe-drift");
for (const consumer of breaking.expectedDelta) {
  const operation = `operation:${consumer}`;
  const endpoint = `endpoint:${consumer}`;
  if (!actualImpact.has(operation) && !actualGraph.has(operation) && !actualImpact.has(endpoint) && !actualGraph.has(endpoint)) {
    fail("breaking-probe-consumer-drift", consumer);
  }
}

process.stdout.write(
  `${JSON.stringify({
    ok: true,
    gate: "test-worklog-changelog-impact",
    graphConsumers: actualGraph.size,
    impactConsumers: actualImpact.size,
    depth: 256,
    complete: transitive.complete,
    mutationProbes: oracle.mutationProbes.length,
  })}\n`,
);
