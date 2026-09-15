#!/usr/bin/env node
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const read = p => JSON.parse(readFileSync(new URL('../'+p, import.meta.url), 'utf8'));
const registry = read('crates/lekalo-core/src/versioning/contracts/version-registry.v0.2.16.json');
assert.equal(registry.registry,'dev.lekalo.version-registry');
assert.equal(registry.registryVersion,'0.2.16');
assert.deepEqual(Object.keys(registry.families), ['model','ir','protocol']);
for(const family of Object.values(registry.families)) {
 assert.deepEqual(Object.keys(family).sort(), ['aliases','current','migrations','versions']);
 assert.equal(family.current, family === registry.families.protocol ? '0.3.1' : '0.2.16');
 assert.deepEqual(family.aliases,[]);
 assert.deepEqual(family.migrations,[]);
 assert.equal(family.versions.length,1);
 assert.equal(family.versions[0].version,family.current);
 assert.equal(family.versions[0].state,'supported');
 assert.ok(family.versions[0].reason.length>0);
}
const projected = {status:'valid',registryVersion:registry.registryVersion,families:Object.entries(registry.families).map(([family,f])=>({family,current:f.current,min:f.current,max:f.current,aliases:[],versions:f.versions.map(({version,state,classification})=>({version,state,classification}))}))};
assert.deepEqual(read('tests/fixtures/versioning/compatibility.golden.json'),projected);
console.log(JSON.stringify({ok:true,version:registry.registryVersion,families:Object.keys(registry.families)}));
