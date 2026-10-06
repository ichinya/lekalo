/**
 * Semantic wire validation (issue #106): the cross-member request rules
 * and the response identity/pairing rules the closed shapes cannot
 * express. These checks are the SDK's own admission layer — the host
 * still validates everything again, and no SDK check can widen host
 * policy.
 */
import { BASE_OPERATIONS, ENUMS, IR_VERSIONS, LATEST_OPERATIONS, SUPPORTED_PROTOCOL_VERSIONS, WIRE_TOKEN } from "../generated/protocol.mjs";
import { isLogicalPath } from "./paths.mjs";
import { isPlanId, isSha256Digest } from "./ids.mjs";

/** The closed semantic refusal tokens (distinct from decode reasons). */
export const VALIDATE_REASONS = [
  "dry-run", "echo-operation", "echo-request-id", "echo-version", "error-pairing",
  "identity", "ir-path", "lint-binding", "lint-pin", "lint-request", "limits",
  "member", "native-request", "plan-id", "profile", "profile-capabilities",
  "profile-digest", "target", "version", "writes-missing", "write-digest",
];

const invalid = (reason) => ({ ok: false, reason });

const isSorted = (values) => {
  if (!Array.isArray(values)) return false;
  for (let index = 1; index < values.length; index += 1) {
    if (values[index - 1] >= values[index]) return false;
  }
  return true;
};
const isDetailToken = (value) => typeof value === "string" && value.length > 0 && value.length <= 128;

const requiresIr = (operation) => ["validate", "generate", "verify"].includes(operation);
const declaresWrites = (operation) => ["generate", "clean", "plan-clean"].includes(operation);
const LINT_VERSION = "0.6.4";

/**
 * Validate the decoded request's cross-member semantics: operation
 * pairing, plan custody, IR paths, native and lint seams, and limits.
 */
export function validateRequest(request) {
  if (request.protocol !== WIRE_TOKEN) return invalid("version");
  if ((request.operation === "lint") !== (request.lint_request !== undefined)) return invalid("lint-request");
  if (request.lint_request !== undefined) {
    if (request.protocol_version !== LINT_VERSION || request.target === undefined) return invalid("lint-request");
    if (request.profile !== undefined || request.profile_digest !== undefined || request.profile_capabilities !== undefined) return invalid("lint-request");
    if (request.ir_path !== undefined || request.native_request !== undefined || request.dry_run !== undefined || request.plan_id !== undefined) return invalid("lint-request");
    const lint = request.lint_request;
    if (!isSorted(lint.scope) || lint.scope.some((scope) => !isDetailToken(scope))) return invalid("lint-request");
    if (!isSorted(lint.files) || lint.files.some((file) => !isLogicalPath(file))) return invalid("lint-request");
    const bindingKeys = new Set();
    for (const binding of lint.bindings) {
      if (!isLogicalPath(binding.path) || !isDetailToken(binding.native_id) || !isDetailToken(binding.symbol)) return invalid("lint-binding");
      if (!["command", "entity"].includes(binding.kind)) return invalid("lint-binding");
      if (!isSha256Digest(binding.fingerprint)) return invalid("lint-binding");
      const key = `${binding.path}\u0000${binding.native_id}\u0000${binding.kind}`;
      if (bindingKeys.has(key) || !lint.files.includes(binding.path)) return invalid("lint-binding");
      bindingKeys.add(key);
    }
    for (const pin of ["observed", "profile", "capabilities"]) {
      if (lint.pins[pin] !== undefined && !isSha256Digest(lint.pins[pin])) return invalid("lint-pin");
    }
    if (lint.pins.revision !== undefined && !isDetailToken(lint.pins.revision)) return invalid("lint-pin");
  }
  if (request.operation === "plan-native" && request.native_request === undefined) return invalid("native-request");
  if (request.operation !== "plan-native" && request.native_request !== undefined) return invalid("native-request");
  const apply = request.operation === "clean" || (request.operation === "generate" && request.dry_run === false);
  if (apply !== (request.plan_id !== undefined)) return invalid("plan-id");
  if (request.plan_id !== undefined && !isPlanId(request.plan_id)) return invalid("plan-id");
  if ((request.operation === "generate") !== (request.dry_run !== undefined)) return invalid("dry-run");
  if (request.operation === "plan-native" && (request.dry_run !== undefined || request.plan_id !== undefined)) return invalid("plan-id");
  if (requiresIr(request.operation) && request.ir_path === undefined) return invalid("ir-path");
  if ((request.operation === "generate" || request.operation === "bind") && request.target === undefined) return invalid("target");
  if (request.operation === "bind" && request.profile === undefined) return invalid("profile");
  if (request.operation === "describe") {
    if (request.target !== undefined || request.profile !== undefined || request.profile_digest !== undefined || request.profile_capabilities !== undefined) return invalid("member");
    if (request.native_request !== undefined || request.ir_path !== undefined) return invalid("member");
  }
  const profileResolution = request.profile_digest !== undefined || request.profile_capabilities !== undefined;
  if (profileResolution && (request.profile_digest === undefined || request.profile_capabilities === undefined || request.profile === undefined)) return invalid("profile-capabilities");
  if (profileResolution && !SUPPORTED_PROTOCOL_VERSIONS.includes(request.protocol_version)) return invalid("member");
  return { ok: true };
}

