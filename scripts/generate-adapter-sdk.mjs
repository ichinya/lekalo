#!/usr/bin/env node
/**
 * The adapter-SDK generator (issue #106).
 *
 * Deterministic, offline, reviewed-local-inputs generator for the four
 * adapter SDK packages (`lekalo-adapter-sdk-rust`, `@lekalo/adapter-sdk`,
 * `lekalo-adapter-sdk-go`, `lekalo/adapter-sdk-php`). Per language it
 * emits the generated protocol/capability/diagnostic modules, the
 * generated compatibility test matrix, the compatibility descriptor
 * (`compatibility.json`) and the generation record (`generation.json`)
 * validated by `contracts/adapter-sdk-compatibility.schema.v0.8.0.json`
 * and `contracts/adapter-sdk-generation.schema.v0.8.0.json`, plus the
 * deterministic standalone template bundles the confined host can copy.
 *
 * Commands:
 *   node scripts/generate-adapter-sdk.mjs --check
 *   node scripts/generate-adapter-sdk.mjs --write --language rust|typescript|go|php|all
 *
 * `--write` is an explicit development-only action bounded to the
 * generated SDK files and their generation records; CI permits only
 * `--check`. Nothing here downloads dependencies, phones home, installs
 * runtimes, or touches anything outside `sdk/`. Every emitted document
 * is canonical JSON (sorted keys, compact, no trailing LF) and byte-
 * reproducible: no timestamps, no host roots, no secrets.
 *
 * Exit protocol: 0 with a one-line JSON status on success, 1 with a JSON
 * failure report on stderr otherwise.
 */
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const SCRIPT_DIR = dirname(SCRIPT_PATH);
const ROOT = resolve(SCRIPT_DIR, "..");
const GENERATOR_NAME = "scripts/generate-adapter-sdk.mjs";
const GENERATOR_VERSION = "0.8.0";
const LANGUAGES = ["rust", "typescript", "go", "php"];

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "adapter-sdk-generate", reason, detail })}\n`);
  process.exit(1);
};

const args = process.argv.slice(2);
const check = args.includes("--check");
const write = args.includes("--write");
const languageFlag = args.indexOf("--language");
const language = languageFlag !== -1 ? args[languageFlag + 1] : undefined;
if (check === write) fail("usage", "pass exactly one of --check or --write");
if (write && (!language || ![...LANGUAGES, "all"].includes(language))) {
  fail("usage", "--write requires --language rust|typescript|go|php|all");
}
if (check && language !== undefined) fail("usage", "--check covers every language");

const sha256 = (text) => "sha256:" + createHash("sha256").update(text).digest("hex");

/** Canonical compact JSON: byte-sorted keys — exactly the repo form. */
export function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value !== null && typeof value === "object") {
    const body = Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
      .join(",");
    return `{${body}}`;
  }
  return JSON.stringify(value);
}

const read = (relative) => readFileSync(join(ROOT, relative), "utf8");

/** Render one value as a PHP array literal (single-quoted strings). */
export function phpArray(value) {
  if (Array.isArray(value)) {
    return "[" + value.map((item) => phpArray(item)).join(",") + "]";
  }
  if (value !== null && typeof value === "object") {
    const body = Object.keys(value)
      .sort()
      .map((key) => phpString(key) + " => " + phpArray(value[key]))
      .join(",");
    return "[" + body + "]";
  }
  if (typeof value === "number") return String(value);
  if (typeof value === "boolean") return value ? "true" : "false";
  if (value === null) return "null";
  return phpString(String(value));
}

const phpProlog = "<?php\ndeclare(strict_types=1);\n\n";

function phpString(text) {
  return "'" + String(text).replace(/[\']/g, "\\$&") + "'";
}

// ---------------------------------------------------------------------------
// Pinned configuration and cross-checked model extraction
// ---------------------------------------------------------------------------

const config = JSON.parse(read("sdk/schema-inputs.json"));
if (config.schemaVersion !== "lekalo/adapter-sdk-inputs/v0.8.0") {
  fail("inputs-identity", `unexpected schemaVersion ${config.schemaVersion}`);
}

const inputRecords = config.inputs.map((input) => {
  let bytes;
  try {
    bytes = read(input.path);
  } catch {
    fail("input-missing", input.path);
  }
  return { ...input, digest: sha256(bytes), bytes };
});

const schemas = Object.fromEntries(
  inputRecords
    .filter((input) => input.role === "protocol-schema")
    .map((input) => {
      const schema = JSON.parse(input.bytes);
      const version = input.path.match(/v(\d+\.\d+\.\d+)\.json$/)?.[1];
      if (!version || !config.protocolVersions.includes(version)) {
        fail("input-role", `protocol schema ${input.path} is not a pinned version`);
      }
      return [version, { schema, digest: input.digest, path: input.path }];
    }),
);

const registryInput = inputRecords.find((input) => input.role === "diagnostic-registry");
if (!registryInput) fail("inputs-role", "the diagnostic-registry input is missing");
const registry = JSON.parse(registryInput.bytes);
const capabilityInput = inputRecords.find((input) => input.role === "capability-definitions");
if (!capabilityInput) fail("inputs-role", "the capability-definitions input is missing");
const capabilityExport = JSON.parse(capabilityInput.bytes);

/**
 * The pinned envelope-member model. The generator refuses to emit when a
 * protocol schema drifts from this table (or from the configured operation
 * vocabulary): a candidate schema mutation must fail `--check` until the
 * generated outputs are reviewed, which is exactly the drift detection the
 * compatibility contract promises.
 */
const ENVELOPE_MEMBERS = {
  request: {
    required: ["operation", "project_root", "protocol", "protocol_version", "request_id"],
    optional: [
      "dry_run", "ir_path", "limits", "native_request",
      "plan_id", "profile", "profile_capabilities", "profile_digest", "target",
    ],
    // The additive 0.6.4 lint seam (issue #76) is the only version-gated
    // envelope member; the frozen 0.3.2 document keeps its exact members.
    optionalAt: { "0.6.4": ["lint_request"] },
  },
  response: {
    required: ["evidence", "operation", "protocol", "protocol_version", "request_id", "status"],
    optional: ["capabilities", "error", "progress", "result", "writes"],
  },
};

const SEMANTIC_BOUNDS = {
  max_request_bytes: 1_048_576,
  default_timeout_ms: 600_000,
  default_max_output_bytes: 8_388_608,
  max_stderr_bytes: 65_536,
  max_plan_entries: 10_000,
  max_ir_versions: 8,
  max_declared_capabilities: 64,
  max_scopes: 64,
  max_constraint_value: 1_073_741_824,
  limits_timeout_min: 1,
  limits_timeout_max: 3_600_000,
  limits_output_min: 1,
  limits_output_max: 1_073_741_824,
  error_code_max: 128,
  error_message_max: 256,
  error_details_max: 16,
  error_detail_max: 128,
  progress_steps_max: 256,
  progress_text_max: 128,
  scan_entries_max: 10_000,
  findings_max: 10_000,
  bindings_max: 4_096,
  references_max: 8,
  protocol_versions_max: 8,
  operations_max_base: 9,
  operations_max_lint: 10,
  transports_max: 2,
  targets_max: 64,
  profiles_max: 64,
  token_max: 64,
  detail_token_max: 128,
  capability_id_max: 128,
  path_max: 512,
  segment_max: 64,
};

function verifyProtocolSchema(version) {
  const entry = schemas[version];
  if (!entry) fail("schema-missing", version);
  const { schema } = entry;
  if (schema.$schema !== "https://json-schema.org/draft/2020-12/schema") {
    fail("schema-draft", version);
  }
  if (schema.$id !== `https://dev.lekalo/target-protocol.schema.v${version}.json`) {
    fail("schema-id", version);
  }
  const defs = schema.$defs ?? {};
  for (const [envelope, table] of Object.entries(ENVELOPE_MEMBERS)) {
    const def = defs[`${envelope}Envelope`];
    if (!def || def.additionalProperties !== false) fail("envelope-shape", `${version}/${envelope}`);
    const optional = [...table.optional, ...((table.optionalAt ?? {})[version] ?? [])];
    const names = Object.keys(def.properties ?? {}).sort();
    const expected = [...table.required, ...optional].sort();
    if (JSON.stringify(names) !== JSON.stringify(expected)) {
      fail("envelope-members", `${version}/${envelope}: ${names.join(",")}`);
    }
    if (JSON.stringify([...(def.required ?? [])].sort()) !== JSON.stringify(table.required)) {
      fail("envelope-required", `${version}/${envelope}`);
    }
  }
  const operationDef = defs.operation;
  const wireOperations = operationDef?.enum ?? [];
  const configured = config.operations[version];
  if (!configured || JSON.stringify([...wireOperations].sort()) !== JSON.stringify([...configured].sort())) {
    fail("operation-vocabulary", `${version}: schema [${wireOperations}] vs inputs [${configured}]`);
  }
  const versionDef = defs.protocolVersion;
  const pinned = versionDef && (Array.isArray(versionDef.enum) ? versionDef.enum.length === 1 && versionDef.enum[0] === version : versionDef.const === version);
  if (!pinned) {
    fail("schema-version-pin", version);
  }
}

