#!/usr/bin/env node
// Issue #22 release gate: the neutral trace-manifest wire schema and the
// pinned canonical golden export, validated with the same pinned
// third-party Draft 2020-12 implementation as the other contract gates.
// Exact Ajv 8.17.1 is provisioned outside this checkout (CI does the same
// on Node 18 and 24) and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.
//
// This script is the independent second canonical implementation: it
// re-derives relation identities, canonical byte order, and the manifest
// digest from the golden without reusing any Rust code, then proves the
// determinism claim by permuting the golden input and re-canonicalizing
// to byte-identical output.

import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  ({ default: Ajv2020 } = require("ajv/dist/2020.js"));
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "ajv-unavailable", detail: String(error) }, null, 2)}\n`,
  );
  process.exit(1);
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "ajv-version", detail: ajvVersion }, null, 2)}\n`,
  );
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));
const readText = (relative) => readFileSync(resolve(root, relative), "utf8");

const schema = read("contracts/trace-manifest.schema.v1.0.0.json");
const goldenFile = "tests/fixtures/trace/golden/planner.trace.json";
const invalidDir = "tests/fixtures/trace/invalid";

const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateTrace = ajv.compile(schema);

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

const sha256 = (text) => createHash("sha256").update(text, "utf8").digest("hex");

// The closed v1 trace vocabulary (ADR-0014); the wire words are registry
// data. Anything outside fails the gate.
const NODE_KINDS = [
  "requirement", "symbol", "artifact", "scenario",
  "native_test", "gate", "diagnostic",
];
const SYSTEMS = ["openspec", "hlv", "source-native", "aifhub", "lekalo"];
const OWNERSHIPS = ["generated", "scaffolded", "checked", "external", "custom"];
const ORIGINS = ["declared", "observed", "inferred", "imported"];
const CONFIDENCES = ["exact", "high", "medium", "low", "unknown"];
const STATUSES = [
  "confirmed", "candidate", "stale", "conflicting",
  "invalid", "unsupported", "infrastructure",
];
const RELATION_KINDS = [
  "implements", "binds", "covers", "verifies",
  "evidences", "derived_from", "references", "supersedes",
];
const PROFILES = {
  "requirement-to-gate": { source: "gate", sink: "requirement" },
  "requirement-to-test": { source: "native_test", sink: "requirement" },
  "requirement-to-scenario": { source: "scenario", sink: "requirement" },
  "artifact-to-gate": { source: "gate", sink: "artifact" },
  "artifact-to-test": { source: "native_test", sink: "artifact" },
};
const IDENTITY_FIELDS = {
  requirement: "requirementId",
  symbol: "semanticId",
  artifact: "artifactId",
  scenario: "scenarioId",
  native_test: "testId",
  gate: "gateId",
  diagnostic: "diagnosticId",
};
// The explicit closed endpoint matrix (ADR-0014).
const ENDPOINTS = (kind, from, to) => {
  const same = from === to;
  switch (kind) {
    case "implements": return from === "symbol" && to === "requirement";
    case "binds": return from === "symbol" && to === "artifact";
    case "covers": return from === "scenario" && (to === "symbol" || to === "requirement");
    case "verifies": return from === "native_test" && (to === "scenario" || to === "symbol");
    case "evidences": return from === "gate" && (to === "native_test" || to === "scenario" || to === "symbol");
    case "derived_from": return same;
    case "supersedes": return same;
    case "references": return (
      (from === "gate" && to === "diagnostic") ||
      (from === "diagnostic" && to === "gate") ||
      (same && ["requirement", "symbol", "artifact", "scenario"].includes(from))
    );
    default: return false;
  }
};
const RELATION_DOMAIN = "lekalo/trace-manifest/v1.0.0/relation";
const relationIdOf = (kind, from, to, occurrence) =>
  `sha256:${sha256(JSON.stringify([RELATION_DOMAIN, kind, from, to, occurrence]))}`;

