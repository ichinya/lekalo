#!/usr/bin/env node
// Issue #33 contract generator: deterministically emits the MCP contract
// family at the product version 0.8.0. Run from the repository root:
//
//   node scripts/gen-mcp-contracts.mjs
//
// The output is byte-stable for a given upstream inventory; regenerated
// artifacts are committed. Every schema is a closed Draft 2020-12 root
// object (type/properties/required/additionalProperties:false, no root
// oneOf/anyOf/allOf/not and no bare root $ref); nested discriminated
// branches remain possible. Embedded upstream payloads are described by
// their pinned discriminator pair and validated against the separately
// published upstream contracts by scripts/test-mcp-contracts.mjs.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const out = (name, document) => {
  writeFileSync(join(root, "contracts", name), `${JSON.stringify(document, null, 2)}\n`);
  process.stdout.write(`${name}\n`);
};

const VERSION = "0.8.0";
const PROTOCOL_DATE = "2025-11-25";
const ID_PATTERN = "^[a-z][a-z0-9-]{0,63}$";
const SYMBOL_PATTERN = "^[A-Za-z][A-Za-z0-9_.-]{0,191}$";
const SHA256 = "^sha256-[0-9a-f]{64}$";
const STATUSES = [
  "valid",
  "invalid",
  "denied",
  "unsupported",
  "unavailable",
  "unsupported-version",
];
const FAILURE_CLASSES = [
  "invalid-input",
  "scope-denied",
  "sensitive-output",
  "request-limit",
  "output-limit",
  "timeout",
  "cancelled",
  "contract-unsupported",
  "snapshot-stale",
  "backend-unavailable",
  "result-invalid",
  "domain-invalid",
  "domain-denied",
  "domain-unavailable",
  "domain-unsupported",
  "domain-unsupported-version",
];
const LIMITS = {
  frameBytes: 262144,
  maxResponseBytes: 1048576,
  maxResponseBytesCeiling: 4194304,
  timeoutMs: 30000,
  timeoutMsCeiling: 120000,
  concurrentCalls: 4,
  queuedRequests: 16,
  semanticRoots: 128,
  pageSize: 100,
  pageSizeCeiling: 500,
  watchedIdentities: 256,
  nestingDepth: 64,
};

const idConst = (value) => ({ const: value });
const scopeSchema = {
  type: "object",
  properties: {
    projectId: { type: "string", pattern: ID_PATTERN },
    groupId: { type: ["string", "null"], pattern: ID_PATTERN },
  },
  required: ["projectId", "groupId"],
  additionalProperties: false,
};
const diagnosticsSchema = {
  type: "array",
  maxItems: 64,
  items: { type: "object" },
  description:
    "Wire diagnostic items (lekalo/diagnostic/v0.2.16) resolved against dev.lekalo.diagnostic-registry@0.8.0; scripts/test-mcp-contracts.mjs validates every item against the published diagnostic schema and the union registry.",
};
const contractPins = (extra) => ({
  type: "object",
  properties: {
    diagnostic: idConst("0.2.16"),
    diagnosticRegistry: idConst("0.8.0"),
    protocolProfile: idConst(PROTOCOL_DATE),
    ...extra,
  },
  required: ["diagnostic", "diagnosticRegistry", "protocolProfile", ...Object.keys(extra)],
  additionalProperties: false,
});
const pinnedBranch = (discriminator, identityConst, payloadKey) => ({
  type: "object",
  properties: {
    schemaVersion: idConst(discriminator),
    identity: idConst(identityConst),
    [payloadKey]: { type: "object" },
  },
  required: ["schemaVersion", "identity", payloadKey],
  additionalProperties: false,
});
const failureBranch = {
  type: "object",
  properties: {
    failureClass: { enum: FAILURE_CLASSES },
    detail: { type: "string", minLength: 1, maxLength: 256 },
  },
  required: ["failureClass"],
  additionalProperties: false,
};

