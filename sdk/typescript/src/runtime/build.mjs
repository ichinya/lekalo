/**
 * Response builders and the validate-before-write boundary (issue #106).
 *
 * Success is operation-specific and never silent: a builder output that
 * fails schema bounds, identity echo, pairing, or digest rules throws an
 * `SdkBuildError` before a single stdout byte is written. For a known
 * request the caller can fall back to a valid closed error envelope;
 * for undecodable input or invalid builder output the runtime emits a
 * bounded safe stderr line and exits nonzero — never a fabricated success.
 */
import { BOUNDS, ENUMS, WIRE_TOKEN } from "../generated/protocol.mjs";
import { canonicalJson, canonicalJson as canonical } from "./json.mjs";
import { isContractVersion, isToken } from "./paths.mjs";
import { isPlanId, isSha256Digest } from "./ids.mjs";
import { validateResponsePayload } from "./validate.mjs";
import { sortWritePlan } from "./canonical.mjs";

/** Thrown when a builder output would violate the closed wire contract. */
export class SdkBuildError extends Error {
  constructor(reason, detail) {
    super(`sdk-build-error: ${reason}${detail ? ` (${detail})` : ""}`);
    this.name = "SdkBuildError";
    this.reason = reason;
  }
}

const ADAPTER_MAX = { code: 128, detail: 128, message: 256 };

/** Build and validate one adapter identity block. */
export function buildAdapterIdentity({ digest, id, version }) {
  if (!isToken(id)) throw new SdkBuildError("identity", "id");
  if (!isContractVersion(version)) throw new SdkBuildError("identity", "version");
  if (!isSha256Digest(digest)) throw new SdkBuildError("identity", "digest");
  return { digest, id, version };
}

/**
 * Build one `describe` capability map. Declared scopes and capability
 * states are honest claims the host will police — describing more than
 * the adapter implements is a refusal, never a capability gain.
 */
export function buildCapabilities({ adapter, constraints, capabilities, irVersions, operations, profiles, progress, protocolVersions, readScopes, targets, transports, writeScopes }) {
  const declared = {
    adapter,
    capabilities: sortedCapabilityMap(capabilities),
    constraints: constraints ?? undefined,
    ir_versions: boundedUnique(irVersions ?? [], BOUNDS.MAX_IR_VERSIONS, isContractVersion, "ir_versions"),
    operations: boundedUnique(operations, BOUNDS.OPERATIONS_MAX_LINT, (op) => typeof op === "string", "operations"),
    profiles: boundedUnique(profiles ?? [], BOUNDS.PROFILES_MAX, isToken, "profiles"),
    progress: progress === true,
    protocol_versions: boundedUnique(protocolVersions, BOUNDS.PROTOCOL_VERSIONS_MAX, isContractVersion, "protocol_versions"),
    read_scopes: boundedUnique(readScopes ?? [], BOUNDS.MAX_SCOPES, (scope) => typeof scope === "string" && scope.length > 0, "read_scopes"),
    targets: boundedUnique(targets ?? [], BOUNDS.TARGETS_MAX, isToken, "targets"),
    transports: boundedUnique(transports, BOUNDS.TRANSPORTS_MAX, (t) => ENUMS.transport.includes(t), "transports"),
    write_scopes: boundedUnique(writeScopes ?? [], BOUNDS.MAX_SCOPES, (scope) => typeof scope === "string" && scope.length > 0, "write_scopes"),
  };
  for (const key of Object.keys(declared)) {
    if (declared[key] === undefined) delete declared[key];
  }
  if (declared.capabilities !== undefined && Object.keys(declared.capabilities).length === 0) delete declared.capabilities;
  return declared;
}

function sortedCapabilityMap(capabilities) {
  if (capabilities === undefined) return undefined;
  if (capabilities === null || typeof capabilities !== "object" || Array.isArray(capabilities)) {
    throw new SdkBuildError("capabilities", "map");
  }
  const entries = Object.entries(capabilities);
  if (entries.length > BOUNDS.MAX_DECLARED_CAPABILITIES) throw new SdkBuildError("capabilities", "count");
  const sorted = {};
  for (const key of Object.keys(capabilities).sort()) {
    if (!ENUMS.supportState.includes(capabilities[key])) throw new SdkBuildError("capabilities", key);
    sorted[key] = capabilities[key];
  }
  return sorted;
}

function boundedUnique(values, max, check, label) {
  if (!Array.isArray(values) || values.length > max) throw new SdkBuildError("bounds", label);
  const unique = [...new Set(values)].sort();
  for (const value of unique) {
    if (!check(value)) throw new SdkBuildError("bounds", label);
  }
  return unique;
}

/**
 * Build one success envelope for the request, validate it against the
 * full closed contract, and return it. Throws `SdkBuildError` instead of
 * ever returning a malformed success.
 */
