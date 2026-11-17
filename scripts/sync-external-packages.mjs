#!/usr/bin/env node
// Transitional, offline source snapshots. Implementations are maintained in
// their package repositories; this script reads committed Git blobs, never
// uncommitted worktree bytes. It is not the user-facing package resolver.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const lockPath = join(root, 'external-packages.lock.json');
const definitions = [
  { name: 'lekalo-openspec', from: '', to: 'packages/lekalo-openspec', includes: ['Cargo.toml', 'Cargo.lock', 'README.md', 'src', 'tests'], roots: [''] },
  { name: 'lekalo-hlv', from: '', to: 'packages/lekalo-hlv', includes: ['Cargo.toml', 'Cargo.lock', 'README.md', 'src'], roots: [''] },
  { name: 'lekalo-python', from: '', to: 'adapters/python', includes: ['Cargo.toml', 'Cargo.lock', 'README.md', 'src', 'vendor', 'framework-source.lock.json', 'tests/scan-protocol.test.mjs', 'tests/fastapi.test.mjs'], roots: ['src', 'vendor'] },
  { name: 'lekalo-go-discovery', owner: 'lekalo-go', from: 'crates/discovery', to: 'packages/lekalo-go-discovery', includes: ['crates/discovery'], roots: [''] },
  { name: 'lekalo-rust-discovery', owner: 'lekalo-rust', from: 'crates/discovery', to: 'packages/lekalo-rust-discovery', includes: ['crates/discovery'], roots: [''] },
  { name: 'lekalo-typescript-discovery', owner: 'lekalo-typescript', from: 'crates/discovery', to: 'packages/lekalo-typescript-discovery', includes: ['crates/discovery'], roots: [''] },
  { name: 'lekalo-python-discovery', owner: 'lekalo-python', from: 'crates/discovery', to: 'packages/lekalo-python-discovery', includes: ['crates/discovery'], roots: [''] },
];
const sha = bytes => `sha256:${createHash('sha256').update(bytes).digest('hex')}`;
// Build intermediates generated from pinned npm declarations. The TypeScript
// byte-parity build gate verifies them through the resulting committed bundle.
const generated = new Set([
  'adapters/node-typescript/src/libs.mjs', 'adapters/node-typescript/src/drizzle-libs.mjs',
  'adapters/node-typescript/node_modules', 'adapters/node-typescript/runtime',
  'adapters/node-typescript/.build', 'adapters/node-typescript/.build-zod',
  'packages/lekalo-openspec/target', 'packages/lekalo-hlv/target',
  ...['go', 'rust', 'typescript', 'python'].map(lang => `packages/lekalo-${lang}-discovery/target`),
]);
const git = (cwd, args) => execFileSync('git', args, { cwd, maxBuffer: 64 * 1024 * 1024 });
function safePath(path) {
  assert(path && !path.includes('\\') && !path.startsWith('/') && path.split('/').every(p => p && p !== '.' && p !== '..'), `Unsafe snapshot path: ${path}`);
  let current = root;
  for (const part of path.split('/')) {
    current = join(current, part);
    if (existsSync(current)) assert(!lstatSync(current).isSymbolicLink(), `Snapshot link: ${path}`);
  }
  return current;
}
function filesUnder(path) {
  const full = safePath(path);
  if (!existsSync(full)) return [];
  return readdirSync(full, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name)).flatMap(entry => {
    const child = `${path}/${entry.name}`;
    assert(!entry.isSymbolicLink(), `Snapshot link: ${child}`);
    if (generated.has(child)) return [];
    return entry.isDirectory() ? filesUnder(child) : [child];
  });
}
function verify(lock, allowMissing = false) {
  assert.equal(lock.schema, 'lekalo/external-source-snapshots/v1');
  const names = lock.packages.map(p => p.name);
  assert.equal(new Set(names).size, names.length, 'Duplicate snapshot package');
  assert.deepEqual(names, definitions.filter(def => !allowMissing || names.includes(def.name)).map(p => p.name));
  let count = 0;
  for (const pkg of lock.packages) {
    const def = definitions.find(def => def.name === pkg.name);
    assert.equal(pkg.repository, `https://github.com/ichinya/${def.owner ?? def.name}`);
    assert(/^[a-f0-9]{40}$/.test(pkg.commit), `Invalid commit: ${pkg.name}`);
    for (const [path, entry] of Object.entries(pkg.files)) {
      assert(path.startsWith(`${def.to}/`), `Wrong package destination: ${path}`);
      const expectedSource = [def.from, path.slice(def.to.length + 1)].filter(Boolean).join('/');
      assert.equal(entry.source, expectedSource);
      assert.equal(sha(readFileSync(safePath(path))), entry.sha256, `Snapshot changed: ${path}`);
      count++;
    }
    for (const sub of def.roots) {
      const home = sub ? `${def.to}/${sub}` : def.to;
      assert.deepEqual(filesUnder(home).sort(), Object.keys(pkg.files).filter(p => p.startsWith(`${home}/`)).sort(), `Unpinned source files: ${def.name}/${sub}`);
    }
  }
  return count;
}
const [mode] = process.argv.slice(2);
assert(process.argv.length === 3 && ['--check', '--sync'].includes(mode), 'Usage: node scripts/sync-external-packages.mjs --check|--sync');
if (mode === '--check') {
  const files = verify(JSON.parse(readFileSync(lockPath, 'utf8')));
  console.log(JSON.stringify({ ok: true, packages: definitions.length, files }));
} else {
  // Refuse to overwrite edits to an existing generated snapshot.
  const previous = existsSync(lockPath) ? JSON.parse(readFileSync(lockPath, 'utf8')) : undefined;
  if (previous) verify(previous, true);
  const previousPaths = new Set((previous?.packages ?? []).flatMap(pkg => Object.keys(pkg.files)));
  const staged = [];
  const packages = definitions.map(def => {
    const sourceRoot = resolve(process.env.LEKALO_ADDS_DIR ?? join(root, '..', 'lekalo-adds'), def.owner ?? def.name);
    const commit = git(sourceRoot, ['rev-parse', 'HEAD']).toString().trim();
    const inventory = git(sourceRoot, ['ls-tree', '-r', '-z', commit, '--', ...def.includes]).toString().split('\0').filter(Boolean);
    const files = {};
    for (const row of inventory) {
      const [header, source] = row.split('\t');
      assert(/^100(644|755) blob [a-f0-9]{40}$/.test(header), `Non-file package entry: ${source}`);
      assert(!def.from || source.startsWith(`${def.from}/`));
      const path = `${def.to}/${def.from ? source.slice(def.from.length + 1) : source}`;
      const bytes = git(sourceRoot, ['show', `${commit}:${source}`]);
      const destination = safePath(path);
      if (!previousPaths.has(path) && existsSync(destination)) {
        assert.equal(sha(readFileSync(destination)), sha(git(root, ['show', `HEAD:${path}`])), `Unowned file: ${path}`);
      }
      files[path] = { source, sha256: sha(bytes) };
      staged.push({ path, bytes });
    }
    assert(Object.keys(files).length > 0, `Empty package: ${def.name}`);
    return { name: def.name, repository: `https://github.com/ichinya/${def.owner ?? def.name}`, commit, files };
  });
  const incoming = new Set(staged.map(f => f.path));
  const stale = new Set((previous?.packages ?? []).flatMap(p => Object.keys(p.files)).filter(p => !incoming.has(p)));
  for (const def of definitions) for (const sub of def.roots) {
    for (const path of filesUnder(sub ? `${def.to}/${sub}` : def.to)) if (!incoming.has(path)) {
      // Initial migration may remove old core-owned source. Only unchanged
      // tracked bytes may be removed; never delete another agent's new work.
      if (!previousPaths.has(path)) assert.equal(sha(readFileSync(safePath(path))), sha(git(root, ['show', `HEAD:${path}`])), `Unowned file: ${path}`);
      stale.add(path);
    }
  }
  for (const { path, bytes } of staged) { const dest = safePath(path); mkdirSync(dirname(dest), { recursive: true }); writeFileSync(dest, bytes); }
  for (const path of stale) rmSync(safePath(path));
  const lock = { schema: 'lekalo/external-source-snapshots/v1', packages };
  writeFileSync(lockPath, JSON.stringify(lock, null, 2) + '\n');
  console.log(JSON.stringify({ ok: true, packages: packages.length, files: verify(lock) }));
}
