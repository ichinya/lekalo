#!/usr/bin/env node

// Reference checker for the canonical Lekalo project structure (issue #4).
// This layer validates file homes, portable paths and physical containment.
// Model, loader/import and lockfile contents belong to issues #5, #7 and #10.

import { lstat, readFile, readdir, realpath } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fixturesDir = join(root, "tests", "fixtures", "structure");

const KIND_FILES = ["entities", "commands", "queries", "policies", "events", "scenarios", "bindings"];
const MODULE_FILES = new Set(["module", ...KIND_FILES]);
const CANONICAL_ROOT_ENTRIES = new Set(["project.yaml", "modules", "targets", "authorization.yaml"]);
const RUNTIME_ENTRIES = new Set(["import", "cache", "generated", "consumer", "privacy"]);
const RUNTIME_CONTAINER_CHILDREN = new Map([
  ["consumer", new Set(["model", "bindings"])],
  ["privacy", new Set(["exports", "redacted", "aggregates", "decisions"])],
  ["privacy/decisions", new Set(["export", "redaction", "aggregate"])]
]);
export const RUNTIME_AUTHORITY_HOMES = Object.freeze([
  Object.freeze({ artifactKind: "lekalo.observed-model-draft", pattern: ".lekalo/import/**" }),
  Object.freeze({ artifactKind: "lekalo.cache", pattern: ".lekalo/cache/**" }),
  Object.freeze({ artifactKind: "lekalo.generated-intermediate", pattern: ".lekalo/generated/**" }),
  Object.freeze({ artifactKind: "consumer.model", pattern: ".lekalo/consumer/model/**" }),
  Object.freeze({ artifactKind: "consumer.target-bindings", pattern: ".lekalo/consumer/bindings/**" }),
  Object.freeze({ artifactKind: "export.artifact", pattern: ".lekalo/privacy/exports/**" }),
  Object.freeze({ artifactKind: "export.decision", pattern: ".lekalo/privacy/decisions/export/**" }),
  Object.freeze({ artifactKind: "redaction.artifact", pattern: ".lekalo/privacy/redacted/**" }),
  Object.freeze({ artifactKind: "redaction.decision", pattern: ".lekalo/privacy/decisions/redaction/**" }),
  Object.freeze({ artifactKind: "aggregate.artifact", pattern: ".lekalo/privacy/aggregates/**" }),
  Object.freeze({ artifactKind: "aggregate.decision", pattern: ".lekalo/privacy/decisions/aggregate/**" })
]);

const MAX_SEGMENT_LENGTH = 64;
const MAX_WALK_DEPTH = 64;
const MAX_WALK_ENTRIES = 10000;
const DOS_DEVICES = new Set([
  "CON", "PRN", "AUX", "NUL",
  "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
  "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
  "CONIN$", "CONOUT$", "CLOCK$"
]);

function invalid(reasonCodes) {
  return { outcome: "invalid", reasonCodes };
}

function denied(reasonCodes) {
  return { outcome: "denied", reasonCodes };
}

// Returns null when a declared project-relative path is portable, otherwise
// the first stable reason code. Compatibility normalization is classification
// only: accepted paths remain NFC and lowercase in their original spelling.
export function checkPathGrammar(path) {
  if (typeof path !== "string" || path.length === 0) return "structure.path-empty";
  if (path !== path.normalize("NFC")) return "structure.path-not-nfc";
  if (path.startsWith("/") || path.startsWith("\\\\") || path.startsWith("//")) return "structure.path-absolute";
  if (/^[a-zA-Z]:/.test(path) || /^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(path) || path.startsWith("~")) return "structure.path-absolute";
  if (path.includes("\\") || /%[0-9a-f]{2}/i.test(path)) return "structure.path-escape";
  if (path.includes(":")) return "structure.path-escape";

  for (const segment of path.split("/")) {
    if (segment.length === 0 || segment.length > MAX_SEGMENT_LENGTH) return "structure.path-segment";
    if (segment === "." || segment === ".." || segment.endsWith(".") || segment.endsWith(" ")) return "structure.path-traversal";
    const compatibilityBase = segment.normalize("NFKC").split(".")[0].toUpperCase();
    if (DOS_DEVICES.has(compatibilityBase)) return "structure.path-device";
    if (/~[0-9]/.test(segment)) return "structure.path-short-name";
    if (/[A-Z]/.test(segment)) return "structure.path-case";
    if (!/^[a-z0-9][a-z0-9._-]{0,63}$/.test(segment)) return "structure.path-segment";
  }
  return null;
}