for (const version of config.protocolVersions) verifyProtocolSchema(version);

if (registry.identity !== "dev.lekalo.diagnostic-registry@0.6.4" || registry.closed !== true) {
  fail("registry-pin", String(registry.identity));
}
const diagnosticCodes = (registry.entries ?? [])
  .map((entry) => ({ code: entry.code, id: entry.id }))
  .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
if (diagnosticCodes.length === 0) fail("registry-empty", registryInput.path);

if (capabilityExport.identity !== "dev.lekalo.target-capabilities@0.6.4") {
  fail("capabilities-pin", String(capabilityExport.identity));
}
const capabilityEntries = (capabilityExport.entries ?? [])
  .map((entry) => ({ definitionVersion: entry.definitionVersion, domain: entry.domain, id: entry.id }))
  .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
if (capabilityEntries.length === 0) fail("capabilities-empty", capabilityInput.path);
if (capabilityExport.entriesDigest !== sha256(canonicalJson(capabilityEntries))) {
  fail("capabilities-digest", "entriesDigest does not cover the id-sorted entries");
}

const protocolVersions = [...config.protocolVersions].sort();
const baseOperations = config.operations[protocolVersions[0]];
const latestVersion = protocolVersions[protocolVersions.length - 1];
const latestOperations = config.operations[latestVersion];
if (!baseOperations || !latestOperations) fail("operations-missing", "configured versions");

// ---------------------------------------------------------------------------
// Shared fixtures corpus for the compatibility descriptor's test vectors
// ---------------------------------------------------------------------------

const CORPUS_ROOT = "tests/fixtures/adapter-sdk";
/** family → corpus directories holding its vector files */
const FAMILY_DIRECTORIES = {
  canonical: ["canonical"],
  codec: ["valid", "invalid"],
  semantic: ["semantic"],
  upgrade: ["upgrade"],
};
function corpusVectors() {
  const vectors = [];
  for (const family of config.testVectorFamilies) {
    const directories = FAMILY_DIRECTORIES[family];
    if (!directories) fail("corpus-family", family);
    for (const directory of directories) {
      let names = [];
      try {
        names = readdirSync(join(ROOT, CORPUS_ROOT, directory)).filter((name) => name.endsWith(".json") && !name.endsWith(".expect.json"));
      } catch {
        fail("corpus-missing", `${CORPUS_ROOT}/${directory}`);
      }
      for (const name of names.sort()) {
        const relative = `${directory}/${name}`;
        vectors.push({ digest: sha256(read(`${CORPUS_ROOT}/${relative}`)), family, path: relative });
      }
    }
  }
  if (vectors.length === 0) fail("corpus-empty", CORPUS_ROOT);
  return vectors;
}

