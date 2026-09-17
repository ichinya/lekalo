/**
 * Pure native gate plan construction for issue #48 (plan §4-§5).
 *
 * `buildNativePlan` joins the workspace inventory, the changed inputs,
 * the execution policy confirmations, and the per-package tsconfig
 * evidence into one immutable proposed plan. It is a pure function of
 * its inputs: no clock, no environment, no absolute paths, no random-
 * ness, no I/O. The plan is a proposal only — it can never authorize
 * its own execution, and the approval always lives outside the plan.
 *
 * The canonical digest is SHA-256 over `domain || canonical(plan)`
 * where the canonical form is UTF-8 JSON with recursively bytewise-
 * sorted keys and compact separators, `plan_digest` absent. Shared
 * golden byte vectors with the Rust core pin the exact encoding.
 */
import { createHash } from "node:crypto";

export const PLAN_DIGEST_DOMAIN = "lekalo.native-plan.v0.3.2";
/** The planner capability id declared in every produced plan. */
export const PLAN_CAPABILITY = "plan.native-gates";
export const CANONICALIZATION_VERSION = "0.3.2";
/** The planner version recorded in every produced plan. */
export const PLANNER_VERSION = "0.3.2";

/** Shell metacharacters and interpolation syntax refused in literals. */
const SHELL_METACHARACTERS = new Set([
  "|", "&", ";", "<", ">", "(", ")", "$", "`", "\\", "\"", "'",
  "\n", "\r", "\t",
]);

/** Assignment-prefix / env-injection prefixes and shell executables. */
const FORBIDDEN_ARG_PREFIXES = ["--eval", "-e", "--require", "-r", "--import", "--run="];
const FORBIDDEN_ARG_EXACT = new Set(["-e", "-r", "-p", "-i", "--eval", "--require", "--import"]);
const FORBIDDEN_ARG_SUFFIXES = [".cmd", ".bat", ".ps1", ".sh", ".exe"];
const FORBIDDEN_ARG_VALUES = new Set([
  "sh", "bash", "cmd", "cmd.exe", "powershell", "powershell.exe",
  "pwsh", "pwsh.exe", "pnpm", "pnpm.cmd", "npm", "npm.cmd",
  "yarn", "yarn.cmd", "bun", "bun.exe", "npx", "npx.cmd",
  "corepack", "corepack.cmd", "yarn-pnp.cjs", ".pnp.cjs",
]);

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function utf8Compare(left, right) {
  const a = Buffer.from(left, "utf8");
  const b = Buffer.from(right, "utf8");
  const length = Math.min(a.length, b.length);
  for (let index = 0; index < length; index += 1) {
    if (a[index] !== b[index]) return a[index] - b[index];
  }
  return a.length - b.length;
}

export class PlanRefusal extends Error {
  constructor(code, message) {
    super(message ?? code);
    this.name = "PlanRefusal";
    this.code = code;
  }
}

/**
 * Canonical JSON text: recursively bytewise key-sorted, compact, UTF-8,
 * finite integers only. Mirrors the Rust core byte for byte.
 */
export function canonicalJsonText(value) {
  if (value === null) return "null";
  switch (typeof value) {
    case "boolean":
      return value ? "true" : "false";
    case "number":
      if (!Number.isFinite(value)) {
        throw new PlanRefusal("digest-nonfinite", "non-finite numbers cannot be canonicalized");
      }
      return Number.isInteger(value) && Math.abs(value) < 1e15
        ? String(value)
        : JSON.stringify(value);
    case "string":
      return JSON.stringify(value);
    case "object":
      break;
    default:
      throw new PlanRefusal("digest-unserializable", "the value cannot be canonicalized");
  }
  if (Array.isArray(value)) {
    return `[${value.map(canonicalJsonText).join(",")}]`;
  }
  const keys = Object.keys(value).sort(utf8Compare);
  return `{${keys.map((key) => `${JSON.stringify(key)}:${canonicalJsonText(value[key])}`).join(",")}}`;
}

