#!/usr/bin/env node
// Issue #33 contract gate: the 18 MCP schema artifacts, their exact
// identities, the diagnostic-registry union successor, and the embedded
// capability manifest, validated with the same pinned third-party Draft
// 2020-12 implementation as the other contract gates. Exact Ajv 8.20.0 is
// provisioned outside this checkout (CI does the same on Node 18 and 24)
// and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.

import "./registry-union.mjs";
import { assertRegistryUnion, ACCEPTED_M8_REGISTRY_FAMILIES } from "./lib/registry-union.mjs";
import { createRequire } from "node:module";
import { readdirSync, readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
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
    `ajv-unavailable: provision exact Ajv 8.20.0 outside the checkout and expose it through NODE_PATH / LEKALO_AJV_NODE_PATH (${error.message})\n`,
  );
  process.exit(2);
}
if (ajvVersion !== "8.20.0") {
  process.stderr.write(`ajv-version: expected 8.20.0, got ${ajvVersion}\n`);
  process.exit(2);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
execFileSync(process.execPath, [resolve(root, "scripts/check-registry-union.mjs"), "--selftest"], { cwd: root });
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));

const fail = (reason, detail) => {
  process.stderr.write(`mcp-contract ${reason}: ${JSON.stringify(detail ?? "")}\n`);
  process.exit(1);
};

// ---- Duplicate-key rejection (house rule; JSON.parse cannot see it). ------

function checkDuplicateKeys(text, name) {
  const stack = [];
  let inString = false;
  let escaped = false;
  let token = "";
  let currentKeyStart = -1;
  for (let index = 0; index < text.length; index++) {
    const character = text[index];
    if (inString) {
      if (escaped) escaped = false;
      else if (character === "\\") escaped = true;
      else if (character === '"') inString = false;
      continue;
    }
    if (character === '"') {
      inString = true;
      currentKeyStart = index;
      continue;
    }
    if (character === "{" || character === "[") {
      stack.push({ keys: new Map(), isObject: character === "{" });
      continue;
    }
    if (character === "}" || character === "]") {
      stack.pop();
      continue;
    }
    if (character === ":" && stack.length && stack[stack.length - 1].isObject) {
      // The key is the string that just closed.
      let start = currentKeyStart + 1;
      let end = index - 1;
      while (text[start] === '"') start++;
      while (end > start && text[end] !== '"') end--;
      const key = text.slice(start, end);
      const frame = stack[stack.length - 1];
      if (frame.keys.has(key)) {
        fail("duplicate-key", { file: name, key });
      }
      frame.keys.set(key, index);
    }
  }
}

// ---- Load the family. ------------------------------------------------------

const contractFiles = readdirSync(resolve(root, "contracts"))
  .filter((name) => name.startsWith("mcp") && name.endsWith(".v0.8.0.json"))
  .sort();
if (contractFiles.length !== 18) {
  fail("family-size", { expected: 18, found: contractFiles.length });
}

const ajv = new Ajv2020({ strict: true, allErrors: true });
const compiled = new Map();
for (const name of contractFiles) {
  const text = readFileSync(resolve(root, "contracts", name), "utf8");
  checkDuplicateKeys(text, name);
  const schema = JSON.parse(text);
  let validate;
  try {
    validate = ajv.compile(schema);
  } catch (error) {
    fail("schema-compile", { name, error: error.message });
  }
  compiled.set(name, { schema, validate });
}

// ---- Identity pins. -------------------------------------------------------

