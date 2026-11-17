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
    mkdirSync(source); mkdirSync(join(host, 'scripts/lib'), {recursive: true});
    copyFileSync(fileURLToPath(new URL('./lib/external-services.mjs', import.meta.url)), join(host,'scripts/lib/external-services.mjs'));
    const git = args => execFileSync('git', args, {cwd: source, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe']}).trim();
    git(['init']);
    writeFileSync(join(source, 'payload.txt'), 'pinned bytes\n');
    mkdirSync(join(source, 'scripts'));
    for (const name of ['probe.mjs','test-node-native-tools.mjs']) writeFileSync(join(source,'scripts',name), 'console.log(process.env.LEKALO_BIN);\n');
    git(['add', 'payload.txt', 'scripts']);
    git(['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', '-c', `core.hooksPath=${join(root, 'no-hooks')}`, '-c', 'commit.gpgsign=false', 'commit', '-m', 'fixture']);
    const commit = git(['rev-parse', 'HEAD']);
    const script = join(host, 'scripts/external-tool.mjs');
    copyFileSync(fileURLToPath(new URL('./external-tool.mjs', import.meta.url)), script);
    writeFileSync(join(host, 'external-tools.lock.json'), JSON.stringify({services:{},packages: {fixture: {repository: source, commit}, 'lekalo-typescript': {repository: source, commit}}}));
    const run = (name = 'fixture', override = '{}') => spawnSync(process.execPath, [script, name], {
      encoding: 'utf8', env: {...process.env, LEKALO_EXTERNAL_TOOL_CACHE: cache, LEKALO_EXTERNAL_TOOLS: override},
    });
    let result = run();
    assert.equal(result.status, 0, result.stderr);
    const checkout = result.stdout.trim();
    assert.equal(readFileSync(join(checkout, 'payload.txt'), 'utf8'), 'pinned bytes\n');
    assert.equal(run().stdout.trim(), checkout);
    const cleanEnv = {...process.env, LEKALO_EXTERNAL_TOOL_CACHE:cache, LEKALO_EXTERNAL_TOOLS:'{}'};
    for (const key of ['LEKALO_BIN','CARGO_TARGET_DIR','RUNTIME_PROFILE']) delete cleanEnv[key];
    const suffix=process.platform==='win32'?'.exe':'';
    for (const [entry,env,expected] of [
      ['probe.mjs',{},join(host,'target/debug','lekalo'+suffix)],
      ['probe.mjs',{RUNTIME_PROFILE:'release'},join(host,'target/release','lekalo'+suffix)],
      ['test-node-native-tools.mjs',{},join(host,'target/release','lekalo'+suffix)],
      ['probe.mjs',{RUNTIME_PROFILE:'release',LEKALO_BIN:join(root,'explicit')},join(root,'explicit')],
    ]) {
      const launched=spawnSync(process.execPath,[script,'fixture','scripts/'+entry],{encoding:'utf8',env:{...cleanEnv,...env}});
      assert.equal(launched.status,0,launched.stderr);
      assert.equal(launched.stdout.trim(),expected);
    }
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