/**
 * The plan digest: sha256 over `domain || canonical(plan without
 * plan_digest)`. Deterministic across Node and Rust (golden vectors).
 */
export function planDigest(plan) {
  const { plan_digest: _omitted, ...rest } = plan;
  const bytes = Buffer.concat([
    Buffer.from(PLAN_DIGEST_DOMAIN, "utf8"),
    Buffer.from(canonicalJsonText(rest), "utf8"),
  ]);
  return "sha256:" + createHash("sha256").update(bytes).digest("hex");
}

/**
 * Whether one string is a safe literal argv element: no shell
 * metacharacters, no interpolation, no command substitution. The check
 * is on the complete string, never a prefix.
 */
export function isSafeLiteral(text) {
  if (typeof text !== "string" || text.length === 0 || text.length > 1024) return false;
  for (const character of text) {
    if (SHELL_METACHARACTERS.has(character)) return false;
  }
  return true;
}

/**
 * Parse one confirmed script string into the exact argv vector and
 * compare it against the policy's recorded argv. The parser accepts
 * only the narrow shell-free literal form: single spaces between
 * elements, no quotes/interpolation/redirection/metacharacters, no
 * assignment prefixes. It compares the FULL string against the
 * confirmed argv — never a known prefix — and refuses anything that
 * would require a shell instead of approximating it.
 */
export function parseConfirmedScript(scriptText, confirmedArgv) {
  if (typeof scriptText !== "string" || scriptText.length === 0 || scriptText.length > 4096) {
    return { ok: false, reason: "script-empty-or-oversize" };
  }
  if (!isSafeLiteral(scriptText) || /\s{2,}/.test(scriptText)) {
    return { ok: false, reason: "script-shell-syntax" };
  }
  if (/^[A-Za-z_][A-Za-z0-9_]*=/.test(scriptText)) {
    return { ok: false, reason: "script-assignment-prefix" };
  }
  const parsed = scriptText.split(" ");
  for (const element of parsed) {
    if (!isSafeLiteral(element)) return { ok: false, reason: "script-unsafe-element" };
    if (FORBIDDEN_ARG_EXACT.has(element)) return { ok: false, reason: "script-node-flag" };
    for (const prefix of FORBIDDEN_ARG_PREFIXES) {
      if (element === prefix || element.startsWith(prefix)) {
        return { ok: false, reason: "script-node-flag" };
      }
    }
    for (const suffix of FORBIDDEN_ARG_SUFFIXES) {
      if (element.toLowerCase().endsWith(suffix)) return { ok: false, reason: "script-shell-executable" };
    }
    if (FORBIDDEN_ARG_VALUES.has(element.toLowerCase())) {
      return { ok: false, reason: "script-package-manager-or-shell" };
    }
  }
  if (confirmedArgv === undefined) return { ok: true, argv: parsed, matched: null };
  if (!Array.isArray(confirmedArgv) || confirmedArgv.length !== parsed.length) {
    return { ok: false, reason: "script-argv-mismatch", argv: parsed };
  }
  for (let index = 0; index < parsed.length; index += 1) {
    if (parsed[index] !== confirmedArgv[index]) {
      return { ok: false, reason: "script-argv-mismatch", argv: parsed };
    }
  }
  return { ok: true, argv: parsed, matched: true };
}

/**
 * Affected closure: starting packages plus the reverse transitive
 * dependency closure, plus graph-bound test packages. Deterministic
 * sorted traversal with a visited set; bounded explanation paths.
 */
