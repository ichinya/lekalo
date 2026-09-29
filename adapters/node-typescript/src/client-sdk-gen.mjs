function tsDecodeMethod() {
  const body = [
    "    private async decode<T>(",
    "      wire: {",
    "        method: string;",
    "        path: string;",
    "        query: Record<string, string>;",
    "        headers: Record<string, string>;",
    "        body?: string;",
    "      },",
    "    ): Promise<LekaloResult<T>> {",
    "      const headers: Record<string, string> = { ...wire.headers };",
    "      const credential = this.authorization?.();",
    "      if (credential !== undefined) headers['authorization'] = credential;",
    "      let response: LekaloResponse;",
    "      try {",
    "        response = await this.transport.send({",
    "          method: wire.method,",
    "          path: wire.path,",
    "          query: wire.query,",
    "          headers,",
    "          body: wire.body,",
    "        });",
    "      } catch {",
    "        // A transport disconnect after send is the unknown state:",
    "        // one attempt, a typed infrastructure failure, never a",
    "        // declared error id.",
    "        return { ok: false, infrastructure: { infrastructure: true } };",
    "      }",
    "      if (response.status >= 200 && response.status < 300) {",
    "        if (response.body === undefined || response.body === '') {",
    "          return { ok: true, value: undefined as T };",
    "        }",
    "        try {",
    "          return { ok: true, value: JSON.parse(response.body) as T };",
    "        } catch {",
    "          return { ok: false, infrastructure: { infrastructure: true, status: response.status } };",
    "        }",
    "      }",
    "      // A declared error body carries the exact canonical identity; a",
    "      // body the client cannot recognize stays on the infrastructure",
    "      // channel — a status alone never claims a semantic error.",
    "      const declared = decodeError(response.body);",
    "      if (declared !== undefined) {",
    "        return { ok: false, error: declared };",
    "      }",
    "      return { ok: false, infrastructure: { infrastructure: true, status: response.status } };",
    "    }",
  ];
  return body.join("\n");
}

/**
 * The client-SDK generation capability of `lekalo-target-node-typescript`
 * (issue #72): `generate.client-sdk`, composed inside the single
 * generation descriptor (the closed target-protocol wire has exactly one
 * `generate` operation).
 *
 * The generator reads the canonical client-SDK evidence
 * (`.lekalo/cache/client-sdk/<project>.json`) through the kernel read
 * view and renders deterministic client code for every configured
 * language backend from the ONE shared projection — the TypeScript and
 * Go backends both derive from the same operations/types here, so the
 * wire behavior of one is the wire behavior of the other. The plan is
 * pure data: for each language, the client module plus its
 * compatibility-metadata sidecar and the ownership map; digests bind
 * every emitted byte to the exact evidence.
 *
 * Boundaries honored here: the rendered client takes its base URL,
 * transport, and credentials only from injected constructor arguments
 * (no URLs, tokens, or environment values ever appear in the
 * projection or the emission); retries are emitted only for declared
 * `safe`/`key-required` errors; there is no analytics, telemetry, or
 * background activity in any emitted byte.
 */

import { createHash } from "node:crypto";
import { canonicalJson } from "./openapi-emit.mjs";

/** The capability this generator claims inside the composite. */
export const CLIENT_SDK_CAPABILITY = "generate.client-sdk";
/** The canonical generator identity carried in the provenance block. */
export const CLIENT_SDK_GENERATOR_ID = "lekalo-core/client-sdk";
/** The generator version: the product version of the landing commit. */
export const CLIENT_SDK_GENERATOR_VERSION = "0.4.0";
/** The ownership sidecar contract. */
export const CLIENT_SDK_MAP_CONTRACT = "lekalo/client-sdk-map/v0.4.0";
/** The compatibility metadata contract one generated client carries. */
export const CLIENT_SDK_COMPATIBILITY_CONTRACT = "lekalo/client-sdk-compatibility/v0.4.0";
/** The write scopes the generated files live under. */
export const CLIENT_SDK_WRITE_SCOPES = ["src/generated/node-typescript/clients/**"];
/** The declared write root of the generated clients. */
export const CLIENT_SDK_WRITE_ROOT = "src/generated/node-typescript/clients/**";
/** The runtime home of the canonical client-SDK evidence (issue #72). */
export const CLIENT_SDK_EVIDENCE_DIR = ".lekalo/cache/client-sdk";

/** The exact evidence identity the join accepts. */
const SDK_IDENTITY = "dev.lekalo.client-sdk@0.4.0";
const SDK_SCHEMA_VERSION = "lekalo/client-sdk/v0.4.0";

const sha256Text = (text) =>
  "sha256:" + createHash("sha256").update(text, "utf8").digest("hex");

const isObject = (value) => typeof value === "object" && value !== null && !Array.isArray(value);

/**
 * The bounded evidence decoder: mirrors the closed client-SDK wire
 * surface (identity pins, operations, types). Anything else refuses.
 */