const TOOL_SLUGS = [
  "inspect",
  "impact",
  "context",
  "graph",
  "validate",
  "target-status",
  "trace-export",
];
const TOOL_NAMES = [
  "lekalo_inspect",
  "lekalo_impact",
  "lekalo_context",
  "lekalo_graph",
  "lekalo_validate",
  "lekalo_target_status",
  "lekalo_trace_export",
];
for (const slug of TOOL_SLUGS) {
  for (const kind of ["input", "output"]) {
    const name = `mcp-tool-${slug}-${kind}.schema.v0.8.0.json`;
    const entry = compiled.get(name);
    if (!entry) fail("missing-artifact", name);
    const expected = `dev.lekalo.mcp-tool-${slug}-${kind}@0.8.0`;
    if (entry.schema.$id !== expected) {
      fail("identity", { name, found: entry.schema.$id, expected });
    }
    // Closed root objects; no root unions and no bare root $ref.
    if (entry.schema.type !== "object") fail("root-type", name);
    if (entry.schema.additionalProperties !== false) fail("root-open", name);
    for (const keyword of ["oneOf", "anyOf", "allOf", "not", "$ref"]) {
      if (entry.schema[keyword] !== undefined) fail(`root-${keyword}`, name);
    }
  }
}
for (const [name, identity] of [
  ["mcp-capabilities.schema.v0.8.0.json", "dev.lekalo.mcp-capabilities@0.8.0"],
  ["mcp-server-config.schema.v0.8.0.json", "dev.lekalo.mcp-server-config@0.8.0"],
  ["mcp-resource.schema.v0.8.0.json", "dev.lekalo.mcp-resource@0.8.0"],
  ["mcp-graph-query.schema.v0.8.0.json", "dev.lekalo.mcp-graph-query@0.8.0"],
]) {
  if (compiled.get(name)?.schema.$id !== identity) fail("identity", { name, identity });
}

// ---- The diagnostic-registry union successor. -----------------------------

// The 0.8.0 allocation is the M8 grand union: every landed family
// (#94 format, #136 adoption, #19 portability, #51 worklog-changelog,
// #41 managed, #113 module-api, #52 tasklink) plus #33's own mcp.*
// additions — never a stale predecessor+own subset (r6 F6/omp F1:
// `registry065.length + 31` pinned a pre-merge state and went stale).
// The union guard mirrors `scripts/check-registry-union.mjs`: against
// every accessible live-line ref the registry keeps every base entry id
// and every base family, so a merge that drops a landed family fails
// this gate instead of waiting for review.
const registry065 = read("contracts/diagnostic-registry.v0.6.5.json");
const registry080 = read("contracts/diagnostic-registry.v0.8.0.json");
const registrySchema = read("contracts/diagnostic-registry.schema.v0.8.0.json");
const validateRegistry = ajv.compile(registrySchema);
if (!validateRegistry(registry080)) {
  fail("registry-instance-invalid", validateRegistry.errors);
}
if (registry080.schema_version !== "lekalo/diagnostic-registry/v0.8.0") {
  fail("registry-discriminator", registry080.schema_version);
}
// Sibling families evolve under their own gates. Preserve the complete
// live union through the shared guard and pin only this gate's MCP family.
assertRegistryUnion(registry065, registry080, {
  families: ACCEPTED_M8_REGISTRY_FAMILIES, ownPrefix: "mcp.", ownCount: 11,
});
// Live-line monotonicity and byte-identical predecessor entries are
// checked by the shared union guard above, not duplicated here.
const byId065 = new Map(registry065.entries.map((entry) => [entry.id, entry]));
for (const entry of registry080.entries) {
  if (byId065.has(entry.id)) {
    const predecessor = byId065.get(entry.id);
    for (const member of [
      "code",
      "category",
      "default_severity",
      "message_id",
      "default_message",
      "location_requirement",
      "lifecycle",
    ]) {
      if (JSON.stringify(entry[member]) !== JSON.stringify(predecessor[member])) {
        fail("predecessor-drift", { id: entry.id, member });
      }
    }
  }
}
const mcpCodes = registry080.entries
  .filter((entry) => entry.id.startsWith("mcp."))
  .map((entry) => entry.code);