// CLI root selectors are invocation-relative, not semantic model paths. They
// may contain ordinary repository names, but never absolute, upward, encoded,
// device or short-name-alias components.
export function checkSelectionGrammar(path) {
  if (typeof path !== "string" || path.length === 0) return "structure.selection-empty";
  if (path !== path.normalize("NFC") || /[\u0000-\u001f\u007f]/.test(path)) return "structure.selection-segment";
  if (path.startsWith("/") || path.startsWith("\\\\") || path.startsWith("//") || /^[a-zA-Z]:/.test(path) || /^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(path) || path.startsWith("~")) return "structure.selection-absolute";
  if (path.includes("\\") || /%[0-9a-f]{2}/i.test(path) || path.includes(":")) return "structure.selection-escape";
  if (path === ".") return null;

  const segments = path.startsWith("./") ? path.slice(2).split("/") : path.split("/");
  for (const segment of segments) {
    if (segment.length === 0 || segment === "." || segment === ".." || /[ .]$/.test(segment)) return "structure.selection-traversal";
    const compatibilityBase = segment.normalize("NFKC").split(".")[0].toUpperCase();
    if (DOS_DEVICES.has(compatibilityBase)) return "structure.selection-device";
    if (/~[0-9]/.test(segment)) return "structure.selection-short-name";
  }
  return null;
}

async function physicalType(path) {
  try {
    const metadata = await lstat(path);
    if (metadata.isSymbolicLink()) return "link";
    if (metadata.isDirectory()) return "directory";
    if (metadata.isFile()) return "file";
    return "special";
  } catch (error) {
    if (error?.code === "ENOENT") return "missing";
    return "unreadable";
  }
}

function sameResolvedPath(left, right) {
  return relative(left, right) === "";
}

async function checkPhysicalSelection(path, missingCode) {
  const resolvedPath = resolve(path);
  const type = await physicalType(resolvedPath);
  if (type === "link") return denied(["structure.selection-alias"]);
  if (type !== "directory") return invalid([missingCode]);
  try {
    const physicalPath = await realpath(resolvedPath);
    if (!sameResolvedPath(resolvedPath, physicalPath)) return denied(["structure.selection-alias"]);
  } catch {
    return invalid(["structure.selection-unreadable"]);
  }
  return null;
}

async function readEntries(dir, where) {
  try {
    const entries = await readdir(dir, { withFileTypes: true });
    return { entries: entries.sort((left, right) => left.name < right.name ? -1 : left.name > right.name ? 1 : 0) };
  } catch {
    return { error: invalid(["structure.directory-unreadable", where]) };
  }
}

async function scanGovernedTree(dir, logicalRoot, visit, depth = 0, counter = { n: 0 }) {
  if (depth > MAX_WALK_DEPTH || counter.n > MAX_WALK_ENTRIES) return invalid(["structure.scan-limit", logicalRoot]);
  const listing = await readEntries(dir, logicalRoot);
  if (listing.error) return listing.error;

  for (const entry of listing.entries) {
    counter.n += 1;
    if (counter.n > MAX_WALK_ENTRIES) return invalid(["structure.scan-limit", logicalRoot]);
    const logicalPath = `${logicalRoot}/${entry.name}`;
    if (entry.isSymbolicLink()) return denied(["structure.path-link", logicalPath]);
    if (!entry.isDirectory() && !entry.isFile()) return denied(["structure.path-special", logicalPath]);
    const pathFailure = checkPathGrammar(entry.name);
    if (pathFailure !== null) return invalid([pathFailure, logicalPath]);
    const visitFailure = visit(entry, logicalPath, depth);
    if (visitFailure) return visitFailure;
    if (entry.isDirectory()) {
      const nestedFailure = await scanGovernedTree(join(dir, entry.name), logicalPath, visit, depth + 1, counter);
      if (nestedFailure) return nestedFailure;
    }
  }
  return null;
}

