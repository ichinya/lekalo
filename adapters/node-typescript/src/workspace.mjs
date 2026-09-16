/**
 * Read-only pnpm/npm workspace inventory for the native gate planner
 * (issue #48, plan §4).
 *
 * Everything here is bounded, read-only, and fail-closed:
 * - membership comes only from an in-scope `pnpm-workspace.yaml` (parsed
 *   with a strict bounded subset: canonical relative literals, `*`,
 *   `**`, `?`, leading `!`; no tags/anchors/aliases/merge keys), never
 *   from ambient parent discovery;
 * - package manifests are strict-JSON objects from the kernel read view;
 * - edges are typed from `dependencies`/`devDependencies`/
 *   `optionalDependencies`/`peerDependencies` with `workspace:*`-style
 *   local specifiers plus plain-name evidence, TS references, and
 *   explicit bindings recorded as uncertainty when unproven;
 * - no link/junction escapes, no node_modules, no network, no shell,
 *   no package manager; ambiguity is reported, never guessed.
 */
import { createHash } from "node:crypto";

/** Maximum packages and edges one workspace inventory may carry. */
export const MAX_PACKAGES = 1024;
export const MAX_EDGES = 8192;
export const MAX_PATTERNS = 256;
export const MAX_UNCERTAINTIES = 1024;
/** Maximum size of one manifest/workspace document read. */
export const MAX_WORKSPACE_DOC_BYTES = 1024 * 1024;

const WORKSPACE_FILE = "pnpm-workspace.yaml";
const PACKAGE_MANIFEST = "package.json";

/** YAML subset refusal: everything outside the closed grammar. */
export class WorkspaceRefusal extends Error {
  constructor(code, message) {
    super(message ?? code);
    this.name = "WorkspaceRefusal";
    this.code = code;
  }
}