export function computeAffectedClosure(inventory, changedRoots) {
  const changed = new Set(changedRoots);
  const consumers = new Map();
  const prerequisites = new Map();
  for (const edge of inventory.edges) {
    if (!consumers.has(edge.to)) consumers.set(edge.to, []);
    consumers.get(edge.to).push(edge.from);
    if (!prerequisites.has(edge.from)) prerequisites.set(edge.from, []);
    prerequisites.get(edge.from).push(edge.to);
  }
  const affected = new Map();
  const visit = (packageId, kind, sourceRef, path) => {
    if (path.length > 32) return; // bounded explanation paths
    const existing = affected.get(packageId);
    const reason = { kind, source_ref: sourceRef, edge_path: [...path] };
    if (existing) {
      if (!existing.reasons.some((candidate) =>
        candidate.kind === kind && candidate.source_ref === sourceRef)) {
        existing.reasons.push(reason);
        existing.reasons.sort((left, right) =>
          utf8Compare(left.kind, right.kind) || utf8Compare(left.source_ref, right.source_ref));
      } else {
        return;
      }
    } else {
      affected.set(packageId, { package_id: packageId, reasons: [reason] });
    }
    for (const consumer of consumers.get(packageId) ?? []) {
      // Reverse transitive closure: every dependent of an affected
      // package is affected in turn (skipping the change reason itself).
      const nextKind = kind === "changed-package" ? "dependent-closure" : kind;
      visit(consumer, nextKind, kind === "changed-package" ? packageId : sourceRef, [...path, packageId]);
    }
  };
  for (const changedId of [...changed].sort(utf8Compare)) {
    visit(changedId, "changed-package", changedId, []);
  }
  // Graph-bound tests: a test-kind package depends-on a selected build
  // prerequisite only via explicit edges; the closure above already
  // carries them. Build prerequisites ride forward edges separately.
  const selected = new Set(affected.keys());
  const prerequisiteAdds = [];
  for (const packageId of [...selected].sort(utf8Compare)) {
    for (const prerequisite of prerequisites.get(packageId) ?? []) {
      if (!selected.has(prerequisite)) {
        affected.set(prerequisite, {
          package_id: prerequisite,
          reasons: [{ kind: "build-prerequisite", source_ref: packageId, edge_path: [prerequisite, packageId] }],
        });
        prerequisiteAdds.push(prerequisite);
      }
    }
  }
  void prerequisiteAdds;
  return [...affected.values()].sort((left, right) => utf8Compare(left.package_id, right.package_id));
}

/**
 * Build the proposed native gate plan. Inputs are already validated:
 * inventory from `workspace.mjs`, `changes` bounded, `policy` the
 * checked-in execution policy document, `toolCatalog` the trusted
 * catalog entries, `profileRef`/digests from the caller's custody.
 * Missing confirmations exclude a package with a stable reason — they
 * never invent commands.
 */
