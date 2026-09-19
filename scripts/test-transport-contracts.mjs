#!/usr/bin/env node
// Issue #70 release gate: the transport-http v0.4.0 attachment
// contract, the valid and invalid fixture documents, the projection
// goldens' canonical form, and the compiled Rust identity constants,
// validated with the same pinned third-party Draft 2020-12
// implementation as the other contract gates. Exact Ajv 8.17.1 is
// provisioned outside this checkout (CI does the same on Node 18 and
// 24) and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.
//
// This script is the independent second canonical implementation: it
// re-checks the wire shape and the wire-level bounds without reusing
// any Rust code. Semantic refusals (unresolved endpoints, error-union
// violations, capability shortfalls) are Rust-owned and only their
// registered diagnostics are checked here.

import { readdirSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
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
const readText = (relative) => readFileSync(resolve(root, relative), "utf8");

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

// ---------------------------------------------------------------------------
// 1. The published 0.4.0 contract is closed and bounded: identity
//    consts, the closed vocabularies, and the wire bounds.
// ---------------------------------------------------------------------------
const schema = read("contracts/transport-http.schema.v0.4.0.json");
if (schema.properties.schemaVersion.const !== "lekalo/transport-http/v0.4.0") {
  fail("schema-schema-version", schema.properties.schemaVersion.const);
}
if (schema.properties.identity.const !== "dev.lekalo.transport-http@0.4.0") {
  fail("schema-identity", schema.properties.identity.const);
}
if (schema.additionalProperties !== false) fail("schema-open-root", "additionalProperties");
const defs = schema.$defs;
if (!defs.endpointBinding || !defs.securityScheme || !defs.errorDefaults) {
  fail("schema-defs", "closed declaration surfaces");
}
const kinds = new Set(defs.securityScheme.properties.kind.enum);
if (
  !["none", "bearer", "api-key", "basic", "oauth2", "mutual-tls", "custom"].every((kind) =>
    kinds.has(kind),
  ) ||
  kinds.size !== 7
) {
  fail("scheme-kinds", [...kinds]);
}
const actors = new Set(defs.actor.enum);
if (
  !["public", "identity.user", "identity.service", "system.job", "internal"].every((actor) =>
    actors.has(actor),
  ) ||
  actors.size !== 5
) {
  fail("actor-vocabulary", [...actors]);
}
const capabilityKinds = new Set(defs.capabilityKind.enum);
if (!["streaming", "upload", "download"].every((kind) => capabilityKinds.has(kind))) {
  fail("capability-kinds", [...capabilityKinds]);
}
// The wire bounds must agree with the owner-approved constants.
if (schema.properties.endpoints.maxItems !== 2048) {
  fail("schema-endpoints-bound", schema.properties.endpoints.maxItems);
}
if (defs.endpointBinding.properties.params.maxItems !== 64) {
  fail("schema-params-bound", defs.endpointBinding.properties.params.maxItems);
}
if (defs.endpointBinding.properties.errors.maxItems !== 256) {
  fail("schema-errors-bound", defs.endpointBinding.properties.errors.maxItems);
}
if (schema.properties.securitySchemes.maxItems !== 64) {
  fail("schema-schemes-bound", schema.properties.securitySchemes.maxItems);
}
if (defs.endpointBinding.properties.scenarios.maxItems !== 32) {
  fail("schema-scenarios-bound", defs.endpointBinding.properties.scenarios.maxItems);
}

// ---------------------------------------------------------------------------
// 2. Fixtures: the valid document is schema-valid; every invalid
//    vector carries a registered rule expectation and is either
//    schema-refused or deliberately schema-legal (the Rust runtime
//    refusal is proven by the core suite).
// ---------------------------------------------------------------------------
const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateAttachment = ajv.compile(schema);

const canonicalJson = (value) => {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value !== null && typeof value === "object") {
    const body = Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
      .join(",");
    return `{${body}}`;
  }
  return JSON.stringify(value);
};

const RULE_ID = /^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+$/;
let goldens = 0;
let invalidVectors = 0;
let schemaAcceptedContextualVectors = 0;

const validDir = "tests/fixtures/transport-http/valid";
for (const name of readdirSync(resolve(root, validDir))) {
  if (!name.endsWith(".json")) continue;
  const document = read(`${validDir}/${name}`);
  if (!validateAttachment(document)) {
    fail(`valid-fixture-${name}`, ajv.errorsText(validateAttachment.errors));
  }
  goldens += 1;
}

