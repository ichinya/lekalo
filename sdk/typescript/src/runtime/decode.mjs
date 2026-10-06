/**
 * Strict wire decoding (issue #106): closed request/response envelopes.
 *
 * Mirrors the host's admission exactly: unknown members, explicit nulls,
 * duplicate keys, wrong protocol tokens, unsupported exact versions, and
 * out-of-bounds values are refused before any callback runs. Every
 * refusal carries one closed safe reason token — never payload content.
 */
import { WIRE_TOKEN, SUPPORTED_PROTOCOL_VERSIONS, LATEST_OPERATIONS, BOUNDS, ENUMS } from "../generated/protocol.mjs";
import { readSingleObject } from "./json.mjs";
import { isPlanId, isRequestId, isSha256Digest } from "./ids.mjs";
import { isCapabilityId, isContractVersion, isLogicalPath, isToken } from "./paths.mjs";

const SUPPORT_STATES = ENUMS.supportState;
const TRANSPORTS = ENUMS.transport;

/** The closed safe reason tokens an SDK decoder refuses with. */
export const DECODE_REASONS = [
  "bounds", "duplicate-key", "explicit-null", "grammar", "member", "not-json",
  "operation", "oversize", "request-id", "shape", "token", "trailing-document",
  "unknown-member", "version",
];

const failure = (reason, detail) => ({ ok: false, reason, ...(detail === undefined ? {} : { detail }) });

const isInt = (value) => typeof value === "number" && Number.isSafeInteger(value) && value >= 0;

function optional(member) {
  // Absent is fine (null = continue); only an explicit null refuses.
  if (member === null) return failure("explicit-null");
  return null;
}

function closeObject(value, allowed) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return failure("shape");
  for (const key of Object.keys(value)) {
    if (!allowed.includes(key)) return failure("unknown-member", key);
  }
  return null;
}

function boundedText(value, max, reason = "bounds") {
  if (typeof value !== "string") return failure("shape");
  if ([...value].length > max) return failure(reason);
  return null;
}

// ---------------------------------------------------------------------------
// Substructures
// ---------------------------------------------------------------------------

function decodeIdentity(value) {
  const closed = closeObject(value, ["digest", "id", "version"]);
  if (closed) return closed;
  if (!isToken(value.id)) return failure("bounds");
  if (!isContractVersion(value.version)) return failure("bounds");
  if (!isSha256Digest(value.digest)) return failure("bounds");
  return { ok: true, value };
}

function decodeEvidence(value) {
  const closed = closeObject(value, ["adapter", "plan_id"]);
  if (closed) return closed;
  const absent = optional(value.adapter);
  if (absent) return absent;
  const identity = decodeIdentity(value.adapter);
  if (!identity.ok) return identity;
  if (value.plan_id !== undefined) {
    const absentPlan = optional(value.plan_id);
    if (absentPlan) return absentPlan;
    if (!isPlanId(value.plan_id)) return failure("bounds");
  }
  return { ok: true, value };
}

function decodeLimits(value) {
  const closed = closeObject(value, ["max_output_bytes", "timeout_ms"]);
  if (closed) return closed;
  for (const member of ["max_output_bytes", "timeout_ms"]) {
    const absent = optional(value[member]);
    if (absent) return absent;
    if (value[member] !== undefined) {
      const range = member === "timeout_ms" ? [BOUNDS.LIMITS_TIMEOUT_MIN, BOUNDS.LIMITS_TIMEOUT_MAX] : [BOUNDS.LIMITS_OUTPUT_MIN, BOUNDS.LIMITS_OUTPUT_MAX];
      if (!isInt(value[member]) || value[member] < range[0] || value[member] > range[1]) return failure("bounds");
    }
  }
  return { ok: true, value };
}

function decodeProfileCapabilities(value) {
  const absent = optional(value);
  if (absent) return absent;
  if (value === undefined) return { ok: true, value: undefined };
  if (!Array.isArray(value) || value.length > BOUNDS.MAX_DECLARED_CAPABILITIES) return failure("bounds");
  let previous = null;
  for (const capability of value) {
    const closed = closeObject(capability, ["id", "support"]);
    if (closed) return closed;
    if (!isCapabilityId(capability.id)) return failure("bounds");
    if (!SUPPORT_STATES.includes(capability.support)) return failure("shape");
    if (previous !== null && capability.id <= previous) return failure("bounds");
    previous = capability.id;
  }
  return { ok: true, value };
}