/**
 * Validate response identity echo against the exact negotiated session.
 * Protocol deviations are version/token refusals; echo deviations are
 * malformed responses.
 */
export function validateResponseIdentity(response, expected) {
  if (response.protocol !== WIRE_TOKEN) return invalid("version");
  if (response.protocol_version !== expected.protocolVersion) return invalid("echo-version");
  if (response.operation !== expected.operation) return invalid("echo-operation");
  if (response.request_id !== expected.requestId) return invalid("echo-request-id");
  return { ok: true };
}

/**
 * Validate the operation payload pairing of one success response (the
 * validate-before-write obligation): exactly the members the operation
 * admits, complete payloads, error/status pairing, and write-plan
 * presence and digest/action pairing.
 */
export function validateResponsePayload(request, response) {
  if (response.capabilities !== undefined && request.operation !== "describe") return invalid("member");
  if (request.operation === "describe" && response.status === "ok" && response.capabilities === undefined) return invalid("member");
  const apply = request.operation === "clean" || (request.operation === "generate" && request.dry_run === false);
  if (apply && response.evidence.plan_id !== request.plan_id) return invalid("plan-id");
  if (request.operation === "generate" && request.dry_run === true && response.evidence.plan_id !== undefined) return invalid("plan-id");
  if (response.status === "error") {
    if (response.error === undefined) return invalid("error-pairing");
    if (response.result !== undefined || response.writes !== undefined || response.progress !== undefined) return invalid("error-pairing");
    if (response.error.partial === true && (request.operation !== "generate" && request.operation !== "clean")) return invalid("member");
    return { ok: true };
  }
  if (response.error !== undefined) return invalid("error-pairing");
  const result = response.result;
  if (result !== undefined) {
    if (result.lint_evidence !== undefined && request.operation !== "lint") return invalid("member");
    if (result.entries !== undefined && request.operation !== "scan") return invalid("member");
    if (result.truncated !== undefined && request.operation !== "scan") return invalid("member");
    if (result.bindings !== undefined && request.operation !== "bind") return invalid("member");
    if (result.ok !== undefined && !["validate", "verify"].includes(request.operation)) return invalid("member");
    if (result.findings !== undefined && !["validate", "verify"].includes(request.operation)) return invalid("member");
    if (result.native_plan !== undefined && request.operation !== "plan-native") return invalid("member");
    if (request.operation === "plan-native" && result.entries !== undefined) return invalid("member");
  }
  const complete = completeResult(request.operation, result);
  if (!complete) return invalid("member");
  if (declaresWrites(request.operation)) {
    if (response.writes === undefined) return invalid("writes-missing");
    for (const entry of response.writes) {
      if (entry.action === "delete") {
        if (entry.sha256 !== undefined) return invalid("write-digest");
      } else if (entry.sha256 === undefined) return invalid("write-digest");
    }
  } else if (response.writes !== undefined) {
    return invalid("member");
  }
  if (request.operation === "generate" && request.dry_run === true && response.evidence.plan_id !== undefined) return invalid("plan-id");
  return { ok: true };
}

function completeResult(operation, result) {
  switch (operation) {
    case "scan": return result !== undefined && result.entries !== undefined;
    case "bind": return result !== undefined && result.bindings !== undefined;
    case "validate":
    case "verify": return result !== undefined && result.ok !== undefined;
    case "plan-native": return result !== undefined && result.native_plan !== undefined;
    case "lint": return result !== undefined && result.lint_evidence !== undefined;
    case "describe": return result === undefined;
    default: return result === undefined;
  }
}

/** Whether the operation belongs to the strict base surface. */
export function isBaseOperation(operation) {
  return BASE_OPERATIONS.includes(operation);
}

/** Whether the operation belongs to this build's full vocabulary. */
export function isKnownOperation(operation) {
  return LATEST_OPERATIONS.includes(operation);
}

/** Whether the enum value belongs to the closed support-state set. */
export function isSupportState(value) {
  return ENUMS.supportState.includes(value);
}
