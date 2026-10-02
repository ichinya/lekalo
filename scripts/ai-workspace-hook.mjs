#!/usr/bin/env node
/**
 * Issue #37 lekalo-side AI Workspace change/export hook: the opt-in
 * adapter between public Lekalo contract changes and an external,
 * separately installed AI Workspace instance. Dependency-free; run
 * from the core checkout root:
 *
 *   node scripts/ai-workspace-hook.mjs --base <ref> [--head <ref>]
 *     --manifest <path> [--send --outbox <dir> --db <path>
 *      --group <name> [--upstream <path|name>] [--config <path>]
 *      [--allow-unadmitted-send] [--quiet]
 *
 * Phases (each closes before the next opens):
 *   plan — resolve base/head to full commit ids, refuse a dirty tracked
 *     worktree, diff only the reviewed routing manifest's approved
 *     public paths over committed bytes, build the envelope (typed role
 *     explanations), validate the envelope against its closed contract
 *     shape, derive the event key over the neutral public routing
 *     projection. Writes nothing.
 *   send — only with --send --outbox --db: refuse a no-op diff, record
 *     the intent in the single-writer outbox BEFORE invoking upstream,
 *     refuse production sends while the authority matrix admits no
 *     workspace change-event kind (explicit operator override flag for
 *     a designated installation), refuse when the workspace's actual
 *     recipient set is not exactly the reviewed one, spawn the upstream
 *     `event create` as an argv array (never a shell) with the widening
 *     flags forced off in the child, then reconcile by readback over
 *     the group-scoped MCP tools before recording `delivered`. Any
 *     spawn failure, timeout, unparseable receipt, readback gap, or
 *     target mismatch is `unknown-delivery`; a later invocation with an
 *     unverified prior attempt under the same key reconciles (searches
 *     existing events for the key) instead of blindly re-sending.
 *
 * The routing manifest path is REQUIRED whenever the hook does
 * anything beyond reporting `disabled`: a deployment must point at its
 * own reviewed manifest, never silently at the committed fixture.
 *
 * Privacy contract of the printed result: public source revisions,
 * approved public paths, contract identities/digests, neutral role
 * aliases, typed explanations, and closed policy references only. No
 * absolute paths, no private checkout names, no upstream numeric ids,
 * no timestamps, no upstream stdout/stderr text, no arbitrary argument
 * reflection. Every exit path — usage errors, filesystem failures,
 * unexpected exceptions — emits a closed `{ok,state,reason}` JSON
 * document; upstream/local failure text never crosses the boundary.
 *
 * The hook never reads or writes the upstream SQLite database directly
 * (all readback goes through the MCP tool surface with the widening
 * flags forced off in the child environment) and never mutates any
 * Lekalo behavior: no Lekalo Rust code path knows this script exists.
 */
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const HOOK_VERSION = "0.6.3";
const ENVELOPE_SCHEMA_VERSION = "lekalo/ai-workspace-event/v0.6.3";
const ENVELOPE_IDENTITY = "dev.lekalo.ai-workspace-event@0.6.3";
const DEFAULT_GROUP = "lekalo-dev";
const SOURCE_SLUG = "lekalo-core";
const BODY_LIMIT_BYTES = 64 * 1024;
const MAX_ARTIFACTS = 64;
const MAX_ROLES = 64;
const MAX_CHAINS = 128;
const MAX_MANIFEST_ROUTES = 64;
const MAX_MANIFEST_PATHS = 64;
const UPSTREAM_TIMEOUT_MS = 60_000;
const READBACK_TIMEOUT_MS = 30_000;
const GIT_TIMEOUT_MS = 30_000;
const OUTBOX_VERSION = 2;

const sha256Hex = (buffer) => createHash("sha256").update(buffer).digest("hex");
const sha256Ref = (buffer) => `sha256:${sha256Hex(buffer)}`;