function decodeContentRef(value) {
  const closed = closeObject(value, ["adapter", "digest", "revision"]);
  if (closed) return closed;
  if (!isSha256Digest(value.digest)) return failure("bounds");
  for (const member of ["adapter", "revision"]) {
    const absent = optional(value[member]);
    if (absent) return absent;
    if (value[member] !== undefined && (typeof value[member] !== "string" || value[member].length > 128)) return failure("bounds");
  }
  return { ok: true, value };
}

function decodeNativeChanges(value) {
  const closed = closeObject(value, ["files", "symbols"]);
  if (closed) return closed;
  const files = value.files === undefined ? [] : value.files;
  const symbols = value.symbols === undefined ? [] : value.symbols;
  if (!Array.isArray(files) || files.length > 1024) return failure("bounds");
  if (!Array.isArray(symbols) || symbols.length > 1024) return failure("bounds");
  let previous = null;
  for (const change of files) {
    const inner = closeObject(change, ["after_digest", "before_digest", "change", "path"]);
    if (inner) return inner;
    if (!isLogicalPath(change.path)) return failure("bounds");
    if (!ENUMS.nativeChangeKind.includes(change.change)) return failure("shape");
    for (const member of ["after_digest", "before_digest"]) {
      const absent = optional(change[member]);
      if (absent) return absent;
      if (change[member] !== undefined && !isSha256Digest(change[member])) return failure("bounds");
    }
    if (previous !== null && change.path <= previous) return failure("bounds");
    previous = change.path;
  }
  for (const symbol of symbols) {
    if (typeof symbol !== "string" || symbol === "" || symbol.length > BOUNDS.ERROR_DETAIL_MAX) return failure("bounds");
  }
  return { ok: true, value: { files, symbols } };
}

function decodeNativeRequest(value) {
  const closed = closeObject(value, [
    "capability_snapshot_digest", "changes", "execution_policy_ref",
    "input_manifest_digest", "observed_ref", "scan_ref", "tool_catalog_digest",
  ]);
  if (closed) return closed;
  for (const member of ["input_manifest_digest", "tool_catalog_digest", "capability_snapshot_digest"]) {
    const absent = optional(value[member]);
    if (absent) return absent;
    if (!isSha256Digest(value[member])) return failure("bounds");
  }
  const changes = decodeNativeChanges(value.changes);
  if (!changes.ok) return changes;
  const scanRef = decodeContentRef(value.scan_ref);
  if (!scanRef.ok) return scanRef;
  const policyRef = decodeContentRef(value.execution_policy_ref);
  if (!policyRef.ok) return policyRef;
  const absentObserved = optional(value.observed_ref);
  if (absentObserved) return absentObserved;
  if (value.observed_ref !== undefined) {
    const observed = decodeContentRef(value.observed_ref);
    if (!observed.ok) return observed;
  }
  return { ok: true, value };
}

function decodeLintBinding(value) {
  const closed = closeObject(value, ["fingerprint", "kind", "native_id", "path", "symbol"]);
  if (closed) return closed;
  if (!isLogicalPath(value.path)) return failure("bounds");
  for (const member of ["native_id", "symbol"]) {
    if (typeof value[member] !== "string" || value[member] === "" || value[member].length > BOUNDS.ERROR_DETAIL_MAX) return failure("bounds");
  }
  if (!["command", "entity"].includes(value.kind)) return failure("shape");
  if (!isSha256Digest(value.fingerprint)) return failure("bounds");
  return { ok: true, value };
}

function decodeLintRequest(value) {
  const closed = closeObject(value, ["bindings", "files", "pins", "scope"]);
  if (closed) return closed;
  for (const member of ["scope", "files", "bindings"]) {
    const absent = optional(value[member]);
    if (absent) return absent;
  }
  if (!Array.isArray(value.scope) || value.scope.length === 0 || value.scope.length > 10_000) return failure("bounds");
  if (!Array.isArray(value.files) || value.files.length > 4_096) return failure("bounds");
  if (!Array.isArray(value.bindings) || value.bindings.length > 10_000) return failure("bounds");
  for (const scope of value.scope) {
    if (typeof scope !== "string" || scope === "" || scope.length > BOUNDS.ERROR_DETAIL_MAX) return failure("bounds");
  }
  for (const file of value.files) {
    if (!isLogicalPath(file)) return failure("bounds");
  }
  for (const binding of value.bindings) {
    const binding_ = decodeLintBinding(binding);
    if (!binding_.ok) return binding_;
  }
  const pins = closeObject(value.pins, ["capabilities", "ir", "model", "observed", "profile", "revision"]);
  if (pins) return pins;
  for (const member of ["model", "ir"]) {
    if (!isSha256Digest(value.pins[member])) return failure("bounds");
  }
  for (const member of ["observed", "profile", "capabilities"]) {
    const absent = optional(value.pins[member]);
    if (absent) return absent;
    if (value.pins[member] !== undefined && !isSha256Digest(value.pins[member])) return failure("bounds");
  }
  const revision = optional(value.pins.revision);
  if (revision) return revision;
  if (value.pins.revision !== undefined && (typeof value.pins.revision !== "string" || value.pins.revision.length > BOUNDS.ERROR_DETAIL_MAX)) return failure("bounds");
  return { ok: true, value };
}

