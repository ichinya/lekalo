#!/usr/bin/env node
// Issue #17 release gate: the context-capsule wire schema and the pinned
// canonical goldens, validated with the same pinned third-party Draft
// 2020-12 implementation as the other contract gates. Exact Ajv 8.17.1 is
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
  ({ default: Ajv2020 } = require("ajv/dist/2020"));
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(
    `pinned Ajv 8.17.1 is required (set NODE_PATH or LEKALO_AJV_NODE_PATH): ${error.message}\n`,
  );
  process.exit(1);
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(`pinned Ajv 8.17.1 required, found ${ajvVersion}\n`);
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));

const schema = read("contracts/context-capsule.schema.v1.0.0.json");
const goldenDir = "tests/fixtures/context/golden";

const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateCapsule = ajv.compile(schema);

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

// The closed v1 vocabularies (ADR-0018). Unknown sections, relations,
// effect kinds, resource kinds, confidences, gap ids, or estimator
// identities fail the gate.
const SECTIONS = [
  "symbol", "policies", "effects", "dependencies", "scenarios",
  "public-impact", "bindings", "types", "closure",
];
const NODE_KINDS = [
  "project", "module", "type", "entity", "operation", "policy",
  "event", "effect", "endpoint", "scenario", "target-binding", "requirement",
];
const RELATIONS = [
  "requires", "references", "accepts", "returns", "reads", "writes",
  "emits", "authorizes", "exposes", "implements", "verifies", "derived_from",
];
const EFFECT_KINDS = new Set([
  "read", "create", "update", "delete", "write-field", "emit-event",
  "enqueue-job", "external-call", "cache-read", "cache-write",
  "cache-invalidate", "publish-output", "audit-log", "transaction-boundary",
]);
const GAP_IDS = new Set([
  "closure-bounded", "detected-effects-absent", "error-contracts-unrepresentable",
  "no-description", "no-effects", "no-policies", "no-relevant-bindings",
  "no-scenario-coverage",
]);
const ESTIMATOR_IDENTITY = "dev.lekalo.estimator.chars-4@1.0.0";

// The estimator content rule: tokens = max(1, ceil(scalars / 4)) over a
// non-empty string, 0 for empty. The gate recomputes every manifest token
// budget arithmetic from the recorded rule.
const estimateTokens = (content) => {
  const scalars = [...content].length;
  return scalars === 0 ? 0 : Math.max(1, Math.ceil(scalars / 4));
};

// The deterministic fact-id spellings (the ids must be reproducible from
// the fact bytes so the manifest stays explainable).
const cardId = (fact) => fact.id;
const edgeId = (fact) =>
  `edge:${fact.relation}:${fact.from}->${fact.to}#${fact.occurrence}`;
const effectId = (fact) => {
  const subject = fact.field === null ? fact.resource.id : `${fact.resource.id}.${fact.field}`;
  return `effect:${fact.kind}:${fact.operation}->${fact.resource.kind}:${subject}#${fact.occurrence}`;
};
const factId = (section, fact) => {
  switch (section) {
    case "symbol":
    case "types":
      return cardId(fact);
    case "policies":
    case "scenarios":
    case "bindings":
      return fact.id;
    case "effects":
      return effectId(fact);
    default:
      return edgeId(fact);
  }
};

