// Issue #51: the worklog-changelog pilot contract artifacts.
//
// Explicit authoring of the two closed 0.8.0 pilot families (composition,
// evidence) and the diagnostic-registry union successor that admits the
// seven proposed `worklog-changelog.*` rules. `--check` is read-only and
// used by the live gate; the default run (re)writes the contracts and the
// pilot registry from the entries declared below.
//
// The registry successor is a strict union: every current 0.6.5 entry is
// carried byte-for-byte and only the seven new LEK-WC-* entries are
// appended. Sort order, code uniqueness, and the message-id == id rule are
// re-verified here so the embedded registry can never fail closed.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

export const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const version = "0.8.0";
export const ordered = (v) =>
  Array.isArray(v)
    ? v.map(ordered)
    : v && typeof v === "object"
      ? Object.fromEntries(
          Object.keys(v)
            .sort()
            .map((k) => [k, ordered(v[k])]),
        )
      : v;
export const canonical = (v) => JSON.stringify(ordered(v));
export const digest = (v) => "sha256:" + createHash("sha256").update(v).digest("hex");
export const hash = (v) => digest(canonical(v));
const pretty = (v) => JSON.stringify(v, null, 2) + "\n";
const read = (path) => JSON.parse(readFileSync(resolve(root, path), "utf8"));
const write = (path, value) => writeFileSync(resolve(root, path), pretty(value));
export const header = (family) => ({
  schemaVersion: `lekalo/${family}/v${version}`,
  identity: `dev.lekalo.${family}@${version}`,
});

const writeMode = !process.argv.includes("--check");

// ---------------------------------------------------------------------------
// The seven pilot diagnostic rules (LEK-WC-001..007).
// ---------------------------------------------------------------------------

const wcStatuses = {
  invalid: ["invalid"],
};

const entries = [
  {
    id: "worklog-changelog.input-invalid",
    code: "LEK-WC-001",
    category: "model",
    default_severity: "error",
    allowed_statuses: wcStatuses.invalid,
    message_id: "worklog-changelog.input-invalid",
    default_message: "The worklog-changelog pilot composition document was refused.",
    location_requirement: "none",
    data_fields: [{ name: "detail", type: "token" }],
    allowed_fix_ids: [],
    lifecycle: "active",
  },
  {
    id: "worklog-changelog.effects-coupled",
    code: "LEK-WC-002",
    category: "semantic",
    default_severity: "error",
    allowed_statuses: wcStatuses.invalid,
    message_id: "worklog-changelog.effects-coupled",
    default_message:
      "Composition roles merged effects or enabled an unselected independent effect.",
    location_requirement: "none",
    data_fields: [{ name: "detail", type: "token" }],
    allowed_fix_ids: [],
    lifecycle: "active",
  },
  {
    id: "worklog-changelog.approval-unbound",
    code: "LEK-WC-003",
    category: "security",
    default_severity: "error",
    allowed_statuses: wcStatuses.invalid,
    message_id: "worklog-changelog.approval-unbound",
    default_message:
      "Publication preconditions cannot bind approval to the current preview, tenant, destination, and authority.",
    location_requirement: "none",
    data_fields: [{ name: "detail", type: "token" }],
    allowed_fix_ids: [],
    lifecycle: "active",
  },
  {
    id: "worklog-changelog.snapshot-mutable",
    code: "LEK-WC-004",
    category: "semantic",
    default_severity: "error",
    allowed_statuses: wcStatuses.invalid,
    message_id: "worklog-changelog.snapshot-mutable",
    default_message:
      "A published projection points to mutable source state or admitted evidence changes an existing snapshot.",
    location_requirement: "none",
    data_fields: [{ name: "detail", type: "token" }],
    allowed_fix_ids: [],
    lifecycle: "active",
  },
  {
    id: "worklog-changelog.public-data-unsafe",
    code: "LEK-WC-005",
    category: "security",
    default_severity: "error",
    allowed_statuses: wcStatuses.invalid,
    message_id: "worklog-changelog.public-data-unsafe",
    default_message:
      "The public projection violates a private-field, visibility, renderer, or safe-URL obligation.",
    location_requirement: "none",
    data_fields: [{ name: "detail", type: "token" }],
    allowed_fix_ids: [],
    lifecycle: "active",
  },
  {
    id: "worklog-changelog.replay-scope-invalid",
    code: "LEK-WC-006",
    category: "semantic",
    default_severity: "error",
    allowed_statuses: wcStatuses.invalid,
    message_id: "worklog-changelog.replay-scope-invalid",
    default_message:
      "Operation or tenant replay scopes collide, or a retry lacks its payload-bound safety policy.",
    location_requirement: "none",
    data_fields: [{ name: "detail", type: "token" }],
    allowed_fix_ids: [],
    lifecycle: "active",
  },
  {
    id: "worklog-changelog.evidence-incomplete",
    code: "LEK-WC-007",
    category: "compatibility",
    default_severity: "error",
    allowed_statuses: ["unsupported"],
    message_id: "worklog-changelog.evidence-incomplete",
    default_message:
      "Required native assertion, capability, or consumer evidence is absent, stale, or incomplete.",
    location_requirement: "none",
    data_fields: [{ name: "detail", type: "token" }],
    allowed_fix_ids: [],
    lifecycle: "active",
  },
];