if (mcpCodes.length !== 11) fail("mcp-family-size", mcpCodes);
const expectedCodes = Array.from({ length: 11 }, (_, index) =>
  `LEK-MCP-${String(index + 1).padStart(3, "0")}`,
);
if (JSON.stringify(mcpCodes.slice().sort()) !== JSON.stringify(expectedCodes)) {
  fail("mcp-codes", mcpCodes);
}
const statusOf = {
  "mcp.input-invalid": ["invalid"],
  "mcp.scope-denied": ["denied"],
  "mcp.sensitive-output": ["denied"],
  "mcp.request-limit": ["denied"],
  "mcp.output-limit": ["denied"],
  "mcp.timeout": ["unavailable"],
  "mcp.cancelled": ["unavailable"],
  "mcp.contract-unsupported": ["unsupported-version"],
  "mcp.snapshot-stale": ["unavailable"],
  "mcp.backend-unavailable": ["unavailable"],
  "mcp.result-invalid": ["invalid"],
};
for (const entry of registry080.entries) {
  if (!(entry.id in statusOf)) continue;
  const allowed = JSON.stringify(entry.allowed_statuses.slice().sort());
  const expected = JSON.stringify(statusOf[entry.id].slice().sort());
  if (allowed !== expected) {
    fail("mcp-status-vocabulary", { id: entry.id, allowed: entry.allowed_statuses });
  }
}
// Entries stay sorted by id and codes stay unique (the registry invariants).
const ids = registry080.entries.map((entry) => entry.id);
const sorted = [...ids].sort();
if (JSON.stringify(ids) !== JSON.stringify(sorted)) fail("registry-not-sorted");
if (ids.length !== new Set(ids).size) fail("registry-duplicate-ids");

// ---- Semantic cross-field rules Ajv cannot express. -----------------------

const validateInput = (slug, document) => {
  const entry = compiled.get(`mcp-tool-${slug}-input.schema.v0.8.0.json`);
  return entry.validate(document);
};

const semanticInputCases = [
  // impact: changed mode needs baseRef and exactly one of headRef/worktree.
  {
    slug: "impact",
    document: { projectId: "p", mode: "changed" },
    expect: "structural-valid-semantic-invalid",
  },
  {
    slug: "impact",
    document: { projectId: "p", mode: "changed", baseRef: "b", headRef: "h", worktree: true },
    expect: "structural-valid-semantic-invalid",
  },
  {
    slug: "impact",
    document: { projectId: "p", mode: "symbol" },
    expect: "structural-valid-semantic-invalid",
  },
  {
    slug: "impact",
    document: { projectId: "p", mode: "symbol", symbol: "m.s", baseRef: "b" },
    expect: "structural-valid-semantic-invalid",
  },
  // context: exactly one of symbol / nonempty changedSymbols per mode.
  {
    slug: "context",
    document: { projectId: "p", mode: "symbol", budget: 100 },
    expect: "structural-valid-semantic-invalid",
  },
  {
    slug: "context",
    document: {
      projectId: "p",
      mode: "symbol",
      symbol: "a",
      changedSymbols: ["b"],
      budget: 100,
    },
    expect: "structural-valid-semantic-invalid",
  },
  // graph: path needs from and to.
  {
    slug: "graph",
    document: { projectId: "p", mode: "path", from: "a" },
    expect: "structural-valid-semantic-invalid",
  },
];
for (const { slug, document, expect } of semanticInputCases) {
  const errors = validateInput(slug, document);
  if (expect === "structural-valid-semantic-invalid" && errors !== true) {
    // The schema must admit well-formed members and let the semantic
    // layer decide; a schema-level rejection here means the exclusivity
    // leaked into the host grammar.
    fail("semantic-rule-in-schema", { slug, document, errors });
  }
}

// Structural violations must be rejected by the schema itself.
const structuralCases = [
  { slug: "inspect", document: { projectId: "p" } },
  { slug: "inspect", document: { projectId: "p", symbol: "s", include: ["nope"] } },
  { slug: "inspect", document: { projectId: "p", symbol: "s", extra: 1 } },
  { slug: "inspect", document: { projectId: "../p", symbol: "s" } },
  { slug: "impact", document: { projectId: "p", mode: "write" } },
  { slug: "impact", document: { projectId: "p", mode: "symbol", symbol: "s", depth: 999 } },
  { slug: "context", document: { projectId: "p", mode: "symbol", symbol: "s" } },
  { slug: "context", document: { projectId: "p", mode: "symbol", symbol: "s", budget: 0 } },
  { slug: "context", document: { projectId: "p", mode: "changed", changedSymbols: [], budget: 1 } },
  { slug: "graph", document: { projectId: "p", mode: "teleport" } },
  { slug: "validate", document: { projectId: "p", module: "../etc" } },
  { slug: "target-status", document: { projectId: "p", targetId: "Not_Valid" } },
  { slug: "trace-export", document: { projectId: "p", manifestRef: "/abs/path" } },
];
for (const { slug, document } of structuralCases) {
  const errors = validateInput(slug, document);
  if (errors !== true) continue;
  fail("structural-violation-accepted", { slug, document });
}