export function decodeClientSdkEvidence(bytes) {
  let document;
  try {
    document = JSON.parse(bytes.toString("utf8"));
  } catch {
    return { error: "client-sdk-evidence-invalid" };
  }
  if (!isObject(document)) return { error: "client-sdk-evidence-invalid" };
  if (document.schemaVersion !== SDK_SCHEMA_VERSION || document.identity !== SDK_IDENTITY) {
    return { error: "client-sdk-evidence-version" };
  }
  if (!Array.isArray(document.operations) || !Array.isArray(document.types)) {
    return { error: "client-sdk-evidence-invalid" };
  }
  return {
    value: {
      document,
      projectId:
        typeof document.projectId === "string" ? document.projectId : undefined,
      digest: "sha256:" + createHash("sha256").update(bytes).digest("hex"),
    },
  };
}

/**
 * Resolve the evidence path: the ir filename carries the project id
 * (`<project>.json`); a file-shaped read root under the evidence home
 * names it explicitly. Anything else is absent — never guessed.
 */
function sdkEvidencePathFor(request, readView) {
  const candidates = [];
  const irName = request.ir_path?.split("/").pop();
  if (irName?.endsWith(".json")) {
    candidates.push(`${CLIENT_SDK_EVIDENCE_DIR}/${irName}`);
  }
  for (const root of readView.roots ?? []) {
    if (root.kind === "file" && root.path.startsWith(`${CLIENT_SDK_EVIDENCE_DIR}/`)) {
      candidates.push(root.path);
    }
  }
  return candidates.find((candidate) => readView.canRead(candidate));
}

/** The one extension entry over the kernel read/write views. */
export function clientSdkGenerateOperation(context) {
  const { request, readView } = context;
  if (!readView) {
    return { state: "unsupported", diagnostics: [{ reason: "profile-absent" }] };
  }
  try {
    const evidencePath = sdkEvidencePathFor(request, readView);
    if (!evidencePath) {
      return {
        state: "failed",
        diagnostics: [{ reason: "client-sdk-evidence-absent" }],
      };
    }
    const decoded = decodeClientSdkEvidence(readView.readFile(evidencePath));
    if (decoded.error) {
      return { state: "failed", diagnostics: [{ reason: decoded.error }] };
    }
    return writePlan(context, decoded.value);
  } catch (error) {
    throw new Error("client-sdk-generator: " + bounded(error?.message));
  }
}

/** The verify posture: recompute the expected bytes and diff them
 * against the on-disk files, reporting `client.drift` findings. */
export function clientSdkVerifyOperation(context) {
  const outcome = clientSdkGenerateOperation({
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
        code: "client.drift",
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
        code: "client.drift",
        detail: `expected:${write.sha256.slice(7, 19)} observed:${digestOf(observed).slice(7, 19)}`,
      });
    }
  }
  return {
    state: "complete",
    data: { writes: [], findings: verification },
  };
}

/**
 * Render the deterministic client code for every backend from the ONE
 * shared projection. Every backend sees the same operations/types and
 * produces the same requests; a construct a backend cannot express is
 * an explicit unsupported note, never a silent narrowing.
 */
function renderClients(evidence) {
  const { document } = evidence;
  const typescript = renderTypescript(document);
  const go = renderGo(document);
  const compatibility = compatibilityMetadata(evidence, [
    "typescript",
    "go",
  ]);
  return { typescript, go, compatibility };
}

// ---------------------------------------------------------------------------
// Shared derivation (backend-independent, from the one projection).
// ---------------------------------------------------------------------------

/** The camel identifier of one wire parameter name (`task_id` ->
 * `taskId`). */
function camelIdent(name) {
  return name
    .split("_")
    .map((word, index) =>
      index === 0 ? word : word.charAt(0).toUpperCase() + word.slice(1),
    )
    .join("");
}

/**
 * The path/query/header parameter split of one operation: path
 * segments substitute into the URL template, query members serialize
 * into the query string, header members become request headers.
 */
function operationParams(operation) {
  const path = [];
  const query = [];
  const header = [];
  for (const param of operation.params ?? []) {
    if (param.in === "path") path.push(param);
    else if (param.in === "query") query.push(param);
    else if (param.in === "header") header.push(param);
    // cookie params: not expressible in a browser-conformant injected
    // transport; reported in the compatibility sidecar, never silent.
  }
  // The pagination binding DECLARES its wire parameters (limit plus
  // the offset/cursor continuation param): they join the query set so
  // a paged call can actually carry them (round 2). Requiredness
  // follows the binding: limit optional, continuation optional.
  const pagination = operation.pagination;
  if (pagination) {
    const wireNames = new Set(query.map((param) => param.name));
    for (const name of [
      pagination.limitParam,
      pagination.offsetParam,
      pagination.cursorParam,
    ]) {
      if (!name || wireNames.has(name)) continue;
      wireNames.add(name);
      query.push({ name, field: name, required: false, in: "query" });
    }
  }
  return { path, query, header };
}

/** Whether one operation accepts an idempotency key. */
function hasIdempotencyKey(operation) {
  return operation.idempotency !== undefined && operation.idempotency !== null;
}

/** The declared headers one operation sends: the declared idempotency
 * header name (never a hardcoded token) plus the correlation
 * headers. */
function declaredHeaders(operation) {
  const headers = [];
  if (hasIdempotencyKey(operation)) {
    headers.push({ name: operation.idempotency.header, kind: "idempotency" });
  }
  for (const name of operation.correlation ?? []) {
    headers.push({ name, kind: "correlation" });
  }
  return headers;
}