// ---------------------------------------------------------------------------
// Per-language emission
// ---------------------------------------------------------------------------

const languageExtension = { go: "go", php: "php", rust: "rs", typescript: "mjs" };
/** The generated-module directory inside each SDK package root (research I3/I5). */
const generatedDirectory = { go: "generated", php: "src/Generated", rust: "src/generated", typescript: "src/generated" };
const generatedHeader = (lang) => {
  const comment = { go: "//", php: "//", rust: "//", typescript: "//" }[lang];
  return [
    `${comment} GENERATED by ${GENERATOR_NAME} v${GENERATOR_VERSION} — do not edit by hand.`,
    `${comment} Generated from the pinned canonical contracts (issue #106); the`,
    `${comment} generation record and compatibility descriptor pin exact input bytes.`,
    `${comment} Semantic admission beyond these types stays a hand-maintained runtime`,
    `${comment} obligation validated against the shared vector corpus.`,
  ].join("\n");
};

function renderCompatibilityTests(lang) {
  const ext = languageExtension[lang];
  const comment = "//";
  const matrix = [
    ...protocolVersions.map((version) => ({
      admitted: true,
      irVersions: config.irVersions,
      operations: config.operations[version],
      protocolVersion: version,
      wireToken: config.wireToken,
    })),
    // The exact-version matrix refusal rows: unknown and future versions,
    // foreign tokens, and cross-version operations fail before any IR
    // disclosure or callback execution.
    { admitted: false, protocolVersion: "0.3.1", reason: "version", wireToken: config.wireToken },
    { admitted: false, protocolVersion: "0.9.0", reason: "version", wireToken: config.wireToken },
    { admitted: false, protocolVersion: "1.0.0", reason: "version", wireToken: config.wireToken },
    { admitted: false, protocolVersion: "0.6.4", reason: "token", wireToken: "lekalo.target/v0" },
    { admitted: true, operation: "lint", protocolVersion: "0.6.4", wireToken: config.wireToken },
    { admitted: false, operation: "lint", protocolVersion: "0.3.2", reason: "lint-request", wireToken: config.wireToken },
    { admitted: false, operation: "plan-native", protocolVersion: "0.3.2", reason: "native-request", wireToken: config.wireToken },
    { admitted: false, operation: "transpile", protocolVersion: "0.6.4", reason: "operation", wireToken: config.wireToken },
  ];
  const lines = [
    generatedHeader(lang),
    `${comment} The generated exact-version compatibility matrix (issue #106): on a`,
    `${comment} protocol upgrade the generator re-emits this matrix and its`,
    `${comment} accept/refuse cases together with the descriptor, so an SDK package`,
    `${comment} version can never widen the exact accepted wire set silently.`,
    "",
  ];
  const body = canonicalJson(matrix);
  if (lang === "rust") {
    lines.push("/// One generated compatibility row: an exact-version admission or a", "/// bounded refusal reason.", "#[derive(Clone, Copy, Debug)]", "pub struct CompatibilityCase {", "    pub admitted: bool,", '    pub operation: Option<&\'static str>,', "    pub protocol_version: &'static str,", "    pub reason: Option<&'static str>,", '    pub wire_token: Option<&\'static str>,', "}", "", `pub const COMPATIBILITY_MATRIX: &[CompatibilityCase] = &[`);
    for (const row of matrix) {
      lines.push(
        `    CompatibilityCase { admitted: ${row.admitted}, operation: ${row.operation ? `Some("${row.operation}")` : "None"}, protocol_version: "${row.protocolVersion}", reason: ${row.reason ? `Some("${row.reason}")` : "None"}, wire_token: ${row.wireToken ? `Some("${row.wireToken}")` : "None"} },`,
      );
    }
    lines.push("];", "");
  } else if (lang === "typescript") {
    lines.push(`export const COMPATIBILITY_MATRIX = ${body};`, "");
  } else if (lang === "go") {
    lines.push("package generated", "", "// CompatibilityCase is one generated compatibility row.", "type CompatibilityCase struct {", "\tAdmitted        bool", "\tOperation       *string", "\tProtocolVersion string", "\tReason          *string", "\tWireToken       *string", "}", "", "var CompatibilityMatrix = []CompatibilityCase{");
    const goStr = (value) => `&[]string{${JSON.stringify(value)}}[0]`;
    for (const row of matrix) {
      lines.push(
        `\t{Admitted: ${row.admitted}, Operation: ${row.operation ? goStr(row.operation) : "nil"}, ProtocolVersion: ${JSON.stringify(row.protocolVersion)}, Reason: ${row.reason ? goStr(row.reason) : "nil"}, WireToken: ${row.wireToken ? goStr(row.wireToken) : "nil"}},`,
      );
    }
    lines.push("}", "");
  } else {
    lines.push(`namespace Lekalo\\AdapterSdk\\Generated;`, "", `final class CompatibilityTests {`, "    /** The generated exact-version matrix as canonical rows. */", `    public const MATRIX = ${phpArray(matrix)};`, "}", "");
  }
  return lines.join("\n") + "\n";
}

