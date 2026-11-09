#!/usr/bin/env node
// #147: public CLI, actual confined adapter; all app sources are synthetic.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdtempSync, readFileSync, realpathSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
assert(process.argv.length === 2 || (process.argv.length === 3 && process.argv[2] === '--validate-contracts'));
const contractMode = process.argv.includes('--validate-contracts');
const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const suffix = process.platform === 'win32' ? '.exe' : '';
const binary = process.env.LEKALO_BIN ?? join(repo, `target/debug/lekalo${suffix}`);
const adapter = process.env.LEKALO_PYTHON_ADAPTER_BIN ?? join(repo, `adapters/python/target/debug/lekalo-target-python${suffix}`);
const parent = realpathSync(tmpdir());
const root = mkdtempSync(join(parent, 'lekalo-public-fastapi-'));
const policy = { schema: 'lekalo/framework-policy', version: 1, providers: [{ id: 'fastapi', state: 'enabled' }] };
const profile = { id: 'standalone', mode: 'observed', target: 'python', readRoots: [{ kind: 'tree', path: 'src' }], exclusions: [], provenance: { origin: 'declared', revision: 'fastapi-public-gate', disposition: 'public-fixture' } };
function cli(args, expected = 0) {
  const result = spawnSync(binary, ['--json', ...args], { cwd: root, encoding: 'utf8', timeout: 120000, maxBuffer: 16 * 1024 * 1024 });
  assert.ifError(result.error); assert.equal(result.status, expected, `${args.join(' ')}\n${result.stdout}\n${result.stderr}`);
  return JSON.parse(result.stdout.trim() || result.stderr.trim());
}
function inventory(path) { return readdirSync(path, { withFileTypes: true }).sort((a,b) => a.name.localeCompare(b.name)).flatMap(e => e.isDirectory() ? inventory(join(path,e.name)).map(([p,d]) => [`${e.name}/${p}`,d]) : [[e.name,createHash('sha256').update(readFileSync(join(path,e.name))).digest('hex')]]); }
function contracts(index) {
  const require = createRequire(process.env.LEKALO_AJV_NODE_PATH ? `${process.env.LEKALO_AJV_NODE_PATH}/` : import.meta.url);
  assert.equal(require('ajv/package.json').version,'8.20.0');
  const Ajv = require('ajv/dist/2020.js').default;
  const ajv = new Ajv({strict:true,allErrors:true});
  const schema = name => JSON.parse(readFileSync(join(repo,`contracts/${name}.json`),'utf8'));
  const currentIndex = ajv.compile(schema('observed-index.schema.v1.2.0'));
  assert(currentIndex(index),JSON.stringify(currentIndex.errors));
  const request = {protocol:'lekalo.target/v1',protocol_version:'1.2.0',operation:'scan',project_root:'.',target:'python',profile:profile.id,request_id:`req-${'0'.repeat(64)}`};
  const raw = spawnSync(adapter,['--lekalo-project-profile-json',JSON.stringify(profile),'--lekalo-framework-policy-json',JSON.stringify(policy)],{cwd:root,input:JSON.stringify(request),encoding:'utf8',timeout:20000,maxBuffer:16*1024*1024});
  assert.ifError(raw.error); assert.equal(raw.status,0,raw.stderr); const wire = JSON.parse(raw.stdout);
  const currentWire = ajv.compile(schema('target-protocol.schema.v1.2.0'));
  assert(currentWire(wire),JSON.stringify(currentWire.errors));
  const evidence = ({identity_fields,...rest}) => ({...rest,...(identity_fields === undefined ? {} : {identityFields:identity_fields})});
  const scan = {schemaVersion:'lekalo/observed-scan/v1.2.0',adapter:index.adapter,project:index.project,revision:index.revision,target:index.target,profile:index.profile,
    symbols:index.symbols.map(s => ({id:s.id,kind:s.kind,stableKey:s.stable_key,location:s.location,fingerprint:s.fingerprint,mappingConfidence:s.provenance.confidence,evidence:evidence(s.evidence),candidates:s.candidates})),
    endpoints:index.endpoints.map(e => ({id:e.id,method:e.method,path:e.path,symbol:e.symbol,httpEvidence:e.http_evidence})),schemas:[],nativeSymbols:index.native_symbols,bindingEvidence:index.binding_evidence.map(b => ({...b,evidence:evidence(b.evidence)})),nativeUncertainty:index.native_uncertainty ?? []};
  const currentScan = ajv.compile(schema('observed-scan.schema.v1.2.0'));
  assert(currentScan(scan),JSON.stringify(currentScan.errors));
  const legacyIndex = structuredClone(index); legacyIndex.schema_version='lekalo/observed-index/v1.1.0';legacyIndex.identity='dev.lekalo.observed-index@1.1.0';
  assert.equal(ajv.compile(schema('observed-index.schema.v1.1.0'))(legacyIndex),false);
  const legacyScan=structuredClone(scan);legacyScan.schemaVersion='lekalo/observed-scan/v1.1.0';
  assert.equal(ajv.compile(schema('observed-scan.schema.v1.1.0'))(legacyScan),false);
  const legacyWire=structuredClone(wire);legacyWire.protocol_version='1.1.0';
  assert.equal(ajv.compile(schema('target-protocol.schema.v1.1.0'))(legacyWire),false);
  const carrier=wire.result.entries.find(e=>e.evidence.http_routes);
  for(const [key,value] of [['operation_id',null],['extra',true],['sources',Array(65).fill(carrier.evidence.http_routes[0].sources[0])]]) {
    const bad=structuredClone(wire);bad.result.entries.find(e=>e.evidence.http_routes).evidence.http_routes[0][key]=value;assert.equal(currentWire(bad),false,key);
  }
}
try {
  const fixture = join(repo, 'tests/fixtures/python-scan/fastapi');
  cpSync(join(fixture,'src'),join(root,'src'),{recursive:true});
  writeFileSync(join(root,'pyproject.toml'),'[project]\nname="fastapi-fixture"\nversion="0.0.0"\n');
  const sourceBefore = inventory(join(root,'src'));
  cli(['init','--adopt','--target','python','--project-id','fastapi_shop','--project','.']);
  if (!existsSync(join(root,'lekalo/modules/shop'))) cli(['module','new','shop','--project','.']);
  cpSync(join(fixture,'bindings.yaml'),join(root,'lekalo/modules/shop/bindings.yaml'));
  cpSync(join(fixture,'commands.yaml'),join(root,'lekalo/modules/shop/commands.yaml'));
  cpSync(join(fixture,'entities.yaml'),join(root,'lekalo/modules/shop/entities.yaml'));
  const modelBefore = inventory(join(root,'lekalo'));
  const scan = () => cli(['scan','--target','python','--profile','standalone','--project','.','--',adapter,'--lekalo-project-profile-json',JSON.stringify(profile),'--lekalo-framework-policy-json',JSON.stringify(policy)]);
  const cold = scan(); assert.equal(cold.endpoints,2);
  const indexPath = join(root,'.lekalo/import/observed/index.json');
  const bytes = readFileSync(indexPath); scan(); assert.deepEqual(readFileSync(indexPath),bytes);
  const index = JSON.parse(bytes); assert.equal(index.schema_version,'lekalo/observed-index/v1.2.0');
  if(contractMode) contracts(index);
  assert.deepEqual(index.endpoints.map(e => e.path).sort(),['/api/v1/items','/public/items']);
  assert(index.endpoints.every(e => e.http_evidence.handler_native === index.symbols.find(s => s.id === 'shop.handlers.list_items').stable_key));
  cli(['scan','--target','python','--profile','standalone','--project','.','--',adapter,'--lekalo-project-profile-json',JSON.stringify(profile)],1);
  assert.deepEqual(readFileSync(indexPath),bytes,'a generic scan cannot downgrade accepted HTTP custody');
  const proposal = () => cli(['bindings','propose','--project','.']).proposals.find(p => p.symbol === 'shop.handlers.list_items');
  const proposed = proposal(); assert(proposed); assert.equal(proposed.ambiguous,false);
  const mainPath = join(root,'src/shop/main.py'); const main = readFileSync(mainPath,'utf8');
  writeFileSync(mainPath,`${main}\n# registration source drift\n`);
  cli(['observe','confirm','shop.handlers.list_items','--project','.'],1);
  assert.deepEqual(readFileSync(indexPath),bytes,'legacy confirmation also refuses source drift without writing');
  writeFileSync(mainPath,main);
  cli(['bindings','confirm',proposed.proposal,'--project','.']); cli(['bindings','audit','--project','.']);
  cli(['validate','--strict','--project','.']);
  for (const command of [['observe','inspect'],['observe','impact'],['inspect'],['impact'],['context']]) cli([...command,'shop.handlers.list_items',...(command[0] === 'context' ? ['--budget','4000'] : []),'--project','.']);
  assert.deepEqual(inventory(join(root,'lekalo')),modelBefore); assert.deepEqual(inventory(join(root,'src')),sourceBefore);
  // The handler file is unchanged: only registration in main.py drifts.
  writeFileSync(mainPath,main.replace('/api','/changed'));
  cli(['bindings','audit','--project','.'],1); scan();
  const changed = proposal(); assert(changed); assert.notEqual(changed.proposal,proposed.proposal);
  const accepted = readFileSync(indexPath);
  writeFileSync(mainPath,`${readFileSync(mainPath,'utf8')}\n# changed after proposal review\n`);
  const refused = cli(['bindings','confirm',changed.proposal,'--project','.'],1);
  assert(refused.diagnostics.some(d => d.id === 'bindings.plan-mismatch')); assert.deepEqual(readFileSync(indexPath),accepted);
  scan(); cli(['bindings','confirm',proposal().proposal,'--project','.']); cli(['bindings','audit','--project','.']);
  // Unknown registration is in main.py, distinct from the handler's file.
  writeFileSync(mainPath,`${readFileSync(mainPath,'utf8')}\napp.add_api_route(dynamic_path,list_items)\n`);
  scan(); cli(['bindings','confirm',proposal().proposal,'--project','.']);
  const partial = cli(['observe','inspect','shop.handlers.list_items','--project','.']);
  assert.equal(partial.completeness,'incomplete');
  assert(partial.native_uncertainty.some(u => u.kind === 'framework-route' && u.path === 'src/shop/main.py'));
  // A default expression runs at definition time and can erase known routes.
  writeFileSync(mainPath,`${readFileSync(mainPath,'utf8')}\ndef clear_routes(value=app.routes.clear()): pass\n`);
  scan();
  const erasedAudit=cli(['bindings','audit','--project','.'],1);
  assert(erasedAudit.diagnostics.some(d=>d.id === 'observed.stale-binding' && d.data?.symbol === 'shop.handlers.list_items' && d.data?.detail === 'state-stale'));
  assert.deepEqual(inventory(join(root,'src')).find(([path])=>path === 'shop/handlers.py'),sourceBefore.find(([path])=>path === 'shop/handlers.py'));
  assert.equal(JSON.parse(readFileSync(indexPath)).endpoints.length,0);
  assert(!proposal(),'a definition-time mutation cannot reconfirm erased route evidence');
  // Removing registrations must not silently turn a confirmed Endpoint into an entity.
  writeFileSync(mainPath,'from fastapi import FastAPI\napp=FastAPI()\n');
  writeFileSync(join(root,'src/shop/routes.py'),'from .handlers import list_items\n');
  scan(); cli(['bindings','audit','--project','.'],1);
  assert(!proposal(), 'a disappeared route has no generic function reconfirmation');
  console.log(JSON.stringify({ ok:true, gate:'public-fastapi-scan', endpoints:cold.endpoints, canonicalPreserved:true, crossFileFreshness:true, staleConfirmationRefused:true, removedEndpointRefused:true, independentContracts:contractMode }));
} finally {
  const checked=realpathSync(root); assert.equal(dirname(checked),parent); assert(basename(checked).startsWith('lekalo-public-fastapi-'));
  rmSync(checked,{recursive:true,force:true,maxRetries:3});
}