// ---------------------------------------------------------------------------
// Envelopes
// ---------------------------------------------------------------------------

/**
 * Decode one strict request envelope from raw text. Structural, closed,
 * and bounded; cross-member semantics stay in `validateRequest`.
 */
export function decodeRequest(text, maxBytes = BOUNDS.MAX_REQUEST_BYTES) {
  const document = readSingleObject(text, maxBytes);
  if (!document.ok) return document;
  const value = document.value;
  const closed = closeObject(value, [
    "dry_run", "ir_path", "limits", "lint_request", "native_request",
    "operation", "plan_id", "profile", "profile_capabilities", "profile_digest",
    "project_root", "protocol", "protocol_version", "request_id", "target",
  ]);
  if (closed) return closed;
  for (const member of ["protocol", "protocol_version", "operation", "request_id", "project_root"]) {
    if (value[member] === undefined) return failure("shape");
    if (value[member] === null) return failure("explicit-null", member);
  }
  if (value.protocol !== WIRE_TOKEN) return failure("token");
  if (!SUPPORTED_PROTOCOL_VERSIONS.includes(value.protocol_version)) return failure("version");
  if (!LATEST_OPERATIONS.includes(value.operation)) return failure("operation");
  if (!isRequestId(value.request_id)) return failure("request-id");
  if (value.project_root !== ".") return failure("grammar");
  for (const member of ["ir_path", "target", "profile"]) {
    const absent = optional(value[member]);
    if (absent) return absent;
    if (value[member] !== undefined) {
      const ok = member === "ir_path" ? isLogicalPath(value[member]) : isToken(value[member]);
      if (!ok) return failure("grammar", member);
    }
  }
  for (const member of ["profile_digest"]) {
    const absent = optional(value[member]);
    if (absent) return absent;
    if (value[member] !== undefined && !isSha256Digest(value[member])) return failure("bounds");
  }
  for (const member of ["dry_run"]) {
    const absent = optional(value[member]);
    if (absent) return absent;
    if (value[member] !== undefined && typeof value[member] !== "boolean") return failure("shape");
  }
  for (const member of ["plan_id"]) {
    const absent = optional(value[member]);
    if (absent) return absent;
    if (value[member] !== undefined && !isPlanId(value[member])) return failure("bounds");
  }
  const absentLimits = optional(value.limits);
  if (absentLimits) return absentLimits;
  if (value.limits !== undefined) {
    const limits = decodeLimits(value.limits);
    if (!limits.ok) return limits;
  }
  const capabilities = decodeProfileCapabilities(value.profile_capabilities);
  if (!capabilities.ok) return capabilities;
  const absentNative = optional(value.native_request);
  if (absentNative) return absentNative;
  if (value.native_request !== undefined) {
    const native = decodeNativeRequest(value.native_request);
    if (!native.ok) return native;
  }
  const absentLint = optional(value.lint_request);
  if (absentLint) return absentLint;
  if (value.lint_request !== undefined) {
    const lint = decodeLintRequest(value.lint_request);
    if (!lint.ok) return lint;
  }
  return { ok: true, value };
}

function decodeWriteEntry(value) {
  const closed = closeObject(value, ["action", "path", "sha256"]);
  if (closed) return closed;
  if (!isLogicalPath(value.path)) return failure("bounds");
  if (!ENUMS.writeAction.includes(value.action)) return failure("shape");
  const absent = optional(value.sha256);
  if (absent) return absent;
  if (value.sha256 !== undefined && !isSha256Digest(value.sha256)) return failure("bounds");
  return { ok: true, value };
}