const invalidDir = "tests/fixtures/transport-http/invalid";
for (const name of readdirSync(resolve(root, invalidDir)).sort()) {
  if (!name.endsWith(".json") || name.endsWith(".expect.json")) continue;
  const document = read(`${invalidDir}/${name}`);
  const expectation = read(
    `${invalidDir}/${name.replace(/\.json$/, ".expect.json")}`,
  );
  if (typeof expectation.rule !== "string" || !RULE_ID.test(expectation.rule)) {
    fail(`bad-rule-${name}`, expectation.rule);
  }
  if (!expectation.rule.startsWith("transport.")) {
    fail(`rule-family-${name}`, expectation.rule);
  }
  invalidVectors += 1;
  if (validateAttachment(document)) schemaAcceptedContextualVectors += 1;
}
if (goldens === 0) fail("no-goldens", "the valid/ directory is empty");
if (invalidVectors === 0) fail("no-invalid-vectors", "the invalid/ directory is empty");

// ---------------------------------------------------------------------------
// 3. The projection goldens: canonical bytes (compact, byte-sorted
//    keys, no trailing LF) over all four namespaces, and the shared
//    canonical core is identical across namespaces.
// ---------------------------------------------------------------------------
const namespaces = ["go", "laravel", "node", "rust"];
const cores = new Set();
for (const namespace of namespaces) {
  const relative = `tests/fixtures/transport-http/projected/${namespace}/${namespace}.expect.json`;
  const raw = readText(relative);
  const document = JSON.parse(raw);
  if (document.namespace !== namespace) fail("golden-namespace", relative);
  if (raw.endsWith("\n")) fail("golden-trailing-lf", relative);
  if (canonicalJson(document) !== raw) fail("golden-canonical", relative);
  const core = { ...document, namespace: undefined };
  core.routes = document.routes.map((route) => ({ ...route, handler: undefined }));
  cores.add(canonicalJson(core));
}
if (cores.size !== 1) {
  fail("golden-parity", "the canonical core differs across namespaces");
}

// ---------------------------------------------------------------------------
// 4. The compiled Rust identity constants and the projection seam.
// ---------------------------------------------------------------------------
const versionSource = readText("crates/lekalo-core/src/transport_http/version.rs");
for (const constant of [
  'FAMILY: &str = "dev.lekalo.transport-http"',
  'VERSION: &str = "0.4.0"',
  'IDENTITY: &str = "dev.lekalo.transport-http@0.4.0"',
  'SCHEMA_VERSION: &str = "lekalo/transport-http/v0.4.0"',
  'IR_IDENTITY: &str = "dev.lekalo.ir@0.2.16"',
  'WIRE_DIALECT: &str = "lekalo-http-wire/v1"',
  "MAX_ENDPOINTS: usize = 2048",
  "MAX_PARAMS: usize = 64",
  "MAX_ERROR_MAP: usize = 256",
  "MAX_SCHEMES: usize = 64",
  "MAX_SCENARIOS: usize = 32",
  "MAX_CANONICAL_BYTES: usize = 1024 * 1024",
]) {
  if (!versionSource.includes(constant)) fail("rust-constant", constant);
}
const projectSource = readText("crates/lekalo-core/src/transport_http/project.rs");
for (const token of ['pub const NAMESPACE_NODE', 'pub const NAMESPACE_LARAVEL', 'pub const NAMESPACE_GO', 'pub const NAMESPACE_RUST']) {
  if (!projectSource.includes(token)) fail("projection-namespace", token);
}
// The projection refuses an unknown namespace and joins the Model
// endpoint symbols (never guessing a route).
if (!projectSource.includes('"namespace-unknown"')) {
  fail("projection-gate", "unknown namespace refuses");
}
if (!projectSource.includes("ENDPOINT_UNRESOLVED")) {
  fail("projection-gate", "unresolved endpoints refuse");
}

process.stdout.write(
  `${JSON.stringify(
    {
      ok: true,
      ajv: ajvVersion,
      schema: "lekalo/transport-http/v0.4.0",
      goldens,
      invalidVectors,
      schemaAcceptedContextualVectors,
      projectedNamespaces: namespaces.length,
    },
    null,
    2,
  )}\n`,
);
