import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';

const source = fileURLToPath(new URL('./test-fixture-provenance.mjs', import.meta.url));
const ownedBase = resolve(tmpdir());
const root = mkdtempSync(join(ownedBase, 'provenance-regression-'));
assert.equal(dirname(resolve(root)), resolve(ownedBase));
assert.match(basename(root), /^provenance-regression-/);
const temporary = join(root, 'tmp');
const env = {
  ...process.env, TMPDIR: temporary, TEMP: temporary, TMP: temporary,
  GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: process.platform === 'win32' ? 'NUL' : '/dev/null',
};
const git = args => execFileSync('git', args, {cwd:root, env, encoding:'utf8', stdio:'pipe', timeout:10000});
const gate = () => spawnSync(process.execPath, ['scripts/test-fixture-provenance.mjs'], {cwd:root, env, encoding:'utf8', timeout:45000});
const sha = value => createHash('sha256').update(value).digest('hex');
const results = [];
try {
  mkdirSync(join(root, 'scripts'));
  mkdirSync(join(root, 'tests/fixtures/binary'), {recursive:true});
  mkdirSync(temporary);
  writeFileSync(join(root, 'scripts/test-fixture-provenance.mjs'), readFileSync(source));
  writeFileSync(join(root, 'tests/fixtures/fixture-provenance.json'), JSON.stringify({
    manifestId:'dev.lekalo.fixture-provenance', version:'1', families:[{family:'binary', origin:'synthetic'}],
  }));
  const fixture = join(root, 'tests/fixtures/binary/sample.sqlite');
  // Larger than a pipe buffer; retain NUL, CRLF and non-UTF-8 bytes.
  writeFileSync(fixture, Buffer.concat([Buffer.alloc(256 * 1024, 65), Buffer.from([0,65,13,10,66,0,255])]));
  const before = sha(readFileSync(fixture));
  const attributes = join(root, '.gitattributes');
  writeFileSync(attributes, 'tests/fixtures/** -text !eol\n');
  git(['init', '--quiet']);
  git(['-c','core.autocrlf=false','add','--','.gitattributes','tests/fixtures']);

  // Use the same synchronous parent/child shape as documentation replay.
  // Repetition exercises the intermittent macOS stdin/exit circular wait.
  for (let attempt = 0; attempt < 5; attempt++) {
    const positive = gate();
    assert.equal(positive.status, 0, positive.stderr || positive.error?.code);
    assert.equal(JSON.parse(positive.stdout).binaryFixtures, 1);
  }
  assert.deepEqual(readdirSync(temporary), [], 'canary temporary directory leaked');
  results.push({case:'five repeated large tracked NUL/CRLF fixture checks', passed:true});

  const index = join(root, '.git/index');
  const savedIndex = readFileSync(index);
  writeFileSync(index, 'invalid-index');
  const invalid = gate();
  writeFileSync(index, savedIndex);
  assert.notEqual(invalid.status, 0, 'corrupt Git index was silently accepted');
  assert.match(invalid.stdout, /FAIL - fixture-provenance-git-tracked-files/);
  results.push({case:'corrupt real Git index fails closed', passed:true});

  const filter = join(root, 'rewrite-clean.mjs');
  writeFileSync(filter, "import{readFileSync}from'node:fs';process.stdout.write(readFileSync(0).toString('latin1').replace('clean-filter-canary','clean-filter-rewritten'),'latin1');\n");
  const quote = value => "'" + value.replaceAll("'", "'\\''") + "'";
  git(['config','filter.rewrite.clean', `${quote(process.execPath.replaceAll('\\','/'))} ${quote(filter.replaceAll('\\','/'))}`]);
  writeFileSync(attributes, 'tests/fixtures/** -text !eol\ntests/fixtures/binary/sample.sqlite filter=rewrite\n');
  const rewrites = gate();
  assert.notEqual(rewrites.status, 0, 'real clean filter rewrote canary bytes without rejection');
  assert.match(rewrites.stderr, /clean filtering rewrites binary fixture bytes/);
  assert.deepEqual(readdirSync(temporary), [], 'failed filter comparison leaked its canary');
  assert.equal(sha(readFileSync(fixture)), before, 'real fixture was modified');
  results.push({case:'real clean-filter byte rewrite rejected; fixture unchanged', passed:true});

  renameSync(join(root, '.git'), join(root, '.git-held'));
  const archive = gate();
  assert.equal(archive.status, 0, archive.stderr);
  assert.equal(JSON.parse(archive.stdout).binaryFixtures, 0);
  results.push({case:'explicit source archive behavior retained', passed:true});
  console.log(JSON.stringify({passed:true,cases:results}));
} finally {
  // Exact absolute child created by this process; never a caller-provided path.
  assert.equal(dirname(resolve(root)), resolve(ownedBase));
  assert.match(basename(root), /^provenance-regression-/);
  rmSync(root, {recursive:true, force:true});
}
