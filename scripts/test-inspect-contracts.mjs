#!/usr/bin/env node
// Issue #15 release gate: the inspect wire schema and the pinned
// canonical golden envelopes, validated with the same pinned third-party
// Draft 2020-12 implementation as the other contract gates. Exact
// Ajv 8.17.1 is provisioned outside this checkout (CI does the same on
// Node 18 and 24) and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.

import { createRequire } from "node:module";
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
    `ajv-unavailable: provision exact Ajv 8.17.1 outside the checkout and expose it through NODE_PATH / LEKALO_AJV_NODE_PATH (${error.message})\n`,
  );
  process.exit(2);
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(`ajv-version: expected 8.17.1, got ${ajvVersion}\n`);
  process.exit(2);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));

const schema = read("contracts/inspect.schema.v1.0.0.json");
const goldenDir = "tests/fixtures/inspect/golden";

const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateInspect = ajv.compile(schema);

const fail = (reason, detail) => {
  process.stderr.write(`inspect-contract ${reason}: ${JSON.stringify(detail ?? "")}\n`);
  process.exit(1);
};

// The closed v1 section order (ADR-0014): every golden envelope carries
// all mandatory sections in exactly this key order.
const SECTION_ORDER = [
  "schemaVersion", "identity", "modelVersion", "project", "selector", "symbol",
  "contract", "invariants", "policies", "effects", "dependencies", "dependents",
  "scenarios", "bindings", "ownership", "portability", "trace", "completeness",
  "diagnostics",
];
const DEFINITION_KINDS = new Set([
  "scalar", "enum", "value-object", "entity", "command", "query", "policy",
  "event", "effect", "endpoint", "scenario", "target-binding",
]);
const SECTION_STATES = new Set([
  "available", "empty", "unknown", "unsupported", "truncated", "error",
]);
const RELATIONS = new Set([
  "requires", "references", "accepts", "returns", "reads", "writes", "emits",
  "authorizes", "exposes", "implements", "verifies", "derived_from",
]);
const EFFECT_KINDS = new Set([
  "read", "create", "update", "delete", "write-field", "emit-event",
  "enqueue-job", "external-call", "cache-read", "cache-write",
  "cache-invalidate", "publish-output", "audit-log", "transaction-boundary",
]);
const CONFIDENCES = new Set(["canonical", "verified", "extracted", "inferred", "unknown"]);

let goldenCount = 0;
let sectionChecks = 0;
let itemChecks = 0;

