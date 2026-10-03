// Shared library for the issue #90 golden fixture suite gates.
//
// Owns the closed registries the suite metadata may reference (runners,
// update recipes), the catalog load/validate helpers, and the path
// policy every suite gate enforces. Fixture data can never widen these
// registries: an unknown runner or recipe id fails the gate.

import { readFileSync, readdirSync, existsSync, statSync } from "node:fs";
import { createHash } from "node:crypto";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

export const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
export const SUITE_ROOT = "tests/fixtures/suite";
export const SUITE_V1 = `${SUITE_ROOT}/v1`;
export const FIXTURE_SCHEMA = "dev.lekalo.fixture@1.0.0";
export const REGISTRY_IDENTITY = "dev.lekalo.diagnostic-registry@0.4.0";
export const REGISTRY_CONTRACT = "contracts/diagnostic-registry.v0.4.0.json";

/** The closed runner registry: id -> execution kind. */
export const RUNNERS = new Map([
  ["cli-validate", "cli-subprocess"],
  ["cli-load", "cli-subprocess"],
  ["cli-load-ir", "cli-subprocess"],
  ["cli-graph-export", "cli-subprocess"],
  ["cli-effects", "cli-subprocess"],
  ["cli-inspect", "cli-subprocess"],
  ["cli-impact", "cli-subprocess"],
  ["cli-context", "cli-subprocess"],
  ["cli-diff", "cli-subprocess"],
  ["cli-trace-export", "cli-subprocess"],
  ["cli-query-model-validate", "cli-subprocess"],
  ["node-scenario-runner", "node-in-process"],
  ["protocol-echo", "node-in-process"],
  ["static-recipe", "static"],
]);

/** The closed update-recipe registry. */
export const UPDATE_RECIPES = new Set([
  "update-golden-case",
  "update-golden-run-manifest",
  "update-golden-checksums",
  "gen-suite-coverage",
  "gen-suite-diagnostic-pairs",
]);

/** The CLI runner ids (executed through the cargo-built binary). */
export const CLI_RUNNERS = new Set(
  [...RUNNERS.entries()].filter(([, kind]) => kind === "cli-subprocess").map(([id]) => id),
);

/**
 * Repo-relative path policy: slash-separated, no `..`, no absolute
 * paths, no backslashes, no drive/UNC spellings. Dangerous path inputs
 * are content recipes inside security cases, never descriptor paths.
 */
export function assertRepoPath(relative, label = "path") {
  if (typeof relative !== "string" || relative.length === 0) {
    throw new Error(`${label}: empty path`);
  }
  if (relative.includes("\\") || /^[a-zA-Z]:/.test(relative) || relative.startsWith("//")) {
    throw new Error(`${label}: host path spelling is forbidden: ${relative}`);
  }
  if (relative.startsWith("/")) {
    throw new Error(`${label}: absolute path is forbidden: ${relative}`);
  }
  const parts = relative.split("/");
  for (const part of parts) {
    if (part === ".." || part === "." || part.length === 0) {
      throw new Error(`${label}: path escapes the repository root: ${relative}`);
    }
  }
  return relative;
}

/** Resolve a validated repo-relative path to an absolute host path. */
export function repoPath(relative, label) {
  return join(REPO_ROOT, assertRepoPath(relative, label));
}

/** Read a validated repo-relative file as UTF-8 text. */
export function readRepoText(relative, label) {
  return readFileSync(repoPath(relative, label), "utf8");
}

/** Read a validated repo-relative file as parsed JSON. */
export function readRepoJson(relative, label) {
  return JSON.parse(readRepoText(relative, label));
}

/** sha256 of a Buffer/UTF-8 string, `sha256:<hex>`-framed. */
export function sha256(data) {
  return `sha256:${createHash("sha256").update(data).digest("hex")}`;
}

/**
 * The exact suite-v1 case list, derived from the catalog. Returns the
 * parsed catalog; the caller validates semantics.
 */
