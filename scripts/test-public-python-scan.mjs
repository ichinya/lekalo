#!/usr/bin/env node
// Issue #146: actual public CLI/compiled adapter, disposable source copy.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const suffix = process.platform === 'win32' ? '.exe' : '';
const binary = process.env.LEKALO_BIN ?? join(repo, `target/debug/lekalo${suffix}`);
const adapter = process.env.LEKALO_PYTHON_ADAPTER_BIN ?? join(repo, `adapters/python/target/debug/lekalo-target-python${suffix}`);
assert.ok(existsSync(binary), 'build the checkout CLI first');
assert.ok(existsSync(adapter), 'build adapters/python/Cargo.toml first');
const parent = realpathSync(tmpdir());
const root = mkdtempSync(join(parent, 'lekalo-public-python-'));
const fixture = join(repo, 'tests/fixtures/python-scan');
const hash = bytes => `sha256:${createHash('sha256').update(bytes).digest('hex')}`;
function inventory(path, prefix = '') {
  return readdirSync(join(path, prefix), { withFileTypes: true }).sort((a, b) => a.name < b.name ? -1 : a.name > b.name ? 1 : 0)
    .flatMap(entry => {
      const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
      return entry.isDirectory() ? inventory(path, relative) : [[relative, hash(readFileSync(join(path, relative)))]];
    });
}
function cli(args, expected = 0) {
  const result = spawnSync(binary, ['--json', ...args], {
    cwd: root, encoding: 'utf8', timeout: 120000, maxBuffer: 16 * 1024 * 1024,
  });
  assert.ifError(result.error);
  assert.equal(result.status, expected, `${args.slice(0, 4).join(' ')}: ${result.stdout}\n${result.stderr}`);
  return JSON.parse(result.stdout.trim() || result.stderr.trim());
}
try {
  for (const name of ['pyproject.toml', 'src', 'profile.json']) {
    cpSync(join(fixture, 'standalone', name), join(root, name), { recursive: true });
  }
  const originals = inventory(root);
  const profile = JSON.parse(readFileSync(join(root, 'profile.json'), 'utf8'));
  cli(['init', '--adopt', '--target', 'python', '--project-id', 'python_shop', '--project', '.']);
  if (!existsSync(join(root, 'lekalo/modules/shop'))) cli(['module', 'new', 'shop', '--project', '.']);
  cpSync(join(fixture, 'model/entities.yaml'), join(root, 'lekalo/modules/shop/entities.yaml'));
  const modelBefore = inventory(join(root, 'lekalo'));
  const scan = () => cli(['scan', '--target', 'python', '--profile', 'standalone', '--project', '.', '--', adapter,
    '--lekalo-project-profile-json', JSON.stringify(profile)]);
  const cold = scan();
  assert.equal(cold.status, 'valid');
  const indexPath = join(root, '.lekalo/import/observed/index.json');
  const coldIndex = readFileSync(indexPath);
  const repeated = scan();
  assert.equal(repeated.revision, cold.revision);
  assert.deepEqual(readFileSync(indexPath), coldIndex, 'unchanged scan preserves index bytes');
  const proposals = cli(['bindings', 'propose', '--project', '.']);
  const selected = proposals.proposals.find(row => row.symbol === 'shop.orders.total');
  assert.ok(selected, JSON.stringify(proposals));
  assert.equal(selected.ambiguous, false);
  const confirmed = cli(['bindings', 'confirm', selected.proposal, '--project', '.']);
  assert.equal(confirmed.symbol, selected.symbol);
  assert.equal(confirmed.binding, 'confirmed');
  assert.equal(confirmed.state, 'current');
  cli(['bindings', 'audit', '--project', '.']);
  cli(['validate', '--strict', '--project', '.']);
  for (const args of [['observe', 'inspect'], ['observe', 'impact'], ['inspect'], ['impact'], ['context']]) {
    cli([...args, selected.symbol, ...(args[0] === 'context' ? ['--budget', '4000'] : []), '--project', '.']);
  }
  assert.deepEqual(inventory(join(root, 'lekalo')), modelBefore, 'scan/confirmation never author canonical semantics');
  for (const [path, digest] of originals) assert.equal(hash(readFileSync(join(root, path))), digest, `source unchanged: ${path}`);

  // Stale approval must not silently replace a previously confirmed fact.
  const source = join(root, 'src/shop/orders.py');
  const sourceBytes = readFileSync(source, 'utf8');
  writeFileSync(source, `${sourceBytes}\n# body-only change\n`);
  cli(['bindings', 'audit', '--project', '.'], 1);
  scan();
  cli(['bindings', 'audit', '--project', '.']);
  writeFileSync(source, sourceBytes.replace('tax: bool = True', 'tax: bool = True, precision: int = 2'));
  cli(['bindings', 'audit', '--project', '.'], 1);
  scan();
  const fresh = cli(['bindings', 'propose', '--project', '.']).proposals.find(row => row.symbol === selected.symbol);
  assert.ok(fresh, 'signature mutation requires explicit reconfirmation');
  assert.notEqual(fresh.proposal, selected.proposal);
  const accepted = readFileSync(indexPath);
  writeFileSync(source, `${readFileSync(source, 'utf8')}\n# changed after review\n`);
  const refused = cli(['bindings', 'confirm', fresh.proposal, '--project', '.'], 1);
  assert.ok(refused.diagnostics.some(row => row.id === 'bindings.plan-mismatch'), JSON.stringify(refused));
  assert.deepEqual(readFileSync(indexPath), accepted, 'stale confirmation refuses before writing');
  scan();
  const reviewed = cli(['bindings', 'propose', '--project', '.']).proposals.find(row => row.symbol === selected.symbol);
  cli(['bindings', 'confirm', reviewed.proposal, '--project', '.']);
  cli(['bindings', 'audit', '--project', '.']);

  // The suite's unsupported generation/storage checks remain honest skips.
  const conformance = cli(['adapter', 'test', '--report', 'json', '--repeats', '2', '--', adapter,
    '--lekalo-project-profile-json', JSON.stringify(profile)]);
  assert.equal(conformance.report.verdict, 'pass', JSON.stringify(conformance));
  assert.equal(conformance.report.counts.fail, 0);
  assert.ok(conformance.report.counts.pass > 0);
  writeFileSync(source, readFileSync(source, 'utf8').replace('def total(', 'def total_renamed('));
  scan();
  cli(['bindings', 'audit', '--project', '.'], 1);
  const renamed = cli(['bindings', 'propose', '--project', '.']).proposals;
  assert.ok(renamed.some(row => row.symbol === 'shop.orders.total_renamed'));
  assert.ok(!renamed.some(row => row.symbol === selected.symbol), 'rename never silently remaps a confirmed semantic binding');
  console.log(JSON.stringify({ ok: true, gate: 'public-python-scan', platform: process.platform,
    symbols: cold.symbols, confirmed: selected.symbol, deterministic: true, sourcePreserved: true,
    canonicalPreserved: true, bodyFreshness: true, renameRefused: true,
    staleConfirmationRefused: true, conformance: conformance.report.counts }));
} finally {
  const checked = realpathSync(root);
  assert.equal(dirname(checked), parent);
  assert.ok(basename(checked).startsWith('lekalo-public-python-'));
  rmSync(checked, { recursive: true, force: true, maxRetries: 3 });
}