const codePattern = /^LEK-[A-Z][A-Z0-9]{1,11}-[0-9]{3}$/;
for (const entry of entries) {
  assert.ok(codePattern.test(entry.code), entry.id);
  assert.equal(entry.message_id, entry.id);
  assert.ok(entry.default_message.length > 0 && entry.default_message.length <= 256);
}

// ---------------------------------------------------------------------------
// The registry union successor (0.8.0).
// ---------------------------------------------------------------------------

const predecessor = read("contracts/diagnostic-registry.v0.6.5.json");
const seenIds = new Set(predecessor.entries.map((entry) => entry.id));
const seenCodes = new Set(predecessor.entries.map((entry) => entry.code));
for (const entry of entries) {
  assert.ok(!seenIds.has(entry.id), `rule id collision: ${entry.id}`);
  assert.ok(!seenCodes.has(entry.code), `code collision: ${entry.code}`);
  seenIds.add(entry.id);
  seenCodes.add(entry.code);
}
const merged = [...predecessor.entries, ...entries].sort((a, b) =>
  a.id < b.id ? -1 : a.id > b.id ? 1 : 0,
);
assert.equal(merged.length, predecessor.entries.length + entries.length);
for (let index = 1; index < merged.length; index += 1) {
  assert.ok(merged[index - 1].id < merged[index].id, "sorted ids");
}
export const registry = {
  schema_version: `lekalo/diagnostic-registry/v${version}`,
  identity: `dev.lekalo.diagnostic-registry@${version}`,
  registry_version: version,
  closed: true,
  entries: merged,
};
export const registrySchema = JSON.parse(
  JSON.stringify(read("contracts/diagnostic-registry.schema.v0.6.5.json")).replaceAll(
    "0.6.5",
    version,
  ),
);
assert.equal(
  registrySchema.properties.schema_version.const,
  registry.schema_version,
  "registry schema identity follows the successor",
);
assert.equal(
  registrySchema.properties.identity.const,
  registry.identity,
  "registry schema identity const follows the successor",
);
export const registryDigest = digest(
  Buffer.from(canonical(registry), "utf8"),
);

// ---------------------------------------------------------------------------
// The closed composition contract (dev.lekalo.worklog-changelog-pilot).
// ---------------------------------------------------------------------------

const token = { type: "string", pattern: "^[A-Za-z0-9_.:/@-]+$", minLength: 1, maxLength: 256 };
const semanticId = {
  type: "string",
  pattern: "^[a-z][a-z0-9_-]*$|^[a-z][a-z0-9_-]*(\\.[a-z][a-z0-9_-]*)+(/[a-z][a-z0-9_-]+)*$",
  minLength: 1,
  maxLength: 256,
};
const digestPin = { type: "string", pattern: "^sha256:[a-f0-9]{64}$" };
const semver = { type: "string", pattern: "^[0-9]+\\.[0-9]+\\.[0-9]+$" };
const prose = {
  type: "string",
  minLength: 1,
  maxLength: 2000,
  pattern: "^[^\\u0000-\\u001f\\u007f]+$",
};
const fieldRef = { type: "string", pattern: "^[a-z][a-z0-9_]{0,63}$" };