// Privacy-safe logical path grammar: a lexical port of the accepted
// structure contract rules (absolute, drive/scheme, backslash, percent
// escape, dot segments, empty/trailing segments, control bytes, DOS
// device names, short-name aliases, NFC).
const DOS_DEVICES = new Set([
  "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5",
  "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5",
  "lpt6", "lpt7", "lpt8", "lpt9", "conin$", "conout$", "clock$",
]);
const isNfc = (text) => text.normalize("NFC") === text;
const pathOk = (path) => {
  if (path.length === 0 || !isNfc(path)) return false;
  if (path.startsWith("/") || path.startsWith("\\\\") || path.startsWith("//")) return false;
  if (/^[A-Za-z][A-Za-z0-9+.-]*:/.test(path) || path.startsWith("~")) return false;
  if (path.includes("\\") || /%[0-9a-fA-F]{2}/.test(path) || path.includes(":")) return false;
  for (const segment of path.split("/")) {
    if (segment.length === 0 || segment.length > 64) return false;
    if (segment === "." || segment === ".." || segment.endsWith(".") || segment.endsWith(" ")) return false;
    for (const char of segment) {
      const code = char.codePointAt(0);
      if (code <= 0x1f || code === 0x7f) return false;
    }
    if (/~[0-9]/.test(segment)) return false;
    const normalized = segment.normalize("NFKC");
    if (DOS_DEVICES.has(normalized.split(".")[0].toLowerCase())) return false;
    if (char_upper(segment)) return false;
    if (!/^[a-z0-9][a-z0-9._-]*$/.test(segment) || segment.length > 64) return false;
  }
  return true;
};
const char_upper = (segment) => [...segment].some((c) => c >= "A" && c <= "Z");

// ---- 1. The golden instance must satisfy the closed wire schema. ----
const golden = read(goldenFile);
if (!validateTrace(golden)) fail("golden-schema-invalid", validateTrace.errors);

// ---- 2. Kind-specific identity exclusivity and grammars the schema
// cannot express. ----
const seenNodeIds = new Map();
const seenIdentities = new Map();
for (const node of golden.nodes) {
  const identityField = IDENTITY_FIELDS[node.nodeKind];
  if (node[identityField] === undefined) fail("golden-missing-identity", node);
  for (const [kind, field] of Object.entries(IDENTITY_FIELDS)) {
    if (kind !== node.nodeKind && node[field] !== undefined) {
      fail("golden-foreign-identity", node);
    }
  }
  if (seenNodeIds.has(node.nodeId)) fail("golden-duplicate-node-id", node.nodeId);
  seenNodeIds.set(node.nodeId, node);
  const identityKey = `${node.nodeKind}:${node[identityField]}`;
  if (seenIdentities.has(identityKey)) fail("golden-duplicate-identity", identityKey);
  seenIdentities.set(identityKey, node);
  if (node.nodeKind === "artifact") {
    if (!pathOk(node.path)) fail("golden-artifact-path", node.path);
  }
  for (const external of node.externalRefs ?? []) {
    if (!SYSTEMS.includes(external.system)) fail("golden-external-system", external);
  }
}

// ---- 3. Relations: endpoint resolution and direction, tuple identity,
// evidence resolution, occurrence safety. ----
const relationTuples = new Set();
for (const relation of golden.relations) {
  if (!RELATION_KINDS.includes(relation.relationKind)) fail("golden-relation-kind", relation);
  const from = seenNodeIds.get(relation.fromNode);
  const to = seenNodeIds.get(relation.toNode);
  if (!from || !to) fail("golden-dangling-endpoint", relation);
  if (!ENDPOINTS(relation.relationKind, from.nodeKind, to.nodeKind)) {
    fail("golden-endpoints-illegal", relation);
  }
  const expectedId = relationIdOf(
    relation.relationKind, relation.fromNode, relation.toNode, relation.occurrence,
  );
  if (relation.relationId !== expectedId) fail("golden-relation-id", relation);
  const tuple = `${relation.relationKind}|${relation.fromNode}|${relation.toNode}|${relation.occurrence}`;
  if (relationTuples.has(tuple)) fail("golden-duplicate-relation", tuple);
  relationTuples.add(tuple);
  if (!STATUSES.includes(relation.status)) fail("golden-status", relation);
  if (!CONFIDENCES.includes(relation.confidence)) fail("golden-confidence", relation);
  if (!ORIGINS.includes(relation.provenance.origin)) fail("golden-origin", relation);
  if (!SYSTEMS.includes(relation.provenance.sourceSystem)) fail("golden-source-system", relation);
  if (relation.provenance.sourceRevision !== golden.sourceRevision && relation.status === "confirmed") {
    fail("golden-confirmed-revision", relation);
  }
  if (relation.status === "confirmed" && (relation.provenance.origin === "inferred" ||
      !(relation.confidence === "exact" || relation.confidence === "high") ||
      relation.evidenceRefs.length === 0)) {
    fail("golden-confirmed-policy", relation);
  }
  for (const evidence of relation.evidenceRefs) {
    if (!seenNodeIds.has(evidence)) fail("golden-dangling-evidence", relation);
  }
}

