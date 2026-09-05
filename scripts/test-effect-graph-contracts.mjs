#!/usr/bin/env node
// Issue #14 release gate: the effect-graph wire schema and the pinned
// canonical golden export, validated with the same pinned third-party
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

const schema = read("contracts/effect-graph.schema.v1.0.0.json");
const goldenDir = "tests/fixtures/effects/golden";

const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateEffects = ajv.compile(schema);

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

// The closed v1 effect vocabulary (ADR-0013). The wire keys are registry
// data; unknown kinds, resource kinds, actions, confidences, or trust
// states fail the gate.
const RESOURCE_KINDS = [
  "canonical", "target-resource", "external-service", "cache",
  "event", "job", "output", "audit",
];
const EFFECT_KINDS = [
  "read", "create", "update", "delete", "write-field", "emit-event",
  "enqueue-job", "external-call", "cache-read", "cache-write",
  "cache-invalidate", "publish-output", "audit-log", "transaction-boundary",
];
const WRITE_ACTIONS = ["set", "clear", "append", "replace", "merge"];
const CONFIDENCES = ["canonical", "verified", "extracted", "inferred", "unknown"];
const DECLARED_ROLES = ["query-reads", "command-effect", "effect-emits"];
const TRUST_STATES = [
  "verified", "current", "extracted", "inferred", "stale", "unknown", "rejected",
];

// Declared (canonical-ir) edges always carry canonical confidence, and
// only they may: detected evidence is at best verified/extracted.
const canonicalEdgeKey = (effect) => [
  effect.operation,
  EFFECT_KINDS.indexOf(effect.kind),
  effect.action === null ? -1 : WRITE_ACTIONS.indexOf(effect.action),
  RESOURCE_KINDS.indexOf(effect.resource.kind),
  effect.resource.id,
  effect.field ?? "",
  effect.effect,
  effect.occurrence,
];

let goldenCount = 0;
let effectCount = 0;
for (const name of readdirSync(resolve(root, goldenDir)).sort()) {
  if (!name.endsWith(".json")) continue;
  const graph = read(`${goldenDir}/${name}`);
  goldenCount += 1;

  // 1. The golden instance must satisfy the closed wire schema.
  if (!validateEffects(graph)) fail(`${name}-invalid`, validateEffects.errors);

  // 2. Counts and identities must be coherent.
  const { metadata } = graph;
  if (metadata.effectCount !== graph.effects.length) fail(`${name}-effect-count`);
  if (metadata.declaredCount + metadata.detectedCount !== metadata.effectCount) {
    fail(`${name}-sector-counts`);
  }
  const keys = new Set(graph.effects.map(canonicalEdgeKey));
  if (keys.size !== graph.effects.length) fail(`${name}-duplicate-effect-keys`);

  // 3. Edges must be in canonical order (byte sort of the canonical key).
  const sorted = [...graph.effects].sort((left, right) => {
    const a = JSON.stringify(canonicalEdgeKey(left));
    const b = JSON.stringify(canonicalEdgeKey(right));
    return a < b ? -1 : 1;
  });
  if (JSON.stringify(graph.effects) !== JSON.stringify(sorted)) fail(`${name}-effect-order`);

  for (const effect of graph.effects) {
    // 4. Kind/action and kind/scope legality on the wire.
    if ((effect.kind === "write-field") !== (effect.action !== null)) {
      fail(`${name}-action-legality`, effect);
    }
    if (effect.kind !== "write-field" && effect.action !== null) {
      fail(`${name}-stray-action`, effect);
    }
    if (effect.kind === "transaction-boundary" && effect.resource.kind !== "canonical") {
      fail(`${name}-boundary-subject`, effect);
    }
    // 5. Provenance/confidence coupling: declared edges are canonical,
    // detected edges never claim canonical confidence.
    if (effect.provenance.type === "canonical-ir") {
      if (effect.confidence !== "canonical") fail(`${name}-declared-confidence`, effect);
      if (!DECLARED_ROLES.includes(effect.provenance.role)) {
        fail(`${name}-declared-role`, effect);
      }
    } else {
      if (effect.confidence === "canonical") fail(`${name}-detected-confidence`, effect);
      if (!TRUST_STATES.includes(effect.provenance.trust)) {
        fail(`${name}-detected-trust`, effect);
      }
    }
    if (!CONFIDENCES.includes(effect.confidence)) fail(`${name}-confidence`, effect);
    // 6. Field-scoped kinds: write-field always names its field; create
    // and delete are entity-wide on the wire.
    if (effect.kind === "write-field" && effect.field === null) {
      fail(`${name}-missing-field`, effect);
    }
    if ((effect.kind === "create" || effect.kind === "delete") && effect.field !== null) {
      fail(`${name}-unexpected-field`, effect);
    }
    effectCount += 1;
  }
}

if (goldenCount === 0) fail("no-goldens");

process.stdout.write(`${JSON.stringify({
  ok: true,
  ajv: ajvVersion,
  goldens: goldenCount,
  effects: effectCount,
}, null, 2)}\n`);
