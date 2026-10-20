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

// Additive diagnostic-family admission, never a milestone-wide fixed count.
import assert from 'node:assert/strict';
// One closed admission table for every family gate on the merged M8 line.
// Counts remain family-owned; adding a sibling must not weaken predecessor
// presence/shape checks or turn this into an arbitrary namespace allowlist.
export const ACCEPTED_M8_REGISTRY_FAMILIES = Object.freeze([
 'format.','adoption.','portability.','worklog-changelog.','managed.',
 'module-api.','tasklink.','search.','change.','watch.','fix.','mcp.',
]);
export function assertRegistryUnion(predecessor,successor,{families,ownPrefix,ownCount}) {
 const before=new Map(predecessor.entries.map(entry=>[entry.id,entry]));
 const after=new Map(successor.entries.map(entry=>[entry.id,entry]));
 assert.equal(after.size,successor.entries.length,'duplicate rule id');
 assert.equal(new Set(successor.entries.map(entry=>entry.code)).size,successor.entries.length,'duplicate diagnostic code');
 // Presence AND exact shape: merely checking added rows would silently
 // admit an integration that dropped or modified a live predecessor.
 for(const [id,entry] of before)assert.deepEqual(after.get(id),entry,`predecessor missing or changed: ${id}`);
 const additions=successor.entries.filter(entry=>!before.has(entry.id));
 assert(additions.every(entry=>families.some(prefix=>entry.id.startsWith(prefix))),'foreign-family addition');
 assert.equal(additions.filter(entry=>entry.id.startsWith(ownPrefix)).length,ownCount,'own-family addition count');
 const ids=successor.entries.map(entry=>entry.id);
 assert.deepEqual(ids,[...ids].sort(),'registry must be sorted unique');
 return additions;
}
