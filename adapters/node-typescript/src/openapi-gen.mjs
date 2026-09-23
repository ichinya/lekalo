/**
 * The OpenAPI generation capability of `lekalo-target-node-typescript`
 * (issue #46): `generate.openapi`, composed inside the single
 * generation descriptor (the closed target-protocol wire has exactly
 * one `generate` operation).
 *
 * The generator reads the canonical transport evidence
 * (`.lekalo/cache/transport/<project>.json`) joined with the
 * compiled-IR evidence (`.lekalo/cache/ir/<project>.json`) through the
 * kernel read view, resolves the target-document policy, renders the
 * deterministic document, and returns the write plan:
 * `<policy.path>`, the `<stem>.ownership.json` pointer manifest, and
 * the `<stem>.map.json` pointer→semantic-id sidecar.
 *
 * Partial honesty: the adapter's read roots carry no #62 error
 * registry and no #64 query-model attachment, so the identity error
 * variants and the declared filter/sort annotations cannot render —
 * every affected member surfaces as an `openapi.partial` finding and
 * the capability claims `partial`, never `full`. Everything else
 * mirrors the core projection semantics member for member; on the
 * same inputs the canonical JSON is byte-identical.
 */

import { canonicalJson, fromYaml, toYaml } from "./openapi-emit.mjs";
import { createHash } from "node:crypto";
import {
  decodeEvidence,
  decodeIrEvidence,
  TRANSPORT_EVIDENCE_DIR,
  IR_EVIDENCE_DIR,
  evidencePathFor,
} from "./transport-extension.mjs";
import { POLICY_PATH, resolvePolicy } from "./openapi-policy.mjs";

/** The capability this generator claims inside the composite. */
export const OPENAPI_CAPABILITY = "generate.openapi";
/** The canonical generator identity carried in the provenance block. */
export const GENERATOR_ID = "lekalo-core/openapi";
/** The generator version: the product version of the landing commit. */
export const GENERATOR_VERSION = "0.4.0";
/** The ownership sidecar contract. */
export const OWNERSHIP_CONTRACT = "lekalo/openapi-map/v0.4.0";
/** The write scopes the generated files live under (the policy path
 * default lives in docs/). */
export const OPENAPI_WRITE_SCOPES = ["docs/**"];

/** The document byte bound (the core's export bound). */
export const MAX_DOCUMENT_BYTES = 4 * 1024 * 1024;
/** The exact embedded IR contract identity the join accepts. */
const IR_IDENTITY = "dev.lekalo.ir@0.2.16";

const sha256Text = (text) =>
  "sha256:" + createHash("sha256").update(text, "utf8").digest("hex");

/** The one extension entry over the kernel read/write views. */
export function openapiGenerateOperation(context) {
  const { request, readView } = context;
  if (!readView) {
    return { state: "unsupported", diagnostics: [{ reason: "profile-absent" }] };
  }
  try {
    const policy = resolvePolicyFromContext(readView);
    if (policy.refusal) {
      return { state: "failed", diagnostics: [{ reason: `policy-${policy.refusal}` }] };
    }
    const transportPath = evidencePathFor(request, readView, TRANSPORT_EVIDENCE_DIR);
    if (!transportPath) {
      return {
        state: "failed",
        diagnostics: [{ reason: "transport-evidence-absent" }],
      };
    }
    const decoded = decodeEvidence(readView.readFile(transportPath));
    if (decoded.error) {
      return { state: "failed", diagnostics: [{ reason: decoded.error }] };
    }
    const irPath = evidencePathFor(request, readView, IR_EVIDENCE_DIR);
    if (!irPath) {
      return {
        state: "failed",
        diagnostics: [{ reason: "ir-evidence-absent" }],
      };
    }
    const ir = decodeIrEvidenceFull(readView.readFile(irPath));
    if (ir.error) {
      return { state: "failed", diagnostics: [{ reason: ir.error }] };
    }
    if (!ir.value.projectId || ir.value.projectId !== decoded.value.projectId) {
      return {
        state: "failed",
        diagnostics: [{ reason: "transport-project-mismatch" }],
      };
    }
    const rendered = renderDocument(decoded.value, ir.value, policy.policy);
    if (rendered.canonical.length > MAX_DOCUMENT_BYTES) {
      return {
        state: "failed",
        diagnostics: [{ reason: "openapi-export-limit" }],
      };
    }
    return writePlan(context, rendered, policy.policy);
  } catch (error) {
    throw new Error("openapi-generator: " + bounded(error?.message));
  }
}

/** The verify posture: recompute the expected bytes and diff them
 * against the on-disk files, reporting `openapi.drift` findings — the
 * semantic companion of the byte-digest gate. */
export function openapiVerifyOperation(context) {
  const outcome = openapiGenerateOperation({
    ...context,
    // The recomputation never writes: a dry-run request plus a no-op
    // write view make the verify posture inert by construction.
    request: { ...context.request, dry_run: true },
    writeView: context.writeView ?? { exists: () => false, write: () => {} },
  });
  if (outcome.state !== "complete") {
    return outcome;
  }
  const verification = [];
  for (const write of outcome.data.writes) {
    if (!context.readView.canRead(write.path)) {
      verification.push({
        path: write.path,
        code: "openapi.drift",
        detail: "unreadable-or-missing",
      });
      continue;
    }
    const observed = context.readView.readFile(write.path);
    const expected = outcome.data.bodies?.get?.(write.path);
    if (expected === undefined) {
      continue;
    }
    if (!observed.equals(Buffer.from(expected, "utf8"))) {
      verification.push({
        path: write.path,
        code: "openapi.drift",
        detail: `expected:${write.sha256.slice(7, 19)} observed:${digestOf(observed).slice(7, 19)}`,
      });
    }
  }
  return {
    state: "complete",
    data: { writes: [], findings: [...outcome.data.findings, ...verification] },
  };
}