// Every minimal request validates.
const minimalCases = [
  ["inspect", { projectId: "p", symbol: "m.s" }],
  ["impact", { projectId: "p", mode: "symbol", symbol: "m.s" }],
  ["impact", { projectId: "p", mode: "changed", baseRef: "b", worktree: true }],
  ["context", { projectId: "p", mode: "symbol", symbol: "m.s", budget: 100 }],
  ["context", { projectId: "p", mode: "changed", changedSymbols: ["a"], budget: 10 }],
  ["graph", { projectId: "p", mode: "export" }],
  ["validate", { projectId: "p" }],
  ["target-status", { projectId: "p" }],
  ["trace-export", { projectId: "p", manifestRef: "m" }],
];
for (const [slug, document] of minimalCases) {
  if (validateInput(slug, document) !== true) {
    fail("minimal-request-rejected", { slug, document });
  }
}


// ---- Closed valid output envelopes validate per tool. ----------------------

// The embedded core payloads are described by their exact bundled upstream
// schemas now, so the gate validates REAL recorded golden instances (the
// same ones the per-family contract gates pin); a fabricated member can
// no longer satisfy the published result branches.
const golden = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));
const graphGolden = golden("tests/fixtures/graph/golden/planner.graph.json");
const graphShow = {
  node: graphGolden.nodes.find((node) => node.id === "operation:planner.focus_task"),
  dependencies: graphGolden.edges.filter((edge) => edge.from === "operation:planner.focus_task"),
  dependents: graphGolden.edges.filter((edge) => edge.to === "operation:planner.focus_task"),
};
if (!graphShow.node) fail("graph-show-fixture", graphGolden.nodes.slice(0, 3));
const outputEnvelope = (tool, slug, status, extra) => ({
  schemaVersion: `lekalo/mcp-tool-${slug}-output/v0.8.0`,
  identity: `dev.lekalo.mcp-tool-${slug}-output@0.8.0`,
  tool,
  scope: { projectId: "planner", groupId: null },
  snapshotRef: `sha256-${"a".repeat(64)}`,
  status,
  contractPins: {
    diagnostic: "0.2.16",
    diagnosticRegistry: "0.8.0",
    protocolProfile: "2025-11-25",
    ...extra.pins,
  },
  diagnostics: [],
  ...(status === "valid" ? { result: extra.result } : { failure: { failureClass: extra.failureClass } }),
});
const outputPins = {
  inspect: "inspect",
  impact: "impact",
  context: "context",
  graph: "graph",
  validate: "validation",
  "target-status": "doctor",
  "trace-export": "traceManifest",
};
const resultShape = {
  inspect: {
    schemaVersion: "lekalo/inspect/v0.8.0",
    identity: "dev.lekalo.inspect@0.8.0",
    // The pinned golden is the FROZEN 0.2.16 envelope; the live output
    // carries the 0.8.0 successor discriminator (#52 convention). This
    // legacy-source golden keeps its payload bytes; only its version
    // identity is patched to the successor spelling before validation.
    inspect: (() => {
      const receipt = golden("tests/fixtures/inspect/golden/planner.focus_task.json").inspect;
      return {
        ...receipt,
        schemaVersion: "lekalo/inspect/v0.8.0",
        identity: "dev.lekalo.inspect@0.8.0",
      };
    })(),
  },
  impact: {
    schemaVersion: "lekalo/impact/v0.2.16",
    identity: "dev.lekalo.impact@0.2.16",
    impact: golden("tests/fixtures/impact/golden/planner.impact.json"),
  },
  context: {
    schemaVersion: "lekalo/context/v0.2.16",
    identity: "dev.lekalo.context@0.2.16",
    context: golden("tests/fixtures/context/golden/planner.context.json"),
  },
  graph: { mode: "export", export: graphGolden },
  validate: {
    ...golden("tests/fixtures/validation/golden/valid-base-strict-envelope.json"),
  },
  "target-status": {
    schemaVersion: "lekalo/doctor/v0.3.2",
    identity: "dev.lekalo.doctor@0.3.2",
    doctor: golden("tests/fixtures/doctor/status.golden.json"),
  },
  "trace-export": {
    schemaVersion: "lekalo/trace-manifest/v0.2.16",
    identity: "dev.lekalo.trace-manifest@0.2.16",
    trace: golden("tests/fixtures/trace/golden/planner.trace.json"),
    manifestDigest: readFileSync(
      resolve(root, "tests/fixtures/trace/golden/planner.trace.sha256"),
      "utf8",
    ).trim(),
  },
};
for (const slug of TOOL_SLUGS) {
  const entry = compiled.get(`mcp-tool-${slug}-output.schema.v0.8.0.json`);
  const tool = TOOL_NAMES[TOOL_SLUGS.indexOf(slug)];
  const valid = outputEnvelope(tool, slug, "valid", {
    pins: { [outputPins[slug]]: outputPinVersion(slug) },
    result: resultShape[slug],
  });
  if (entry.validate(valid) !== true) {
    fail("valid-output-rejected", { slug, errors: entry.validate(valid) });
  }
  const refused = outputEnvelope(tool, slug, "denied", {
    pins: { [outputPins[slug]]: outputPinVersion(slug) },
    failureClass: "scope-denied",
  });
  if (entry.validate(refused) !== true) {
    fail("refusal-output-rejected", { slug, errors: entry.validate(refused) });
  }
}
function outputPinVersion(slug) {
  return {
    inspect: "0.8.0",
    impact: "0.2.16",
    context: "0.2.16",
    graph: "0.2.16",
    validate: "0.6.4",
    "target-status": "0.3.2",
    "trace-export": "0.2.16",
  }[slug];
}

