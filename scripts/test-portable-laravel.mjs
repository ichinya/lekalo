// Exercise real PHP and Laravel packages through the same public CLI as a third-party runtime.
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {cpSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import test from 'node:test';
import {externalRoot} from './external-tool.mjs';

const cli = process.env.LEKALO_BIN ?? resolve('target/debug/lekalo');
const write = (file, value) => writeFileSync(file, JSON.stringify(value, null, 2));
const workspace = (root, declarations) => write(join(root, 'lekalo.workspace.yaml'), {
  schema: 'lekalo/workspace/v1', ...declarations,
});

function fixture(t) {
  const tmp = mkdtempSync(join(process.env.BB_THREAD_STORAGE ?? tmpdir(), 'portable-contract-'));
  t.after(() => rmSync(tmp, {recursive: true, force: true}));
  const env = {...process.env, LEKALO_PACKAGE_HOME: join(tmp, 'cache')};
  delete env.LEKALO_PROJECT;
  delete env.LEKALO_SERVICES;
  function run(cwd, args, expected = 0) {
    const result = spawnSync(cli, ['--json', ...args], {cwd, env, encoding: 'utf8', timeout: 120000});
    assert.equal(result.status, expected, String(result.error ?? '') + result.stderr + result.stdout);
    return JSON.parse(result.stdout || result.stderr);
  }
  return {tmp, run};
}

const independentRuntime = `import {readFileSync} from 'node:fs';
const q = JSON.parse(readFileSync(0, 'utf8'));
function check(input) {
  const routes = input.files.flatMap(file => file.content.trim().split('\\n').map(line => {
    const [method, path] = line.split(' ');
    return {method, path};
  }));
  const facts = {};
  for (const pack of input.rules) {
    facts[pack.id] = Object.fromEntries(pack.rules.checks.map(check => [check.id, {routes}]));
  }
  return {state: 'complete', facts};
}
const description = {id: 'independent-zen', version: '1.0.0', capabilities: ['http.routes/v1'],
  operations: ['check'], rules_api: ['lekalo/contract-rules/v1']};
const result = q.operation === 'describe' ? description : check(q.input);
process.stdout.write(JSON.stringify({protocol: q.protocol, request_id: q.request_id, result}));`;

test('the Laravel contract is portable and parent validation identifies the affected consumer', t => {
  const {tmp, run} = fixture(t);
  const parent = join(tmp, 'pushhard');
  const api = join(parent, 'api');
  const web = join(parent, 'web');
  for (const child of [api, web]) mkdirSync(join(child, 'src'), {recursive: true});

  const zen = join(tmp, 'zen.mjs');
  writeFileSync(zen, independentRuntime);
  const rules = ['path:' + externalRoot('lekalo-php-laravel')];
  for (const [root, runtime] of [[api, 'path:' + externalRoot('lekalo-php')], [web, {command: [process.execPath, zen]}]]) {
    write(join(root, 'lekalo.yaml'), {schema: 'lekalo/runtime-config/v1', components: {app: {path: 'src', runtime, rules}}});
  }
  const zenSource = join(web, 'src/http.zen');
  const route = path => "R::get('" + path + "', function () { return 'ok'; });";
  const phpRoutes = (...lines) => writeFileSync(join(api, 'src/web.php'),
    "<?php\nuse Illuminate\\Support\\Facades\\Route as R;\n" + lines.join('\n') + '\n');
  phpRoutes(route('/health'), route('/orders'));
  writeFileSync(zenSource, 'GET /health\n');

  const pins = [api, web].map(root => run(root, ['runtime', 'lock']).lock.components.app.rules[0].digest);
  assert.equal(pins[0], pins[1]);
  const validate = ['validate', '--strict'];
  for (const root of [api, web]) run(root, validate);
  writeFileSync(zenSource, 'GET /v2/health\n');
  assert.equal(run(web, validate, 1).diagnostics[0].data.rule, 'health-route');
  writeFileSync(zenSource, 'GET /health\n');

  workspace(api, {exports: {routes: {kind: 'runtime-fact', component: 'app', package: 'lekalo-php-laravel', rule: 'health-route'}}});
  const bundle = run(api, ['workspace', 'export', 'routes']);
  write(join(web, 'routes.json'), bundle);
  workspace(web, {imports: {routes: {from: 'api', export: 'routes', bundle: 'routes.json', digest: bundle.digest}}});
  workspace(parent, {children: {api: 'api', web: 'web'}});
  assert.deepEqual(run(parent, validate).dependencies, [
    {consumer: 'web', import: 'routes', producer: 'api', export: 'routes', digest: bundle.digest},
  ]);

  phpRoutes(route('/orders'), route('/health'), '// internal implementation changed');
  run(parent, validate);
  phpRoutes(route('/health'), route('/v2/orders'));
  run(api, validate);
  renameSync(api, join(tmp, 'separate-api-checkout'));
  run(web, validate);
  renameSync(join(tmp, 'separate-api-checkout'), api);
  assert.match(JSON.stringify(run(parent, validate, 1)), /workspace-contract-changed.web.routes.api/);
  assert.deepEqual(JSON.parse(readFileSync(join(web, 'routes.json'))), bundle);

  phpRoutes('// ' + route('/health'), route('/orders'));
  assert.equal(run(api, ['runtime', 'check'], 1).diagnostics[0].data.rule, 'health-route');
  const failure = JSON.stringify(run(parent, validate, 1));
  assert.match(failure, /workspace-child-invalid.api/);
  assert.match(failure, /workspace-export-unavailable.web.routes.api/);
  phpRoutes("R::prefix('internal')->group(function () { " + route('/health') + " });");
  assert.match(JSON.stringify(run(api, ['runtime', 'check'], 1)), /runtime-incomplete/);
  write(join(web, 'routes.json'), {...bundle, contract: {...bundle.contract, fact: {routes: []}}});
  assert.match(JSON.stringify(run(web, validate, 1)), /workspace-pin-mismatch/);
});

test('configured runtimes preserve module API validation, public digest and consumer lock checks', t => {
  const {tmp, run} = fixture(t);
  const root = join(tmp, 'project');
  cpSync(resolve('tests/fixtures/module-api/reference/tasks'), root, {recursive: true});
  const args = ['validate', '--module-api', 'module-api.json'];
  const baseline = run(root, args);
  mkdirSync(join(root, 'src'));
  write(join(root, 'lekalo.yaml'), {schema: 'lekalo/runtime-config/v1',
    components: {app: {path: 'src', runtime: 'path:' + externalRoot('lekalo-php')}}});
  run(root, ['runtime', 'lock']);
  assert.deepEqual(run(root, args), baseline);
  workspace(root, {exports: {tasks: {kind: 'module-api', api: 'module-api.json', module: 'tasks'}}});
  assert.deepEqual(run(root, args), baseline);

  const lock = run(root, ['module', 'api', 'lock', '--api', 'module-api.json', '--module', 'tasks', '--consumer', 'planner']).moduleApiLock;
  write(join(root, 'consumer.json'), lock);
  run(root, [...args, '--module-api-lock', 'consumer.json']);
  write(join(root, 'consumer.json'), {...lock, digest: 'sha256:' + 'a'.repeat(64)});
  run(root, [...args, '--module-api-lock', 'consumer.json'], 3);
  const published = run(root, ['workspace', 'export', 'tasks']);
  const standalone = run(root, ['module', 'api', 'export', 'tasks', '--api', 'module-api.json']);
  assert.equal(published.digest, standalone.modulePublicContract.digest);
});
