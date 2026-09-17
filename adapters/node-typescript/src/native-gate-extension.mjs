/**
 * The production plan-native extension (issue #48, plan §4/§5).
 *
 * One launch-seam extension that turns a validated `plan-native`
 * request into a proposed immutable native gate plan. Read-only:
 * every input comes from the request envelope plus the kernel's
 * validated read view; nothing is executed, spawned, installed, or
 * written. The extension refuses honestly when the launch profile or
 * a required input is absent — never guessing.
 *
 * The planner pipeline:
 *   1. build the workspace inventory over the read view (bounded,
 *      fail-closed pnpm/npm detection);
 *   2. join the changed inputs onto the inventory and compute the
 *      affected closure;
 *   3. verify every confirmed command against the package's real
 *      manifest `scripts` entry with parseConfirmedScript (the policy
 *      argv is verified, never copied blindly);
 *   4. derive depends_on edges (build prerequisites before dependents)
 *      and the tool catalog digest from the policy tool table;
 *   5. emit the plan with its deterministic digest.
 */
import { createHash } from "node:crypto";

import {
  buildNativePlan,
  parseConfirmedScript,
  PlanRefusal,
} from "./native-plan.mjs";
import {
  buildWorkspaceInventory,
  WorkspaceRefusal,
} from "./workspace.mjs";

/** Maximum packages one workspace may carry (mirrors the plan bounds). */
export const PLAN_NATIVE_CAPABILITY = "plan.native-gates";
/** The one operation this extension serves. */
export const PLAN_NATIVE_OPERATION = "plan-native";
/** Canonical version of the planner implementation. */
export const PLANNER_VERSION = "0.3.2";

/**
 * The committed synthetic-fixture execution policy: the trusted launch
 * input the bundle pins at build time. Null on the source deployment —
 * plan-native then refuses with execution-policy-absent.
 */
export let launchPolicy = null;

/** The adapter identity of the deployment (set by the bundle entry). */
export let adapterIdentity = null;

/** Set the adapter identity (called by the bundle entry). */
export function setAdapterIdentity(identity) {
  adapterIdentity = identity;
}

/** Set the launch policy (called by the bundle entry). */
export function setLaunchPolicy(policy) {
  launchPolicy = policy;
}

function sha256Text(text) {
  return "sha256:" + createHash("sha256").update(text, "utf8").digest("hex");
}

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * The internal native_request decoder: mirrors the kernel's closed
 * shape and bounds. Returns the request or a bounded refusal reason.
 */
function decodeNativeRequest(value) {
  if (!isObject(value)) {
    return { error: "native_request must be an object" };
  }
  for (const key of Object.keys(value)) {
    if (!["changes", "scan_ref", "observed_ref", "execution_policy_ref",
      "input_manifest_digest", "tool_catalog_digest", "capability_snapshot_digest"]
      .includes(key)) {
      return { error: `unknown native_request member ${key}` };
    }
  }
  for (const key of ["changes", "scan_ref", "execution_policy_ref",
    "input_manifest_digest", "tool_catalog_digest", "capability_snapshot_digest"]) {
    if (!(key in value)) {
      return { error: `missing native_request member ${key}` };
    }
  }
  if (!isObject(value.changes)
    || !Array.isArray(value.changes.files) || value.changes.files.length > 1024
    || !Array.isArray(value.changes.symbols) || value.changes.symbols.length > 1024) {
    return { error: "changes bound violated" };
  }
  for (const file of value.changes.files) {
    if (!isObject(file) || typeof file.path !== "string"
      || !["added", "modified", "deleted", "renamed"].includes(file.change)) {
      return { error: "changes.files entry malformed" };
    }
  }
  for (const key of ["input_manifest_digest", "tool_catalog_digest", "capability_snapshot_digest"]) {
    if (typeof value[key] !== "string" || !/^sha256:[0-9a-f]{64}$/.test(value[key])) {
      return { error: `${key} is not a sha256 digest` };
    }
  }
  return { value };
}