const inputEnvelope = (tool, members, required) => ({
  $schema: "https://json-schema.org/draft/2020-12/schema",
  $id: `dev.lekalo.mcp-tool-${tool}-input@${VERSION}`,
  title: `Lekalo MCP tool input: ${tool}`,
  description: `Issue #33 closed request grammar of the ${tool} tool. Structural violations (missing, unknown, wrong-typed, or out-of-range members) are JSON-RPC -32602; cross-field semantic invariants are the typed mcp.input-invalid refusal. projectId must name a server-catalog project; groupId an optional catalog group of that project.`,
  type: "object",
  properties: {
    projectId: { type: "string", pattern: ID_PATTERN },
    groupId: { type: "string", pattern: ID_PATTERN },
    timeoutMs: { type: "integer", minimum: 1, maximum: LIMITS.timeoutMsCeiling },
    ...members,
  },
  required: ["projectId", ...required],
  additionalProperties: false,
});

const outputEnvelope = (tool, toolConst, pins, discriminator, identityConst, result, defs) => ({
  $schema: "https://json-schema.org/draft/2020-12/schema",
  $id: `dev.lekalo.mcp-tool-${tool}-output@${VERSION}`,
  title: `Lekalo MCP tool output: ${tool}`,
  description: `Issue #33 closed result envelope of the ${tool} tool. status "valid" carries the preserved core receipt in "result"; every other status carries the typed "failure" branch and never a fabricated payload. Presence of result/failure per status, and refusal classes, are enforced semantically by the server and by scripts/test-mcp-contracts.mjs.`,
  type: "object",
  properties: {
    schemaVersion: idConst(discriminator),
    identity: idConst(identityConst),
    tool: idConst(toolConst),
    scope: scopeSchema,
    snapshotRef: { type: "string", pattern: SHA256 },
    status: { enum: STATUSES },
    contractPins: contractPins(pins),
    diagnostics: diagnosticsSchema,
    result,
    failure: failureBranch,
  },
  required: [
    "schemaVersion",
    "identity",
    "tool",
    "scope",
    "snapshotRef",
    "status",
    "contractPins",
    "diagnostics",
  ],
  additionalProperties: false,
  ...(defs ? { $defs: defs } : {}),
});

// ---- Result branches. ------------------------------------------------------
//
// Uniform discriminator-pinned payload branches (inspect/impact/context)
// preserve the core receipt object under its family key. The validate
// receipt, the graph receipts, and the trace export have tool-specific
// upstream shapes and pin them directly; no discriminator is ever injected
// into a frozen core payload.

// ---- The seven tool input schemas. ----------------------------------------

out(
  "mcp-tool-inspect-input.schema.v0.8.0.json",
  inputEnvelope(
    "inspect",
    {
      symbol: { type: "string", pattern: SYMBOL_PATTERN },
      include: {
        type: "array",
        items: { enum: ["bindings", "scenarios"] },
        uniqueItems: true,
        maxItems: 2,
      },
    },
    ["symbol"],
  ),
);

out(
  "mcp-tool-impact-input.schema.v0.8.0.json",
  inputEnvelope(
    "impact",
    {
      mode: { enum: ["symbol", "changed"] },
      symbol: { type: "string", pattern: SYMBOL_PATTERN },
      baseRef: { type: "string", minLength: 1, maxLength: 128 },
      headRef: { type: "string", minLength: 1, maxLength: 128 },
      worktree: { type: "boolean" },
      depth: { type: "integer", minimum: 1, maximum: 256 },
      module: { type: "string", pattern: ID_PATTERN },
      target: { type: "string", pattern: ID_PATTERN },
      relations: {
        type: "array",
        items: { type: "string", pattern: "^[a-z_-]{1,32}$" },
        uniqueItems: true,
        maxItems: 32,
      },
      profile: { type: "string", pattern: "^[a-z][a-z0-9-]{0,31}$" },
    },
    ["mode"],
  ),
);

out(
  "mcp-tool-context-input.schema.v0.8.0.json",
  inputEnvelope(
    "context",
    {
      mode: { enum: ["symbol", "changed"] },
      symbol: { type: "string", pattern: SYMBOL_PATTERN },
      changedSymbols: {
        type: "array",
        items: { type: "string", pattern: SYMBOL_PATTERN },
        minItems: 1,
        maxItems: 64,
        uniqueItems: true,
      },
      budget: { type: "integer", minimum: 1, maximum: 1000000 },
      spans: { type: "boolean" },
    },
    ["mode", "budget"],
  ),
);

