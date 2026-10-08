#!/usr/bin/env node
// Issue #136: deterministic authoring of the ten closed adoption contract
// schemas at the 0.8.0 product generation. Committed bytes are the only
// accepted artifacts; `--check` regenerates in memory and compares without
// writing, so the schemas can never drift from their declared shape.
//
//   node scripts/gen-adoption-contracts.mjs           # rewrite the schemas
//   node scripts/gen-adoption-contracts.mjs --check   # verify only
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const check = process.argv.slice(2).includes("--check");

const V = "0.8.0";
const sha256 = { type: "string", pattern: "^sha256:[0-9a-f]{64}$" };
const digestOrNull = { type: ["string", "null"], pattern: "^sha256:[0-9a-f]{64}$" };
const semanticId = {
  type: "string",
  minLength: 1,
  maxLength: 128,
  pattern: "^[a-z][a-z0-9_]*(\\.[a-z0-9_]+)*$",
};
const moduleId = { type: "string", pattern: "^[a-z][a-z0-9_]{0,62}$" };
const relPath = {
  allOf: [
    {
      type: "string",
      minLength: 1,
      maxLength: 1024,
      pattern: "^[a-z0-9.][a-z0-9._-]*(/[a-z0-9.][a-z0-9._-]*)*$",
    },
    {
      // Dotted runtime homes are admitted; `.` and `..` segments never are.
      not: { type: "string", pattern: "(^|/)\\.\\.?(/|$)" },
    },
  ],
};
const nativeId = {
  type: "string",
  minLength: 1,
  maxLength: 128,
  pattern: "^[^\\u0000-\\u001f\\u007f]+$",
};
const confidence = {
  enum: ["exact", "high", "medium", "low", "unknown"],
};
const target = { type: "string", minLength: 1, maxLength: 63, pattern: "^[a-z][a-z0-9_-]{0,62}$" };
const profile = { type: "string", minLength: 1, maxLength: 64, pattern: "^[a-z][a-z0-9-]{0,63}$" };
const adapter = {
  type: "object",
  additionalProperties: false,
  required: ["id", "version", "digest"],
  properties: {
    id: { type: "string", minLength: 1, maxLength: 128, pattern: "^[a-z0-9][a-z0-9._-]*$" },
    version: { type: "string", pattern: "^[0-9]+\\.[0-9]+\\.[0-9]+$" },
    digest: digestOrNull,
  },
};
const adapterOrNull = { type: ["object", "null"], oneOf: [adapter, { type: "null" }] };
const scope = {
  type: "object",
  additionalProperties: false,
  required: ["kind", "id"],
  properties: {
    kind: { enum: ["symbol", "module", "use-case"] },
    id: { type: "string", minLength: 1, maxLength: 128, pattern: "^[a-z][a-z0-9_.-]*$" },
  },
};
const sourcePins = {
  type: "array",
  maxItems: 4096,
  items: {
    type: "object",
    additionalProperties: false,
    required: ["path", "sha256"],
    properties: { path: relPath, sha256 },
  },
};
const pins = {
  type: "object",
  additionalProperties: false,
  required: ["model", "lock", "observed_index", "adapter", "sources"],
  properties: {
    model: sha256,
    lock: digestOrNull,
    observed_index: digestOrNull,
    adapter: adapterOrNull,
    sources: sourcePins,
  },
};
const writeEntry = (deletable) => ({
  type: "object",
  additionalProperties: false,
  required: ["path", "action", "before", "after", "content"],
  properties: {
    path: relPath,
    action: deletable
      ? { enum: ["create", "replace", "delete", "binding"] }
      : { enum: ["create", "replace"] },
    before: digestOrNull,
    after: digestOrNull,
    content: { type: ["string", "null"], maxLength: 1048576 },
  },
});
const rollback = {
  type: "object",
  additionalProperties: false,
  required: ["journal", "snapshots"],
  properties: { journal: relPath, snapshots: relPath },
};
const token = { type: "string", minLength: 1, maxLength: 256 };
const gap = {
  type: "object",
  additionalProperties: false,
  required: ["subject", "detail"],
  properties: { subject: { type: "string", minLength: 1, maxLength: 128 }, detail: token },
};
const effectiveMode = {
  enum: ["observed", "contracted-legacy", "contracted-new", "unbound", "conflicting"],
};
const bindingRef = {
  type: ["object", "null"],
  oneOf: [
    {
      type: "object",
      additionalProperties: false,
      required: ["proposal", "candidate"],
      properties: { proposal: nativeId, candidate: nativeId },
    },
    { type: "null" },
  ],
};

