// Single deterministic source for the Model fixture inventory shared by
// scripts/test-model-contracts.mjs and scripts/test-model-ajv.mjs.
//
// Both suites must derive their fixture partitions from this manifest and
// verify it against the fixture tree. A fixture directory added on disk
// without a manifest entry therefore fails both suites instead of silently
// missing one of them, and the two suites can never hand-duplicate
// diverging partition lists again.

export const MODEL_DOCUMENT_SCHEMA = {
  "project.yaml": "projectDocument",
  "module.yaml": "moduleDocument",
  "entities.yaml": "entitiesDocument",
  "commands.yaml": "commandsDocument",
  "queries.yaml": "queriesDocument",
  "policies.yaml": "policiesDocument",
  "events.yaml": "eventsDocument",
  "scenarios.yaml": "scenariosDocument",
  "bindings.yaml": "bindingsDocument"
};

export const MODEL_FIXTURE_SETS = [
  {
    version: "0.1.0",
    schema: "contracts/model.schema.v0.1.0.json",
    fixtures: "tests/fixtures/model",
    schemaInvalid: [
      "invalid-constraint",
      "invalid-document-parse",
      "invalid-field-unknown",
      "invalid-id-grammar",
      "invalid-kind-not-allowed-in-file",
      "invalid-schema-version"
    ],
    schemaValid: [
      "invalid-duplicate-id",
      "invalid-identity-field-missing",
      "invalid-module-mismatch",
      "invalid-ref-kind-mismatch",
      "invalid-ref-unresolved",
      "invalid-target-unresolved",
      "valid-planner"
    ]
  },
  {
    version: "1.0.0",
    schema: "contracts/model.schema.v1.0.0.json",
    fixtures: "tests/fixtures/model-v1",
    schemaInvalid: [
      "invalid-case",
      "invalid-hyphen",
      "invalid-mixed-version",
      "invalid-module-import-hyphen",
      "invalid-module-renamed-from",
      "invalid-project-renamed-from",
      "invalid-registry-empty",
      "invalid-reserved-module",
      "invalid-reserved-project",
      "invalid-same-identity-false",
      "invalid-schema-version-unknown",
      "invalid-segment-count-four",
      "invalid-segment-count-one",
      "invalid-segment-length",
      "invalid-tombstone-deleted-target",
      "invalid-tombstone-replaced-by-missing"
    ],
    schemaValid: [
      "invalid-alias-ambiguous",
      "invalid-alias-tombstone-overlap",
      "invalid-convergence-unasserted",
      "invalid-duplicate-module",
      "invalid-duplicate-symbol",
      "invalid-kind-namespace",
      "invalid-old-reference-alias",
      "invalid-qualifier",
      "invalid-registry-project-version",
      "invalid-rename-cycle",
      "invalid-rename-history-missing",
      "invalid-rename-metadata-missing",
      "invalid-rename-source-live",
      "invalid-rename-target-missing",
      "invalid-rename-version-chain",
      "invalid-rename-version-terminal",
      "invalid-tombstone-duplicate",
      "invalid-tombstone-reuse",
      "invalid-tombstone-target-missing",
      "invalid-tombstone-version",
      "valid-alias-convergence",
      "valid-canonical-order",
      "valid-kind-namespaced",
      "valid-max-length",
      "valid-minimal",
      "valid-module-move",
      "valid-one-character-ids",
      "valid-planner",
      "valid-rename-chain",
      "valid-target-canonical-key",
      "valid-tombstones"
    ]
  }
];

// The Model 0.1.0 -> 1.0.0 compatibility golden pair. Each side validates
// against its exact respective schema in the third-party Ajv release gate.
export const MODEL_COMPAT_GOLDEN_PAIR = {
  root: "tests/fixtures/model-compat",
  sides: [
    { version: "0.1.0", schema: "contracts/model.schema.v0.1.0.json", directory: "0.1.0" },
    { version: "1.0.0", schema: "contracts/model.schema.v1.0.0.json", directory: "1.0.0" }
  ]
};

export function manifestFixtureSet(version) {
  const fixtureSet = MODEL_FIXTURE_SETS.find((candidate) => candidate.version === version);
  if (fixtureSet === undefined) {
    throw new Error(`model fixture manifest has no set for schema version ${version}`);
  }
  return fixtureSet;
}

export function manifestFixtureNames(fixtureSet) {
  return [...fixtureSet.schemaInvalid, ...fixtureSet.schemaValid].sort();
}

function partitionDefects(label, names) {
  const defects = [];
  if (!Array.isArray(names) || names.length === 0 || names.some((name) => typeof name !== "string" || name.length === 0)) {
    return [`${label} must be a non-empty array of non-empty strings`];
  }
  if (new Set(names).size !== names.length) {
    defects.push(`${label} must not repeat a fixture name`);
  }
  const sorted = [...names].sort();
  if (sorted.some((name, index) => name !== names[index])) {
    defects.push(`${label} must be listed in sorted order`);
  }
  return defects;
}