// ---- 4. Completeness coherence and directed sink coverage. ----
const gaps = golden.gaps ?? [];
for (const gap of gaps) {
  if (gap.status === "confirmed") fail("golden-gap-confirmed", gap);
  if (gap.anchorNode !== undefined && !seenNodeIds.has(gap.anchorNode)) {
    fail("golden-gap-anchor", gap);
  }
  if (gap.sourcePath !== undefined && !pathOk(gap.sourcePath)) fail("golden-gap-path", gap);
}
if (gaps.length > 0 && golden.completeness === "full") fail("golden-full-with-gaps");
if (gaps.length === 0 && golden.completeness === "partial") fail("golden-partial-without-gap");
if (golden.completeness === "full" &&
    golden.relations.some((relation) => relation.status !== "confirmed")) {
  fail("golden-full-with-unconfirmed");
}
const profile = PROFILES[golden.exportProfile];
if (!profile) fail("golden-profile", golden.exportProfile);
const reached = new Set();
const queue = [];
golden.nodes.forEach((node, index) => {
  if (node.nodeKind === profile.source) { reached.add(node.nodeId); queue.push(node.nodeId); }
});
while (queue.length > 0) {
  const current = queue.pop();
  for (const relation of golden.relations) {
    if (relation.fromNode === current && !reached.has(relation.toNode)) {
      reached.add(relation.toNode);
      queue.push(relation.toNode);
    }
  }
}
const uncovered = golden.nodes
  .filter((node) => node.nodeKind === profile.sink && !reached.has(node.nodeId))
  .map((node) => node.nodeId);
if (golden.completeness === "full" && uncovered.length > 0) {
  fail("golden-full-with-uncovered-sink", uncovered);
}