function decodeResult(value) {
  const closed = closeObject(value, ["bindings", "entries", "findings", "lint_evidence", "native_plan", "ok", "truncated"]);
  if (closed) return closed;
  for (const member of ["ok", "truncated"]) {
    const absent = optional(value[member]);
    if (absent) return absent;
    if (value[member] !== undefined && typeof value[member] !== "boolean") return failure("shape");
  }
  if (value.entries !== undefined) {
    const absent = optional(value.entries);
    if (absent) return absent;
    if (!Array.isArray(value.entries) || value.entries.length > BOUNDS.SCAN_ENTRIES_MAX) return failure("bounds");
    for (const entry of value.entries) {
      const decoded = decodeScanEntry(entry);
      if (!decoded.ok) return decoded;
    }
  }
  if (value.findings !== undefined) {
    const absent = optional(value.findings);
    if (absent) return absent;
    if (!Array.isArray(value.findings) || value.findings.length > BOUNDS.FINDINGS_MAX) return failure("bounds");
    for (const finding of value.findings) {
      const decoded = decodeFinding(finding);
      if (!decoded.ok) return decoded;
    }
  }
  if (value.bindings !== undefined) {
    const absent = optional(value.bindings);
    if (absent) return absent;
    if (!Array.isArray(value.bindings) || value.bindings.length > BOUNDS.BINDINGS_MAX) return failure("bounds");
    for (const binding of value.bindings) {
      const inner = closeObject(binding, ["module", "profile", "target"]);
      if (inner) return inner;
      if (!isToken(binding.module) || !isToken(binding.target) || !isToken(binding.profile)) return failure("bounds");
    }
  }
  if (value.native_plan !== undefined) {
    const decoded = decodeNativePlan(value.native_plan);
    if (!decoded.ok) return decoded;
  }
  if (value.lint_evidence !== undefined) {
    const absent = optional(value.lint_evidence);
    if (absent) return absent;
    if (value.lint_evidence === null || typeof value.lint_evidence !== "object" || Array.isArray(value.lint_evidence)) {
      return failure("shape");
    }
  }
  return { ok: true, value };
}

function decodeScanEntry(value) {
  const closed = closeObject(value, ["detail", "evidence", "kind", "path"]);
  if (closed) return closed;
  if (!isLogicalPath(value.path)) return failure("bounds");
  if (boundedText(value.kind, BOUNDS.ERROR_DETAIL_MAX)) return failure("bounds");
  const absentDetail = optional(value.detail);
  if (absentDetail) return absentDetail;
  if (value.detail !== undefined && boundedText(value.detail, BOUNDS.ERROR_DETAIL_MAX)) return failure("bounds");
  const absentEvidence = optional(value.evidence);
  if (absentEvidence) return absentEvidence;
  if (value.evidence !== undefined) {
    const inner = closeObject(value.evidence, ["references", "signature"]);
    if (inner) return inner;
    const absentSignature = optional(value.evidence.signature);
    if (absentSignature) return absentSignature;
    if (value.evidence.signature !== undefined && !isSha256Digest(value.evidence.signature)) return failure("bounds");
    const references = value.evidence.references === undefined ? [] : value.evidence.references;
    if (!Array.isArray(references) || references.length > BOUNDS.REFERENCES_MAX) return failure("bounds");
    for (const reference of references) {
      const refClosed = closeObject(reference, ["confidence", "role", "target"]);
      if (refClosed) return refClosed;
      if (typeof reference.target !== "string" || reference.target === "" || reference.target.length > 192) return failure("bounds");
      if (!ENUMS.scanEntryReferenceRole.includes(reference.role)) return failure("shape");
      if (!ENUMS.scanEntryConfidence.includes(reference.confidence)) return failure("shape");
    }
  }
  return { ok: true, value };
}

function decodeFinding(value) {
  const closed = closeObject(value, ["code", "detail", "path"]);
  if (closed) return closed;
  if (!isLogicalPath(value.path)) return failure("bounds");
  if (boundedText(value.code, BOUNDS.ERROR_CODE_MAX)) return failure("bounds");
  if (value.code === "") return failure("bounds");
  const absent = optional(value.detail);
  if (absent) return absent;
  if (value.detail !== undefined && boundedText(value.detail, BOUNDS.ERROR_DETAIL_MAX)) return failure("bounds");
  return { ok: true, value };
}