// The status/result|failure coherence rule is published now: a forged
// member on the wrong branch must fail host validation (r2 F11).
const incoherent = {
  ...outputEnvelope(
    TOOL_NAMES[0],
    "inspect",
    "denied",
    { pins: { inspect: "0.8.0" }, failureClass: "scope-denied" },
  ),
  result: resultShape.inspect,
};
if (compiled.get("mcp-tool-inspect-output.schema.v0.8.0.json").validate(incoherent) !== false) {
  fail("schema-must-reject-refusal-with-result");
}
const validWithoutResult = outputEnvelope(TOOL_NAMES[0], "inspect", "valid", {
  pins: { inspect: "0.8.0" },
  result: undefined,
});
delete validWithoutResult.result;
if (compiled.get("mcp-tool-inspect-output.schema.v0.8.0.json").validate(validWithoutResult) !== false) {
  fail("schema-must-reject-valid-without-result");
}

// Hostile nested tampering (r2 F11 / codex 8): the bundled upstream
// closures reject forged payloads and forged diagnostic items.
const inspectSchema = compiled.get("mcp-tool-inspect-output.schema.v0.8.0.json");
const forgedResult = outputEnvelope(TOOL_NAMES[0], "inspect", "valid", {
  pins: { inspect: "0.8.0" },
  result: {
    schemaVersion: "lekalo/inspect/v0.8.0",
    identity: "dev.lekalo.inspect@0.8.0",
    inspect: { attackerExtra: "unconstrained" },
  },
});
if (inspectSchema.validate(forgedResult) !== false) {
  fail("schema-must-reject-forged-payload");
}
const forgedDiagnostics = {
  ...outputEnvelope(TOOL_NAMES[0], "inspect", "denied", {
    pins: { inspect: "0.8.0" },
    failureClass: "scope-denied",
  }),
  diagnostics: [{ anything: "not-a-diagnostic" }],
};
if (inspectSchema.validate(forgedDiagnostics) !== false) {
  fail("schema-must-reject-forged-diagnostics");
}
// A real diagnostic item ships through the closed items schema.
const realDiagnostic = golden(
  "tests/fixtures/validation/golden/valid-base-strict-envelope.json",
);
const diagnosticRegistry = read("contracts/diagnostic-registry.v0.8.0.json");
const diagnosticSample = (realDiagnostic.diagnostics ?? [])[0];
if (diagnosticSample) {
  const withDiagnostic = {
    ...outputEnvelope(TOOL_NAMES[5], "validate", "valid", {
      pins: { validation: "0.6.4" },
      result: resultShape.validate,
    }),
    diagnostics: [diagnosticSample],
  };
  const validateSchema = compiled.get("mcp-tool-validate-output.schema.v0.8.0.json");
  if (validateSchema.validate(withDiagnostic) !== true) {
    fail("real-diagnostic-rejected", validateSchema.validate(withDiagnostic));
  }
}
if (diagnosticRegistry.entries.length === 0) fail("registry-empty");

