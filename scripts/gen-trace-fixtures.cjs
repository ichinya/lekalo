#!/usr/bin/env node
// Throwaway generator for the issue #22 trace fixture matrix.
// Produces the valid-full input, the valid-partial input, and the
// invalid wire/semantics fixtures; golden canonical bytes are produced
// separately through the CLI export.

const { createHash } = require("node:crypto");
const { mkdirSync, writeFileSync } = require("node:fs");
const { join, resolve } = require("node:path");

const root = resolve(process.argv[2] ?? ".");
const out = (relative, content) => {
  const path = join(root, relative);
  mkdirSync(join(path, ".."), { recursive: true });
  writeFileSync(path, content);
};

const DOMAIN = "lekalo/trace-manifest/v1.0.0/relation";
const sha = (text) => createHash("sha256").update(text).digest("hex");
const digest = (text) => `sha256:${sha(text)}`;
const relationId = (kind, from, to, occurrence) =>
  `sha256:${sha(JSON.stringify([DOMAIN, kind, from, to, occurrence]))}`;

const REV = "a".repeat(40);
const OLD_REV = "b".repeat(40);
const MODEL_DIGEST = digest("model-bytes");
const IR_DIGEST = digest("ir-bytes");
const GRAPH_DIGEST = digest("graph-bytes");
const ARTIFACT_CONTENT = digest("focus-task.ts");
const SCENARIO_EVIDENCE = digest("scenario-evidence");
const TEST_EVIDENCE = digest("test-evidence");
const GATE_EVIDENCE = digest("gate-evidence");

const rel = (kind, from, to, occurrence, extra = {}) => ({
  relationId: relationId(kind, from, to, occurrence),
  relationKind: kind,
  fromNode: from,
  toNode: to,
  occurrence,
  provenance: {
    origin: "declared",
    sourceSystem: "openspec",
    sourceRevision: REV,
    sourceDigest: MODEL_DIGEST,
    recordedBy: "lekalo.core",
    ...(extra.provenance ?? {}),
  },
  confidence: extra.confidence ?? "exact",
  status: extra.status ?? "confirmed",
  evidenceRefs: extra.evidenceRefs ?? ["gate:hlv-gate-focus"],
});

const baseManifest = (extra = {}) => ({
  zzzShuffleMarker: undefined,
  gaps: extra.gaps ?? [],
  relations: extra.relations,
  nodes: extra.nodes,
  exportProfile: extra.exportProfile ?? "requirement-to-gate",
  modelRef: { schemaVersion: "1.0.0", digest: MODEL_DIGEST },
  sourceRevision: extra.sourceRevision ?? REV,
  completeness: extra.completeness,
  projectRef: "planner",
  identity: "dev.lekalo.trace-manifest@1.0.0",
  manifestId: extra.manifestId ?? "planner-trace-full",
  schemaVersion: "lekalo/trace-manifest/v1.0.0",
  irRef: { schemaVersion: "0.1.0", digest: IR_DIGEST },
  graphRef: { schemaVersion: "1.0.0", digest: GRAPH_DIGEST },
});

const node = (nodeId, nodeKind, identityField, identity, extra = {}) => ({
  nodeId,
  nodeKind,
  [identityField]: identity,
  ...extra,
});

const requirement = (id, refs = []) =>
  node(`requirement:${id}`, "requirement", "requirementId", id, refs.length ? { externalRefs: refs } : {});
const symbol = (id, refs = []) =>
  node(`symbol:${id}`, "symbol", "semanticId", id, refs.length ? { externalRefs: refs } : {});

const artifactNode = {
  nodeId: "artifact:apps-api-focus-task",
  nodeKind: "artifact",
  artifactId: "apps-api-focus-task",
  ownership: "generated",
  path: "apps/api/src/planner/focus-task.ts",
  contentDigest: ARTIFACT_CONTENT,
  revision: REV,
  generatorRef: "dev.lekalo/lekalo-generator",
  manifestDigest: GRAPH_DIGEST,
  externalRefs: [{ system: "aifhub", originalId: "provider.node.artifact.1" }],
};

const scenarioNode = {
  nodeId: "scenario:planner-scenario-switch-focus",
  nodeKind: "scenario",
  scenarioId: "planner.scenario.switch_focus",
  contractVersion: "0.1.0",
  evidenceDigest: SCENARIO_EVIDENCE,
};

const testNode = {
  nodeId: "test:node-focus-task-switch",
  nodeKind: "native_test",
  testId: "node.focus-task-switch",
  contractVersion: "1.0.0",
  evidenceDigest: TEST_EVIDENCE,
  externalRefs: [{ system: "source-native", originalId: "node.test.focus-task-switch" }],
};

const gateNode = {
  nodeId: "gate:hlv-gate-focus",
  nodeKind: "gate",
  gateId: "hlv.gate.focus",
  contractVersion: "1.0.0",
  evidenceDigest: GATE_EVIDENCE,
  externalRefs: [{ system: "hlv", originalId: "hlv.native.gate.focus" }],
};