// The canonical byte form of an envelope: every object serialized
// with recursively lexicographically sorted member names (see the
// contract description). Must stay in exact agreement with
// scripts/test-ai-workspace-contracts.mjs.
const canonicalize = (value) => {
  if (Array.isArray(value)) return `[${value.map(canonicalize).join(",")}]`;
  if (value && typeof value === "object") {
    const keys = Object.keys(value).sort();
    return `{${keys.map((key) => `${JSON.stringify(key)}:${canonicalize(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
};

const deriveEventKey = (envelope) =>
  sha256Ref(Buffer.from(canonicalize(envelope) + "|" + ENVELOPE_SCHEMA_VERSION, "utf8"));

// ---------------------------------------------------------------------------
// closed failure boundary (no text, no paths, no reflection escapes)
// ---------------------------------------------------------------------------

const USAGE = 2;
const REFUSED = 3;
const UNKNOWN_DELIVERY = 4;

// Printed never; used only to locate state for closed diagnostics.
let CURRENT_OUTBOX_DIR = null;

const writeResult = (quiet, result) => {
  if (!quiet) {
    try {
      process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
    } catch {
      // The result channel itself failed; there is nothing safe to add.
    }
  }
};

const installClosedFailureBoundary = (quiet) => {
  const emitClosed = (reason) => {
    writeResult(quiet, { ok: false, hook: "ai-workspace-hook", state: "refused", reason });
    process.exit(REFUSED);
  };
  process.on("uncaughtException", () => emitClosed("internal-error"));
  process.on("unhandledRejection", () => emitClosed("internal-error"));
};

// ---------------------------------------------------------------------------
// process plumbing (closed translation; child text never escapes)
// ---------------------------------------------------------------------------

const runProcess = (command, argv, options = {}) => {
  let result;
  const useShell = process.platform === "win32" && /\.(cmd|bat)$/i.test(command);
  try {
    result = spawnSync(command, argv, {
      encoding: "utf8",
      timeout: options.timeoutMs ?? UPSTREAM_TIMEOUT_MS,
      input: options.input,
      cwd: options.cwd,
      env: options.env,
      windowsHide: true,
      shell: useShell,
    });
  } catch {
    return { kind: "throw", code: "spawn-throw", stdout: "", stderr: "", status: null };
  }
  if (result.error) {
    return { kind: "error", code: result.error.code ?? "spawn-error", stdout: "", stderr: "", status: null };
  }
  return {
    kind: result.status === 0 ? "ok" : "nonzero",
    code: null,
    status: result.status,
    stdout: typeof result.stdout === "string" ? result.stdout : "",
    stderr: typeof result.stderr === "string" ? result.stderr : "",
  };
};

// ---------------------------------------------------------------------------
// git inputs (argv only; never a shell)
// ---------------------------------------------------------------------------

const git = (argv) => runProcess("git", argv, { cwd: REPO_ROOT, timeoutMs: GIT_TIMEOUT_MS });

const resolveRevision = (ref) => {
  const result = git(["rev-parse", "--verify", "--quiet", `${ref}^{commit}`]);
  const value = result.stdout.trim();
  return result.kind === "ok" && /^[0-9a-f]{40}$/.test(value) ? value : null;
};

const committedBytes = (revision, repoPath) => {
  const result = git(["show", `${revision}:${repoPath}`]);
  if (result.kind !== "ok") return null;
  return Buffer.from(result.stdout, "utf8");
};

const worktreeIsClean = () => {
  // "Clean" means: no tracked working-tree modification relative to
  // the head revision whose bytes the envelope digests. Staged-new
  // files, untracked local files (operator config, outbox) and index
  // state never taint committed digests; a tracked file whose
  // worktree bytes differ from HEAD does.
  const result = git(["diff", "--quiet"]);
  return result.status === 0 || result.status === 1 ? result.status === 0 : false;
};

// ---------------------------------------------------------------------------
// routing manifest (reviewed operator input; validated closed before use)
// ---------------------------------------------------------------------------

const PUBLIC_PATH_RE = /^[a-zA-Z0-9][a-zA-Z0-9._-]*(\/[a-zA-Z0-9][a-zA-Z0-9._-]*)*$/;
const ROLE_RE = /^[a-z][a-z0-9-]{2,63}$/;
const MANIFEST_VERSION_RE = /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/;
const ROUTE_ORIGINS = new Set(["declared-service-link", "declared-artifact-dependency"]);
const ROUTE_KINDS = new Set(["depends_on", "related_to", "references", "consumes_api", "documents", "configures"]);
const ROUTE_REACTIONS = new Set(["inspect", "update", "delete", "remove_reference"]);
const ROUTE_FAMILIES = new Set(["protocol", "schema"]);

// The public routing projection: the neutral, private-member-free
// view of the manifest that feeds the public digest/event key.
// Installation-local bindings (workspaceSlug) and unknown extra
// members never enter it.
const publicRoutingProjection = (manifest) => ({
  manifestKind: "lekalo-ai-workspace-routing",
  manifestVersion: manifest.manifestVersion,
  approvedPaths: [...manifest.approvedPaths].sort(),
  routes: [...manifest.routes]
    .map((route) => ({
      role: route.role,
      origin: route.origin,
      dependencyKind: route.dependencyKind,
      reaction: route.reaction,
      watches: [...(route.watches ?? ["protocol", "schema"])].sort(),
    }))
    .sort((left, right) => left.role.localeCompare(right.role)),
});

const loadManifest = (manifestArg) => {
  // Required for every mode beyond `disabled`: a deployment must point
  // at its own reviewed manifest; silently falling back to the
  // committed fixture would route sends through template routes.
  if (!manifestArg) return { error: "manifest-required" };
  const manifestPath = resolve(REPO_ROOT, manifestArg);
  let parsed;
  try {
    if (!existsSync(manifestPath)) return { error: "manifest-missing" };
    parsed = JSON.parse(readFileSync(manifestPath, "utf8"));
  } catch {
    return { error: "manifest-malformed" };
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return { error: "manifest-malformed" };
  if (parsed.manifestKind !== "lekalo-ai-workspace-routing") return { error: "manifest-not-routing" };
  if (typeof parsed.manifestVersion !== "string" || !MANIFEST_VERSION_RE.test(parsed.manifestVersion)) {
    return { error: "manifest-version-grammar" };
  }
  if (!Array.isArray(parsed.approvedPaths) || parsed.approvedPaths.length === 0) return { error: "manifest-no-paths" };
  if (parsed.approvedPaths.length > MAX_MANIFEST_PATHS) return { error: "manifest-too-many-paths" };
  for (const path of parsed.approvedPaths) {
    if (typeof path !== "string" || !PUBLIC_PATH_RE.test(path)) return { error: "manifest-path-grammar" };
  }
  if (new Set(parsed.approvedPaths).size !== parsed.approvedPaths.length) return { error: "manifest-duplicate-paths" };
  if (!Array.isArray(parsed.routes) || parsed.routes.length === 0) return { error: "manifest-no-routes" };
  if (parsed.routes.length > MAX_MANIFEST_ROUTES) return { error: "manifest-too-many-routes" };
  for (const route of parsed.routes) {
    if (!route || typeof route !== "object" || Array.isArray(route)) return { error: "manifest-route-shape" };
    if (typeof route.role !== "string" || !ROLE_RE.test(route.role)) return { error: "manifest-role-grammar" };
    if (route.origin !== undefined && !ROUTE_ORIGINS.has(route.origin)) return { error: "manifest-origin-unknown" };
    if (route.dependencyKind !== undefined && !ROUTE_KINDS.has(route.dependencyKind)) return { error: "manifest-dependency-kind-unknown" };
    if (route.reaction !== undefined && !ROUTE_REACTIONS.has(route.reaction)) return { error: "manifest-reaction-unknown" };
    if (route.watches !== undefined) {
      if (!Array.isArray(route.watches) || route.watches.length === 0) return { error: "manifest-watches-shape" };
      for (const family of route.watches) {
        if (!ROUTE_FAMILIES.has(family)) return { error: "manifest-watches-unknown" };
      }
    }
    if (route.workspaceSlug !== undefined && (typeof route.workspaceSlug !== "string" || !ROLE_RE.test(route.workspaceSlug))) {
      return { error: "manifest-workspace-slug-grammar" };
    }
  }
  const roles = parsed.routes.map((route) => route.role);
  if (new Set(roles).size !== roles.length) return { error: "manifest-duplicate-roles" };
  return { manifest: parsed, projection: publicRoutingProjection(parsed), digest: sha256Ref(Buffer.from(JSON.stringify(publicRoutingProjection(parsed)), "utf8")) };
};

// ---------------------------------------------------------------------------
// envelope validation against the closed contract shape (independent of
// Ajv: the hook is dependency-free, so the closed checks are compiled
// from the same rules the schema states; the contracts gate cross-checks
// this validator against real Ajv behavior on the fixture)
// ---------------------------------------------------------------------------

const ENVELOPE_LIMITATIONS = new Set([
  "direct-links-only",
  "no-transitive-traversal",
  "no-schema-version-filter",
  "compatibility-unknown",
  "semantic-evidence-absent",
  "codegraph-evidence-absent",
  "upstream-receipt-unavailable",
  "no-declared-subscribers",
]);
const EXPLANATION_ORIGINS = new Set([
  "public-artifact-changed",
  "declared-service-link",
  "declared-artifact-dependency",
  "lekalo-impact",
]);
const DEPENDENCY_KINDS = ROUTE_KINDS;
const REACTIONS = ROUTE_REACTIONS;

const envelopeIsValid = (envelope) => {
  if (!envelope || typeof envelope !== "object") return false;
  if (envelope.schemaVersion !== ENVELOPE_SCHEMA_VERSION) return false;
  if (envelope.identity !== ENVELOPE_IDENTITY) return false;
  if (!/^sha256:[0-9a-f]{64}$/.test(envelope.eventKey ?? "")) return false;
  if (envelope.eventKind !== "protocol-change" && envelope.eventKind !== "schema-change") return false;
  const source = envelope.source;
  if (!source || typeof source !== "object" || Object.keys(source).length !== 6) return false;
  if (source.producer !== SOURCE_SLUG) return false;
  if (!/^[0-9a-f]{40}$/.test(source.base ?? "") || !/^[0-9a-f]{40}$/.test(source.head ?? "")) return false;
  if (source.clean !== true) return false;
  if (typeof source.producerVersion !== "string" || !MANIFEST_VERSION_RE.test(source.producerVersion)) return false;
  if (typeof source.hookVersion !== "string" || !MANIFEST_VERSION_RE.test(source.hookVersion)) return false;
  const manifest = envelope.manifest;
  if (!manifest || typeof manifest !== "object" || Object.keys(manifest).length !== 3) return false;
  if (typeof manifest.manifestVersion !== "string" || !MANIFEST_VERSION_RE.test(manifest.manifestVersion)) return false;
  if (!/^sha256:[0-9a-f]{64}$/.test(manifest.digest ?? "")) return false;
  if (!Number.isInteger(manifest.routes) || manifest.routes < 0 || manifest.routes > MAX_MANIFEST_ROUTES) return false;
  if (!Array.isArray(envelope.artifacts) || envelope.artifacts.length > MAX_ARTIFACTS) return false;
  for (const artifact of envelope.artifacts) {
    if (!artifact || typeof artifact !== "object" || Object.keys(artifact).length !== 4) return false;
    if (typeof artifact.path !== "string" || !PUBLIC_PATH_RE.test(artifact.path)) return false;
    if (!["added", "modified", "deleted"].includes(artifact.change)) return false;
    for (const side of [artifact.old, artifact.new]) {
      if (!side || typeof side !== "object" || Object.keys(side).length !== 4) return false;
      if (typeof side.name !== "string" || !/^[a-z][a-z0-9.-]*$/.test(side.name)) return false;
      const versionOk = side.version === "unknown" || MANIFEST_VERSION_RE.test(side.version);
      if (!versionOk) return false;
      if (!["present", "absent", "unknown"].includes(side.state)) return false;
      if (side.digest !== "unknown" && !/^sha256:[0-9a-f]{64}$/.test(side.digest ?? "")) return false;
    }
  }
  const impact = envelope.impact;
  if (!impact || typeof impact !== "object" || Object.keys(impact).length !== 3) return false;
  if (!Array.isArray(impact.affectedRoles) || impact.affectedRoles.length > MAX_ROLES) return false;
  for (const role of impact.affectedRoles) {
    if (!role || typeof role !== "object" || Object.keys(role).length !== 2) return false;
    if (typeof role.role !== "string" || !ROLE_RE.test(role.role)) return false;
    if (!Array.isArray(role.explanation) || role.explanation.length === 0 || role.explanation.length > MAX_CHAINS) return false;
    for (const step of role.explanation) {
      if (!step || typeof step !== "object") return false;
      if (!EXPLANATION_ORIGINS.has(step.origin)) return false;
      if (typeof step.detail !== "string" || step.detail.length === 0 || step.detail.length > 512) return false;
      if (step.sourcePath !== undefined && (typeof step.sourcePath !== "string" || !PUBLIC_PATH_RE.test(step.sourcePath))) return false;
      if (step.sourceRole !== undefined && !ROLE_RE.test(step.sourceRole ?? "")) return false;
      if (step.dependencyKind !== undefined && !DEPENDENCY_KINDS.has(step.dependencyKind)) return false;
      if (step.reaction !== undefined && !REACTIONS.has(step.reaction)) return false;
      if (step.confidence !== undefined && (typeof step.confidence !== "number" || step.confidence < 0 || step.confidence > 1)) return false;
      const allowed = Object.keys(step).filter((key) => key !== "origin" && key !== "detail" && key !== "sourcePath" && key !== "sourceRole" && key !== "dependencyKind" && key !== "reaction" && key !== "confidence");
      if (allowed.length > 0) return false;
    }
  }
  if (!["declared-only", "unknown"].includes(impact.complete)) return false;
  if (!Array.isArray(impact.limitations) || impact.limitations.length > 16) return false;
  for (const limitation of impact.limitations) {
    if (!ENVELOPE_LIMITATIONS.has(limitation)) return false;
  }
  const policy = envelope.policy;
  if (!policy || typeof policy !== "object" || Object.keys(policy).length !== 4) return false;
  if (typeof policy.sendAdmitted !== "boolean") return false;
  if (!["none", "policy-not-admitted", "send-disabled", "flag-forced-off"].includes(policy.refusalReason)) return false;
  for (const ref of [policy.authorityRef, policy.privacyPolicyRef]) {
    if (!ref || typeof ref !== "object" || Object.keys(ref).length !== 3) return false;
    if (typeof ref.contractId !== "string" || ref.contractId.length === 0 || ref.contractId.length > 128) return false;
    if (typeof ref.version !== "string" || !MANIFEST_VERSION_RE.test(ref.version)) return false;
    if (!/^sha256:[0-9a-f]{64}$/.test(ref.digest ?? "")) return false;
  }
  return true;
};

// ---------------------------------------------------------------------------
// contract identities and envelope construction
// ---------------------------------------------------------------------------

const SCHEMA_FILE_RE = /\.schema\.v(\d+\.\d+\.\d+)\.json$/;

const contractNameFor = (repoPath) => {
  const base = repoPath.split("/").pop();
  const match = base.match(SCHEMA_FILE_RE);
  if (match) {
    const family = base.replace(/\.schema\.v.*$/, "").split("/").pop();
    return `dev.lekalo.${family}`;
  }
  return `lekalo.${base.replace(/\./g, "-").toLowerCase()}`;
};

const contractIdentityFor = (revision, repoPath) => {
  const bytes = committedBytes(revision, repoPath);
  if (!bytes) return { name: contractNameFor(repoPath), version: "unknown", state: "absent", digest: "unknown" };
  const version = repoPath.match(SCHEMA_FILE_RE)?.[1] ?? "unknown";
  return { name: contractNameFor(repoPath), version, state: "present", digest: sha256Ref(bytes) };
};

const buildArtifacts = (base, head, approvedPaths) => {
  const artifacts = [];
  for (const repoPath of [...approvedPaths].sort()) {
    const oldIdentity = contractIdentityFor(base, repoPath);
    const newIdentity = contractIdentityFor(head, repoPath);
    if (oldIdentity.state === "present" && newIdentity.state === "absent") {
      artifacts.push({ path: repoPath, change: "deleted", old: oldIdentity, new: newIdentity });
    } else if (oldIdentity.state === "absent" && newIdentity.state === "present") {
      artifacts.push({ path: repoPath, change: "added", old: oldIdentity, new: newIdentity });
    } else if (oldIdentity.state === "present" && newIdentity.state === "present" && oldIdentity.digest !== newIdentity.digest) {
      artifacts.push({ path: repoPath, change: "modified", old: oldIdentity, new: newIdentity });
    }
    // absent on both sides: allowlisted but outside this range; not an
    // error and not an event.
  }
  return artifacts;
};

const SCHEMA_PATH_RE = /\.schema\.v\d+\.\d+\.\d+\.json$/;
const isProtocolPath = (repoPath) =>
  repoPath === "docs/target-protocol.md" || (/target-protocol/.test(repoPath) && SCHEMA_PATH_RE.test(repoPath));
const watchesPath = (route, repoPath) =>
  (route.watches ?? ["protocol", "schema"]).some((family) =>
    family === "protocol" ? isProtocolPath(repoPath) : SCHEMA_PATH_RE.test(repoPath)
  );

const buildImpact = (manifest, artifacts) => {
  const affectedRoles = [];
  for (const route of [...manifest.routes].sort((left, right) => left.role.localeCompare(right.role))) {
    const relevant = artifacts.filter((entry) => watchesPath(route, entry.path));
    if (relevant.length === 0) continue;
    if (affectedRoles.length >= MAX_ROLES) return { error: "too-many-roles" };
    const explanation = [];
    for (const entry of relevant) {
      if (explanation.length >= MAX_CHAINS) return { error: "too-many-chains" };
      explanation.push({
        origin: "public-artifact-changed",
        detail: `${entry.path} ${entry.change}`,
        sourcePath: entry.path,
      });
    }
    explanation.push({
      origin: route.origin === "declared-artifact-dependency" ? "declared-artifact-dependency" : "declared-service-link",
      detail: `${route.role} subscribes to lekalo-core protocol/schema changes`,
      sourceRole: SOURCE_SLUG,
      dependencyKind: route.dependencyKind ?? "depends_on",
      reaction: route.reaction ?? "inspect",
    });
    affectedRoles.push({ role: route.role, explanation });
  }
  return { affectedRoles };
};

// Accepted canonical references, read from the committed custody
// artifacts: the authority ref digests exact file bytes (the repo's
// declared authority domain); the privacy policy ref carries the
// accepted canonical policy identity from the committed contract
// manifest (NOT the raw file digest, which lives in the separate
// policyFileDigest domain). Any mismatch refuses.
const readPolicyRefs = () => {
  try {
    const authorityBytes = readFileSync(join(REPO_ROOT, "contracts/authority-matrix.v0.3.2.json"));
    const authority = JSON.parse(authorityBytes.toString("utf8"));
    const manifestBytes = readFileSync(join(REPO_ROOT, "contracts/privacy-policy.v0.3.2.manifest.json"));
    const policyManifest = JSON.parse(manifestBytes.toString("utf8"));
    const accepted = Array.isArray(policyManifest.acceptedContracts) ? policyManifest.acceptedContracts[0] : null;
    const current = policyManifest.currentAcceptedRef ?? accepted?.policyRef ?? null;
    if (!current?.policyId || !current.version || !/^sha256:[0-9a-f]{64}$/.test(current.digest ?? "")) return null;
    // The raw file must match the manifest's file digest (custody),
    // and the manifest must still be accepted.
    if (policyManifest.status !== "accepted" || policyManifest.accepted !== true) return null;
    const fileDigest = sha256Ref(readFileSync(join(REPO_ROOT, "contracts/privacy-policy.v0.3.2.json")));
    const declaredFileDigest = accepted?.policyFileDigest;
    if (declaredFileDigest !== undefined && declaredFileDigest !== fileDigest) return null;
    return {
      authorityRef: { contractId: authority.contractId, version: authority.version, digest: sha256Ref(authorityBytes) },
      privacyPolicyRef: { contractId: current.policyId, version: current.version, digest: current.digest },
    };
  } catch {
    return null;
  }
};

const producerVersion = () =>
  readFileSync(join(REPO_ROOT, "Cargo.toml"), "utf8").match(/\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"]+)"/m)?.[1] ?? null;

// ---------------------------------------------------------------------------
// upstream projection and invocation
// ---------------------------------------------------------------------------

const buildUpstreamBody = (envelope) => {
  // Bounded public provenance: the envelope itself is the public wire
  // form; this projection carries the key, identities, and digests so
  // a consumer can verify what it received (upstream stores it as the
  // event body).
  const body = [
    `eventKey=${envelope.eventKey}`,
    `eventKind=${envelope.eventKind}`,
    `schema=${ENVELOPE_IDENTITY}`,
    `source=lekalo-core ${envelope.source.base}..${envelope.source.head}`,
    `manifest=${envelope.manifest.manifestVersion} ${envelope.manifest.digest}`,
    ...envelope.artifacts.map((entry) =>
      `artifact=${entry.change}:${entry.path} ${entry.old.name}@${entry.old.version} ${entry.old.digest} -> ${entry.new.name}@${entry.new.version} ${entry.new.digest}`
    ),
    `affectedRoles=${envelope.impact.affectedRoles.map((role) => role.role).join(",")}`,
    `complete=${envelope.impact.complete}`,
    `limitations=${envelope.impact.limitations.join(",")}`,
    `policy=${envelope.policy.authorityRef.contractId}@${envelope.policy.authorityRef.version} ${envelope.policy.authorityRef.digest}`,
  ].join("\n");
  return Buffer.byteLength(body, "utf8") <= BODY_LIMIT_BYTES ? body : null;
};

const UPSTREAM_TITLE = "Lekalo target contract change";

// The one event the pipeline sends: an existing upstream kind, warning
// severity, bounded projection body. argv array; never a shell.
const sendUpstreamEvent = ({ upstream, config, dbPath, cwd, body }) => {
  const argv = [upstream];
  if (config) argv.push("--config", config);
  argv.push(
    "event", "create",
    "--kind", "service_changed",
    "--source", SOURCE_SLUG,
    "--severity", "warning",
    "--title", UPSTREAM_TITLE,
    "--body", body,
  );
  return runProcess(argv[0], argv.slice(1), {
    cwd,
    env: {
      ...process.env,
      AI_WORKSPACE_DB: dbPath,
      AI_WORKSPACE_ALLOW_PROJECT_WIDE_TOOLS: "0",
      AI_WORKSPACE_ALLOW_PROJECT_FILE_WRITE: "0",
    },
  });
};

// Keyed readback over the group-scoped MCP tool surface: the child
// server runs with the widening flags forced off regardless of what
// this process inherited. Reads the event details, the current service
// graph (which consumer slugs are actually linked to the source), and
// the full event list of the source (for keyed reconciliation).
const readbackAfterSend = ({ upstream, config, dbPath, cwd, group, eventId }) => {
  const argv = [upstream];
  if (config) argv.push("--config", config);
  argv.push("serve", "--group", group);
  const requests = [
    { jsonrpc: "2.0", id: 1, method: "tools/call", params: { name: "workspace_event_details", arguments: { event_id: eventId } } },
    { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "workspace_service_graph", arguments: {} } },
    { jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "workspace_events", arguments: { source: SOURCE_SLUG } } },
  ];
  const result = runProcess(argv[0], argv.slice(1), {
    cwd,
    timeoutMs: READBACK_TIMEOUT_MS,
    input: `${requests.map((request) => JSON.stringify(request)).join("\n")}\n`,
    env: {
      ...process.env,
      AI_WORKSPACE_DB: dbPath,
      AI_WORKSPACE_ALLOW_PROJECT_WIDE_TOOLS: "0",
      AI_WORKSPACE_ALLOW_PROJECT_FILE_WRITE: "0",
    },
  });
  if (result.kind === "error" || result.kind === "throw") return { error: "readback-unavailable" };
  const responses = result.stdout
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      try {
        return JSON.parse(line);
      } catch {
        return null;
      }
    })
    .filter(Boolean);
  const parseContent = (response) => {
    if (!response || response.error) return null;
    const text = response.result?.content?.find((entry) => entry.type === "text")?.text;
    if (!text) return null;
    try {
      return JSON.parse(text);
    } catch {
      return null;
    }
  };
  const details = parseContent(responses.find((message) => message.id === 1));
  // A missing or errored graph read is a hard verification gap, never
  // an implicit empty recipient set (fail closed).
  const graphResponse = responses.find((message) => message.id === 2);
  const graph = parseContent(graphResponse);
  if (!details) return { error: responses.some((message) => message?.error) ? "readback-denied" : "readback-no-response" };
  if (!graph || !Array.isArray(graph.links)) return { error: "readback-graph-unavailable" };
  const events = parseContent(responses.find((message) => message.id === 3));
  if (!Array.isArray(events)) return { error: "readback-events-unavailable" };
  return { details, graph, events };
};

