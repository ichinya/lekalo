/**
 * Deterministic sorting, serialization, and result-building helpers
 * (issue #106): canonical write plans, artifact-manifest candidates,
 * and scenario/evidence result normalization. Contract-specific byte
 * policies stay explicit — write plans are path-sorted, scenario
 * findings are code/path-sorted, and digests frame exact bytes — a
 * universal JSON sorter is never a substitute.
 */
import { canonicalJson } from "./json.mjs";
import { isSha256Digest, planId, sha256 } from "./ids.mjs";
import { isLogicalPath, planScopeViolation, protectedHome } from "./paths.mjs";
import { BOUNDS } from "../generated/protocol.mjs";

/** Sort one write plan by path and verify the closed entry grammar. */
export function sortWritePlan(entries) {
  if (!Array.isArray(entries) || entries.length > BOUNDS.MAX_PLAN_ENTRIES) return null;
  for (const entry of entries) {
    if (entry === null || typeof entry !== "object" || Array.isArray(entry)) return null;
    if (!isLogicalPath(entry.path)) return null;
    if (!["create", "replace", "delete"].includes(entry.action)) return null;
    if (entry.action === "delete") {
      if (entry.sha256 !== undefined) return null;
    } else if (!isSha256Digest(entry.sha256)) {
      return null;
    }
  }
  return [...entries].sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
}

/** The deterministic plan identifier of one already path-sorted plan. */
export function canonicalPlanId(sortedEntries) {
  return planId(sortedEntries);
}

/** The canonical bytes of the exact file content a write entry commits to. */
export function writeEntryDigest(contentBytes) {
  return sha256(contentBytes);
}

/**
 * Build one artifact-manifest candidate (issue #106): path-sorted, each
 * row digest-addressed over its exact bytes. The candidate is evidence
 * for the host, never a publication: the host verifies ownership, lock,
 * input, and content links and owns durable publication.
 */
export function buildArtifactManifest(entries) {
  if (!Array.isArray(entries) || entries.length === 0 || entries.length > BOUNDS.MAX_PLAN_ENTRIES) return null;
  const rows = [];
  for (const entry of entries) {
    if (entry === null || typeof entry !== "object") return null;
    if (!isLogicalPath(entry.path)) return null;
    if (protectedHome(entry.path) !== null) return null;
    const digest = typeof entry.sha256 === "string" ? entry.sha256 : sha256(entry.bytes ?? "");
    if (!isSha256Digest(digest)) return null;
    rows.push({ action: entry.action ?? "create", path: entry.path, sha256: digest });
  }
  rows.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
  return { algorithm: "sha256", canonicalForm: canonicalJson(rows), entries: rows };
}

const SCENARIO_STATUSES = ["pass", "fail", "unsupported", "infrastructure", "degraded"];

/**
 * Normalize one scenario result (issue #106): the closed status set,
 * sorted findings, and bounded evidence. An unknown status never folds
 * into `pass`; the normalized result propagates the adapter's own verdict.
 */
export function normalizeScenarioResult(result) {
  if (result === null || typeof result !== "object" || Array.isArray(result)) return null;
  const status = result.status;
  if (!SCENARIO_STATUSES.includes(status)) return null;
  const findings = result.findings === undefined ? [] : result.findings;
  if (!Array.isArray(findings) || findings.length > 10_000) return null;
  for (const finding of findings) {
    if (finding === null || typeof finding !== "object") return null;
    if (typeof finding.code !== "string" || finding.code === "" || finding.code.length > 128) return null;
    if (typeof finding.path !== "string" || !isLogicalPath(finding.path)) return null;
    if (finding.detail !== undefined && (typeof finding.detail !== "string" || finding.detail.length > 128)) return null;
  }
  const sorted = [...findings].sort((a, b) => {
    const byPath = a.path < b.path ? -1 : a.path > b.path ? 1 : 0;
    if (byPath !== 0) return byPath;
    return a.code < b.code ? -1 : a.code > b.code ? 1 : 0;
  });
  const evidenceDigest = result.evidenceDigest === undefined ? null : result.evidenceDigest;
  if (evidenceDigest !== null && !isSha256Digest(evidenceDigest)) return null;
  return { evidenceDigest, findings: sorted, status };
}

/**
 * Whether a plan entry violates its write scopes or a protected home.
 * Returns the bounded violation token or null; the host enforces the
 * real denial, this check only keeps honest adapters honest.
 */
export function writePlanViolation(entry, writeScopes) {
  return planScopeViolation(entry, writeScopes);
}
