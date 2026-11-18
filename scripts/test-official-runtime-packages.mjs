// Exercise the public CLI against the pinned owner packages and their actual grammars.
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname, join, resolve} from 'node:path';
import test from 'node:test';
import {externalRoot} from './external-tool.mjs';

const cases = [
  ['go', 'go', null, 'service.go', 'package api\nfunc Health() {}', 'package api'],
  ['rust', 'rust', null, 'lib.rs', 'pub fn health() {}', '// no declarations'],
  ['hono', 'typescript', 'typescript-hono', 'app.ts', "import { Hono } from 'hono'; const app = new Hono(); app.get('/health', c => c.text('ok'));", "import { Hono } from 'hono'; const app = new Hono();"],
  ['zod', 'typescript', 'typescript-zod', 'types.ts', "import { z } from 'zod'; export const Cat = z.object({});", "import { z } from 'another'; export const Cat = z.object({});"],
  ['drizzle', 'typescript', 'typescript-drizzle', 'tables.ts', "import { pgTable as table } from 'drizzle-orm/pg-core'; export const cat = table('cat', {});", "import { pgTable as table } from 'another'; export const cat = table('cat', {});"],
  ['fastapi', 'python', 'python-fastapi', 'main.py', "from fastapi import FastAPI\napp = FastAPI()\n@app.get('/health')\ndef health(): pass", "from fastapi import FastAPI\napp = FastAPI()"],
  ['fastmcp', 'python', 'python-fastmcp', 'server.py', "from fastmcp import FastMCP\ns = FastMCP('tools')\n@s.tool()\ndef health(): pass", "from fastmcp import FastMCP\ns = FastMCP('tools')\ndef health(): pass"],
  ['symfony', 'php', 'php-symfony', 'src/Controller/HealthController.php', "<?php use Symfony\\Component\\Routing\\Attribute\\Route; final class C { #[Route('/health', methods: ['GET'])] public function index() {} }", '<?php final class C { public function index() {} }'],
];

test('independent language and framework packages certify and reject real sources', t => {
  const scratch = mkdtempSync(join(process.env.BB_THREAD_STORAGE ?? tmpdir(), 'lekalo-packages-'));
  t.after(() => rmSync(scratch, {recursive:true, force:true}));
  const project = join(scratch, 'app'), declarations = join(scratch, 'declarations');
  const json = (file, obj) => writeFileSync(file, JSON.stringify(obj));
  mkdirSync(declarations);
  json(join(declarations, 'lekalo.package.json'), {schema:'lekalo/package/v1',id:'declarations',version:'1.0.0',kind:'rules',rules:'rule.json',files:['rule.json']});
  json(join(declarations, 'rule.json'), {schema:'lekalo/contract-rules/v1',requires:['source.declarations/v1'],checks:[{id:'public-declaration',schema:{required:['declarations'],properties:{declarations:{type:'array',minItems:1}}}}]});
  const config = {schema:'lekalo/runtime-config/v1',components:{}};
  for (const [id, language, framework, file, good] of cases) {
    const path = join(project,id,file);
    mkdirSync(dirname(path), {recursive:true});
    writeFileSync(path,good);
    config.components[id] = {path:id,runtime:'path:'+externalRoot('lekalo-'+language),
      rules:['path:'+(framework ? externalRoot('lekalo-'+framework) : declarations)]};
  }
  json(join(project,'lekalo.yaml'),config);
  function run(...args) {
    return spawnSync(process.env.LEKALO_BIN ?? resolve('target/debug/lekalo'), ['--json','runtime',...args], {
      cwd:project, encoding:'utf8', timeout:180_000, env:{...process.env,LEKALO_PACKAGE_HOME:join(scratch,'cache')},
    });
  }
  const lock=run('lock'); assert.equal(lock.status,0,lock.stderr);
  const check=run('check'); assert.equal(check.status,0,check.stderr);
  for (const [id, , , file, , broken] of cases) writeFileSync(join(project,id,file),broken);
  const failed=run('check'); assert.equal(failed.status,1,failed.stderr);
  const diagnostics=JSON.parse(failed.stderr).diagnostics;
  assert.deepEqual(diagnostics.map(d=>d.data.component).sort(),cases.map(c=>c[0]).sort());
  assert(diagnostics.every(d=>d.id==='runtime.rule-failed'));
  const pinned=JSON.parse(readFileSync(join(project,'lekalo.runtime.lock')));
  assert.equal(pinned.components.go.runtime.package.dependencies.parser.manifest.kind,'library');
});

test('Ruby and Java discovery resolve project and Gradle workspace roots', t => {
  const scratch = mkdtempSync(join(process.env.BB_THREAD_STORAGE ?? tmpdir(), 'lekalo-discovery-'));
  t.after(() => rmSync(scratch, {recursive: true, force: true}));
  const env = {...process.env, LEKALO_PACKAGE_HOME: join(scratch, 'cache')};
  delete env.LEKALO_SERVICES;
  delete env.LEKALO_PROJECT;
  function invoke(cwd, ...args) {
    const binary = process.env.LEKALO_BIN ?? resolve('target/debug/lekalo');
    const run = spawnSync(binary, ['--json', ...args], {cwd, env, encoding: 'utf8', timeout: 120_000});
    assert.ifError(run.error);
    assert.equal(run.status, 0, run.stderr);
    return JSON.parse(run.stdout);
  }
  for (const [language, marker, basis] of [
    ['ruby', 'Gemfile', 'manifest'],
    ['java', 'settings.gradle.kts', 'workspace-root'],
  ]) {
    const root = join(scratch, language);
    const nested = join(root, 'subdirectory');
    mkdirSync(nested, {recursive: true});
    writeFileSync(join(root, marker), '');
    const adopt = () => invoke(nested, 'init', '--adopt', '--dry-run').detection;
    // Manifest names alone no longer grant the host a language selection.
    assert.equal(adopt().basis, 'invocation-directory');
    writeFileSync(join(root, 'lekalo.yaml'), JSON.stringify({
      schema: 'lekalo/runtime-config/v1', components: {},
      services: {['discovery/' + language]: 'path:' + externalRoot('lekalo-' + language)},
    }));
    invoke(root, 'runtime', 'lock');
    const detection = adopt();
    assert.equal(detection.basis, basis);
    assert.deepEqual(detection.candidates, ['..']);
    assert.ok(detection.manifests.some(m => m.path === marker && m.detail === language));
  }
});
