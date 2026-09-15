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
    version: "0.2.16",
    schema: "contracts/model.schema.v0.2.16.json",
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
    version: "0.2.16",
    schema: "contracts/model.schema.v0.2.16.json",
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

// Every fixture root is covered once; all sets use the current schema.
export function manifestIntegrityFailure(sets = MODEL_FIXTURE_SETS) {
 if (!Array.isArray(sets) || !sets.length) return ['fixture sets required'];
 const roots = new Set();
 for(const s of sets) {
  if (!s || s.version !== '0.2.16' || s.schema !== 'contracts/model.schema.v0.2.16.json' || typeof s.fixtures !== 'string' || !s.fixtures || roots.has(s.fixtures)) return ['invalid fixture set'];
  roots.add(s.fixtures);
 }
 return null;
}