const diagnosticNode = {
  nodeId: "diagnostic:hlv-diag-focus",
  nodeKind: "diagnostic",
  diagnosticId: "hlv.diag.focus",
  contractVersion: "1.0.0",
  externalRefs: [{ system: "hlv", originalId: "hlv.native.diag.focus" }],
};

const fullRelations = [
  rel("implements", "symbol:planner.focus_task", "requirement:PLANNER-REQ-001", "requirements.focus_task"),
  rel("implements", "symbol:planner.switch_focus", "requirement:PLANNER-REQ-001", "requirements.switch_focus"),
  rel("implements", "symbol:planner.focus_task", "requirement:PLANNER-REQ-002", "requirements.focus_task_archive"),
  rel("binds", "symbol:planner.focus_task", "artifact:apps-api-focus-task", "bindings.focus_task"),
  rel("binds", "symbol:planner.focus_task", "artifact:apps-api-focus-task", "bindings.focus_task_shadow", {
    provenance: { sourceSystem: "aifhub" },
    confidence: "high",
  }),
  rel("covers", "scenario:planner-scenario-switch-focus", "symbol:planner.focus_task", "covers.focus_task", {
    provenance: { sourceSystem: "lekalo" },
  }),
  rel("verifies", "test:node-focus-task-switch", "scenario:planner-scenario-switch-focus", "verifies.switch_focus", {
    provenance: { sourceSystem: "source-native" },
    confidence: "high",
  }),
  rel("evidences", "gate:hlv-gate-focus", "test:node-focus-task-switch", "evidences.focus", {
    provenance: { sourceSystem: "hlv" },
  }),
  rel("references", "gate:hlv-gate-focus", "diagnostic:hlv-diag-focus", "references.focus_diag", {
    provenance: { sourceSystem: "hlv" },
  }),
];

const fullNodes = [
  requirement("PLANNER-REQ-001", [{ system: "openspec", originalId: "PLANNER-REQ-001" }]),
  requirement("PLANNER-REQ-002", [{ system: "openspec", originalId: "PLANNER-REQ-002" }]),
  symbol("planner.focus_task"),
  symbol("planner.switch_focus"),
  artifactNode,
  scenarioNode,
  testNode,
  gateNode,
  diagnosticNode,
];

// ---- Valid full ----
out("tests/fixtures/trace/full.trace.json", JSON.stringify(baseManifest({ completeness: "full", relations: fullRelations, nodes: fullNodes }), null, 2) + "\n");

// ---- Valid partial: REQ-003's symbol has no gate chain, plus one stale
// retained prior-revision binding. ----
const staleBinding = rel("binds", "symbol:planner.archive_task", "artifact:apps-api-focus-task", "bindings.archive_prior", {
  status: "stale",
  confidence: "medium",
  provenance: { sourceRevision: OLD_REV, sourceSystem: "aifhub" },
});
const partialRelations = [
  ...fullRelations,
  rel("implements", "symbol:planner.archive_task", "requirement:PLANNER-REQ-003", "requirements.archive_task"),
  staleBinding,
];
const partialNodes = [
  ...fullNodes,
  requirement("PLANNER-REQ-003", [{ system: "openspec", originalId: "PLANNER-REQ-003" }]),
  symbol("planner.archive_task"),
];
const partialGaps = [
  {
    gapKind: "missing-gate",
    status: "candidate",
    anchorNode: "symbol:planner.archive_task",
    expected: "hlv.gate.archive",
    sourcePath: "hlv/gates.md",
  },
  {
    gapKind: "stale-revision",
    status: "stale",
    anchorNode: "symbol:planner.archive_task",
  },
];
out("tests/fixtures/trace/partial.trace.json", JSON.stringify(baseManifest({ completeness: "partial", relations: partialRelations, nodes: partialNodes, gaps: partialGaps, manifestId: "planner-trace-partial" }), null, 2) + "\n");
// ---- Invalid matrix: one deliberate flaw each. ----
const clone = (value) => JSON.parse(JSON.stringify(value));
const full = () => clone(baseManifest({ completeness: "full", relations: fullRelations, nodes: fullNodes }));

const mutate = (name, mutateFn) => {
  const manifest = clone(full());
  mutateFn(manifest);
  out(`tests/fixtures/trace/invalid/${name}.json`, JSON.stringify(manifest, null, 2) + "\n");
};

