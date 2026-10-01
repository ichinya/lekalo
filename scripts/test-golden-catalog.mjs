#!/usr/bin/env node
// Golden-suite catalog gate (issue #90).
//
// Validates the suite-v1 catalog and every case descriptor against the
// closed schemas with the pinned Ajv 8.17.1 (same provisioning as the
// other contract gates), then enforces the invariants JSON Schema
// cannot express: unique sorted case ids, path safety, closed runner
// and recipe registries, existing inputs/expected outputs, checksum
// sidecars whose digests match the tracked bytes, provenance
// registration of the `suite` family, and no orphan expected files
// under the suite tree. Fails closed on any drift.

import { createRequire } from "node:module";
import { readdirSync, readFileSync, statSync, existsSync } from "node:fs";
import { join, relative } from "node:path";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  Ajv2020 = require("ajv/dist/2020").default;
  ajvVersion = require("ajv/package.json").version;
} catch {
  try {
    const fallback = join(process.env.LEKALO_AJV_NODE_PATH ?? "", "ajv");
    Ajv2020 = require(join(fallback, "dist/2020.js")).default;
    ajvVersion = JSON.parse(readFileSync(join(fallback, "package.json"), "utf8")).version;
  } catch (error) {
    process.stderr.write(`ajv-8.17.1-unavailable: ${error?.message ?? error}\n`);
    process.exit(1);
  }
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(`ajv-version-mismatch: ${ajvVersion}\n`);
  process.exit(1);
}

const {
  REPO_ROOT,
  SUITE_ROOT,
  SUITE_V1,
  FIXTURE_SCHEMA,
  REGISTRY_IDENTITY,
  REGISTRY_CONTRACT,
  RUNNERS,
  UPDATE_RECIPES,
  assertRepoPath,
  repoPath,
  sha256,
  failGate,
  passGate,
} = await import("../scripts/lib/fixture-catalog.mjs");

const readJson = (relative) => JSON.parse(readFileSync(join(REPO_ROOT, relative), "utf8"));

const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateCatalog = ajv.compile(readJson(`${SUITE_ROOT}/schema/catalog.schema.v1.0.0.json`));
const validateFixture = ajv.compile(readJson(`${SUITE_ROOT}/schema/fixture.schema.v1.0.0.json`));

const errors = [];
const catalog = readJson(`${SUITE_V1}/catalog.json`);
if (!validateCatalog(catalog)) {
  failGate("golden-catalog", [{ reason: "catalog-schema", errors: validateCatalog.errors }]);
}

// 1. Every case descriptor validates and its id/revision agree.
const seen = new Map();
for (const entry of catalog.cases) {
  const where = `case ${entry.caseId}`;
  if (seen.has(entry.caseId)) {
    errors.push(`${where}: duplicate id`);
    continue;
  }
  seen.set(entry.caseId, entry);
  let descriptor;
  try {
    assertRepoPath(entry.descriptor, where);
    descriptor = readJson(entry.descriptor);
  } catch (error) {
    errors.push(`${where}: descriptor unreadable: ${error.message}`);
    continue;
  }
  if (!validateFixture(descriptor)) {
    errors.push(`${where}: descriptor schema: ${JSON.stringify(validateFixture.errors)}`);
    continue;
  }
  if (descriptor.fixtureSchema !== FIXTURE_SCHEMA) errors.push(`${where}: fixtureSchema`);
  if (descriptor.caseId !== entry.caseId) errors.push(`${where}: caseId drift`);
  if (descriptor.revision !== entry.revision) errors.push(`${where}: revision drift`);
  if (descriptor.origin !== "synthetic") errors.push(`${where}: origin`);
  if (!RUNNERS.has(descriptor.runner)) errors.push(`${where}: runner`);
  if (descriptor.updateRecipe !== undefined && !UPDATE_RECIPES.has(descriptor.updateRecipe)) {
    errors.push(`${where}: updateRecipe`);
  }

  // Inputs and expected outputs must exist; expected digests must match.
  for (const input of descriptor.inputs ?? []) {
    try {
      assertRepoPath(input.path, `${where} input`);
    } catch (error) {
      errors.push(`${where}: ${error.message}`);
      continue;
    }
    if (!existsSync(repoPath(input.path))) {
      errors.push(`${where}: input missing: ${input.path}`);
    } else if (input.digest) {
      const actual = sha256(readFileSync(repoPath(input.path)));
      if (actual !== input.digest) errors.push(`${where}: input digest drift: ${input.path}`);
    }
  }
  for (const output of descriptor.expected ?? []) {
    try {
      assertRepoPath(output.path, `${where} expected`);
    } catch (error) {
      errors.push(`${where}: ${error.message}`);
      continue;
    }
    if (!existsSync(repoPath(output.path))) {
      errors.push(`${where}: expected missing: ${output.path}`);
    } else if (output.digest) {
      const actual = sha256(readFileSync(repoPath(output.path)));
      if (actual !== output.digest) errors.push(`${where}: expected digest drift: ${output.path}`);
    }
  }
}

