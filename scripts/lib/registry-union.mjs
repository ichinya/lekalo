// The milestone registry is an ADDITIVE union, not an exhaustive hand-maintained
// list of landed families. Every consumer validates the actual data (r10 #73):
// predecessorIds MUST be a subset, predecessor entries must be byte-identical,
// and all successor IDs/codes must be unique and sorted. Each owning family
// gate separately pins its own closed rule set. New live M8 families therefore
// require no arithmetic update in unrelated gates or this helper.

export function registryUnion(predecessor, successor) {
  const fail = (reason, id) => { throw new Error(`${reason}${id ? `: ${id}` : ""}`); };
  const originals = new Map(predecessor.entries.map((entry) => [entry.id, entry]));
  const byId = new Map();
  const codes = new Set();
  let previous = null;
  for (const entry of successor.entries) {
    if (previous !== null && entry.id <= previous) fail("registry-order", entry.id);
    if (byId.has(entry.id)) fail("registry-duplicate-id", entry.id);
    if (codes.has(entry.code)) fail("registry-code-collision", entry.code);
    previous = entry.id;
    byId.set(entry.id, entry);
    codes.add(entry.code);
  }
  for (const [id, original] of originals) {
    if (!byId.has(id)) fail("registry-predecessor-missing", id);
    if (JSON.stringify(byId.get(id)) !== JSON.stringify(original)) {
      fail("registry-predecessor-entry-drift", id);
    }
  }
  const families = new Map();
  for (const entry of successor.entries) {
    if (originals.has(entry.id)) continue;
    const family = entry.id.split(".")[0];
    if (!family || !entry.id.includes(".")) fail("registry-additive-family", entry.id);
    families.set(family, (families.get(family) ?? 0) + 1);
  }
  return {
    size: byId.size,
    added: byId.size - originals.size,
    families: [...families].map(([family, entries]) => ({ family, entries })),
  };
}