// Delivery proof: kind, title, body, and event key must match the
// projection, every linked declared consumer must appear as a direct
// linked-service target, and NO unexpected target may appear.
const verifyDelivery = (details, { body, expectedConsumerSlugs }) => {
  if (!details || details.event?.kind !== "service_changed") return false;
  if (details.event?.title !== UPSTREAM_TITLE || details.event?.body !== body) return false;
  if (typeof details.event?.body === "string" && !details.event.body.includes(deriveKeyLine(body))) return false;
  const delivered = new Set(
    (details.affected_services ?? [])
      .filter((target) => target.relation_kind === "linked_service")
      .map((target) => target.project)
      .filter(Boolean)
  );
  // No unexpected recipients beyond the reviewed set, and every
  // reviewed linked recipient present.
  return delivered.size === expectedConsumerSlugs.length
    && expectedConsumerSlugs.every((slug) => delivered.has(slug));
};

const deriveKeyLine = (body) => {
  const match = body.match(/^eventKey=(sha256:[0-9a-f]{64})$/m);
  return match ? match[1] : "\u0000never-matches";
};

// ---------------------------------------------------------------------------
// outbox (single-writer, private local state)
// ---------------------------------------------------------------------------

const loadOutbox = (outboxDir) => {
  const path = join(outboxDir, "outbox.json");
  if (!existsSync(path)) return { version: OUTBOX_VERSION, entries: [] };
  try {
    const parsed = JSON.parse(readFileSync(path, "utf8"));
    if (parsed.version !== OUTBOX_VERSION || !Array.isArray(parsed.entries)) return null;
    return parsed;
  } catch {
    return null;
  }
};

