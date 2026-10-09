/**
 * Strict wire decoding (issue #106): closed request/response envelopes.
 *
 * Mirrors the host's admission exactly: unknown members, explicit nulls,
 * duplicate keys, wrong protocol tokens, unsupported exact versions, and
 * out-of-bounds values are refused before any callback runs. Every
 * refusal carries one closed safe reason token — never payload content.
 */
import { WIRE_TOKEN, SUPPORTED_PROTOCOL_VERSIONS, LATEST_OPERATIONS, BOUNDS, ENUMS } from "../generated/protocol.mjs";
import { JsonFloat, readSingleObject } from "./json.mjs";
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
  // Production requires BOTH members whenever `limits` is present
  // (wire.rs: absent or out-of-range is admission `shape`); the SDK used
  // to admit partial limits objects. Member order mirrors production's
  // validate: timeout_ms first, so a zero timeout still refuses `bounds`
  // before the missing-member `shape` is reached. A float lexeme (1.0,
  // 1e2 — a `JsonFloat` from the lexeme-aware parser) where production
  // types `u64` is likewise `shape`, never an accepted integer (issue
  // #106 review r3 #3): integrality of the rounded value is never
  // consulted.
  for (const member of ["timeout_ms", "max_output_bytes"]) {
    const absent = optional(value[member]);
    if (absent) return absent;
    if (value[member] === undefined || value[member] instanceof JsonFloat) return failure("shape");
    const range = member === "timeout_ms" ? [BOUNDS.LIMITS_TIMEOUT_MIN, BOUNDS.LIMITS_TIMEOUT_MAX] : [BOUNDS.LIMITS_OUTPUT_MIN, BOUNDS.LIMITS_OUTPUT_MAX];
    if (!isInt(value[member]) || value[member] < range[0] || value[member] > range[1]) return failure("bounds");
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

function decodeStateString(value) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return failure("shape");
  const keys = Object.keys(value);
  if (value.state === "known") {
    if (keys.length !== 2 || !keys.includes("state") || !keys.includes("value")) return failure("shape");
    if (typeof value.value !== "string" || value.value === "" || value.value.length > BOUNDS.ERROR_DETAIL_MAX) return failure("bounds");
    return { ok: true, value };
  }
  if (["unknown", "withheld", "unsupported"].includes(value.state)) {
    if (keys.length !== 1) return failure("shape");
    return { ok: true, value };
  }
  return failure("shape");
}

function decodeLintBinding(value) {
  // The pinned 0.6.4 schema spells the members camelCase (nativeId).
  const closed = closeObject(value, ["fingerprint", "kind", "nativeId", "path", "symbol"]);
  if (closed) return closed;
  if (!isLogicalPath(value.path)) return failure("bounds");
  for (const member of ["nativeId", "symbol"]) {
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
  // Every pin is a State-tagged value; model/ir must carry known digests.
  for (const member of ["capabilities", "ir", "model", "observed", "profile", "revision"]) {
    const state = decodeStateString(value.pins[member]);
    if (!state.ok) return member === "model" || member === "ir" ? failure("bounds") : state;
  }
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
    // The closed LintEvidence subtree is validated before emission
    // (issue #106 review r2 #3): a shallow object check admitted `{}` as
    // `status:"ok"`; the full pinned schema subtree must hold instead.
    const decoded = decodeLintEvidence(value.lint_evidence);
    if (!decoded.ok) return decoded;
  }
  return { ok: true, value };
}

/**
 * The closed `LintEvidence` subtree of a lint result (issue #106 review
 * r2 #3): every member of the pinned 0.6.4 schema is required, the
 * identity/schemaVersion are consts, and each nested object is closed.
 * A shallow object check used to admit `{}` and emit `status:"ok"`.
 * Refusal reasons stay inside the closed decoder vocabulary.
 */
