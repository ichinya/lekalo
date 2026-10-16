/**
 * Strict wire decoding (issue #106): closed request/response envelopes.
 *
 * Closed wire spellings follow the host; lint_evidence additionally has
 * deliberate SDK emission ceilings (sdk/spec.md section 4). Unknown
 * members, forbidden nulls, duplicate keys, wrong tokens/versions and
 * out-of-bounds values refuse before callbacks. Each refusal carries a
 * closed local reason token — never payload content.
 */
import { WIRE_TOKEN, SUPPORTED_PROTOCOL_VERSIONS, LATEST_OPERATIONS, BOUNDS, ENUMS } from "../generated/protocol.mjs";
import { CAPABILITY_DEFINITIONS } from "../generated/capabilities.mjs";
import { JsonFloat, canonicalJson, readSingleObject } from "./json.mjs";
import { isPlanId, isRequestId, isSha256Digest } from "./ids.mjs";
import { isCapabilityId, isContractVersion, isLogicalPath, isSemanticId, isToken } from "./paths.mjs";

const SUPPORT_STATES = ENUMS.supportState;
const TRANSPORTS = ENUMS.transport;
const LINT_VERSION = "0.6.4";

/** Whether a capability id is a member of the generated registry —
 * production admission consults `capability::definition(id).is_some()`
 * for every describe map key (issue #106 review r5 #1), so the generated
 * per-SDK table is authoritative here, not just the grammar. */
const isKnownCapabilityId = (id) => CAPABILITY_DEFINITIONS.some((entry) => entry.id === id);

/** Serde's typed-struct decoding accepts BOTH spellings at every closed
 * wire struct: the member map and, via `visit_seq`, the positional
 * sequence in declared field order. A seq shorter than the field list
 * is prefix-tolerant (trailing members are absent or defaulted); a
 * longer one is serde's invalid-length `shape` error (issue #106
 * review r6 #1). `fields` must be the DECLARED field order. */
function seqAsMembers(value, fields, path) {
  if (!Array.isArray(value)) return { ok: true, value };
  if (value.length > fields.length) return failure("shape");
  const members = {};
  fields.forEach((field, index) => {
    if (index < value.length) members[field] = value[index];
  });
  return { ok: true, value: members };
}

/** The struct-position admission shape: a member map (closed) or a
 * positional sequence prefix of the declared fields — never a scalar
 * (issue #106 review r6 #1). */
function structValue(value, fields) {
  if (value === null || typeof value !== "object") return false;
  if (Array.isArray(value)) return value.length <= fields.length;
  for (const key of Object.keys(value)) {
    if (!fields.includes(key)) return false;
  }
  return true;
}

/** The closed safe reason tokens an SDK decoder refuses with. */
export const DECODE_REASONS = [
  "bounds", "duplicate-key", "explicit-null", "grammar", "member", "not-json",
  "operation", "oversize", "request-id", "shape", "token", "trailing-document",
  "unknown-member", "version",
];

/** The declared wire field order of the closed structs (wire.rs), the
 * positional-seq mapping order (serde visit_seq parity, issue #106
 * review r6 #1). */
const REQUEST_FIELDS = ["protocol", "protocol_version", "operation", "request_id", "project_root", "ir_path", "target", "profile", "profile_digest", "profile_capabilities", "dry_run", "limits", "plan_id", "native_request", "lint_request"];
const RESPONSE_FIELDS = ["protocol", "protocol_version", "operation", "request_id", "status", "evidence", "capabilities", "result", "writes", "progress", "error"];
const NATIVE_REQUEST_FIELDS = ["changes", "scan_ref", "observed_ref", "execution_policy_ref", "input_manifest_digest", "tool_catalog_digest", "capability_snapshot_digest"];
const CAPABILITIES_FIELDS = ["adapter", "protocol_versions", "operations", "transports", "targets", "profiles", "read_scopes", "write_scopes", "progress", "ir_versions", "capabilities", "constraints"];

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

function decodeIdentity(rawValue) {
  if (!structValue(rawValue, ["id", "version", "digest"])) return failure("shape");
  const value = seqAsMembers(rawValue, ["id", "version", "digest"]).value;
  const closed = closeObject(value, ["digest", "id", "version"]);
  if (closed) return closed;
  if (!isToken(value.id)) return failure("bounds");
  if (!isContractVersion(value.version)) return failure("bounds");
  if (!isSha256Digest(value.digest)) return failure("bounds");
  return { ok: true, value };
}