const contractRef = {
  type: "object",
  additionalProperties: false,
  required: ["identity", "version", "digest"],
  properties: {
    identity: { type: "string", pattern: "^dev\\.lekalo\\.[a-z0-9-]+@0\\.2\\.16$", maxLength: 128 },
    version: { const: "0.2.16" },
    digest: digestPin,
  },
};

// The closed operation-role vocabulary: one role per pilot operation.
const roles = [
  "worklog.complete_task",
  "worklog.write_worklog",
  "worklog.edit_worklog",
  "worklog.request_provider_comment",
  "worklog.deliver_provider_comment",
  "changelog.create_draft",
  "changelog.edit_draft",
  "changelog.preview_entry",
  "changelog.approve_entry",
  "changelog.publish_entry",
  "changelog.withdraw_entry",
  "changelog.public_entry",
];

const effectKinds = [
  "create",
  "update",
  "delete",
  "emit-event",
  "enqueue-job",
  "external-call",
  "publish-output",
  "audit-log",
  "read",
];

const replayPolicy = {
  type: "object",
  additionalProperties: false,
  required: ["scope", "keyRef", "duplicate", "sameKeyDifferentRequest", "payloadBinding"],
  properties: {
    scope: semanticId,
    keyRef: fieldRef,
    duplicate: { enum: ["replay_result", "conflict"] },
    sameKeyDifferentRequest: { enum: ["conflict"] },
    payloadBinding: { enum: ["canonical_digest", "none"] },
  },
};