export function buildNativePlan({
  inventory,
  changes,
  policy,
  toolCatalog,
  profileRef,
  profileDigest,
  adapterIdentity,
  scanRef,
  observedRef,
  inputManifestDigest,
  capabilitySnapshotDigest,
  toolCatalogDigest,
}) {
  if (!isObject(inventory) || !Array.isArray(inventory.packages)) {
    throw new PlanRefusal("plan-inventory-invalid", "the workspace inventory is missing");
  }
  if (!isObject(policy) || !Array.isArray(policy.confirmations)) {
    throw new PlanRefusal("plan-policy-invalid", "the execution policy is missing or malformed");
  }
  const changedRoots = new Set();
  for (const file of changes?.files ?? []) {
    // Attribute the change to the deepest package whose root is a path
    // prefix of the changed file; root-level files attribute to the
    // root package id when one exists, else stay unattributed.
    let best = null;
    for (const pkg of inventory.packages) {
      if (pkg.root === ".") {
        if (best === null) best = pkg.id;
        continue;
      }
      if (file.path === pkg.root || file.path.startsWith(`${pkg.root}/`)) {
        if (best === null || pkg.root.length > (inventory.packages.find((c) => c.id === best)?.root?.length ?? 0)) {
          best = pkg.id;
        }
      }
    }
    if (best !== null) changedRoots.add(best);
  }
  // Changed symbols drive selection too: a symbol entry of the form
  // "<package_id>#<symbol>" attributes directly to its package; a
  // bare symbol (no package attribution available in M3) is recorded
  // as an uncertainty rather than silently ignored.
  for (const symbol of changes?.symbols ?? []) {
    const hashIndex = symbol.indexOf("#");
    if (hashIndex > 0) {
      const packageId = symbol.slice(0, hashIndex);
      if (inventory.packages.some((pkg) => pkg.id === packageId)) {
        changedRoots.add(packageId);
        continue;
      }
    }
    inventory.uncertainties.push({
      kind: "unknown",
      detail: `changed symbol without package attribution: ${symbol}`.slice(0, 256),
    });
  }
  const confirmationByPackage = new Map();
  for (const confirmation of policy.confirmations) {
    confirmationByPackage.set(confirmation.package_id, confirmation);
  }
  const toolById = new Map();
  for (const tool of toolCatalog ?? []) toolById.set(tool.id, tool);
  const affectedList = computeAffectedClosure(inventory, changedRoots);
  const commands = [];
  const excluded = [];
  const affectedReasonRefs = new Map();
  const sortedAffected = [...affectedList].sort((left, right) => utf8Compare(left.package_id, right.package_id));
  for (const affected of sortedAffected) {
    affectedReasonRefs.set(affected.package_id, `${affected.package_id}#${affected.reasons[0].kind}`);
  }
  for (const affected of sortedAffected) {
    const pkg = inventory.packages.find((candidate) => candidate.id === affected.package_id);
    const confirmation = confirmationByPackage.get(affected.package_id);
    if (!confirmation) {
      excluded.push({ package_id: affected.package_id, reason: "no-confirmation" });
      continue;
    }
    const tool = toolById.get(confirmation.tool_ref.id);
    if (!tool) {
      excluded.push({ package_id: affected.package_id, reason: "no-confirmation" });
      continue;
    }
    if (confirmation.gate && !policy.allowed_gate_kinds.includes(confirmation.gate)) {
      excluded.push({ package_id: affected.package_id, reason: "no-confirmation" });
      continue;
    }
    const scriptName = confirmation.script_name;
    const scriptDigest = confirmation.script_digest;
    const commandId = `gate-${affected.package_id.replace(/[^a-z0-9._-]/g, "_")}-${confirmation.gate}`;
    const cwd = pkg ? pkg.root : ".";
    commands.push({
      id: commandId,
      package_id: affected.package_id,
      gate: confirmation.gate,
      script_name: scriptName,
      script_digest: scriptDigest,
      confirmation_ref: confirmation.rule_digest,
      cwd,
      tool_ref: confirmation.tool_ref.id,
      argv: confirmation.argv,
      env: confirmation.env ?? [],
      depends_on: [],
      affected_reason_refs: [affectedReasonRefs.get(affected.package_id)],
      read_manifest_ref: inputManifestDigest,
      allowed_writes: { mode: "stage-only" },
      limits: policy.limits,
      tsconfig_ref: confirmation.tsconfig_ref ?? (pkg ? `${pkg.root}/tsconfig.json` : "tsconfig.json"),
    });
  }
  for (const pkg of inventory.packages) {
    if (!affectedList.some((affected) => affected.package_id === pkg.id)
      && !excluded.some((entry) => entry.package_id === pkg.id)) {
      excluded.push({ package_id: pkg.id, reason: "no-reason" });
    }
  }
  excluded.sort((left, right) => utf8Compare(left.package_id, right.package_id));
  // Selection mode derives from the checked-in policy's fallback rule:
  // targeted unless an explicit release-full rule with a recorded rule
  // digest exists. Caller input is never trusted.
  const derivedSelectionMode =
    isObject(policy.fallback_rule) && policy.fallback_rule.mode === "release-full"
      ? "release-full"
      : "targeted";
  const fallbackRuleRef =
    isObject(policy.fallback_rule) && typeof policy.fallback_rule.rule_digest === "string"
      ? policy.fallback_rule.rule_digest
      : undefined;
  // Build-prerequisite ordering: commands of a selected package depend
  // on the commands of every selected package it consumes.
  const forwardPrerequisites = new Map();
  for (const edge of inventory.edges) {
    if (!forwardPrerequisites.has(edge.from)) forwardPrerequisites.set(edge.from, []);
    forwardPrerequisites.get(edge.from).push(edge.to);
  }
  const selectedIds = new Set(sortedAffected.map((entry) => entry.package_id));
  const dependsOnByPackage = new Map();
  for (const packageId of selectedIds) {
    const chain = [];
    for (const prerequisite of forwardPrerequisites.get(packageId) ?? []) {
      if (!selectedIds.has(prerequisite)) continue;
      chain.push(prerequisite);
      for (const nested of dependsOnByPackage.get(prerequisite) ?? []) {
        chain.push(nested);
      }
    }
    if (chain.length > 0) dependsOnByPackage.set(packageId, [...new Set(chain)]);
  }
  for (const [packageId, dependencies] of dependsOnByPackage) {
    for (const command of commands) {
      if (command.package_id === packageId) {
        command.depends_on = [...dependencies].sort(utf8Compare);
      }
    }
  }
  const plan = {
    schema_version: "lekalo/native-gate-plan/v0.3.2",
    kind: "native-plan",
    plan_digest: `sha256:${"0".repeat(64)}`,
    adapter: adapterIdentity,
    planner_version: "0.3.2",
    canonicalization_version: CANONICALIZATION_VERSION,
    repository_role: policy.repository_role,
    trust: policy.trust,
    authority_ref: policy.authority_ref,
    policy_ref: policy.policy_ref,
    classification_ref: policy.classification_ref,
    execution_policy_ref: { ...policy.identity, digest: policy.policy_digest },
    profile_ref: { id: profileRef, digest: profileDigest },
    input_manifest_digest: inputManifestDigest,
    scan_ref: scanRef,
    observed_ref: observedRef,
    tool_catalog_digest: toolCatalogDigest,
    capability_snapshot_digest: capabilitySnapshotDigest,
    workspace: {
      manager: inventory.manager,
      declared_version: "unknown",
      compatibility_path: inventory.compatibilityPath,
      root: inventory.root,
      manifest_digest: inventory.workspaceManifestDigest ?? `sha256:${"0".repeat(64)}`,
      lock_digest_state: inventory.lockDigestState ?? "absent",
      packages: inventory.packages.map((pkg) => ({
        id: pkg.id,
        name: pkg.name,
        root: pkg.root,
        manifest_digest: pkg.manifestDigest,
      })),
      edges: inventory.edges,
      completeness: inventory.completeness,
      uncertainties: inventory.uncertainties,
    },
    changes: changes ?? { files: [], symbols: [] },
    affected: sortedAffected,
    excluded,
    selection_mode: derivedSelectionMode,
    ...(fallbackRuleRef !== undefined ? { fallback_rule_ref: fallbackRuleRef } : {}),
    commands,
    env: policy.env_recipe,
    tools: (toolCatalog ?? []).map((tool) => ({
      id: tool.id,
      name: tool.name,
      version: tool.version ?? "unknown",
      artifact_digest: tool.artifact_digest,
      entry_digest: tool.entry_digest,
      platform: tool.platform,
      provenance: tool.provenance ?? "fixture-catalog",
    })),
    required_capabilities: ["plan.native-gates"],
    capabilities: [{ id: "plan.native-gates", definition_version: PLANNER_VERSION, state: "full", source: "declared" }],
    run_eligibility: {
      state: commands.length > 0 ? "plan-only" : "blocked",
      reason_codes: commands.length > 0 ? ["fixture-runner-not-in-production"] : ["no-commands"],
    },
    limits: policy.limits,
    write_policy: policy.write_policy,
  };
  plan.plan_digest = planDigest(plan);
  return plan;
}