function decodeEvidence(rawValue) {
  if (!structValue(rawValue, ["adapter", "plan_id"])) return failure("shape");
  const value = seqAsMembers(rawValue, ["adapter", "plan_id"]).value;
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

function decodeLimits(rawValue) {
  if (!structValue(rawValue, ["timeout_ms", "max_output_bytes"])) return failure("shape");
  const value = seqAsMembers(rawValue, ["timeout_ms", "max_output_bytes"]).value;
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
  // Production requires a present, non-empty, strictly-sorted list
  // (wire.rs profile_capabilities: `!capabilities.is_empty()` is part
  // of the sorted_unique verdict) — an empty list is admission `bounds`,
  // never an empty default (issue #106 review r5 #1).
  if (!Array.isArray(value) || value.length === 0 || value.length > BOUNDS.MAX_DECLARED_CAPABILITIES) return failure("bounds");
  let previous = null;
  for (const rawCapability of value) {
    if (!structValue(rawCapability, ["id", "support"])) return failure("shape");
    const capability = seqAsMembers(rawCapability, ["id", "support"]).value;
    const closed = closeObject(capability, ["id", "support"]);
    if (closed) return closed;
    if (!isCapabilityId(capability.id)) return failure("bounds");
    if (!SUPPORT_STATES.includes(capability.support)) return failure("shape");
    if (previous !== null && capability.id <= previous) return failure("bounds");
    previous = capability.id;
  }
  return { ok: true, value };
}

function decodeContentRef(rawValue) {
  if (!structValue(rawValue, ["digest", "revision", "adapter"])) return failure("shape");
  const value = seqAsMembers(rawValue, ["digest", "revision", "adapter"]).value;
  const closed = closeObject(value, ["adapter", "digest", "revision"]);
  if (closed) return closed;
  if (!isSha256Digest(value.digest)) return failure("bounds");
  // Revision is an unbounded wire string; the optional adapter identity carries the production adapter token
  // grammar (issue #106 review r4 #2, core wire.rs validate:
  // `!scopes::is_token(adapter)` refuses `native-request`): a present,
  // non-token identity must never reach dispatch.
  const absentAdapter = optional(value.adapter);
  if (absentAdapter) return absentAdapter;
  if (value.adapter !== undefined && !isToken(value.adapter)) return failure("bounds");
  for (const member of ["revision"]) {
    const absent = optional(value[member]);
    if (absent) return absent;
    // Production types `revision` `Option<String>` with no bound beyond
    // serde's own string admission (issue #106 review r5 #9): only the
    // member type is checked, never a length the wire does not declare.
    if (value[member] !== undefined && typeof value[member] !== "string") return failure("shape");
  }
  return { ok: true, value };
}

function decodeNativeChanges(rawValue) {
  // All-default struct: accepted from `{}` and from a positional seq
  // (`[]` decodes with both lists defaulted) exactly like production's
  // serde struct-from-seq (issue #106 review r5 #9).
  if (!structValue(rawValue, ["files", "symbols"])) return failure("shape");
  const seq = seqAsMembers(rawValue, ["files", "symbols"]);
  if (!seq.ok) return seq;
  const value = seq.value;
  const closed = closeObject(value, ["files", "symbols"]);
  if (closed) return closed;
  const files = value.files === undefined ? [] : value.files;
  const symbols = value.symbols === undefined ? [] : value.symbols;
  if (!Array.isArray(files) || files.length > 1024) return failure("bounds");
  if (!Array.isArray(symbols) || symbols.length > 1024) return failure("bounds");
  // Production validates the changed files without any ordering rule
  // (wire.rs native-request: member checks only) — the SDK's old
  // ascending-path requirement refused legal documents (review r5 #9).
  for (const [index, rawChange] of files.entries()) {
    if (!structValue(rawChange, ["path", "change", "before_digest", "after_digest"])) return failure("shape");
    const change = seqAsMembers(rawChange, ["path", "change", "before_digest", "after_digest"]).value;
    files[index] = change;
    const inner = closeObject(change, ["after_digest", "before_digest", "change", "path"]);
    if (inner) return inner;
    if (!isLogicalPath(change.path)) return failure("bounds");
    if (!ENUMS.nativeChangeKind.includes(change.change)) return failure("shape");
    for (const member of ["after_digest", "before_digest"]) {
      const absent = optional(change[member]);
      if (absent) return absent;
      if (change[member] !== undefined && !isSha256Digest(change[member])) return failure("bounds");
    }
  }
  // Symbols carry no per-item bound on the wire (only the count is
  // bounded) — empty and long strings are both legal (review r5 #9).
  for (const symbol of symbols) {
    if (typeof symbol !== "string") return failure("shape");
  }
  return { ok: true, value: { files, symbols } };
}

function decodeNativeRequest(rawValue) {
  if (!structValue(rawValue, NATIVE_REQUEST_FIELDS)) return failure("shape");
  const value = seqAsMembers(rawValue, NATIVE_REQUEST_FIELDS).value;
  const closed = closeObject(value, NATIVE_REQUEST_FIELDS);
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
  // The adjacent-tag enum also admits the serde `visit_seq` pair:
  // `["known", value]` and `[tag, null]` (the unit variants' serde
  // `None` payload) positionally; `[tag]` alone, a longer seq, a
  // non-null payload at a unit variant, or a non-string payload at
  // `known` is serde's invalid-length / invalid-type `shape` (issue
  // #106 review r6 #1, r8 #2). The seq spelling is NORMALIZED onto the
  // member map so every later validate reader sees {state,value}
  // (r8 #3).
  if (Array.isArray(value)) {
    if (value.length !== 2) return failure("shape");
    const tag = value[0];
    if (tag === "known") {
      if (typeof value[1] !== "string" || value[1] === "") return failure("shape");
      if (value[1].length > BOUNDS.ERROR_DETAIL_MAX) return failure("bounds");
      return { ok: true, value: { state: "known", value: value[1] } };
    }
    if (["unknown", "withheld", "unsupported"].includes(tag)) {
      if (value[1] !== null) return failure("shape");
      return { ok: true, value: { state: tag } };
    }
    return failure("shape");
  }
  if (value === null || typeof value !== "object" || Array.isArray(value)) return failure("shape");
  const keys = Object.keys(value);
  if (value.state === "known") {
    if (keys.length !== 2 || !keys.includes("state") || !keys.includes("value")) return failure("shape");
    if (typeof value.value !== "string" || value.value === "") return failure("shape");
    if (value.value.length > BOUNDS.ERROR_DETAIL_MAX) return failure("bounds");
    return { ok: true, value };
  }
  if (["unknown", "withheld", "unsupported"].includes(value.state)) {
    // Serde also admits explicit null content on map-spelled unit variants
    // (r10); normalize all three legal spellings onto the same unit map.
    if (keys.some((key) => key !== "state" && key !== "value") || (value.value !== undefined && value.value !== null)) return failure("shape");
    return { ok: true, value: { state: value.state } };
  }
  return failure("shape");
}

function decodeLintBinding(rawValue) {
  // The pinned 0.6.4 schema spells the members camelCase (nativeId);
  // the seq spelling follows the declared struct order (r6 #1).
  if (!structValue(rawValue, ["path", "nativeId", "symbol", "kind", "fingerprint"])) return failure("shape");
  const value = seqAsMembers(rawValue, ["path", "nativeId", "symbol", "kind", "fingerprint"]).value;
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

function decodeLintRequest(rawValue) {
  if (!structValue(rawValue, ["scope", "pins", "bindings", "files"])) return failure("shape");
  const value = seqAsMembers(rawValue, ["scope", "pins", "bindings", "files"]).value;
  const closed = closeObject(value, ["bindings", "files", "pins", "scope"]);
  if (closed) return closed;
  // Every lint member is required on the wire (production's LintRequest
  // has no serde defaults): a missing member is `shape` exactly like
  // serde's missing-field error — only a null gets its own token
  // (issue #106 review r5 #8).
  for (const member of ["scope", "files", "bindings", "pins"]) {
    if (value[member] === undefined) return failure("shape");
    // pins is a REQUIRED struct (not an Option): an explicit null is
    // serde's wrong-type `shape`, never the Option null token
    // (issue #106 review r6 #7).
    if (member !== "pins") {
      const absent = optional(value[member]);
      if (absent) return absent;
    }
  }
  if (!Array.isArray(value.scope) || value.scope.length === 0 || value.scope.length > 10_000) return failure("bounds");
  if (!Array.isArray(value.files) || value.files.length > 4_096) return failure("bounds");
  if (!Array.isArray(value.bindings) || value.bindings.length > 10_000) return failure("bounds");
  for (const scope of value.scope) {
    // Type only at decode (issue #106 review r8 #9): an empty or
    // non-token scope item is the semantic layer's `lint-request`
    // refusal (the ai-lint token grammar), never a decode-stage length
    // bound — production's typed Vec<String> admits any string item.
    if (typeof scope !== "string" || scope.length > BOUNDS.ERROR_DETAIL_MAX) return failure("bounds");
  }
  for (const file of value.files) {
    if (!isLogicalPath(file)) return failure("bounds");
  }
  for (const [bindingIndex, rawBinding] of value.bindings.entries()) {
    const binding_ = decodeLintBinding(rawBinding);
    if (!binding_.ok) return binding_;
    value.bindings[bindingIndex] = binding_.value;
  }
  if (!structValue(value.pins, ["model", "ir", "observed", "revision", "profile", "capabilities"])) return failure("shape");
  const pinsValue = seqAsMembers(value.pins, ["model", "ir", "observed", "revision", "profile", "capabilities"]).value;
  value.pins = pinsValue;
  const pins = closeObject(value.pins, ["capabilities", "ir", "model", "observed", "profile", "revision"]);
  if (pins) return pins;
  // Every pin is a State-tagged value; model/ir must carry known digests.
  // The admitted value is the NORMALIZED map form (seq pairs remap onto
  // {state,value}) so the semantic validate pass never re-reads the seq
  // spelling (issue #106 review r8 #3).
  for (const member of ["capabilities", "ir", "model", "observed", "profile", "revision"]) {
    const state = decodeStateString(value.pins[member]);
    if (!state.ok) return state;
    value.pins[member] = state.value;
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
  let value = document.value;
  if (!structValue(value, REQUEST_FIELDS)) {
    // Preserve the closed-vocabulary reason: an object with an unknown
    // member is `unknown-member`, any other shape mismatch is `shape`.
    if (value !== null && typeof value === "object" && !Array.isArray(value)) {
      const closed = closeObject(value, REQUEST_FIELDS);
      if (closed) return closed;
    }
    return failure("shape");
  }
  // A null ELEMENT inside a positional envelope seq is serde's typed
  // invalid-type `shape`, never the map spelling's explicit-null token
  // (issue #106 review r8 #9).
  const seqSpelled = Array.isArray(value);
  const absentMember = (member) => {
    if (value[member] === null) return seqSpelled ? failure("shape", member) : failure("explicit-null", member);
    return null;
  };
  value = seqAsMembers(value, REQUEST_FIELDS).value;
  const closed = closeObject(value, REQUEST_FIELDS);
  if (closed) return closed;
  for (const member of ["protocol", "protocol_version", "operation", "request_id", "project_root"]) {
    if (value[member] === undefined) return failure("shape");
    const absent = absentMember(member);
    if (absent) return absent;
  }
  if (value.protocol !== WIRE_TOKEN) return failure("token");
  if (!SUPPORTED_PROTOCOL_VERSIONS.includes(value.protocol_version)) return failure("version");
  if (!LATEST_OPERATIONS.includes(value.operation)) return failure("operation");
  if (!isRequestId(value.request_id)) return failure("request-id");
  if (value.project_root !== ".") return failure("grammar");
  for (const member of ["ir_path", "target", "profile"]) {
    const absent = absentMember(member);
    if (absent) return absent;
    if (value[member] !== undefined) {
      const ok = member === "ir_path" ? isLogicalPath(value[member]) : isToken(value[member]);
      if (!ok) return failure("grammar", member);
    }
  }
  for (const member of ["profile_digest"]) {
    const absent = absentMember(member);
    if (absent) return absent;
    if (value[member] !== undefined && !isSha256Digest(value[member])) return failure("bounds");
  }
  for (const member of ["dry_run"]) {
    const absent = absentMember(member);
    if (absent) return absent;
    if (value[member] !== undefined && typeof value[member] !== "boolean") return failure("shape");
  }
  for (const member of ["plan_id"]) {
    const absent = absentMember(member);
    if (absent) return absent;
    if (value[member] !== undefined && !isPlanId(value[member])) return failure("bounds");
  }
  const absentLimits = absentMember("limits");
  if (absentLimits) return absentLimits;
  if (value.limits !== undefined) {
    const limits = decodeLimits(value.limits);
    if (!limits.ok) return limits;
  }
  const absentCaps = absentMember("profile_capabilities");
  if (absentCaps) return absentCaps;
  const capabilities = decodeProfileCapabilities(value.profile_capabilities);
  if (!capabilities.ok) return capabilities;
  const absentNative = absentMember("native_request");
  if (absentNative) return absentNative;
  if (value.native_request !== undefined) {
    const native = decodeNativeRequest(value.native_request);
    if (!native.ok) return native;
  }
  const absentLint = absentMember("lint_request");
  if (absentLint) return absentLint;
  if (value.lint_request !== undefined) {
    const lint = decodeLintRequest(value.lint_request);
    if (!lint.ok) return lint;
    value.lint_request = lint.value;
  }
  return { ok: true, value };
}

function decodeWriteEntry(rawValue) {
  if (!structValue(rawValue, ["path", "action", "sha256"])) return failure("shape");
  const value = seqAsMembers(rawValue, ["path", "action", "sha256"]).value;
  const closed = closeObject(value, ["action", "path", "sha256"]);
  if (closed) return closed;
  if (!isLogicalPath(value.path)) return failure("bounds");
  if (!ENUMS.writeAction.includes(value.action)) return failure("shape");
  // `sha256` is a free-form `Option<String>` on the wire (production
  // has no digest grammar here — the action/digest pairing is the
  // client's semantic layer): any string is admissible, only the type
  // and the explicit-null refusal remain (issue #106 review r5 #9).
  const absent = optional(value.sha256);
  if (absent) return absent;
  if (value.sha256 !== undefined && typeof value.sha256 !== "string") return failure("shape");
  return { ok: true, value };
}

function decodeResult(rawValue) {
  // All-default operation result: production deserializes it from a
  // positional seq too (`result:[]` is the all-default value), exactly
  // like serde struct-from-seq (issue #106 review r5 #9).
  if (!structValue(rawValue, ["ok", "truncated", "entries", "findings", "bindings", "native_plan", "lint_evidence"])) return failure("shape");
  const seq = seqAsMembers(rawValue, ["ok", "truncated", "entries", "findings", "bindings", "native_plan", "lint_evidence"]);
  if (!seq.ok) return seq;
  const value = seq.value;
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
    for (const [bindingIndex, rawBinding] of value.bindings.entries()) {
      if (!structValue(rawBinding, ["module", "target", "profile"])) return failure("shape");
      const binding = seqAsMembers(rawBinding, ["module", "target", "profile"]).value;
      value.bindings[bindingIndex] = binding;
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
 * Defensive token/digest/count ceilings exceed typed wire admission by
 * design; oracle-backed productionAccepts rows pin them (sdk/spec.md §4).
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

/** The lint-side State<T> seq spelling (issue #106 review r6 #1, r8
 * #2): returns the normalized {state,value} map only for the STRICT
 * serde pair — exactly two elements, and the unit variants carry only
 * the null payload. Anything else is not the seq spelling. */
const lintStateSeq = (value) => {
  if (!Array.isArray(value) || value.length !== 2) return null;
  const tag = value[0];
  if (!["known", "unknown", "withheld", "unsupported"].includes(tag)) return null;
  if (tag !== "known" && value[1] !== null) return null;
  return { state: tag, value: value[1] };
};

/** The lint-side State<T> verdicts are tri-state (issue #106 review r8
 * #9): a wrong SPELLING is serde's typed decode failure `shape`; a
 * well-shaped known value that violates the bounded grammar is
 * `bounds`. */
const lintStateString = (value) => {
  const seq = lintStateSeq(value);
  if (seq) {
    if (seq.state !== "known") return true;
    if (typeof seq.value !== "string") return "shape";
    return lintBounded256(seq.value) ? true : "bounds";
  }
  const closed = closeObject(value, ["state", "value"]);
  if (closed) return "shape";
  if (value.state === "known") {
    if (typeof value.value !== "string") return "shape";
    return lintBounded256(value.value) ? true : "bounds";
  }
  if (["unknown", "withheld", "unsupported"].includes(value.state)) {
    return value.value === undefined || value.value === null ? true : "shape";
  }
  return "shape";
};

const lintStateBool = (value) => {
  const seq = lintStateSeq(value);
  if (seq) {
    if (seq.state !== "known") return true;
    return typeof seq.value === "boolean" ? true : "shape";
  }
  const closed = closeObject(value, ["state", "value"]);
  if (closed) return "shape";
  if (value.state === "known") {
    return typeof value.value === "boolean" ? true : "shape";
  }
  if (["unknown", "withheld", "unsupported"].includes(value.state)) {
    return value.value === undefined || value.value === null ? true : "shape";
  }
  return "shape";
};

const lintStateU64 = (value) => {
  const seq = lintStateSeq(value);
  if (seq) {
    if (seq.state !== "known") return true;
    if (!isInt(seq.value)) return "shape";
    return seq.value <= 10_000_000 ? true : "bounds";
  }
  const closed = closeObject(value, ["state", "value"]);
  if (closed) return "shape";
  if (value.state === "known") {
    if (!isInt(value.value)) return "shape";
    return value.value <= 10_000_000 ? true : "bounds";
  }
  if (["unknown", "withheld", "unsupported"].includes(value.state)) {
    return value.value === undefined || value.value === null ? true : "shape";
  }
  return "shape";
};

const lintPins = (rawValue) => {
  if (!structValue(rawValue, ["model", "ir", "observed", "revision", "profile", "capabilities"])) return "shape";
  const value = seqAsMembers(rawValue, ["model", "ir", "observed", "revision", "profile", "capabilities"]).value;
  const closed = closeObject(value, ["capabilities", "ir", "model", "observed", "profile", "revision"]);
  if (closed) return "shape";
  for (const pin of ["capabilities", "ir", "model", "observed", "profile", "revision"]) {
    const verdict = lintStateString(value[pin]);
    if (verdict !== true) return verdict;
  }
  return true;
};

const lintProducer = (rawValue) => {
  if (!structValue(rawValue, ["id", "version", "artifactDigest", "tool", "compiler", "framework", "recipe"])) return "shape";
  const value = seqAsMembers(rawValue, ["id", "version", "artifactDigest", "tool", "compiler", "framework", "recipe"]).value;
  const closed = closeObject(value, ["artifactDigest", "compiler", "framework", "id", "recipe", "tool", "version"]);
  if (closed) return "shape";
  for (const member of ["id", "version", "tool", "recipe"]) {
    if (typeof value[member] !== "string") return "shape";
    if (!lintBounded256(value[member])) return "bounds";
  }
  if (typeof value.artifactDigest !== "string") return "shape";
  if (!(value.artifactDigest.length <= 71 && LINT_SHA256.test(value.artifactDigest))) return "bounds";
  for (const member of ["compiler", "framework"]) {
    const verdict = lintStateString(value[member]);
    if (verdict !== true) return verdict;
  }
  return true;
};

const lintSource = (rawValue) => {
  if (!structValue(rawValue, ["id", "path", "fingerprint", "bytes"])) return false;
  const value = seqAsMembers(rawValue, ["id", "path", "fingerprint", "bytes"]).value;
  const closed = closeObject(value, ["bytes", "fingerprint", "id", "path"]);
  if (closed) return false;
  return lintBounded256(value.id)
    && typeof value.path === "string" && value.path.length <= 512 && isLogicalPath(value.path)
    && typeof value.fingerprint === "string" && value.fingerprint.length <= 71 && LINT_SHA256.test(value.fingerprint)
    && isInt(value.bytes) && value.bytes <= BOUNDS.MAX_REQUEST_BYTES;
};

const lintSpan = (rawValue) => {
  if (!structValue(rawValue, ["id", "source", "start", "end", "line", "column", "endLine", "endColumn"])) return false;
  const value = seqAsMembers(rawValue, ["id", "source", "start", "end", "line", "column", "endLine", "endColumn"]).value;
  const closed = closeObject(value, ["column", "end", "endColumn", "endLine", "id", "line", "source", "start"]);
  if (closed) return false;
  // Every span member is an unbounded `u64` in production (0 is a legal
  // line/column; the SDK's old min-1 refused legal evidence — review
  // r5 #9).
  const bounded = (member) => isInt(value[member]) && value[member] <= BOUNDS.MAX_REQUEST_BYTES;
  return lintBounded256(value.id)
    && lintBounded256(value.source)
    && bounded("start")
    && bounded("end")
    && bounded("line")
    && bounded("column")
    && bounded("endLine")
    && bounded("endColumn");
};

const lintCoverage = (rawValue) => {
  if (!structValue(rawValue, ["rule", "target", "scope", "state", "eligible", "examined", "limitations"])) return "shape";
  const value = seqAsMembers(rawValue, ["rule", "target", "scope", "state", "eligible", "examined", "limitations"]).value;
  const closed = closeObject(value, ["eligible", "examined", "limitations", "rule", "scope", "state", "target"]);
  if (closed) return "shape";
  if (!["complete", "partial", "unknown", "unsupported", "withheld", "disabled"].includes(value.state)) return "shape";
  for (const member of ["rule", "target", "scope"]) {
    if (typeof value[member] !== "string") return "shape";
    if (!lintBounded256(value[member])) return "bounds";
  }
  for (const member of ["eligible", "examined"]) {
    const verdict = lintStateU64(value[member]);
    if (verdict !== true) return verdict;
  }
  if (!Array.isArray(value.limitations)) return "shape";
  return lintStringArray(value.limitations, 10_000, true) ? true : "bounds";
};

const lintRecord = (rawValue) => {
  const recordFields = ["id", "kind", "mechanism", "subject", "semanticSymbol", "operation", "resource", "field", "nativeId", "confidence", "currency", "origin", "claim", "binding", "configuration", "ownership", "trace", "value", "key", "candidates", "activation", "locations", "guards"];
  if (!structValue(rawValue, recordFields)) return "shape";
  const value = seqAsMembers(rawValue, recordFields).value;
  const closed = closeObject(value, [
    "activation", "binding", "candidates", "claim", "confidence", "configuration",
    "currency", "field", "guards", "id", "key", "kind", "locations", "mechanism",
    "nativeId", "operation", "origin", "ownership", "resource", "semanticSymbol",
    "subject", "trace", "value",
  ]);
  if (closed) return "shape";
  // Required members and enums are serde's typed-decode `shape` class;
  // the bounded string grammars stay `bounds` (issue #106 review r8 #9).
  for (const member of ["kind", "mechanism", "confidence", "currency", "origin", "claim", "id", "subject", "nativeId"]) {
    if (value[member] === undefined) return "shape";
  }
  const enumsOk = ["binding-candidates", "dispatch", "convention", "effect", "path", "reflection", "string-reference", "default", "native-edge", "field-write"].includes(value.kind)
    && ["direct", "observer", "hook", "magic"].includes(value.mechanism)
    && ["unknown", "low", "medium", "high", "exact"].includes(value.confidence)
    && ["current", "stale", "unknown"].includes(value.currency)
    && ["explicit", "extracted", "inferred"].includes(value.origin)
    && ["structural", "possible-behavior", "verified-behavior"].includes(value.claim);
  if (!enumsOk) return "shape";
  for (const member of ["id", "subject", "nativeId"]) {
    if (typeof value[member] !== "string") return "shape";
    if (!lintBounded256(value[member])) return "bounds";
  }
  const route = (rawEntry) => {
    if (!structValue(rawEntry, ["identity", "confidence", "currency", "origin"])) return false;
    const entry = seqAsMembers(rawEntry, ["identity", "confidence", "currency", "origin"]).value;
    if (closeObject(entry, ["confidence", "currency", "identity", "origin"])) return false;
    return lintBounded256(entry.identity)
      && ["unknown", "low", "medium", "high", "exact"].includes(entry.confidence)
      && ["current", "stale", "unknown"].includes(entry.currency)
      && ["explicit", "extracted", "inferred"].includes(entry.origin);
  };
  const step = (rawEntry) => {
    if (!structValue(rawEntry, ["role", "identity", "locations", "guards", "confidence"])) return false;
    const entry = seqAsMembers(rawEntry, ["role", "identity", "locations", "guards", "confidence"]).value;
    if (closeObject(entry, ["confidence", "guards", "identity", "locations", "role"])) return false;
    return ["binding", "trigger", "registration", "callback", "effect"].includes(entry.role)
      && lintBounded256(entry.identity)
      && lintStringArray(entry.locations, 32, true)
      && lintStringArray(entry.guards, 32, true)
      && ["unknown", "low", "medium", "high", "exact"].includes(entry.confidence);
  };
  for (const member of ["semanticSymbol", "operation", "resource", "field", "value", "key"]) {
    const verdict = lintStateString(value[member]);
    if (verdict !== true) return verdict;
  }
  for (const member of ["binding", "configuration", "ownership", "trace"]) {
    const verdict = lintStateBool(value[member]);
    if (verdict !== true) return verdict;
  }
  for (const member of ["candidates", "activation"]) {
    if (!Array.isArray(value[member])) return "shape";
    if (value[member].length > 32) return "bounds";
  }
  if (!value.candidates.every(route)) return "bounds";
  if (!value.activation.every(step)) return "bounds";
  for (const member of ["locations", "guards"]) {
    if (!Array.isArray(value[member])) return "shape";
    if (!lintStringArray(value[member], 32, true)) return "bounds";
  }
  return true;
};

function decodeLintEvidence(rawValue) {
  if (!structValue(rawValue, ["schemaVersion", "identity", "target", "scope", "producer", "pins", "inputManifestDigest", "sources", "locations", "coverage", "records", "limitations"])) return failure("shape");
  const value = seqAsMembers(rawValue, ["schemaVersion", "identity", "target", "scope", "producer", "pins", "inputManifestDigest", "sources", "locations", "coverage", "records", "limitations"]).value;
  const closed = closeObject(value, LINT_EVIDENCE_MEMBERS);
  if (closed) return closed;
  for (const member of LINT_EVIDENCE_MEMBERS) {
    if (value[member] === undefined) return failure("shape", member);
  }
  if (value.schemaVersion !== "lekalo/ai-lint-evidence/v0.6.4") return failure("shape");
  if (value.identity !== "dev.lekalo.ai-lint-evidence@0.6.4") return failure("shape");
  if (typeof value.target !== "string") return failure("shape");
  if (!lintBounded256(value.target)) return failure("bounds");
  if (!Array.isArray(value.scope)) return failure("shape");
  if (!lintStringArray(value.scope, 10_000, true)) return failure("bounds");
  // The lint-subtree helpers are tri-state (issue #106 review r8 #9): a
  // wrong spelling is the typed-decode `shape` class, the bounded
  // grammars are `bounds`.
  for (const [helper, member] of [[lintProducer, "producer"], [lintPins, "pins"]]) {
    const verdict = helper(value[member]);
    if (verdict !== true) return failure(verdict);
  }
  if (typeof value.inputManifestDigest !== "string") return failure("shape");
  if (value.inputManifestDigest.length > 71 || !LINT_SHA256.test(value.inputManifestDigest)) {
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
    const verdict = lintCoverage(coverage);
    if (verdict !== true) return failure(verdict);
  }
  if (!Array.isArray(value.records) || value.records.length > 10_000) return failure("bounds");
  for (const record of value.records) {
    const verdict = lintRecord(record);
    if (verdict !== true) return failure(verdict);
  }
  if (!lintStringArray(value.limitations, 10_000, true)) return failure("bounds");
  return { ok: true, value };
}

/** Stable structural key for array-of-object uniqueness checks (issue
 * #106 review r3 #7): the canonical serializer (recursively sorted
 * member names), so deep-equal spans hash equally regardless of member
 * order and ≥2 distinct locations never collide. The old key fed
 * flattenMembers VALUES to JSON.stringify as a PropertyList — which
 * kept only the numeric indices — so every span serialized as "{}" and
 * any evidence object with two or more distinct locations was falsely
 * refused as `bounds`. */
const canonicalKey = (value) => canonicalJson(value);

function decodeScanEntry(rawValue) {
  if (!structValue(rawValue, ["path", "kind", "detail", "evidence"])) return failure("shape");
  let value = seqAsMembers(rawValue, ["path", "kind", "detail", "evidence"]).value;
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
    // All-default evidence: `{}` and the positional `[]` both decode
    // (signature/references default) exactly like production's serde
    // struct-from-seq (issue #106 review r5 #9).
    if (!structValue(value.evidence, ["signature", "references"])) return failure("shape");
    const seq = seqAsMembers(value.evidence, ["signature", "references"]);
    if (!seq.ok) return seq;
    const evidence = seq.value;
    const inner = closeObject(evidence, ["references", "signature"]);
    if (inner) return inner;
    const absentSignature = optional(evidence.signature);
    if (absentSignature) return absentSignature;
    if (evidence.signature !== undefined && !isSha256Digest(evidence.signature)) return failure("bounds");
    // `references` is a `#[serde(default)] Vec`: absent defaults empty;
    // an explicit null is a typed refusal, never an empty default.
    if (evidence.references === null) return failure("explicit-null");
    const references = evidence.references === undefined ? [] : evidence.references;
    if (!Array.isArray(references) || references.length > BOUNDS.REFERENCES_MAX) return failure("bounds");
    for (const [refIndex, rawReference] of references.entries()) {
      if (!structValue(rawReference, ["target", "role", "confidence"])) return failure("shape");
      const reference = seqAsMembers(rawReference, ["target", "role", "confidence"]).value;
      references[refIndex] = reference;
      const refClosed = closeObject(reference, ["confidence", "role", "target"]);
      if (refClosed) return refClosed;
      // The target carries the manifest-local semantic-id grammar (core
      // trace::id::is_semantic_id, issue #106 review r3 #8), not a
      // length bound: production's response decoder refuses path-style
      // slash targets, so the SDK must too.
      if (!isSemanticId(reference.target)) return failure("bounds");
      if (!ENUMS.scanEntryReferenceRole.includes(reference.role)) return failure("shape");
      if (!ENUMS.scanEntryConfidence.includes(reference.confidence)) return failure("shape");
    }
    value = { ...value, evidence };
  }
  return { ok: true, value };
}

function decodeFinding(rawValue) {
  if (!structValue(rawValue, ["path", "code", "detail"])) return failure("shape");
  const value = seqAsMembers(rawValue, ["path", "code", "detail"]).value;
  const closed = closeObject(value, ["code", "detail", "path"]);
  if (closed) return closed;
  if (!isLogicalPath(value.path)) return failure("bounds");
  if (boundedText(value.code, BOUNDS.ERROR_CODE_MAX)) return failure("bounds");
  const absent = optional(value.detail);
  if (absent) return absent;
  if (value.detail !== undefined && boundedText(value.detail, BOUNDS.ERROR_DETAIL_MAX)) return failure("bounds");
  return { ok: true, value };
}

function decodeNativePlan(rawValue) {
  // The canonical closed NativePlanRef has exactly these six fields,
  // in declared order for serde's positional spelling (r10).
  const fields = ["kind", "digest", "commands", "packages", "completeness", "run_eligibility"];
  if (!structValue(rawValue, fields)) return failure("shape");
  const value = seqAsMembers(rawValue, fields).value;
  const closed = closeObject(value, fields);
  if (closed) return closed;
  if (!ENUMS.nativePlanKind.includes(value.kind)) return failure("shape");
  if (!isSha256Digest(value.digest)) return failure("bounds");
  for (const member of ["commands", "packages"]) {
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
  for (const [stepIndex, rawStep] of value.entries()) {
    if (!structValue(rawStep, ["step", "state", "detail"])) return failure("shape");
    const step = seqAsMembers(rawStep, ["step", "state", "detail"]).value;
    value[stepIndex] = step;
    const closed = closeObject(step, ["detail", "state", "step"]);
    if (closed) return closed;
    if (boundedText(step.step, BOUNDS.PROGRESS_TEXT_MAX)) return failure("bounds");
    if (!ENUMS.progressState.includes(step.state)) return failure("shape");
    const absent = optional(step.detail);
    if (absent) return absent;
    if (step.detail !== undefined && boundedText(step.detail, BOUNDS.PROGRESS_TEXT_MAX)) return failure("bounds");
  }
  return { ok: true, value };
}

function decodeError(rawValue) {
  if (!structValue(rawValue, ["class", "code", "message", "retryable", "partial", "detail"])) return failure("shape");
  const value = seqAsMembers(rawValue, ["class", "code", "message", "retryable", "partial", "detail"]).value;
  const closed = closeObject(value, ["class", "code", "detail", "message", "partial", "retryable"]);
  if (closed) return closed;
  if (!ENUMS.errorClass.includes(value.class)) return failure("shape");
  // Production refuses empty error strings after the bounded check
  // (wire.rs `error.code.is_empty()`), so empty strings are the typed
  // decode `shape` class, never `bounds` (issue #106 review r6 #3, r8
  // #7/#9).
  if (typeof value.code !== "string" || typeof value.message !== "string") return failure("shape");
  if (value.code === "" || value.message === "") return failure("shape");
  if (boundedText(value.code, BOUNDS.ERROR_CODE_MAX)) return failure("bounds");
  if (boundedText(value.message, BOUNDS.ERROR_MESSAGE_MAX)) return failure("bounds");
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

function decodeCapabilities(rawValue, protocolVersion) {
  if (!structValue(rawValue, CAPABILITIES_FIELDS)) return failure("shape");
  const value = seqAsMembers(rawValue, CAPABILITIES_FIELDS).value;
  const closed = closeObject(value, CAPABILITIES_FIELDS);
  if (closed) return closed;
  const identity = decodeIdentity(value.adapter);
  if (!identity.ok) return identity;
  // Required with a non-empty closed bound on the wire (core: unique(…, 1,
  // …) — min-1 is part of admission, issue #106 review r5 #1).
  const requiredList = (member, max, check) => {
    if (!Array.isArray(value[member]) || value[member].length === 0 || value[member].length > max) return false;
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
  // Operation set gating mirrors production validate_bounds exactly
  // (issue #106 review r5 #1): `lint` is legal only on a 0.6.4 describe,
  // and the closed operation ceiling is 10 there, 9 on every other
  // version.
  const operationsMax = protocolVersion === LINT_VERSION ? BOUNDS.OPERATIONS_MAX_LINT : BOUNDS.OPERATIONS_MAX_BASE;
  const isOperation = (op) => LATEST_OPERATIONS.includes(op) && (protocolVersion === LINT_VERSION || op !== "lint");
  if (!requiredList("protocol_versions", BOUNDS.PROTOCOL_VERSIONS_MAX, isContractVersion)) return failure("bounds");
  if (!requiredList("operations", operationsMax, isOperation)) return failure("bounds");
  if (!requiredList("transports", BOUNDS.TRANSPORTS_MAX, (t) => TRANSPORTS.includes(t))) return failure("bounds");
  if (!optionalList("targets", BOUNDS.TARGETS_MAX, isToken)) return failure("bounds");
  if (!optionalList("profiles", BOUNDS.PROFILES_MAX, isToken)) return failure("bounds");
  // Production checks only uniqueness and the count ceiling on the
  // scope lists (wire.rs `unique(&c.read_scopes, 0, 64)`); item content
  // is unconstrained text, so the SDK's extra non-empty / length
  // grammar is an over-strict divergence (issue #106 review r6 #4, r8
  // #8) and stays out of admission.
  if (!optionalList("read_scopes", BOUNDS.MAX_SCOPES, (s) => typeof s === "string")) return failure("bounds");
  if (!optionalList("write_scopes", BOUNDS.MAX_SCOPES, (s) => typeof s === "string")) return failure("bounds");
  if (!optionalList("ir_versions", BOUNDS.MAX_IR_VERSIONS, isContractVersion)) return failure("bounds");
  // `progress` is a `#[serde(default)] bool` in production: absent is
  // legal (defaults false); a present non-boolean is a typed refusal
  // (review r5 #9).
  if (value.progress !== undefined && typeof value.progress !== "boolean") return failure("shape");
  if (value.capabilities !== undefined) {
    const absentMap = optional(value.capabilities);
    if (absentMap) return absentMap;
    if (value.capabilities === null || typeof value.capabilities !== "object" || Array.isArray(value.capabilities)) return failure("shape");
    const ids = Object.keys(value.capabilities);
    if (ids.length > BOUNDS.MAX_DECLARED_CAPABILITIES) return failure("bounds");
    for (const id of ids) {
      if (!isCapabilityId(id)) return failure("bounds");
      // Registry membership is part of admission (production consults
      // `capability::definition(id)`), not a discovery-time nicety
      // (issue #106 review r5 #1).
      if (!isKnownCapabilityId(id)) return failure("bounds");
      if (!SUPPORT_STATES.includes(value.capabilities[id])) return failure("shape");
    }
  }
  // `constraints` is a plain `Option` in production: absent and explicit
  // null are both legal, the members are `1..=MAX` when present, and the
  // all-default struct deserializes from `{}` or a positional `[]`
  // (issue #106 review r5 #1/#9).
  if (value.constraints !== undefined && value.constraints !== null) {
    if (!structValue(value.constraints, ["max_entries", "max_writes"])) return failure("shape");
    const seq = seqAsMembers(value.constraints, ["max_entries", "max_writes"]);
    if (!seq.ok) return seq;
    const constraints = seq.value;
    const inner = closeObject(constraints, ["max_entries", "max_writes"]);
    if (inner) return inner;
    for (const member of ["max_entries", "max_writes"]) {
      if (constraints[member] === null || constraints[member] === undefined) continue;
      if (!isInt(constraints[member]) || constraints[member] < 1 || constraints[member] > BOUNDS.MAX_CONSTRAINT_VALUE) return failure("bounds");
    }
  }
  return { ok: true, value };
}

/**
 * Decode one strict response envelope from raw text (the conformance
 * client and the validate-before-write path both use this admission).
 * Duplicate keys are refused everywhere except map-typed member
 * positions: production's `capabilities.capabilities` is a serde
 * `BTreeMap` whose duplicates take last-wins (issue #106 review r5 #9).
 */
export function decodeResponse(text, maxBytes = BOUNDS.DEFAULT_MAX_OUTPUT_BYTES) {
  const document = readSingleObject(text, maxBytes, (path, previous, next) =>
    path.join(".") === "capabilities.capabilities" && ENUMS.supportState.includes(previous) && ENUMS.supportState.includes(next));
  if (!document.ok) return document;
  let value = document.value;
  if (!structValue(value, RESPONSE_FIELDS)) {
    if (value !== null && typeof value === "object" && !Array.isArray(value)) {
      const closed = closeObject(value, RESPONSE_FIELDS);
      if (closed) return closed;
    }
    return failure("shape");
  }
  // A null ELEMENT inside a positional envelope seq is serde's typed
  // invalid-type `shape`, never the map spelling's explicit-null token
  // (issue #106 review r8 #9).
  const seqSpelled = Array.isArray(value);
  const absentMember = (member) => {
    if (value[member] === null) return seqSpelled ? failure("shape", member) : failure("explicit-null", member);
    return null;
  };
  value = seqAsMembers(value, RESPONSE_FIELDS).value;
  const closed = closeObject(value, RESPONSE_FIELDS);
  if (closed) return closed;
  for (const member of ["protocol", "protocol_version", "operation", "request_id", "status", "evidence"]) {
    if (value[member] === undefined) return failure("shape");
    const absent = absentMember(member);
    if (absent) return absent;
  }
  if (value.protocol !== WIRE_TOKEN) return failure("token");
  if (!isContractVersion(value.protocol_version)) return failure("version");
  if (!LATEST_OPERATIONS.includes(value.operation)) return failure("operation");
  if (!isRequestId(value.request_id)) return failure("request-id");
  if (!ENUMS.responseStatus.includes(value.status)) return failure("shape");
  const evidence = decodeEvidence(value.evidence);
  if (!evidence.ok) return evidence;
  const absentCapabilities = absentMember("capabilities");
  if (absentCapabilities) return absentCapabilities;
  if (value.capabilities !== undefined) {
    const capabilities = decodeCapabilities(value.capabilities, value.protocol_version);
    if (!capabilities.ok) return capabilities;
  }
  const absentResult = absentMember("result");
  if (absentResult) return absentResult;
  if (value.result !== undefined) {
    const result = decodeResult(value.result);
    if (!result.ok) return result;
    // Lint evidence is version-gated exactly like production
    // validate_bounds (issue #106 review r5 #1): the member is legal
    // only on a 0.6.4 `lint` response.
    if (result.value.lint_evidence !== undefined && (value.protocol_version !== LINT_VERSION || value.operation !== "lint")) {
      return failure("shape");
    }
  }
  if (value.writes !== undefined) {
    const absent = absentMember("writes");
    if (absent) return absent;
    if (!Array.isArray(value.writes) || value.writes.length > BOUNDS.MAX_PLAN_ENTRIES) return failure("bounds");
    for (const entry of value.writes) {
      const decoded = decodeWriteEntry(entry);
      if (!decoded.ok) return decoded;
    }
  }
  const absentProgress = absentMember("progress");
  if (absentProgress) return absentProgress;
  if (value.progress !== undefined) {
    const progress = decodeProgress(value.progress);
    if (!progress.ok) return progress;
  }
  const absentError = absentMember("error");
  if (absentError) return absentError;
  if (value.error !== undefined) {
    const error = decodeError(value.error);
    if (!error.ok) return error;
  }
  return { ok: true, value };
}
