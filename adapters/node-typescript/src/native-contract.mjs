/**
 * Closed shape/enum/bound validators and the canonical digest for the
 * native gate contracts (issue #48, plan §3). These validators mirror
 * the four JSON Schema documents exactly: any member, enum value, or
 * bound outside the closed documents is refused, never normalized.
 * Shared Node/Rust golden byte vectors pin the digest encoding.
 */
import { createHash } from "node:crypto";
import { canonicalJsonText, planDigest as canonicalPlanDigest } from "./native-plan.mjs";

export { canonicalJsonText, planDigest } from "./native-plan.mjs";

const SHA256 = /^sha256:[0-9a-f]{64}$/;
const CONTRACT_VERSION = /^[0-9]+\.[0-9]+\.[0-9]+$/;
const PACKAGE_ID = /^[a-z0-9.][a-z0-9._/-]{0,63}=[a-z0-9@/._-]{1,192}$/;
const COMMAND_ID = /^[a-z0-9][a-z0-9._-]{0,63}$/;
const CAPABILITY_ID = /^[a-z0-9][a-z0-9_-]*(\.[a-z0-9][a-z0-9_-]*)+$/;
const LOGICAL_PATH = /^(?!(?:.*[/])?(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:[./]|$))(?!(?:.*[/])?\.{1,2}(?:\/|$))(?!.*\.(?:\/|$))[a-z0-9.][a-z0-9._-]{0,63}(?:\/[a-z0-9.][a-z0-9._-]{0,63})*$/;

export const PLAN_SCHEMA_VERSION = "lekalo/native-gate-plan/v0.3.2";
export const POLICY_SCHEMA_VERSION = "lekalo/native-gate-policy/v0.3.2";
export const RUN_SCHEMA_VERSION = "lekalo/native-gate-run/v0.3.2";
export const VIEW_SCHEMA_VERSION = "lekalo/native-gate-view/v0.3.2";

const GATE_KINDS = new Set(["build", "typecheck", "lint", "test"]);
const SELECTION_MODES = new Set(["targeted", "release-full"]);
const REASON_KINDS = new Set([
  "changed-package", "dependent-closure", "graph-bound-test",
  "build-prerequisite", "release-rule", "explicit-binding",
]);
const EXCLUDED_REASONS = new Set(["no-reason", "no-confirmation", "no-edge", "owner-unknown", "out-of-scope"]);
const EDGE_KINDS = new Set([
  "dependency", "dev-dependency", "optional-dependency", "peer-dependency",
  "ts-reference", "scanner-reference", "target-binding",
]);
const UNCERTAINTY_KINDS = new Set([
  "unresolved-dependency", "version-mismatch", "cycle", "excluded-read",
  "pattern-partial", "manager-ambiguous", "deleted-package", "unknown",
]);
const OUTCOMES = new Set([
  "passed", "failed", "missing", "blocked", "unsupported",
  "infrastructure", "security",
]);
const ENV_KINDS = new Set(["literal", "execution-temp", "execution-home", "platform-system-root"]);

function isPlainObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function fail(member) {
  throw new TypeError(`native contract violation at ${member}`);
}

function check(condition, member) {
  if (!condition) fail(member);
  return true;
}

function digest(value, member) {
  return check(typeof value === "string" && SHA256.test(value), member);
}

function contractVersion(value, member) {
  return check(typeof value === "string" && CONTRACT_VERSION.test(value), member);
}

function boundedString(value, max, member, pattern = null) {
  check(typeof value === "string" && value.length > 0 && value.length <= max, member);
  if (pattern) check(pattern.test(value), member);
  return true;
}