function runtimePlacementFailure(entry, logicalPath) {
  const runtimePath = logicalPath.slice(".lekalo/".length);
  const segments = runtimePath.split("/");
  const top = segments[0];
  if (!RUNTIME_ENTRIES.has(top)) return denied(["structure.runtime-unexpected-entry", logicalPath]);
  if (segments.length === 1) return entry.isDirectory() ? null : denied(["structure.runtime-unexpected-entry", logicalPath]);
  if (["import", "cache", "generated"].includes(top)) return null;

  if (top === "consumer") {
    if (!RUNTIME_CONTAINER_CHILDREN.get("consumer").has(segments[1])) return denied(["structure.runtime-unexpected-entry", logicalPath]);
    if (segments.length === 2 && !entry.isDirectory()) return denied(["structure.runtime-unexpected-entry", logicalPath]);
    return null;
  }

  if (!RUNTIME_CONTAINER_CHILDREN.get("privacy").has(segments[1])) return denied(["structure.runtime-unexpected-entry", logicalPath]);
  if (segments.length === 2 && !entry.isDirectory()) return denied(["structure.runtime-unexpected-entry", logicalPath]);
  if (segments[1] !== "decisions") return null;
  if (segments.length === 2) return null;
  if (!RUNTIME_CONTAINER_CHILDREN.get("privacy/decisions").has(segments[2])) return denied(["structure.runtime-unexpected-entry", logicalPath]);
  if (segments.length === 3 && !entry.isDirectory()) return denied(["structure.runtime-unexpected-entry", logicalPath]);
  return null;
}

async function checkRuntimeArea(projectRoot) {
  const runtimeDir = join(projectRoot, ".lekalo");
  const type = await physicalType(runtimeDir);
  if (type === "missing") return { present: false, failure: null };
  if (type === "link") return { present: true, failure: denied(["structure.path-link", ".lekalo"]) };
  if (type !== "directory") return { present: true, failure: denied(["structure.runtime-unexpected-entry", ".lekalo"]) };

  const failure = await scanGovernedTree(runtimeDir, ".lekalo", (entry, logicalPath) => {
    if (entry.isDirectory() && entry.name === "lekalo") return denied(["structure.nested-root", logicalPath]);
    return runtimePlacementFailure(entry, logicalPath);
  });
  return { present: true, failure };
}

export async function findRoot(fromDir) {
  const selectionFailure = await checkPhysicalSelection(fromDir, "structure.from-not-directory");
  if (selectionFailure) throw Object.assign(new Error(selectionFailure.reasonCodes[0]), selectionFailure);
  let current = resolve(fromDir);
  for (;;) {
    const lekaloType = await physicalType(join(current, "lekalo"));
    const markerType = await physicalType(join(current, "lekalo", "project.yaml"));
    if (lekaloType === "link" || markerType === "link") throw Object.assign(new Error("structure.path-link"), denied(["structure.path-link"]));
    if (lekaloType === "directory" && markerType === "file") return current;
    const parent = dirname(current);
    if (parent === current) return null;
    current = parent;
  }
}