const defs = { sha256, digestOrNull, semanticId, moduleId, relPath, nativeId, confidence, target, profile, adapter, adapterOrNull, scope, pins, sourcePins, writeEntry: writeEntry(false), rollback, token, gap, effectiveMode, bindingRef };

function schema(family, title, description, properties, required, extraDefs = {}) {
  return {
    $schema: "https://json-schema.org/draft/2020-12/schema",
    $id: `https://dev.lekalo/${family}.schema.v${V}.json`,
    title,
    description,
    type: "object",
    additionalProperties: false,
    unevaluatedProperties: false,
    required: ["schema_version", "identity", ...required],
    properties: {
      schema_version: { const: `lekalo/adoption-${family}/v${V}` },
      identity: { const: `dev.lekalo.adoption-${family}@${V}` },
      ...properties,
    },
    $defs: { ...defs, ...extraDefs },
  };
}

const base =
  "Issue #136 scoped brownfield adoption, selective refactoring, and hybrid mode. " +
  `Closed Draft 2020-12 contract at the 0.8.0 product generation; the exact discriminator is lekalo/adoption-{family}/v${V} and the schema identity is dev.lekalo.adoption-{family}@${V}. `;

const schemas = {
  "adoption-analysis": schema(
    "analysis",
    "Lekalo adoption analysis (v0.8.0)",
    base +
      "Derived, read-only local proposal for exactly one selected scope: exact source/model/lock/adapter/profile pins, the proposed module, definitions, bindings, and contract drafts with per-fact provenance and confidence, plus explicit unknown, ambiguous, discrepancy, and coverage rows. The analysis is never the canonical Model, never an ownership authority, and never a write permission; confirmation requires a separately reviewed plan.",
    {
      project: { $ref: "#/$defs/moduleId" },
      scope: { $ref: "#/$defs/scope" },
      target: { type: ["string", "null"], oneOf: [{ $ref: "#/$defs/target" }, { type: "null" }] },
      profile: { type: ["string", "null"], oneOf: [{ $ref: "#/$defs/profile" }, { type: "null" }] },
      pins: { $ref: "#/$defs/pins" },
      module: {
        type: "object",
        additionalProperties: false,
        required: ["id", "proposed", "native_root"],
        properties: {
          id: { $ref: "#/$defs/moduleId" },
          proposed: { type: "boolean" },
          native_root: { type: ["string", "null"], oneOf: [{ $ref: "#/$defs/relPath" }, { type: "null" }] },
        },
      },
      definitions: {
        type: "array",
        maxItems: 10000,
        items: {
          type: "object",
          additionalProperties: false,
          required: ["id", "kind", "status", "confidence", "provenance"],
          properties: {
            id: { $ref: "#/$defs/semanticId" },
            kind: { enum: ["command", "query", "value-object", "scalar", "event", "policy"] },
            status: { enum: ["proposed", "confirmed"] },
            confidence: { $ref: "#/$defs/confidence" },
            provenance: {
              type: "object",
              additionalProperties: false,
              required: ["source", "native", "path", "line"],
              properties: {
                source: { enum: ["observed-index", "authored"] },
                native: { type: ["string", "null"], oneOf: [{ $ref: "#/$defs/nativeId" }, { type: "null" }] },
                path: { type: ["string", "null"], oneOf: [{ $ref: "#/$defs/relPath" }, { type: "null" }] },
                line: { type: ["integer", "null"], minimum: 1, maximum: 1000000 },
              },
            },
          },
        },
      },
      bindings: {
        type: "array",
        maxItems: 10000,
        items: {
          type: "object",
          additionalProperties: false,
          required: ["proposal", "symbol", "native", "confidence", "state"],
          properties: {
            proposal: { $ref: "#/$defs/nativeId" },
            symbol: { $ref: "#/$defs/semanticId" },
            native: { $ref: "#/$defs/nativeId" },
            confidence: { $ref: "#/$defs/confidence" },
            state: { enum: ["explicit", "confirmed", "inferred", "stale"] },
          },
        },
      },
      contracts: {
        type: "array",
        maxItems: 10000,
        items: {
          type: "object",
          additionalProperties: false,
          required: ["symbol", "tests", "gates"],
          properties: {
            symbol: { $ref: "#/$defs/semanticId" },
            tests: { type: "array", maxItems: 64, items: { $ref: "#/$defs/nativeId" } },
            gates: { type: "array", maxItems: 64, items: { $ref: "#/$defs/nativeId" } },
          },
        },
      },
      unknowns: { type: "array", maxItems: 1000, items: { $ref: "#/$defs/gap" } },
      ambiguities: {
        type: "array",
        maxItems: 1000,
        items: {
          type: "object",
          additionalProperties: false,
          required: ["symbol", "candidates"],
          properties: {
            symbol: { $ref: "#/$defs/semanticId" },
            candidates: { type: "array", minItems: 2, maxItems: 64, items: { $ref: "#/$defs/nativeId" } },
          },
        },
      },
      discrepancies: {
        type: "array",
        maxItems: 1000,
        items: {
          type: "object",
          additionalProperties: false,
          required: ["symbol", "kind", "detail"],
          properties: {
            symbol: { $ref: "#/$defs/semanticId" },
            kind: { enum: ["binding", "definition", "evidence"] },
            detail: { $ref: "#/$defs/token" },
          },
        },
      },
    },
    ["project", "scope", "target", "profile", "pins", "module", "definitions", "bindings", "contracts", "unknowns", "ambiguities", "discrepancies"],
  ),

  "adoption-confirmation-plan": schema(
    "confirmation-plan",
    "Lekalo adoption confirmation plan (v0.8.0)",
    base +
      "The reviewable authorization boundary of one selective confirmation: the explicitly selected definition set plus at most one binding/candidate pair, the complete rendered canonical write set with expected before/after digests and exact new bytes, the exact re-fingerprinted input pins, unresolved dependencies, and the rollback home. Only the reviewed plan digest plus an admitted write scope authorize application; a binding confirmation alone never confirms a Model definition.",
    {
      plan: { $ref: "#/$defs/sha256" },
      project: { $ref: "#/$defs/moduleId" },
      analysis: { type: ["string", "null"], oneOf: [{ $ref: "#/$defs/sha256" }, { type: "null" }] },
      selections: {
        type: "object",
        additionalProperties: false,
        required: ["definitions", "binding"],
        properties: {
          definitions: { type: "array", minItems: 1, maxItems: 1000, items: { $ref: "#/$defs/semanticId" } },
          binding: { $ref: "#/$defs/bindingRef" },
        },
      },
      writes: { type: "array", maxItems: 4096, items: { $ref: "#/$defs/writeEntry" } },
      pins: { $ref: "#/$defs/pins" },
      unresolved: {
        type: "array",
        maxItems: 1000,
        items: {
          type: "object",
          additionalProperties: false,
          required: ["id", "reason"],
          properties: { id: { $ref: "#/$defs/semanticId" }, reason: { $ref: "#/$defs/token" } },
        },
      },
      rollback: { $ref: "#/$defs/rollback" },
    },
    ["plan", "project", "analysis", "selections", "writes", "pins", "unresolved", "rollback"],
  ),

  "adoption-patch": schema(
    "patch",
    "Lekalo adoption patch (v0.8.0)",
    base +
      "A closed, bounded, data-only candidate change set for maintained source files: the complete create/replace/delete set with exact relative paths, expected before/after digests, and exact new content. The patch never embeds executable tools or scripts, never carries commands, and its origin (a qualified adapter or a supplied reviewed patch) is recorded explicitly. Declining a preview writes nothing.",
    {
      patch: { $ref: "#/$defs/sha256" },
      origin: {
        type: "object",
        additionalProperties: false,
        required: ["kind", "identity"],
        properties: {
          kind: { enum: ["supplied-reviewed", "adapter-generated"] },
          identity: { $ref: "#/$defs/token" },
        },
      },
      scope: {
        type: "object",
        additionalProperties: false,
        required: ["symbol", "target", "profile"],
        properties: {
          symbol: { $ref: "#/$defs/semanticId" },
          target: { type: ["string", "null"], oneOf: [{ $ref: "#/$defs/target" }, { type: "null" }] },
          profile: { type: ["string", "null"], oneOf: [{ $ref: "#/$defs/profile" }, { type: "null" }] },
        },
      },
      files: { type: "array", minItems: 1, maxItems: 4096, items: { $ref: "#/$defs/writeEntryDeletable" } },
    },
    ["patch", "origin", "scope", "files"],
    { writeEntryDeletable: writeEntry(true) },
  ),

  "adoption-refactor-plan": schema(
    "refactor-plan",
    "Lekalo adoption refactor plan (v0.8.0)",
    base +
      "The exact, revision-bound selective refactor plan for one selected scope: every recommendation names its reason, files, risk, verification obligations, and uncertainty across the contract/API/effect/policy/test/binding discrepancy classes; unsupported transformations stay advice. The plan binds the optional candidate patch digest, the pinned verification specification digest, the read and mutation scopes, the preservation obligations, and the rollback inventory. A plan without a concrete patch is non-applicable advice and cannot apply.",
    {
      plan: { $ref: "#/$defs/sha256" },
      project: { $ref: "#/$defs/moduleId" },
      analysis: { $ref: "#/$defs/sha256" },
      selection: { $ref: "#/$defs/scope" },
      patch: { type: ["string", "null"], oneOf: [{ $ref: "#/$defs/sha256" }, { type: "null" }] },
      verification: { type: ["string", "null"], oneOf: [{ $ref: "#/$defs/sha256" }, { type: "null" }] },
      recommendations: {
        type: "array",
        maxItems: 1000,
        items: {
          type: "object",
          additionalProperties: false,
          required: ["id", "kind", "reason", "files", "risk", "verification", "uncertainty"],
          properties: {
            id: { $ref: "#/$defs/token" },
            kind: { enum: ["contract", "api", "effect", "policy", "test", "binding"] },
            reason: { $ref: "#/$defs/token" },
            files: { type: "array", maxItems: 256, items: { $ref: "#/$defs/relPath" } },
            risk: { enum: ["low", "medium", "high"] },
            verification: { type: "array", maxItems: 64, items: { $ref: "#/$defs/token" } },
            uncertainty: { type: ["string", "null"], maxLength: 256 },
          },
        },
      },
      unsupported: {
        type: "array",
        maxItems: 1000,
        items: {
          type: "object",
          additionalProperties: false,
          required: ["kind", "reason"],
          properties: { kind: { $ref: "#/$defs/token" }, reason: { $ref: "#/$defs/token" } },
        },
      },
      read_scope: { type: "array", maxItems: 4096, items: { $ref: "#/$defs/relPath" } },
      mutation_scope: { type: "array", maxItems: 4096, items: { $ref: "#/$defs/relPath" } },
      preservation: {
        type: "array",
        maxItems: 256,
        items: {
          type: "object",
          additionalProperties: false,
          required: ["obligation", "check"],
          properties: { obligation: { $ref: "#/$defs/token" }, check: { $ref: "#/$defs/token" } },
        },
      },
      pins: { $ref: "#/$defs/pins" },
      rollback: { $ref: "#/$defs/rollback" },
    },
    ["plan", "project", "analysis", "selection", "patch", "verification", "recommendations", "unsupported", "read_scope", "mutation_scope", "preservation", "pins", "rollback"],
  ),

  "adoption-verification": schema(
    "verification",
    "Lekalo adoption verification (v0.8.0)",
    base +
      "The closed verification specification and execution evidence variants. The specification pins the baseline source, the confirmed literal tool argv, every check with its independent expected outcome, and the enforced confinement dimensions. The execution variant records the actual baseline and candidate outcomes per check plus the behavior-oracle verdict; a specification or test id never decodes as an execution receipt, and preservation means exactly the stated obligations passed.",
    {
      variant: { enum: ["specification", "execution"] },
      verification: { $ref: "#/$defs/token" },
      spec: { type: ["string", "null"], oneOf: [{ $ref: "#/$defs/sha256" }, { type: "null" }] },
      plan: { type: ["string", "null"], oneOf: [{ $ref: "#/$defs/sha256" }, { type: "null" }] },
      tool: {
        type: ["object", "null"],
        oneOf: [
          {
            type: "object",
            additionalProperties: false,
            required: ["program", "args", "digest"],
            properties: {
              program: { type: "string", minLength: 1, maxLength: 256 },
              args: { type: "array", maxItems: 64, items: { type: "string", maxLength: 256 } },
              digest: { $ref: "#/$defs/digestOrNull" },
            },
          },
          { type: "null" },
        ],
      },
      baseline_source: { type: ["string", "null"], oneOf: [{ $ref: "#/$defs/sha256" }, { type: "null" }] },
      checks: {
        type: "array",
        maxItems: 256,
        items: {
          type: "object",
          additionalProperties: false,
          required: ["id", "kind", "expect_exit", "expect_stdout"],
          properties: {
            id: { $ref: "#/$defs/token" },
            kind: { enum: ["native-test", "behavior-oracle", "semantic"] },
            expect_exit: { type: "integer", minimum: 0, maximum: 255 },
            expect_stdout: { type: "array", maxItems: 64, items: { type: "string", maxLength: 1024 } },
          },
        },
      },
      confinement: {
        type: "array",
        maxItems: 8,
        items: {
          anyOf: [
            { enum: ["disposable-copy", "bounded-output", "no-network"] },
            // The bound disposable-copy token: the exact candidate
            // revision digest one real run covered (mirror of the
            // closed runtime spelling).
            { type: "string", pattern: "^disposable-copy:sha256:[0-9a-f]{64}$" },
          ],
        },
      },
      baseline: {
        type: ["object", "null"],
        oneOf: [
          {
            type: "object",
            additionalProperties: false,
            required: ["checks"],
            properties: { checks: { $ref: "#/$defs/checkOutcomes" } },
          },
          { type: "null" },
        ],
      },
      candidate: {
        type: ["object", "null"],
        oneOf: [
          {
            type: "object",
            additionalProperties: false,
            required: ["checks"],
            properties: { checks: { $ref: "#/$defs/checkOutcomes" } },
          },
          { type: "null" },
        ],
      },
      oracle: {
        type: ["object", "null"],
        oneOf: [
          {
            type: "object",
            additionalProperties: false,
            required: ["status", "detail"],
            properties: { status: { enum: ["passed", "failed", "unavailable"] }, detail: { $ref: "#/$defs/token" } },
          },
          { type: "null" },
        ],
      },
      conclusion: { enum: ["preserved", "failed", "unavailable", null] },
      // The execution-custody envelope of one real run (typed null on
      // specifications): the closed member mirrors the Rust custody
      // struct so every emitted execution document validates against
      // its own schema.
      custody: {
        type: ["object", "null"],
        oneOf: [
          {
            type: "object",
            additionalProperties: false,
            required: ["run", "measured_ms"],
            properties: {
              run: { $ref: "#/$defs/sha256" },
              measured_ms: { type: "integer", minimum: 1, maximum: 60000000 },
            },
          },
          { type: "null" },
        ],
      },
    },
    ["variant", "verification", "spec", "plan", "tool", "baseline_source", "checks", "confinement", "custody"],
    {
      checkOutcomes: {
        type: "array",
        maxItems: 256,
        items: {
          type: "object",
          additionalProperties: false,
          required: ["id", "status", "exit", "detail"],
          properties: {
            id: { $ref: "#/$defs/token" },
            status: { enum: ["passed", "failed", "unavailable"] },
            exit: { type: ["integer", "null"], minimum: 0, maximum: 255 },
            detail: { type: "string", maxLength: 1024 },
          },
        },
      },
    },
  ),

  "adoption-receipt": schema(
    "receipt",
    "Lekalo adoption receipt (v0.8.0)",
    base +
      "The closed receipt of one confirmation, apply, or rollback phase: the plan and input digests, the exact mutation set with before/after digests, the journal and snapshot references, the consumed verification evidence, and the completion or recovery state. No completed outcome exists without the required independent passing evidence; interrupted or failed phases record recovery-required instead.",
    {
      receipt: { $ref: "#/$defs/sha256" },
      phase: { enum: ["confirmation", "apply", "rollback"] },
      project: { $ref: "#/$defs/moduleId" },
      plan: { $ref: "#/$defs/sha256" },
      outcome: { enum: ["completed", "failed", "rolled-back", "recovery-required"] },
      writes: { type: "array", maxItems: 4096, items: { $ref: "#/$defs/writeEntry" } },
      journal: { type: ["string", "null"], oneOf: [{ $ref: "#/$defs/relPath" }, { type: "null" }] },
      snapshots: { type: "array", maxItems: 4096, items: { $ref: "#/$defs/relPath" } },
      verification: { type: ["string", "null"], oneOf: [{ $ref: "#/$defs/sha256" }, { type: "null" }] },
      pins: { $ref: "#/$defs/pins" },
      analysis: { type: ["string", "null"], oneOf: [{ $ref: "#/$defs/sha256" }, { type: "null" }] },
    },
    ["receipt", "phase", "project", "plan", "outcome", "writes", "journal", "snapshots", "verification", "pins", "analysis"],
  ),

  "adoption-coverage": schema(
    "coverage",
    "Lekalo adoption coverage (v0.8.0)",
    base +
      "The derived hybrid join of observed and contracted records for the selected scope: per-symbol effective mode (untouched observed, adapted legacy contracted, new contracted, unbound, conflicting), freshness state, origin, confirmed contract and binding/test coverage, verification receipts, and explicit gap rows. Aggregate counts use disjoint effective sets while raw provenance is preserved; the coverage view is never an ownership authority. The join binds the producing scan revision and its exact consumed input pins, so a consumer can distinguish a fresh panel from a stale one by re-fingerprinting the pinned set (a hand-authored document carries neither).",
    {
      project: { $ref: "#/$defs/moduleId" },
      scope: { $ref: "#/$defs/scope" },
      revision: { type: ["string", "null"], maxLength: 256 },
      pins: { type: ["object", "null"], oneOf: [{ $ref: "#/$defs/pins" }, { type: "null" }] },
      rows: {
        type: "array",
        maxItems: 10000,
        items: {
          type: "object",
          additionalProperties: false,
          required: ["symbol", "effective", "state", "origin", "contract", "binding", "tests", "verification", "gaps"],
          properties: {
            symbol: { $ref: "#/$defs/semanticId" },
            effective: { $ref: "#/$defs/effectiveMode" },
            state: { enum: ["current", "stale", "unknown"] },
            origin: { enum: ["legacy", "new", null] },
            contract: { type: ["string", "null"], maxLength: 256 },
            binding: { $ref: "#/$defs/bindingRef" },
            tests: { type: "array", maxItems: 64, items: { $ref: "#/$defs/nativeId" } },
            verification: { type: ["string", "null"], oneOf: [{ $ref: "#/$defs/sha256" }, { type: "null" }] },
            gaps: { type: "array", maxItems: 64, items: { type: "string", maxLength: 256 } },
          },
        },
      },
      counts: {
        type: "object",
        additionalProperties: false,
        required: ["observed", "contracted-legacy", "contracted-new", "unbound", "conflicting"],
        properties: {
          observed: { type: "integer", minimum: 0 },
          "contracted-legacy": { type: "integer", minimum: 0 },
          "contracted-new": { type: "integer", minimum: 0 },
          unbound: { type: "integer", minimum: 0 },
          conflicting: { type: "integer", minimum: 0 },
        },
      },
      gaps: { type: "array", maxItems: 1000, items: { $ref: "#/$defs/gap" } },
    },
    ["project", "scope", "revision", "pins", "rows", "counts", "gaps"],
  ),

  "adoption-status-report": schema(
    "status-report",
    "Lekalo adoption status report (v0.8.0)",
    base +
      "The typed opt-in wrapper around one upstream status result plus the admitted coverage attachment: the exact upstream envelope is preserved verbatim (including its failures) and joined with the effective mode, freshness, and verification panels and the explicit adoption gaps. Existing calls without the coverage flag keep their current wire family unchanged.",
    {
      upstream: { type: "object" },
      coverage: { $ref: "#/$defs/sha256" },
      panels: {
        type: "array",
        maxItems: 10000,
        items: {
          type: "object",
          additionalProperties: false,
          required: ["symbol", "mode", "state", "freshness"],
          properties: {
            symbol: { $ref: "#/$defs/semanticId" },
            mode: { $ref: "#/$defs/effectiveMode" },
            state: { enum: ["current", "stale", "unknown"] },
            freshness: { enum: ["fresh", "unqualified", "stale", "unknown"] },
          },
        },
      },
      gaps: { type: "array", maxItems: 1000, items: { $ref: "#/$defs/gap" } },
    },
    ["upstream", "coverage", "panels", "gaps"],
  ),

  "adoption-impact-report": schema(
    "impact-report",
    "Lekalo adoption impact report (v0.8.0)",
    base +
      "The typed opt-in wrapper around one upstream impact result plus the admitted coverage attachment: the exact upstream envelope is preserved verbatim and joined with mode-boundary witnesses (including observed neighbors outside the canonical graph) and explicit coverage gaps. Original impact completeness is never narrowed by the adoption projection.",
    {
      upstream: { type: "object" },
      coverage: { $ref: "#/$defs/sha256" },
      boundaries: {
        type: "array",
        maxItems: 10000,
        items: {
          type: "object",
          additionalProperties: false,
          required: ["symbol", "mode", "witness", "freshness"],
          properties: {
            symbol: { $ref: "#/$defs/semanticId" },
            mode: { $ref: "#/$defs/effectiveMode" },
            witness: { $ref: "#/$defs/token" },
            // The same typed freshness qualification the status panels
            // carry: a witness never presents stale evidence as plain
            // current.
            freshness: { enum: ["fresh", "unqualified", "stale", "unknown"] },
          },
        },
      },
      gaps: { type: "array", maxItems: 1000, items: { $ref: "#/$defs/gap" } },
    },
    ["upstream", "coverage", "boundaries", "gaps"],
  ),

  "adoption-trace-report": schema(
    "trace-report",
    "Lekalo adoption trace report (v0.8.0)",
    base +
      "The typed opt-in wrapper around one exact trace manifest and its query result plus the admitted coverage attachment: source, model, coverage, and verification provenance joins with uncovered and stale obligations kept visible. Linking a test or a receipt never fabricates passing execution; the supplied manifest is bound, not rewritten.",
    {
      manifest: { type: "object" },
      query: { type: "object" },
      coverage: { $ref: "#/$defs/sha256" },
      verification: { type: "array", maxItems: 1000, items: { $ref: "#/$defs/sha256" } },
      uncovered: { type: "array", maxItems: 1000, items: { $ref: "#/$defs/gap" } },
      stale: { type: "array", maxItems: 1000, items: { $ref: "#/$defs/gap" } },
    },
    ["manifest", "query", "coverage", "verification", "uncovered", "stale"],
  ),
};

const outputs = Object.entries(schemas).map(([family, body]) => [
  `contracts/${family}.schema.v${V}.json`,
  JSON.stringify(body, null, 2) + "\n",
]);

let drift = [];
for (const [path, body] of outputs) {
  if (check) {
    let current;
    try {
      current = readFileSync(resolve(root, path), "utf8");
    } catch {
      drift.push(`${path}: missing`);
      continue;
    }
    if (current !== body) drift.push(`${path}: drift`);
  } else {
    writeFileSync(resolve(root, path), body);
  }
}
if (check) {
  if (drift.length) {
    process.stderr.write(`${JSON.stringify({ ok: false, reason: "contract-drift", detail: drift }, null, 2)}\n`);
    process.exitCode = 1;
  } else {
    process.stdout.write(`${JSON.stringify({ ok: true, checked: outputs.length })}\n`);
  }
} else {
  process.stdout.write(`${JSON.stringify({ ok: true, wrote: outputs.map(([path]) => path) }, null, 2)}\n`);
}