// The graph show query receipt validates against the bundled defs.
const graphOutput = compiled.get("mcp-tool-graph-output.schema.v0.8.0.json");
const graphShowEnvelope = outputEnvelope(TOOL_NAMES[3], "graph", "valid", {
  pins: { graph: "0.2.16" },
  result: { mode: "show", query: graphShow },
});
if (graphOutput.validate(graphShowEnvelope) !== true) {
  fail("graph-show-receipt-rejected", graphOutput.validate(graphShowEnvelope));
}

// The status/result|failure coherence semantic validator (server parity).
function semanticOutput(slug, document) {
  const errors = [];
  if (document.status === "valid") {
    if (!("result" in document)) errors.push("valid-without-result");
    if ("failure" in document) errors.push("valid-with-failure");
  } else {
    if (!("failure" in document)) errors.push("refusal-without-failure");
    if ("result" in document) errors.push("refusal-with-result");
    if (document.failure && !document.failure.failureClass) {
      errors.push("failure-without-class");
    }
  }
  return errors;
}
if (semanticOutput("inspect", incoherent).length === 0) {
  fail("semantic-validator-missed-incoherence");
}

// ---- The capabilities manifest validates against its own schema. ----------

// The manifest the server embeds is produced by the Rust binary; the
// stdio gate validates the live `mcp describe` output. Here the schema
// itself must admit the exact seven-name/five-family vocabulary.
const capabilities = compiled.get("mcp-capabilities.schema.v0.8.0.json");
const manifestShape = {
  schemaVersion: "lekalo/mcp-capabilities/v0.8.0",
  identity: "dev.lekalo.mcp-capabilities@0.8.0",
  protocol: { name: "mcp", transport: "stdio", profile: "2025-11-25", frameDelimiter: "lf" },
  server: { name: "lekalo", readOnly: true },
  limits: {
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
  },
  tools: TOOL_NAMES.map((name, index) => ({
    name,
    title: `Lekalo ${TOOL_SLUGS[index]}`,
    input: {
      identity: `dev.lekalo.mcp-tool-${TOOL_SLUGS[index]}-input@0.8.0`,
      discriminator: `lekalo/mcp-tool-${TOOL_SLUGS[index]}-input/v0.8.0`,
    },
    output: {
      identity: `dev.lekalo.mcp-tool-${TOOL_SLUGS[index]}-output@0.8.0`,
      discriminator: `lekalo/mcp-tool-${TOOL_SLUGS[index]}-output/v0.8.0`,
    },
  })),
  resources: [
    "project-summary",
    "module-list",
    "symbol-metadata",
    "target-status",
    "diagnostics",
  ].map((kind) => ({
    kind,
    uriTemplate: `lekalo://{projectId}/${kind === "project-summary" ? "summary" : kind === "module-list" ? "modules" : kind === "symbol-metadata" ? "symbol/{symbol}" : kind === "target-status" ? "targets" : "diagnostics"}`,
    description: "closed family",
  })),
  pins: {
    diagnostic: "lekalo/diagnostic/v0.2.16",
    diagnosticRegistry: "dev.lekalo.diagnostic-registry@0.8.0",
    serverConfig: "dev.lekalo.mcp-server-config@0.8.0",
    graphQuery: "dev.lekalo.mcp-graph-query@0.8.0",
  },
  diagnosticCodes: expectedCodes,
};
if (capabilities.validate(manifestShape) !== true) {
  fail("capabilities-shape-rejected", capabilities.validate(manifestShape).errors);
}