// ---- 5. Canonical byte order: the second canonical implementation
// re-serializes with fixed contract key order and canonical sorts, and
// the bytes must be identical to the pinned golden. ----
const canonicalString = (value) => {
  if (Array.isArray(value)) return `[${value.map(canonicalString).join(",")}]`;
  if (value !== null && typeof value === "object") {
    const keys = canonicalKeyOrder(value);
    return `{${keys.map((key) => `${JSON.stringify(key)}:${canonicalString(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
};
const dropUndefined = (value) => {
  if (Array.isArray(value)) return value.map(dropUndefined);
  if (value !== null && typeof value === "object") {
    const out = {};
    for (const [key, member] of Object.entries(value)) {
      if (member !== undefined) out[key] = dropUndefined(member);
    }
    return out;
  }
  return value;
};
const canonicalKeyOrder = (value) => {
  // Fixed contract order per level, matching the Rust declaration order.
  const top = [
    "schemaVersion", "identity", "manifestId", "projectRef", "completeness",
    "sourceRevision", "modelRef", "irRef", "graphRef", "artifactManifestRef",
    "exportProfile", "nodes", "relations", "gaps",
  ];
  const node = [
    "nodeId", "nodeKind", "requirementId", "semanticId", "artifactId",
    "ownership", "path", "contentDigest", "revision", "generatorRef",
    "manifestDigest", "scenarioId", "testId", "gateId", "diagnosticId",
    "contractVersion", "evidenceDigest", "externalRefs",
  ];
  const relation = [
    "relationId", "relationKind", "fromNode", "toNode", "occurrence",
    "provenance", "confidence", "status", "evidenceRefs",
  ];
  const provenance = [
    "origin", "sourceSystem", "sourceRevision", "sourceDigest",
    "recordedBy", "sourcePath",
  ];
  const external = ["system", "originalId", "contractVersion", "revision", "digest"];
  const contractRef = ["schemaVersion", "digest"];
  const gap = ["gapKind", "status", "anchorNode", "expected", "sourcePath"];
  let order;
  if ("relations" in value && "nodes" in value) order = top;
  else if ("nodeKind" in value) order = node;
  else if ("relationKind" in value) order = relation;
  else if ("origin" in value) order = provenance;
  else if ("originalId" in value) order = external;
  else if ("gapKind" in value) order = gap;
  else if ("schemaVersion" in value) order = contractRef;
  else order = Object.keys(value).sort();
  return order.filter((key) => key in value);
};
const sortedGolden = cloneSorted(golden);
function cloneSorted(manifest) {
  const out = dropUndefined(JSON.parse(JSON.stringify(manifest)));
  out.nodes.sort((a, b) =>
    (NODE_KINDS.indexOf(a.nodeKind) - NODE_KINDS.indexOf(b.nodeKind)) ||
    byteCompare(a.nodeId, b.nodeId));
  for (const node of out.nodes) {
    (node.externalRefs ?? []).sort((a, b) =>
      byteCompare(a.system, b.system) || byteCompare(a.originalId, b.originalId));
  }
  out.relations.sort((a, b) =>
    (RELATION_KINDS.indexOf(a.relationKind) - RELATION_KINDS.indexOf(b.relationKind)) ||
    byteCompare(a.fromNode, b.fromNode) ||
    byteCompare(a.toNode, b.toNode) ||
    byteCompare(a.occurrence, b.occurrence));
  (out.gaps ?? []).sort((a, b) =>
    byteCompare(a.gapKind, b.gapKind) ||
    byteCompare(a.anchorNode ?? "", b.anchorNode ?? "") ||
    byteCompare(a.expected ?? "", b.expected ?? ""));
  return out;
}
function byteCompare(a, b) {
  return Buffer.compare(Buffer.from(a, "utf8"), Buffer.from(b, "utf8"));
}
const canonicalBytes = canonicalString(sortedGolden);
const goldenText = readText(goldenFile);
const goldenCanonical = goldenText.replace(/\r\n/g, "\n").replace(/\n$/, "");
if (goldenCanonical !== canonicalBytes) fail("golden-not-canonical", {
  expectedLength: canonicalBytes.length,
  goldenLength: goldenCanonical.length,
});

// ---- 6. The manifest digest must match the pinned sidecar. ----
const digest = `sha256:${sha256(goldenCanonical)}`;
const pinnedDigest = readText("tests/fixtures/trace/golden/planner.trace.sha256").trim();
if (digest !== pinnedDigest) fail("golden-digest", { digest, pinnedDigest });

// ---- 7. Determinism: permute nodes, relations, gaps, and set-like refs;
// permuted input; canonical order and the digest must be unchanged.
const permuted = JSON.parse(JSON.stringify(sortedGolden));
const reverse = (list) => list?.slice().reverse();
permuted.nodes = reverse(permuted.nodes);
permuted.relations = reverse(permuted.relations);
permuted.gaps = reverse(permuted.gaps);
for (const node of permuted.nodes) node.externalRefs = reverse(node.externalRefs);
for (const relation of permuted.relations) relation.evidenceRefs = reverse(relation.evidenceRefs);
const permutedCanonical = canonicalString(cloneSorted(permuted));
if (permutedCanonical !== canonicalBytes) fail("permutation-not-stable");
if (`sha256:${sha256(permutedCanonical)}` !== digest) fail("permutation-digest");

// ---- 8. Every invalid fixture must fail the schema or the semantics. ----
let invalidCount = 0;
for (const name of readdirSync(resolve(root, invalidDir)).sort()) {
  if (!name.endsWith(".json")) continue;
  const text = readText(`${invalidDir}/${name}`);
  // The Rust scanner rejects a leading BOM at the byte layer; the Node
  // gate must reject it identically instead of silently stripping it.
  let rejected = text.startsWith("﻿");
  const cleaned = text.replace(/^﻿/, "");
  let parsed = null;
  try {
    // Duplicate JSON keys: JSON.parse silently collapses them, so detect
    // with a reviving scanner over the raw text.
    const dupe = findDuplicateKey(cleaned);
    if (dupe) rejected = true;
    if (!rejected) {
      parsed = JSON.parse(cleaned);
      if (!validateTrace(parsed)) rejected = true;
    }
    if (!rejected && parsed) rejected = !conforms(parsed);
  } catch {
    rejected = true;
  }
  if (!rejected) fail(`${name}-accepted`, name);
  invalidCount += 1;
}

function findDuplicateKey(text) {
  // Minimal structural scan mirroring the Rust scanner: track key sets
  // per object depth.
  const stack = [new Set()];
  let inString = false;
  let escape = false;
  let expectKey = false;
  let keyStart = 0;
  for (let index = 0; index < text.length; index += 1) {
    const char = text[index];
    if (inString) {
      if (escape) { escape = false; continue; }
      if (char === "\\") { escape = true; continue; }
      if (char === '"') {
        inString = false;
        if (expectKey) {
          const key = text.slice(keyStart, index);
          const keys = stack[stack.length - 1];
          if (keys.has(key)) return key;
          keys.add(key);
          expectKey = false;
        }
      }
      continue;
    }
    if (char === '"') {
      inString = true;
      escape = false;
      keyStart = index + 1;
      // Heuristic: a string is a key iff the next non-ws char is ':' —
      // checked after the string closes; treat as key optimistically and
      // verify below.
      expectKey = isObjectKey(text, index);
      continue;
    }
    if (char === "{") { stack.push(new Set()); expectKey = false; continue; }
    if (char === "}") { stack.pop(); continue; }
  }
  return null;
}
function isObjectKey(text, quoteIndex) {
  for (let index = quoteIndex + 1; index < text.length; index += 1) {
    const char = text[index];
    if (char === "\\") { index += 1; continue; }
    if (char === '"') {
      for (let rest = index + 1; rest < text.length; rest += 1) {
        const next = text[rest];
        if (next === " " || next === "\t" || next === "\n" || next === "\r") continue;
        return next === ":";
      }
      return false;
    }
  }
  return false;
}
function conforms(manifest) {
  // The semantic layer, re-stated independently.
  try {
    const ids = new Map();
    const identities = new Set();
    for (const node of manifest.nodes) {
      const field = IDENTITY_FIELDS[node.nodeKind];
      if (!field || node[field] === undefined) return false;
      for (const [kind, key] of Object.entries(IDENTITY_FIELDS)) {
        if (kind !== node.nodeKind && node[key] !== undefined) return false;
      }
      if (ids.has(node.nodeId)) return false;
      ids.set(node.nodeId, node);
      const identityKey = `${node.nodeKind}:${node[field]}`;
      if (identities.has(identityKey)) return false;
      identities.add(identityKey);
      if (node.nodeKind === "artifact" && (node.artifactId === undefined ||
          node.ownership === undefined || node.path === undefined ||
          node.contentDigest === undefined || !pathOk(node.path))) return false;
    }
    const versionPin = (ref, allowed) =>
      ref === undefined || allowed.includes(ref.schemaVersion);
    if (!versionPin(manifest.modelRef, ["0.1.0", "1.0.0"])) return false;
    if (!versionPin(manifest.irRef, ["0.1.0"])) return false;
    if (!versionPin(manifest.graphRef, ["1.0.0"])) return false;
    const tuples = new Set();
    for (const relation of manifest.relations) {
      const from = ids.get(relation.fromNode);
      const to = ids.get(relation.toNode);
      if (!from || !to) return false;
      if (!ENDPOINTS(relation.relationKind, from.nodeKind, to.nodeKind)) return false;
      if (relation.relationId !== relationIdOf(
        relation.relationKind, relation.fromNode, relation.toNode, relation.occurrence,
      )) return false;
      const tuple = `${relation.relationKind}|${relation.fromNode}|${relation.toNode}|${relation.occurrence}`;
      if (tuples.has(tuple)) return false;
      tuples.add(tuple);
      if (!SYSTEMS.includes(relation.provenance.sourceSystem)) return false;
      if (!ORIGINS.includes(relation.provenance.origin)) return false;
      if (!STATUSES.includes(relation.status)) return false;
      if (!CONFIDENCES.includes(relation.confidence)) return false;
      for (const evidence of relation.evidenceRefs) {
        if (!ids.has(evidence)) return false;
      }
      if (relation.status === "confirmed" &&
          (relation.provenance.origin === "inferred" ||
           !(relation.confidence === "exact" || relation.confidence === "high") ||
           relation.evidenceRefs.length === 0 ||
           relation.provenance.sourceRevision !== manifest.sourceRevision)) {
        return false;
      }
    }
    for (const gap of manifest.gaps ?? []) {
      if (gap.status === "confirmed") return false;
      if (gap.anchorNode !== undefined && !ids.has(gap.anchorNode)) return false;
      if (gap.sourcePath !== undefined && !pathOk(gap.sourcePath)) return false;
    }
    if ((manifest.gaps ?? []).length > 0 && manifest.completeness === "full") return false;
    if ((manifest.gaps ?? []).length === 0 && manifest.completeness === "partial") return false;
    if (manifest.completeness === "full" &&
        manifest.relations.some((relation) => relation.status !== "confirmed")) return false;
    const profile = PROFILES[manifest.exportProfile];
    if (!profile) return false;
    const reached = new Set();
    const queue = [];
    for (const node of manifest.nodes) {
      if (node.nodeKind === profile.source) { reached.add(node.nodeId); queue.push(node.nodeId); }
    }
    while (queue.length > 0) {
      const current = queue.pop();
      for (const relation of manifest.relations) {
        if (relation.fromNode === current && !reached.has(relation.toNode)) {
          reached.add(relation.toNode);
          queue.push(relation.toNode);
        }
      }
    }
    if (manifest.completeness === "full" &&
        manifest.nodes.some((node) => node.nodeKind === profile.sink && !reached.has(node.nodeId))) {
      return false;
    }
    return true;
  } catch {
    return false;
  }
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  ajv: ajvVersion,
  golden: goldenFile,
  digest,
  nodes: golden.nodes.length,
  relations: golden.relations.length,
  gaps: (golden.gaps ?? []).length,
  invalidFixtures: invalidCount,
}, null, 2)}\n`);
