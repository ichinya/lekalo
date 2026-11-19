// Exercise parent imports and module exports through a standalone protocol fixture.
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {cpSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import test from 'node:test';

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
  routes.sort((left, right) => left.path.localeCompare(right.path));
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

test('parent validation identifies the consumer of a changed runtime contract', t => {
  const {tmp, run} = fixture(t);
  const parent = join(tmp, 'pushhard');
  const api = join(parent, 'api');
  const web = join(parent, 'web');
  for (const child of [api, web]) mkdirSync(join(child, 'src'), {recursive: true});

  const zen = join(tmp, 'zen.mjs');
  writeFileSync(zen, independentRuntime);
  const policy = join(tmp, 'rules');
  mkdirSync(policy);
  write(join(policy, 'lekalo.package.json'), {schema:'lekalo/package/v1',id:'route-policy',version:'1.0.0',kind:'rules',rules:'routes.json',files:['routes.json']});
  write(join(policy, 'routes.json'), {schema:'lekalo/contract-rules/v1',requires:['http.routes/v1'],checks:[{id:'health-route',schema:{required:['routes'],properties:{routes:{type:'array',contains:{required:['method','path'],properties:{method:{const:'GET'},path:{const:'/health'}}}}}}}]});
  const rules = ['path:' + policy];
  for (const root of [api, web]) {
    write(join(root, 'lekalo.yaml'), {schema:'lekalo/runtime-config/v1', components:{app:{path:'src',runtime:{command:[process.execPath,zen]},rules}}});
  }
  const zenSource = join(web, 'src/http.zen');
  const apiRoutes = (...paths) => writeFileSync(join(api,'src/routes.zen'),paths.map(path => 'GET ' + path).join('\n'));
  apiRoutes('/health','/orders');
  writeFileSync(zenSource, 'GET /health\n');

  const pins = [api, web].map(root => run(root, ['runtime', 'lock']).lock.components.app.rules[0].digest);
  assert.equal(pins[0], pins[1]);
  const validate = ['validate', '--strict'];
  for (const root of [api, web]) run(root, validate);
  writeFileSync(zenSource, 'GET /v2/health\n');
  assert.equal(run(web, validate, 1).diagnostics[0].data.rule, 'health-route');
  writeFileSync(zenSource, 'GET /health\n');

  workspace(api, {exports: {routes: {kind: 'runtime-fact', component: 'app', package: 'route-policy', rule: 'health-route'}}});
  const bundle = run(api, ['workspace', 'export', 'routes']);
  write(join(web, 'routes.json'), bundle);
  workspace(web, {imports: {routes: {from: 'api', export: 'routes', bundle: 'routes.json', digest: bundle.digest}}});
  workspace(parent, {children: {api: 'api', web: 'web'}});
  assert.deepEqual(run(parent, validate).dependencies, [
    {consumer: 'web', import: 'routes', producer: 'api', export: 'routes', digest: bundle.digest},
  ]);

  apiRoutes('/orders', '/health');
  run(parent, validate);
  apiRoutes('/health', '/v2/orders');
  run(api, validate);
  renameSync(api, join(tmp, 'separate-api-checkout'));
  run(web, validate);
  renameSync(join(tmp, 'separate-api-checkout'), api);
  assert.match(JSON.stringify(run(parent, validate, 1)), /workspace-contract-changed\/web\/routes\/api\/unknown/);
  assert.deepEqual(JSON.parse(readFileSync(join(web, 'routes.json'))), bundle);

  apiRoutes('/orders');
  assert.equal(run(api, ['runtime', 'check'], 1).diagnostics[0].data.rule, 'health-route');
  const failure = JSON.stringify(run(parent, validate, 1));
  assert.match(failure, /workspace-child-invalid.api/);
  assert.match(failure, /workspace-export-unavailable.web.routes.api/);
  write(join(web, 'routes.json'), {...bundle, contract: {...bundle.contract, fact: {routes: []}}});
  assert.match(JSON.stringify(run(web, validate, 1)), /workspace-pin-mismatch/);
});

test('configured runtimes preserve module API validation, public digest and consumer lock checks', t => {
  const {tmp, run} = fixture(t);
  const root = join(tmp, 'project');
  cpSync(resolve('tests/fixtures/module-api/reference/tasks'), root, {recursive: true});
  const zen = join(tmp, 'runtime.mjs');
  writeFileSync(zen, independentRuntime);
  const args = ['validate', '--module-api', 'module-api.json'];
  const baseline = run(root, args);
  mkdirSync(join(root, 'src'));
  write(join(root, 'lekalo.yaml'), {schema: 'lekalo/runtime-config/v1',
    components: {app: {path: 'src', runtime: {command:[process.execPath,zen]}}}});
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

test('a parent classifies module API changes against the web project’s accepted bundle', t => {
  const {tmp, run} = fixture(t);
  const api = join(tmp, 'api');
  const web = join(tmp, 'web');
  cpSync(resolve('tests/fixtures/module-api/reference/tasks'), api, {recursive:true});
  mkdirSync(join(web, 'src'), {recursive:true});
  const zen = join(tmp, 'zen.mjs');
  writeFileSync(zen, independentRuntime);
  write(join(web, 'lekalo.yaml'), {
    schema:'lekalo/runtime-config/v1', components:{app:{path:'src',runtime:{command:[process.execPath,zen]}}},
  });
  run(web, ['runtime', 'lock']);
  workspace(tmp, {children:{api:'api', web:'web'}});
  workspace(api, {exports:{tasks:{kind:'module-api', api:'module-api.json', module:'tasks'}}});

  const bundlePath = join(web, 'tasks.json');
  function accept() {
    const bundle = run(api, ['workspace', 'export', 'tasks']);
    write(bundlePath, bundle);
    workspace(web, {imports:{tasks:{from:'api', export:'tasks', bundle:'tasks.json', digest:bundle.digest}}});
    return bundle;
  }
  accept();
  run(tmp, ['workspace', 'check']);

  const specPath = join(api, 'module-api.json');
  const spec = JSON.parse(readFileSync(specPath));
  const tasks = spec.modules[0];
  const declarations = join(api, 'lekalo/modules/tasks/entities.yaml');
  const previous = readFileSync(declarations, 'utf8');
  writeFileSync(declarations, previous.trimEnd() + `
  - id: tasks.reference
    kind: scalar
    version: 1
    description: "External task reference"
    base: uuid
    visibility: project
`);
  tasks.apiVersion = '1.3.0';
  tasks.exports.push({export:'tasks.reference',kind:'scalar',since:'1.3.0',aliasOf:null});
  write(specPath, spec);
  assert.match(JSON.stringify(run(tmp, ['workspace', 'check'], 1)), /workspace-contract-changed\/web\/tasks\/api\/compatible/);
  const additive = accept();
  run(tmp, ['workspace', 'check']);

  tasks.apiVersion = '2.0.0';
  tasks.exports.pop();
  write(specPath, spec);
  renameSync(api, join(tmp, 'separate-api-checkout'));
  run(web, ['validate', '--strict']);
  renameSync(join(tmp, 'separate-api-checkout'), api);
  assert.match(JSON.stringify(run(tmp, ['workspace', 'check'], 1)), /workspace-contract-changed\/web\/tasks\/api\/breaking/);
  assert.deepEqual(JSON.parse(readFileSync(bundlePath)), additive);
});
