/**
 * Hono framework-evidence envelope (issue #115).
 *
 * Every relation the Hono provider reports is one closed, canonical
 * record. The envelope is adapter evidence only: relations live in the
 * `dev.lekalo.hono/` namespace, never in core's canonical vocabulary,
 * and the provider never mints confirmations, canonical domain fields,
 * or authorization facts.
 *
 * Required fact model of the issue, one field per fact:
 * - semantic/native symbol: `from`/`to` (module + native id + name);
 * - relation kind: `relation` (namespaced, closed vocabulary);
 * - source span: `span` (1-based lines/columns, logical project path);
 * - source revision: `revision` (declared provenance revision of the
 *   scan's profile, bound by input bytes);
 * - provenance: `provenance` explicit/detected/inferred (confirmation
 *   is a displayed status core owns; a scanner cannot mint it);
 * - confidence: exact/high/medium/low/unknown, never percentages;
 * - adapter/framework version: `adapterVersion`, `framework`,
 *   `rulesRevision`;
 * - freshness fingerprint: `fingerprint` — a domain-separated digest
 *   over the whole record, whose fields carry every dependency of the
 *   fact (registration method/path/order, handler/middleware identity
 *   AND structural signature, mount chain, spans). Any route/signature/
 *   source edit changes the digest and marks derived evidence stale.
 *
 * Dynamic or ambiguous facts are `incomplete`/`unknown` records with
 * machine reasons — never guesses, never silently dropped.
 */
import { createHash } from "node:crypto";

/** The adapter namespace of every relation this provider reports. */
export const HONO_RELATION_NAMESPACE = "dev.lekalo.hono/";

/** Hono package specifiers the provider recognizes (closed set). */
export const HONO_SPECIFIERS = Object.freeze([
  "hono",
  "hono/validator",
  "hono/http-exception",
  "hono/testing",
  "@hono/zod-openapi",
  "@hono/zod-validator",
]);

/** The closed relation vocabulary (namespace-prefixed at build time). */
export const HONO_RELATIONS = Object.freeze([
  "app-discovered",
  "route-handler",
  "mounts-router",
  "base-path",
  "uses-middleware",
  "validates-request",
  "returns-response",
  "handles-error",
  "context-write",
  "context-read",
  "openapi-operation",
  "handler-call",
  "route-test",
  "endpoint-contract",
].map((name) => HONO_RELATION_NAMESPACE + name));

/** Provenance of a fact: where it comes from. `confirmed` is NOT one —
 * confirmation is user/core state a scanner cannot mint. */
export const HONO_PROVENANCE = Object.freeze(["explicit", "detected", "inferred"]);

/** Confidence vocabulary (mirrors the scanner's closed set). */
export const HONO_CONFIDENCE = Object.freeze(["exact", "high", "medium", "low", "unknown"]);

/** Completeness of a fact: complete, bounded-incomplete, or unknown. */
export const HONO_STATUS = Object.freeze(["complete", "incomplete", "unknown"]);

/** Route facets: API vs server-rendered pages stay distinct. */
export const HONO_FACETS = Object.freeze(["api", "html", "ssr", "mixed", "unknown"]);

/** Explicit middleware roles. Presence is never authorization proof. */
export const HONO_MIDDLEWARE_ROLES = Object.freeze(["auth", "tenant", "context", "logging", "custom"]);

/** Closed machine reasons for incomplete/unknown records. */
export const HONO_REASONS = Object.freeze([
  "dynamic-path",
  "dynamic-method",
  "unresolved-constructor",
  "unsupported-receiver",
  "mutable-alias",
  "unknown-handler",
  "local-handler",
  "cross-module-registration-order",
  "post-mount-registration",
  "composition-cycle",
  "composition-depth",
  "conditional-applicability",
  "no-next-call-detected",
  "dynamic-context-key",
  "dynamic-status",
  "unresolved-schema",
  "unresolved-validator-target",
  "mixed-response",
  "no-response-evidence",
  "dynamic-test-target",
  "unknown-test-app",
  "ambiguous-endpoint-join",
  "missing-endpoint-join",
  "ssr-api-conflict",
  "record-budget",
  "framework-version-unknown",
]);

/** The rules revision of this provider (bumped when rules change). */
export const HONO_RULES_REVISION = "hono-rules-v1";

/** The freshness domain separator. */
const FRESHNESS_DOMAIN = "lekalo.hono.freshness.v1";

/** The provider record budget; overflow is explicit, never silent. */
export const MAX_HONO_RECORDS = 4096;
export const MAX_HONO_UNCERTAINTY = 1024;

/** Canonical JSON of one closed plain value (sorted keys, compact). */
export function canonicalHonoText(value) {
  if (value === null) return "null";
  switch (typeof value) {
    case "boolean": return value ? "true" : "false";
    case "number":
      if (!Number.isFinite(value)) return "null";
      return Number.isInteger(value) && Math.abs(value) < 1e15 ? String(value) : JSON.stringify(value);
    case "string": return JSON.stringify(value);
    case "object": break;
    default: return "null";
  }
  if (Array.isArray(value)) {
    return `[${value.map(canonicalHonoText).join(",")}]`;
  }
  const keys = Object.keys(value).sort((left, right) => (left < right ? -1 : left > right ? 1 : 0));
  return `{${keys.map((key) => `${JSON.stringify(key)}:${canonicalHonoText(value[key])}`).join(",")}}`;
}