mutate("wrong-schema-version", (m) => { m.schemaVersion = "lekalo/trace/v1"; });
mutate("wrong-identity", (m) => { m.identity = "dev.lekalo.trace@1.0.0"; });
mutate("unknown-top-key", (m) => { m.extraKey = true; });
mutate("unknown-node-key", (m) => { m.nodes[0].extraNodeKey = 1; });
mutate("duplicate-json-key", (m) => {
  // Hand-splice a duplicate nodeId key into the first node's raw JSON.
});
{
  // Splice duplicate key: serialize, then inject into the first node.
  const manifest = full();
  const text = JSON.stringify(manifest, null, 2);
  const marker = '"nodeId": "requirement:PLANNER-REQ-001",';
  const injected = text.replace(marker, `${marker}\n      "nodeId": "requirement:PLANNER-REQ-001",`);
  out("tests/fixtures/trace/invalid/duplicate-json-key.json", injected + "\n");
}
mutate("duplicate-node-id", (m) => { m.nodes[1].nodeId = m.nodes[0].nodeId; });
mutate("duplicate-identity", (m) => { m.nodes[3].semanticId = "planner.focus_task"; });
mutate("foreign-identity", (m) => { m.nodes[2].gateId = "hlv.gate.focus"; });
mutate("duplicate-relation-tuple", (m) => { m.relations.push(clone(m.relations[0])); });
mutate("malformed-semantic-id", (m) => { m.nodes[2].semanticId = ".hidden"; });
mutate("malformed-digest", (m) => { m.modelRef.digest = "sha256:short"; });
mutate("dangling-endpoint", (m) => {
  m.relations.push(rel("implements", "symbol:planner.ghost", "requirement:PLANNER-REQ-001", "requirements.ghost"));
});
mutate("dangling-evidence", (m) => {
  m.relations[0].evidenceRefs.push("gate:hlv-gate-ghost");
});
mutate("endpoints-illegal", (m) => {
  m.relations[0] = rel("implements", "requirement:PLANNER-REQ-001", "symbol:planner.focus_task", "requirements.reversed");
});
mutate("confirmed-policy-revision", (m) => {
  m.relations[0].provenance.sourceRevision = OLD_REV;
});
mutate("confirmed-policy-inferred", (m) => {
  m.relations[0].provenance.origin = "inferred";
});
mutate("confirmed-policy-no-evidence", (m) => {
  m.relations[0].evidenceRefs = [];
});
mutate("relation-id-mismatch", (m) => { m.relations[0].relationId = digest("forged"); });
mutate("full-with-gaps", (m) => {
  m.completeness = "full";
  m.gaps = [{ gapKind: "missing-gate", status: "candidate" }];
});
mutate("partial-without-gap", (m) => { m.completeness = "partial"; });
mutate("full-with-uncovered-sink", (m) => {
  m.nodes.push(requirement("PLANNER-REQ-009"));
  m.relations.push(rel("implements", "symbol:planner.orphan", "requirement:PLANNER-REQ-009", "requirements.orphan"));
  m.nodes.push(symbol("planner.orphan"));
});
mutate("absolute-path", (m) => {
  const artifact = m.nodes.find((n) => n.nodeKind === "artifact");
  artifact.path = "/etc/passwd";
});
mutate("backslash-path", (m) => {
  const artifact = m.nodes.find((n) => n.nodeKind === "artifact");
  artifact.path = "apps\\api\\focus-task.ts";
});
mutate("traversal-path", (m) => {
  const artifact = m.nodes.find((n) => n.nodeKind === "artifact");
  artifact.path = "apps/../secrets/focus-task.ts";
});
mutate("device-path", (m) => {
  const artifact = m.nodes.find((n) => n.nodeKind === "artifact");
  artifact.path = "CON/focus-task.ts";
});
mutate("bad-revision", (m) => { m.sourceRevision = "HEAD"; });
mutate("bad-model-version", (m) => { m.modelRef.schemaVersion = "2.0.0"; });
mutate("bad-ir-version", (m) => { m.irRef.schemaVersion = "9.9.9"; });
mutate("unknown-external-system", (m) => {
  m.nodes[0].externalRefs = [{ system: "wiki", originalId: "PLANNER-REQ-001" }];
});
mutate("gap-confirmed-status", (m) => {
  m.completeness = "partial";
  m.gaps = [{ gapKind: "missing-gate", status: "confirmed" }];
});
mutate("gap-dangling-anchor", (m) => {
  m.completeness = "partial";
  m.gaps = [{ gapKind: "missing-gate", status: "candidate", anchorNode: "symbol:planner.ghost" }];
});
mutate("over-limit-nodes", (m) => {
  m.nodes = Array.from({ length: 100_001 }, (_, index) =>
    requirement(`X-${index}`));
});
mutate("bad-occurrence", (m) => { m.relations[0].occurrence = ".leading"; });
mutate("artifact-missing-digest", (m) => {
  const artifact = m.nodes.find((n) => n.nodeKind === "artifact");
  delete artifact.contentDigest;
});
mutate("manifest-id-uppercase", (m) => { m.manifestId = "Planner-Trace"; });

// BOM and trailing-bytes fixtures carry raw byte prefixes/suffixes.
{
  const text = JSON.stringify(full(), null, 2);
  out("tests/fixtures/trace/invalid/bom-prefix.json", "\uFEFF" + text + "\n");
  out("tests/fixtures/trace/invalid/trailing-bytes.json", text + "\n{}\n");
}

console.log("fixtures written");