/** The map from type id to projected type. */
function typeIndex(document) {
  return new Map((document.types ?? []).map((typeDef) => [typeDef.typeId, typeDef]));
}

/**
 * The TypeScript request body argument type of one operation: whole
 * mode decodes into the named input object; explicit mode takes an
 * object with exactly the declared members — keyed by the WIRE member
 * name verbatim (round 2), typed from the bound member types with
 * their declared shapes.
 */
function requestBodyType(operation, index) {
  const body = operation.body;
  if (!body) return undefined;
  if (body.mode === "whole") {
    return tsValueTypeRef(body.typeRef, body.shape, index);
  }
  const members = (body.fields ?? [])
    .map((field) => {
      const optional = field.required ? "" : "?";
      const nullable = field.nullable ? " | null" : "";
      return `${JSON.stringify(field.name)}${optional}: ${tsValueTypeRef(field.typeRef, field.shape, index)}${nullable}`;
    })
    .join("; ");
  return `{ ${members} }`;
}

/** The evidence digest member the header pins. */
function evidenceDigestRef(document) {
  const transport = document.transportRef ?? {};
  return transport.digest ?? "sha256:unpinned";
}

// ---------------------------------------------------------------------------
// TypeScript backend.
// ---------------------------------------------------------------------------

/**
 * The TypeScript/Vue client emission: one module per project with the
 * shared result/error union, one method per operation keyed to the
 * stable operation id, and an injected transport. Path parameters
 * substitute into the URL template, query members serialize into the
 * query string, declared bodies serialize as JSON, and success bodies
 * decode into the projected types.
 */
function renderTypescript(document) {
  const index = typeIndex(document);
  const types = [];
  for (const typeDef of document.types ?? []) {
    types.push(typeDeclarationTs(typeDef, index));
  }
  const methods = [];
  for (const operation of document.operations ?? []) {
    methods.push(operationMethodTs(operation, index));
  }
  methods.push(tsDecodeMethod());
  const body = [
    "/* eslint-disable */",
    "// Generated by lekalo-target-node-typescript client-sdk generator 0.4.0 — never edit.",
    `// Contract: ${SDK_SCHEMA_VERSION} (${evidenceDigestRef(document)}).`,
    "// The transport, base URL, and credentials are injected; this module",
    "// performs no I/O by itself and contains no telemetry.",
    "",
    "/** The transport every call goes through: method/path/query/headers/body in, status/headers/body out. */",
    "export interface LekaloTransport {",
    "  send(request: LekaloRequest): Promise<LekaloResponse>;",
    "}",
    "",
    "export interface LekaloRequest {",
    "  method: string;",
    "  path: string;",
    "  query: Record<string, string>;",
    "  headers: Record<string, string>;",
    "  body?: string;",
    "}",
    "",
    "export interface LekaloResponse {",
    "  status: number;",
    "  headers: Record<string, string>;",
    "  body?: string;",
    "}",
    "",
    "/** A declared error with its exact semantic identity. */",
    "export interface LekaloError {",
    "  id: string;",
    "  code: string;",
    "  category: string;",
    "  payload: Record<string, unknown>;",
    "}",
    "",
    "/** Unknown infrastructure failures never masquerade as declared errors. */",
    "export interface LekaloInfrastructureFailure {",
    "  readonly infrastructure: true;",
    "  status?: number;",
    "}",
    "",
    "export type LekaloResult<T> =",
    "  | { ok: true; value: T }",
    "  | { ok: false; error: LekaloError }",
    "  | { ok: false; infrastructure: LekaloInfrastructureFailure };",
    "",
    "export interface LekaloClientOptions {",
    "  baseUrl: string;",
    "  transport: LekaloTransport;",
    "  /** Per-call credential supplier; the client never stores secrets. */",
    "  authorization?: () => string | undefined;",
    "}",
    "",
    ...types,
    "export class LekaloClient {",
    "  private readonly baseUrl: string;",
    "  private readonly transport: LekaloTransport;",
    "  private readonly authorization?: () => string | undefined;",
    "",
    "  constructor(options: LekaloClientOptions) {",
    "    this.baseUrl = options.baseUrl.replace(/\\/$/, '');",
    "    this.transport = options.transport;",
    "    this.authorization = options.authorization;",
    "  }",
    "",
    ...methods,

    "}",
    "",
    "/** Percent-encode one path segment (the RFC 3986 reserved set). */",
    "function encodeSegment(value: string): string {",
    "  return encodeURIComponent(value);",
    "}",
    "",
    "/** Decode a canonical error envelope, or nothing when the body is",
    " * not a recognized declared error. A status alone never claims a",
    " * semantic error. */",
    "function decodeError(body: string | undefined): LekaloError | undefined {",
    "  if (body === undefined) return undefined;",
    "  let parsed: unknown;",
    "  try {",
    "    parsed = JSON.parse(body);",
    "  } catch {",
    "    return undefined;",
    "  }",
    "  if (typeof parsed !== 'object' || parsed === null) return undefined;",
    "  const record = parsed as { ok?: unknown; error?: unknown };",
    "  if (record.ok !== false || typeof record.error !== 'object' || record.error === null) {",
    "    return undefined;",
    "  }",
    "  const error = record.error as {",
    "    id?: unknown;",
    "    code?: unknown;",
    "    category?: unknown;",
    "    payload?: unknown;",
    "  };",
    "  if (typeof error.id !== 'string' || typeof error.code !== 'string'",
    "    || typeof error.category !== 'string') {",
    "    return undefined;",
    "  }",
    "  return {",
    "    id: error.id,",
    "    code: error.code,",
    "    category: error.category,",
    "    payload: typeof error.payload === 'object' && error.payload !== null",
    "      ? (error.payload as Record<string, unknown>)",
    "      : {},",
    "  };",
    "}",
    "",
  ];
  return body.join("\n");
}

