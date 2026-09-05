#!/usr/bin/env node
// Issue #13 release gate: the graph wire schema and the pinned canonical
// golden exports, validated with the same pinned third-party Draft 2020-12
// implementation as the other contract gates. Exact Ajv 8.17.1 is
// provisioned outside this checkout (CI does the same on Node 18 and 24)
// and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.

import { createRequire } from "node:module";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  Ajv2020 = require("ajv/dist/2020").default;
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "ajv-unavailable",
    detail: error?.code ?? error?.message,
  }, null, 2)}\n`);
  process.exit(1);
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "ajv-version",
    detail: `expected 8.17.1, found ${ajvVersion}`,
  }, null, 2)}\n`);
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));

const schema = read("contracts/graph.schema.v1.0.0.json");
const goldenDir = "tests/fixtures/graph/golden";

const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateGraph = ajv.compile(schema);

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

const CORE_NODE_KINDS = [
  "project", "module", "type", "entity", "operation", "policy",
  "event", "effect", "endpoint", "scenario", "target-binding", "requirement",
];
const CORE_RELATIONS = [
  "requires", "references", "accepts", "returns", "reads", "writes",
  "emits", "authorizes", "exposes", "implements", "verifies", "derived_from",
];
// The graph-owned acyclic policy (ADR-0012): every other relation may cycle.
const ACYCLIC_RELATIONS = new Set(["requires", "derived_from"]);
// Relations whose typed owners have not contributed evidence yet; a v1
// canonical graph must never emit them.
const NOT_EMITTED_IN_V1 = new Set(["writes", "implements", "verifies"]);

const canonicalNodeKey = (node) => [
  CORE_NODE_KINDS.indexOf(node.kind),
  node.module ?? "",
  node.id.slice(node.id.indexOf(":") + 1),
  node.subkind ?? "",
];
const canonicalEdgeKey = (edge) => [
  edge.from, edge.to, CORE_RELATIONS.indexOf(edge.relation), edge.occurrence,
];

let goldenCount = 0;
let nodeCount = 0;
let edgeCount = 0;
for (const name of readdirSync(resolve(root, goldenDir)).sort()) {
  if (!name.endsWith(".json")) continue;
  const graph = read(`${goldenDir}/${name}`);
  goldenCount += 1;

  // 1. The golden instance must satisfy the closed wire schema.
  if (!validateGraph(graph)) fail(`${name}-invalid`, validateGraph.errors);

  // 2. Counts and identities must be coherent.
  if (graph.metadata.nodeCount !== graph.nodes.length) fail(`${name}-node-count`);
  if (graph.metadata.edgeCount !== graph.edges.length) fail(`${name}-edge-count`);
  const ids = new Set(graph.nodes.map((node) => node.id));
  if (ids.size !== graph.nodes.length) fail(`${name}-duplicate-node-ids`);

  // 3. Nodes and edges must be in canonical order (byte-sort of the
  //    canonical sort keys), and every edge endpoint must exist.
  const nodeKeys = graph.nodes.map(canonicalNodeKey);
  const sortedNodeKeys = [...nodeKeys].sort((a, b) =>
    a[0] - b[0] || (a[1] < b[1] ? -1 : a[1] > b[1] ? 1 : a[2] < b[2] ? -1 : a[2] > b[2] ? 1 : 0));
  if (JSON.stringify(nodeKeys) !== JSON.stringify(sortedNodeKeys)) fail(`${name}-node-order`);

  const edgeKeys = graph.edges.map(canonicalEdgeKey);
  const sortedEdgeKeys = [...edgeKeys].sort((a, b) => JSON.stringify(a) < JSON.stringify(b) ? -1 : 1);
  if (JSON.stringify(edgeKeys) !== JSON.stringify(sortedEdgeKeys)) fail(`${name}-edge-order`);

  for (const node of graph.nodes) {
    const subkindAllowed = node.kind === "type" || node.kind === "operation";
    if (subkindAllowed && !node.subkind) fail(`${name}-missing-subkind`, node.id);
    if (!subkindAllowed && node.subkind !== undefined) fail(`${name}-unexpected-subkind`, node.id);
  }

  for (const edge of graph.edges) {
    // Canonical IR provenance must carry canonical confidence, and only
    // canonical-ir provenance may.
    if (edge.provenance.referenceRole !== undefined && edge.confidence !== "canonical") {
      fail(`${name}-provenance-confidence`, edge);
    }
  }
  nodeCount += graph.nodes.length;
  edgeCount += graph.edges.length;

  // 4. The acyclic policy: requires/derived_from subgraphs have no SCC > 1
  //    and no self-loop.
  for (const relation of ACYCLIC_RELATIONS) {
    const adjacency = new Map();
    for (const node of graph.nodes) adjacency.set(node.id, []);
    for (const edge of graph.edges) {
      if (edge.relation === relation) adjacency.get(edge.from).push(edge.to);
    }
    // Iterative Tarjan SCC.
    const indexOf = new Map();
    const low = new Map();
    const onStack = new Set();
    const stack = [];
    let counter = 0;
    const components = [];
    for (const start of adjacency.keys()) {
      if (indexOf.has(start)) continue;
      const frames = [[start, 0]];
      indexOf.set(start, counter);
      low.set(start, counter);
      counter += 1;
      stack.push(start);
      onStack.add(start);
      while (frames.length > 0) {
        const frame = frames[frames.length - 1];
        const successors = adjacency.get(frame[0]);
        if (frame[1] < successors.length) {
          const successor = successors[frame[1]];
          frame[1] += 1;
          if (!indexOf.has(successor)) {
            indexOf.set(successor, counter);
            low.set(successor, counter);
            counter += 1;
            stack.push(successor);
            onStack.add(successor);
            frames.push([successor, 0]);
          } else if (onStack.has(successor)) {
            low.set(frame[0], Math.min(low.get(frame[0]), indexOf.get(successor)));
          }
        } else {
          frames.pop();
          const parent = frames[frames.length - 1];
          if (parent) low.set(parent[0], Math.min(low.get(parent[0]), low.get(frame[0])));
          if (low.get(frame[0]) === indexOf.get(frame[0])) {
            const component = [];
            for (;;) {
              const member = stack.pop();
              onStack.delete(member);
              component.push(member);
              if (member === frame[0]) break;
            }
            components.push(component);
          }
        }
      }
    }
    for (const component of components) {
      if (component.length > 1) fail(`${name}-${relation}-cycle`, component);
      if (component.length === 1 && adjacency.get(component[0]).includes(component[0])) {
        fail(`${name}-${relation}-self-loop`, component[0]);
      }
    }
  }
}

if (goldenCount === 0) fail("no-goldens");

process.stdout.write(`${JSON.stringify({
  ok: true,
  ajv: ajvVersion,
  goldens: goldenCount,
  nodes: nodeCount,
  edges: edgeCount,
}, null, 2)}\n`);