out(
  "mcp-tool-graph-input.schema.v0.8.0.json",
  inputEnvelope(
    "graph",
    {
      mode: { enum: ["show", "callers", "path", "export"] },
      symbol: { type: "string", pattern: SYMBOL_PATTERN },
      from: { type: "string", pattern: SYMBOL_PATTERN },
      to: { type: "string", pattern: SYMBOL_PATTERN },
      transitive: { type: "boolean" },
    },
    ["mode"],
  ),
);

out(
  "mcp-tool-validate-input.schema.v0.8.0.json",
  inputEnvelope(
    "validate",
    {
      module: { type: "string", pattern: ID_PATTERN },
      strict: { type: "boolean" },
    },
    [],
  ),
);

out(
  "mcp-tool-target-status-input.schema.v0.8.0.json",
  inputEnvelope(
    "target-status",
    {
      targetId: { type: "string", pattern: ID_PATTERN },
      profileId: { type: "string", pattern: ID_PATTERN },
    },
    [],
  ),
);

out(
  "mcp-tool-trace-export-input.schema.v0.8.0.json",
  inputEnvelope(
    "trace-export",
    {
      manifestRef: { type: "string", pattern: ID_PATTERN },
    },
    ["manifestRef"],
  ),
);

// ---- The seven tool output schemas. ---------------------------------------

const RESULT_BRANCHES = {
  inspect: pinnedBranch("lekalo/inspect/v0.2.16", "dev.lekalo.inspect@0.2.16", "inspect"),
  impact: pinnedBranch("lekalo/impact/v0.2.16", "dev.lekalo.impact@0.2.16", "impact"),
  context: pinnedBranch("lekalo/context/v0.2.16", "dev.lekalo.context@0.2.16", "context"),
  "target-status": pinnedBranch("lekalo/doctor/v0.3.2", "dev.lekalo.doctor@0.3.2", "doctor"),
  validate: {
    type: "object",
    description:
      "The exact validation success receipt (contracts/validation-report.schema.v0.6.4.json): status, modelVersion, the report wire, and the optional findings/reasonCodes members.",
    properties: {
      status: idConst("valid"),
      modelVersion: { type: "string", pattern: "^[0-9]+\\.[0-9]+\\.[0-9]+$" },
      validation: { type: "object" },
      diagnostics: { type: "array", maxItems: 512, items: { type: "object" } },
      reasonCodes: {
        type: "array",
        maxItems: 512,
        uniqueItems: true,
        items: { type: "string", pattern: "^[a-z][a-z0-9-]*(\\.[a-z][a-z0-9-]*)+$" },
      },
    },
    required: ["status", "modelVersion", "validation"],
    additionalProperties: false,
  },
  graph: {
    type: "object",
    description:
      "Mode-tagged receipt: show/callers/path carry the frozen core query payload unchanged; export carries the canonical whole-graph document (lekalo/graph/v0.2.16). Mode/branch coherence is semantic.",
    properties: {
      mode: { enum: ["show", "callers", "path", "export"] },
      query: { $ref: "#/$defs/graphQuery" },
      export: {
        type: "object",
        properties: {
          schemaVersion: idConst("lekalo/graph/v0.2.16"),
          identity: idConst("dev.lekalo.graph@0.2.16"),
        },
        required: ["schemaVersion", "identity"],
      },
    },
    required: ["mode"],
    additionalProperties: false,
  },
  "trace-export": {
    type: "object",
    properties: {
      schemaVersion: idConst("lekalo/trace-manifest/v0.2.16"),
      identity: idConst("dev.lekalo.trace-manifest@0.2.16"),
      trace: { type: "object" },
      manifestDigest: { type: "string", pattern: "^[0-9a-f]{64}$" },
    },
    required: ["schemaVersion", "identity", "trace", "manifestDigest"],
    additionalProperties: false,
  },
};

