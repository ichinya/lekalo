import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, test } from 'node:test';

const executable = process.env.LEKALO_PYTHON_ADAPTER_BIN ?? fileURLToPath(new URL(`../target/debug/lekalo-target-python${process.platform === 'win32' ? '.exe' : ''}`, import.meta.url));
const parent = realpathSync(tmpdir());
const scratch = mkdtempSync(join(parent, 'lekalo-fastapi-'));
after(() => { const checked = realpathSync(scratch); assert.equal(dirname(checked), parent); assert(basename(checked).startsWith('lekalo-fastapi-')); rmSync(checked, { recursive: true, force: true, maxRetries: 3 }); });
const policy = { schema: 'lekalo/framework-policy', version: 1, providers: [{ id: 'fastapi', state: 'enabled' }] };
const profile = { id: 'standalone', mode: 'observed', target: 'python', readRoots: [{ kind: 'tree', path: 'src' }], exclusions: [], provenance: { origin: 'declared', revision: 'fastapi-synthetic', disposition: 'public-fixture' } };
function run(files, { enabled = true, operation = 'scan', version = enabled ? '1.2.0' : '1.1.0' } = {}) {
  const cwd = mkdtempSync(join(scratch, 'case-'));
  for (const [name, source] of Object.entries(files)) { const path = join(cwd, 'src', name); mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, source); }
  const request = { protocol: 'lekalo.target/v1', protocol_version: version, operation, project_root: '.', target: 'python', profile: profile.id };
  request.request_id = `req-${createHash('sha256').update(JSON.stringify(request)).digest('hex')}`;
  const args = ['--lekalo-project-profile-json', JSON.stringify(profile)];
  if (enabled) args.push('--lekalo-framework-policy-json', JSON.stringify(policy));
  const result = spawnSync(executable, args, { cwd, input: JSON.stringify(request), encoding: 'utf8', timeout: 20000, maxBuffer: 16 * 1024 * 1024 });
  assert.ifError(result.error); assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
}
const routes = result => result.result.entries.flatMap(e => e.evidence.http_routes ?? []);
const unknowns = result => result.evidence.native_uncertainty.filter(e => e.kind === 'framework-route');
const simple = `from fastapi import FastAPI\napp = FastAPI()\n@app.get('/items', operation_id='list_items')\nasync def items():\n    return []\n`;

test('explicit policy negotiates HTTP successor; opt-out preserves generic scanner', () => {
  const files = { 'shop.py': simple };
  const generic = run(files, { enabled: false });
  assert.equal(generic.status, 'ok'); assert.equal(routes(generic).length, 0);
  assert(generic.result.entries.every(e => e.kind !== 'endpoint'));
  const disabled = run(files, { enabled: false, operation: 'describe', version: '0.3.2' });
  const enabled = run(files, { operation: 'describe', version: '0.3.2' });
  assert(!JSON.stringify(disabled.capabilities).includes('1.2.0')); assert(JSON.stringify(enabled.capabilities).includes('1.2.0'));
  assert.equal(run(files, { version: '1.1.0' }).error.class, 'unsupported');
  assert.equal(run(files, { enabled: false, version: '1.2.0' }).error.class, 'invalid');
  assert.equal(run(files, { enabled: false, operation: 'describe', version: '1.2.0' }).error.class, 'invalid');
});

test('decorator facts attach to the actual native function with complete provenance', () => {
  const result = run({ 'shop.py': simple }); assert.equal(result.status, 'ok');
  const [route] = routes(result); assert(route); assert.equal(routes(result).length, 1);
  assert.equal(route.method, 'GET'); assert.equal(route.path, '/items'); assert.equal(route.operation_id, 'list_items');
  assert.equal(route.provider, 'fastapi'); assert.equal(route.confidence, 'high');
  const entry = result.result.entries.find(e => e.evidence.http_routes);
  const handler = entry.evidence.native_symbols.find(s => s.native === route.handler_native);
  assert.equal(handler.qualified_name, 'shop.items'); assert.equal(handler.signature, entry.evidence.signature);
  const app = result.result.entries.flatMap(e => e.evidence.native_symbols).find(s => s.native === route.application_native);
  assert.equal(app.qualified_name, 'shop.app'); assert.equal(entry.kind, 'endpoint');
  assert.deepEqual(route.sources.map(s => s.path), ['src/shop.py']); assert.match(route.sources[0].source_digest, /^sha256:[a-f0-9]{64}$/);
  assert.equal(unknowns(result).length, 0);
  assert.deepEqual(run({ 'shop.py': simple }), result, 'byte-equivalent repeated evidence');
});