/**
 * Build the plan-native outcome. `context` is the kernel dispatch
 * context ({ operation, request, profile, readView, limits });
 * `policyDocument` is the checked-in execution policy supplied by the
 * trusted launch seam — the extension never searches for one.
 */
export function planNativeOperation(context, policyDocument) {
  const { request, profile, readView } = context;
  const permittedProjectRoot = readView?.permittedProjectRoot;
  if (!profile || !permittedProjectRoot) {
    return { state: "unsupported", diagnostics: [{ reason: "profile-absent" }] };
  }
  if (!isObject(policyDocument) || !Array.isArray(policyDocument.confirmations)) {
    return {
      state: "failed",
      diagnostics: [{ reason: "execution-policy-absent" }],
    };
  }
  const decoded = decodeNativeRequest(request.native_request);
  if (decoded.error) {
    return { state: "failed", diagnostics: [{ reason: decoded.error.slice(0, 64) }] };
  }
  const nativeRequest = decoded.value;
  let inventory;
  try {
    // Membership over the read view: pnpm-workspace.yaml + every
    // package.json the inventory enumerates (both must be in scope).
    const directories = listInventoryDirectories(readView);
    inventory = buildWorkspaceInventory({
      readView,
      permittedProjectRoot,
      directories,
    });
  } catch (error) {
    const reason = error instanceof WorkspaceRefusal ? error.code : "workspace-refused";
    return { state: "failed", diagnostics: [{ reason }] };
  }
  // Verify every confirmation against the package's real manifest
  // scripts entry: argv derives from the manifest text and must match
  // the policy argv and the recorded digests.
  const verification = verifyConfirmations(inventory, policyDocument, readView);
  if (verification.unverifiable.length > 0) {
    return {
      state: "failed",
      diagnostics: [{
        reason: ("confirmations-unverifiable:" + verification.unverifiable[0]).slice(0, 128),
      }],
    };
  }
  const toolCatalog = buildToolCatalog(policyDocument);
  const toolCatalogDigest = computeToolCatalogDigest(toolCatalog);
  let plan;
  try {
    plan = buildNativePlan({
      inventory,
      changes: {
        files: nativeRequest.changes.files,
        symbols: nativeRequest.changes.symbols,
      },
      policy: policyDocument,
      toolCatalog,
      toolCatalogDigest,
      profileRef: profile.id,
      profileDigest: profile.targetResolution?.digest ?? "sha256:" + "0".repeat(64),
      adapterIdentity,
      scanRef: { id: "scan", version: PLANNER_VERSION, digest: nativeRequest.scan_ref.digest },
      observedRef: nativeRequest.observed_ref
        ? { id: "observed", version: PLANNER_VERSION, digest: nativeRequest.observed_ref.digest }
        : { id: "observed", version: PLANNER_VERSION, digest: "sha256:" + "0".repeat(64) },
      inputManifestDigest: nativeRequest.input_manifest_digest,
      capabilitySnapshotDigest: nativeRequest.capability_snapshot_digest,
    });
  } catch (error) {
    const reason = error instanceof PlanRefusal ? error.code : "plan-refused";
    return { state: "failed", diagnostics: [{ reason }] };
  }
  return {
    state: "complete",
    data: {
      complete: true,
      native_plan: plan,
    },
    evidence: {
      plannerVersion: PLANNER_VERSION,
      capability: PLAN_NATIVE_CAPABILITY,
      commands: plan.commands.length,
      packages: plan.workspace.packages.length,
    },
  };
}

/**
 * List the directories of the inventory (bounded, sorted): walks the
 * read view's roots breadth-first without following links (the read
 * view itself refuses links on any touched path).
 */
/**
 * Candidate directories from the trusted launch profile: every tree
 * read root is a candidate, and every direct child directory of a
 * tree root that contains a package manifest in scope is one too.
 * The kernel read view enforces scope/exclusions on every probe; no
 * ambient walk exists.
 */