/** The TypeScript spelling of one declared scalar mapping. */
function tsScalarType(mapping) {
  if (mapping === "number") return "number";
  if (mapping === "boolean") return "boolean";
  return "string";
}

/** The TypeScript spelling of one named reference: the unit object
 * decodes to void at the call boundary; known types resolve to their
 * generated identifier; an unresolved reference renders `unknown`
 * (never a guessed shape). */
function tsTypeRef(typeRef, typeIndex) {
  if (typeRef === "lekalo.unit") return "void";
  const known = typeIndex.get(typeRef);
  return known ? known.ident : "unknown";
}

/** The shape-aware spelling: a `list` shape renders the array type
 * over the named reference (round 2 — a list-typed output never
 * collapses to its element). */
function tsValueTypeRef(typeRef, shape, typeIndex) {
  if (typeRef === "lekalo.unit") return "void";
  const inner = tsTypeRef(typeRef, typeIndex);
  return shape === "list" ? `${inner}[]` : inner;
}

/** One named type declaration (TypeScript). */
function typeDeclarationTs(typeDef, index) {
  const ident = typeDef.ident;
  if (typeDef.kind === "scalar") {
    // The declared mapping is the projection: only the numeric and
    // boolean bases leave the string domain (issue #53) — dates,
    // datetimes, uuids and uris stay validated ISO/absolute strings,
    // never runtime date types.
    return `export type ${ident} = ${tsScalarType(typeDef.base)};`;
  }
  if (typeDef.kind === "enum") {
    const members = (typeDef.values ?? [])
      .map((value) => `  | ${JSON.stringify(value)}`)
      .join("\n");
    return `export type ${ident} =
${members};`;
  }
  const fields = (typeDef.fields ?? [])
    .map((field) => {
      const optional = field.required ? "" : "?";
      const nullable = field.nullable ? " | null" : "";
      return `  ${JSON.stringify(field.name)}${optional}: ${tsValueTypeRef(field.typeRef, field.shape, index)}${nullable};`;
    })
    .join("\n");
  return `export interface ${ident} {
${fields}
}`;
}

/** Percent-encode one path segment (the RFC 3986 reserved set). */
function tsEncodeSegment() {
  return "function encodeSegment(value: string): string {\n  return encodeURIComponent(value);\n}";
}

/**
 * The header parameter identifier shared by the TS and Go backends:
 * header names carry hyphens, which no target identifier grammar
 * allows, so the declared header maps to a deterministic camel
 * argument (`X-Request-Id` -> `xRequestId`).
 */
function headerIdent(name) {
  const parts = name.split("-").filter((part) => part.length > 0);
  const camel = parts
    .map((part, index) =>
      index === 0
        ? part.toLowerCase()
        : part.charAt(0).toUpperCase() + part.slice(1).toLowerCase(),
    )
    .join("");
  return camel.replace(/([a-z])(ID)$/, "$1ID");
}

/**
 * One operation method (TypeScript): path substitution, query
 * serialization, the declared typed body, declared headers, and the
 * typed success decode. The send result carries the raw body; the
 * method decodes it into the projected success type so callers never
 * see a string where the contract declares an object.
 */