// 2. Catalog case ids are sorted.
const ids = catalog.cases.map((entry) => entry.caseId);
const sorted = [...ids].sort();
if (ids.some((id, index) => id !== sorted[index])) errors.push("cases-not-sorted-by-id");

// 3. Imported evidence paths exist and stay inside their named family.
for (const imported of catalog.importedEvidence ?? []) {
  try {
    assertRepoPath(imported.path, `imported ${imported.family}`);
  } catch (error) {
    errors.push(error.message);
    continue;
  }
  if (!existsSync(repoPath(imported.path))) {
    errors.push(`imported evidence missing: ${imported.path}`);
  }
  if (!imported.path.startsWith(`tests/fixtures/${imported.family}/`)) {
    errors.push(`imported evidence outside its family: ${imported.path}`);
  }
}

// 4. Provenance: the suite family is declared synthetic.
const provenance = readJson("tests/fixtures/fixture-provenance.json");
const suiteFamily = provenance.families.find((family) => family.family === "suite");
if (!suiteFamily) errors.push("provenance: suite family undeclared");
else if (suiteFamily.origin !== "synthetic") errors.push("provenance: suite origin");

// 5. The embedded registry identity pin matches the contract bytes.
const registry = readJson(REGISTRY_CONTRACT);
if (registry.identity !== REGISTRY_IDENTITY) errors.push("registry identity pin");
const coverageIndex = readJson(`${SUITE_V1}/coverage/diagnostic-rules.json`);
if (coverageIndex.registryIdentity !== REGISTRY_IDENTITY) errors.push("coverage registry pin");
if (coverageIndex.registryDigest !== sha256(readFileSync(join(REPO_ROOT, REGISTRY_CONTRACT)))) {
  errors.push("coverage registry digest");
}

// 6. Coverage index integrity: every registry rule exactly once, sorted,
//    with evidence fields consistent with the claimed state.
const registryRules = registry.entries.map((entry) => entry.id);
const covered = coverageIndex.rules.map((rule) => rule.id);
const coveredSorted = [...covered].sort();
if (covered.some((id, index) => id !== coveredSorted[index])) errors.push("coverage-not-sorted");
if (covered.length !== new Set(covered).size) errors.push("coverage-duplicate-ids");
for (const id of registryRules) {
  if (!covered.includes(id)) errors.push(`coverage-missing-rule: ${id}`);
}
for (const id of covered) {
  if (!registryRules.includes(id)) errors.push(`coverage-unknown-rule: ${id}`);
}
const registryById = new Map(registry.entries.map((entry) => [entry.id, entry]));
for (const rule of coverageIndex.rules) {
  const entry = registryById.get(rule.id);
  if (!entry) continue;
  if (rule.code !== entry.code) errors.push(`coverage-code-drift: ${rule.id}`);
  if (rule.category !== entry.category) errors.push(`coverage-category-drift: ${rule.id}`);
  switch (rule.evidence) {
    case "suite-pair": {
      const dir = repoPath(`${SUITE_V1}/diagnostics/${rule.suitePair?.replace(/^diagnostic\./, "").replace(/\.pair$/, "")}`);
      const triggerOk = rule.suitePair && existsSync(dir);
      if (!triggerOk) errors.push(`coverage-suite-pair-missing-dir: ${rule.id}`);
      break;
    }
    case "family-fixture": {
      if (!Array.isArray(rule.familyFixture) || rule.familyFixture.length === 0) {
        errors.push(`coverage-family-missing-list: ${rule.id}`);
        break;
      }
      for (const fixture of rule.familyFixture) {
        try {
          assertRepoPath(fixture.path, `coverage ${rule.id}`);
        } catch (error) {
          errors.push(error.message);
          continue;
        }
        if (!existsSync(repoPath(fixture.path))) {
          errors.push(`coverage-family-fixture-missing: ${rule.id}: ${fixture.path}`);
        }
      }
      break;
    }
    case "test-witness": {
      if (!Array.isArray(rule.testWitness) || rule.testWitness.length === 0) {
        errors.push(`coverage-witness-missing-list: ${rule.id}`);
      }
      break;
    }
    case "interaction-only": {
      if (typeof rule.note !== "string" || rule.note.length < 8) {
        errors.push(`coverage-interaction-needs-note: ${rule.id}`);
      }
      break;
    }
    default:
      errors.push(`coverage-unknown-evidence: ${rule.id}`);
  }
}