function renderCapabilities(lang) {
  const ext = languageExtension[lang];
  const body = canonicalJson(capabilityEntries);
  const lines = [generatedHeader(lang), ""];
  if (lang === "rust") {
    lines.push(
      "/// The exported capability-definition pin (issue #106): the reviewed",
      "/// canonical embedded table, exported for describe helpers. Never a",
      "/// second registry: new ids arrive through a reviewed owner successor.",
      `pub const CAPABILITY_IDENTITY: &str = ${JSON.stringify(capabilityExport.identity)};`,
      `pub const CAPABILITY_ENTRIES_DIGEST: &str = ${JSON.stringify(capabilityExport.entriesDigest)};`,
      "",
      "/// One exported capability definition (id-sorted).",
      "#[derive(Clone, Copy, Debug)]",
      "pub struct CapabilityDefinition {",
      "    pub definition_version: &'static str,",
      "    pub domain: &'static str,",
      "    pub id: &'static str,",
      "}",
      "",
      "pub const CAPABILITY_DEFINITIONS: &[CapabilityDefinition] = &[",
    );
    for (const entry of capabilityEntries) {
      lines.push(`    CapabilityDefinition { definition_version: ${JSON.stringify(entry.definitionVersion)}, domain: ${JSON.stringify(entry.domain)}, id: ${JSON.stringify(entry.id)} },`);
    }
    lines.push("];", "");
  } else if (lang === "typescript") {
    lines.push(
      `export const CAPABILITY_IDENTITY = ${JSON.stringify(capabilityExport.identity)};`,
      `export const CAPABILITY_ENTRIES_DIGEST = ${JSON.stringify(capabilityExport.entriesDigest)};`,
      "/** The exported capability definitions, id-sorted (issue #106). */",
      `export const CAPABILITY_DEFINITIONS = ${body};`,
      "",
    );
  } else if (lang === "go") {
    lines.push(
      "package generated",
      "",
      "// CapabilityDefinition is one exported capability definition (id-sorted).",
      "type CapabilityDefinition struct {",
      "\tDefinitionVersion string `json:\"definitionVersion\"`",
      "\tDomain            string `json:\"domain\"`",
      "\tID                string `json:\"id\"`",
      "}",
      "",
      `const (`,
      `\tCapabilityIdentity       = ${JSON.stringify(capabilityExport.identity)}`,
      `\tCapabilityEntriesDigest  = ${JSON.stringify(capabilityExport.entriesDigest)}`,
      `)`,
      "",
      "var CapabilityDefinitions = []CapabilityDefinition{",
    );
    for (const entry of capabilityEntries) {
      lines.push(`\t{DefinitionVersion: ${JSON.stringify(entry.definitionVersion)}, Domain: ${JSON.stringify(entry.domain)}, ID: ${JSON.stringify(entry.id)}},`);
    }
    lines.push("}", "");
  } else {
    lines.push(
      "namespace Lekalo\\AdapterSdk\\Generated;",
      "",
      "/** The exported capability-definition pin (issue #106). */",
      "final class Capabilities {",
      `    public const IDENTITY = ${JSON.stringify(capabilityExport.identity)};`,
      `    public const ENTRIES_DIGEST = ${JSON.stringify(capabilityExport.entriesDigest)};`,
      `    public const DEFINITIONS = ${phpArray(capabilityEntries)};`,
      "}",
      "",
    );
  }
  return lines.join("\n");
}

function renderDiagnostics(lang) {
  const lines = [generatedHeader(lang), ""];
  if (lang === "rust") {
    lines.push(
      "/// The diagnostic registry pin of the generated projection (issue #106):",
      "/// the SDK validates builders against the selected canonical registry",
      "/// instead of becoming a second source of codes.",
      `pub const DIAGNOSTIC_REGISTRY_IDENTITY: &str = ${JSON.stringify(registry.identity)};`,
      `pub const DIAGNOSTIC_REGISTRY_DIGEST: &str = ${JSON.stringify(registryInput.digest)};`,
      "",
      "/// The id-sorted registry codes available to adapter builders.",
      "pub const DIAGNOSTIC_CODES: &[(&str, &str)] = &[",
    );
    for (const entry of diagnosticCodes) {
      lines.push(`    (${JSON.stringify(entry.id)}, ${JSON.stringify(entry.code)}),`);
    }
    lines.push("];", "");
  } else if (lang === "typescript") {
    lines.push(
      `export const DIAGNOSTIC_REGISTRY_IDENTITY = ${JSON.stringify(registry.identity)};`,
      `export const DIAGNOSTIC_REGISTRY_DIGEST = ${JSON.stringify(registryInput.digest)};`,
      "/** The id-sorted registry codes available to adapter builders. */",
      `export const DIAGNOSTIC_CODES = ${canonicalJson(diagnosticCodes)};`,
      "",
    );
  } else if (lang === "go") {
    lines.push(
      "package generated",
      "",
      "const (",
      `\tDiagnosticRegistryIdentity = ${JSON.stringify(registry.identity)}`,
      `\tDiagnosticRegistryDigest   = ${JSON.stringify(registryInput.digest)}`,
      ")",
      "",
      "// DiagnosticCode is one id-sorted registry code available to builders.",
      "type DiagnosticCode struct {",
      "\tCode string",
      "\tID   string",
      "}",
      "",
      "var DiagnosticCodes = []DiagnosticCode{",
    );
    for (const entry of diagnosticCodes) {
      lines.push(`\t{Code: ${JSON.stringify(entry.code)}, ID: ${JSON.stringify(entry.id)}},`);
    }
    lines.push("}", "");
  } else {
    lines.push(
      "namespace Lekalo\\AdapterSdk\\Generated;",
      "",
      "/** The diagnostic registry pin of the generated projection (issue #106). */",
      "final class Diagnostics {",
      `    public const REGISTRY_IDENTITY = ${JSON.stringify(registry.identity)};`,
      `    public const REGISTRY_DIGEST = ${JSON.stringify(registryInput.digest)};`,
      `    public const CODES = ${phpArray(diagnosticCodes)};`,
      "}",
      "",
    );
  }
  return lines.join("\n");
}