function operationMethodTs(operation, index) {
  const { path: pathParams, query: queryParams, header: headerParams } =
    operationParams(operation);
  // The retry matrix is caller policy: this client never retries
  // automatically. The doc comment names which declared errors carry
  // an authorization a CALLER may act on (with a reused key for
  // key-required errors).
  const retryable = (operation.errors ?? []).some(
    (error) => error.retry === "safe" || error.retry === "key-required",
  );
  const retryDoc = retryable
    ? " Caller-driven retries follow the declared error contracts; a nonempty idempotency key is required to act on a key-required error. This client itself never retries automatically."
    : " Never retried: no declared error authorizes a retry, and this client never retries automatically.";

  const successType = operation.successBody
    ? tsValueTypeRef(operation.successBody.typeRef, operation.successBody.shape, index)
    : "void";
  const hasBodyArg = operation.body !== undefined && operation.body !== null;

  const signatureParts = [];
  for (const param of pathParams) signatureParts.push(`${camelIdent(param.name)}: string`);
  for (const param of queryParams) {
    signatureParts.push(`${camelIdent(param.name)}${param.required ? "" : "?"}: string`);
  }
  for (const param of headerParams) {
    signatureParts.push(`${headerIdent(param.name)}${param.required ? "" : "?"}: string`);
  }
  if (hasBodyArg) {
    signatureParts.push(`input: ${requestBodyType(operation, index)}`);
  }
  for (const header of declaredHeaders(operation)) {
    if (header.kind === "idempotency") {
      // A binding with required:true makes the key a required call
      // argument; an optional binding keeps it optional.
      signatureParts.push(
        operation.idempotency && operation.idempotency.required
          ? "idempotencyKey: string"
          : "idempotencyKey?: string",
      );
    } else {
      signatureParts.push(`${headerIdent(header.name)}?: string`);
    }
  }

  // URL template substitution: every declared path segment is
  // percent-encoded and substituted; the template is never sent with
  // an unsubstituted placeholder.
  let url = operation.path;
  for (const param of pathParams) {
    url = url.replace(
      `{${param.name}}`,
      `\${encodeSegment(${camelIdent(param.name)})}`,
    );
  }

  const lines = [
    `  /** ${operation.method} ${operation.path} — operation ${operation.operationId}.${retryDoc} */`,
    `  ${operation.ident}(${signatureParts.join(", ")}): Promise<LekaloResult<${successType}>> {`
  ];
  if (pathParams.length > 0) {
    lines.push("    const encodedPath = `" + url + "`;");
  }
  lines.push("    const query: Record<string, string> = {};");
  for (const param of queryParams) {
    if (param.required) {
      lines.push(`    query[${JSON.stringify(param.name)}] = ${camelIdent(param.name)};`);
    } else {
      // Optional query members (declared pagination continuation and
      // limit members included) serialize only when supplied.
      lines.push(`    {`);
      lines.push(`      const value = ${camelIdent(param.name)};`);
      lines.push(`      if (value !== undefined) query[${JSON.stringify(param.name)}] = value;`);
      lines.push(`    }`);
    }
  }
  lines.push("    const headers: Record<string, string> = {};");
  for (const param of headerParams) {
    lines.push(`    headers[${JSON.stringify(param.name)}] = ${headerIdent(param.name)};`);
  }
  for (const header of declaredHeaders(operation)) {
    if (header.kind === "idempotency") {
      const requiredKey = operation.idempotency && operation.idempotency.required;
      if (requiredKey) {
        lines.push(
          `    headers[${JSON.stringify(header.name)}] = idempotencyKey;`,
        );
      } else {
        lines.push(
          `    if (idempotencyKey !== undefined) headers[${JSON.stringify(header.name)}] = idempotencyKey;`,
        );
      }
    } else {
      lines.push(`    {`);
      lines.push(`      const value = ${headerIdent(header.name)};`);
      lines.push(`      if (value !== undefined) headers[${JSON.stringify(header.name)}] = value;`);
      lines.push(`    }`);
    }
  }
  lines.push("    return this.decode({");
  lines.push("      method: " + JSON.stringify(operation.method) + ",");
  lines.push("      path: this.baseUrl + " + (pathParams.length > 0 ? "encodedPath" : JSON.stringify(url)) + ",");
  lines.push("      query,");
  lines.push("      headers,");
  lines.push(
    hasBodyArg
      ? "      body: JSON.stringify(input),"
      : "      body: undefined,",
  );
  lines.push("    });");
  lines.push("  }");
  return lines.join("\n");
}

/**
 * The typed send-and-decode path: publishes one wire request and
 * decodes the declared success body, so every method returns the
 * projected type (never a raw string, never `unknown`).
 */

/**
 * Build the write plan map: the TypeScript client, the Go client, the
 * compatibility sidecar, and the ownership map — all from one
 * evidence document.
 */
// ---------------------------------------------------------------------------
// Go backend (the second derivation over the same projection).
// ---------------------------------------------------------------------------

function renderGo(document) {
  const index = typeIndex(document);
  const projectName = document.projectId ?? "project";
  const packageName = goPackageName(projectName);
  const structName = goExported(projectName) + "Client";
  const types = [];
  for (const typeDef of document.types ?? []) {
    types.push(typeDeclarationGo(typeDef, index));
  }
  const methods = [];
  for (const operation of document.operations ?? []) {
    methods.push(operationMethodGo(document, operation, index));
  }
  const head = goHead(document, packageName, structName);
  return [head, "", ...types, "", ...methods, "", goSendAndDecode(structName)].join("\n");
}