/** Resolve the policy document through the read view (absent → defaults). */
function resolvePolicyFromContext(readView) {
  if (!readView.canRead(POLICY_PATH)) {
    return { policy: { version: "3.1", mode: "full", path: "docs/openapi.yaml" } };
  }
  const bytes = readView.readFile(POLICY_PATH);
  return resolvePolicy(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
}

/** The full IR evidence decode: the route layer needs only endpoint
 * symbols; the document renderer needs every definition. */
function decodeIrEvidenceFull(bytes) {
  const decoded = decodeIrEvidence(bytes);
  if (decoded.error) {
    return decoded;
  }
  let document;
  try {
    document = JSON.parse(bytes.toString("utf8"));
  } catch {
    return { error: "ir-evidence-invalid" };
  }
  if (document.contract !== IR_IDENTITY) {
    return { error: "ir-evidence-version" };
  }
  return { value: { ...decoded.value, definitions: document.definitions } };
}

// ---------------------------------------------------------------------------
// The closed mapping table (plan §2.3): the exact core semantics.
// ---------------------------------------------------------------------------

/** The document-wide uniform `(category, status)` error-default pairs:
 * exactly the pairs every endpoint declares alike — the set that may
 * render once under `components.responses` (r1 F-1). */
function computeUniformDefaults(attach) {
  const endpoints = attach?.endpoints ?? [];
  if (endpoints.length === 0) return new Set();
  const membersOf = (endpoint) => {
    const members = new Set();
    for (const [category, code] of Object.entries(endpoint.errorDefaults ?? {})) {
      if (code !== 0) members.add(`${category},${code}`);
    }
    return members;
  };
  const uniform = membersOf(endpoints[0]);
  for (let index = 1; index < endpoints.length; index++) {
    const members = membersOf(endpoints[index]);
    for (const pair of uniform) {
      if (!members.has(pair)) uniform.delete(pair);
    }
  }
  return uniform;
}

/** One declared security scheme, or nothing when the scheme has no
 * native OpenAPI shape at the declared version (r1 F-2). */
function securitySchemeObject(scheme, version) {
  switch (scheme.kind) {
    case "bearer": {
      const object = { type: "http", scheme: "bearer" };
      if (scheme.format) {
        object.bearerFormat = scheme.format;
      }
      return object;
    }
    case "api-key": {
      const object = { type: "apiKey" };
      if (scheme.location) {
        object.in = scheme.location;
      }
      if (scheme.name) {
        object.name = scheme.name;
      }
      return object;
    }
    case "basic":
      return { type: "http", scheme: "basic" };
    case "mutual-tls":
      return version === "3.1" ? { type: "mutualTLS" } : null;
    // oauth2/custom: no native shape without invented URLs or
    // semantics; mutual-tls at 3.0: not expressible (G3/G4).
    default:
      return null;
  }
}

/** The document render over one decoded evidence join. */
function renderDocument(attachment, ir, policy) {
  const definitions = new Map();
  for (const definition of ir.definitions ?? []) {
    if (definition && typeof definition.id === "string") {
      definitions.set(definition.id, definition);
    }
  }
  const state = {
    version: policy.version,
    components: new Map(),
    sharedResponses: {},
    findings: [],
    partial(symbol, detail) {
      const finding = { detail, symbol };
      if (
        !state.findings.some(
          (existing) => existing.symbol === symbol && existing.detail === detail,
        )
      ) {
        state.findings.push(finding);
      }
    },
  };

  // Pass one: every operation.
  const pathItems = new Map();
  const pointers = [];
  for (const endpoint of attachment.endpoints) {
    const definition = definitions.get(endpoint.endpoint);
    if (!definition || definition.kind !== "endpoint") {
      state.partial(endpoint.endpoint, "endpoint-unresolved");
      continue;
    }
    const operation = operationOf(attachment, endpoint, definition, definitions, state);
    const template = definition.path;
    const method = definition.method.toLowerCase();
    const pointer = pathsPointer(template, method);
    pointers.push([pointer, endpoint.endpoint]);
    const item = pathItems.get(template) ?? {};
    item[method] = operation;
    pathItems.set(template, item);
  }
  pointers.sort((left, right) => (left[0] < right[0] ? -1 : left[0] > right[0] ? 1 : 0));

  // Pass two: the reusable components (schemas only — the #62 identity
  // variants cannot render without the bound registry; every declared
  // error response stays open with a finding).
  const schemas = {};
  for (const [symbol, name] of [...state.components].sort(byKey)) {
    const definition = definitions.get(symbol);
    if (!definition) continue;
    const body = componentBody(definition, definitions, state);
    if (body !== null) {
      schemas[name] = withSymbol(body, symbol);
    }
  }

  // Security schemes: every declared, renderable scheme once.
  const securitySchemes = {};
  for (const scheme of attachment.securitySchemes ?? []) {
    const object = securitySchemeObject(scheme, policy.version);
    if (object !== null) {
      securitySchemes[scheme.id] = withSymbol(object, scheme.id);
    }
  }

  // The root: canonical member order is the byte-sorted key order.
  // info.version is the attachment revision — the documented document
  // revision, never the generator's own version (r1 devin F-6).
  const root = {
    openapi: versionWire(policy.version),
    info: { title: attachment.projectId, version: attachment.attachmentRevision ?? "" },
    paths: Object.fromEntries(
      [...pathItems.entries()].sort(byKey).map(([template, item]) => [
        template,
        Object.fromEntries(Object.keys(item).sort().map((method) => [method, item[method]])),
      ]),
    ),
  };
  if (
    Object.keys(schemas).length > 0 ||
    Object.keys(state.sharedResponses ?? {}).length > 0 ||
    Object.keys(securitySchemes).length > 0
  ) {
    root.components = {};
    if (Object.keys(schemas).length > 0) {
      root.components.schemas = sortKeys(schemas);
    }
    if (Object.keys(state.sharedResponses ?? {}).length > 0) {
      root.components.responses = sortKeys(state.sharedResponses);
    }
    if (Object.keys(securitySchemes).length > 0) {
      root.components.securitySchemes = sortKeys(securitySchemes);
    }
  }
  root["x-lekalo-provenance"] = {
    generator: { id: GENERATOR_ID, version: GENERATOR_VERSION },
    irRef: {
      digest: attachment.irRef?.digest ?? "",
      identity: attachment.irRef?.identity ?? "",
    },
    modelRef: {
      digest: attachment.modelRef?.digest ?? "",
      modelVersion: attachment.modelRef?.modelVersion ?? "",
    },
    transportRef: {
      // The exact bytes of the evidence document that were read.
      digest: attachment.digest,
      schemaVersion: "lekalo/transport-http/v0.4.0",
    },
  };
  const canonical = canonicalJson(root);
  return {
    root,
    canonical,
    digest: sha256Text(canonical),
    findings: state.findings.sort(compareFindings),
    pointers,
  };
}

/** One rendered operation object. */
function operationOf(attachment, endpoint, definition, definitions, state) {
  const operation = {
    operationId: effectiveOperationId(endpoint),
    responses: responsesOf(attachment, endpoint, definition, definitions, state),
    "x-lekalo-endpoint": endpoint.endpoint,
    "x-lekalo-operation": definition.invokes,
  };
  if (endpoint.tags !== undefined) {
    operation.tags = [...endpoint.tags];
  }
  if (endpoint.summary !== undefined) {
    operation.summary = endpoint.summary;
  }
  const parameters = [];
  for (const param of endpoint.params ?? []) {
    parameters.push(paramOf(param, definition, definitions, state));
  }
  if (endpoint.pagination !== undefined) {
    for (const name of [
      endpoint.pagination.limitParam,
      endpoint.pagination.offsetParam,
      endpoint.pagination.cursorParam,
    ]) {
      if (name === undefined || name === null) continue;
      parameters.push({ in: "query", name, required: false, schema: {} });
    }
  }
  if (endpoint.idempotency !== undefined) {
    parameters.push({
      in: "header",
      name: endpoint.idempotency.header,
      required: endpoint.idempotency.required === true,
      schema: {},
    });
  }
  if (endpoint.correlation !== undefined) {
    for (const header of endpoint.correlation.headers ?? []) {
      parameters.push({ in: "header", name: header, required: false, schema: {} });
    }
  }
  if (endpoint.apiVersion !== undefined && endpoint.apiVersion.in === "header") {
    parameters.push({
      in: "header",
      name: endpoint.apiVersion.name,
      required: false,
      schema: {},
    });
  }
  if (parameters.length > 0) {
    operation.parameters = parameters;
  }
  if (endpoint.body !== undefined && endpoint.body !== null) {
    const command = definitions.get(definition.invokes);
    const input =
      command && command.kind === "command" ? (command.input ?? []) : null;
    if (endpoint.body.mode === "whole-input") {
      if (input === null) {
        state.partial(endpoint.endpoint, "input-undeclared");
        operation.requestBody = jsonContent({});
      } else {
        operation.requestBody = jsonContent(objectSchema(input, definitions, state));
      }
    } else {
      operation.requestBody = jsonContent(
        explicitObject(endpoint.body.fields ?? [], definitions, input, null, state),
      );
    }
  }
  // Security: the native requirement when every declared scheme
  // renders; annotated otherwise.
  if (endpoint.auth !== undefined && endpoint.auth !== null) {
    const auth = endpoint.auth;
    if (auth.actor === "public") {
      operation.security = [];
    } else {
      const schemes = attachment.securitySchemes ?? [];
      const renderable = (id) => {
        const scheme = schemes.find((candidate) => candidate.id === id);
        if (scheme === undefined) return false;
        if (scheme.kind === "oauth2" || scheme.kind === "custom") return false;
        // mutualTLS exists only in OpenAPI 3.1 (G3): a declared 3.0
        // render annotates the scheme instead of a native requirement.
        if (scheme.kind === "mutual-tls" && state.version !== "3.1") return false;
        return true;
      };
      const requirement = {};
      const annotated = [];
      for (const id of auth.schemes ?? []) {
        if (renderable(id)) {
          requirement[id] = [];
        } else {
          annotated.push(id);
          state.partial(id, "scheme-not-expressible");
        }
      }
      if (Object.keys(requirement).length > 0) {
        operation.security = [requirement];
      }
      if (annotated.length > 0) {
        operation["x-lekalo-scheme"] = annotated.sort();
      }
      if (auth.policyRef !== undefined) {
        operation["x-lekalo-policy"] = auth.policyRef;
      }
    }
  }
  if (endpoint.rateLimit !== undefined) {
    operation["x-lekalo-rate-limit"] = {
      limit: endpoint.rateLimit.limit,
      scope: endpoint.rateLimit.scope,
      windowSeconds: endpoint.rateLimit.windowSeconds,
    };
  }
  if (endpoint.cache !== undefined) {
    operation["x-lekalo-cache"] = {
      etag: endpoint.cache.etag === true,
      maxAgeSeconds: endpoint.cache.maxAgeSeconds,
      policy: endpoint.cache.policy,
    };
  }
  if (endpoint.apiVersion !== undefined && endpoint.apiVersion.in === "path") {
    operation["x-lekalo-api-version"] = {
      in: "path",
      name: endpoint.apiVersion.name,
    };
  }
  if ((endpoint.capabilities ?? []).length > 0) {
    operation["x-lekalo-capabilities"] = endpoint.capabilities.map((decl) => ({
      capability: decl.capability,
      detail: decl.detail,
      minimumSupport: decl.minimumSupport,
    }));
  }
  return operation;
}

/** The responses object of one operation. */
function responsesOf(attachment, endpoint, definition, definitions, state) {
  void attachment;
  const responses = {};
  const success = { description: "Success response." };
  const status = String(endpoint.success?.status ?? 200);
  if (endpoint.success?.status !== 204) {
    const body = endpoint.success?.body;
    if (body === undefined || body === null) {
      success.content = { "application/json": { schema: {} } };
    } else {
      const query = definitions.get(definition.invokes);
      const returns = query && query.kind === "query" ? (query.returns ?? null) : null;
      let schema;
      if (body.mode === "whole-output") {
        if (returns === null) {
          state.partial(endpoint.endpoint, "output-undeclared");
          schema = {};
        } else {
          schema = typeOf(returns, definitions, state);
        }
      } else {
        schema = explicitObject(body.fields ?? [], definitions, null, returns, state);
      }
      const content = { "application/json": { schema } };
      if (
        (endpoint.capabilities ?? []).some(
          (decl) => decl.capability === "streaming" && decl.detail === "sse",
        )
      ) {
        content["text/event-stream"] = {};
      }
      success.content = content;
    }
  }
  if ((endpoint.success?.headers ?? []).length > 0) {
    success.headers = Object.fromEntries(
      endpoint.success.headers.map((header) => [
        header.name,
        { required: header.required === true, schema: {} },
      ]),
    );
  }
  if (endpoint.pagination?.cursorField !== undefined) {
    success["x-lekalo-cursor-field"] = endpoint.pagination.cursorField;
  }
  responses[status] = success;

  // The declared error statuses: without the bound #62 registry the
  // identity variants cannot render — the status response stays open
  // and the gap is reported, never a dangling $ref.
  const byStatus = new Map();
  for (const entry of endpoint.errors ?? []) {
    const list = byStatus.get(entry.status) ?? [];
    list.push(entry.error);
    byStatus.set(entry.status, list);
  }
  for (const [code, errors] of [...byStatus.entries()].sort(byNumericKey)) {
    for (const error of errors) {
      state.partial(error, "error-variant-unrendered");
    }
    responses[code] = {
      content: { "application/json": { schema: {} } },
      description: "Error response.",
    };
  }
  // The category defaults: uncovered statuses reference the shared
  // category response if uniform, otherwise inline the category body.
  const defaults = endpoint.errorDefaults ?? {};
  const covered = new Set([...byStatus.keys()]);
  const uniform = computeUniformDefaults(attachment);
  const shared = {};
  for (const [category, code] of Object.entries(defaults)) {
    if (code === 0 || covered.has(code) || responses[code] !== undefined) continue;
    const isUniform = uniform.has(`${category},${code}`);
    if (isUniform) {
      responses[code] = { $ref: `#/components/responses/Error${pascal(category)}` };
    } else {
      // Divided default: the shared component would be ambiguous, so
      // the category body renders inline — never a dangling $ref.
      responses[code] = categoryResponse(category, state.version);
    }
  }
  // Build shared components only for uniform defaults
  for (const [category, code] of Object.entries(defaults)) {
    if (code === 0) continue;
    if (uniform.has(`${category},${code}`)) {
      const name = `Error${pascal(category)}`;
      shared[name] = categoryResponse(category, state.version);
    }
  }
  if (Object.keys(shared).length > 0 && state.components.size >= 0) {
    state.sharedResponses = shared;
  }
  return responses;
}

/** One declared parameter with its resolved schema. */
function paramOf(param, definition, definitions, state) {
  const result = { in: param.in, name: param.name, required: param.required === true };
  if (param.style !== undefined) {
    result.style = param.style;
  }
  if (param.explode !== undefined) {
    result.explode = param.explode;
  }
  result.schema = fieldSchema(param.field, definition, definitions, state);
  return result;
}

/** The schema of one field reference: a command input member's type. */
function fieldSchema(field, definition, definitions, state) {
  if (typeof field === "string" && field.startsWith("input.")) {
    const name = field.slice("input.".length);
    const command = definitions.get(definition.invokes);
    const member =
      command && command.kind === "command"
        ? (command.input ?? []).find((candidate) => candidate.name === name)
        : undefined;
    if (member !== undefined) {
      return typeOf(member.type, definitions, state);
    }
    state.partial(name, "input-member-unresolved");
    return {};
  }
  // A bare query-model parameter name: the adapter carries no #64
  // attachment, so the type cannot resolve — reported, never guessed.
  state.partial(typeof field === "string" ? field : String(field), "parameter-unresolved");
  return {};
}

/** One explicit-projection object schema. */
function explicitObject(fields, definitions, input, returns, state) {
  const properties = {};
  const required = [];
  for (const field of fields) {
    let schema = {};
    if (input !== null) {
      const member = input.find((candidate) => candidate.name === field.field?.slice?.(6));
      schema = member !== undefined ? typeOf(member.type, definitions, state) : schema;
    } else if (returns !== null && typeof returns.ref === "string") {
      const source = definitions.get(returns.ref);
      const member =
        source && Array.isArray(source.fields)
          ? source.fields.find((candidate) => candidate.name === field.field)
          : undefined;
      schema = member !== undefined ? typeOf(member.type, definitions, state) : schema;
    }
    properties[field.name] = schema;
    if (field.required === true) {
      required.push(field.name);
    }
  }
  return objectSchemaFrom(properties, required);
}

/** The object schema over declared fields (the §2.3 shape). */
function objectSchema(fields, definitions, state) {
  const properties = {};
  const required = [];
  for (const field of fields) {
    let schema = typeOf(field.type, definitions, state);
    if (field.description !== undefined) {
      schema = { ...schema, description: field.description };
    }
    properties[field.name] = schema;
    if (field.required === true) {
      required.push(field.name);
    }
  }
  return objectSchemaFrom(properties, required);
}

/** Assemble one object schema from sorted properties. */
function objectSchemaFrom(properties, required) {
  const object = {
    additionalProperties: false,
    properties: sortKeys(properties),
    type: "object",
  };
  if (required.length > 0) {
    object.required = [...required].sort();
  }
  return object;
}

/** The closed `type` expression mapping (the §2.3 table). */
function typeOf(type, definitions, state) {
  if (type === null || typeof type !== "object") {
    return {};
  }
  if (typeof type.ref === "string") {
    return refSchema(type.ref, definitions, state);
  }
  if (type.list !== undefined) {
    return { items: typeOf(type.list, definitions, state), type: "array" };
  }
  if (type.optional !== undefined) {
    return optionalOf(typeOf(type.optional, definitions, state), state.version);
  }
  return {};
}

/** The `$ref` (or open fallback) of one named symbol. */
function refSchema(symbol, definitions, state) {
  const definition = definitions.get(symbol);
  if (definition === undefined || componentKind(definition) === undefined) {
    state.partial(symbol, "symbol-unresolved");
    return {};
  }
  const name = componentName(symbol);
  if (!state.components.has(symbol)) {
    state.components.set(symbol, name);
  }
  return { $ref: `#/components/schemas/${name}` };
}

/** The nullable composition of one inner schema at the declared
 * version (the shared table's Optional row): 3.1 composes the 2020-12
 * forms (oneOf with the null type over refs, the widened type array
 * over value schemas, exactly one `"null"`); 3.0 uses the `nullable`
 * sibling — `allOf` over refs (a $ref carries no siblings in 3.0) and
 * the `nullable: true` member over value schemas (r1 F-3/cline F-1). */
function optionalOf(inner, version) {
  if (inner !== null && typeof inner === "object" && inner.$ref !== undefined) {
    if (version === "3.0") {
      return { nullable: true, allOf: [inner] };
    }
    return { oneOf: [inner, { type: "null" }] };
  }
  if (inner !== null && typeof inner === "object" && inner.type !== undefined) {
    if (version === "3.0") {
      return { ...inner, nullable: true };
    }
    const types = Array.isArray(inner.type) ? [...inner.type] : [inner.type];
    // A nested `optional<optional<T>>` widens to exactly one `"null"`:
    // the meta-schema requires unique type-array items.
    if (!types.includes("null")) {
      types.push("null");
    }
    return { ...inner, type: types };
  }
  return inner;
}

/** The body of one reusable component, or nothing. */
function componentBody(definition, definitions, state) {
  switch (definition.kind) {
    case "scalar":
      return scalarSchema(definition.base);
    case "enum":
      return {
        enum: definition.values.map((value) => value.value),
        type: "string",
      };
    case "value-object":
    case "entity":
      return objectSchema(definition.fields ?? [], definitions, state);
    default:
      return null;
  }
}

/** The closed scalar-base projection. */
function scalarSchema(base) {
  switch (base) {
    case "string":
      return { type: "string" };
    case "number":
      return { type: "number" };
    case "boolean":
      return { type: "boolean" };
    case "date":
      return { format: "date", type: "string" };
    case "datetime":
      return { format: "date-time", type: "string" };
    case "uuid":
      return { format: "uuid", type: "string" };
    case "uri":
      return { format: "uri", type: "string" };
    default:
      return {};
  }
}

/** One shared category response body (no declared id/code), spelled
 * per the declared version: 3.1 pins the identity members with
 * `const`; 3.0 spells single-value `enum`s (r1 F-3/cline F-1). */
function categoryResponse(category, version) {
  const constant = (value) => (version === "3.0" ? { enum: [value] } : { const: value });
  return {
    content: {
      "application/json": {
        schema: {
          additionalProperties: false,
          properties: {
            error: {
              additionalProperties: false,
              properties: {
                category: constant(category),
                payload: { type: "object" },
              },
              required: ["category", "payload"],
              type: "object",
            },
            ok: constant(false),
          },
          required: ["error", "ok"],
          type: "object",
        },
      },
    },
    description: "Error response.",
  };
}

/** Attach the semantic-id anchor to one reusable component. */
function withSymbol(body, symbol) {
  return { ...body, "x-lekalo-symbol": symbol };
}

/** The component kinds that render as reusable schemas. */
function componentKind(definition) {
  switch (definition.kind) {
    case "scalar":
    case "enum":
    case "value-object":
    case "entity":
      return definition.kind;
    default:
      return undefined;
  }
}

/** `task_id` → `TaskId`; the #45 export-name rule. */
export function pascal(text) {
  return text
    .split("_")
    .filter((part) => part.length > 0)
    .map((part) => part[0].toUpperCase() + part.slice(1))
    .join("");
}

/** The reusable-schema component name of one semantic id. */
export function componentName(symbol) {
  const [module, local] = splitSymbol(symbol);
  return pascal(module) + pascal(local);
}

function splitSymbol(symbol) {
  const index = symbol.indexOf(".");
  return index < 0 ? ["", symbol] : [symbol.slice(0, index), symbol.slice(index + 1)];
}

/** `validation` → `Validation` (one-token Pascal). */
/** The deterministic operation id (the camel-case derivation). */
function effectiveOperationId(endpoint) {
  if (typeof endpoint.operationId === "string" && endpoint.operationId.length > 0) {
    return endpoint.operationId;
  }
  return endpoint.endpoint
    .split(".")
    .map((segment, index) =>
      index === 0
        ? segment
        : segment
            .split("_")
            .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
            .join(""),
    )
    .join("");
}

/** The RFC 6901 escaped operation pointer. */
function pathsPointer(template, method) {
  return `/paths/${template.replaceAll("~", "~0").replaceAll("/", "~1")}/${method}`;
}

/** The emitted `openapi` root member of one declared version. */
function versionWire(version) {
  return version === "3.0" ? "3.0.0" : "3.1.0";
}

/** The JSON content member of one schema. */
function jsonContent(schema) {
  return { content: { "application/json": { schema } }, required: true };
}

/** Sort object keys by unsigned UTF-8 bytes. */
function sortKeys(object) {
  return Object.fromEntries(Object.keys(object).sort(byKey).map((key) => [key, object[key]]));
}

/** The unsigned UTF-8 byte order comparison. */
function byKey(left, right) {
  const a = Buffer.from(left, "utf8");
  const b = Buffer.from(right, "utf8");
  const length = Math.min(a.length, b.length);
  for (let index = 0; index < length; index += 1) {
    if (a[index] !== b[index]) return a[index] - b[index];
  }
  return a.length - b.length;
}

/** Numeric map-key ordering for the status codes. */
function byNumericKey(left, right) {
  return Number(left[0]) - Number(right[0]);
}

/** Findings order: symbol then detail, byte-sorted, deduplicated. */
function compareFindings(left, right) {
  const symbol = byKey(left.symbol, right.symbol);
  return symbol !== 0 ? symbol : byKey(left.detail, right.detail);
}

/** The sha256 of exact bytes. */
function digestOf(bytes) {
  return "sha256:" + createHash("sha256").update(bytes).digest("hex");
}

/** Bound one echoed reason token. */
function bounded(text) {
  return String(text ?? "unknown").replace(/[^a-zA-Z0-9._: -]+/g, "?").slice(0, 128);
}

/** Build the write plan: the document, the ownership manifest, and the
 * pointer sidecar. Dry runs never write; applies publish the exact
 * bytes inside the permitted root.
 *
 * Fragments mode is ownership-aware emission (r1 F-7/cline F-2): the
 * maintained document is read back through the closed YAML reader and
 * the generated fragments merge into it — generator-owned pointers
 * replace their own bytes, absent pointers insert, unclaimed manual
 * content is preserved verbatim and never overwritten, and
 * generator-owned pointers absent from the new render are removed.
 * A maintained document outside the closed dialect refuses
 * (`existing-document-unparseable`) instead of a guess. Full mode
 * stays the declared wholesale regeneration of one generator-owned
 * document. */
function writePlan(context, rendered, policy) {
  const { request, readView, writeView } = context;
  const ownership = ownershipManifest(rendered);
  const map = pointerMap(rendered);
  let documentText;
  const mergeNotes = [];
  if (policy.mode === "fragments" && readView.canRead(policy.path)) {
    const existingBytes = readView.readFile(policy.path);
    if (existingBytes === undefined || existingBytes === null) {
      return { state: "failed", diagnostics: [{ reason: "existing-document-unreadable" }] };
    }
    let existingTree;
    try {
      existingTree = fromYaml(new TextDecoder("utf-8", { fatal: true }).decode(existingBytes));
    } catch (error) {
      return {
        state: "failed",
        diagnostics: [{ reason: "existing-document-unparseable", detail: error?.reason }],
      };
    }
    // A maintained document spelling another version than the
    // fragments are rendered at would merge mixed-dialect content;
    // the check path refuses the same way (r1 F-4).
    if (existingTree.openapi !== rendered.root.openapi) {
      return {
        state: "failed",
        diagnostics: [
          {
            reason: "existing-document-version",
            detail: bounded(`${existingTree.openapi}:${rendered.root.openapi}`),
          },
        ],
      };
    }
    const existingOwnership = readOwnershipManifest(readView, policy.path);
    const merged = mergeFragments(existingTree, existingOwnership, rendered, mergeNotes);
    documentText = toYaml(deepSort(merged));
  } else {
    // Full mode, or an unmanaged fragments target (first generation).
    documentText = toYaml(rendered.root);
  }
  const files = new Map([
    [policy.path, documentText],
    [sidecarPath(policy.path, "ownership.json"), `${canonicalJson(ownership)}\n`],
    [sidecarPath(policy.path, "map.json"), `${canonicalJson(map)}\n`],
  ]);
  const writes = [];
  for (const [path, text] of files) {
    writes.push({ path, action: writeView.exists(path) ? "replace" : "create", sha256: sha256Text(text) });
  }
  writes.sort((left, right) => (left.path < right.path ? -1 : left.path > right.path ? 1 : 0));
  if (request.dry_run === false) {
    if (!request.plan_id) {
      return { state: "failed", diagnostics: [{ reason: "missing-plan-id" }] };
    }
    for (const write of writes) {
      writeView.write(write.path, write.action, Buffer.from(files.get(write.path), "utf8"));
    }
  }
  return {
    state: "complete",
    data: {
      writes,
      // The wire reserves result.findings for validate/verify; the
      // partial projections ride as bounded evidence notes — the
      // document is emitted, and every unrenderable member is reported,
      // never silent (the transport notes precedent).
      findings: [],
      partial: [...rendered.findings, ...mergeNotes].slice(0, 16),
      bodies: files,
      plan_id: planIdOf(writes),
    },
    evidence: {
      document: policy.path,
      projectId: rendered.root.info.title,
      partialCount: rendered.findings.length + mergeNotes.length,
    },
  };
}

/** The existing ownership manifest of one maintained document, or the
 * empty default when none was shipped. */
function readOwnershipManifest(readView, documentPath) {
  const path = sidecarPath(documentPath, "ownership.json");
  if (!readView.canRead(path)) return { pointers: {} };
  const bytes = readView.readFile(path);
  if (bytes === undefined || bytes === null) return { pointers: {} };
  try {
    const parsed = JSON.parse(new TextDecoder("utf-8").decode(bytes));
    return parsed && typeof parsed === "object" ? parsed : { pointers: {} };
  } catch {
    return { pointers: {} };
  }
}

/** Merge the generated fragments of one render into the maintained
 * tree under the old ownership manifest. Generator-owned pointers
 * replace their own bytes; unclaimed manual content survives
 * verbatim — never overwritten; orphans of the old generator
 * ownership are dropped. Every divergence from the pure regeneration
 * is reported in `notes` (bounded by the caller). */
function mergeFragments(existingTree, existingOwnership, rendered, notes) {
  const oldOwners = existingOwnership?.pointers ?? {};
  const generated = new Map();
  for (const [pointer, endpoint] of rendered.pointers) {
    const parts = pointer.split("/");
    const template = (parts[2] ?? "").replaceAll("~1", "/").replaceAll("~0", "~");
    const method = parts[3] ?? "";
    generated.set(pointer, {
      value: rendered.root.paths?.[template]?.[method],
      owner: endpoint,
    });
  }
  const components = rendered.root.components ?? {};
  for (const [section, generatorOwned] of [
    ["schemas", false],
    ["responses", true],
    ["securitySchemes", true],
  ]) {
    for (const [name, value] of Object.entries(components?.[section] ?? {})) {
      const escaped = name.replaceAll("~", "~0").replaceAll("/", "~1");
      generated.set(`/components/${section}/${escaped}`, {
        value,
        owner: generatorOwned ? GENERATOR_ID : (value["x-lekalo-symbol"] ?? GENERATOR_ID),
      });
    }
  }

  const deepEqual = (left, right) => canonicalJson(left) === canonicalJson(right);
  const place = (pointer, value) => {
    const parts = pointer.split("/").slice(1);
    let node = merged;
    for (let index = 0; index < parts.length - 1; index += 1) {
      const key = parts[index].replaceAll("~1", "/").replaceAll("~0", "~");
      if (node[key] === undefined || node[key] === null || typeof node[key] !== "object") {
        node[key] = {};
      }
      node = node[key];
    }
    node[parts[parts.length - 1].replaceAll("~1", "/").replaceAll("~0", "~")] = value;
  };
  const remove = (pointer) => {
    const parts = pointer.split("/").slice(1);
    let node = merged;
    for (let index = 0; index < parts.length - 1; index += 1) {
      const key = parts[index].replaceAll("~1", "/").replaceAll("~0", "~");
      if (node === null || typeof node !== "object" || node[key] === undefined) return;
      node = node[key];
    }
    delete node[parts[parts.length - 1].replaceAll("~1", "/").replaceAll("~0", "~")];
  };

  // The core-merge posture (merge.rs starts from the maintained tree
  // and touches only claimed pointers): the merged tree begins as the
  // maintained document itself, so every unclaimed member — root
  // members like servers/tags/security/webhooks, manual x-*, hand-
  // edited info subfields — survives the apply untouched (r2 F-1).
  // The generator-identity members (the openapi wire member and the
  // self-pinning provenance block) are the only root members the
  // regeneration replaces: the fragments were rendered at the policy
  // version, so a preserved foreign wire member would corrupt the
  // document.
  const merged = structuredClone(existingTree);
  merged.openapi = rendered.root.openapi;
  merged["x-lekalo-provenance"] = rendered.root["x-lekalo-provenance"];
  for (const [pointer, fragment] of generated) {
    const oldValue = pointerValue(existingTree, pointer);
    const oldOwner = oldOwners[pointer];
    if (oldValue === undefined) {
      place(pointer, fragment.value);
      continue;
    }
    if (oldOwner === undefined || oldOwner === null) {
      // Unclaimed content is manual by definition: already in place
      // (the merged tree started as the maintained document), never
      // overwritten.
      notes.push({
        symbol: pointer,
        detail: deepEqual(oldValue, fragment.value) ? "manual-identical" : "merge-conflict",
      });
      continue;
    }
    place(pointer, fragment.value);
    if (!deepEqual(oldValue, fragment.value)) {
      notes.push({ symbol: pointer, detail: "generator-replaced" });
    }
  }
  // Manual content the new render does not generate survives in
  // place (it is already in the cloned tree); generator-owned
  // orphans (claimed, now absent) are dropped.
  for (const [template, item] of Object.entries(existingTree.paths ?? {})) {
    for (const [method, operation] of Object.entries(item ?? {})) {
      void operation;
      const pointer = pathsPointer(template, method);
      if (generated.has(pointer)) continue;
      if (oldOwners[pointer] === undefined || oldOwners[pointer] === null) {
        notes.push({ symbol: pointer, detail: "manual-preserved" });
      } else {
        remove(pointer);
        notes.push({ symbol: pointer, detail: "orphan-removed" });
      }
    }
  }
  for (const section of ["schemas", "responses", "securitySchemes"]) {
    for (const [name, value] of Object.entries(existingTree.components?.[section] ?? {})) {
      void value;
      const escaped = name.replaceAll("~", "~0").replaceAll("/", "~1");
      const pointer = `/components/${section}/${escaped}`;
      if (generated.has(pointer)) continue;
      if (oldOwners[pointer] === undefined || oldOwners[pointer] === null) {
        notes.push({ symbol: pointer, detail: "manual-preserved" });
      } else {
        remove(pointer);
        notes.push({ symbol: pointer, detail: "orphan-removed" });
      }
    }
  }
  return merged;
}

/** The value of one pointer in a tree, or nothing. */
function pointerValue(tree, pointer) {
  let node = tree;
  for (const part of pointer.split("/").slice(1)) {
    const key = part.replaceAll("~1", "/").replaceAll("~0", "~");
    if (node === null || typeof node !== "object") return undefined;
    node = node[key];
  }
  return node;
}

/** Recursively byte-sort every mapping of one merged tree: the merged
 * output is canonical regardless of where fragments landed. */
function deepSort(value) {
  if (Array.isArray(value)) return value.map(deepSort);
  if (value !== null && typeof value === "object") {
    return sortKeys(
      Object.fromEntries(Object.entries(value).map(([key, item]) => [key, deepSort(item)])),
    );
  }
  return value;
}

/** The plan identity of one write set (the composite union domain). */
export function planIdOf(writes) {
  return "plan-" + sha256Text(canonicalJson(writes)).slice("sha256:".length);
}

/** The ownership manifest of one render: every generated pointer with
 * the semantic id that owns it. Mirrors the core's
 * `OwnershipManifest::of_document`: operations by endpoint id, schemas
 * by their symbol, shared responses and security schemes by the
 * generator itself, and the canonical input digests recorded (r1
 * devin F-11) — so a check over our own output never classifies our
 * bytes as unowned manual inventory. */
function ownershipManifest(rendered) {
  const pointers = {};
  for (const [pointer, endpoint] of rendered.pointers) {
    pointers[pointer] = endpoint;
  }
  const components = rendered.root.components ?? {};
  for (const [section, owner] of [
    ["schemas", null],
    ["responses", GENERATOR_ID],
    ["securitySchemes", GENERATOR_ID],
  ]) {
    for (const [name, value] of Object.entries(components?.[section] ?? {})) {
      const pointer = `/components/${section}/${name.replaceAll("~", "~0").replaceAll("/", "~1")}`;
      if (pointers[pointer] !== undefined) continue;
      pointers[pointer] =
        owner ?? (value["x-lekalo-symbol"] !== undefined ? value["x-lekalo-symbol"] : GENERATOR_ID);
    }
  }
  const provenance = rendered.root["x-lekalo-provenance"] ?? {};
  const inputs = {};
  const modelDigest = provenance.modelRef?.digest;
  const irDigest = provenance.irRef?.digest;
  const transportDigest = provenance.transportRef?.digest;
  if (modelDigest) inputs.model = modelDigest;
  if (irDigest) inputs.ir = irDigest;
  if (transportDigest) inputs.transport = transportDigest;
  return {
    contract: OWNERSHIP_CONTRACT,
    generator: { id: GENERATOR_ID, version: GENERATOR_VERSION },
    inputs,
    pointers: sortKeys(pointers),
  };
}

/** The pointer→semantic-id sidecar of one render: every generated
 * pointer, schemas by symbol and generator-owned components by the
 * generator id (r1 devin F-11). */
function pointerMap(rendered) {
  const map = {};
  for (const [pointer, endpoint] of rendered.pointers) {
    map[pointer] = endpoint;
  }
  const components = rendered.root.components ?? {};
  for (const [name, schema] of Object.entries(components?.schemas ?? {})) {
    if (schema["x-lekalo-symbol"] !== undefined) {
      map[`/components/schemas/${name}`] = schema["x-lekalo-symbol"];
    }
  }
  for (const section of ["responses", "securitySchemes"]) {
    for (const name of Object.keys(components?.[section] ?? {})) {
      map[`/components/${section}/${name}`] = GENERATOR_ID;
    }
  }
  return sortKeys(map);
}

/** The sidecar path of one document path (`docs/openapi.yaml` →
 * `docs/openapi.<suffix>`). */
function sidecarPath(documentPath, suffix) {
  const stem = documentPath.replace(/\.yaml$/, "");
  return `${stem}.${suffix}`;
}
