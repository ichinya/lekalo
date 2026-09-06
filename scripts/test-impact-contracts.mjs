#!/usr/bin/env node
// Issue #16 release gate: the impact wire schema and the pinned canonical
// golden export, validated with the same pinned third-party Draft 2020-12
// implementation as the other contract gates. Exact Ajv 8.17.1 is
// provisioned outside this checkout (CI does the same on Node 18 and 24)
// and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.

import { createRequire } from "node:module";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  const specifier = process.env.LEKALO_AJV_SPECIFIER ?? "ajv/dist/2020";
  Ajv2020 = require(specifier).default;
  ajvVersion = require("ajv/package.json").version;
} catch {
  Ajv2020 = undefined;
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(
    `${JSON.stringify(
      {
        ok: false,
        reason: "ajv-version",
        detail: `expected 8.17.1, found ${ajvVersion}`,
      },
      null,
      2,
    )}\n`,
  );
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));

const schema = read("contracts/impact.schema.v1.0.0.json");
const goldenDir = "tests/fixtures/impact/golden";

const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateImpact = ajv.compile(schema);

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

// The closed v1 impact vocabularies (ADR-0017). Unknown dimensions, gate
// states, scopes, surfaces, or relations fail the gate.
const NODE_KINDS = [
  "project", "module", "type", "entity", "operation", "policy", "event",
  "effect", "endpoint", "scenario", "target-binding", "requirement",
];
const digestOf = (impact) => {
  const canonical = { ...impact, digest: "" };
  return `sha256:${createHash("sha256").update(JSON.stringify(canonical)).digest("hex")}`;
};
const RELATION_KINDS = [
  "requires", "references", "accepts", "returns", "reads", "writes", "emits",
  "authorizes", "exposes", "implements", "verifies", "derived_from",
];
const RISK_DIMENSIONS = [
  "public_contract", "migration_data", "transaction", "authorization",
  "portability", "effects", "bindings_artifacts", "scenarios_tests",
];
const GATE_STATES = ["selected", "not-required", "unknown", "blocked"];
const SECTION_STATES = [
  "complete", "incomplete", "stale", "unknown", "unsupported", "conflicting",
];
const SCOPES = ["direct", "transitive", "mandatory-public"];
const SURFACES = [
  "graph", "effects", "changed-inputs", "detected-evidence", "manifests",
  "scenarios", "tests",
];
const SECTION_KEYS = [
  "state", "complete", "returned", "omitted", "frontier", "reasonRefs",
  "provenance", "confidence",
];
const ITEM_KEYS = [
  "subject", "scope", "distance", "reasonRefs", "pathRefs", "evidenceState",
  "confidence", "riskRefs", "requiredGateRefs",
];

const assertSectionShape = (name, section) => {
  for (const key of SECTION_KEYS) {
    if (!(key in section)) fail(`${name}-missing-${key}`);
  }
  if (!SECTION_STATES.includes(section.state)) fail(`${name}-state`);
  if (typeof section.complete !== "boolean") fail(`${name}-complete-flag`);
};

const isSorted = (values) =>
  values.every((value, index) => index === 0 || values[index - 1] <= value);

let goldenCount = 0;
let itemCount = 0;
const kindRank = (ref) => {
  const kind = ref.slice(0, ref.indexOf(":"));
  const rank = NODE_KINDS.indexOf(kind);
  if (rank < 0) fail("unknown-node-kind", ref);
  return rank;
};
const nodeKey = (ref) => [kindRank(ref), ref];
const isSortedByNode = (refs) =>
  refs.every((value, index) => {
    if (index === 0) return true;
    const previous = nodeKey(refs[index - 1]);
    const current = nodeKey(value);
    return previous[0] < current[0] ||
      (previous[0] === current[0] && previous[1] <= current[1]);
  });