function sha256Hex(text) {
  return createHash("sha256").update(text, "utf8").digest("hex");
}

/**
 * The freshness fingerprint of one record: a domain-separated digest
 * over the complete record content (every field the fact depends on).
 */
export function fingerprintOf(record) {
  return "sha256:" + sha256Hex(FRESHNESS_DOMAIN + "\0" + canonicalHonoText(record));
}

/**
 * Build one evidence record. Throws on closed-vocabulary violations so
 * provider bugs fail loudly instead of fabricating malformed evidence.
 */
export function makeRecord({
  relation,
  from = null,
  to = null,
  method = null,
  path = null,
  facet = null,
  role = null,
  ordinal = null,
  unwindOrdinal = null,
  httpStatus = null,
  note = null,
  provenance,
  confidence,
  status = "complete",
  reasons = [],
  span,
  revision,
  adapterVersion,
  frameworkVersion = "unknown",
}) {
  if (!HONO_RELATIONS.includes(relation)) {
    throw new Error(`unknown hono relation ${relation}`);
  }
  if (!HONO_PROVENANCE.includes(provenance)) {
    throw new Error(`unknown hono provenance ${provenance}`);
  }
  if (!HONO_CONFIDENCE.includes(confidence)) {
    throw new Error(`unknown hono confidence ${confidence}`);
  }
  if (!HONO_STATUS.includes(status)) {
    throw new Error(`unknown hono status ${status}`);
  }
  for (const reason of reasons) {
    if (!HONO_REASONS.includes(reason)) {
      throw new Error(`unknown hono reason ${reason}`);
    }
  }
  if (httpStatus !== null && typeof httpStatus !== "number") {
    throw new Error("hono record httpStatus must be a number or null");
  }
  if (facet !== null && !HONO_FACETS.includes(facet)) {
    throw new Error(`unknown hono facet ${facet}`);
  }
  if (role !== null && !HONO_MIDDLEWARE_ROLES.includes(role)) {
    throw new Error(`unknown hono middleware role ${role}`);
  }
  const record = {
    relation,
    from: normalizeEndpoint(from),
    to: normalizeEndpoint(to),
    method,
    path: path === null ? null : String(path).slice(0, 256),
    facet,
    role,
    ordinal,
    unwindOrdinal,
    httpStatus: typeof httpStatus === "number" ? httpStatus : null,
    note: note === null ? null : String(note).slice(0, 128),
    provenance,
    confidence,
    status,
    reasons: [...reasons],
    span: normalizeSpan(span),
    revision,
    adapterVersion,
    framework: { name: "hono", version: frameworkVersion },
    rulesRevision: HONO_RULES_REVISION,
  };
  record.fingerprint = fingerprintOf(record);
  return record;
}

function normalizeEndpoint(endpoint) {
  if (endpoint === null) return null;
  const normalized = {
    module: typeof endpoint.module === "string" ? endpoint.module : null,
    native: typeof endpoint.native === "string" ? endpoint.native : null,
    name: typeof endpoint.name === "string" ? endpoint.name.slice(0, 192) : null,
    indexed: endpoint.indexed === true,
    signature: typeof endpoint.signature === "string" ? endpoint.signature : null,
    digest: typeof endpoint.digest === "string" ? endpoint.digest : null,
  };
  return normalized;
}

function normalizeSpan(span) {
  return {
    path: String(span.path),
    startLine: span.startLine,
    startColumn: span.startColumn,
    endLine: span.endLine,
    endColumn: span.endColumn,
  };
}

/**
 * One uncertainty row of the provider (same shape as the generic
 * scanner's `anyUncertainty`, with a `hono:` kind prefix).
 */
export function makeUncertainty(path, kind, detail, line) {
  return { path, kind: `hono-${kind}`, detail: String(detail).slice(0, 128), line };
}

/** UTF-8 byte order comparator over canonical record text. */
export function honoCompare(left, right) {
  const a = Buffer.from(canonicalHonoText(left), "utf8");
  const b = Buffer.from(canonicalHonoText(right), "utf8");
  const length = Math.min(a.length, b.length);
  for (let index = 0; index < length; index += 1) {
    if (a[index] !== b[index]) return a[index] - b[index];
  }
  return a.length - b.length;
}

/** Canonical order: identical inputs always produce identical bytes. */
export function sortHonoRecords(records) {
  records.sort(honoCompare);
  return records;
}

/**
 * Strict validation of a whole record set (the gate the provider and
 * the tests run before evidence is attached to the index). Returns the
 * list of violations; an empty list means the set is well-formed.
 */
export function validateHonoRecords(records) {
  const violations = [];
  const seen = new Set();
  for (const record of records) {
    try {
      const { fingerprint, ...content } = record;
      const rebuilt = makeRecord(content);
      if (rebuilt.fingerprint !== fingerprint) {
        violations.push({ code: "fingerprint-mismatch", record });
      }
    } catch (error) {
      violations.push({ code: "invalid-record", message: String(error?.message ?? error), record });
    }
    const key = canonicalHonoText(record);
    if (seen.has(key)) violations.push({ code: "duplicate-record", record });
    seen.add(key);
  }
  return violations;
}