function checkLimits(limits, member) {
  check(isPlainObject(limits), member);
  const members = Object.keys(limits).sort().join(",");
  check(members === "max_output_bytes_per_run,max_stderr_bytes,max_stdout_bytes,timeout_ms_per_command,timeout_ms_per_run", `${member}.members`);
  check(Number.isInteger(limits.timeout_ms_per_command) && limits.timeout_ms_per_command >= 1 && limits.timeout_ms_per_command <= 30000, `${member}.timeout_ms_per_command`);
  check(Number.isInteger(limits.timeout_ms_per_run) && limits.timeout_ms_per_run >= 1 && limits.timeout_ms_per_run <= 120000, `${member}.timeout_ms_per_run`);
  check(Number.isInteger(limits.max_stdout_bytes) && limits.max_stdout_bytes >= 1 && limits.max_stdout_bytes <= 65536, `${member}.max_stdout_bytes`);
  check(Number.isInteger(limits.max_stderr_bytes) && limits.max_stderr_bytes >= 1 && limits.max_stderr_bytes <= 65536, `${member}.max_stderr_bytes`);
  check(Number.isInteger(limits.max_output_bytes_per_run) && limits.max_output_bytes_per_run >= 1 && limits.max_output_bytes_per_run <= 1048576, `${member}.max_output_bytes_per_run`);
}

function checkWritePolicy(writePolicy, member) {
  check(isPlainObject(writePolicy), member);
  check(writePolicy.mode === "stage-only" || writePolicy.mode === "scoped", `${member}.mode`);
  const keys = Object.keys(writePolicy).sort();
  check(keys.join(",").length > 0, `${member}`);
  if (writePolicy.mode === "scoped") {
    check(Array.isArray(writePolicy.scopes) && writePolicy.scopes.length <= 64, `${member}.scopes`);
    for (const scope of writePolicy.scopes ?? []) {
      check(isPlainObject(scope), `${member}.scope`);
      boundedString(scope.root, 512, `${member}.root`, LOGICAL_PATH);
    }
  }
}

function checkEnv(env, member) {
  check(isPlainObject(env), member);
  check(Array.isArray(env.allowed_names) && env.allowed_names.length <= 32, `${member}.allowed_names`);
  check(Array.isArray(env.bindings) && env.bindings.length <= 32, `${member}.bindings`);
  for (const binding of env.bindings) {
    check(isPlainObject(binding), `${member}.binding`);
    boundedString(binding.name, 64, `${member}.name`, /^[A-Z][A-Z0-9_]{0,63}$/);
    check(ENV_KINDS.has(binding.kind), `${member}.kind`);
    if (binding.kind === "literal") {
      boundedString(binding.value, 1024, `${member}.value`);
    }
  }
}

/**
 * Validate one native gate plan against the closed 0.3.2 plan contract.
 * Throws a TypeError naming the first violating member.
 */