const toolPins = {
  inspect: { inspect: idConst("0.2.16") },
  impact: { impact: idConst("0.2.16") },
  context: { context: idConst("0.2.16") },
  graph: { graph: idConst("0.2.16") },
  validate: { validation: idConst("0.6.4") },
  "target-status": { doctor: idConst("0.3.2") },
  "trace-export": { traceManifest: idConst("0.2.16") },
};
const toolConsts = {
  inspect: "lekalo_inspect",
  impact: "lekalo_impact",
  context: "lekalo_context",
  graph: "lekalo_graph",
  validate: "lekalo_validate",
  "target-status": "lekalo_target_status",
  "trace-export": "lekalo_trace_export",
};
const GRAPH_QUERY_DEFS = {
  graphQuery: {
    description:
      "The frozen core query payloads of `graph show`, `graph callers`, and `graph path`, described without injecting a discriminator; exactly one branch matches.",
    oneOf: [
      {
        type: "object",
        properties: {
          node: { type: "object" },
          dependencies: { type: "array", items: { type: "object" } },
          dependents: { type: "array", items: { type: "object" } },
        },
        required: ["node", "dependencies", "dependents"],
        additionalProperties: false,
      },
      {
        type: "object",
        properties: {
          node: { type: "object" },
          callers: { type: "array", items: { type: "object" } },
          complete: { type: "boolean" },
        },
        required: ["node", "callers", "complete"],
        additionalProperties: false,
      },
      {
        type: "object",
        properties: {
          confidence: { type: "string" },
          complete: { type: "boolean" },
          edges: { type: "array", items: { type: "object" } },
          from: { type: "string" },
          length: { type: "integer", minimum: 0 },
          nodes: { type: "array", items: { type: "string" } },
          to: { type: "string" },
        },
        required: ["confidence", "complete", "edges", "from", "length", "nodes", "to"],
        additionalProperties: false,
      },
    ],
  },
};

for (const tool of Object.keys(toolConsts)) {
  out(
    `mcp-tool-${tool}-output.schema.v0.8.0.json`,
    outputEnvelope(
      tool,
      toolConsts[tool],
      toolPins[tool],
      `lekalo/mcp-tool-${tool}-output/v${VERSION}`,
      `dev.lekalo.mcp-tool-${tool}-output@${VERSION}`,
      RESULT_BRANCHES[tool],
      tool === "graph" ? GRAPH_QUERY_DEFS : undefined,
    ),
  );
}

// ---- Capabilities manifest. -----------------------------------------------

const toolManifest = (tool) => ({
  name: toolConsts[tool],
  title: `Lekalo ${tool}`,
  input: {
    identity: `dev.lekalo.mcp-tool-${tool}-input@${VERSION}`,
    discriminator: `lekalo/mcp-tool-${tool}-input/v${VERSION}`,
  },
  output: {
    identity: `dev.lekalo.mcp-tool-${tool}-output@${VERSION}`,
    discriminator: `lekalo/mcp-tool-${tool}-output/v${VERSION}`,
  },
});
const RESOURCE_FAMILIES = [
  {
    kind: "project-summary",
    uriTemplate: "lekalo://{projectId}/summary",
    description: "Project identity, schema pins, admitted scope, revision digest, and counts.",
  },
  {
    kind: "module-list",
    uriTemplate: "lekalo://{projectId}/modules",
    description: "Deterministic bounded page of authorized module metadata with an opaque cursor.",
  },
  {
    kind: "symbol-metadata",
    uriTemplate: "lekalo://{projectId}/symbol/{symbol}",
    description: "The authorized inspect projection of one symbol; never raw source.",
  },
  {
    kind: "target-status",
    uriTemplate: "lekalo://{projectId}/targets",
    description: "Selected lock/profile/inventory observations with separate unavailable states.",
  },
  {
    kind: "diagnostics",
    uriTemplate: "lekalo://{projectId}/diagnostics",
    description: "Safe stable diagnostics and scoped evidence counts; no raw logs or history.",
  },
];

