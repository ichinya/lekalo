#!/usr/bin/env node
// Issue #153: a domain/client workspace needs no HTTP or storage binding.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {cpSync, existsSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {basename, dirname, join, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const binary = process.env.LEKALO_BIN ?? join(repo, 'target', 'debug', process.platform === 'win32' ? 'lekalo.exe' : 'lekalo');
const fixtures = join(repo, 'tests', 'fixtures', 'public-typescript-scan');
const temp = realpathSync(tmpdir());
const root = mkdtempSync(join(temp, 'lekalo-domain-workspace-'));
function files(dir, prefix = '') {
  return readdirSync(join(dir, prefix), {withFileTypes: true}).flatMap(e => {
    const p = prefix ? `${prefix}/${e.name}` : e.name;
    return e.isDirectory() ? files(dir, p) : [[p, readFileSync(join(dir, p))]];
  });
}
function cli(args, expected = 0) {
  const r = spawnSync(binary, ['--json', ...args, '--project', '.'], {cwd: root, encoding: 'utf8', timeout: 120000, maxBuffer: 16 * 1024 * 1024});
  assert.equal(r.status, expected, `${args.join(' ')}: ${r.stdout}\n${r.stderr}`);
  return JSON.parse(r.stdout.trim() || r.stderr.trim());
}
try {
  cpSync(join(fixtures, 'workspace'), root, {recursive: true});
  const source = files(root);
  const preview = cli(['init', '--adopt', '--project-id', 'domain_workspace', '--dry-run']);
  assert.equal(preview.status, 'valid');
  assert.equal(existsSync(join(root, 'lekalo')), false);
  const applied = cli(['init', '--adopt', '--project-id', 'domain_workspace']);
  assert.equal(applied.status, 'valid');
  for (const name of ['public_core', 'public_web']) {
    if (!existsSync(join(root, 'lekalo', 'modules', name))) cli(['module', 'new', name]);
  }
  cpSync(join(fixtures, 'model', 'entities.yaml'), join(root, 'lekalo', 'modules', 'public_core', 'entities.yaml'));
  cpSync(join(fixtures, 'domain', 'commands.yaml'), join(root, 'lekalo', 'modules', 'public_core', 'commands.yaml'));
  const denied = cli(['validate', '--strict'], 3);
  assert.ok(denied.diagnostics.some(d => d.id === 'authorization.effect-unprotected'));
  const model = cli(['load', '--ir']);
  const authorization = JSON.parse(readFileSync(join(fixtures, 'domain', 'authorization.json'), 'utf8'));
  authorization.model_ref.ir_digest = 'sha256:' + createHash('sha256').update(JSON.stringify(model.ir)).digest('hex');
  writeFileSync(join(root, 'lekalo', 'authorization.yaml'), JSON.stringify(authorization, null, 2) + '\n');
  const validation = cli(['validate', '--strict']);
  assert.equal(validation.status, 'valid');
  const currentDigest = authorization.model_ref.ir_digest;
  authorization.model_ref.ir_digest = 'sha256:' + '0'.repeat(64);
  writeFileSync(join(root, 'lekalo', 'authorization.yaml'), JSON.stringify(authorization, null, 2) + '\n');
  const stale = cli(['validate', '--strict'], 3);
  assert.ok(stale.diagnostics.some(d => d.id === 'authorization.mapping-stale'));
  authorization.model_ref.ir_digest = currentDigest;
  writeFileSync(join(root, 'lekalo', 'authorization.yaml'), JSON.stringify(authorization, null, 2) + '\n');
  const command = cli(['inspect', 'public_core.set_value']);
  const effect = cli(['inspect', 'public_core.write_score']);
  assert.ok(JSON.stringify(command).includes('public_core.write_score'));
  assert.ok(JSON.stringify(effect).includes('public_core.score'));
  cli(['impact', 'public_core.score']);
  cli(['context', 'public_core.set_value', '--budget', '4000']);
  const loaded = cli(['load', '--ir']);
  const ir = JSON.stringify(loaded);
  assert.equal(ir.includes('"kind":"http"'), false);
  assert.equal(ir.includes('"kind":"persistence"'), false);
  for (const [path, bytes] of source) assert.deepEqual(readFileSync(join(root, path)), bytes, `source/config preserved: ${path}`);
  assert.equal(existsSync(join(root, 'node_modules')), false);
  console.log(JSON.stringify({ok: true, gate: 'domain-workspace', strict: true, domainCommand: 'public_core.set_value', localEffect: 'public_core.write_score', sourcePreserved: true, backendRequired: false}));
} finally {
  assert.equal(dirname(root), temp);
  assert.ok(basename(root).startsWith('lekalo-domain-workspace-'));
  assert.equal(realpathSync(root), root);
  rmSync(root, {recursive: true, force: true});
}