// Fail-closed partition parity for one fixture set: sorted, unique, disjoint
// partitions whose union is exactly the set of fixture directories observed
// on disk. The schemaValid partition intentionally holds checker-invalid
// fixtures whose documents are schema-valid. Returns null or a defect list.
export function fixtureManifestParityFailure(fixtureSet, diskNames) {
  const defects = [
    ...partitionDefects(`${fixtureSet.version}:schemaInvalid`, fixtureSet.schemaInvalid),
    ...partitionDefects(`${fixtureSet.version}:schemaValid`, fixtureSet.schemaValid)
  ];
  const overlap = fixtureSet.schemaInvalid.filter((name) => fixtureSet.schemaValid.includes(name));
  if (overlap.length > 0) {
    defects.push(`${fixtureSet.version}: partitions must be disjoint: ${overlap.join(",")}`);
  }
  const manifestNames = manifestFixtureNames(fixtureSet);
  const missing = diskNames.filter((name) => !manifestNames.includes(name));
  if (missing.length > 0) {
    defects.push(`${fixtureSet.version}: fixture directories missing from the manifest: ${missing.join(",")}`);
  }
  const unknown = manifestNames.filter((name) => !diskNames.includes(name));
  if (unknown.length > 0) {
    defects.push(`${fixtureSet.version}: manifest entries without fixture directories: ${unknown.join(",")}`);
  }
  return defects.length === 0 ? null : defects;
}

// Cross-set manifest integrity: unique versions, schemas, and fixture roots,
// and a compatibility golden pair that covers every fixture set row with
// exactly one side carrying the exact (version, schema) pair of that row.
// Every fixture-set version/schema/fixture-root and golden-side
// version/schema/directory must be a non-empty string, and the golden pair
// root must be a non-empty string, so malformed or missing values come back
// as defects here instead of flowing into the pairing joins below. Golden
// side versions, schemas, and directories must be unique, so extra, missing,
// duplicate, and swapped or otherwise mismatched pairs are all defects.
// Arguments default to the shipped manifest; the contract suite passes
// direct mutations as rejection probes. Returns null or a list of defects.
export function manifestIntegrityFailure(
  fixtureSets = MODEL_FIXTURE_SETS,
  goldenPair = MODEL_COMPAT_GOLDEN_PAIR
) {
  if (!Array.isArray(fixtureSets)) {
    return ["fixture sets must be an array"];
  }
  const defects = [];
  const versions = new Set();
  const schemas = new Set();
  const fixtureRoots = new Set();
  for (const fixtureSet of fixtureSets) {
    if (fixtureSet === null || typeof fixtureSet !== "object") {
      defects.push("every fixture set must be an object");
      continue;
    }
    for (const [seen, value, label] of [
      [versions, fixtureSet.version, "version"],
      [schemas, fixtureSet.schema, "schema"],
      [fixtureRoots, fixtureSet.fixtures, "fixture root"]
    ]) {
      if (typeof value !== "string" || value.length === 0) {
        defects.push(`fixture set ${label} must be a non-empty string`);
        continue;
      }
      if (seen.has(value)) {
        defects.push(`duplicate fixture set ${label} ${value}`);
      }
      seen.add(value);
    }
  }
  if (typeof goldenPair?.root !== "string" || goldenPair.root.length === 0) {
    defects.push("the compatibility golden pair root must be a non-empty string");
  }
  const sides = Array.isArray(goldenPair?.sides) ? goldenPair.sides : [];
  if (sides.length !== fixtureSets.length) {
    defects.push(`the compatibility golden pair must cover every fixture set schema version exactly once (expected ${fixtureSets.length} sides, found ${sides.length})`);
  }
  const goldenVersions = new Set();
  const goldenSchemas = new Set();
  const goldenDirectories = new Set();
  for (const side of sides) {
    if (side === null || typeof side !== "object") {
      defects.push("every compatibility golden side must be an object");
      continue;
    }
    for (const [seen, value, label] of [
      [goldenVersions, side.version, "version"],
      [goldenSchemas, side.schema, "schema"],
      [goldenDirectories, side.directory, "directory"]
    ]) {
      if (typeof value !== "string" || value.length === 0) {
        defects.push(`golden side ${label} must be a non-empty string`);
        continue;
      }
      if (seen.has(value)) {
        defects.push(`duplicate golden side ${label} ${value}`);
      }
      seen.add(value);
    }
    if (
      typeof side.version === "string" && side.version.length > 0
      && typeof side.schema === "string" && side.schema.length > 0
      && !fixtureSets.some((fixtureSet) => fixtureSet?.version === side.version && fixtureSet?.schema === side.schema)
    ) {
      defects.push(`golden side pairs version ${side.version} with schema ${side.schema}, which is no fixture set (version, schema) row`);
    }
  }
  for (const fixtureSet of fixtureSets) {
    const exact = sides.filter((side) =>
      side !== null && typeof side === "object"
        && side.version === fixtureSet?.version && side.schema === fixtureSet?.schema
    ).length;
    if (exact !== 1) {
      defects.push(`fixture set ${fixtureSet?.version} must have exactly one golden side with schema ${fixtureSet?.schema} (found ${exact})`);
    }
  }
  return defects.length === 0 ? null : defects;
}