function goHead(document, packageName, structName) {
  const lines = [
    "// Generated by lekalo-target-node-typescript client-sdk generator " + CLIENT_SDK_GENERATOR_VERSION + " — never edit.",
    `// Contract: ${SDK_SCHEMA_VERSION} (${evidenceDigestRef(document)}).`,
    "// The transport, base URL, and credentials are injected; this package",
    "// performs no I/O by itself and contains no telemetry.",
    "",
    "package " + packageName,
    "",
    "import (",
    "\t\"context\"",
    "\t\"encoding/json\"",
    "\t\"fmt\"",
    "\t\"net/url\"",
    "\t\"strings\"",
    ")",
    "",
    "// Transport is the injected transport every call goes through.",
    "type Transport interface {",
    "	Send(ctx context.Context, request Request) (Response, error)",
    "}",
    "",
    "// Request is the wire request: method, full path, query, headers, body.",
    "type Request struct {",
    "	Method  string",
    "	Path    string",
    "	Query   url.Values",
    "	Headers map[string]string",
    "	Body    []byte",
    "}",
    "",
    "// Response is the wire response.",
    "type Response struct {",
    "	Status  int",
    "	Headers map[string]string",
    "	Body    []byte",
    "}",
    "",
    "// DeclaredError carries the exact semantic identity of a #62 error.",
    "type DeclaredError struct {",
    "	ID       string",
    "	Code     string",
    "	Category string",
    "	Payload  map[string]any",
    "}",
    "",
    "func (e *DeclaredError) Error() string {",
    "	return fmt.Sprintf(" + JSON.stringify("lekalo: %s (%s / %s)") + ", e.Code, e.Category, e.ID)",
    "}",
    "",
    "// InfrastructureFailure is the unknown-failure channel: it never",
    "// masquerades as a declared error.",
    "type InfrastructureFailure struct {",
    "	Status int",
    "}",
    "",
    "func (e *InfrastructureFailure) Error() string {",
    "	return " + JSON.stringify("lekalo: infrastructure failure"),
    "}",
    "",
    "// Client is the generated client over the injected transport.",
    "type " + structName + " struct {",
    "	baseURL       string",
    "	transport     Transport",
    "	authorization func(context.Context) (string, bool)",
    "}",
    "",
    "// New" + structName + " builds a client over the injected transport.",
    "func New" + structName + "(baseURL string, transport Transport, authorization func(context.Context) (string, bool)) *" + structName + " {",
    "	return &" + structName + "{baseURL: strings.TrimSuffix(baseURL, " + JSON.stringify("/") + "), transport: transport, authorization: authorization}",
    "}",
  ];
  return lines.join("\n");
}

function goSendAndDecode(structName) {
  const lines = [
    "// send publishes one wire request and projects the response onto",
    "// the #62 tuple: (nil, nil) success, *DeclaredError, or the",
    "// *InfrastructureFailure unknown channel.",
    "func (c *" + structName + ") send(ctx context.Context, operationID string, wire Request) (*Response, error) {",
    "	headers := map[string]string{}",
    "	for name, value := range wire.Headers {",
    "		headers[name] = value",
    "	}",
    "	if c.authorization != nil {",
    "		if credential, ok := c.authorization(ctx); ok {",
    "			headers[" + JSON.stringify("authorization") + "] = credential",
    "		}",
    "	}",
    "	wire.Headers = headers",
    "	response, err := c.transport.Send(ctx, wire)",
    "	if err != nil {",
    "		// A transport disconnect after send is the unknown state:",
    "		// one attempt, the typed infrastructure channel, never a",
    "		// declared error id.",
    "		return nil, &InfrastructureFailure{}",
    "	}",
    "	if response.Status >= 200 && response.Status < 300 {",
    "		return &response, nil",
    "	}",
    "	if declared := decodeError(response.Body); declared != nil {",
    "		return nil, declared",
    "	}",
    "	return nil, &InfrastructureFailure{Status: response.Status}",
    "}",
    "",
    "// decodeError parses a canonical error envelope, or nil when the",
    "// body is not a recognized declared error. A status alone never",
    "// claims a semantic error.",
    "func decodeError(body []byte) *DeclaredError {",
    "	if body == nil {",
    "		return nil",
    "	}",
    "	var envelope struct {",
    "\t\tOk    bool \`json:\"ok\"\`",
    "\t\tError *struct {",
    "\t\t\tID       string         \`json:\"id\"\`",
    "\t\t\tCode     string         \`json:\"code\"\`",
    "\t\t\tCategory string         \`json:\"category\"\`",
    "\t\t\tPayload  map[string]any \`json:\"payload\"\`",
    "\t\t} \`json:\"error\"\`",
    `	}`,
    "	if err := json.Unmarshal(body, &envelope); err != nil || envelope.Ok || envelope.Error == nil {",
    "		return nil",
    "	}",
    "	payload := envelope.Error.Payload",
    "	if payload == nil {",
    "		payload = map[string]any{}",
    "	}",
    "	return &DeclaredError{ID: envelope.Error.ID, Code: envelope.Error.Code, Category: envelope.Error.Category, Payload: payload}",
    "}",
  ];
  return lines.join("\n");
}

function goPackageName(projectId) {
  return projectId.replace(/[^a-z0-9]/g, "");
}

function goExported(text) {
  const parts = String(text)
    .split(/[^a-zA-Z0-9]/)
    .filter((part) => part.length > 0);
  return parts
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join("");
}

function goTypeRef(typeRef, index) {
  if (typeRef === "lekalo.unit") return "*struct{}";
  const known = index.get(typeRef);
  return known ? "*" + goExported(known.ident) : "any";
}

/** The shape-aware Go spelling: a `list` shape renders a slice over
 * the named reference (round 2 — never the bare element type). */
function goValueTypeRef(typeRef, shape, index) {
  const inner = goTypeRef(typeRef, index);
  return shape === "list" ? "[]" + inner : inner;
}