const saveOutbox = (outboxDir, outbox) => {
  writeFileSync(join(outboxDir, "outbox.json"), `${JSON.stringify(outbox, null, 2)}\n`);
};

// Keyed reconciliation: before any new create, search the source's
// existing events for one whose body carries this event key. Returns
// the matching numeric id or null.
const reconcileByKey = (events, eventKey) => {
  for (const event of events) {
    if (typeof event?.body === "string" && event.body.includes(`eventKey=${eventKey}`)) {
      if (Number.isInteger(event.id)) return event.id;
    }
  }
  return null;
};

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

const main = () => {
  const args = {
    base: null, head: null, manifest: null, send: false, outbox: null,
    upstream: "ai-workspace", config: null, db: null, group: DEFAULT_GROUP,
    allowUnadmitted: false, quiet: false,
  };
  // Closed usage errors: the offending argument NAME is part of the
  // public CLI surface; values are never reflected.
  const bad = (argName) => {
    writeResult(args.quiet ?? false, {
      ok: false, hook: "ai-workspace-hook", state: "refused", reason: "usage", detail: argName,
    });
    process.exit(USAGE);
  };
  const argv = process.argv.slice(2);
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const value = () => {
      if (argv[index + 1] === undefined) bad(arg);
      return argv[++index];
    };
    if (arg === "--base") args.base = value();
    else if (arg === "--head") args.head = value();
    else if (arg === "--manifest") args.manifest = value();
    else if (arg === "--send") args.send = true;
    else if (arg === "--outbox") args.outbox = value();
    else if (arg === "--upstream") args.upstream = value();
    else if (arg === "--config") args.config = value();
    else if (arg === "--db") args.db = value();
    else if (arg === "--group") args.group = value();
    else if (arg === "--allow-unadmitted-send") args.allowUnadmitted = true;
    else if (arg === "--quiet") args.quiet = true;
    else bad(arg);
  }
  const emit = (result, code = 0) => {
    writeResult(args.quiet, result);
    if (code !== 0) process.exit(code);
  };
  const refuse = (reason, extra = {}) =>
    emit({ ok: false, hook: "ai-workspace-hook", state: "refused", reason, ...extra }, REFUSED);

  // Optionality is checked before anything else: without a manifest
  // the integration is off, cleanly, regardless of other arguments.
  if (!args.manifest && !args.send) {
    emit({ ok: true, hook: "ai-workspace-hook", state: "disabled", reason: "no-manifest-configured" });
    return;
  }
  if (!args.base) bad("--base");

  installClosedFailureBoundary(args.quiet);

  if (args.send && (!args.outbox || !args.db)) {
    bad("--send-requires-outbox-and-db");
  }

  const manifestLoaded = loadManifest(args.manifest);
  if (manifestLoaded.error) refuse(manifestLoaded.error);

  const base = resolveRevision(args.base);
  if (!base) refuse("base-unresolvable");
  const head = args.head ? resolveRevision(args.head) : resolveRevision("HEAD");
  if (!head) refuse("head-unresolvable");

  if (!worktreeIsClean()) refuse("worktree-dirty");
  const version = producerVersion();
  if (!version) refuse("producer-version-missing");

  const artifacts = buildArtifacts(base, head, manifestLoaded.manifest.approvedPaths);
  if (artifacts.length > MAX_ARTIFACTS) refuse("too-many-artifacts");

  // A revision range with no approved-path change is not an event: a
  // send would fan a no-op service change out to every linked consumer.
  if (artifacts.length === 0) {
    emit({ ok: true, hook: "ai-workspace-hook", state: "no-change", reason: "no-approved-artifact-changed", base, head });
    return;
  }

  const impact = buildImpact(manifestLoaded.manifest, artifacts);
  if (impact.error) refuse(impact.error);
  const policyRefs = readPolicyRefs();
  if (!policyRefs) refuse("policy-refs-unreadable");

  const limitations = [...impact.affectedRoles].length === 0
    ? ["direct-links-only", "no-transitive-traversal", "no-schema-version-filter", "compatibility-unknown", "no-declared-subscribers", "upstream-receipt-unavailable"]
    : ["direct-links-only", "no-transitive-traversal", "no-schema-version-filter", "compatibility-unknown", "upstream-receipt-unavailable"];

  const envelope = {
    schemaVersion: ENVELOPE_SCHEMA_VERSION,
    identity: ENVELOPE_IDENTITY,
    eventKind: artifacts.some((entry) => isProtocolPath(entry.path)) ? "protocol-change" : "schema-change",
    source: {
      producer: SOURCE_SLUG,
      base,
      head,
      clean: true,
      producerVersion: version,
      hookVersion: HOOK_VERSION,
    },
    manifest: {
      manifestVersion: manifestLoaded.manifest.manifestVersion,
      digest: manifestLoaded.digest,
      routes: manifestLoaded.manifest.routes.length,
    },
    artifacts,
    impact: {
      affectedRoles: impact.affectedRoles,
      complete: "declared-only",
      limitations,
    },
    policy: {
      sendAdmitted: false,
      refusalReason: "policy-not-admitted",
      ...policyRefs,
    },
  };
  envelope.eventKey = deriveEventKey(envelope);

  // The emitted envelope must satisfy the closed contract shape before
  // it is printed or sent — the hook's own boundary, independent of
  // the external validator.
  if (!envelopeIsValid(envelope)) refuse("envelope-invalid");

  if (!args.send) {
    emit({ ok: true, hook: "ai-workspace-hook", state: "planned", eventKey: envelope.eventKey, envelope });
    return;
  }

  // ---- send path ----

  // No silent widening: refuse when this process inherited either
  // widening switch enabled, whatever the child env would force.
  if (process.env.AI_WORKSPACE_ALLOW_PROJECT_WIDE_TOOLS === "1" || process.env.AI_WORKSPACE_ALLOW_PROJECT_FILE_WRITE === "1") {
    refuse("widening-flags-enabled", { eventKey: envelope.eventKey });
  }

  CURRENT_OUTBOX_DIR = args.outbox;
  try {
    mkdirSync(args.outbox, { recursive: true });
  } catch {
    refuse("outbox-unavailable", { eventKey: envelope.eventKey });
  }
  const outbox = loadOutbox(args.outbox);
  if (!outbox) refuse("outbox-corrupt", { eventKey: envelope.eventKey });

  let entry = outbox.entries.find((candidate) => candidate.eventKey === envelope.eventKey);
  if (entry?.states?.some((record) => record.state === "delivered")) {
    // Ordinary repeats after verified delivery are no-ops.
    emit({ ok: true, hook: "ai-workspace-hook", state: "delivered", eventKey: envelope.eventKey, note: "already-delivered" });
    return;
  }
  if (!entry) {
    entry = { eventKey: envelope.eventKey, eventKind: envelope.eventKind, states: [] };
    outbox.entries.push(entry);
  }
  const hasUnverifiedAttempt = entry.states.some((record) => record.state === "unknown-delivery" || record.state === "sending");
  entry.states.push({ state: "planned" });
  saveOutbox(args.outbox, outbox);

  if (!args.allowUnadmitted) {
    entry.states.push({ state: "refused", reason: "policy-not-admitted" });
    saveOutbox(args.outbox, outbox);
    refuse("policy-not-admitted", { eventKey: envelope.eventKey, envelope, outbox: "recorded" });
  }

  // Upstream availability probe (non-destructive). The probe and the
  // send share one execution path: a binary that cannot be probed to a
  // clean zero exit cannot be sent through.
  const probe = runProcess(args.upstream, ["--version"], { timeoutMs: 15_000 });
  if (probe.kind !== "ok") {
    entry.states.push({ state: "unavailable", reason: probe.kind === "nonzero" ? "upstream-probe-nonzero" : `upstream-${probe.code ?? "failed"}` });
    saveOutbox(args.outbox, outbox);
    emit({ ok: true, hook: "ai-workspace-hook", state: "unavailable", reason: "upstream-binary-missing", eventKey: envelope.eventKey });
    return;
  }

  const body = buildUpstreamBody(envelope);
  if (!body) refuse("body-limit-exceeded", { eventKey: envelope.eventKey });

  const workdir = process.cwd();

  // Reconciliation before any new create: an earlier attempt under the
  // same key may have inserted an event whose receipt we never saw.
  // Search by key through the group-scoped read surface.
  if (hasUnverifiedAttempt) {
    const preflight = readbackAfterSend({ upstream: args.upstream, config: args.config, dbPath: args.db, cwd: workdir, group: args.group, eventId: 0 });
    if (preflight.details === undefined && preflight.error === "readback-denied") {
      entry.states.push({ state: "unknown-delivery", reason: "reconciliation-readback-denied" });
      saveOutbox(args.outbox, outbox);
      emit({ ok: false, hook: "ai-workspace-hook", state: "unknown-delivery", eventKey: envelope.eventKey, reason: "reconciliation-readback-denied" }, UNKNOWN_DELIVERY);
      return;
    }
    if (Array.isArray(preflight.events)) {
      const existingId = reconcileByKey(preflight.events, envelope.eventKey);
      if (existingId !== null) {
        // The earlier attempt landed; verify it instead of re-sending.
        const readback = readbackAfterSend({ upstream: args.upstream, config: args.config, dbPath: args.db, cwd: workdir, group: args.group, eventId: existingId });
        const resolution = resolveDelivery({ readback, envelope, body, consumerSlugs: manifestLoaded.manifest.routes.map((route) => route.workspaceSlug ?? route.role), entry, outboxDir: args.outbox });
        emit(resolution.result, resolution.code);
        return;
      }
    }
    // Nothing found under this key: the earlier attempt can be
    // re-attempted (its outbox record shows what happened).
    entry.states.push({ state: "reconciled", outcome: "not-found" });
    saveOutbox(args.outbox, outbox);
  }

  // Recipient preflight BEFORE create: the workspace's actual
  // dependents of the source must be exactly the reviewed consumer
  // slugs — an unreviewed linked project would receive the event, and
  // a post-send filter cannot undo that exposure.
  const preflight = readbackAfterSend({ upstream: args.upstream, config: args.config, dbPath: args.db, cwd: workdir, group: args.group, eventId: 0 });
  if (!Array.isArray(preflight.graph?.links)) {
    entry.states.push({ state: "refused", reason: "recipient-preflight-unavailable" });
    saveOutbox(args.outbox, outbox);
    refuse("recipient-preflight-unavailable", { eventKey: envelope.eventKey });
  }
  const linked = new Set(
    preflight.graph.links
      .filter((link) => link.to === SOURCE_SLUG && typeof link.from === "string")
      .map((link) => link.from)
  );
  const consumerSlugs = manifestLoaded.manifest.routes
    .map((route) => route.workspaceSlug ?? route.role)
    .sort();
  const linkedConsumers = consumerSlugs.filter((slug) => linked.has(slug));
  const unexpected = [...linked].filter((slug) => !consumerSlugs.includes(slug)).sort();
  if (unexpected.length > 0) {
    // Review the new link and extend the manifest before sending; the
    // event would reach an unreviewed recipient.
    entry.states.push({ state: "refused", reason: "unreviewed-recipients-present" });
    saveOutbox(args.outbox, outbox);
    refuse("unreviewed-recipients-present", {
      eventKey: envelope.eventKey,
      detail: { unreviewedLinkedConsumers: unexpected.length },
    });
  }
  if (linkedConsumers.length === 0) {
    // No declared consumer is linked: the event would reach nobody.
    // That makes a create a pure fan-out no-op; refuse as misrouting.
    entry.states.push({ state: "refused", reason: "no-declared-consumer-linked" });
    saveOutbox(args.outbox, outbox);
    refuse("no-declared-consumer-linked", { eventKey: envelope.eventKey });
  }

  entry.states.push({ state: "sending" });
  saveOutbox(args.outbox, outbox);

  const send = sendUpstreamEvent({ upstream: args.upstream, config: args.config, dbPath: args.db, cwd: workdir, body });
  if (send.kind !== "ok") {
    // Timeout/crash/nonzero after invocation: unknown delivery; the
    // next invocation reconciles by key before re-sending.
    entry.states.push({ state: "unknown-delivery", reason: send.kind === "nonzero" ? "upstream-nonzero" : `upstream-${send.code ?? "failed"}` });
    saveOutbox(args.outbox, outbox);
    emit({ ok: false, hook: "ai-workspace-hook", state: "unknown-delivery", eventKey: envelope.eventKey, reason: send.kind === "nonzero" ? "upstream-nonzero" : "upstream-spawn-failed" }, UNKNOWN_DELIVERY);
    return;
  }

  // Upstream prints "Created event '<title>' (id=N)" with no --json:
  // the id is installation-private routing data used only for the
  // readback below.
  const eventId = Number((send.stdout.match(/\(id=(\d+)\)/) ?? [])[1]);
  if (!Number.isInteger(eventId)) {
    entry.states.push({ state: "unknown-delivery", reason: "upstream-receipt-unparseable" });
    saveOutbox(args.outbox, outbox);
    emit({ ok: false, hook: "ai-workspace-hook", state: "unknown-delivery", eventKey: envelope.eventKey, reason: "upstream-receipt-unparseable" }, UNKNOWN_DELIVERY);
    return;
  }

  const readback = readbackAfterSend({ upstream: args.upstream, config: args.config, dbPath: args.db, cwd: workdir, group: args.group, eventId });
  const resolution = resolveDelivery({ readback, envelope, body, consumerSlugs, entry, outboxDir: args.outbox });
  emit(resolution.result, resolution.code);
};

