#!/usr/bin/env node
// Issue #151: lossless native plans. Preserve every published predecessor.
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync} from 'node:fs';
const wire = JSON.parse(readFileSync('contracts/target-protocol.schema.v0.9.0.json', 'utf8'));
wire.$id = 'https://dev.lekalo/target-protocol.schema.v0.9.1.json';
wire.$defs.protocolVersion = {const: '0.9.1'};
wire.description += ' Successor 0.9.1 carries the complete immutable native plan in successful plan-native responses. Its digest and bounded summary must agree with the complete document; a summary alone cannot authorize production execution. Every predecessor keeps its published wire members.';
const plan = JSON.parse(readFileSync('contracts/native-gate-plan.schema.v0.4.0.json', 'utf8'));
function prefixRefs(value) {
  if (Array.isArray(value)) return value.map(prefixRefs);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, prefixRefs(v)]));
  return typeof value === 'string' && value.startsWith('#/$defs/') ? value.replace('#/$defs/', '#/$defs/NativePlan_') : value;
}
for (const [key, value] of Object.entries(plan.$defs)) wire.$defs[`NativePlan_${key}`] = prefixRefs(value);
wire.$defs.NativePlanDocument = prefixRefs(Object.fromEntries(Object.entries(plan).filter(([key]) => !['$id', '$schema', '$defs'].includes(key))));
wire.$defs.nativePlanRef.properties.plan = {$ref: '#/$defs/NativePlanDocument'};
wire.$defs.responseEnvelope.allOf.push({
  if: {type: 'object', required: ['operation', 'status'], properties: {operation: {const: 'plan-native'}, status: {const: 'ok'}}},
  then: {type: 'object', required: ['result'], properties: {result: {type: 'object', required: ['native_plan'], properties: {
    native_plan: {type: 'object', required: ['plan'], properties: {plan: {$ref: '#/$defs/NativePlanDocument'}}},
  }}}},
});
const path = 'contracts/target-protocol.schema.v0.9.1.json';
const bytes = JSON.stringify(wire, null, 2) + '\n';
if (process.argv.includes('--check')) assert.equal(readFileSync(path, 'utf8'), bytes, 'native execution protocol drift');
else writeFileSync(path, bytes);
