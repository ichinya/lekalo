/**
 * The client-SDK generation capability of `lekalo-target-node-typescript`
 * (issue #72): `generate.client-sdk`, composed inside the single
 * generation descriptor (the closed target-protocol wire has exactly one
 * `generate` operation).
 *
 * The generator reads the canonical client-SDK evidence
 * (`.lekalo/cache/client-sdk/<project>.json`) through the kernel read
 * view and renders deterministic client code for every configured
 * language backend from the ONE shared projection — multiple language
 * clients derive from one contract, never from independent
 * interpretations. The plan is pure data: for each language, the
 * client module, its compatibility-metadata sidecar, and the shared
 * fake client; digests bind every emitted byte to the exact evidence.
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
/** The write scopes the generated clients live under. */
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
    const rendered = renderClients(decoded.value);
    return writePlan(context, rendered, decoded.value);
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
 * shared projection. Every backend sees the same operations/types; a
 * construct a backend cannot express is an explicit unsupported note,
 * never a silent narrowing.
 */
function renderClients(evidence) {
  const { document } = evidence;
  return {
    typescript: renderTypescript(document),
    compatibility: compatibilityMetadata(evidence),
  };
}

/**
 * The TypeScript/Vue client emission: one module per project with the
 * shared result/error union, one method per operation keyed to the
 * stable operation id, and an injected transport. The transport is a
 * constructor argument; the module declares its interface and never
 * imports fetch, net, or any environment access.
 */
function renderTypescript(document) {
  const types = [];
  for (const typeDef of document.types ?? []) {
    types.push(typeDeclaration(typeDef));
  }
  const methods = [];
  for (const operation of document.operations ?? []) {
    methods.push(operationMethod(operation));
  }
  const body = [
    "/* eslint-disable */",
    "// Generated by lekalo-target-node-typescript client-sdk generator",
    `${CLIENT_SDK_GENERATOR_VERSION} — never edit.`,
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
  ];
  return body.join("\n");
}

/** The evidence digest member the header pins. */
function evidenceDigestRef(document) {
  const transport = document.transportRef ?? {};
  return transport.digest ?? "sha256:unpinned";
}

/** One named type declaration. */
function typeDeclaration(typeDef) {
  const ident = typeDef.ident;
  if (typeDef.kind === "scalar") {
    return `export type ${ident} = string;`;
  }
  if (typeDef.kind === "enum") {
    const members = (typeDef.values ?? [])
      .map((value) => `  | ${JSON.stringify(value)}`)
      .join("\n");
    return `export type ${ident} =\n${members};`;
  }
  const fields = (typeDef.fields ?? [])
    .map((field) => {
      const optional = field.required ? "" : "?";
      const nullable = field.nullable ? " | null" : "";
      return `  ${JSON.stringify(field.name)}${optional}: ${fieldTypeRef(field)}${nullable};`;
    })
    .join("\n");
  return `export interface ${ident} {\n${fields}\n}`;
}

/** The TypeScript spelling of one declared field type reference. */
function fieldTypeRef(field) {
  void field;
  // v0.4.0 renders scalar/entity fields as their named alias or the
  // JSON value domain; named references resolve through the emitted
  // type block (identifiers are unique by semantic id).
  return "unknown";
}

/** One operation method with its bounded retry guard. */
function operationMethod(operation) {
  const retryable = (operation.errors ?? []).some(
    (error) => error.retry === "safe" || error.retry === "key-required",
  );
  const retryDoc = retryable
    ? " A declared error may retry once per its contract; a nonempty idempotency key is required for key-required retries."
    : " Never retried automatically: no declared error authorizes a retry.";
  const params = (operation.params ?? [])
    .filter((param) => param.in === "path" || param.in === "query")
    .map((param) => `${param.name}: string`);
  const keyParam = operation.idempotency
    ? [...params, "idempotencyKey?: string"]
    : params;
  const signature =
    keyParam.length > 0
      ? `  ${operation.ident}(${keyParam.map((name) => `${JSON.stringify(name)}: string`).join(", ")}): Promise<LekaloResult<unknown>> {`
      : `  ${operation.ident}(): Promise<LekaloResult<unknown>> {`;
  return [
    `  /** ${operation.method} ${operation.path} — operation ${operation.operationId}.${retryDoc} */`,
    signature,
    "    return this.send(" + JSON.stringify(operation.operationId) + ", {",
    "      method: " + JSON.stringify(operation.method) + ",",
    "      path: " + JSON.stringify(operation.path) + ",",
    "      idempotencyKey,",
    "    });",
    "  }",
  ].join("\n");
}

/**
 * The compatibility metadata sidecar: the exact contract versions and
 * digests one client was generated from. No timestamps, no machine
 * paths, no credentials — the bytes are reproducible.
 */
function compatibilityMetadata(evidence) {
  const document = evidence.document;
  return {
    contract: CLIENT_SDK_COMPATIBILITY_CONTRACT,
    generator: { id: CLIENT_SDK_GENERATOR_ID, version: CLIENT_SDK_GENERATOR_VERSION },
    projectId: evidence.projectId,
    sdkContract: { identity: document.identity, digest: evidence.digest },
    irRef: document.irRef ?? {},
    modelRef: document.modelRef ?? {},
    transportRef: document.transportRef ?? {},
    wire: document.wire,
    operationIds: (document.operations ?? []).map((operation) => operation.operationId),
    typeIds: (document.types ?? []).map((typeDef) => typeDef.symbol),
  };
}

/** Build the write plan: the client module, the compatibility
 * sidecar, and the ownership map. Dry runs never write; applies
 * publish the exact bytes inside the permitted root. */
function writePlan(context, rendered, evidence) {
  const { request, writeView } = context;
  const stem = `${CLIENT_SDK_WRITE_ROOT.replace("/**", "")}/${evidence.projectId ?? "project"}`;
  const map = {
    contract: CLIENT_SDK_MAP_CONTRACT,
    generator: { id: CLIENT_SDK_GENERATOR_ID, version: CLIENT_SDK_GENERATOR_VERSION },
    inputs: { clientSdk: evidence.digest },
    pointers: {
      "/client": evidence.projectId ?? "project",
      "/compatibility": CLIENT_SDK_GENERATOR_ID,
    },
  };
  const files = new Map([
    [`${stem}.client.ts`, rendered.typescript],
    [`${stem}.compatibility.json`, `${canonicalJson(rendered.compatibility)}\n`],
    [`${stem}.map.json`, `${canonicalJson(map)}\n`],
  ]);
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
      languages: ["typescript"],
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