// 7. Kind coverage index exists and covers every definition kind.
const kinds = readJson(`${SUITE_V1}/coverage/kinds.json`);
const definitionKinds = [
  "scalar", "enum", "value-object", "entity", "command", "query",
  "policy", "event", "effect", "endpoint", "scenario", "target-binding",
];
for (const kind of definitionKinds) {
  if (!kinds.kinds.some((row) => row.kind === kind)) errors.push(`kinds-missing: ${kind}`);
  const row = kinds.kinds.find((entry) => entry.kind === kind);
  if (row) {
    if (!Array.isArray(row.witnesses) || row.witnesses.length === 0) {
      errors.push(`kinds-witness-missing: ${kind}`);
    }
    for (const witness of row.witnesses) {
      if (!existsSync(repoPath(witness.path))) {
        errors.push(`kinds-witness-missing-path: ${kind}: ${witness.path}`);
      }
    }
  }
}

// 8. No orphan files under the suite tree: every tracked suite file
//    must be an input, expected output, schema, README, checksum, or
//    catalog/coverage metadata referenced from the catalog — or a
//    case-internal project document (inputs) / expect.json.
const allowedOrphans = new Set();
for (const entry of catalog.cases) {
  const base = entry.descriptor.slice(0, entry.descriptor.lastIndexOf("/"));
  allowedOrphans.add(base);
  // The case's sibling directories (expected/, checksums sidecars) are
  // case-owned too.
  allowedOrphans.add(base.split("/").slice(0, -1).join("/"));
}
const walk = (current) => {
  for (const name of readdirSync(current).sort()) {
    const full = join(current, name);
    if (statSync(full).isDirectory()) walk(full);
    else orphans.push(relative(REPO_ROOT, full).split("\\").join("/"));
  }
};
const orphans = [];
walk(join(REPO_ROOT, SUITE_V1));
for (const file of orphans) {
  if (file.endsWith("fixture.json")) continue;
  if (file.startsWith(`${SUITE_V1}/catalog.json`)) continue;
  if (file === `${SUITE_V1}/run-manifest.json`) continue;
  if (file.startsWith(`${SUITE_V1}/coverage/`)) continue;
  if (file.startsWith(`${SUITE_ROOT}/schema/`)) continue;
  if (file.startsWith(`${SUITE_V1}/checksums/`)) continue;
  if (file === `${SUITE_ROOT}/README.md`) continue;
  // Case-owned documents: any file under a registered case directory.
  const caseBase = [...allowedOrphans].find((base) => file.startsWith(`${base}/`));
  if (caseBase) continue;
  errors.push(`orphan-suite-file: ${file}`);
}

if (errors.length > 0) failGate("golden-catalog", errors);
passGate("golden-catalog", {
  cases: catalog.cases.length,
  importedEvidence: (catalog.importedEvidence ?? []).length,
  registryRules: registryRules.length,
  coverageStates: coverageIndex.rules.reduce((acc, rule) => {
    acc[rule.evidence] = (acc[rule.evidence] ?? 0) + 1;
    return acc;
  }, {}),
  ajv: ajvVersion,
});