// ---- The resource envelope and the graph-query describing schema. ----------

const resource = compiled.get("mcp-resource.schema.v0.8.0.json");
const resourceEnvelope = {
  schemaVersion: "lekalo/mcp-resource/v0.8.0",
  identity: "dev.lekalo.mcp-resource@0.8.0",
  kind: "project-summary",
  // The served URI spelling: `lekalo://PROJECT/family` (the gate example
  // uses the same `build_uri` projection the server emits).
  uri: "lekalo://planner/summary",
  scope: { projectId: "planner", groupId: null },
  revision: `sha256-${"c".repeat(64)}`,
  data: {
    project: "planner",
    modelVersion: "0.2.16",
    moduleCount: 3,
    definitionCount: 13,
    unknownStates: [],
  },
};
if (resource.validate(resourceEnvelope) !== true) {
  fail("resource-envelope-rejected", resource.validate(resourceEnvelope).errors);
}

// Every spellable resource URI family validates, including the symbol
// qualifier with literal dots and percent escapes and the group-qualified
// prefix (r2 F10 / codex 9). The old frozen pattern could not admit its
// own symbol URIs; the amended pattern must admit every family live.
const resourceUri = (uri, kind, data) =>
  resource.validate({
    ...resourceEnvelope,
    kind,
    uri,
    data,
  });
const symbolDocument = {
  schemaVersion: "lekalo/inspect/v0.8.0",
  identity: "dev.lekalo.inspect@0.8.0",
  // The frozen 0.2.16 golden patched to the 0.8.0 successor spelling
  // (#52 convention, as in resultShape above).
  inspect: (() => {
    const receipt = golden("tests/fixtures/inspect/golden/planner.focus_task.json").inspect;
    return { ...receipt, schemaVersion: "lekalo/inspect/v0.8.0", identity: "dev.lekalo.inspect@0.8.0" };
  })(),
};
for (const [uri, kind, data] of [
  ["lekalo://planner/summary", "project-summary", resourceEnvelope.data],
  ["lekalo://planner/modules", "module-list", { modules: [{ id: "planner", definitionCount: 7 }] }],
  ["lekalo://planner/symbol/planner.focus_task", "symbol-metadata", symbolDocument],
  ["lekalo://planner/symbol/planner%2Efocus_task", "symbol-metadata", symbolDocument],
  ["lekalo://planner/g/planner-review/summary", "project-summary", resourceEnvelope.data],
  ["lekalo://planner/g/planner-review/symbol/notify.send_digest", "symbol-metadata", symbolDocument],
  ["lekalo://planner/targets", "target-status", {
    schemaVersion: "lekalo/doctor/v0.3.2",
    identity: "dev.lekalo.doctor@0.3.2",
    doctor: golden("tests/fixtures/doctor/status.golden.json"),
  }],
  ["lekalo://planner/diagnostics", "diagnostics", {
    diagnosticCount: 0,
    evidenceCount: 0,
    complete: true,
    reasons: [],
  }],
]) {
  if (resourceUri(uri, kind, data) !== true) {
    fail("resource-uri-rejected", { uri, errors: resource.validate({ ...resourceEnvelope, kind, uri, data }) });
  }
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  ajv: ajvVersion,
  artifacts: contractFiles.length,
  tools: TOOL_NAMES.length,
  registryEntries: registry080.entries.length,
  mcpCodes: mcpCodes.length,
  semanticCases: semanticInputCases.length + structuralCases.length,
}, null, 2)}\n`);