out("mcp-capabilities.schema.v0.8.0.json", {
  $schema: "https://json-schema.org/draft/2020-12/schema",
  $id: `dev.lekalo.mcp-capabilities@${VERSION}`,
  title: "Lekalo MCP capabilities manifest",
  description:
    "Issue #33 deterministic, embedded capability manifest of the read-only Lekalo MCP server. Emitted by `lekalo mcp describe --json` without reading a project or starting a server.",
  type: "object",
  properties: {
    schemaVersion: idConst(`lekalo/mcp-capabilities/v${VERSION}`),
    identity: idConst(`dev.lekalo.mcp-capabilities@${VERSION}`),
    protocol: {
      type: "object",
      properties: {
        name: idConst("mcp"),
        transport: idConst("stdio"),
        profile: idConst(PROTOCOL_DATE),
        frameDelimiter: idConst("lf"),
      },
      required: ["name", "transport", "profile", "frameDelimiter"],
      additionalProperties: false,
    },
    server: {
      type: "object",
      properties: {
        name: idConst("lekalo"),
        readOnly: idConst(true),
      },
      required: ["name", "readOnly"],
      additionalProperties: false,
    },
    limits: {
      type: "object",
      properties: Object.fromEntries(
        Object.entries(LIMITS).map(([key, value]) => [key, idConst(value)]),
      ),
      required: Object.keys(LIMITS),
      additionalProperties: false,
    },
    tools: {
      type: "array",
      minItems: 7,
      maxItems: 7,
      uniqueItems: true,
      items: {
        type: "object",
        properties: {
          name: { type: "string", pattern: "^lekalo_[a-z_]{1,32}$" },
          title: { type: "string", minLength: 1, maxLength: 64 },
          input: {
            type: "object",
            properties: {
              identity: { type: "string", pattern: `^dev\\.lekalo\\.mcp-tool-[a-z-]+-input@${VERSION.replace(/\./g, "\\.")}$` },
              discriminator: { type: "string", pattern: `^lekalo/mcp-tool-[a-z-]+-input/v${VERSION.replace(/\./g, "\\.")}$` },
            },
            required: ["identity", "discriminator"],
            additionalProperties: false,
          },
          output: {
            type: "object",
            properties: {
              identity: { type: "string", pattern: `^dev\\.lekalo\\.mcp-tool-[a-z-]+-output@${VERSION.replace(/\./g, "\\.")}$` },
              discriminator: { type: "string", pattern: `^lekalo/mcp-tool-[a-z-]+-output/v${VERSION.replace(/\./g, "\\.")}$` },
            },
            required: ["identity", "discriminator"],
            additionalProperties: false,
          },
        },
        required: ["name", "title", "input", "output"],
        additionalProperties: false,
      },
    },
    resources: {
      type: "array",
      minItems: 5,
      maxItems: 5,
      uniqueItems: true,
      items: {
        type: "object",
        properties: {
          kind: {
            enum: RESOURCE_FAMILIES.map((family) => family.kind),
          },
          uriTemplate: { type: "string", pattern: "^lekalo://\\{projectId\\}/[a-z{}A-Z-]+$" },
          description: { type: "string", minLength: 1, maxLength: 256 },
        },
        required: ["kind", "uriTemplate", "description"],
        additionalProperties: false,
      },
    },
    pins: {
      type: "object",
      properties: {
        diagnostic: idConst("0.2.16"),
        diagnosticRegistry: idConst("0.8.0"),
        serverConfig: idConst(VERSION),
        graphQuery: idConst(VERSION),
      },
      required: ["diagnostic", "diagnosticRegistry", "serverConfig", "graphQuery"],
      additionalProperties: false,
    },
    diagnosticCodes: {
      type: "array",
      minItems: 11,
      maxItems: 11,
      uniqueItems: true,
      items: { type: "string", pattern: "^LEK-MCP-0(0[1-9]|1[01])$" },
    },
  },
  required: [
    "schemaVersion",
    "identity",
    "protocol",
    "server",
    "limits",
    "tools",
    "resources",
    "pins",
    "diagnosticCodes",
  ],
  additionalProperties: false,
});

// ---- Trusted server config. ----------------------------------------------