const composition = {
  $schema: "https://json-schema.org/draft/2020-12/schema",
  $id: "dev.lekalo.worklog-changelog-pilot.schema@0.8.0",
  title: "Lekalo worklog-changelog pilot composition",
  description:
    "Issue #51 closed composition contract: the neutral pilot Model pins, the selected operation roles with their declared effect boundaries, replay scopes, approval and snapshot field references, the public field allowlist, and required capabilities. No runtime values, secrets, or physical paths.",
  type: "object",
  additionalProperties: false,
  unevaluatedProperties: false,
  required: [
    "schemaVersion",
    "identity",
    "projectId",
    "closed",
    "modelRef",
    "irRef",
    "consumed",
    "operations",
    "independentEffects",
    "correlations",
    "approvals",
    "snapshots",
    "publicAllowlist",
    "replayScopes",
    "capabilityRequirements",
  ],
  properties: {
    schemaVersion: { const: "lekalo/worklog-changelog-pilot/v0.8.0" },
    identity: { const: "dev.lekalo.worklog-changelog-pilot@0.8.0" },
    projectId: { $ref: "#/$defs/semanticId" },
    closed: { const: true },
    modelRef: {
      type: "object",
      additionalProperties: false,
      required: ["modelVersion", "digest"],
      properties: {
        modelVersion: { const: "0.2.16" },
        digest: digestPin,
      },
    },
    irRef: {
      type: "object",
      additionalProperties: false,
      required: ["identity", "digest"],
      properties: {
        identity: { const: "dev.lekalo.ir@0.2.16" },
        digest: digestPin,
      },
    },
    consumed: {
      type: "array",
      minItems: 5,
      maxItems: 16,
      uniqueItems: true,
      items: {
        type: "object",
        additionalProperties: false,
        required: ["family", "ref"],
        properties: {
          family: {
            enum: [
              "authorization",
              "transaction-concurrency",
              "extended-effects",
              "error-contract",
              "invariant-transition",
              "requirements",
            ],
          },
          ref: contractRef,
        },
      },
    },
    operations: {
      type: "array",
      minItems: 12,
      maxItems: 128,
      uniqueItems: true,
      items: { $ref: "#/$defs/operationRole" },
    },
    independentEffects: {
      type: "array",
      minItems: 1,
      maxItems: 128,
      uniqueItems: true,
      items: { $ref: "#/$defs/independenceRule" },
    },
    correlations: {
      type: "array",
      maxItems: 64,
      uniqueItems: true,
      items: {
        type: "object",
        additionalProperties: false,
        required: ["correlationId", "keyRef", "memberOperations"],
        properties: {
          correlationId: semanticId,
          keyRef: fieldRef,
          memberOperations: {
            type: "array",
            minItems: 1,
            maxItems: 16,
            uniqueItems: true,
            items: { $ref: "#/$defs/roleName" },
          },
        },
      },
    },
    approvals: {
      type: "array",
      maxItems: 32,
      uniqueItems: true,
      items: {
        type: "object",
        additionalProperties: false,
        required: [
          "approvalId",
          "operation",
          "previewRevisionField",
          "previewDigestField",
          "tenantField",
          "destinationField",
          "authorityField",
          "policyRevisionField",
        ],
        properties: {
          approvalId: semanticId,
          operation: { $ref: "#/$defs/roleName" },
          previewRevisionField: fieldRef,
          previewDigestField: fieldRef,
          tenantField: fieldRef,
          destinationField: fieldRef,
          authorityField: fieldRef,
          policyRevisionField: fieldRef,
        },
      },
    },
    snapshots: {
      type: "array",
      maxItems: 32,
      uniqueItems: true,
      items: {
        type: "object",
        additionalProperties: false,
        required: [
          "snapshotId",
          "entity",
          "revisionField",
          "digestField",
          "schemaVersionField",
          "immutability",
        ],
        properties: {
          snapshotId: semanticId,
          entity: semanticId,
          revisionField: fieldRef,
          digestField: fieldRef,
          schemaVersionField: fieldRef,
          immutability: { enum: ["append_only"] },
        },
      },
    },
    publicAllowlist: {
      type: "object",
      additionalProperties: false,
      required: ["dto", "fields", "excluded", "rendering"],
      properties: {
        dto: semanticId,
        fields: {
          type: "array",
          minItems: 1,
          maxItems: 64,
          uniqueItems: true,
          items: { $ref: "#/$defs/publicField" },
        },
        excluded: {
          type: "array",
          minItems: 1,
          maxItems: 64,
          uniqueItems: true,
          items: fieldRef,
        },
        rendering: {
          type: "object",
          additionalProperties: false,
          required: ["defaultMode", "urlPolicy"],
          properties: {
            defaultMode: { enum: ["plain_text"] },
            urlPolicy: {
              type: "object",
              additionalProperties: false,
              required: ["schemes", "rejectEncoded", "rejectControlChars"],
              properties: {
                schemes: { type: "array", minItems: 1, maxItems: 8, uniqueItems: true, items: { enum: ["https", "http"] } },
                rejectEncoded: { const: true },
                rejectControlChars: { const: true },
              },
            },
          },
        },
      },
    },
    replayScopes: {
      type: "array",
      minItems: 1,
      maxItems: 64,
      uniqueItems: true,
      items: { $ref: "#/$defs/replayScope" },
    },
    capabilityRequirements: {
      type: "array",
      maxItems: 64,
      uniqueItems: true,
      items: {
        type: "object",
        additionalProperties: false,
        required: ["requirementId", "capability", "minimum", "reason", "operationRefs"],
        properties: {
          requirementId: semanticId,
          capability: token,
          minimum: { enum: ["full", "partial"] },
          reason: prose,
          operationRefs: {
            type: "array",
            minItems: 1,
            maxItems: 16,
            uniqueItems: true,
            items: { $ref: "#/$defs/roleName" },
          },
        },
      },
    },
  },
  $defs: {
    semanticId,
    roleName: { enum: roles },
    publicField: {
      type: "object",
      additionalProperties: false,
      required: ["name", "type", "source"],
      properties: {
        name: fieldRef,
        type: token,
        source: { enum: ["snapshot", "reviewed_projection"] },
      },
    },
    operationRole: {
      type: "object",
      additionalProperties: false,
      required: ["operation", "effects", "commitBoundary", "defaultSelection"],
      properties: {
        operation: { $ref: "#/$defs/roleName" },
        effects: {
          type: "array",
          minItems: 0,
          maxItems: 16,
          uniqueItems: true,
          items: { $ref: "#/$defs/effectDeclaration" },
        },
        commitBoundary: { enum: ["local", "local_then_external", "read_only"] },
        defaultSelection: { const: false },
        replay: { $ref: "#/$defs/replayScope" },
      },
    },
    effectDeclaration: {
      type: "object",
      additionalProperties: false,
      required: ["kind", "effectId"],
      properties: {
        kind: { enum: effectKinds },
        effectId: semanticId,
      },
    },
    independenceRule: {
      type: "object",
      additionalProperties: false,
      required: ["operation", "mustNotEnable"],
      properties: {
        operation: { $ref: "#/$defs/roleName" },
        mustNotEnable: {
          type: "array",
          minItems: 1,
          maxItems: 16,
          uniqueItems: true,
          items: { $ref: "#/$defs/roleName" },
        },
      },
    },
    replayScope: {
      type: "object",
      additionalProperties: false,
      required: ["operation", "scope", "keyRef", "duplicate", "sameKeyDifferentRequest", "payloadBinding"],
      properties: {
        operation: { $ref: "#/$defs/roleName" },
        scope: semanticId,
        keyRef: fieldRef,
        duplicate: { enum: ["replay_result", "conflict"] },
        sameKeyDifferentRequest: { const: "conflict" },
        payloadBinding: { enum: ["canonical_digest", "none"] },
      },
    },
  },
};

