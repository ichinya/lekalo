#!/usr/bin/env node
// Issue #150: only the public CLI and manifested committed adapter qualify.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
import {cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {basename, dirname, join, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const binary = process.env.LEKALO_BIN ?? join(repo, 'target', 'debug', process.platform === 'win32' ? 'lekalo.exe' : 'lekalo');
assert.ok(existsSync(binary), 'build the checkout CLI first');
const adapter = join(repo, 'adapters', 'node-typescript', 'adapter.mjs');
const tempParent = realpathSync(tmpdir());
const work = mkdtempSync(join(tempParent, 'lekalo-public-scan-'));
const sha = bytes => 'sha256:' + createHash('sha256').update(bytes).digest('hex');
const require = createRequire((process.env.LEKALO_AJV_NODE_PATH ?? repo) + '/');
assert.equal(require('ajv/package.json').version, '8.20.0');
const Ajv = require('ajv/dist/2020').default;
const validate = new Ajv({strict: true, allErrors: true}).compile(JSON.parse(readFileSync(join(repo, 'contracts', 'target-protocol.schema.v0.9.0.json'), 'utf8')));
const digest = 'sha256:' + 'a'.repeat(64);
const vector = {protocol: 'lekalo.target/v1', protocol_version: '0.9.0', operation: 'scan', status: 'ok', request_id: 'req-' + '0'.repeat(64), evidence: {adapter: {id: 'example', version: '1.0.0', digest}, scan_profile_digest: digest}, result: {entries: [{path: 'src/score.ts', kind: 'entity', detail: 'x'.repeat(200), evidence: {source_digest: digest}}]}};
assert.ok(validate(vector), JSON.stringify(validate.errors));
let schemaRefusals = 0;
for (const mutation of [
  v => {delete v.result.entries[0].evidence;},
  v => {delete v.result.entries[0].evidence.source_digest;},
  v => {delete v.evidence.scan_profile_digest;},
  v => {v.result.entries[0].detail = 'x'.repeat(2049);},
  v => {v.result.entries[0].evidence.invented = true;},
  v => {v.operation = 'validate';},
]) {const bad = structuredClone(vector); mutation(bad); assert.equal(validate(bad), false); schemaRefusals++;}

// Fault injection belongs only to negative lanes. The qualifying positive
// lanes always execute the untouched committed manifested bundle.
function faultPackage(root, profile, mode, path) {
  const dir = join(work, `fault-${basename(root)}-${mode}`);
  mkdirSync(dir);
  const manifest = JSON.parse(readFileSync(join(repo, 'adapters/node-typescript/adapter.manifest.json'), 'utf8'));
  const capabilities = {};
  for (const version of manifest.compatibility.protocolVersions) {
    const request = {protocol: 'lekalo.target/v1', protocol_version: version, operation: 'describe', project_root: '.', request_id: 'req-' + '0'.repeat(64)};
    const run = spawnSync(process.execPath, [adapter, '--lekalo-project-profile-json', JSON.stringify(profile)], {cwd: root, input: JSON.stringify(request), encoding: 'utf8', timeout: 30000});
    assert.equal(run.status, 0, run.stderr);
    capabilities[version] = JSON.parse(run.stdout).capabilities;
  }
  const sourceDigest = sha(readFileSync(join(root, path)));
  const code = `import fs from 'node:fs'; import {createHash} from 'node:crypto';
const r=JSON.parse(fs.readFileSync(0,'utf8')), mode=${JSON.stringify(mode)};
const identity={id:'lekalo-target-node-typescript',version:${JSON.stringify(manifest.adapter.version)},digest:'sha256:'+createHash('sha256').update(fs.readFileSync(new URL(import.meta.url))).digest('hex')};
const out={protocol:r.protocol,protocol_version:r.protocol_version,operation:r.operation,request_id:r.request_id,status:'ok',evidence:{adapter:identity}};
if(r.operation==='describe'){out.capabilities=${JSON.stringify(capabilities)}[r.protocol_version];out.capabilities.adapter=identity;}
else{out.evidence.scan_profile_digest=${JSON.stringify(digest)};out.result={entries:[{path:${JSON.stringify(path)},kind:'entity',detail:JSON.stringify({s:'public_core.score',n:'ts-fault',l:1,q:'medium'}),evidence:{source_digest:${JSON.stringify(sourceDigest)}}}]};if(mode==='loss')delete out.result.entries[0].evidence;if(mode==='forged')out.result.entries[0].evidence.source_digest='sha256:'+'0'.repeat(64);if(mode==='broken'){process.stdout.write('{broken');process.exit(0);}}
process.stdout.write(JSON.stringify(out));`;
  writeFileSync(join(dir, 'adapter.mjs'), code);

  const bytes = Buffer.from(code);
  manifest.integrity.files = [{path: 'adapter.mjs', digest: sha(bytes), bytes: bytes.length}];
  manifest.integrity.packageDigest = 'sha256:' + '0'.repeat(64);
  const canonical = v => Array.isArray(v) ? '[' + v.map(canonical).join(',') + ']' : v && typeof v === 'object' ? '{' + Object.keys(v).sort().map(k => JSON.stringify(k) + ':' + canonical(v[k])).join(',') + '}' : JSON.stringify(v);
  const framed = (name, content) => {const n = Buffer.alloc(8); n.writeBigUInt64BE(BigInt(content.length)); return Buffer.concat([Buffer.from(name), Buffer.from([0]), n, Buffer.from([0]), content]);};
  const parts = [framed('adapter.mjs', bytes), framed('adapter.manifest.json', Buffer.from(canonical(manifest)))].map(p => {const n = Buffer.alloc(8); n.writeBigUInt64BE(BigInt(p.length)); return Buffer.concat([n, p]);});
  manifest.integrity.packageDigest = sha(Buffer.concat(parts));
  writeFileSync(join(dir, 'adapter.manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
  return join(dir, 'adapter.mjs');
}
function inventory(root, prefix = '') {
  return readdirSync(join(root, prefix), {withFileTypes: true}).sort((a, b) => a.name.localeCompare(b.name))
    .flatMap(e => {const p = prefix ? `${prefix}/${e.name}` : e.name; return e.isDirectory() ? inventory(root, p) : [[p, sha(readFileSync(join(root, p)))]];});
}
function cli(root, args, expected = 0) {
  const run = spawnSync(binary, ['--json', ...args, '--project', '.'], {cwd: root, encoding: 'utf8', timeout: 120000, maxBuffer: 16 * 1024 * 1024});
  assert.equal(run.status, expected, `${args.slice(0, 4).join(' ')}: ${run.stdout}\n${run.stderr}\n${run.error ?? ''}`);
  for (const text of [run.stdout, run.stderr]) {try {return JSON.parse(text.trim());} catch {}}
  throw Error('missing JSON envelope');
}
const results = [];
try {
  for (const kind of ['standalone', 'workspace']) {
    const root = join(work, kind);
    cpSync(join(repo, 'tests', 'fixtures', 'public-typescript-scan', kind), root, {recursive: true});
    const original = inventory(root);
    const profile = JSON.parse(readFileSync(join(root, 'profile.json'), 'utf8'));
    const adopt = cli(root, ['init', '--adopt', '--project-id', `public_${kind}`]);
    assert.equal(adopt.status, 'valid');
    for (const name of kind === 'standalone' ? ['public_core'] : ['public_core', 'public_web']) {
      if (!existsSync(join(root, 'lekalo', 'modules', name))) cli(root, ['module', 'new', name]);
    }
    // Explicitly authored semantic intent, not a fabricated scanner result
    // or an automatic inference/promotion of business semantics.
    cpSync(join(repo, 'tests', 'fixtures', 'public-typescript-scan', 'model', 'entities.yaml'), join(root, 'lekalo', 'modules', 'public_core', 'entities.yaml'));
    // Trailing adapter argv consume all following tokens, so the selector
    // precedes the program and is never accidentally passed to the adapter.
    function scan(selected = profile, {entry = adapter, expected = 0, reason} = {}) {
      const args = ['--json', 'scan', '--target', 'node-typescript', '--profile', 'standalone', '--project', '.', process.execPath, entry];
      if (selected !== null) args.push('--lekalo-project-profile-json', JSON.stringify(selected));
      const run = spawnSync(binary, args, {cwd: root, encoding: 'utf8', timeout: 120000, maxBuffer: 16 * 1024 * 1024});
      assert.equal(run.status, expected, `public ${kind} scan: ${run.stdout}\n${run.stderr}`);
      const envelope = JSON.parse(run.stdout.trim() || run.stderr.trim());
      if (reason) assert.ok(envelope.diagnostics.some(d => d.id === reason), JSON.stringify(envelope));
      return envelope;
    }
    const cold = scan();
    const indexPath = join(root, '.lekalo', 'import', 'observed', 'index.json');
    const beforeRepeat = readFileSync(indexPath);
    const repeat = scan();
    assert.equal(cold.revision, repeat.revision);
    assert.equal(cold.profile_digest, repeat.profile_digest);
    assert.match(cold.profile_digest, /^sha256:[a-f0-9]{64}$/);
    assert.deepEqual(readFileSync(indexPath), beforeRepeat, 'repeated scan preserves canonical index bytes');
    assert.ok(cold.symbols >= 2);
    const proposals = cli(root, ['bindings', 'propose']);
    assert.ok(proposals.proposals.length >= 2);
    for (const proposal of proposals.proposals) {
      assert.equal(proposal.ambiguous, false);
      cli(root, ['bindings', 'confirm', proposal.proposal]);
    }
    cli(root, ['bindings', 'audit']);
    cli(root, ['validate', '--strict']);
    const inspected = cli(root, ['observe', 'inspect', 'public_core.score']);
    cli(root, ['inspect', 'public_core.score']);
    cli(root, ['impact', 'public_core.score']);
    cli(root, ['observe', 'impact', 'public_core.score']);
    cli(root, ['context', 'public_core.score', '--budget', '4000']);
    const stableIndex = readFileSync(indexPath);
    const source = kind === 'standalone' ? 'src/score.ts' : 'packages/core/src/score.ts';
    const refused = action => {action(); assert.deepEqual(readFileSync(indexPath), stableIndex, 'refusal leaves accepted index intact');};
    refused(() => scan({...profile, id: 'wrong-profile'}, {expected: 3, reason: 'adapter.manifest-mismatch'}));
    refused(() => scan({...profile, readRoots: [...profile.readRoots, {kind: 'tree', path: 'forbidden'}]}, {expected: 3, reason: 'adapter.manifest-mismatch'}));
    refused(() => scan(null, {expected: 4, reason: 'target.capability-unsupported'}));
    const sourceBytes = readFileSync(join(root, source));
    writeFileSync(join(root, source), Buffer.concat([sourceBytes, Buffer.from('\nimport "unavailable-private-package";\n')]));
    refused(() => scan(profile, {expected: 1, reason: 'target.operation-failed'}));
    writeFileSync(join(root, source), sourceBytes);
    writeFileSync(join(root, source), Buffer.concat([sourceBytes, Buffer.from(`\nexport function ${'f'.repeat(193)}(): number { return 1; }\n`)]));
    refused(() => scan(profile, {expected: 1, reason: 'observed.scan-invalid'}));
    writeFileSync(join(root, source), sourceBytes);
    const directory = dirname(source);
    for (let i = 0; i < 9; i++) writeFileSync(join(root, directory, `ref${i}.ts`), `export const ref${i} = ${i};\n`);
    writeFileSync(join(root, source), Buffer.concat([sourceBytes, Buffer.from(Array.from({length: 9}, (_, i) => `\nimport { ref${i} } from "./ref${i}.js"; export const use${i} = ref${i};`).join(''))]));
    refused(() => scan(profile, {expected: 1, reason: 'target.operation-failed'}));
    writeFileSync(join(root, source), sourceBytes);
    for (let i = 0; i < 9; i++) rmSync(join(root, directory, `ref${i}.ts`));
    writeFileSync(join(root, source), Buffer.concat([sourceBytes, Buffer.from('\nexport class carrier { field = 1; method(): number { return this.field; } }\n')]));
    refused(() => scan(profile, {expected: 1, reason: 'target.operation-failed'}));
    writeFileSync(join(root, source), sourceBytes);
    for (const mode of ['loss', 'forged', 'broken']) {
      const entry = faultPackage(root, profile, mode, source);
      refused(() => scan(profile, {entry, expected: mode === 'forged' ? 1 : 4, reason: mode === 'forged' ? 'observed.scan-invalid' : 'target.response-invalid'}));
    }
    const changedProfile = scan({...profile, provenance: {...profile.provenance, revision: 'public-scan-v2'}});
    assert.notEqual(changedProfile.profile_digest, cold.profile_digest);
    assert.notEqual(changedProfile.revision, cold.revision, 'revision pins profile provenance');
    for (const [path, digest] of original) assert.equal(sha(readFileSync(join(root, path))), digest, `source unchanged: ${path}`);
    results.push({kind, symbols: cold.symbols, profileDigest: cold.profile_digest, confirmed: proposals.proposals.length, refusalLanes: 10, binding: inspected.binding});
  }
  console.log(JSON.stringify({ok: true, gate: 'public-typescript-scan', publicCli: true, manifested: true, schemaRefusals, results}));
} finally {
  assert.equal(dirname(work), tempParent);
  assert.ok(basename(work).startsWith('lekalo-public-scan-'));
  assert.equal(realpathSync(work), work);
  rmSync(work, {recursive: true, force: true});
}
