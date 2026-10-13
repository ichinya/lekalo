#!/usr/bin/env node
// Issue #15 release gate: the inspect wire schema and the pinned
// canonical golden envelopes, validated with the same pinned third-party
// Draft 2020-12 implementation as the other contract gates. Exact
// Ajv 8.17.1 is provisioned outside this checkout (CI does the same on
// Node 18 and 24) and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.

import { createRequire } from "node:module";
import { cpSync, existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join, relative, resolve } from "node:path";
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

const schema = read("contracts/inspect.schema.v0.2.16.json");
// The 0.8.0 successor schema: the frozen predecessor stays in place
// byte-for-byte; live output now carries the successor discriminator
// and its modelVersion stays source-dependent.
const successorSchema = read("contracts/inspect.schema.v0.8.0.json");
if (successorSchema.properties.schemaVersion.const !== "lekalo/inspect/v0.8.0") {
  fail("successor-discriminator", String(successorSchema.properties.schemaVersion.const));
}
if (schema.properties.schemaVersion.const !== "lekalo/inspect/v0.2.16") {
  fail("predecessor-not-frozen", String(schema.properties.schemaVersion.const));
}
const goldenDir = "tests/fixtures/inspect/golden";

const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateInspect = ajv.compile(schema);
const validateInspectV8 = ajv.compile(successorSchema);

function fail(reason, detail) {
  process.stderr.write(`inspect-contract ${reason}: ${JSON.stringify(detail ?? "")}\n`);
  process.exit(1);
}

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
  if (inspect.schemaVersion !== "lekalo/inspect/v0.2.16") fail(`${name}-discriminator`);
  if (inspect.identity !== "dev.lekalo.inspect@0.2.16") fail(`${name}-identity`);
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