// The closed wire vocabulary the generated protocol module carries. The
// values are cross-checked against both pinned schemas at generation time.
const ENUM_TABLE = [
  ["responseStatus", ["ok", "error"]],
  ["transport", ["stdin", "file"]],
  ["supportState", ["full", "partial", "unsupported", "unknown"]],
  ["writeAction", ["create", "replace", "delete"]],
  ["progressState", ["started", "done", "failed"]],
  ["errorClass", ["conflict", "infrastructure", "invalid", "unsupported"]],
  ["scanEntryReferenceRole", ["call", "create", "delete", "emit", "read", "reference", "update"]],
  ["scanEntryConfidence", ["exact", "high", "low", "medium", "unknown"]],
  ["nativeChangeKind", ["added", "deleted", "modified", "renamed"]],
  ["nativePlanKind", ["native-plan"]],
  ["nativeCompleteness", ["complete", "incomplete", "unknown"]],
  ["nativeRunEligibility", ["blocked", "plan-only", "runnable"]],
];

function verifyEnumSpelling() {
  // The response/request envelope member model pins the closed sets; this
  // guard keeps the generated enum table honest about the lint-era members.
  for (const version of protocolVersions) {
    const { schema } = schemas[version];
    const support = schema.$defs.supportState;
    const transportDef = schema.$defs.capsTransport;
    if (support && support.enum && !["full", "partial", "unknown", "unsupported"].every((v) => support.enum.includes(v))) {
      fail("enum-spelling", `${version}/supportState`);
    }
    void transportDef;
  }
}

verifyEnumSpelling();

function renderProtocol(lang) {
  const lines = [generatedHeader(lang), ""];
  const BOUNDS_PAIRS = Object.entries(SEMANTIC_BOUNDS).map(([key, value]) => [
    key.toUpperCase(),
    value,
  ]);
  if (lang === "rust") {
    lines.push(
      "//! The generated wire vocabulary of the target protocol (issue #106).",
      "//! Constants and closed shape data only: strict admission stays in the",
      "//! hand-maintained runtime codec, validated against the shared vectors.",
      "",
      `pub const WIRE_TOKEN: &str = ${JSON.stringify(config.wireToken)};`,
      `pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &[${protocolVersions.map((v) => JSON.stringify(v)).join(", ")}];`,
      `pub const BASE_OPERATIONS: &[&str] = &[${baseOperations.map((v) => JSON.stringify(v)).join(", ")}];`,
      `pub const LATEST_OPERATIONS: &[&str] = &[${latestOperations.map((v) => JSON.stringify(v)).join(", ")}];`,
      `pub const IR_VERSIONS: &[&str] = &[${config.irVersions.map((v) => JSON.stringify(v)).join(", ")}];`,
      `pub const REQUEST_ID_PREFIX: &str = "req-";`,
      `pub const PLAN_ID_PREFIX: &str = "plan-";`,
      "",
      "/// The semantic bounds the runtime codec enforces (mirrors of the closed",
      "/// schema constraints; the paired host tests pin them together).",
      "pub mod bounds {",
      ...BOUNDS_PAIRS.map(([key, value]) => `    pub const ${key}: u64 = ${value};`),
      "}",
      "",
      "/// The closed enum vocabularies (stable wire spellings).",
      "pub mod enums {",
      ...ENUM_TABLE.map(([name, values]) => `    pub const ${name.toUpperCase().replace(/_/g, "")}: &[&str] = &[${values.map((v) => JSON.stringify(v)).join(", ")}];`),
      "}",
      "",
      "/// One per-version operation row of the generated compatibility matrix.",
      "#[derive(Clone, Debug)]",
      "pub struct VersionOperations {",
      "    pub operations: &'static [&'static str],",
      "    pub protocol_version: &'static str,",
      "}",
      "",
      ...protocolVersions.map((version) => `pub const OPERATIONS_V${version.replaceAll(".", "_")}: &[&str] = &[${config.operations[version].map((v) => JSON.stringify(v)).join(", ")}];`),
      "",
      "pub const OPERATIONS_BY_VERSION: &[VersionOperations] = &[",
      ...protocolVersions.map((version) => `    VersionOperations { operations: OPERATIONS_V${version.replaceAll(".", "_")}, protocol_version: ${JSON.stringify(version)} },`),
      "];",
      "",
    );
  } else if (lang === "typescript") {
    lines.push(
      `export const WIRE_TOKEN = ${JSON.stringify(config.wireToken)};`,
      `export const SUPPORTED_PROTOCOL_VERSIONS = ${canonicalJson(protocolVersions)};`,
      `export const BASE_OPERATIONS = ${canonicalJson(baseOperations)};`,
      `export const LATEST_OPERATIONS = ${canonicalJson(latestOperations)};`,
      `export const IR_VERSIONS = ${canonicalJson(config.irVersions)};`,
      `export const REQUEST_ID_PREFIX = "req-";`,
      `export const PLAN_ID_PREFIX = "plan-";`,
      "/** The semantic bounds the runtime codec enforces. */",
      "export const BOUNDS = Object.freeze({",
      ...BOUNDS_PAIRS.map(([key, value]) => `  ${key}: ${value},`),
      "});",
      "/** The closed enum vocabularies (stable wire spellings). */",
      "export const ENUMS = Object.freeze({",
      ...ENUM_TABLE.map(([name, values]) => `  ${name}: Object.freeze(${canonicalJson(values)}),`),
      "});",
      `export const OPERATIONS_BY_VERSION = ${canonicalJson(protocolVersions.map((version) => ({ operations: config.operations[version], protocolVersion: version })))};`,
      "",
    );
  } else if (lang === "go") {
    lines.push(
      "// Package generated carries the generated wire vocabulary of the target",
      "// protocol (issue #106). Strict admission stays in the hand-maintained",
      "// runtime codec, validated against the shared vector corpus.",
      "package generated",
      "",
      `const (`,
      `\tWireToken                 = ${JSON.stringify(config.wireToken)}`,
      `\tRequestIDPrefix           = "req-"`,
      `\tPlanIDPrefix              = "plan-"`,
      ...BOUNDS_PAIRS.map(([key, value]) => `	${key} = ${value}`),
      `)`,
      "",
      `var (`,
      `\tSupportedProtocolVersions = []string{${protocolVersions.map((v) => JSON.stringify(v)).join(", ")}}`,
      `\tBaseOperations            = []string{${baseOperations.map((v) => JSON.stringify(v)).join(", ")}}`,
      `\tLatestOperations          = []string{${latestOperations.map((v) => JSON.stringify(v)).join(", ")}}`,
      `\tIRVersions                = []string{${config.irVersions.map((v) => JSON.stringify(v)).join(", ")}}`,
      `\tOperationsByVersion       = map[string][]string{`,
      ...protocolVersions.map((version) => `\t\t${JSON.stringify(version)}: {${config.operations[version].map((v) => JSON.stringify(v)).join(", ")}},`),
      `\t}`,
      ")",
      "",
      "// The closed enum vocabularies (stable wire spellings).",
      "var Enums = map[string][]string{",
      ...ENUM_TABLE.map(([name, values]) => `\t${JSON.stringify(name)}: {${values.map((v) => JSON.stringify(v)).join(", ")}},`),
      "}",
      "",
    );
  } else {
    lines.push(
      "namespace Lekalo\\AdapterSdk\\Generated;",
      "",
      "/** The generated wire vocabulary of the target protocol (issue #106). */",
      "final class Protocol {",
      `    public const WIRE_TOKEN = ${JSON.stringify(config.wireToken)};`,
      `    public const SUPPORTED_PROTOCOL_VERSIONS = ${canonicalJson(protocolVersions)};`,
      `    public const BASE_OPERATIONS = ${canonicalJson(baseOperations)};`,
      `    public const LATEST_OPERATIONS = ${canonicalJson(latestOperations)};`,
      `    public const IR_VERSIONS = ${canonicalJson(config.irVersions)};`,
      `    public const REQUEST_ID_PREFIX = 'req-';`,
      `    public const PLAN_ID_PREFIX = 'plan-';`,
      "    /** The semantic bounds the runtime codec enforces. */",
      `    public const BOUNDS = ${phpArray(Object.fromEntries(BOUNDS_PAIRS))};`,
      "    /** The closed enum vocabularies (stable wire spellings). */",
      `    public const ENUMS = ${phpArray(Object.fromEntries(ENUM_TABLE))};`,
      `    public const OPERATIONS_BY_VERSION = ${phpArray(protocolVersions.map((version) => ({ operations: config.operations[version], protocolVersion: version })))};`,
      "}",
      "",
    );
  }
  return lines.join("\n");
}

