#!/usr/bin/env node
// Cross-platform runtime/packaging gates. No project commands or install scripts.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
import {cpSync, copyFileSync, mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, chmodSync, existsSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, dirname, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const root = join(repo, 'adapters/node-typescript');
const binary = join(repo, 'target/debug/examples', process.platform === 'win32' ? 'typescript-native-contract.exe' : 'typescript-native-contract');
const cli = join(repo, 'target/debug', process.platform === 'win32' ? 'lekalo.exe' : 'lekalo');
const hash = bytes => 'sha256:' + createHash('sha256').update(bytes).digest('hex');
const require = createRequire((process.env.LEKALO_AJV_NODE_PATH ?? repo) + '/');
assert.equal(require('ajv/package.json').version, '8.20.0');
const Ajv = require('ajv/dist/2020').default;
const schema = name => new Ajv({strict: true, allErrors: true}).compile(JSON.parse(readFileSync(join(repo, 'contracts', name))));
const validate = schema('compiler-runtime.schema.v1.3.0.json');
const protocol = schema('target-protocol.schema.v1.3.0.json');
const bundle = JSON.parse(readFileSync(join(root, 'runtime-bundle.json')));
assert.ok(validate(bundle), JSON.stringify(validate.errors));
const platform = `${process.platform}-${process.arch}`;
const selected = bundle.compilers.find(row => row.platform === platform);
assert.ok(selected, 'a qualified native compiler is required');
const pins = JSON.parse(readFileSync(join(root, 'compiler-resources.json')));
const identity = {id: 'runtime-schema-control', version: '1.3.0', digest: 'sha256:' + 'a'.repeat(64)};
const response = {protocol: 'lekalo.target/v1', protocol_version: '1.3.0', operation: 'describe', request_id: 'req-' + 'b'.repeat(64),
  status: 'ok', evidence: {adapter: identity}, capabilities: {adapter: identity, protocol_versions: ['1.3.0'],
    operations: ['describe'], transports: ['stdin'], read_scopes: [], write_scopes: [], progress: false}};
assert.ok(protocol(response), JSON.stringify(protocol.errors)); // Ordinary adapters have no compiler component.
response.evidence.compiler_runtime = {schema_version: bundle.schema_version, kind: selected.kind, version: selected.version,
  platform, digest: selected.resource.digest, libraries_digest: `sha256:${pins.standardLibraries.sha256}`};
assert.ok(protocol(response), JSON.stringify(protocol.errors));
for (const change of [value => delete value.digest, value => value.version = '6.0.0', value => value.platform = 'win32-arm64', value => value.libraries_digest = 'sha256:invalid']) {
  const mutated = structuredClone(response); change(mutated.evidence.compiler_runtime); assert.equal(protocol(mutated), false);
}
const predecessor = schema('target-protocol.schema.v1.1.0.json');
const oldResponse = structuredClone(response); oldResponse.protocol_version = '1.1.0'; oldResponse.capabilities.protocol_versions = ['1.1.0'];
assert.equal(predecessor(oldResponse), false, 'runtime provenance must never enter a frozen predecessor');
delete oldResponse.evidence.compiler_runtime; assert.ok(predecessor(oldResponse), JSON.stringify(predecessor.errors));
assert.ok(existsSync(binary), 'build --example typescript-native-contract first');
assert.ok(existsSync(cli), 'build the checkout CLI first');
for (const change of [value => value.compilers.pop(), value => value.compilers[0].platform = value.compilers[1].platform,
  value => value.libraries.pop(), value => value.entry.path = '../entry.mjs', value => value.extra = true]) {
  const mutated = structuredClone(bundle); change(mutated); assert.equal(validate(mutated), false, JSON.stringify(mutated.entry));
}
const work = mkdtempSync(join(tmpdir(), 'lekalo-ts7-runtime-'));
const artifacts = join(work, 'package'); mkdirSync(artifacts);
try {
  // A native API flag is never an authority to copy an identically named
  // unscoped project file into the compiler's runtime view.
  writeFileSync(join(work, '--api'), 'POISON: project data is not a compiler script');
  for (const path of ['adapter.mjs', 'adapter.manifest.json', 'runtime-bundle.json', 'typescript-native-notices.txt']) copyFileSync(join(root, path), join(artifacts, path));
  for (const path of [selected.resource.path, 'runtime/libraries']) {
    mkdirSync(dirname(join(artifacts, path)), {recursive: true}); cpSync(join(root, path), join(artifacts, path), {recursive: true});
  }
  const forged = `import fs from 'node:fs'; import {createHash} from 'node:crypto';
const request=JSON.parse(fs.readFileSync(0,'utf8'));
const adapter={id:'unattested-runtime',version:'1.3.0',digest:'sha256:'+createHash('sha256').update(fs.readFileSync(new URL(import.meta.url))).digest('hex')};
const evidence={adapter};
if(request.protocol_version==='1.3.0')evidence.compiler_runtime=${JSON.stringify(response.evidence.compiler_runtime)};
process.stdout.write(JSON.stringify({protocol:request.protocol,protocol_version:request.protocol_version,operation:request.operation,request_id:request.request_id,status:'ok',evidence,
capabilities:{adapter,protocol_versions:['0.3.2','1.3.0'],operations:['describe'],transports:['stdin'],read_scopes:[],write_scopes:[],progress:false}}));`;
  writeFileSync(join(artifacts, 'forged.mjs'), forged);
  const run = (mode, entry, expected) => {
    const result = spawnSync(binary, [mode, process.execPath, entry], {cwd: work, encoding: 'utf8', timeout: 20000, windowsHide: true, maxBuffer: 1024 * 1024});
    assert.equal(result.status, 0, `${mode}: ${result.stdout}\n${result.stderr}`);
    const receipt = JSON.parse(result.stdout); assert.equal(receipt.outcome, expected, JSON.stringify(receipt));
    assert.ok(receipt.elapsed_ms < 12000, 'peer cleanup must finish within the host deadline'); return receipt;
  };
  const receipts = [run('describe', join(artifacts, 'adapter.mjs'), 'ok')];
  assert.equal(receipts[0].version, '1.3.0');
  receipts.push(run('forged', join(artifacts, 'forged.mjs'), 'provenance-refused'));
  // Missing/tampered transitive assets refuse during package admission, before
  // describe can start either code component.
  const packageCheck = () => spawnSync(cli, ['--json', 'adapter', 'install', `path:${artifacts}`, '--dry-run', '--project', '.'], {cwd: work, encoding: 'utf8', timeout: 20000, windowsHide: true});
  const installedControl = packageCheck();
  assert.equal(installedControl.status, 0, installedControl.stdout + installedControl.stderr);
  for (const path of [selected.resource.path, 'runtime/libraries/lib.d.ts']) {
    const target = join(artifacts, path), original = readFileSync(target);
    chmodSync(target, 0o600); const altered = Buffer.from(original); altered[altered.length - 1] ^= 1; writeFileSync(target, altered);
    const result = packageCheck(); assert.notEqual(result.status, 0); assert.match(result.stdout + result.stderr, /adapter\.checksum-mismatch/);
    writeFileSync(target, original);
  }
  // Fault entry opts into the SAME explicit runtime closure; compiler bytes
  // remain pinned. Only the synthetic producer behavior changes per case.
  for (const mode of ['timeout', 'cancel', 'cancel-before', 'output', 'request', 'peer']) {
    const code = '#!/usr/bin/env node\n// lekalo-runtime-bundle: runtime-bundle.json\n' +
      (mode === 'output' ? 'import fs from "node:fs"; fs.writeSync(2,Buffer.alloc(2048,120)); setInterval(()=>{},1000);' :
        mode === 'peer' ? 'setTimeout(()=>{process.stderr.write("{}");},500);' : 'setInterval(()=>{},1000);');
    writeFileSync(join(artifacts, 'adapter.mjs'), code);
    const document = structuredClone(bundle); document.entry.digest = hash(code); document.entry.bytes = Buffer.byteLength(code);
    if (mode === 'peer') {
      const harmless = process.platform === 'win32' ? join(process.env.SystemRoot, 'System32/where.exe') : '/usr/bin/true';
      const bytes = readFileSync(harmless); document.compilers.find(row => row.platform === platform).resource = {...selected.resource, digest: hash(bytes), bytes: bytes.length};
      writeFileSync(join(artifacts, selected.resource.path), bytes);
    }
    writeFileSync(join(artifacts, 'runtime-bundle.json'), JSON.stringify(document));
    receipts.push(run(mode, join(artifacts, 'adapter.mjs'), ({timeout: 'timeout', cancel: 'cancelled', 'cancel-before': 'cancelled', output: 'output-limit', request: 'request-size', peer: 'peer-failure'})[mode]));
  }
  console.log(JSON.stringify({ok: true, gate: 'typescript-native-runtime', protocol: '1.3.0', platform, schemaRefusals: 5,
    tamperRefusals: 2, provenanceRefusals: 5, receipts}));
} finally { rmSync(work, {recursive: true, force: true}); }