for (const name of readdirSync(resolve(root, goldenDir)).sort()) {
  if (!name.endsWith(".json")) continue;
  const envelope = read(`${goldenDir}/${name}`);
  goldenCount += 1;

  // 1. The envelope is the accepted success wrapper and the payload
  // satisfies the closed wire schema.
  if (envelope.status !== "valid") fail(`${name}-envelope-status`, envelope.status);
  const inspect = envelope.inspect;
  if (!validateInspect(inspect)) fail(`${name}-invalid`, validateInspect.errors);

  // 2. Fixed wire order: top-level keys follow the schema order exactly.
  const keys = Object.keys(inspect);
  if (keys.length !== SECTION_ORDER.length) fail(`${name}-key-count`, keys.length);
  SECTION_ORDER.forEach((key, index) => {
    if (keys[index] !== key) fail(`${name}-section-order`, { at: index, got: keys[index] });
  });

  // 3. Identities and coherence of the header.
  if (inspect.schemaVersion !== "lekalo/inspect/v1.0.0") fail(`${name}-discriminator`);
  if (inspect.identity !== "dev.lekalo.inspect@1.0.0") fail(`${name}-identity`);
  if (inspect.selector.mode !== "short-name" && inspect.selector.input !== inspect.symbol.id) {
    fail(`${name}-selector-identity`, inspect.selector);
  }
  if (inspect.selector.mode === "short-name" && inspect.selector.resolvedId !== inspect.symbol.id) {
    fail(`${name}-short-resolution`, inspect.selector);
  }
  if (!DEFINITION_KINDS.has(inspect.symbol.kind)) fail(`${name}-kind`, inspect.symbol.kind);

  // 4. Every mandatory section carries a closed state and complete flag;
  // truncated sections carry bounds; unsupported sections carry reasons.
  for (const section of ["contract", "invariants", "policies", "effects", "dependencies",
    "dependents", "scenarios", "bindings", "ownership", "portability", "trace"]) {
    const body = inspect[section];
    if (!SECTION_STATES.has(body.state)) fail(`${name}-${section}-state`, body.state);
    sectionChecks += 1;
  }
  if (inspect.ownership.state !== "unsupported" || inspect.trace.state !== "unsupported") {
    fail(`${name}-optional-owner-states`, { ownership: inspect.ownership, trace: inspect.trace });
  }
  if (inspect.ownership.complete !== false || inspect.trace.complete !== false) {
    fail(`${name}-optional-owner-complete`);
  }

  // 5. Completeness coherence: partial in v1 (ownership/trace owners are
  // not accepted), with the fixed reasons; omitted items counted.
  if (inspect.completeness.state !== "partial") fail(`${name}-completeness`, inspect.completeness);
  const expectedReasons = [
    "ownership-unavailable", "trace-unavailable", "scenario-details-unavailable",
  ];
  for (const reason of expectedReasons) {
    if (!inspect.completeness.reasons.includes(reason)) fail(`${name}-missing-reason`, reason);
  }
  if (new Set(inspect.completeness.reasons).size !== inspect.completeness.reasons.length) {
    fail(`${name}-duplicate-reasons`);
  }
  let omittedTotal = 0;
  for (const section of ["contract", "invariants", "policies", "effects", "dependencies",
    "dependents", "scenarios", "bindings"]) {
    const bounds = inspect[section].bounds;
    if (bounds) {
      omittedTotal += bounds.omitted;
      if (bounds.omitted > 0 && inspect[section].state !== "truncated") {
        fail(`${name}-bounds-without-truncation`, section);
      }
      if (bounds.returned + bounds.omitted < bounds.limit && bounds.omitted > 0) {
        fail(`${name}-bound-arithmetic`, section);
      }
    }
  }
  if (omittedTotal !== inspect.completeness.omittedItems) {
    fail(`${name}-omitted-total`, { omittedTotal, recorded: inspect.completeness.omittedItems });
  }

  // 6. Set-like item arrays are sorted by unsigned UTF-8 bytes of their
  // ids and every item is internally closed.
  const idOf = (item) => item.id ?? item.bindingId;
  for (const section of ["policies", "scenarios"]) {
    const items = inspect[section].items ?? [];
    const ids = items.map(idOf);
    const sorted = [...ids].sort();
    if (JSON.stringify(ids) !== JSON.stringify(sorted)) fail(`${name}-${section}-order`, ids);
  }
  const bindingItems = inspect.bindings.items ?? [];
  const bindingIds = bindingItems.map((item) => item.bindingId);
  if (JSON.stringify(bindingIds) !== JSON.stringify([...bindingIds].sort())) {
    fail(`${name}-bindings-order`, bindingIds);
  }
  for (const binding of bindingItems) {
    if (binding.status !== "declared") fail(`${name}-binding-status`, binding);
    if (binding.confidence !== "canonical") fail(`${name}-binding-confidence`, binding);
    itemChecks += 1;
  }
  for (const relation of [...(inspect.dependencies.items ?? []), ...(inspect.dependents.items ?? [])]) {
    if (!RELATIONS.has(relation.relation)) fail(`${name}-relation`, relation);
    if (!CONFIDENCES.has(relation.confidence)) fail(`${name}-relation-confidence`, relation);
    if (relation.provenance.referenceRole !== undefined) {
      if (relation.provenance.type !== undefined) fail(`${name}-relation-provenance-shape`, relation);
    }
    itemChecks += 1;
  }
  for (const effect of inspect.effects.items ?? []) {
    if (!EFFECT_KINDS.has(effect.kind)) fail(`${name}-effect-kind`, effect);
    if (!CONFIDENCES.has(effect.confidence)) fail(`${name}-effect-confidence`, effect);
    if (effect.provenance.type === "canonical-ir" && effect.confidence !== "canonical") {
      fail(`${name}-declared-confidence`, effect);
    }
    if (effect.provenance.type === "evidence" && effect.confidence === "canonical") {
      fail(`${name}-detected-confidence`, effect);
    }
    if ((effect.kind === "create" || effect.kind === "delete") && effect.field !== null) {
      fail(`${name}-entity-wide-scope`, effect);
    }
    itemChecks += 1;
  }
}

if (goldenCount === 0) fail("no-goldens");

process.stdout.write(`${JSON.stringify({
  ok: true,
  ajv: ajvVersion,
  goldens: goldenCount,
  sections: sectionChecks,
  items: itemChecks,
}, null, 2)}\n`);