export async function validateProject(projectRoot) {
  const selectionFailure = await checkPhysicalSelection(projectRoot, "structure.project-not-directory");
  if (selectionFailure) return selectionFailure;

  const lekaloDir = join(projectRoot, "lekalo");
  const lekaloType = await physicalType(lekaloDir);
  if (lekaloType === "link") return denied(["structure.path-link", "lekalo"]);
  if (lekaloType !== "directory") return invalid(["structure.document-missing", "lekalo/project.yaml"]);

  const canonicalFailure = await scanGovernedTree(lekaloDir, "lekalo", (entry, logicalPath) => {
    if (entry.isDirectory() && entry.name === "lekalo") return denied(["structure.nested-root", logicalPath]);
    return null;
  });
  if (canonicalFailure) return canonicalFailure;

  if (await physicalType(join(lekaloDir, "project.yaml")) !== "file") return invalid(["structure.document-missing", "lekalo/project.yaml"]);
  const lekaloListing = await readEntries(lekaloDir, "lekalo");
  if (lekaloListing.error) return lekaloListing.error;
  for (const entry of lekaloListing.entries) {
    if (!CANONICAL_ROOT_ENTRIES.has(entry.name)) return denied(["structure.canonical-unexpected-entry", `lekalo/${entry.name}`]);
    if (entry.name === "project.yaml" && !entry.isFile()) return invalid(["structure.document-missing", "lekalo/project.yaml"]);
    if (entry.name === "authorization.yaml" && !entry.isFile()) return invalid(["structure.directory-required", "lekalo/authorization.yaml"]);
    if (["modules", "targets"].includes(entry.name) && !entry.isDirectory()) return invalid(["structure.directory-required", `lekalo/${entry.name}`]);
  }

  const modulesDir = join(lekaloDir, "modules");
  const modulesType = await physicalType(modulesDir);
  if (!["missing", "directory"].includes(modulesType)) return invalid(["structure.directory-required", "lekalo/modules"]);
  const moduleListing = modulesType === "directory" ? await readEntries(modulesDir, "lekalo/modules") : { entries: [] };
  if (moduleListing.error) return moduleListing.error;
  const modules = [];
  for (const entry of moduleListing.entries) {
    const modulePath = `lekalo/modules/${entry.name}`;
    if (!entry.isDirectory()) return denied(["structure.canonical-unexpected-entry", modulePath]);
    modules.push(entry.name);
    const moduleDir = join(modulesDir, entry.name);
    if (await physicalType(join(moduleDir, "module.yaml")) !== "file") return invalid(["structure.document-missing", `${modulePath}/module.yaml`]);
    const fileListing = await readEntries(moduleDir, modulePath);
    if (fileListing.error) return fileListing.error;
    for (const file of fileListing.entries) {
      const filePath = `${modulePath}/${file.name}`;
      if (file.isDirectory()) return denied(["structure.module-subdirectory", filePath]);
      const stem = file.name.endsWith(".yaml") ? file.name.slice(0, -5) : null;
      if (stem === null || !MODULE_FILES.has(stem)) return denied(["structure.canonical-unexpected-entry", filePath]);
    }
  }

  const targets = [];
  const targetsDir = join(lekaloDir, "targets");
  if (await physicalType(targetsDir) === "directory") {
    const targetListing = await readEntries(targetsDir, "lekalo/targets");
    if (targetListing.error) return targetListing.error;
    for (const entry of targetListing.entries) {
      const targetPath = `lekalo/targets/${entry.name}`;
      if (!entry.isFile() || !entry.name.endsWith(".yaml") || entry.name === ".yaml") return denied(["structure.canonical-unexpected-entry", targetPath]);
      targets.push(entry.name.slice(0, -5));
    }
  }

  const lockType = await physicalType(join(projectRoot, "lekalo.lock"));
  if (lockType === "link") return denied(["structure.path-link", "lekalo.lock"]);
  if (!["missing", "file"].includes(lockType)) return invalid(["structure.lock-not-file", "lekalo.lock"]);

  const runtime = await checkRuntimeArea(projectRoot);
  if (runtime.failure) return runtime.failure;

  return { outcome: "valid", report: { status: "valid", modules, targets, lock: lockType === "file", runtime: runtime.present } };
}

async function readExpectation(fixtureDir) {
  try {
    return JSON.parse(await readFile(join(fixtureDir, "expect.json"), "utf8"));
  } catch {
    return null;
  }
}

async function runFixtureConformance() {
  const listing = await readEntries(fixturesDir, "tests/fixtures/structure");
  if (listing.error) return { outcome: "invalid", report: { status: "invalid", reasonCodes: listing.error.reasonCodes } };
  const results = { valid: 0, malformed: 0, denied: 0 };
  const mismatches = [];
  for (const entry of listing.entries) {
    if (!entry.isDirectory() || (!entry.name.startsWith("valid-") && !entry.name.startsWith("invalid-"))) continue;
    const fixtureDir = join(fixturesDir, entry.name);
    const expectation = entry.name.startsWith("valid-") ? { outcome: "valid" } : await readExpectation(fixtureDir);
    if (expectation === null || !["valid", "invalid", "denied"].includes(expectation.outcome) || (expectation.outcome !== "valid" && typeof expectation.reasonCodes?.[0] !== "string")) {
      mismatches.push({ fixture: entry.name, expectedReason: "structure.expect-missing", actualReason: null });
      continue;
    }
    const result = await validateProject(fixtureDir);
    if (result.outcome !== expectation.outcome || (expectation.outcome !== "valid" && result.reasonCodes?.[0] !== expectation.reasonCodes[0])) {
      mismatches.push({ fixture: entry.name, expected: expectation.outcome, actual: result.outcome, expectedReason: expectation.reasonCodes?.[0] ?? null, actualReason: result.reasonCodes?.[0] ?? null });
      continue;
    }
    results[result.outcome === "valid" ? "valid" : result.outcome === "denied" ? "denied" : "malformed"] += 1;
  }
  if (mismatches.length > 0) return { outcome: "invalid", report: { status: "invalid", reasonCodes: ["structure.fixture-mismatch"], mismatches } };
  return { outcome: "valid", report: { status: "valid", fixtures: results } };
}