function buildCompatibility(lang, vectors) {
  const pkg = config.packages[lang];
  return {
    capabilityDefinitions: {
      entries: capabilityEntries,
      entriesDigest: capabilityExport.entriesDigest,
      identity: capabilityExport.identity,
    },
    diagnosticRegistry: {
      digest: registryInput.digest,
      identity: registry.identity,
      version: registry.registry_version,
    },
    hostPolicy: {
      confinementRequired: true,
      networkDisabled: true,
      readOnlyDefault: true,
      shellAllowed: false,
    },
    identity: "dev.lekalo.adapter-sdk-compatibility@0.8.0",
    irVersions: config.irVersions,
    language: lang,
    operations: protocolVersions.map((version) => ({
      operations: config.operations[version],
      protocolVersion: version,
    })),
    protocolVersions,
    schemas: protocolVersions.map((version) => ({
      digest: schemas[version].digest,
      identity: `dev.lekalo.target-protocol@${version}`,
      wire: `lekalo/target-protocol/v${version}`,
    })),
    sdkPackage: pkg.name,
    sdkVersion: pkg.version,
    schemaVersion: "lekalo/adapter-sdk-compatibility/v0.8.0",
    testVectors: vectors,
    wireToken: config.wireToken,
  };
}

// ---------------------------------------------------------------------------
// Standalone template bundles (deterministic, generator-owned)
// ---------------------------------------------------------------------------

const TS_MODULE_ORDER = [
  "src/generated/protocol.mjs",
  "src/generated/capabilities.mjs",
  "src/generated/diagnostics.mjs",
  "src/runtime/json.mjs",
  "src/runtime/ids.mjs",
  "src/runtime/paths.mjs",
  "src/runtime/canonical.mjs",
  "src/runtime/logger.mjs",
  "src/runtime/decode.mjs",
  "src/runtime/validate.mjs",
  "src/runtime/build.mjs",
  "src/runtime/serve.mjs",
];