for (const name of readdirSync(resolve(root, goldenDir)).sort()) {
  if (!name.endsWith(".json")) continue;
  const raw = readFileSync(resolve(root, goldenDir, name), "utf8");
  const impact = JSON.parse(raw);
  goldenCount += 1;

  // 1. The golden instance must satisfy the closed wire schema.
  if (!validateImpact(impact)) fail(`${name}-invalid`, validateImpact.errors);

  // 2. The digest must be exactly reproducible from the canonical bytes.
  if (digestOf(impact) !== impact.digest) fail(`${name}-digest`);

  // 3. Object keys must follow the frozen schema order.
  const keys = Object.keys(impact);
  const expected = [
    "schemaVersion", "identity", "algorithm", "modelVersion", "project",
    "input", "request", "roots", "direct", "transitive", "mandatoryPublic",
    "risks", "targets", "artifacts", "scenarios", "tests", "gates",
    "explanations", "evidence", "completeness", "diagnosticRefs", "digest",
  ];
  if (JSON.stringify(keys) !== JSON.stringify(expected)) fail(`${name}-key-order`);

  // 4. Roots are kind-qualified and sorted.
  for (const root of impact.roots) {
  }
  if (!isSortedByNode(impact.roots)) fail(`${name}-roots-order`);

  // 5. Radius sections: shape, scope/distance coherence, subject order.
  for (const sectionName of ["direct", "transitive", "mandatoryPublic"]) {
    assertSectionShape(sectionName, impact[sectionName]);
    const subjects = [];
    for (const item of impact[sectionName].items ?? []) {
      for (const key of ITEM_KEYS) {
        if (!(key in item)) fail(`${name}-${sectionName}-missing-${key}`);
      }
      if (!SCOPES.includes(item.scope)) fail(`${name}-scope`, item.scope);
      if (sectionName === "direct" && item.scope !== "direct") {
        fail(`${name}-wrong-scope`, item.subject);
      }
      if (sectionName === "transitive" && item.scope !== "transitive") {
        fail(`${name}-wrong-scope`, item.subject);
      }
      if (sectionName === "mandatoryPublic" && item.scope !== "mandatory-public") {
        fail(`${name}-wrong-scope`, item.subject);
      }
      if (item.distance < 1) fail(`${name}-distance`, item.subject);
      if (item.pathRefs.length === 0) fail(`${name}-unexplained`, item.subject);
      subjects.push(item.subject);
      itemCount += 1;
    }
    if (!isSortedByNode(subjects)) fail(`${name}-${sectionName}-order`);
  }

  // 6. Risks: closed dimensions only, sorted, never optimistic.
  const riskDimensions = (impact.risks.items ?? []).map((risk) => risk.dimension);
  if (!isSorted(riskDimensions)) fail(`${name}-risk-order`);
  for (const risk of impact.risks.items ?? []) {
    if (!RISK_DIMENSIONS.includes(risk.dimension)) fail(`${name}-risk-dimension`);
    if (!SECTION_STATES.includes(risk.state)) fail(`${name}-risk-state`);
    if (risk.state === "unknown" && risk.confidence !== "unknown") {
      fail(`${name}-risk-unknown-confidence`, risk.dimension);
    }
  }

  // 7. Gates: one row per dimension plus semantic validation; states are
  // closed; required gates never claim more evidence than exists.
  const gateIds = (impact.gates.items ?? []).map((gate) => gate.gateId);
  if (!isSorted(gateIds)) fail(`${name}-gate-order`);
  if (!gateIds.includes("impact.gate.semantic-validate")) fail(`${name}-validation-gate`);
  for (const gate of impact.gates.items ?? []) {
    if (!GATE_STATES.includes(gate.state)) fail(`${name}-gate-state`);
    if (!gate.gateId.startsWith("impact.gate.")) fail(`${name}-gate-id`);
  }

  // 8. Explanations: every referenced path id resolves.
  const pathIds = new Set((impact.explanations ?? []).map((path) => path.pathId));
  for (const sectionName of ["direct", "transitive", "mandatoryPublic"]) {
    for (const item of impact[sectionName].items ?? []) {
      for (const ref of item.pathRefs) {
        if (!pathIds.has(ref)) fail(`${name}-dangling-path-ref`, ref);
      }
    }
  }
  for (const path of impact.explanations ?? []) {
    if (path.relationKinds.some((relation) => !RELATION_KINDS.includes(relation))) {
      fail(`${name}-path-relation`, path.pathId);
    }
    if (path.orderedEdges.length === 0) fail(`${name}-empty-path`);
    if (path.orderedEdges.length + 1 < path.relationKinds.length) {
      fail(`${name}-relations-exceed-edges`);
    }
  }

  // 9. Evidence surfaces: closed vocabulary, sorted.
  const surfaces = (impact.evidence.items ?? []).map((surface) => surface.surface);
  if (!isSorted(surfaces)) fail(`${name}-evidence-order`);
  for (const surface of impact.evidence.items ?? []) {
    if (!SURFACES.includes(surface.surface)) fail(`${name}-surface`);
  }

  // 10. Completeness is consistent with the section facts.
  if (!SECTION_STATES.includes(impact.completeness.state)) fail(`${name}-completeness-state`);
  if (impact.completeness.complete && impact.completeness.state !== "complete") {
    fail(`${name}-completeness-coherence`);
  }
}

if (goldenCount === 0) fail("no-goldens");

process.stdout.write(
  `${JSON.stringify(
    {
      ok: true,
      ajv: ajvVersion,
      goldens: goldenCount,
      items: itemCount,
    },
    null,
    2,
  )}\n`,
);