function usage() {
  return [
    "usage: node scripts/check-structure.mjs",
    "       node scripts/check-structure.mjs --project <relative-dir>",
    "       node scripts/check-structure.mjs --find-root [--from <relative-dir>]",
    "       node scripts/check-structure.mjs --validate-path <project-relative-path>"
  ].join("\n");
}

function writeFailure(result) {
  const envelope = JSON.stringify({ status: result.outcome === "denied" ? "denied" : "invalid", reasonCodes: result.reasonCodes }, null, 2) + "\n";
  if (result.outcome === "denied") {
    process.stdout.write(envelope);
    return 3;
  }
  process.stderr.write(envelope);
  return 1;
}

function writeUsageFailure() {
  process.stderr.write(JSON.stringify({ status: "invalid", reasonCodes: ["structure.usage"], usage: usage() }, null, 2) + "\n");
  return 1;
}

export async function main(argv) {
  const args = argv ?? process.argv.slice(2);
  let action = null;
  let project = null;
  let from = null;
  let validatePathValue = null;
  const seenOptions = new Set();

  function claimAction(nextAction) {
    if (action !== null) return false;
    action = nextAction;
    return true;
  }

  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (!["--project", "--from", "--find-root", "--validate-path"].includes(arg) || seenOptions.has(arg)) {
      return writeUsageFailure();
    }
    seenOptions.add(arg);

    if (arg === "--find-root") {
      if (!claimAction("find-root")) return writeUsageFailure();
      continue;
    }

    const value = args[i + 1];
    if (value === undefined || value.startsWith("--")) return writeUsageFailure();
    i += 1;
    if (arg === "--project") {
      if (!claimAction("project")) return writeUsageFailure();
      project = value;
    } else if (arg === "--from") {
      from = value;
    } else if (arg === "--validate-path") {
      if (!claimAction("validate-path")) return writeUsageFailure();
      validatePathValue = value;
    }
  }

  if (from !== null && action !== "find-root") return writeUsageFailure();

  if (action === null) {
    if (process.env.LEKALO_PROJECT !== undefined) {
      action = "project";
      project = process.env.LEKALO_PROJECT;
    } else {
      action = "conformance";
    }
  }

  if (action === "validate-path") {
    const violation = checkPathGrammar(validatePathValue);
    if (violation !== null) return writeFailure(invalid([violation, "structure.validate-path"]));
    process.stdout.write(JSON.stringify({ status: "valid", path: validatePathValue }, null, 2) + "\n");
    return 0;
  }

  if (action === "find-root") {
    const start = from ?? ".";
    const grammarFailure = checkSelectionGrammar(start);
    if (grammarFailure !== null) return writeFailure(invalid([grammarFailure]));
    const physicalFailure = await checkPhysicalSelection(start, "structure.from-not-directory");
    if (physicalFailure) return writeFailure(physicalFailure);
    try {
      const found = await findRoot(resolve(start));
      if (found === null) return writeFailure(invalid(["structure.root-not-found"]));
      process.stdout.write(JSON.stringify({ status: "valid", root: found }, null, 2) + "\n");
      return 0;
    } catch (error) {
      return writeFailure({ outcome: error.outcome ?? "invalid", reasonCodes: error.reasonCodes ?? [error.reasonCode ?? "structure.root-unreadable"] });
    }
  }

  if (action === "project") {
    const grammarFailure = checkSelectionGrammar(project);
    if (grammarFailure !== null) return writeFailure(invalid([grammarFailure]));
    const result = await validateProject(resolve(project));
    if (result.outcome === "valid") {
      process.stdout.write(JSON.stringify(result.report, null, 2) + "\n");
      return 0;
    }
    return writeFailure(result);
  }

  const conformance = await runFixtureConformance();
  if (conformance.outcome === "valid") {
    process.stdout.write(JSON.stringify(conformance.report, null, 2) + "\n");
    return 0;
  }
  process.stderr.write(JSON.stringify(conformance.report, null, 2) + "\n");
  return 1;
}

const invokedDirectly = process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) process.exitCode = await main();
