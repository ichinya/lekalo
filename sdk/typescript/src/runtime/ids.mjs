/**
 * Deterministic identifiers and digest grammar of the target protocol
 * (issue #106): SHA-256 digests, `req-`/`plan-` identifiers, and the
 * canonical bytes they hash. Byte-framing mirrors the core exactly:
 * an identifier is the prefix plus the lowercase hex SHA-256 of the
 * canonical JSON bytes.
 */
import { createHash } from "node:crypto";
import { canonicalJson } from "./json.mjs";

/** Lowercase hex SHA-256 of the UTF-8 bytes of the text. */
export function sha256Hex(text) {
  return createHash("sha256").update(text, "utf8").digest("hex");
}

/** The `sha256:<64 hex>` digest spelling of the UTF-8 bytes of the text. */
export function sha256(text) {
  return "sha256:" + sha256Hex(text);
}

export const REQUEST_ID_PREFIX = "req-";
export const PLAN_ID_PREFIX = "plan-";

export function isSha256Digest(value) {
  if (typeof value !== "string" || !value.startsWith("sha256:")) return false;
  const hex = value.slice("sha256:".length);
  return hex.length === 64 && /^[0-9a-f]{64}$/.test(hex);
}

export function isRequestId(value) {
  if (typeof value !== "string" || !value.startsWith(REQUEST_ID_PREFIX)) return false;
  const hex = value.slice(REQUEST_ID_PREFIX.length);
  return hex.length === 64 && /^[0-9a-f]{64}$/.test(hex);
}

export function isPlanId(value) {
  if (typeof value !== "string" || !value.startsWith(PLAN_ID_PREFIX)) return false;
  const hex = value.slice(PLAN_ID_PREFIX.length);
  return hex.length === 64 && /^[0-9a-f]{64}$/.test(hex);
}

/**
 * The deterministic request identifier over the canonical JSON of the
 * envelope with its `request_id` member absent. Adapters normally echo
 * the host identifier; this helper exists for the conformance client.
 */
export function requestId(envelope) {
  const copy = { ...envelope };
  delete copy.request_id;
  return REQUEST_ID_PREFIX + sha256Hex(canonicalJson(copy));
}

/** The deterministic plan identifier over the canonical bytes of one sorted plan. */
export function planId(entries) {
  return PLAN_ID_PREFIX + sha256Hex(canonicalJson(entries));
}