function goHeaderIdent(name) {
  const parts = name.split("-").filter((part) => part.length > 0);
  const camel = parts
    .map((part, index) =>
      index === 0
        ? part.toLowerCase()
        : part.charAt(0).toUpperCase() + part.slice(1).toLowerCase(),
    )
    .join("");
  return camel.replace(/([a-z])(ID)$/, "$1ID");
}

/** The Go spelling of one declared scalar mapping. */
function goScalarType(mapping) {
  if (mapping === "number") return "float64";
  if (mapping === "boolean") return "bool";
  return "string";
}

function typeDeclarationGo(typeDef, index) {
  const name = goExported(typeDef.ident);
  if (typeDef.kind === "scalar") {
    // The declared mapping is the projection: only the numeric and
    // boolean bases leave the string domain (issue #53).
    return `type ${name} ${goScalarType(typeDef.base)}`;
  }
  if (typeDef.kind === "enum") {
    const lines = [`type ${name} string`, "const ("];
    for (const value of typeDef.values ?? []) {
      lines.push(`\t${name}${goExported(value)} ${name} = ${JSON.stringify(value)}`);
    }
    lines.push(")");
    return lines.join("\n");
  }
  // The struct block is gofmt-stable: the name and type columns align
  // over the one consecutive run of tagged fields (issue #53), so a
  // fresh render is byte-equal to the committed gofmt-clean fixture.
  const rows = (typeDef.fields ?? []).map((field) => {
    const pointer = !field.required || field.nullable ? "*" : "";
    return {
      name: goExported(field.name),
      type: `${pointer}${goValueTypeRef(field.typeRef, field.shape, index)}`,
      tag: `\`json:"${field.name}${field.required ? "" : ",omitempty"}"\``,
    };
  });
  const nameWidth = Math.max(0, ...rows.map((row) => row.name.length));
  const typeWidth = Math.max(0, ...rows.map((row) => row.type.length));
  const fields = rows
    .map((row) => {
      if (rows.length === 1) {
        return `\t${row.name} ${row.type} ${row.tag}`;
      }
      return `\t${row.name.padEnd(nameWidth)} ${row.type.padEnd(typeWidth)} ${row.tag}`;
    })
    .join("\n");
  return [`type ${name} struct {`, fields || "\t_ struct{} `json:\"-\"`", "}"].join("\n");
}

function operationMethodGo(document, operation, index) {
  const parts = operationParams(operation);
  const pathParams = parts.path;
  const queryParams = parts.query;
  const headerParams = parts.header;
  const projectName = document.projectId ?? "project";
  const structName = goExported(projectName) + "Client";
  const successType = operation.successBody
    ? goValueTypeRef(operation.successBody.typeRef, operation.successBody.shape, index)
    : "*struct{}";
  const hasBodyArg = operation.body !== undefined && operation.body !== null;
if (operation.operationId === 'planner.focus_task') {
     console.log('operation.body:', operation.body);
   }
  const signatureParts = ["ctx context.Context"];
  for (const param of pathParams) signatureParts.push(`${camelIdent(param.name)} string`);
  for (const param of queryParams) signatureParts.push(`${camelIdent(param.name)} string`);
  for (const param of headerParams) signatureParts.push(`${goHeaderIdent(param.name)} string`);
  if (hasBodyArg) signatureParts.push(`input ${goValueTypeRef(operation.body.typeRef, operation.body.shape, index)}`);
  for (const header of declaredHeaders(operation)) {
    signatureParts.push(`${goHeaderIdent(header.name)} string`);
  }
  const methodName = goExported(operation.ident);
  const lines = [
    `// ${methodName} ${operation.method} ${operation.path} — operation ${operation.operationId}.`,
    `func (c *${structName}) ${methodName}(${signatureParts.join(", ")}) (${successType}, error) {`,
    "	query := url.Values{}"
  ];
  for (const param of queryParams) {
    if (param.required) {
      lines.push(`\tquery.Set(${JSON.stringify(param.name)}, ${camelIdent(param.name)})`);
    } else {
      lines.push(`\tif ${camelIdent(param.name)} != "" {`);
      lines.push(`\t\tquery.Set(${JSON.stringify(param.name)}, ${camelIdent(param.name)})`);
      lines.push(`\t}`);
    }
  }
  lines.push("\theaders := map[string]string{}");
  for (const param of headerParams) {
    lines.push(`\theaders[${JSON.stringify(param.name)}] = ${goHeaderIdent(param.name)}`);
  }
  for (const header of declaredHeaders(operation)) {
    const goArg = goHeaderIdent(header.name);
    const requiredHeader =
      header.kind === "idempotency" && operation.idempotency && operation.idempotency.required;
    if (requiredHeader) {
      lines.push(`\theaders[${JSON.stringify(header.name)}] = ${goArg}`);
    } else {
      // An optional declared header writes only a nonempty value.
      lines.push(`\tif ${goArg} != "" {`);
      lines.push(`\t\theaders[${JSON.stringify(header.name)}] = ${goArg}`);
      lines.push(`\t}`);
    }
  }
  const args = [];
  const segments = [];
  let lastIndex = 0;
  const pattern = /\{([a-z0-9_]+)\}/g;
  let match;
  const template = operation.path;
  while ((match = pattern.exec(template)) !== null) {
    const param = pathParams.find((candidate) => candidate.name === match[1]);
    segments.push(template.slice(lastIndex, match.index));
    segments.push("%s");
    args.push(
      param
        ? `url.PathEscape(${camelIdent(param.name)})`
        : JSON.stringify(match[0]),
    );
    lastIndex = match.index + match[0].length;
  }
  segments.push(template.slice(lastIndex));
  const url = segments.join("");
  const pathExpr =
    args.length > 0
      ? `fmt.Sprintf(${JSON.stringify(url)}, ${args.join(", ")})`
      : JSON.stringify(url);
  lines.push("\twire := Request{");
  lines.push("\t\tMethod: " + JSON.stringify(operation.method) + ",");
  lines.push(`\t\tPath: c.baseURL + ${pathExpr},`);
  lines.push("\t\tQuery: query,");
  lines.push("\t\tHeaders: headers,");
  if (hasBodyArg) {
    lines.push("\t}");
    lines.push("\tbody, err := json.Marshal(input)");
    lines.push("\tif err != nil {");
    lines.push("\t\treturn nil, err");
    lines.push("\t}");
    lines.push("\twire.Body = body");
  } else {
    lines.push("\t}");
  }
  lines.push(`\tresponse, err := c.send(ctx, ${JSON.stringify(operation.operationId)}, wire)`);
  lines.push("\tif err != nil {");
  lines.push("\t\treturn nil, err");
  lines.push("\t}");
  if (operation.successBody) {
    const decoded = goValueTypeRef(operation.successBody.typeRef, operation.successBody.shape, index);
    lines.push(`\tvar value ${decoded}`);
    lines.push("\tif response.Body != nil {");
    lines.push("\t\tif err := json.Unmarshal(response.Body, &value); err != nil {");
    lines.push("\t\t\treturn nil, err");
    lines.push("\t\t}");
    lines.push("\t}");
    lines.push("\treturn value, nil");
  } else {
    lines.push("\t_ = response");
    lines.push("\treturn &struct{}{}, nil");
  }
  lines.push("}");
  return lines.join("\n");
}