test('aliases, explicit registration and nested/repeated live includes compose static prefixes', () => {
  const result = run({
    '__init__.py': '',
    'handlers.py': `def list_items():\n    return []\n`,
    'routes.py': `from fastapi import APIRouter as Router\nfrom .handlers import list_items as actual\nBASE='/items'\nrouter=Router(prefix=BASE)\nrouter.add_api_route('/all', actual, methods=['GET','POST'], operation_id='all_items')\n`,
    'shop.py': `import fastapi as api\nfrom .routes import router as child\napp=api.FastAPI()\nouter=api.APIRouter(prefix='/v1')\napp.include_router(outer, prefix='/api')\nouter.include_router(child, prefix='/catalog')\napp.include_router(child, prefix='/direct')\n`,
  });
  assert.equal(result.status, 'ok');
  assert.deepEqual(routes(result).map(r => [r.method,r.path]), [
    ['GET','/api/v1/catalog/items/all'], ['GET','/direct/items/all'],
    ['POST','/api/v1/catalog/items/all'], ['POST','/direct/items/all'],
  ]);
  assert(routes(result).every(r => r.sources.some(s => s.path === 'src/handlers.py') && r.sources.some(s => s.path === 'src/routes.py') && r.sources.some(s => s.path === 'src/shop.py')));
  assert.equal(unknowns(result).length, 0);
});

test('routes added after inclusion remain visible and decorator method lists are literal', () => {
  const result = run({ 'shop.py': `from fastapi import FastAPI, APIRouter\napp=FastAPI()\nr=APIRouter()\napp.include_router(r, prefix='/v2')\n@r.api_route('/late', methods=['patch','DELETE'])\ndef late(): pass\n` });
  assert.deepEqual(routes(result).map(r => [r.method,r.path]), [['DELETE','/v2/late'],['PATCH','/v2/late']]);
});

test('local lookalikes, reassignment, local package shadows and unknown decorators never invent handlers', () => {
  const examples = [
    `class FastAPI: pass\napp=FastAPI()\n@app.get('/fake')\ndef fake(): pass\n`,
    `from fastapi import FastAPI\nFastAPI=unknown\napp=FastAPI()\n@app.get('/fake')\ndef fake(): pass\n`,
    `from fastapi import FastAPI\napp=FastAPI()\n@wrap\n@app.get('/wrapped')\ndef wrapped(): pass\n`,
    `from fastapi import FastAPI\napp=FastAPI()\ndef handler(): pass\nhandler=wrap(handler)\napp.add_api_route('/wrapped',handler)\n`,
  ];
  for (const source of examples) assert.equal(routes(run({ 'shop.py': source })).length, 0);
  const local = run({ 'fastapi.py': 'class FastAPI: pass\n', 'shop.py': simple });
  assert.equal(routes(local).length, 0);
});

test('dynamic paths and unresolved handlers remain explicit unknown while unrelated facts survive', () => {
  const result = run({ 'shop.py': `${simple}\n@app.get(dynamic_path)\ndef dynamic(): pass\napp.add_api_route('/missing',missing_handler)\n` });
  assert.deepEqual(routes(result).map(r => r.path), ['/items']);
  assert(unknowns(result).some(r => r.detail === 'route-registration-unresolved'));
});

