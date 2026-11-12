import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, statSync, symlinkSync, unlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { prepare, verify } from './ci-contracts-cli.mjs';

const sourceSha = 'a'.repeat(40);
function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'lekalo-contracts-cli-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const binary = join(root, 'built-cli');
  const directory = join(root, 'artifact');
  writeFileSync(binary, 'fixture executable bytes');
  const options = { sourceSha, platform: 'linux', arch: 'x64' };
  return { root, binary, directory, options,
    prepare: () => prepare({ binary, destination: directory, ...options }),
    verify: overrides => verify({ directory, ...options, ...overrides }),
    mutate(change) {
      const path = join(directory, 'contracts-cli.json');
      const manifest = JSON.parse(readFileSync(path, 'utf8'));
      change(manifest);
      writeFileSync(path, JSON.stringify(manifest));
    },
  };
}

test('matching producer artifact survives permission normalization and restores execution', t => {
  const f = fixture(t);
  const manifest = f.prepare();
  chmodSync(join(f.directory, 'lekalo'), 0o644);
  assert.deepEqual(f.verify(), manifest);
  if (process.platform !== 'win32') assert.equal(statSync(join(f.directory, 'lekalo')).mode & 0o777, 0o755);
});

test('mismatched checkout, runtime and malformed source SHA are refused', t => {
  const f = fixture(t); f.prepare();
  for (const overrides of [{ sourceSha: 'b'.repeat(40) }, { sourceSha: 'short' }, { platform: 'darwin' }, { arch: 'arm64' }]) {
    assert.throws(() => f.verify(overrides), /manifest|SHA/);
  }
});

test('tampered binary or truncated download cannot pass digest verification', t => {
  const f = fixture(t); f.prepare();
  const path = join(f.directory, 'lekalo');
  writeFileSync(path, 'fixture executable byte!');
  assert.throws(() => f.verify(), /digest or size/);
  writeFileSync(path, 'short');
  assert.throws(() => f.verify(), /digest or size/);
});

test('manifest cannot select paths, platforms, versions or invalid sizes and hashes', t => {
  const changes = [
    m => { m.binary = '../built-cli'; }, m => { m.schemaVersion = 2; },
    m => { m.platform = 'win32'; }, m => { m.arch = 'arm64'; },
    m => { m.bytes = 0; }, m => { m.bytes = '24'; }, m => { m.sha256 = 'bad'; },
  ];
  for (const change of changes) {
    const f = fixture(t); f.prepare(); f.mutate(change);
    assert.throws(() => f.verify(), /manifest/);
  }
});

test('missing, malformed and symlinked artifacts fail closed', t => {
  const f = fixture(t); f.prepare();
  const path = join(f.directory, 'contracts-cli.json');
  writeFileSync(path, '{');
  assert.throws(() => f.verify(), SyntaxError);
  unlinkSync(path);
  assert.throws(() => f.verify(), /ENOENT/);
  const link = join(f.root, 'linked-artifact');
  symlinkSync(f.directory, link, process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => f.verify({ directory: link }), /plain artifact directory/);
  // A directory link also cannot masquerade as either required file.
  symlinkSync(f.root, path, process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => f.verify(), /plain file/);
});

test('producer never overwrites an existing destination or accepts another runtime', t => {
  const f = fixture(t); f.prepare();
  assert.throws(f.prepare, /EEXIST/);
  assert(existsSync(join(f.directory, 'lekalo')));
  assert.throws(() => prepare({ binary: f.binary, destination: join(f.root, 'other'), ...f.options, platform: 'darwin' }), /linux\/x64/);
});

test('external Cargo target can package into a new checkout target directory', t => {
  const f = fixture(t);
  const directory = join(f.root, 'new-checkout-target', 'contracts-cli');
  const manifest = prepare({ binary: f.binary, destination: directory, ...f.options });
  assert.deepEqual(f.verify({ directory }), manifest);
  writeFileSync(f.binary, '');
  assert.throws(() => prepare({ binary: f.binary, destination: join(f.root, 'empty'), ...f.options }), /empty/);
});

test('stripped Linux executable retains behavior and hashes the copy without changing Cargo output', {
  skip: process.platform !== 'linux' || process.arch !== 'x64',
}, t => {
  const f = fixture(t);
  const source = join(f.root, 'main.c');
  writeFileSync(source, '#include <stdio.h>\nint main(void) { puts("contracts CLI fixture"); return 0; }\n');
  execFileSync('cc', ['-g', source, '-o', f.binary]);
  const original = readFileSync(f.binary);
  const expected = execFileSync(f.binary, [], { encoding: 'utf8' });
  const manifest = prepare({ binary: f.binary, destination: f.directory, ...f.options, stripSymbols: true });
  const packagedBinary = join(f.directory, 'lekalo');
  assert(manifest.bytes < original.length, 'fixture symbols should be removed');
  assert.deepEqual(readFileSync(f.binary), original);
  chmodSync(packagedBinary, 0o644);
  assert.deepEqual(f.verify(), manifest);
  assert.equal(execFileSync(packagedBinary, [], { encoding: 'utf8' }), expected);
  writeFileSync(packagedBinary, 'tampered stripped executable');
  assert.throws(() => f.verify(), /digest or size/);
});

test('failed stripping cannot publish a manifest or change the source', {
  skip: process.platform !== 'linux' || process.arch !== 'x64',
}, t => {
  const f = fixture(t);
  const original = readFileSync(f.binary);
  assert.throws(() => prepare({ binary: f.binary, destination: f.directory, ...f.options, stripSymbols: true }));
  assert.equal(existsSync(join(f.directory, 'contracts-cli.json')), false);
  assert.deepEqual(readFileSync(f.binary), original);
  assert.throws(() => f.verify(), /ENOENT/);
});

test('CLI accepts the strip option only once and only for preparation', t => {
  const f = fixture(t);
  const script = fileURLToPath(new URL('./ci-contracts-cli.mjs', import.meta.url));
  for (const args of [['verify', f.directory, '--strip'], ['prepare', f.directory, '--strip', '--strip'],
    ['prepare', f.directory, '--unknown']]) {
    const result = spawnSync(process.execPath, [script, ...args], { encoding: 'utf8', cwd: f.root });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Usage:/);
    assert.equal(existsSync(f.directory), false);
  }
});

test('CLI refuses a workflow SHA different from the actual checkout before using an artifact', t => {
  const f = fixture(t);
  const script = fileURLToPath(new URL('./ci-contracts-cli.mjs', import.meta.url));
  // A private repository also works when the source is a Windows worktree
  // mounted into WSL: that worktree's Git metadata contains Windows paths.
  execFileSync('git', ['init', '--quiet', f.root]);
  execFileSync('git', ['-c', 'user.name=CI fixture', '-c', 'user.email=ci@example.invalid',
    '-c', 'commit.gpgsign=false', '-c', `core.hooksPath=${join(f.root, 'unused-hooks')}`,
    'commit', '--quiet', '--allow-empty', '-m', 'fixture'], { cwd: f.root });
  const result = spawnSync(process.execPath, [script, 'verify', f.directory], {
    cwd: f.root,
    env: { ...process.env, GITHUB_SHA: '0'.repeat(40) }, encoding: 'utf8',
  });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /Checkout SHA differs from GITHUB_SHA/);
  assert.equal(existsSync(f.directory), false);
});