out("mcp-server-config.schema.v0.8.0.json", {
  $schema: "https://json-schema.org/draft/2020-12/schema",
  $id: `dev.lekalo.mcp-server-config@${VERSION}`,
  title: "Lekalo MCP server config (trusted catalog)",
  description:
    "Issue #33 trusted launcher catalog for `lekalo mcp serve`: admitted projects with physical roots, named module allow-list groups, approved trace manifest inputs, changed-mode Git admission, and optional limit reductions. Physical roots never appear in discovery or result payloads; this config is not a persisted canonical artifact.",
  type: "object",
  properties: {
    schemaVersion: idConst(`lekalo/mcp-server-config/v${VERSION}`),
    identity: idConst(`dev.lekalo.mcp-server-config@${VERSION}`),
    projects: {
      type: "array",
      minItems: 1,
      maxItems: LIMITS.semanticRoots,
      uniqueItems: true,
      items: {
        type: "object",
        properties: {
          id: { type: "string", pattern: ID_PATTERN },
          root: { type: "string", minLength: 1, maxLength: 1024 },
        },
        required: ["id", "root"],
        additionalProperties: false,
      },
    },
    groups: {
      type: "array",
      maxItems: 128,
      uniqueItems: true,
      items: {
        type: "object",
        properties: {
          id: { type: "string", pattern: ID_PATTERN },
          projectId: { type: "string", pattern: ID_PATTERN },
          modules: {
            type: "array",
            minItems: 1,
            maxItems: 500,
            uniqueItems: true,
            items: { type: "string", pattern: ID_PATTERN },
          },
        },
        required: ["id", "projectId", "modules"],
        additionalProperties: false,
      },
    },
    manifests: {
      type: "array",
      maxItems: 256,
      uniqueItems: true,
      items: {
        type: "object",
        properties: {
          id: { type: "string", pattern: ID_PATTERN },
          projectId: { type: "string", pattern: ID_PATTERN },
          path: {
            type: "string",
            pattern: "^[A-Za-z0-9][A-Za-z0-9/._-]{0,511}$",
            description:
              "Project-relative logical path of one approved trace manifest input; no traversal, no backslash, no dot segments.",
          },
        },
        required: ["id", "projectId", "path"],
        additionalProperties: false,
      },
    },
    git: {
      type: "object",
      properties: {
        changedMode: { type: "boolean" },
      },
      required: ["changedMode"],
      additionalProperties: false,
      description:
        "Explicit admission of changed-mode Git access per project catalog; without it the changed impact mode refuses instead of scanning.",
    },
    limits: {
      type: "object",
      properties: {
        timeoutMs: { type: "integer", minimum: 1, maximum: LIMITS.timeoutMs },
        maxResponseBytes: {
          type: "integer",
          minimum: 1024,
          maximum: LIMITS.maxResponseBytes,
        },
        pageSize: { type: "integer", minimum: 1, maximum: LIMITS.pageSize },
      },
      additionalProperties: false,
      description: "Optional reductions of the server defaults; never a raise.",
    },
  },
  required: ["schemaVersion", "identity", "projects"],
  additionalProperties: false,
});

// ---- Resource envelope. --------------------------------------------------

const resourceData = {
  oneOf: [
    {
      type: "object",
      properties: {
        project: { type: "string", pattern: ID_PATTERN },
        modelVersion: { type: "string", pattern: "^[0-9]+\\.[0-9]+\\.[0-9]+$" },
        moduleCount: { type: "integer", minimum: 0 },
        definitionCount: { type: "integer", minimum: 0 },
        unknownStates: {
          type: "array",
          maxItems: 16,
          uniqueItems: true,
          items: { type: "string", pattern: "^[a-z-]{1,32}$" },
        },
      },
      required: ["project", "modelVersion", "moduleCount", "definitionCount", "unknownStates"],
      additionalProperties: false,
    },
    {
      type: "object",
      properties: {
        modules: {
          type: "array",
          maxItems: 500,
          items: {
            type: "object",
            properties: {
              id: { type: "string", pattern: ID_PATTERN },
              definitionCount: { type: "integer", minimum: 0 },
            },
            required: ["id", "definitionCount"],
            additionalProperties: false,
          },
        },
      },
      required: ["modules"],
      additionalProperties: false,
    },
    pinnedBranch("lekalo/inspect/v0.2.16", "dev.lekalo.inspect@0.2.16", "inspect"),
    pinnedBranch("lekalo/doctor/v0.3.2", "dev.lekalo.doctor@0.3.2", "doctor"),
    {
      type: "object",
      properties: {
        diagnosticCount: { type: "integer", minimum: 0 },
        evidenceCount: { type: "integer", minimum: 0 },
        complete: { type: "boolean" },
        reasons: {
          type: "array",
          maxItems: 16,
          uniqueItems: true,
          items: { type: "string", pattern: "^[a-z-]{1,48}$" },
        },
      },
      required: ["diagnosticCount", "evidenceCount", "complete"],
      additionalProperties: false,
    },
  ],
};