test('dynamic include prefixes, escape, mutations, cycles and conditional registration are not clean evidence', () => {
  const suffixes = [
    `r=APIRouter()\napp.include_router(r,prefix=dynamic)\n`,
    `install(app)\n`,
    `app.routes.append(unknown)\n`,
    `apps=[app]\ninstall(apps)\n`,
    `r=APIRouter()\nr.include_router(r)\napp.include_router(r)\n`,
    `if enabled:\n    app.add_api_route('/conditional',items)\n`,
    `def factory():\n    app.add_api_route('/runtime',items)\n`,
  ];
  for (const suffix of suffixes) {
    const result = run({ 'shop.py': `${simple.replace('import FastAPI','import FastAPI, APIRouter')}\n${suffix}` });
    assert.equal(routes(result).length, 0, suffix); assert(unknowns(result).length, suffix);
  }
});

test('unmounted routers are unknown and repeated identical includes merge their custody', () => {
  const source = `from fastapi import FastAPI, APIRouter\nr=APIRouter()\n@r.get('/same')\ndef same(): pass\n`;
  const unmounted = run({ 'shop.py': source });
  assert.equal(routes(unmounted).length,0); assert(unknowns(unmounted).some(r => r.detail === 'unmounted-router-prefix-unknown'));
  const repeated = run({ 'shop.py': `${source}\napp=FastAPI()\napp.include_router(r)\napp.include_router(r)\n` });
  assert.equal(routes(repeated).length,1);
  const tooMany = run({ 'shop.py': `from fastapi import FastAPI\napp=FastAPI()\n${Array.from({length:65},(_,i) => `@app.get('/route${i}')`).join('\n')}\ndef many(): pass\n` });
  assert.equal(routes(tooMany).length,0); assert.equal(tooMany.result.truncated,true); assert(unknowns(tooMany).some(r => r.detail === 'handler-route-bound'));
});

test('same path on distinct applications is valid; conflicting handlers in one app are unknown', () => {
  const result = run({ 'shop.py': `from fastapi import FastAPI\napp=FastAPI()\nother=FastAPI()\n@app.get('/same')\n@other.get('/same')\ndef one(): pass\n@app.get('/same')\ndef two(): pass\n` });
  assert.equal(routes(result).length, 1); assert(unknowns(result).some(r => r.detail === 'ambiguous-route-handler'));
});

test('cross-file registration revisions qualify handler signatures', () => {
  const files = { 'handlers.py': 'def actual(): pass\n', 'shop.py': `from fastapi import FastAPI\nfrom handlers import actual\napp=FastAPI()\napp.add_api_route('/before',actual)\n` };
  const before = run(files); const after = run({ ...files, 'shop.py': files['shop.py'].replace('/before','/after') });
  const handler = value => value.result.entries.find(e => e.evidence.http_routes);
  assert.equal(routes(before)[0].handler_native, routes(after)[0].handler_native);
  assert.notEqual(handler(before).evidence.signature, handler(after).evidence.signature);
});

test('module-qualified mutation and container escape cannot preserve a clean route inventory', () => {
  for (const mutation of ['application.app.routes.append(unknown)', 'apps=[application.app]\ninstall(apps)', 'register=application.app.add_api_route\ninstall(register)']) {
    const result = run({ 'application.py': simple, 'entry.py': `import application\n${mutation}\n` });
    assert.equal(routes(result).length,0,mutation); assert(unknowns(result).length,mutation);
  }
});

test('invalid registration argument shapes are unknown rather than guessed routes', () => {
  for (const decorator of ["app.get('/bad', methods=['POST'])", "app.get('/bad', typo=True)", "app.api_route('/bad', endpoint=other)"]) {
    const result = run({ 'shop.py': `from fastapi import FastAPI\napp=FastAPI()\n@${decorator}\ndef bad(): pass\n` });
    assert.equal(routes(result).length,0,decorator); assert(unknowns(result).length,decorator);
  }
  const wrongRouter = run({ 'shop.py': `${simple}\nother=FastAPI()\napp.include_router(other)\n` });
  assert.equal(routes(wrongRouter).length,0); assert(unknowns(wrongRouter).length);
  const duplicate = run({ 'shop.py': `${simple}\ndef items(): pass\n` });
  assert.equal(routes(duplicate).length,0); assert(unknowns(duplicate).some(r => r.detail === 'ambiguous-handler-carrier'));
});