function decodeNativePlan(value) {
  const closed = closeObject(value, ["commands", "completeness", "digest", "entries", "kind", "packages", "run_eligibility", "units"]);
  if (closed) return closed;
  if (!ENUMS.nativePlanKind.includes(value.kind)) return failure("shape");
  if (!isSha256Digest(value.digest)) return failure("bounds");
  for (const member of ["commands", "entries", "packages", "units"]) {
    const absent = optional(value[member]);
    if (absent) return absent;
    if (value[member] !== undefined && (!isInt(value[member]) || value[member] > BOUNDS.MAX_CONSTRAINT_VALUE)) return failure("bounds");
  }
  for (const [member, values] of [["completeness", ENUMS.nativeCompleteness], ["run_eligibility", ENUMS.nativeRunEligibility]]) {
    const absent = optional(value[member]);
    if (absent) return absent;
    if (value[member] !== undefined && !values.includes(value[member])) return failure("shape");
  }
  return { ok: true, value };
}

function decodeProgress(value) {
  if (!Array.isArray(value) || value.length > BOUNDS.PROGRESS_STEPS_MAX) return failure("bounds");
  for (const step of value) {
    const closed = closeObject(step, ["detail", "state", "step"]);
    if (closed) return closed;
    if (boundedText(step.step, BOUNDS.PROGRESS_TEXT_MAX) || step.step === "") return failure("bounds");
    if (!ENUMS.progressState.includes(step.state)) return failure("shape");
    const absent = optional(step.detail);
    if (absent) return absent;
    if (step.detail !== undefined && boundedText(step.detail, BOUNDS.PROGRESS_TEXT_MAX)) return failure("bounds");
  }
  return { ok: true, value };
}

function decodeError(value) {
  const closed = closeObject(value, ["class", "code", "detail", "message", "partial", "retryable"]);
  if (closed) return closed;
  if (!ENUMS.errorClass.includes(value.class)) return failure("shape");
  if (boundedText(value.code, BOUNDS.ERROR_CODE_MAX) || value.code === "") return failure("bounds");
  if (boundedText(value.message, BOUNDS.ERROR_MESSAGE_MAX) || value.message === "") return failure("bounds");
  for (const member of ["partial", "retryable"]) {
    const absent = optional(value[member]);
    if (absent) return absent;
    if (value[member] !== undefined && typeof value[member] !== "boolean") return failure("shape");
  }
  if (value.detail !== undefined) {
    const absent = optional(value.detail);
    if (absent) return absent;
    if (!Array.isArray(value.detail) || value.detail.length > BOUNDS.ERROR_DETAILS_MAX) return failure("bounds");
    for (const detail of value.detail) {
      if (boundedText(detail, BOUNDS.ERROR_DETAIL_MAX)) return failure("bounds");
    }
  }
  return { ok: true, value };
}