// ---------------------------------------------------------------------------
// The closed evidence contract (dev.lekalo.worklog-changelog-evidence).
// ---------------------------------------------------------------------------

const evidence = {
  $schema: "https://json-schema.org/draft/2020-12/schema",
  $id: "dev.lekalo.worklog-changelog-evidence.schema@0.8.0",
  title: "Lekalo worklog-changelog pilot evidence report",
  description:
    "Issue #51 closed evidence report: exact input and toolchain pins, stable scenario ids, per-effect outcomes and assertions, requirement-to-test and gate references, completeness and consumer witnesses, and per-target portability verdicts. No raw bodies, logs, host paths, or credentials.",
  type: "object",
  additionalProperties: false,
  unevaluatedProperties: false,
  required: [
    "schemaVersion",
    "identity",
    "reportVersion",
    "compositionRef",
    "toolchain",
    "profiles",
    "targets",
    "scenarios",
    "consumers",
    "completeness",
    "verdict",
  ],
  properties: {
    schemaVersion: { const: "lekalo/worklog-changelog-evidence/v0.8.0" },
    identity: { const: "dev.lekalo.worklog-changelog-evidence@0.8.0" },
    reportVersion: { const: "0.8.0" },
    compositionRef: {
      type: "object",
      additionalProperties: false,
      required: ["identity", "digest"],
      properties: {
        identity: { const: "dev.lekalo.worklog-changelog-pilot@0.8.0" },
        digest: digestPin,
      },
    },
    toolchain: {
      type: "object",
      additionalProperties: false,
      required: ["harness", "node", "ajv"],
      properties: {
        harness: token,
        node: token,
        ajv: token,
      },
    },
    profiles: {
      type: "array",
      minItems: 1,
      maxItems: 2,
      uniqueItems: true,
      items: { enum: ["strict", "permissive"] },
    },
    targets: {
      type: "array",
      minItems: 1,
      maxItems: 8,
      uniqueItems: true,
      items: { $ref: "#/$defs/targetReport" },
    },
    scenarios: {
      type: "array",
      minItems: 1,
      maxItems: 64,
      uniqueItems: true,
      items: { $ref: "#/$defs/scenarioResult" },
    },
    consumers: {
      type: "array",
      maxItems: 128,
      uniqueItems: true,
      items: {
        type: "object",
        additionalProperties: false,
        required: ["consumer", "witness", "relation"],
        properties: {
          consumer: semanticId,
          witness: token,
          relation: token,
        },
      },
    },
    completeness: {
      type: "object",
      additionalProperties: false,
      required: ["consumerClosure", "coverage", "unknownExternalConsumers"],
      properties: {
        consumerClosure: { enum: ["complete", "incomplete"] },
        coverage: { enum: ["full", "partial"] },
        unknownExternalConsumers: { const: true },
      },
    },
    verdict: {
      type: "object",
      additionalProperties: false,
      required: ["strict", "permissive", "degraded"],
      properties: {
        strict: { enum: ["pass", "denied", "unsupported"] },
        permissive: { enum: ["pass", "degraded"] },
        degraded: { type: "boolean" },
      },
    },
  },
  $defs: {
    targetReport: {
      type: "object",
      additionalProperties: false,
      required: ["target", "backend", "execution", "capabilities", "assertions", "blocking"],
      properties: {
        target: { enum: ["php", "go"] },
        backend: {
          type: "object",
          additionalProperties: false,
          required: ["state"],
          properties: {
            state: { enum: ["bound", "absent"] },
            identity: { type: ["string", "null"], maxLength: 128 },
            digest: { type: ["string", "null"], pattern: digestPin.pattern },
          },
        },
        execution: { enum: ["executed", "not_executed"] },
        capabilities: {
          type: "array",
          maxItems: 32,
          uniqueItems: true,
          items: {
            type: "object",
            additionalProperties: false,
            required: ["capability", "support", "blocking", "note"],
            properties: {
              capability: token,
              support: { enum: ["supported", "partial", "unsupported", "unknown"] },
              approval: { enum: ["not_required", "reviewed"] },
              blocking: { type: "boolean" },
              note: prose,
            },
          },
        },
        assertions: {
          type: "array",
          maxItems: 256,
          uniqueItems: true,
          items: {
            type: "object",
            additionalProperties: false,
            required: ["scenario", "status"],
            properties: {
              scenario: semanticId,
              status: { enum: ["pass", "fail", "unsupported", "not_executed"] },
            },
          },
        },
        blocking: {
          type: "array",
          maxItems: 32,
          uniqueItems: true,
          items: token,
        },
      },
    },
    scenarioResult: {
      type: "object",
      additionalProperties: false,
      required: ["scenario", "status", "targets"],
      properties: {
        scenario: semanticId,
        status: { enum: ["pass", "fail", "unsupported", "not_executed"] },
        targets: {
          type: "array",
          minItems: 1,
          maxItems: 8,
          uniqueItems: true,
          items: { enum: ["php", "go", "fake-reference"] },
        },
      },
    },
  },
};

export const shapes = {
  "worklog-changelog-pilot": composition,
  "worklog-changelog-evidence": evidence,
};

// ---------------------------------------------------------------------------
// Emit or check.
// ---------------------------------------------------------------------------

const outputs = new Map([
  ["contracts/diagnostic-registry.v0.8.0.json", registry],
  ["contracts/diagnostic-registry.schema.v0.8.0.json", registrySchema],
  ["contracts/worklog-changelog-pilot.schema.v0.8.0.json", composition],
  ["contracts/worklog-changelog-evidence.schema.v0.8.0.json", evidence],
]);

let drift = 0;
for (const [path, value] of outputs) {
  const text = pretty(value);
  let current = null;
  try {
    current = readFileSync(resolve(root, path), "utf8");
  } catch {
    current = null;
  }
  if (current !== text) {
    drift += 1;
    if (writeMode) writeFileSync(resolve(root, path), text);
  }
}
console.log(
  JSON.stringify({
    ok: true,
    product: version,
    registryEntries: registry.entries.length,
    addedRules: entries.map((entry) => `${entry.id}=${entry.code}`),
    drift,
    mode: writeMode ? "write" : "check",
  }),
);
if (drift > 0 && !writeMode) process.exit(1);