out("mcp-resource.schema.v0.8.0.json", {
  $schema: "https://json-schema.org/draft/2020-12/schema",
  $id: `dev.lekalo.mcp-resource@${VERSION}`,
  title: "Lekalo MCP resource envelope",
  description:
    "Issue #33 closed resource identity, scope, revision, data, and bounded-page metadata of the five resource families. Per-kind data branches are nested; kind/data coherence is enforced semantically by the server and the contract gate.",
  type: "object",
  properties: {
    schemaVersion: idConst(`lekalo/mcp-resource/v${VERSION}`),
    identity: idConst(`dev.lekalo.mcp-resource@${VERSION}`),
    kind: { enum: RESOURCE_FAMILIES.map((family) => family.kind) },
    uri: { type: "string", pattern: "^lekalo://[a-z][a-z0-9-]{0,63}/[a-z][a-zA-Z0-9/-]{0,240}$" },
    scope: scopeSchema,
    revision: { type: "string", pattern: SHA256 },
    data: resourceData,
    page: {
      type: "object",
      properties: {
        cursor: { type: "string", maxLength: 256 },
        nextCursor: { type: ["string", "null"], maxLength: 256 },
        returned: { type: "integer", minimum: 0, maximum: 500 },
        total: { type: "integer", minimum: 0 },
      },
      required: ["cursor", "nextCursor", "returned", "total"],
      additionalProperties: false,
    },
  },
  required: ["schemaVersion", "identity", "kind", "uri", "scope", "revision", "data"],
  additionalProperties: false,
});

// ---- Graph query describing schema. --------------------------------------

out("mcp-graph-query.schema.v0.8.0.json", {
  $schema: "https://json-schema.org/draft/2020-12/schema",
  $id: `dev.lekalo.mcp-graph-query@${VERSION}`,
  title: "Lekalo MCP graph query receipts (describing)",
  description:
    "Issue #33 describing schema for the current `graph show`, `graph callers`, and `graph path` query payloads. This schema never injects a discriminator into the frozen core query payloads; it only describes them so host validators can parse receipts. The whole-graph export keeps its own published contract (lekalo/graph/v0.2.16).",
  type: "object",
  oneOf: [
    {
      type: "object",
      properties: {
        node: { type: "object" },
        dependencies: { type: "array", items: { type: "object" } },
        dependents: { type: "array", items: { type: "object" } },
      },
      required: ["node", "dependencies", "dependents"],
      additionalProperties: false,
    },
    {
      type: "object",
      properties: {
        node: { type: "object" },
        callers: { type: "array", items: { type: "object" } },
        complete: { type: "boolean" },
      },
      required: ["node", "callers", "complete"],
      additionalProperties: false,
    },
    {
      type: "object",
      properties: {
        confidence: { type: "string" },
        complete: { type: "boolean" },
        edges: { type: "array", items: { type: "object" } },
        from: { type: "string" },
        length: { type: "integer", minimum: 0 },
        nodes: { type: "array", items: { type: "string" } },
        to: { type: "string" },
      },
      required: ["confidence", "complete", "edges", "from", "length", "nodes", "to"],
      additionalProperties: false,
    },
  ],
});

process.stdout.write("mcp contracts generated\n");