// Live successor probes: the real binary's ordinary inspect output is
// schema-valid under the published 0.8.0 successor, carries its
// discriminator, and — for Model 0.8.0 sources — reports the typed
// relation-cardinality and secondary-uniqueness facts in the
// invariants section. A legacy 0.2.16 source inspects valid under the
// SAME successor with its own source-dependent modelVersion (the
// negotiated identity, never a widened predecessor).
{
  const binaryCandidates = [
    join(root, "target/debug/lekalo"),
    join(root, "target/debug/lekalo.exe"),
  ];
  const binary = binaryCandidates.find((candidate) => existsSync(candidate));
  if (!binary) {
    fail("binary-missing", "build the workspace first: cargo build --workspace --locked");
  } else {
    const liveInspect = (symbol, project) => {
      const run = spawnSync(
        binary,
        ["--no-cache", "--json", "inspect", symbol, "--project", project],
        { cwd: root, encoding: "utf8", timeout: 120000 },
      );
      if (run.error || run.status !== 0) {
        fail("live-inspect-exit", `${symbol}: ${String(run.error ?? run.stderr).slice(0, 200)}`);
        return null;
      }
      return JSON.parse(run.stdout).inspect;
    };
    const tasklinks = liveInspect(
      "tasklinks.task_external_link",
      "tests/fixtures/task-external-links/project",
    );
    if (tasklinks) {
      if (!validateInspectV8(tasklinks)) {
        fail("live-successor-invalid", validateInspectV8.errors);
      }
      if (tasklinks.schemaVersion !== "lekalo/inspect/v0.8.0") fail("live-discriminator", tasklinks.schemaVersion);
      if (tasklinks.modelVersion !== "0.8.0") fail("live-model-version", tasklinks.modelVersion);
      if (tasklinks.project?.irRef?.identity !== "dev.lekalo.ir@0.8.0") {
        fail("live-ir-identity", tasklinks.project?.irRef?.identity);
      }
      const invariants = tasklinks.invariants.items ?? [];
      const relation = invariants.find((item) => item.type === "relation");
      const constraint = invariants.find((item) => item.type === "unique-constraint");
      if (!relation || relation.relation !== "tasklinks.rel.task_links"
        || relation.target !== "planner.task" || relation.min !== 1 || relation.max !== 1
        || relation.onDetach !== "retain-history" || relation.onDelete !== "restrict") {
        fail("live-relation-invariant", JSON.stringify(relation));
      }
      if (!constraint || constraint.constraint !== "tasklinks.uq_external_identity"
        || constraint.fields.length !== 5 || constraint.comparison !== "exact-text"
        || constraint.normalizationRef !== null) {
        fail("live-unique-invariant", JSON.stringify(constraint));
      }
      if (tasklinks.invariants.state !== "available" || tasklinks.invariants.complete !== true) {
        fail("live-invariants-state", tasklinks.invariants.state);
      }
      const contradictory = structuredClone(tasklinks);
      contradictory.invariants.items.find((item) => item.type === "unique-constraint").normalizationRef = "planner.task";
      if (validateInspectV8(contradictory)) {
        fail("successor-exact-text-normalization-accepted");
      }
      contradictory.invariants.items.find((item) => item.type === "unique-constraint").comparison = "declared-normalization";
      contradictory.invariants.items.find((item) => item.type === "unique-constraint").normalizationRef = null;
      if (validateInspectV8(contradictory)) {
        fail("successor-unpinned-normalization-accepted");
      }
    }
    const scratch = mkdtempSync(join(root, "target/inspect-contract-"));
    try {
      const project = relative(root, join(scratch, "many-invariants")).replaceAll("\\", "/");
      cpSync(join(root, "tests/fixtures/task-external-links/project"), project, { recursive: true });
      const source = join(project, "lekalo/modules/tasklinks/entities.yaml");
      const before = readFileSync(source, "utf8");
      const pattern = /(      - id: "tasklinks\.rel\.task_links"[\s\S]*?)(?=    uniqueConstraints:)/;
      const match = before.match(pattern);
      if (!match) fail("invariant-bound-probe-no-op");
      // Stay within the Model's 64-relation bound; every new fact
      // must be present, not silently replaced by primary identity.
      const additions = Array.from({ length: 62 }, (_, index) =>
        match[1].replace("tasklinks.rel.task_links", `tasklinks.rel.extra_${String(index).padStart(3, "0")}`)).join("");
      writeFileSync(source, before.replace(pattern, match[1] + additions));
      const bounded = liveInspect("tasklinks.task_external_link", project);
      if (!validateInspectV8(bounded)) fail("bounded-successor-invalid", validateInspectV8.errors);
      if (bounded.invariants.state !== "available" || bounded.invariants.complete !== true
        || bounded.invariants.items.length !== 65 || bounded.invariants.bounds !== undefined) {
        fail("invariant-facts-missing", bounded.invariants);
      }
    } finally {
      rmSync(scratch, { recursive: true, force: true });
    }
    const legacy = liveInspect("planner.task", "tests/fixtures/inspect/planner");
    if (legacy) {
      if (!validateInspectV8(legacy)) {
        fail("live-legacy-successor-invalid", validateInspectV8.errors);
      }
      if (legacy.modelVersion !== "0.2.16") fail("live-legacy-model-version", legacy.modelVersion);
      if (legacy.project?.irRef?.identity && legacy.project.irRef.identity !== "dev.lekalo.ir@0.2.16") {
        fail("live-legacy-ir-identity", legacy.project?.irRef?.identity);
      }
      // A 0.2.16 source declares no successor invariants: the section
      // stays closed and the typed facts stay absent, never invented.
      if ((legacy.invariants.items ?? []).some((item) => item.type !== "identity")) {
        fail("live-legacy-invariant-invented", JSON.stringify(legacy.invariants.items));
      }
    }
  }
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  ajv: ajvVersion,
  goldens: goldenCount,
  sections: sectionChecks,
  items: itemChecks,
  liveSuccessor: true,
}, null, 2)}\n`);
