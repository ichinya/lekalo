#!/usr/bin/env node
// Issue #76: deterministic closed schema projections of the wire DTOs.
import { readFileSync, writeFileSync } from 'node:fs';
const version = '0.6.4';
const wire = readFileSync('crates/lekalo-core/src/ai_lint/wire.rs', 'utf8');
const camel = s => s.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
const object = properties => ({ type: 'object', additionalProperties: false, required: Object.keys(properties), properties });
const string = { type: 'string', minLength: 1, maxLength: 256, pattern: '^[A-Za-z0-9][A-Za-z0-9_.:/#@\\\\-]*$' };
const prose = { type: 'string', minLength: 1, maxLength: 2000, pattern: '^[^\\u0000-\\u001f\\u007f]+$' };
const digest = { type: 'string', pattern: '^sha256:[0-9a-f]{64}$', maxLength: 71 };
const path = { type: 'string', maxLength: 512, pattern: '^(?!.*(?:^|/)\\.{1,2}(?:/|$))(?!.*\\.(?:/|$))[a-z0-9.][a-z0-9._-]{0,63}(?:/[a-z0-9.][a-z0-9._-]{0,63})*$' };
const integer = { type: 'integer', minimum: 0, maximum: 10000000 };
const defs = {};
function type(t) {
  if (t === 'String') return string;
  if (t === 'bool') return { type: 'boolean' };
  if (t === 'u64') return integer;
  if (t === 'i64') return { type: 'integer', minimum: -10000000, maximum: 10000000 };
  if (t.startsWith('Vec<')) return { type: 'array', maxItems: 10000, items: type(t.slice(4, -1)) };
  if (t.startsWith('State<')) {
    const inner = t.slice(6, -1), name = `State${inner}`;
    defs[name] ??= { oneOf: [object({ state: { const: 'known' }, value: type(inner) }), ...['unknown', 'withheld', 'unsupported'].map(s => object({ state: { const: s } }))] };
    return { $ref: `#/$defs/${name}` };
  }
  return { $ref: `#/$defs/${t}` };
}
for (const [name, values] of Object.entries({ Mechanism: ['direct','observer','hook','magic'], Confidence: ['unknown', 'low', 'medium', 'high', 'exact'], Claim: ['structural', 'possible-behavior', 'verified-behavior'], Currency: ['current', 'stale', 'unknown'], Origin: ['explicit', 'extracted', 'inferred'], CoverageState: ['complete', 'partial', 'unknown', 'unsupported', 'withheld', 'disabled'], StepRole: ['binding', 'trigger', 'registration', 'callback', 'effect'], Kind: ['binding-candidates', 'dispatch', 'convention', 'effect', 'path', 'reflection', 'string-reference', 'default', 'native-edge', 'field-write'] })) defs[name] = { enum: values };
for (const match of wire.matchAll(/pub struct (\w+)\s*\{([^}]+)\}/g)) {
  const properties = {};
  for (const field of match[2].matchAll(/pub\s+(\w+):\s*([^,\n}]+)/g)) {
    const [_, name, fieldType] = field, key = camel(name);
    let schema = structuredClone(type(fieldType.trim()));
    if (['reason', 'alternative', 'message', 'semantics'].includes(key)) schema = prose;
    if (key === 'path') schema = path;
    if (['digest', 'fingerprint', 'artifactDigest', 'inputManifestDigest', 'conditionDigest', 'baselineRef', 'candidateRef', 'configRef', 'modelRef', 'irRef', 'profileRef'].includes(key)) schema = digest;
    if (['evidenceRefs', 'attachmentRefs'].includes(key)) schema = { type: 'array', maxItems: 10000, uniqueItems: true, items: digest };
    if (['witness', 'locations', 'activation', 'guards', 'candidates'].includes(key)) schema.maxItems = 32;
    if (schema.type === 'array' && ['scope', 'recipes', 'requiredCoverage', 'limitations', 'targets', 'operations', 'locations', 'guards'].includes(key)) schema.uniqueItems = true;
    if (['start', 'end', 'bytes', 'line', 'column', 'endLine', 'endColumn'].includes(key)) schema = { type: 'integer', minimum: ['line', 'column', 'endLine', 'endColumn'].includes(key) ? 1 : 0, maximum: 8388608 };
    if (key === 'disposition') schema = { enum: match[1] === 'Finding' ? ['active', 'waived'] : ['applied', 'expired', 'condition-changed', 'orphan', 'source-changed'] };
    if (key === 'severity') schema = match[1] === 'Finding' ? { enum: ['warning', 'info'] } : { oneOf: [object({ state: { const: 'known' }, value: { const: 'info' } }), ...['unknown', 'withheld', 'unsupported'].map(s => object({ state: { const: s } }))] };
    properties[key] = schema;
  }
  defs[match[1]] = object(properties);
}
for (const family of ['report', 'evidence', 'config', 'waivers', 'comparison']) {
  const root = structuredClone(defs[camel(family).replace(/^./, c => c.toUpperCase())]);
  root.properties.schemaVersion = { const: `lekalo/ai-lint-${family}/v${version}` };
  root.properties.identity = { const: `dev.lekalo.ai-lint-${family}@${version}` };
  const schema = { $schema: 'https://json-schema.org/draft/2020-12/schema', $id: `https://dev.lekalo/ai-lint-${family}.schema.v${version}.json`, title: `Lekalo AI lint ${family}`, ...root, $defs: defs };
  const filename = `contracts/ai-lint-${family}.schema.v${version}.json`;
  const bytes = JSON.stringify(schema, null, 2) + '\n';
  if (process.argv.includes('--check')) { if (readFileSync(filename, 'utf8') !== bytes) throw new Error(`schema drift: ${filename}`); }
  else writeFileSync(filename, bytes);
}