// Shared delivery resolution: honest verification against the actual
// linked set, with missing declared routes surfaced explicitly.
const resolveDelivery = ({ readback, envelope, body, consumerSlugs, entry, outboxDir }) => {
  const save = () => saveOutbox(outboxDir, entry ? { version: OUTBOX_VERSION, entries: [entry] } : null);
  if (!readback || !readback.details) {
    const reason = readback?.error ?? "readback-unavailable";
    if (entry) {
      entry.states.push({ state: "unknown-delivery", reason });
      saveOutbox(outboxDir, reconstructOutbox(entry, outboxDir));
    }
    return { result: { ok: false, hook: "ai-workspace-hook", state: "unknown-delivery", eventKey: envelope.eventKey, reason }, code: UNKNOWN_DELIVERY };
  }
  const links = Array.isArray(readback.graph?.links) ? readback.graph.links : [];
  const linked = new Set(
    links.filter((link) => link.to === SOURCE_SLUG && typeof link.from === "string").map((link) => link.from)
  );
  const expectedConsumerSlugs = consumerSlugs.filter((slug) => linked.has(slug)).sort();
  const missingDeclared = consumerSlugs.filter((slug) => !linked.has(slug));
  if (!verifyDelivery(readback.details, { body, expectedConsumerSlugs })) {
    // A partial or divergent snapshot needs repair, not a resend.
    if (entry) {
      entry.states.push({ state: "unknown-delivery", reason: "readback-targets-mismatch" });
      saveOutbox(outboxDir, reconstructOutbox(entry, outboxDir));
    }
    return { result: { ok: false, hook: "ai-workspace-hook", state: "unknown-delivery", eventKey: envelope.eventKey, reason: "readback-targets-mismatch" }, code: UNKNOWN_DELIVERY };
  }
  const verifiedRoles = envelope.impact.affectedRoles
    .filter((role) => expectedConsumerSlugs.includes(role.role))
    .map((role) => role.role);
  if (entry) {
    entry.states.push({
      state: "delivered",
      override: "explicit-unadmitted",
      verifiedTargets: expectedConsumerSlugs.length,
      unverifiedDeclaredRoutes: missingDeclared.length,
    });
    saveOutbox(outboxDir, reconstructOutbox(entry, outboxDir));
  }
  return {
    result: {
      ok: true,
      hook: "ai-workspace-hook",
      state: "delivered",
      eventKey: envelope.eventKey,
      eventKind: envelope.eventKind,
      affectedRoles: verifiedRoles,
      unverifiedDeclaredRoutes: missingDeclared.length,
      policyAdmitted: false,
    },
    code: 0,
  };
};

// Outbox persistence for the resolution path: reload, patch the entry
// by key, save (single-writer discipline; the file may have grown).
const reconstructOutbox = (entry, outboxDir) => {
  const outbox = loadOutbox(outboxDir) ?? { version: OUTBOX_VERSION, entries: [] };
  const index = outbox.entries.findIndex((candidate) => candidate.eventKey === entry.eventKey);
  if (index >= 0) outbox.entries[index] = entry;
  else outbox.entries.push(entry);
  return outbox;
};

main();
