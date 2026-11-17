import assert from 'node:assert/strict';
import {execFileSync, spawnSync} from 'node:child_process';
import {copyFileSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {test} from 'node:test';

test('external tooling checks out pinned bytes, reuses clean caches and refuses drift', () => {
  const root = mkdtempSync(join(process.env.BB_THREAD_STORAGE ?? tmpdir(), 'lekalo-external-tool-'));
  try {
    const source = join(root, 'source'), host = join(root, 'host'), cache = join(root, 'cache');
    mkdirSync(source); mkdirSync(join(host, 'scripts'), {recursive: true});
    const git = args => execFileSync('git', args, {cwd: source, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe']}).trim();
    git(['init']);
    writeFileSync(join(source, 'payload.txt'), 'pinned bytes\n');
    git(['add', 'payload.txt']);
    git(['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', '-c', `core.hooksPath=${join(root, 'no-hooks')}`, '-c', 'commit.gpgsign=false', 'commit', '-m', 'fixture']);
    const commit = git(['rev-parse', 'HEAD']);
    const script = join(host, 'scripts/external-tool.mjs');
    copyFileSync(fileURLToPath(new URL('./external-tool.mjs', import.meta.url)), script);
    writeFileSync(join(host, 'external-tools.lock.json'), JSON.stringify({packages: {fixture: {repository: source, commit}}}));
    const run = (name = 'fixture', override = '{}') => spawnSync(process.execPath, [script, name], {
      encoding: 'utf8', env: {...process.env, LEKALO_EXTERNAL_TOOL_CACHE: cache, LEKALO_EXTERNAL_TOOLS: override},
    });
    let result = run();
    assert.equal(result.status, 0, result.stderr);
    const checkout = result.stdout.trim();
    assert.equal(readFileSync(join(checkout, 'payload.txt'), 'utf8'), 'pinned bytes\n');
    assert.equal(run().stdout.trim(), checkout);
    writeFileSync(join(checkout, 'payload.txt'), 'changed\n');
    result = run();
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /not the pinned clean checkout/);
    result = run('fixture', JSON.stringify({fixture: source}));
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.trim(), source);
    assert.notEqual(run('missing').status, 0);
  } finally {
    rmSync(root, {recursive: true, force: true});
  }
});