export function loadCatalog() {
  const catalog = readRepoJson(`${SUITE_V1}/catalog.json`, "catalog");
  const errors = [];
  if (catalog.catalogId !== "dev.lekalo.fixture-catalog") errors.push("catalog identity");
  if (catalog.fixtureSchema !== FIXTURE_SCHEMA) errors.push("catalog fixtureSchema");
  if (catalog.version !== "1.0.0") errors.push("catalog version");
  return { catalog, errors };
}

/**
 * Load and validate every case descriptor referenced by the catalog.
 * Returns `{ cases, errors }`; each case carries its parsed descriptor.
 */
export function loadCases(catalog) {
  const cases = [];
  const errors = [];
  const seen = new Set();
  for (const entry of catalog.cases) {
    const id = entry.caseId;
    if (seen.has(id)) {
      errors.push(`duplicate case id ${id}`);
      continue;
    }
    seen.add(id);
    let descriptor;
    try {
      descriptor = readRepoJson(entry.descriptor, `case ${id} descriptor`);
    } catch (error) {
      errors.push(`case ${id}: descriptor unreadable: ${error.message}`);
      continue;
    }
    const caseErrors = validateDescriptor(id, entry, descriptor);
    errors.push(...caseErrors);
    cases.push({ id, revision: entry.revision, labels: entry.labels ?? [], descriptor, path: entry.descriptor });
  }
  return { cases, errors };
}

/** Validate one descriptor against the closed invariants. */
function validateDescriptor(id, entry, d) {
  const errors = [];
  const where = `case ${id}`;
  if (d.fixtureSchema !== FIXTURE_SCHEMA) errors.push(`${where}: fixtureSchema`);
  if (d.caseId !== id) errors.push(`${where}: caseId mismatch`);
  if (!Number.isInteger(d.revision) || d.revision < 1) errors.push(`${where}: revision`);
  if (entry.revision !== d.revision) errors.push(`${where}: catalog/descriptor revision drift`);
  if (d.origin !== "synthetic") errors.push(`${where}: origin must be synthetic`);
  if (!RUNNERS.has(d.runner)) errors.push(`${where}: unknown runner ${d.runner}`);
  if (!Array.isArray(d.contractPins) || d.contractPins.length === 0) {
    errors.push(`${where}: contractPins required`);
  }
  const expect = d.expectation ?? {};
  if (expect.witnessRule && !expect.witnessRule.includes(".")) {
    errors.push(`${where}: witnessRule must be a dotted rule id`);
  }
  for (const input of d.inputs ?? []) {
    try {
      assertRepoPath(input.path, `${where} input`);
    } catch (error) {
      errors.push(`${where}: ${error.message}`);
    }
    if (!existsSync(repoPath(input.path, `${where} input`))) {
      errors.push(`${where}: input missing on disk: ${input.path}`);
    }
  }
  for (const output of d.expected ?? []) {
    try {
      assertRepoPath(output.path, `${where} expected`);
    } catch (error) {
      errors.push(`${where}: ${error.message}`);
      continue;
    }
    if (!existsSync(repoPath(output.path, `${where} expected`))) {
      errors.push(`${where}: expected output missing on disk: ${output.path}`);
    }
  }
  if (d.updateRecipe !== undefined && !UPDATE_RECIPES.has(d.updateRecipe)) {
    errors.push(`${where}: unknown updateRecipe ${d.updateRecipe}`);
  }
  return errors;
}

/** The digests of every file under a repo-relative directory (sorted). */
export function directoryDigests(relative) {
  const absolute = repoPath(relative, "directory");
  const out = new Map();
  const walk = (current, logical) => {
    for (const name of readdirSync(current).sort()) {
      const full = join(current, name);
      const logicalChild = `${logical}/${name}`;
      if (statSync(full).isDirectory()) {
        walk(full, logicalChild);
      } else {
        out.set(logicalChild, sha256(readFileSync(full)));
      }
    }
  };
  walk(absolute, relative.split(sep).join("/"));
  return out;
}

/** The typed fail helper every gate uses. */
export function failGate(gate, errors) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate, errors }, null, 2)}\n`);
  process.exit(1);
}

/** The typed success printer. */
export function passGate(gate, detail) {
  process.stdout.write(`${JSON.stringify({ ok: true, gate, ...detail }, null, 2)}\n`);
}