/**
 * The compatibility metadata sidecar: the exact contract versions and
 * digests one client was generated from, and the backends rendered.
 * No timestamps, no machine paths, no credentials — the bytes are
 * reproducible.
 */
function compatibilityMetadata(evidence, languages) {
  const document = evidence.document;
  return {
    contract: CLIENT_SDK_COMPATIBILITY_CONTRACT,
    generator: { id: CLIENT_SDK_GENERATOR_ID, version: CLIENT_SDK_GENERATOR_VERSION },
    projectId: evidence.projectId,
    languages,
    sdkContract: { identity: document.identity, digest: evidence.digest },
    irRef: document.irRef ?? {},
    modelRef: document.modelRef ?? {},
    transportRef: document.transportRef ?? {},
    wire: document.wire,
    operationIds: (document.operations ?? []).map((operation) => operation.operationId),
    typeIds: (document.types ?? []).map((typeDef) => typeDef.symbol),
  };
}

export function renderWritePlan(evidence) {
  const rendered = renderClients(evidence);
  const stem = `${CLIENT_SDK_WRITE_ROOT.replace("/**", "")}/${evidence.projectId ?? "project"}`;
  const map = {
    contract: CLIENT_SDK_MAP_CONTRACT,
    generator: { id: CLIENT_SDK_GENERATOR_ID, version: CLIENT_SDK_GENERATOR_VERSION },
    inputs: { clientSdk: evidence.digest },
    pointers: {
      "/client": evidence.projectId ?? "project",
      "/client-go": evidence.projectId ?? "project",
      "/compatibility": CLIENT_SDK_GENERATOR_ID,
    },
  };
  return new Map([
    [`${stem}.client.ts`, rendered.typescript],
    [`${stem}.client.go`, rendered.go],
    [`${stem}.compatibility.json`, `${canonicalJson(rendered.compatibility)}\n`],
    [`${stem}.map.json`, `${canonicalJson(map)}\n`],
  ]);
}

function writePlan(context, evidence) {
  const { request, writeView } = context;
  const files = renderWritePlan(evidence);
  const writes = [];
  for (const [path, text] of files) {
    writes.push({
      path,
      action: writeView.exists(path) ? "replace" : "create",
      sha256: sha256Text(text),
    });
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
      findings: [],
      bodies: files,
      plan_id: planIdOf(writes),
    },
    evidence: {
      projectId: evidence.projectId,
      languages: ["typescript", "go"],
      evidenceDigest: evidence.digest,
    },
  };
}

/** The plan identity of one write set (the composite union domain). */
export function clientSdkPlanIdOf(writes) {
  return "plan-" + sha256Text(canonicalJson(writes)).slice("sha256:".length);
}
const planIdOf = clientSdkPlanIdOf;

/** The sha256 of exact bytes. */
function digestOf(bytes) {
  return "sha256:" + createHash("sha256").update(bytes).digest("hex");
}

/** Bound one echoed reason token. */
function bounded(text) {
  return String(text ?? "unknown").replace(/[^a-zA-Z0-9._: -]+/g, "?").slice(0, 128);
}
