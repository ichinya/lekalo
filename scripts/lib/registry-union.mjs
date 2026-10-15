// The merged 0.8.0 diagnostic-registry union arithmetic, stated once
// (issue #73 r8, review finding omp F7): every family gate and the
// managed-contracts generator derived "predecessor + landed families"
// independently across six sites, so the next family merge (or one
// family gaining a rule) touched every gate. This module is the single
// source of that arithmetic; gates import it instead of re-deriving.
//
// The counts are the number of registry entries each merged family
// landed in the 0.8.0 union successor on top of the frozen 0.6.5
// predecessor (522 entries, preserved byte-identically). `codePrefix`
// is the registry code prefix the family's entries carry.

export const LANDED_FAMILIES = [
  { family: 'format', issue: 94, codePrefix: 'LEK-FORMAT-', entries: 11 },
  // Issue #136 landed nine rules; issue #73 r8 added the denied-allowed
  // shared-writer busy rule `adoption.writer-locked` (LEK-ADOPT-010).
  { family: 'adoption', issue: 136, codePrefix: 'LEK-ADOPT-', entries: 10 },
  { family: 'portability', issue: 19, codePrefix: 'LEK-PORT-', entries: 9 },
  { family: 'worklog-changelog', issue: 51, codePrefix: 'LEK-WC-', entries: 7 },
  { family: 'managed', issue: 41, codePrefix: 'LEK-MGD-', entries: 12 },
  { family: 'change', issue: 73, codePrefix: 'LEK-CHANGE-', entries: 12 },
];

// The total number of landed entries across every merged family.
export const LANDED_ENTRY_COUNT = LANDED_FAMILIES.reduce(
  (sum, family) => sum + family.entries,
  0,
);

// The expected 0.8.0 union successor size for one predecessor size.
export const unionSize = (predecessorCount) => predecessorCount + LANDED_ENTRY_COUNT;

// The number of entries one named family landed (`undefined` when the
// family is not part of the merged union).
export const familyCount = (family) =>
  LANDED_FAMILIES.find((candidate) => candidate.family === family)?.entries;

// The number of entries carrying one registry code prefix.
export const countByCodePrefix = (entries, codePrefix) =>
  entries.filter((entry) => entry.code.startsWith(codePrefix)).length;