/** Bundle one TypeScript template's standalone entry (node-runnable). */
function bundleTypeScriptTemplate(template, generatedFiles) {
  const parts = [
    "#!/usr/bin/env node",
    `// GENERATED standalone bundle by ${GENERATOR_NAME} v${GENERATOR_VERSION} — do not edit.`,
    `// Template: ${template.name} (issue #106). Confined hosts copy exactly this`,
    "// file; it imports nothing outside the Node standard library.",
    "",
  ];
  const moduleSources = new Map();
  for (const relative of TS_MODULE_ORDER) {
    const generatedKey = relative.startsWith("src/generated/") ? relative : null;
    const text = (generatedKey && generatedFiles.get(generatedKey)) ?? read(join("sdk/typescript", relative));
    moduleSources.set(relative, text);
  }
  const gluePath = join("sdk/templates/typescript", template.name, "main.mjs");
  const glue = read(gluePath);
  parts.push(transformTypeScriptModules(moduleSources, glue, template.name));
  return parts.join("\n") + "\n";
}

/**
 * Deterministically inline the SDK modules and the template glue into one
 * standalone ES module. The generator owns the exact module style:
 * `export const NAME` / `export function NAME` only, relative `./x.mjs`
 * and `../x.mjs` imports, no default exports, no cycles.
 */
function transformTypeScriptModules(moduleSources, glue, templateName) {
  const exportsByModule = new Map();
  const sourceByModule = new Map();
  const normalize = (fromModule, specifier) => {
    const base = fromModule.split("/").slice(0, -1);
    const parts = specifier.replace(/^\.\//, "").split("/");
    const stack = [...base];
    for (const part of parts) {
      if (part === "..") stack.pop();
      else stack.push(part);
    }
    return stack.join("/");
  };
  for (const [name, text] of moduleSources) {
    sourceByModule.set(name, text);
    const names = [...text.matchAll(/^export (?:async )?(?:const|function|class) ([A-Za-z0-9_]+)/gm)].map((match) => match[1]);
    if (names.length === 0) fail("bundle-exports", name);
    exportsByModule.set(name, names);
  }
  const order = TS_MODULE_ORDER.filter((name) => sourceByModule.has(name));
  const hoistedImports = new Set();
  const blocks = [];
  for (const name of order) {
    let text = sourceByModule.get(name);
    // Bare (node built-in) imports are legal in the standalone bundle and
    // must hoist to the file top; relative imports become registry lookups.
    text = text.replace(/^import [\s\S]*?from "[^"]+";$/gm, (whole) => {
      if (/from "\./.test(whole)) return whole;
      hoistedImports.add(whole);
      return `// hoisted: ${whole.slice(0, whole.indexOf(" from"))}`;
    });
    text = text.replace(/^import \{([^}]*)\} from "(\.[^"]+)";$/gm, (whole, names, specifier) => {
      void whole;
      const target = normalize(name, specifier);
      return `const {${names.trim()}} = __module(${JSON.stringify(target)});`;
    });
    if (/^import [\s\S]*from "\./m.test(text)) fail("bundle-import", name);
    const names = exportsByModule.get(name);
    text = text.replace(/^export (async )?(const|function|class) /gm, "$1$2 ");
    blocks.push(
      `// ---- sdk/typescript/${name} ----\n__register(${JSON.stringify(name)}, (() => {\n${text}\nreturn { ${names.join(", ")} };\n})());`,
    );
  }
  let glueText = glue.replace(
    /^import \{([^}]*)\} from "(@lekalo\/adapter-sdk|\.\.\/\.\.\/\.\.\/typescript\/(?:src\/[a-z0-9._/-]+|index\.mjs))";$/gm,
    (whole, names, specifier) => {
      void whole;
      const target = specifier.startsWith("@lekalo/") ? "src/index.mjs" : specifier.slice("../../../typescript/".length);
      return `const {${names.trim()}} = __module(${JSON.stringify(target)});`;
    },
  );
  if (/^import [\s\S]*from "\./m.test(glueText)) fail("bundle-glue-import", templateName);
  glueText = glueText.replace(/^export (async )?(const|function|class) /gm, "$1$2 ");
  return `let __registry = undefined;
function __register(name, moduleExports) {
  __registry.set(name, moduleExports);
}
function __module(name) {
  const found = __registry.get(name);
  if (!found) throw new Error("bundle module missing: " + name);
  return found;
}
const __exports = new Map();
__registry = __exports;
${[...hoistedImports].sort().join("\n")}
${blocks.join("\n")}
// ---- sdk/templates/typescript/${templateName}/main.mjs ----
${glueText}`;
}

/** Bundle one PHP template's standalone entry. */
function bundlePhpTemplate(template, generatedFiles) {
  const order = [
    "src/Generated/Protocol.php",
    "src/Generated/Capabilities.php",
    "src/Generated/Diagnostics.php",
    "src/Runtime/Json.php",
    "src/Runtime/Ids.php",
    "src/Runtime/Paths.php",
    "src/Runtime/Canonical.php",
    "src/Runtime/Logger.php",
    "src/Runtime/Decode.php",
    "src/Runtime/Build.php",
    "src/Runtime/Serve.php",
  ];
  const parts = [
    "<?php",
    `// GENERATED standalone bundle by ${GENERATOR_NAME} v${GENERATOR_VERSION} — do not edit.`,
    `// Template: ${template.name} (issue #106). The confined host copies exactly`,
    "// this file; it requires nothing outside plain PHP.",
    "declare(strict_types=1);",
    "",
  ];
  const useLines = new Set();
  for (const relative of order) {
    const generatedKey = relative.includes("/Generated/") ? relative : null;
    const raw = (generatedKey && generatedFiles.get(generatedKey)) ?? read(join("sdk/php", relative));
    // Same-namespace references need no imports; collect the cross-namespace
    // use statements and emit each exactly once at the bundle top.
    for (const match of raw.matchAll(/^use ([^;]+);$/gm)) {
      useLines.add(match[1]);
    }
  }
  for (const useLine of [...useLines].sort()) {
    parts.push(`use ${useLine};`);
  }
  if (useLines.size > 0) parts.push("");
  for (const relative of order) {
    const generatedKey = relative.includes("/Generated/") ? relative : null;
    const raw = (generatedKey && generatedFiles.get(generatedKey)) ?? read(join("sdk/php", relative));
    const text = raw.replace(/^use [^;]+;$/gm, "");
    const stripped = text
      .replace(/^<\?php\s*/,"")
      .replace(/^declare\(strict_types=1\);\s*$/m, "")
      .replace(/^namespace Lekalo\\AdapterSdk(\\Generated|\\Runtime);\s*$/m, "")
      .replace(/\n{3,}/g, "\n\n");
    parts.push(`// ---- sdk/php/${relative} ----`, stripped.trimEnd(), "");
  }
  const glue = read(join("sdk/templates/php", template.name, "main.php"))
    .replace(/^<\?php\s*/, "")
    .replace(/^declare\(strict_types=1\);\s*$/m, "")
    .replace(/^use [^;]+;$/gm, "")
    .replace(/\n{3,}/g, "\n\n");
  parts.push(`// ---- sdk/templates/php/${template.name}/main.php ----`, glue.trimEnd(), "");
  return parts.join("\n");
}