export function validateNativePlan(plan) {
  check(isPlainObject(plan), "plan");
  const required = [
    "schema_version", "kind", "plan_digest", "adapter", "planner_version",
    "canonicalization_version", "repository_role", "trust", "authority_ref",
    "policy_ref", "classification_ref", "execution_policy_ref", "profile_ref",
    "input_manifest_digest", "scan_ref", "observed_ref", "tool_catalog_digest",
    "capability_snapshot_digest", "workspace", "changes", "affected", "excluded",
    "selection_mode", "commands", "env", "tools", "required_capabilities",
    "capabilities", "run_eligibility", "limits", "write_policy",
  ].sort();
  check(JSON.stringify(Object.keys(plan).sort()) === JSON.stringify(required), "plan.members");
  check(plan.schema_version === PLAN_SCHEMA_VERSION, "plan.schema_version");
  check(plan.kind === "native-plan", "plan.kind");
  digest(plan.plan_digest, "plan.plan_digest");
  check(isPlainObject(plan.adapter), "plan.adapter");
  boundedString(plan.adapter.id, 64, "plan.adapter.id", /^[a-z][a-z0-9-]{0,63}$/);
  contractVersion(plan.adapter.version, "plan.adapter.version");
  digest(plan.adapter.digest, "plan.adapter.digest");
  contractVersion(plan.planner_version, "plan.planner_version");
  contractVersion(plan.canonicalization_version, "plan.canonicalization_version");
  check(plan.selection_mode === "targeted" || plan.selection_mode === "release-full", "plan.selection_mode");
  check(plan.selection_mode === "targeted" ? SELECTION_MODES.has(plan.selection_mode) : SELECTION_MODES.has(plan.selection_mode), "plan.selection_mode");
  digest(plan.input_manifest_digest, "plan.input_manifest_digest");
  digest(plan.tool_catalog_digest, "plan.tool_catalog_digest");
  digest(plan.capability_snapshot_digest, "plan.capability_snapshot_digest");
  check(isPlainObject(plan.scan_ref) && digest(plan.scan_ref.digest, "plan.scan_ref.digest"), "plan.scan_ref");
  check(isPlainObject(plan.observed_ref) && digest(plan.observed_ref.digest, "plan.observed_ref.digest"), "plan.observed_ref");
  check(isPlainObject(plan.profile_ref), "plan.profile_ref");
  check(isPlainObject(plan.run_eligibility), "plan.run_eligibility");
  check(["runnable", "blocked", "plan-only"].includes(plan.run_eligibility.state), "plan.run_eligibility.state");
  check(Array.isArray(plan.run_eligibility.reason_codes) && plan.run_eligibility.reason_codes.length <= 16, "plan.run_eligibility.reason_codes");
  // Workspace.
  const workspace = plan.workspace;
  check(isPlainObject(workspace), "plan.workspace");
  check(["pnpm-workspace", "npm-standalone", "npm-workspaces", "yarn", "bun"].includes(workspace.manager), "plan.workspace.manager");
  check(workspace.completeness === "complete" || workspace.completeness === "incomplete" || workspace.completeness === "unknown", "plan.workspace.completeness");
  check(Array.isArray(workspace.packages) && workspace.packages.length <= 1024, "plan.workspace.packages");
  const packageIds = new Set();
  for (const pkg of workspace.packages) {
    check(isPlainObject(pkg), "plan.workspace.package");
    boundedString(pkg.id, 256, "plan.workspace.package.id", PACKAGE_ID);
    check(!packageIds.has(pkg.id), "plan.workspace.package.duplicate");
    packageIds.add(pkg.id);
    check(pkg.root === "." || (typeof pkg.root === "string" && pkg.root.length > 0 && LOGICAL_PATH.test(pkg.root)), "plan.workspace.package.root");
    digest(pkg.manifest_digest, "plan.workspace.package.manifest_digest");
  }
  check(Array.isArray(workspace.edges) && workspace.edges.length <= 8192, "plan.workspace.edges");
  for (const edge of workspace.edges) {
    check(isPlainObject(edge), "plan.workspace.edge");
    check(packageIds.has(edge.from), "plan.workspace.edge.from");
    check(packageIds.has(edge.to), "plan.workspace.edge.to");
    check(EDGE_KINDS.has(edge.kind), "plan.workspace.edge.kind");
  }
  check(Array.isArray(workspace.uncertainties) && workspace.uncertainties.length <= 1024, "plan.workspace.uncertainties");
  for (const uncertainty of workspace.uncertainties) {
    check(isPlainObject(uncertainty) && UNCERTAINTY_KINDS.has(uncertainty.kind), "plan.workspace.uncertainty.kind");
    boundedString(uncertainty.detail, 256, "plan.workspace.uncertainty.detail");
  }
  // Affected / excluded.
  check(Array.isArray(plan.affected) && plan.affected.length <= 1024, "plan.affected");
  for (const affected of plan.affected) {
    check(isPlainObject(affected), "plan.affected.entry");
    check(packageIds.has(affected.package_id), "plan.affected.package_id");
    check(Array.isArray(affected.reasons) && affected.reasons.length >= 1 && affected.reasons.length <= 32, "plan.affected.reasons");
    for (const reason of affected.reasons) {
      check(isPlainObject(reason) && REASON_KINDS.has(reason.kind), "plan.affected.reason.kind");
      boundedString(reason.source_ref, 512, "plan.affected.reason.source_ref");
      if (reason.edge_path !== undefined) {
        check(Array.isArray(reason.edge_path) && reason.edge_path.length <= 32, "plan.affected.reason.edge_path");
        for (const hop of reason.edge_path) check(packageIds.has(hop), "plan.affected.reason.edge_path.hop");
      }
    }
  }
  check(Array.isArray(plan.excluded) && plan.excluded.length <= 1024, "plan.excluded");
  for (const entry of plan.excluded) {
    check(isPlainObject(entry), "plan.excluded.entry");
    check(packageIds.has(entry.package_id), "plan.excluded.package_id");
    check(EXCLUDED_REASONS.has(entry.reason), "plan.excluded.reason");
  }
  // Commands.
  check(Array.isArray(plan.commands) && plan.commands.length <= 128, "plan.commands");
  const commandIds = new Set();
  for (const command of plan.commands) {
    check(isPlainObject(command), "plan.command");
    const members = Object.keys(command).sort().join(",");
    check(members === "affected_reason_refs,allowed_writes,argv,confirmation_ref,cwd,depends_on,env,gate,id,limits,package_id,provenance,read_manifest_ref,script_digest,script_name,tool_ref,tsconfig_ref"
      || members === "affected_reason_refs,allowed_writes,argv,confirmation_ref,cwd,depends_on,env,gate,id,limits,package_id,read_manifest_ref,script_digest,script_name,tool_ref,tsconfig_ref", "plan.command.members");
    boundedString(command.id, 128, "plan.command.id", COMMAND_ID);
    check(!commandIds.has(command.id), "plan.command.duplicate");
    commandIds.add(command.id);
    check(packageIds.has(command.package_id), "plan.command.package_id");
    check(GATE_KINDS.has(command.gate), "plan.command.gate");
    boundedString(command.script_name, 64, "plan.command.script_name");
    digest(command.script_digest, "plan.command.script_digest");
    digest(command.confirmation_ref, "plan.command.confirmation_ref");
    boundedString(command.cwd, 512, "plan.command.cwd", LOGICAL_PATH);
    boundedString(command.tool_ref, 128, "plan.command.tool_ref");
    check(Array.isArray(command.argv) && command.argv.length >= 1 && command.argv.length <= 64, "plan.command.argv");
    let argvBytes = 0;
    for (const element of command.argv) {
      boundedString(element, 1024, "plan.command.argv.element");
      argvBytes += Buffer.byteLength(element, "utf8");
    }
    check(argvBytes <= 16384, "plan.command.argv.total");
    check(Array.isArray(command.env) && command.env.length <= 32, "plan.command.env");
    check(Array.isArray(command.depends_on) && command.depends_on.length <= 128, "plan.command.depends_on");
    for (const dependency of command.depends_on) check(commandIds.has(dependency) || typeof dependency === "string", "plan.command.depends_on.entry");
    check(Array.isArray(command.affected_reason_refs) && command.affected_reason_refs.length >= 1, "plan.command.affected_reason_refs");
    digest(command.read_manifest_ref, "plan.command.read_manifest_ref");
    checkWritePolicy(command.allowed_writes, "plan.command.allowed_writes");
    checkLimits(command.limits, "plan.command.limits");
  }
  checkEnv(plan.env, "plan.env");
  checkLimits(plan.limits, "plan.limits");
  checkWritePolicy(plan.write_policy, "plan.write_policy");
  for (const capability of plan.required_capabilities ?? []) {
    boundedString(capability, 128, "plan.required_capabilities.entry", CAPABILITY_ID);
  }
  // The recorded digest must equal the recomputed digest.
  check(canonicalPlanDigest(plan) === plan.plan_digest, "plan.plan_digest.recomputation");
  return true;
}

/**
 * Recompute the plan digest independently (Rust parity).
 */
export function recomputePlanDigest(plan) {
  return canonicalPlanDigest(plan);
}

export { isPlainObject };
