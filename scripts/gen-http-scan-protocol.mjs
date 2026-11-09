#!/usr/bin/env node
// #147: versioned transport facts; all published predecessors remain immutable.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
const version = '1.2.0';
const ref = name => ({ $ref: `#/$defs/${name}` });
const text = maxLength => ({ type: 'string', minLength: 1, maxLength, pattern: '^[^\\u0000-\\u001f\\u007f-\\u009f]+$' });
const sha = { type: 'string', pattern: '^sha256:[a-f0-9]{64}$' };
function definitions(path) {
  return {
    httpRouteSource: { type: 'object', additionalProperties: false, required: ['path', 'source_digest'], properties: { path, source_digest: sha } },
    httpRouteEvidence: { type: 'object', additionalProperties: false,
      required: ['provider', 'rules_revision', 'policy_digest', 'method', 'path', 'application_native', 'handler_native', 'registration_path', 'registration_line', 'sources', 'confidence'],
      properties: {
        provider: { type: 'string', maxLength: 64, pattern: '^[a-z][a-z0-9-]*$' },
        rules_revision: text(64), policy_digest: sha,
        method: { enum: ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS', 'TRACE'] },
        path: { ...text(2048), pattern: '^/[^\\u0000-\\u001f\\u007f-\\u009f]*$' },
        application_native: text(256), handler_native: text(256), operation_id: text(256),
        registration_path: path, registration_line: { type: 'integer', minimum: 1, maximum: 1000000 },
        sources: { type: 'array', minItems: 1, maxItems: 64, items: ref('httpRouteSource') },
        confidence: { enum: ['exact', 'high', 'medium', 'low', 'unknown'] },
      } },
  };
}
function save(path, value) {
  const bytes = JSON.stringify(value, null, 2) + '\n';
  if (process.argv.includes('--check')) assert.equal(readFileSync(path, 'utf8'), bytes, `${path} drift`);
  else writeFileSync(path, bytes);
}
const wire = JSON.parse(readFileSync('contracts/target-protocol.schema.v1.1.0.json', 'utf8'));
wire.$id = `https://dev.lekalo/target-protocol.schema.v${version}.json`;
wire.description += ' Successor 1.2.0 carries bounded opt-in HTTP registration evidence bound to actual native handlers, application identities, trusted framework policy and complete source attachments. Dynamic framework uncertainty is explicit. This does not author canonical Endpoint semantics.';
wire.$defs.protocolVersion = { const: version };
Object.assign(wire.$defs, definitions(wire.$defs.scanEntry.properties.path));
wire.$defs.scanEntryEvidence.properties.http_routes = { type: 'array', minItems: 1, maxItems: 64, items: ref('httpRouteEvidence') };
wire.$defs.nativeUncertainty.properties.kind = { enum: ['any-surface', 'framework-route'] };
wire.$defs.responseEnvelope.allOf.push({
  if: { type: 'object', properties: { operation: { const: 'scan' }, status: { const: 'ok' } }, required: ['operation', 'status'] },
  then: {},
  else: { type: 'object', properties: { result: { type: 'object', properties: { entries: { type: 'array', items: {
    type: 'object', properties: { evidence: { type: 'object', properties: { http_routes: false } } },
  } } } } } },
});
save(`contracts/target-protocol.schema.v${version}.json`, wire);
for (const kind of ['scan', 'index']) {
  const schema = JSON.parse(readFileSync(`contracts/observed-${kind}.schema.v1.1.0.json`, 'utf8'));
  schema.$id = schema.$id.replace('v1.1.0', `v${version}`);
  schema.description += ' HTTP evidence retains native-handler and registration-source custody under an explicit framework policy. Missing or dynamic route information remains uncertainty.';
  if (kind === 'scan') schema.properties.schemaVersion.const = `lekalo/observed-scan/v${version}`;
  else {
    schema.properties.schema_version.const = `lekalo/observed-index/v${version}`;
    schema.properties.identity.const = `dev.lekalo.observed-index@${version}`;
  }
  Object.assign(schema.$defs, definitions({ type: 'string', minLength: 1, maxLength: 512 }));
  schema.$defs.nativeUncertainty.properties.kind = { enum: ['any-surface', 'framework-route'] };
  const endpoint = schema.$defs.endpoint;
  assert.ok(endpoint?.properties?.method, 'endpoint schema not found');
  endpoint.properties.method = wire.$defs.httpRouteEvidence.properties.method;
  endpoint.properties.path = wire.$defs.httpRouteEvidence.properties.path;
  endpoint.properties[kind === 'scan' ? 'httpEvidence' : 'http_evidence'] = ref('httpRouteEvidence');
  save(`contracts/observed-${kind}.schema.v${version}.json`, schema);
}
console.log(JSON.stringify({ ok: true, version, contracts: 3, check: process.argv.includes('--check') }));