function decodeCapabilities(value) {
  const closed = closeObject(value, [
    "adapter", "capabilities", "constraints", "ir_versions", "operations",
    "profiles", "progress", "protocol_versions", "read_scopes", "targets",
    "transports", "write_scopes",
  ]);
  if (closed) return closed;
  const identity = decodeIdentity(value.adapter);
  if (!identity.ok) return identity;
  // Required with a non-empty closed bound on the wire (core: unique).
  const requiredList = (member, max, check) => {
    if (!Array.isArray(value[member]) || value[member].length > max) return false;
    const unique = new Set(value[member]);
    if (unique.size !== value[member].length) return false;
    return value[member].every((entry) => check(entry));
  };
  // Optional members are absent-legal (core serde default), never null.
  const optionalList = (member, max, check) => {
    const absent = optional(value[member]);
    if (absent) return false;
    if (value[member] === undefined) return true;
    if (!Array.isArray(value[member]) || value[member].length > max) return false;
    const unique = new Set(value[member]);
    if (unique.size !== value[member].length) return false;
    return value[member].every((entry) => check(entry));
  };
  if (!requiredList("protocol_versions", BOUNDS.PROTOCOL_VERSIONS_MAX, isContractVersion)) return failure("bounds");
  if (!requiredList("operations", BOUNDS.OPERATIONS_MAX_LINT, (op) => LATEST_OPERATIONS.includes(op))) return failure("bounds");
  if (!requiredList("transports", BOUNDS.TRANSPORTS_MAX, (t) => TRANSPORTS.includes(t))) return failure("bounds");
  if (!optionalList("targets", BOUNDS.TARGETS_MAX, isToken)) return failure("bounds");
  if (!optionalList("profiles", BOUNDS.PROFILES_MAX, isToken)) return failure("bounds");
  if (!optionalList("read_scopes", BOUNDS.MAX_SCOPES, (s) => typeof s === "string" && s.length > 0 && s.length <= BOUNDS.PATH_MAX)) return failure("bounds");
  if (!optionalList("write_scopes", BOUNDS.MAX_SCOPES, (s) => typeof s === "string" && s.length > 0 && s.length <= BOUNDS.PATH_MAX)) return failure("bounds");
  if (!optionalList("ir_versions", BOUNDS.MAX_IR_VERSIONS, isContractVersion)) return failure("bounds");
  if (typeof value.progress !== "boolean") return failure("shape");
  if (value.capabilities !== undefined) {
    const absentMap = optional(value.capabilities);
    if (absentMap) return absentMap;
    if (value.capabilities === null || typeof value.capabilities !== "object" || Array.isArray(value.capabilities)) return failure("shape");
    const ids = Object.keys(value.capabilities);
    if (ids.length > BOUNDS.MAX_DECLARED_CAPABILITIES) return failure("bounds");
    for (const id of ids) {
      if (!isCapabilityId(id)) return failure("bounds");
      if (!SUPPORT_STATES.includes(value.capabilities[id])) return failure("shape");
    }
  }
  if (value.constraints !== undefined) {
    const absent = optional(value.constraints);
    if (absent) return absent;
    const inner = closeObject(value.constraints, ["max_entries", "max_writes"]);
    if (inner) return inner;
    for (const member of ["max_entries", "max_writes"]) {
      const absentValue = optional(value.constraints[member]);
      if (absentValue) return absentValue;
      if (value.constraints[member] !== undefined && (!isInt(value.constraints[member]) || value.constraints[member] > BOUNDS.MAX_CONSTRAINT_VALUE)) return failure("bounds");
    }
  }
  return { ok: true, value };
}

/**
 * Decode one strict response envelope from raw text (the conformance
 * client and the validate-before-write path both use this admission).
 */
export function decodeResponse(text, maxBytes = BOUNDS.DEFAULT_MAX_OUTPUT_BYTES) {
  const document = readSingleObject(text, maxBytes);
  if (!document.ok) return document;
  const value = document.value;
  const closed = closeObject(value, [
    "capabilities", "error", "evidence", "operation", "progress", "protocol",
    "protocol_version", "request_id", "result", "status", "writes",
  ]);
  if (closed) return closed;
  for (const member of ["protocol", "protocol_version", "operation", "request_id", "status", "evidence"]) {
    if (value[member] === undefined) return failure("shape");
    if (value[member] === null) return failure("explicit-null", member);
  }
  if (value.protocol !== WIRE_TOKEN) return failure("token");
  if (!isContractVersion(value.protocol_version)) return failure("version");
  if (!LATEST_OPERATIONS.includes(value.operation)) return failure("operation");
  if (!isRequestId(value.request_id)) return failure("request-id");
  if (!ENUMS.responseStatus.includes(value.status)) return failure("shape");
  const evidence = decodeEvidence(value.evidence);
  if (!evidence.ok) return evidence;
  const absentCapabilities = optional(value.capabilities);
  if (absentCapabilities) return absentCapabilities;
  if (value.capabilities !== undefined) {
    const capabilities = decodeCapabilities(value.capabilities);
    if (!capabilities.ok) return capabilities;
  }
  const absentResult = optional(value.result);
  if (absentResult) return absentResult;
  if (value.result !== undefined) {
    const result = decodeResult(value.result);
    if (!result.ok) return result;
  }
  if (value.writes !== undefined) {
    const absent = optional(value.writes);
    if (absent) return absent;
    if (!Array.isArray(value.writes) || value.writes.length > BOUNDS.MAX_PLAN_ENTRIES) return failure("bounds");
    for (const entry of value.writes) {
      const decoded = decodeWriteEntry(entry);
      if (!decoded.ok) return decoded;
    }
  }
  const absentProgress = optional(value.progress);
  if (absentProgress) return absentProgress;
  if (value.progress !== undefined) {
    const progress = decodeProgress(value.progress);
    if (!progress.ok) return progress;
  }
  const absentError = optional(value.error);
  if (absentError) return absentError;
  if (value.error !== undefined) {
    const error = decodeError(value.error);
    if (!error.ok) return error;
  }
  return { ok: true, value };
}
