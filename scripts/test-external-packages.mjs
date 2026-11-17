import assert from 'node:assert/strict';
import { spawnSync, execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

test('snapshots read committed source, detect drift and preserve edited copies', () => {
  const scratch = mkdtempSync(join(tmpdir(), 'lekalo-package-custody-'));
  try {
    const root = join(scratch, 'core'), adds = join(scratch, 'adds');
    mkdirSync(join(root, 'scripts'), { recursive: true });
    copyFileSync(fileURLToPath(new URL('./sync-external-packages.mjs', import.meta.url)), join(root, 'scripts/sync-external-packages.mjs'));
    const definitions = [
      ['lekalo-openspec', 'src/index.txt', 'packages/lekalo-openspec/src/index.txt'],
      ['lekalo-hlv', 'src/index.txt', 'packages/lekalo-hlv/src/index.txt'],
      ['lekalo-python', 'src/index.txt', 'adapters/python/src/index.txt'],
      ['lekalo-typescript', 'adapters/node-typescript/src/index.txt', 'adapters/node-typescript/src/index.txt'],
    ];
    for (const [name, source] of definitions) {
      const cwd = join(adds, name), path = join(cwd, source);
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, 'committed\n');
      const git = args => execFileSync('git', args, { cwd, stdio: 'pipe' });
      git(['init', '-q']); git(['add', '.']);
      git(['-c', 'user.name=Custody test', '-c', 'user.email=custody@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture']);
      writeFileSync(path, 'uncommitted must not be supplied\n');
    }
    const run = mode => spawnSync(process.execPath, [join(root, 'scripts/sync-external-packages.mjs'), mode], {
      cwd: root, encoding: 'utf8', env: { ...process.env, LEKALO_ADDS_DIR: adds },
    });
    let result = run('--sync'); assert.equal(result.status, 0, result.stderr);
    for (const [, , destination] of definitions) assert.equal(readFileSync(join(root, destination), 'utf8'), 'committed\n');
    assert.equal(run('--check').status, 0);
    // The builder owns these untracked declaration maps, not the source lock.
    writeFileSync(join(root, 'adapters/node-typescript/src/libs.mjs'), '// generated');
    assert.equal(run('--check').status, 0);
    const extra = join(root, 'adapters/python/src/unpinned.rs');
    writeFileSync(extra, 'untracked source');
    assert.notEqual(run('--check').status, 0);
    rmSync(extra);
    const changed = join(root, 'packages/lekalo-openspec/src/index.txt');
    writeFileSync(changed, 'local edit\n');
    assert.notEqual(run('--check').status, 0);
    assert.notEqual(run('--sync').status, 0);
    assert.equal(readFileSync(changed, 'utf8'), 'local edit\n');
    writeFileSync(changed, 'committed\n');
    rmSync(changed);
    assert.notEqual(run('--check').status, 0);
    writeFileSync(changed, 'committed\n');
    const lockPath = join(root, 'external-packages.lock.json');
    const lock = JSON.parse(readFileSync(lockPath, 'utf8'));
    lock.packages[0].files['../outside'] = { source: '../outside', sha256: 'sha256:bad' };
    writeFileSync(lockPath, JSON.stringify(lock));
    assert.notEqual(run('--check').status, 0);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});
