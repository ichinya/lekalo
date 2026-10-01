#!/usr/bin/env node
/**
 * Issue #37 lekalo-side AI Workspace change/export hook: the opt-in
 * adapter between public Lekalo contract changes and an external,
 * separately installed AI Workspace instance. Dependency-free; run
 * from the core checkout root:
 *
 *   node scripts/ai-workspace-hook.mjs --base <ref> [--head <ref>]
 *     [--manifest <path>] [--send --outbox <dir> --db <path>
 *      [--upstream <path|name>] [--config <path>] [--group <name>]
 *      [--allow-unadmitted-send] [--quiet]
 *
 * Phases (each closes before the next opens):
 *   plan — resolve base/head to full commit ids, diff the reviewed
 *     routing manifest's approved public paths over committed bytes,
 *     build the envelope (typed role explanations), derive the event
 *     key. Writes nothing.
 *   send — only with --send --outbox --db: record the intended key in
 *     the single-writer outbox BEFORE invoking upstream, spawn the
 *     upstream `event create` as an argv array (never a shell), then
 *     reconcile by readback over the group-scoped MCP tools before
 *     recording `delivered`. Without --allow-unadmitted-send the send
 *     closes as `refused/policy-not-admitted`: the accepted authority
 *     matrix admits no workspace change-event kind yet, so production
 *     emission stays off. The flag is the operator's explicit,
 *     reviewed override for a designated installation (the gates use
 *     it against an isolated temporary database).
 *
 * Exit codes: 0 planned/delivered/disabled/unavailable/no-op,
 * 2 usage, 3 refused, 4 unknown-delivery.
 *
 * Closed result states: planned | delivered | unknown-delivery |
 * refused | disabled | unavailable.
 *
 * Privacy contract of the printed result: public source revisions,
 * approved public paths, contract identities/digests, neutral role
 * aliases, typed explanations, and closed policy references only. No
 * absolute paths, no private checkout names, no upstream numeric ids,
 * no timestamps, no upstream stdout/stderr text (only byte counts).
 * Upstream output is captured privately and discarded; upstream
 * failure text becomes a closed reason code.
 *
 * The hook never reads or writes the upstream SQLite database
 * directly (all readback goes through the MCP tool surface with the
 * widening flags forced off in the child environment) and never
 * mutates any Lekalo behavior: no Lekalo Rust code path knows this
 * script exists.
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
const BODY_LIMIT_BYTES = 64 * 1024;
const MAX_ARTIFACTS = 64;
const MAX_ROLES = 64;
const MAX_CHAINS = 128;
const UPSTREAM_TIMEOUT_MS = 60_000;
const READBACK_TIMEOUT_MS = 30_000;
const GIT_TIMEOUT_MS = 30_000;

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
      windowsVerbatimArguments: useShell ? false : undefined,
      shell: useShell,
    });
  } catch (error) {
    return { kind: "throw", code: error?.code ?? "spawn-throw", stdout: "", stderr: "", status: null };
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
// routing manifest (reviewed operator input; the only share/role source)
// ---------------------------------------------------------------------------

const loadManifest = (manifestArg) => {
  const manifestPath = manifestArg ? resolve(REPO_ROOT, manifestArg) : join(REPO_ROOT, "tests/fixtures/ai-workspace/routing-manifest.json");
  if (!existsSync(manifestPath)) return { error: "manifest-missing" };
  let parsed;
  try {
    parsed = JSON.parse(readFileSync(manifestPath, "utf8"));
  } catch {
    return { error: "manifest-malformed" };
  }
  if (parsed.manifestKind !== "lekalo-ai-workspace-routing") return { error: "manifest-not-routing" };
  if (!Array.isArray(parsed.approvedPaths) || parsed.approvedPaths.length === 0) return { error: "manifest-no-paths" };
  if (!Array.isArray(parsed.routes)) return { error: "manifest-no-routes" };
  for (const route of parsed.routes) {
    if (!route || !/^[a-z][a-z0-9-]{2,63}$/.test(route.role)) return { error: "manifest-role-grammar" };
  }
  const digest = sha256Ref(readFileSync(manifestPath));
  return { manifest: parsed, digest };
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
        detail: `${entry.path} ${entry.change}d`,
        sourcePath: entry.path,
      });
    }
    explanation.push({
      origin: route.origin === "declared-artifact-dependency" ? "declared-artifact-dependency" : "declared-service-link",
      detail: `${route.role} subscribes to lekalo-core protocol/schema changes`,
      sourceRole: "lekalo-core",
      dependencyKind: route.dependencyKind ?? "depends_on",
      reaction: route.reaction ?? "inspect",
    });
    affectedRoles.push({ role: route.role, explanation });
  }
  return { affectedRoles };
};

const readPolicyRefs = () => {
  try {
    const authorityBytes = readFileSync(join(REPO_ROOT, "contracts/authority-matrix.v0.3.2.json"));
    const authority = JSON.parse(authorityBytes.toString("utf8"));
    const policyBytes = readFileSync(join(REPO_ROOT, "contracts/privacy-policy.v0.3.2.json"));
    return {
      authorityRef: { contractId: authority.contractId, version: authority.version, digest: sha256Ref(authorityBytes) },
      privacyPolicyRef: {
        contractId: "dev.lekalo.privacy-export-policy",
        version: "0.3.2",
        digest: sha256Ref(policyBytes),
      },
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
  const body = [
    `eventKey=${envelope.eventKey}`,
    `eventKind=${envelope.eventKind}`,
    `source=lekalo-core ${envelope.source.base.slice(0, 12)}..${envelope.source.head.slice(0, 12)}`,
    `artifacts=${envelope.artifacts.map((entry) => `${entry.change}:${entry.path}`).join("; ")}`,
    `affectedRoles=${envelope.impact.affectedRoles.map((role) => role.role).join(",")}`,
    `complete=${envelope.impact.complete}`,
    `schema=${ENVELOPE_IDENTITY}`,
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
    "--source", "lekalo-core",
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
// this process inherited. Returns the parsed details document or a
// closed error reason.
const readbackEventDetails = ({ upstream, config, dbPath, cwd, group, eventId }) => {
  const argv = [upstream];
  if (config) argv.push("--config", config);
  argv.push("serve", "--group", group);
  const request = {
    jsonrpc: "2.0",
    id: 1,
    method: "tools/call",
    params: { name: "workspace_event_details", arguments: { event_id: eventId } },
  };
  const result = runProcess(argv[0], argv.slice(1), {
    cwd,
    timeoutMs: READBACK_TIMEOUT_MS,
    input: `${JSON.stringify(request)}\n`,
    env: {
      ...process.env,
      AI_WORKSPACE_DB: dbPath,
      AI_WORKSPACE_ALLOW_PROJECT_WIDE_TOOLS: "0",
      AI_WORKSPACE_ALLOW_PROJECT_FILE_WRITE: "0",
    },
  });
  if (result.kind === "error" || result.kind === "throw") return { error: "readback-unavailable" };
  const responseLine = result.stdout
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
    .find((message) => message && message.id === 1);
  if (!responseLine) return { error: "readback-no-response" };
  if (responseLine.error) return { error: "readback-denied" };
  const text = responseLine.result?.content?.find((entry) => entry.type === "text")?.text;
  if (!text) return { error: "readback-empty" };
  try {
    return { details: JSON.parse(text) };
  } catch {
    return { error: "readback-malformed" };
  }
};

// Delivery proof: kind, title, and body must match the projection and
// every declared consumer slug must appear as a direct linked-service
// target. Public inputs only; the numeric id and slugs stay private.
const verifyDelivery = (details, { body, consumerSlugs }) => {
  if (!details || details.event?.kind !== "service_changed") return false;
  if (details.event?.title !== UPSTREAM_TITLE || details.event?.body !== body) return false;
  const delivered = new Set(
    (details.affected_services ?? [])
      .filter((target) => target.relation_kind === "linked_service")
      .map((target) => target.project)
      .filter(Boolean)
  );
  return consumerSlugs.every((slug) => delivered.has(slug));
};

// ---------------------------------------------------------------------------
// outbox (single-writer, private local state)
// ---------------------------------------------------------------------------

const OUTBOX_VERSION = 1;

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

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

const USAGE = 2;
const REFUSED = 3;
const UNKNOWN_DELIVERY = 4;

const main = () => {
  const args = {
    base: null, head: null, manifest: null, send: false, outbox: null,
    upstream: "ai-workspace", config: null, db: null, group: DEFAULT_GROUP,
    allowUnadmitted: false, quiet: false,
  };
  const bad = (message) => {
    process.stderr.write(`${JSON.stringify({ ok: false, hook: "ai-workspace-hook", state: "refused", reason: "usage", detail: message })}\n`);
    process.exit(USAGE);
  };
  const argv = process.argv.slice(2);
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const value = () => {
      if (argv[index + 1] === undefined) bad(`missing value for ${arg}`);
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
    else bad(`unknown argument ${arg}`);
  }
  if (!args.base) bad("--base is required");
  const emit = (result, code = 0) => {
    if (!args.quiet) process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
    if (code !== 0) process.exit(code);
  };
  const refuse = (reason, extra = {}) =>
    emit({ ok: false, hook: "ai-workspace-hook", state: "refused", reason, ...extra }, REFUSED);

  // Optionality: without a manifest the integration is off, cleanly.
  if (!args.manifest && !args.send) {
    emit({ ok: true, hook: "ai-workspace-hook", state: "disabled", reason: "no-manifest-configured" });
    return;
  }
  if (args.send && (!args.outbox || !args.db)) {
    bad("--send requires --outbox and --db");
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
  const impact = buildImpact(manifestLoaded.manifest, artifacts);
  if (impact.error) refuse(impact.error);
  const policyRefs = readPolicyRefs();
  if (!policyRefs) refuse("policy-refs-unreadable");

  const envelope = {
    schemaVersion: ENVELOPE_SCHEMA_VERSION,
    identity: ENVELOPE_IDENTITY,
    eventKind: artifacts.some((entry) => isProtocolPath(entry.path)) ? "protocol-change" : "schema-change",
    source: {
      producer: "lekalo-core",
      base,
      head,
      clean: true,
      producerVersion: version,
      hookVersion: HOOK_VERSION,
    },
    manifest: {
      manifestVersion: manifestLoaded.manifest.manifestVersion ?? "0.0.0",
      digest: manifestLoaded.digest,
      routes: manifestLoaded.manifest.routes.length,
    },
    artifacts,
    impact: {
      affectedRoles: impact.affectedRoles,
      complete: "declared-only",
      limitations: [
        "direct-links-only",
        "no-transitive-traversal",
        "no-schema-version-filter",
        "compatibility-unknown",
        "upstream-receipt-unavailable",
      ],
    },
    policy: {
      sendAdmitted: false,
      refusalReason: "policy-not-admitted",
      ...policyRefs,
    },
  };
  envelope.eventKey = deriveEventKey(envelope);

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

  mkdirSync(args.outbox, { recursive: true });
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
  entry.states.push({ state: "planned" });
  saveOutbox(args.outbox, outbox);

  if (!args.allowUnadmitted) {
    entry.states.push({ state: "refused", reason: "policy-not-admitted" });
    saveOutbox(args.outbox, outbox);
    refuse("policy-not-admitted", { eventKey: envelope.eventKey, envelope, outbox: "recorded" });
  }

  // Upstream availability probe (non-destructive). The probe and the
  // send share one execution path: a binary that cannot even be
  // probed cannot be sent through.
  const probe = runProcess(args.upstream, ["--version"], { timeoutMs: 15_000 });
  if (probe.kind === "error" || probe.kind === "throw") {
    entry.states.push({ state: "unavailable" });
    saveOutbox(args.outbox, outbox);
    emit({ ok: true, hook: "ai-workspace-hook", state: "unavailable", reason: "upstream-binary-missing", eventKey: envelope.eventKey });
    return;
  }

  const body = buildUpstreamBody(envelope);
  if (!body) refuse("body-limit-exceeded", { eventKey: envelope.eventKey });

  // The consumer slugs live only in the routing manifest and the
  // operator's workspace; they are routing data, printed nowhere.
  const consumerSlugs = manifestLoaded.manifest.routes
    .map((route) => route.workspaceSlug ?? route.role)
    .sort();
  const workdir = process.cwd();

  const send = sendUpstreamEvent({ upstream: args.upstream, config: args.config, dbPath: args.db, cwd: workdir, body });
  if (send.kind !== "ok") {
    // Timeout/crash/nonzero after invocation: unknown delivery, never
    // a blind retry and never success.
    entry.states.push({ state: "unknown-delivery", reason: send.kind === "nonzero" ? "upstream-nonzero" : `upstream-${send.code ?? "failed"}` });
    saveOutbox(args.outbox, outbox);
    emit({ ok: false, hook: "ai-workspace-hook", state: "unknown-delivery", eventKey: envelope.eventKey }, UNKNOWN_DELIVERY);
    return;
  }

  // Upstream prints "Created event '<title>' (id=N)" with no --json:
  // the id is installation-private routing data used only for the
  // readback below.
  const eventId = Number((send.stdout.match(/\(id=(\d+)\)/) ?? [])[1]);
  if (!Number.isInteger(eventId)) {
    entry.states.push({ state: "unknown-delivery", reason: "upstream-receipt-unparseable" });
    saveOutbox(args.outbox, outbox);
    emit({ ok: false, hook: "ai-workspace-hook", state: "unknown-delivery", eventKey: envelope.eventKey }, UNKNOWN_DELIVERY);
    return;
  }

  const readback = readbackEventDetails({ upstream: args.upstream, config: args.config, dbPath: args.db, cwd: workdir, group: args.group, eventId });
  if (!readback.details) {
    entry.states.push({ state: "unknown-delivery", reason: readback.error });
    saveOutbox(args.outbox, outbox);
    emit({ ok: false, hook: "ai-workspace-hook", state: "unknown-delivery", eventKey: envelope.eventKey }, UNKNOWN_DELIVERY);
    return;
  }
  if (!verifyDelivery(readback.details, { body, consumerSlugs })) {
    // A partial target snapshot needs repair, not a resend.
    entry.states.push({ state: "unknown-delivery", reason: "readback-targets-mismatch" });
    saveOutbox(args.outbox, outbox);
    emit({ ok: false, hook: "ai-workspace-hook", state: "unknown-delivery", eventKey: envelope.eventKey }, UNKNOWN_DELIVERY);
    return;
  }

  entry.states.push({ state: "delivered", override: "explicit-unadmitted", verifiedTargets: consumerSlugs.length });
  saveOutbox(args.outbox, outbox);
  emit({
    ok: true,
    hook: "ai-workspace-hook",
    state: "delivered",
    eventKey: envelope.eventKey,
    eventKind: envelope.eventKind,
    affectedRoles: envelope.impact.affectedRoles.map((role) => role.role),
    policyAdmitted: false,
  });
};

main();