export function buildSuccess({ adapter, capabilities, payload, planId, progress, request, writes }) {
  const envelope = {
    evidence: planId === undefined ? { adapter } : { adapter, plan_id: planId },
    operation: request.operation,
    protocol: WIRE_TOKEN,
    protocol_version: request.protocol_version,
    request_id: request.request_id,
    status: "ok",
  };
  if (request.operation === "describe") {
    envelope.capabilities = capabilities;
  }
  if (payload !== undefined) {
    envelope.result = payload;
  }
  if (writes !== undefined) {
    envelope.writes = writes;
  }
  if (progress !== undefined) {
    envelope.progress = progress;
  }
  return finishEnvelope(envelope, request);
}

/**
 * Build one closed in-envelope error envelope for the request. The class
 * is closed, the code/message bounded; partial errors stay illegal here
 * (the host negotiates partial handling, templates never need it).
 */
export function buildError({ adapter, detail, errorCode, errorClass, message, request }) {
  if (!ENUMS.errorClass.includes(errorClass)) throw new SdkBuildError("error-class", errorClass);
  if (typeof errorCode !== "string" || errorCode === "" || [...errorCode].length > ADAPTER_MAX.code) {
    throw new SdkBuildError("error-code");
  }
  if (typeof message !== "string" || message === "" || [...message].length > ADAPTER_MAX.message) {
    throw new SdkBuildError("error-message");
  }
  if (detail !== undefined) {
    if (!Array.isArray(detail) || detail.length > BOUNDS.ERROR_DETAILS_MAX) throw new SdkBuildError("error-detail");
    for (const item of detail) {
      if (typeof item !== "string" || [...item].length > ADAPTER_MAX.detail) throw new SdkBuildError("error-detail");
    }
  }
  const envelope = {
    error: { class: errorClass, code: errorCode, message, ...(detail === undefined ? {} : { detail }) },
    evidence: { adapter },
    operation: request.operation,
    protocol: WIRE_TOKEN,
    protocol_version: request.protocol_version,
    request_id: request.request_id,
    status: "error",
  };
  return finishEnvelope(envelope, request);
}

/** Final admission: every envelope passes the full validator before emission. */
function finishEnvelope(envelope, request) {
  if (envelope.writes !== undefined) {
    const sorted = sortWritePlan(envelope.writes);
    if (sorted === null) throw new SdkBuildError("writes");
    envelope.writes = sorted;
  }
  const verdict = validateResponsePayload(request, envelope);
  if (!verdict.ok) throw new SdkBuildError(verdict.reason);
  if (!isPlanIdSafe(envelope)) throw new SdkBuildError("plan-id");
  return envelope;
}

function isPlanIdSafe(envelope) {
  return envelope.evidence.plan_id === undefined || isPlanId(envelope.evidence.plan_id);
}

/** Build one wire `Finding` (validate/verify result row). */
export function buildFinding({ code, detail, path }) {
  if (typeof code !== "string" || code === "" || [...code].length > BOUNDS.ERROR_CODE_MAX) {
    throw new SdkBuildError("finding-code");
  }
  const finding = { code, path };
  if (detail !== undefined) {
    if (typeof detail !== "string" || [...detail].length > BOUNDS.ERROR_DETAIL_MAX) throw new SdkBuildError("finding-detail");
    finding.detail = detail;
  }
  return finding;
}

/** Build one wire `OperationError` payload member (used inside buildError). */
export function buildOperationError({ detail, errorClass, errorCode, message, partial, retryable }) {
  if (!ENUMS.errorClass.includes(errorClass)) throw new SdkBuildError("error-class", errorClass);
  return {
    class: errorClass,
    code: errorCode,
    ...(detail === undefined ? {} : { detail }),
    message,
    ...(partial === undefined ? {} : { partial }),
    ...(retryable === undefined ? {} : { retryable }),
  };
}

/**
 * Validate a full diagnostic record against the pinned registry (the
 * builder refuses unknown codes and unbounded metadata; it can never
 * mint a new public code). The full diagnostic is a host-side artifact —
 * adapters emit wire findings and closed operation errors only.
 */
export function buildDiagnostic(codes, { code, data, message, rule }) {
  if (!codes.some((entry) => entry.code === code)) throw new SdkBuildError("diagnostic-code", code);
  if (typeof message !== "string" || message === "" || [...message].length > BOUNDS.ERROR_MESSAGE_MAX) {
    throw new SdkBuildError("diagnostic-message");
  }
  if (typeof rule !== "string" || rule.length > BOUNDS.ERROR_DETAIL_MAX) throw new SdkBuildError("diagnostic-rule");
  if (data !== undefined) {
    if (data === null || typeof data !== "object" || Array.isArray(data)) throw new SdkBuildError("diagnostic-data");
    for (const [key, value] of Object.entries(data)) {
      if (typeof key !== "string" || key.length > BOUNDS.ERROR_DETAIL_MAX) throw new SdkBuildError("diagnostic-data");
      if (typeof value !== "string" || value.length > BOUNDS.ERROR_DETAIL_MAX) throw new SdkBuildError("diagnostic-data");
    }
  }
  return { code, ...(data === undefined ? {} : { data }), message, rule };
}

/** The canonical bytes of one envelope: the exact stdout form. */
export function responseBytes(envelope) {
  return canonicalJson(envelope);
}

/** Compatibility re-export: canonical bytes of any closed value. */
export const canonicalBytes = canonical;