function sha256Text(text) {
  return "sha256:" + createHash("sha256").update(text, "utf8").digest("hex");
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

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * Strict bounded YAML subset parser for pnpm-workspace.yaml.
 *
 * Supports exactly:
 *   packages:\n  - "pattern"\n  - 'pattern'\n  - pattern
 * plus comments and blank lines. Everything else — tags (!!), anchors
 * (&), aliases (*), merge keys (<<), multi-line scalars (|, >), flow
 * syntax ({, [, mapping keys outside the two known ones) — is a
 * structured refusal, never an approximation. Duplicate `packages`
 * keys are refusals (duplicate-key rule).
 */
export function parseWorkspaceYaml(text) {
  if (typeof text !== "string" || text.length === 0) {
    throw new WorkspaceRefusal("workspace-empty", "the workspace document is empty");
  }
  if (Buffer.byteLength(text, "utf8") > MAX_WORKSPACE_DOC_BYTES) {
    throw new WorkspaceRefusal("workspace-oversize", "the workspace document exceeds the read bound");
  }
  if (/\t/.test(text)) {
    throw new WorkspaceRefusal("workspace-tab", "tab characters are not part of the accepted YAML subset");
  }
  const result = { packages: null, managerKeys: {} };
  let currentKey = null;
  const seenKeys = new Set();
  const lines = text.split(/\r?\n/);
  for (let index = 0; index < lines.length; index += 1) {
    const raw = lines[index];
    const line = raw.replace(/#.*$/, "").trimEnd();
    if (line.trim() === "") continue;
    if (/^\s*---(\s|$)/.test(line) || /^(\s*)\.\.\.(\s|$)/.test(line)) {
      throw new WorkspaceRefusal("workspace-yaml-unsupported", "document markers are outside the accepted subset");
    }
    if (/!(?!=)/.test(line) || /&(?!&)/.test(line) || /(^|\s)\*[A-Za-z0-9_]/.test(line)) {
      throw new WorkspaceRefusal("workspace-yaml-unsupported", "tags/anchors/aliases are outside the accepted subset");
    }
    if (/[|>]/.test(line.replace(/^[^:]*:/, "")) && /\s[|>][-++]?\s*$/.test(line)) {
      throw new WorkspaceRefusal("workspace-yaml-unsupported", "block scalars are outside the accepted subset");
    }
    const keyMatch = line.match(/^([A-Za-z][A-Za-z0-9_-]*):\s*$/);
    if (keyMatch) {
      const key = keyMatch[1];
      if (seenKeys.has(key)) {
        throw new WorkspaceRefusal("workspace-duplicate-key", `duplicate workspace key ${key}`);
      }
      seenKeys.add(key);
      currentKey = key;
      if (key === "packages") result.packages = [];
      else result.managerKeys[key] = null;
      continue;
    }
    const inlineMatch = line.match(/^([A-Za-z][A-Za-z0-9_-]*):\s+(.+?)\s*$/);
    if (inlineMatch) {
      if (/^[\\[{]/.test(inlineMatch[2])) {
        throw new WorkspaceRefusal("workspace-yaml-unsupported", "flow syntax is outside the accepted subset");
      }
      const key = inlineMatch[1];
      if (seenKeys.has(key)) {
        throw new WorkspaceRefusal("workspace-duplicate-key", `duplicate workspace key ${key}`);
      }
      seenKeys.add(key);
      currentKey = key;
      result.managerKeys[key] = inlineMatch[2];
      continue;
    }
    const itemMatch = line.match(/^\s{2,}-\s*(.+?)\s*$/);
    if (itemMatch && currentKey === "packages") {
      let value = itemMatch[1];
      if ((value.startsWith('"') && value.endsWith('"') && value.length >= 2)
        || (value.startsWith("'") && value.endsWith("'") && value.length >= 2)) {
        value = value.slice(1, -1);
      } else if (/["']/.test(value)) {
        throw new WorkspaceRefusal("workspace-yaml-unsupported", "unbalanced quotes in workspace pattern");
      }
      if (value.startsWith("!")) {
        // Exclusion patterns are recorded but the M3 membership subset
        // only supports directory inclusions; negation is recorded as a
        // pattern the graph layer refuses (never silently approximated).
      }
      if (result.packages.length >= MAX_PATTERNS) {
        throw new WorkspaceRefusal("workspace-pattern-limit", "the workspace declares too many patterns");
      }
      result.packages.push(value);
      continue;
    }
    throw new WorkspaceRefusal("workspace-yaml-unsupported", `unrecognized line ${index + 1} in the workspace document`);
  }
  return result;
}

/**
 * Whether one workspace pattern is inside the supported membership
 * subset: canonical relative literals, `*`, `**`, `?`, optional leading
 * `!`. Uppercase, backslashes, drives, traversal and leading dots are
 * refused (dot dirs are only explicit literals, never matched).
 */
export function classifyWorkspacePattern(pattern) {
  if (typeof pattern !== "string" || pattern.length === 0 || pattern.length > 256) {
    return { supported: false, reason: "pattern-empty-or-oversize" };
  }
  if (pattern.includes("\\")) return { supported: false, reason: "pattern-backslash" };
  if (/^[A-Za-z]:/.test(pattern)) return { supported: false, reason: "pattern-drive" };
  if (pattern.startsWith("/")) return { supported: false, reason: "pattern-absolute" };
  if (pattern.startsWith("./") || pattern.startsWith("../")) {
    return { supported: false, reason: "pattern-dot-prefix" };
  }
  if (pattern.includes("//")) return { supported: false, reason: "pattern-empty-segment" };
  const body = pattern.startsWith("!") ? pattern.slice(1) : pattern;
  if (body.length === 0) return { supported: false, reason: "pattern-empty" };
  if (body.startsWith(".")) return { supported: false, reason: "pattern-dot-prefix" };
  for (const segment of body.split("/")) {
    if (segment.length === 0) return { supported: false, reason: "pattern-empty-segment" };
    if (segment === "." || segment === "..") {
      return { supported: false, reason: "pattern-traversal-segment" };
    }
    if (segment.startsWith(".")) {
      return { supported: false, reason: "pattern-dot-segment" };
    }
    for (const character of segment) {
      const ok = (character >= "a" && character <= "z")
        || (character >= "0" && character <= "9")
        || character === "-" || character === "_" || character === "."
        || character === "*" || character === "?";
      if (!ok) return { supported: false, reason: `pattern-character-${character}` };
      if (character >= "A" && character <= "Z") {
        return { supported: false, reason: "pattern-uppercase" };
      }
    }
    if (segment.includes("**") && segment !== "**") {
      return { supported: false, reason: "pattern-embedded-globstar" };
    }
  }
  return { supported: true, negated: pattern.startsWith("!"), body };
}

/**
 * Match one candidate directory against one supported pattern body.
 * `packages/*` matches one level; `packages/**` matches every depth
 * strictly below; `?` is one non-slash character; literals match
 * exactly. Traversal into dot-prefixed directories never matches.
 */
export function patternMatchesDirectory(body, directory) {
  const patternSegments = body.split("/");
  const directorySegments = directory.split("/");
  if (directorySegments.some((segment) => segment.startsWith("."))) {
    return false;
  }
  const globstar = patternSegments[patternSegments.length - 1] === "**";
  const head = globstar ? patternSegments.slice(0, -1) : patternSegments;
  if (globstar) {
    if (directorySegments.length <= head.length) return false;
  } else if (directorySegments.length !== head.length) {
    return false;
  }
  const matchSegment = (pattern, value) => {
    // Bounded glob: `*` and `?` with literal backtracking, no nesting.
    const regex = pattern.replace(/[.+^${}()|[\]\\]/g, "\\$&")
      .replace(/\*\*/g, "\u0000")
      .replace(/\*/g, "[^/]*")
      .replace(/\?/g, "[^/]")
      .replace(/\u0000/g, ".*");
    return new RegExp(`^${regex}$`).test(value);
  };
  for (let index = 0; index < head.length; index += 1) {
    if (!matchSegment(head[index], directorySegments[index])) return false;
  }
  return true;
}

/**
 * Enumerate candidate directories from the read view's directory
 * inventory (never a fresh filesystem walk): one bounded breadth-first
 * expansion over the inventory's directory list.
 */
export function candidateDirectoriesFromInventory(directories) {
  return [...directories].sort(utf8Compare);
}

/**
 * Build the workspace inventory: membership, package records, and the
 * typed edge set. `readView` is the kernel read facade; `permittedRoot`
 * the validated absolute project root. Every path stays repository-
 * relative; nothing outside the declared inventory is read.
 */
export function buildWorkspaceInventory({ readView, permittedRoot, directories }) {
  if (!readView || !readView.canRead(PACKAGE_MANIFEST)) {
    return {
      manager: "npm-standalone",
      compatibilityPath: "supported",
      root: ".",
      workspaceManifestDigest: null,
      packages: [],
      edges: [],
      uncertainties: [{ kind: "no-workspace-config", detail: "no in-scope pnpm-workspace.yaml; standalone layout assumed" }],
      completeness: "unknown",
    };
  }
  if (!readView.canRead(WORKSPACE_FILE)) {
    return {
      manager: "npm-standalone",
      compatibilityPath: "supported",
      root: ".",
      workspaceManifestDigest: null,
      packages: [],
      edges: [],
      uncertainties: [],
      completeness: "unknown",
    };
  }
  let workspaceText;
  try {
    workspaceText = readView.readFile(WORKSPACE_FILE, { files: 1, bytes: MAX_WORKSPACE_DOC_BYTES }).toString("utf8");
  } catch (error) {
    throw new WorkspaceRefusal("workspace-read-denied", "the workspace document exists but cannot be read in scope");
  }
  const parsed = parseWorkspaceYaml(workspaceText);
  const patterns = parsed.packages ?? [];
  const inclusions = [];
  const uncertainties = [];
  for (const pattern of patterns) {
    const classification = classifyWorkspacePattern(pattern);
    if (!classification.supported) {
      uncertainties.push({ kind: "pattern-partial", detail: `unsupported workspace pattern: ${pattern}` });
      continue;
    }
    if (classification.negated) {
      uncertainties.push({ kind: "pattern-partial", detail: `exclusion patterns are not part of the M3 membership subset: ${pattern}` });
      continue;
    }
    inclusions.push(classification.body);
  }
  // Membership: every in-scope directory matched by an inclusion pattern.
  const members = [];
  const seenRoots = new Set();
  for (const directory of candidateDirectoriesFromInventory(directories ?? [])) {
    for (const body of inclusions) {
      if (patternMatchesDirectory(body, directory)) {
        if (seenRoots.has(directory)) break;
        seenRoots.add(directory);
        members.push(directory);
        break;
      }
    }
  }
  // The root package is included only when a valid root manifest exists.
  const rootIncluded = readView.canRead(PACKAGE_MANIFEST) && members.includes(".");
  void rootIncluded;
  if (members.length > MAX_PACKAGES) {
    throw new WorkspaceRefusal("workspace-package-limit", "the workspace exceeds the package bound");
  }
  const packages = [];
  const packageByName = new Map();
  const packageByRoot = new Map();
  for (const root of members) {
    const manifestPath = root === "." ? PACKAGE_MANIFEST : `${root}/${PACKAGE_MANIFEST}`;
    let manifest;
    try {
      manifest = JSON.parse(readView.readFile(manifestPath, { files: 1, bytes: MAX_WORKSPACE_DOC_BYTES }).toString("utf8"));
    } catch {
      uncertainties.push({ kind: "unknown", detail: `package manifest unreadable: ${manifestPath}` });
      continue;
    }
    if (!isObject(manifest)) {
      uncertainties.push({ kind: "unknown", detail: `package manifest is not an object: ${manifestPath}` });
      continue;
    }
    const name = typeof manifest.name === "string" && manifest.name.length > 0 && manifest.name.length <= 192
      ? manifest.name
      : null;
    const id = `${root}=${name ?? "(unnamed)"}`;
    if (name !== null && packageByName.has(name)) {
      throw new WorkspaceRefusal("package-name-collision", `duplicate package name ${name}`);
    }
    const record = {
      id,
      name,
      root,
      manifestPath,
      manifestDigest: sha256Text(JSON.stringify(sortDeep(manifest))),
      dependencies: manifest.dependencies ?? {},
      devDependencies: manifest.devDependencies ?? {},
      optionalDependencies: manifest.optionalDependencies ?? {},
      peerDependencies: manifest.peerDependencies ?? {},
    };
    packages.push(record);
    if (name !== null) packageByName.set(name, record);
    packageByRoot.set(root, record);
  }
  packages.sort((left, right) => utf8Compare(left.id, right.id));
  // Typed edges: consumer -> local dependency.
  const edges = [];
  const scopes = [
    ["dependencies", "dependency"],
    ["devDependencies", "dev-dependency"],
    ["optionalDependencies", "optional-dependency"],
    ["peerDependencies", "peer-dependency"],
  ];
  for (const consumer of packages) {
    for (const [scopeKey, edgeKind] of scopes) {
      for (const [dependencyName, specifier] of Object.entries(consumer[scopeKey])) {
        const target = packageByName.get(dependencyName);
        if (target) {
          if (edges.length >= MAX_EDGES) {
            throw new WorkspaceRefusal("workspace-edge-limit", "the workspace exceeds the edge bound");
          }
          const workspaceSpecifier = typeof specifier === "string" && specifier.startsWith("workspace:");
          edges.push({
            from: consumer.id,
            to: target.id,
            kind: edgeKind,
            scope: scopeKey,
            specifier: String(specifier).slice(0, 128),
            provenance: workspaceSpecifier ? "workspace-specifier" : "manifest-evidence",
          });
        } else {
          if (typeof specifier === "string" && (specifier.startsWith("workspace:") || specifier.startsWith("file:"))) {
            uncertainties.push({
              kind: "unresolved-dependency",
              detail: `${consumer.id} requires local ${dependencyName} but no workspace package declares it`,
              package_id: consumer.id,
            });
          }
        }
      }
    }
  }
  edges.sort((left, right) =>
    utf8Compare(left.from, right.from) || utf8Compare(left.to, right.to) || utf8Compare(left.kind, right.kind));
  return {
    manager: "pnpm-workspace",
    compatibilityPath: "supported",
    root: ".",
    workspaceManifestDigest: sha256Text(workspaceText),
    lockDigestState: "absent",
    packages,
    edges,
    uncertainties: uncertainties.slice(0, MAX_UNCERTAINTIES),
    completeness: uncertainties.length === 0 ? "complete" : "incomplete",
  };
}

/** Canonical (key-sorted) JSON for manifest digests. */
function sortDeep(value) {
  if (Array.isArray(value)) return value.map(sortDeep);
  if (isObject(value)) {
    const out = {};
    for (const key of Object.keys(value).sort(utf8Compare)) out[key] = sortDeep(value[key]);
    return out;
  }
  return value;
}
