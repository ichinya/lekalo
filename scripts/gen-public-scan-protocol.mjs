#!/usr/bin/env node
// Issue #150: additive scan-detail capacity, never edit published predecessors.
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync} from 'node:fs';
const wire = JSON.parse(readFileSync('contracts/target-protocol.schema.v0.8.0.json', 'utf8'));
wire.$id = 'https://dev.lekalo/target-protocol.schema.v0.9.0.json';
wire.description += ' Successor 0.9.0 retains the 0.8.0 wire surface and admits scan entry detail up to 2048 Unicode code points, so complete semantic/native/test binding evidence fits without truncation. Other operations retain the 128-character bound. Successful scans require exact source and normalized launch-profile digests. Watch selection still requires an explicit adapter capability declaration.';
wire.$defs.protocolVersion = {const: '0.9.0'};
wire.$defs.scanEntry.properties.detail = {...wire.$defs.detailToken, maxLength: 2048};
const sha = {type: 'string', pattern: '^sha256:[a-f0-9]{64}$'};
wire.$defs.evidence.properties.scan_profile_digest = sha;
wire.$defs.scanEntryEvidence.properties.source_digest = sha;
wire.$defs.responseEnvelope.allOf.push({
  if: {type: 'object', properties: {operation: {const: 'scan'}}, required: ['operation']},
  then: {},
  else: {type: 'object', properties: {result: {type: 'object', properties: {
    entries: {type: 'array', items: {type: 'object', properties: {detail: {type: 'string', maxLength: 128}}}},
  }}}},
});
wire.$defs.responseEnvelope.allOf.push({
  if: {type: 'object', properties: {operation: {const: 'scan'}, status: {const: 'ok'}}, required: ['operation', 'status']},
  then: {type: 'object', required: ['result'], properties: {
    evidence: {type: 'object', required: ['scan_profile_digest'], properties: {scan_profile_digest: sha}},
    result: {type: 'object', required: ['entries'], properties: {entries: {type: 'array', items: {
      type: 'object', required: ['evidence'], properties: {evidence: {
        type: 'object', required: ['source_digest'], properties: {source_digest: sha},
      }},
    }}}},
  }},
  else: {type: 'object', properties: {result: {type: 'object', properties: {entries: {type: 'array', items: {
    type: 'object', properties: {evidence: {type: 'object', not: {type: 'object', properties: {source_digest: {}}, required: ['source_digest']}}},
  }}}}, evidence: {type: 'object', not: {
    type: 'object', properties: {scan_profile_digest: {}}, required: ['scan_profile_digest'],
  }}}},
});
const path = 'contracts/target-protocol.schema.v0.9.0.json';
const bytes = JSON.stringify(wire, null, 2) + '\n';
if (process.argv.includes('--check')) assert.equal(readFileSync(path, 'utf8'), bytes, 'public scan protocol drift');
else writeFileSync(path, bytes);