export function listInventoryDirectories(readView) {
  const candidates = new Set();
  for (const root of readView.roots ?? []) {
    if (root.kind !== "tree") continue;
    if (root.path !== ".") candidates.add(root.path);
    for (const child of profileDeclaredChildren(root.path, readView)) {
      candidates.add(child);
    }
  }
  return [...candidates].sort();
}

/**
 * Direct child directory names discoverable in scope: a child counts
 * when a package manifest or a source file under it is readable.
 * Bounded to the known workspace layout vocabulary ("packages" and
 * siblings of the declared roots), never an ambient walk.
 */
function profileDeclaredChildren(rootPath, readView) {
  const children = [];
  for (const name of ["packages", "apps", "libs", "tools"]) {
    const childPath = rootPath === "." ? name : rootPath + "/" + name;
    for (const leaf of ["package.json", "tsconfig.json", "src/main.ts"]) {
      if (readView.canRead(childPath + "/" + leaf)) {
        children.push(childPath);
        break;
      }
    }
  }
  return children;
}

function verifyConfirmations(inventory, policyDocument, readView) {
  const unverifiable = [];
  const packageById = new Map(inventory.packages.map((pkg) => [pkg.id, pkg]));
  for (const confirmation of policyDocument.confirmations) {
    const pkg = packageById.get(confirmation.package_id);
    if (!pkg) {
      // A confirmation for a package outside this workspace is not
      // applicable here (e.g. the standalone policy entry against the
      // monorepo fixture); it is neither verified nor a failure.
      continue;
    }
    if (pkg.manifestDigest !== confirmation.manifest_digest) {
      unverifiable.push(`manifest digest drift ${confirmation.package_id}`);
      continue;
    }
    let manifestText;
    try {
      manifestText = readView
        .readFile(pkg.manifestPath, { files: 4096, bytes: 1024 * 1024 })
        .toString("utf8");
    } catch (readError) {
      unverifiable.push(`manifest unreadable ${confirmation.package_id}: ${readError?.code ?? readError?.message ?? String.fromCharCode(63)}`);
      continue;
    }
    let scripts;
    try {
      scripts = JSON.parse(manifestText).scripts ?? {};
    } catch {
      unverifiable.push(`manifest unparsable ${confirmation.package_id}`);
      continue;
    }
    const scriptText = scripts[confirmation.script_name];
    if (typeof scriptText !== "string") {
      unverifiable.push(`script ${confirmation.script_name} absent ${confirmation.package_id}`);
      continue;
    }
    if (sha256Text(scriptText) !== confirmation.script_digest) {
      unverifiable.push(`script digest drift ${confirmation.package_id}`);
      continue;
    }
    const parsed = parseConfirmedScript(scriptText, confirmation.argv);
    if (!parsed.ok) {
      unverifiable.push(`argv mismatch ${confirmation.package_id}:${confirmation.script_name}`);
    }
  }
  return { unverifiable: unverifiable.slice(0, 16) };
}

/**
 * The tool catalog from the policy's confirmed tool refs, deduplicated
 * by id: name/version/provenance stay declared metadata; the digests
 * are the custody anchors the runner verifies.
 */
function buildToolCatalog(policyDocument) {
  const seen = new Map();
  for (const confirmation of policyDocument.confirmations) {
    const tool = confirmation.tool_ref;
    if (!isObject(tool) || typeof tool.id !== "string" || seen.has(tool.id)) continue;
    seen.set(tool.id, {
      id: tool.id,
      name: tool.id,
      version: tool.version ?? "unknown",
      artifact_digest: tool.artifact_digest,
      entry_digest: tool.entry_digest,
      platform: process.platform === "win32" ? "windows" : process.platform,
      provenance: "fixture-catalog",
    });
  }
  return [...seen.values()];
}

/**
 * The real tool catalog digest: sha256 over the canonical JSON of the
 * catalog entries (id/version/digests/platform), not a placeholder.
 */
function computeToolCatalogDigest(catalog) {
  const canonical = (value) => {
    if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
    if (value !== null && typeof value === "object") {
      return `{${Object.keys(value).sort()
        .map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
    }
    return JSON.stringify(value);
  };
  return sha256Text(canonical(catalog));
}