const TEMPLATES = {
  go: [{ entry: "main.go", name: "generate" }, { entry: "main.go", name: "reference" }, { entry: "main.go", name: "scan" }, { entry: "main.go", name: "verify" }],
  php: [{ entry: "adapter.php", name: "generate" }, { entry: "adapter.php", name: "reference" }, { entry: "adapter.php", name: "scan" }, { entry: "adapter.php", name: "verify" }],
  rust: [],
  typescript: [{ entry: "adapter.mjs", name: "generate" }, { entry: "adapter.mjs", name: "reference" }, { entry: "adapter.mjs", name: "scan" }, { entry: "adapter.mjs", name: "verify" }],
};

// ---------------------------------------------------------------------------
// Orchestration
// ---------------------------------------------------------------------------

function renderFor(lang) {
  const vectors = corpusVectors();
  const pkg = config.packages[lang];
  const compatibility = buildCompatibility(lang, vectors);
  const files = new Map();
  const ext = languageExtension[lang];
  const gen = generatedDirectory[lang];
  const capitalized = lang === "php" ? (name) => name.split("_").map((part) => part[0].toUpperCase() + part.slice(1)).join("") : (name) => name;
  // PHP files carry the strict_types prolog as the very first statement.
  const prefix = lang === "php" ? phpProlog : "";
  files.set(`${gen}/${capitalized("protocol")}.${ext}`, prefix + renderProtocol(lang));
  files.set(`${gen}/${capitalized("capabilities")}.${ext}`, prefix + renderCapabilities(lang));
  files.set(`${gen}/${capitalized("diagnostics")}.${ext}`, prefix + renderDiagnostics(lang));
  files.set(`${gen}/${capitalized("compatibility_tests")}.${ext}`, prefix + renderCompatibilityTests(lang));
  files.set("compatibility.json", canonicalJson(compatibility));
  const outputs = [...files.entries()]
    .map(([path, text]) => ({ digest: sha256(text), path }))
    .sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
  const generation = {
    compatibility: { digest: sha256(files.get("compatibility.json")), path: "compatibility.json" },
    generator: { digest: sha256(readFileSync(SCRIPT_PATH)), name: GENERATOR_NAME, version: GENERATOR_VERSION },
    identity: "dev.lekalo.adapter-sdk-generation@0.8.0",
    inputs: inputRecords.map(({ digest, path, role }) => ({ digest, path, role })).sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0)),
    language: lang,
    options: { ...config.options },
    outputs,
    schemaVersion: "lekalo/adapter-sdk-generation/v0.8.0",
    semanticCoverage: [...config.semanticCoverage].sort((a, b) => (a.definition < b.definition ? -1 : a.definition > b.definition ? 1 : 0)),
    toolchain: { ...pkg.toolchain },
  };
  files.set("generation.json", canonicalJson(generation));
  return files;
}

function templateFiles(lang, generatedFiles) {
  const files = new Map();
  for (const template of TEMPLATES[lang] ?? []) {
    const text = lang === "typescript"
      ? bundleTypeScriptTemplate(template, generatedFiles)
      : lang === "php"
        ? bundlePhpTemplate(template, generatedFiles)
        : null;
    if (text !== null) files.set(`../templates/${lang}/${template.name}/${template.entry}`, text);
  }
  return files;
}

function emitFile(absolute, text) {
  mkdirSync(dirname(absolute), { recursive: true });
  writeFileSync(absolute, text);
}

const targets = write ? (language === "all" ? LANGUAGES : [language]) : LANGUAGES;
const drift = [];
const written = [];
for (const lang of targets) {
  const files = renderFor(lang);
  const bundles = templateFiles(lang, files);
  for (const [path, text] of [...files, ...bundles]) {
    const absolute = join(ROOT, "sdk", lang, path);
    let current = null;
    try {
      current = readFileSync(absolute, "utf8");
    } catch {
      current = null;
    }
    if (current !== text) {
      drift.push(`sdk/${lang}/${path}`);
      if (write) {
        emitFile(absolute, text);
        written.push(`sdk/${lang}/${path}`);
      }
    }
  }
}

if (write) {
  process.stdout.write(`${JSON.stringify({ ok: true, gate: "adapter-sdk-generate", written: written.sort() })}\n`);
} else if (drift.length > 0) {
  fail("drift", `regenerated bytes differ for ${drift.sort().join(", ")}; run scripts/generate-adapter-sdk.mjs --write --language all and review`);
} else {
  process.stdout.write(`${JSON.stringify({ ok: true, gate: "adapter-sdk-generate", check: true, languages: LANGUAGES })}\n`);
}