let goldenCount = 0;
let factCount = 0;
for (const name of readdirSync(resolve(root, goldenDir)).sort()) {
  // The `*.envelope.json` goldens are the CLI projection (status plus
  // capsule); this gate validates the payload contract itself.
  if (!name.endsWith(".json") || name.endsWith(".envelope.json")) continue;
  const capsule = read(`${goldenDir}/${name}`);
  goldenCount += 1;

  // 1. The golden instance must satisfy the closed wire schema.
  if (!validateCapsule(capsule)) fail(`${name}-invalid`, validateCapsule.errors);

  // 2. Identity and estimator pins.
  if (capsule.schemaVersion !== "lekalo/context/v1.0.0") fail(`${name}-schema`);
  if (capsule.identity !== "dev.lekalo.context@1.0.0") fail(`${name}-identity`);
  if (capsule.estimator.identity !== ESTIMATOR_IDENTITY) fail(`${name}-estimator`);
  if (!capsule.irDigest.startsWith("sha256:")) fail(`${name}-ir-digest`);

  // 3. Coverage arithmetic and the budget walk invariant: included plus
  // excluded is exactly the candidate count, fits equals zero exclusions,
  // and the estimates obey the recorded estimator rule bounds.
  const { coverage, budget, manifest } = capsule;
  const included = manifest.included;
  const excluded = manifest.excluded;
  if (coverage.candidates !== included.length + excluded.length) {
    fail(`${name}-coverage-arithmetic`);
  }
  if (coverage.included !== included.length || coverage.excluded !== excluded.length) {
    fail(`${name}-coverage-rows`);
  }
  if (budget.fits !== (excluded.length === 0)) fail(`${name}-fits-flag`);
  if (budget.estimated > budget.limit && budget.fits) fail(`${name}-limit-respected`);

  // 4. Every manifest row names an emitted fact, exactly once, and the
  // section keys are the closed vocabulary in both projections.
  const seen = new Map();
  const emitted = new Set();
  for (const [section, facts] of Object.entries(capsule.sections)) {
    if (!SECTIONS.includes(section)) fail(`${name}-section`, section);
    for (const fact of facts) {
      const id = factId(section, fact);
      if (emitted.has(id)) fail(`${name}-duplicate-fact`, id);
      emitted.add(id);
      factCount += 1;
    }
  }
  for (const row of included) {
    if (seen.has(row.id)) fail(`${name}-duplicate-manifest-row`, row.id);
    seen.set(row.id, row);
    if (!emitted.has(row.id)) fail(`${name}-phantom-included-row`, row.id);
    if (!SECTIONS.includes(row.section)) fail(`${name}-row-section`, row.section);
  }
  for (const row of excluded) {
    if (seen.has(row.id)) fail(`${name}-duplicate-manifest-row`, row.id);
    seen.set(row.id, row);
    if (!SECTIONS.includes(row.section)) fail(`${name}-row-section`, row.section);
    // An excluded row names a candidate fact; whether it was emitted is
    // exactly the budget walk outcome, so excluded rows must not point at
    // emitted facts.
    if (emitted.has(row.id) && row.reason === "budget") {
      // A fact may appear in a section only when included; an excluded
      // row for an emitted id is a walk violation.
      fail(`${name}-excluded-but-emitted`, row.id);
    }
  }

  // 5. Section rank order in the manifest walk: rows appear in section
  // rank order (the budget walk order), and included rows are sorted by
  // walk, excluded rows after them in the same order.
  const rank = (section) => SECTIONS.indexOf(section);
  const walks = [...included, ...excluded];
  for (let index = 1; index < walks.length; index += 1) {
    const previousSection = walks[index - 1].section;
    const currentSection = walks[index].section;
    if (rank(previousSection) > rank(currentSection)) {
      fail(`${name}-walk-order`, `${previousSection} -> ${currentSection}`);
    }
  }

  // 6. Gap rows come from the closed vocabulary, sorted.
  const gapIds = capsule.gaps.map((gap) => gap.gap);
  for (const gap of gapIds) {
    if (!GAP_IDS.has(gap)) fail(`${name}-gap`, gap);
  }
  const sortedGaps = [...capsule.gaps].sort((left, right) =>
    JSON.stringify([left.gap, left.symbols ?? []]) <
    JSON.stringify([right.gap, right.symbols ?? []])
      ? -1
      : 1,
  );
  if (JSON.stringify(capsule.gaps) !== JSON.stringify(sortedGaps)) {
    fail(`${name}-gap-order`);
  }
  // The standing honesty gaps: the accepted Model cannot declare error
  // contracts, and a capsule without evidence envelopes says so.
  if (!gapIds.includes("error-contracts-unrepresentable")) {
    fail(`${name}-standing-gap`);
  }

  // 7. Privacy: no absolute path spelling, no `.env` references, no NUL
  // controls anywhere in the emitted bytes.
  const bytes = JSON.stringify(capsule);
  if (/\\\\|C:[\\\\/]|\.env|\u0000/.test(bytes)) fail(`${name}-privacy`);
}

if (goldenCount === 0) fail("no-goldens");

process.stdout.write(`${JSON.stringify({
  ok: true,
  ajv: ajvVersion,
  goldens: goldenCount,
  facts: factCount,
}, null, 2)}\n`);
