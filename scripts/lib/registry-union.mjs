// Additive diagnostic-family admission, never a milestone-wide fixed count.
import assert from 'node:assert/strict';
// One closed admission table for every family gate on the merged M8 line.
// Counts remain family-owned; adding a sibling must not weaken predecessor
// presence/shape checks or turn this into an arbitrary namespace allowlist.
export const ACCEPTED_M8_REGISTRY_FAMILIES = Object.freeze([
 'format.','adoption.','portability.','worklog-changelog.','managed.',
 'module-api.','tasklink.','search.','watch.',
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