const LINT_EVIDENCE_MEMBERS = [
  "coverage", "identity", "inputManifestDigest", "limitations", "locations",
  "pins", "producer", "records", "schemaVersion", "scope", "sources", "target",
];
const LINT_STRING256 = /^[A-Za-z0-9][A-Za-z0-9_.:\/#@\\-]*$/;
const LINT_SHA256 = /^sha256:[0-9a-f]{64}$/;

const lintBounded256 = (value) => typeof value === "string" && value.length > 0 && value.length <= 256 && LINT_STRING256.test(value);

const lintStringArray = (value, max, unique) => {
  if (!Array.isArray(value) || value.length > max) return false;
  if (unique && new Set(value).size !== value.length) return false;
  return value.every(lintBounded256);
};

const lintStateString = (value) => {
  const closed = closeObject(value, ["state", "value"]);
  if (closed) return false;
  if (!["known", "unknown", "withheld", "unsupported"].includes(value.state)) return false;
  if (value.state === "known") return lintBounded256(value.value);
  return value.value === undefined;
};

const lintStateBool = (value) => {
  const closed = closeObject(value, ["state", "value"]);
  if (closed) return false;
  if (!["known", "unknown", "withheld", "unsupported"].includes(value.state)) return false;
  if (value.state === "known") return typeof value.value === "boolean";
  return value.value === undefined;
};

const lintStateU64 = (value) => {
  const closed = closeObject(value, ["state", "value"]);
  if (closed) return false;
  if (!["known", "unknown", "withheld", "unsupported"].includes(value.state)) return false;
  if (value.state === "known") return isInt(value.value) && value.value <= 10_000_000;
  return value.value === undefined;
};

const lintPins = (value) => {
  const closed = closeObject(value, ["capabilities", "ir", "model", "observed", "profile", "revision"]);
  if (closed) return false;
  return ["capabilities", "ir", "model", "observed", "profile", "revision"].every((pin) => lintStateString(value[pin]));
};

const lintProducer = (value) => {
  const closed = closeObject(value, ["artifactDigest", "compiler", "framework", "id", "recipe", "tool", "version"]);
  if (closed) return false;
  return lintBounded256(value.id)
    && lintBounded256(value.version)
    && typeof value.artifactDigest === "string" && value.artifactDigest.length <= 71 && LINT_SHA256.test(value.artifactDigest)
    && lintBounded256(value.tool)
    && lintBounded256(value.recipe)
    && lintStateString(value.compiler)
    && lintStateString(value.framework);
};

const lintSource = (value) => {
  const closed = closeObject(value, ["bytes", "fingerprint", "id", "path"]);
  if (closed) return false;
  return lintBounded256(value.id)
    && typeof value.path === "string" && value.path.length <= 512 && isLogicalPath(value.path)
    && typeof value.fingerprint === "string" && value.fingerprint.length <= 71 && LINT_SHA256.test(value.fingerprint)
    && isInt(value.bytes) && value.bytes <= BOUNDS.MAX_REQUEST_BYTES;
};

const lintSpan = (value) => {
  const closed = closeObject(value, ["column", "end", "endColumn", "endLine", "id", "line", "source", "start"]);
  if (closed) return false;
  const bounded = (member, min) => isInt(value[member]) && value[member] >= min && value[member] <= BOUNDS.MAX_REQUEST_BYTES;
  return lintBounded256(value.id)
    && lintBounded256(value.source)
    && bounded("start", 0)
    && bounded("end", 0)
    && bounded("line", 1)
    && bounded("column", 1)
    && bounded("endLine", 1)
    && bounded("endColumn", 1);
};

const lintCoverage = (value) => {
  const closed = closeObject(value, ["eligible", "examined", "limitations", "rule", "scope", "state", "target"]);
  if (closed) return false;
  return lintBounded256(value.rule)
    && lintBounded256(value.target)
    && lintBounded256(value.scope)
    && ["complete", "partial", "unknown", "unsupported", "withheld", "disabled"].includes(value.state)
    && lintStateU64(value.eligible)
    && lintStateU64(value.examined)
    && lintStringArray(value.limitations, 10_000, true);
};

const lintRecord = (value) => {
  const closed = closeObject(value, [
    "activation", "binding", "candidates", "claim", "confidence", "configuration",
    "currency", "field", "guards", "id", "key", "kind", "locations", "mechanism",
    "nativeId", "operation", "origin", "ownership", "resource", "semanticSymbol",
    "subject", "trace", "value",
  ]);
  if (closed) return false;
  const enumsOk = ["binding-candidates", "dispatch", "convention", "effect", "path", "reflection", "string-reference", "default", "native-edge", "field-write"].includes(value.kind)
    && ["direct", "observer", "hook", "magic"].includes(value.mechanism)
    && ["unknown", "low", "medium", "high", "exact"].includes(value.confidence)
    && ["current", "stale", "unknown"].includes(value.currency)
    && ["explicit", "extracted", "inferred"].includes(value.origin)
    && ["structural", "possible-behavior", "verified-behavior"].includes(value.claim);
  const stringsOk = lintBounded256(value.id)
    && lintBounded256(value.subject)
    && lintBounded256(value.nativeId);
  const route = (entry) => {
    if (closeObject(entry, ["confidence", "currency", "identity", "origin"])) return false;
    return lintBounded256(entry.identity)
      && ["unknown", "low", "medium", "high", "exact"].includes(entry.confidence)
      && ["current", "stale", "unknown"].includes(entry.currency)
      && ["explicit", "extracted", "inferred"].includes(entry.origin);
  };
  const step = (entry) => {
    if (closeObject(entry, ["confidence", "guards", "identity", "locations", "role"])) return false;
    return ["binding", "trigger", "registration", "callback", "effect"].includes(entry.role)
      && lintBounded256(entry.identity)
      && lintStringArray(entry.locations, 32, true)
      && lintStringArray(entry.guards, 32, true)
      && ["unknown", "low", "medium", "high", "exact"].includes(entry.confidence);
  };
  const stateMembers = ["semanticSymbol", "operation", "resource", "field", "value", "key"].every((member) => lintStateString(value[member]));
  const boolMembers = ["binding", "configuration", "ownership", "trace"].every((member) => lintStateBool(value[member]));
  const arraysOk = Array.isArray(value.candidates) && value.candidates.length <= 32 && value.candidates.every(route)
    && Array.isArray(value.activation) && value.activation.length <= 32 && value.activation.every(step)
    && lintStringArray(value.locations, 32, true)
    && lintStringArray(value.guards, 32, true);
  return enumsOk && stringsOk && stateMembers && boolMembers && arraysOk;
};

function decodeLintEvidence(value) {
  const closed = closeObject(value, LINT_EVIDENCE_MEMBERS);
  if (closed) return closed;
  for (const member of LINT_EVIDENCE_MEMBERS) {
    if (value[member] === undefined) return failure("shape", member);
  }
  if (value.schemaVersion !== "lekalo/ai-lint-evidence/v0.6.4") return failure("shape");
  if (value.identity !== "dev.lekalo.ai-lint-evidence@0.6.4") return failure("shape");
  if (!lintBounded256(value.target)) return failure("bounds");
  if (!lintStringArray(value.scope, 10_000, true)) return failure("bounds");
  const producer = lintProducer(value.producer);
  if (!producer) return failure("bounds");
  if (!lintPins(value.pins)) return failure("bounds");
  if (typeof value.inputManifestDigest !== "string" || value.inputManifestDigest.length > 71 || !LINT_SHA256.test(value.inputManifestDigest)) {
    return failure("bounds");
  }
  if (!Array.isArray(value.sources) || value.sources.length > 10_000) return failure("bounds");
  for (const source of value.sources) {
    if (!lintSource(source)) return failure("bounds");
  }
  if (!Array.isArray(value.locations) || value.locations.length > 32) return failure("bounds");
  if (new Set(value.locations.map(canonicalKey)).size !== value.locations.length) return failure("bounds");
  for (const span of value.locations) {
    if (!lintSpan(span)) return failure("bounds");
  }
  if (!Array.isArray(value.coverage) || value.coverage.length > 10_000) return failure("bounds");
  for (const coverage of value.coverage) {
    if (!lintCoverage(coverage)) return failure("bounds");
  }
  if (!Array.isArray(value.records) || value.records.length > 10_000) return failure("bounds");
  for (const record of value.records) {
    if (!lintRecord(record)) return failure("bounds");
  }
  if (!lintStringArray(value.limitations, 10_000, true)) return failure("bounds");
  return { ok: true, value };
}

/** Stable structural key for array-of-object uniqueness checks. */
const canonicalKey = (value) => JSON.stringify(value, Object.keys(flattenMembers(value)));

/** Deterministic deep member order for the canonical key above. */
function flattenMembers(value) {
  const out = [];
  const walk = (member) => {
    out.push(member);
    if (member !== null && typeof member === "object") {
      for (const key of Object.keys(member)) walk(member[key]);
    }
  };
  walk(value);
  return out;
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
