import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { assetNames, collectAssets, createArchive, extractArchive, sha256, targets, validatePackageVersions, validateRelease, verifyDownloadedAssets, versionForTag } from './release.mjs';

const tag = 'v1.2.3';
const commit = 'a'.repeat(40);
const temp = () => mkdtempSync(join(tmpdir(), 'lekalo-release-test-'));

function releaseFixture() {
  const directory = temp();
  for (const target of Object.keys(targets)) {
    const { archive, manifest } = assetNames(tag, target);
    writeFileSync(join(directory, archive), `archive fixture for ${target}`);
    writeFileSync(join(directory, manifest), JSON.stringify({
      tag, version: '1.2.3', target, sourceCommit: commit,
      archive, archiveSha256: sha256(join(directory, archive)), binarySha256: 'b'.repeat(64),
    }));
  }
  return directory;
}

test('release tag, published release title and both Cargo versions must agree', () => {
  const release = { tag_name: tag, name: '1.2.3', draft: false };
  const packages = [{ name: 'lekalo-cli', version: '1.2.3' }, { name: 'lekalo-core', version: '1.2.3' }];
  assert.equal(validateRelease(tag, release, packages), '1.2.3');
  assert.equal(validateRelease(tag, { ...release, name: tag }, packages), '1.2.3');
  assert.throws(() => validateRelease(tag, release, [{ ...packages[0], version: '1.2.2' }, packages[1]]), /version differs/);
  assert.throws(() => validateRelease(tag, release, [packages[0], { ...packages[1], version: '1.2.2' }]), /version differs/);
  assert.throws(() => validateRelease(tag, { ...release, draft: true }, packages), /published/);
  assert.throws(() => validateRelease(tag, { ...release, tag_name: 'v1.2.2' }, packages), /tag mismatch/);
  assert.throws(() => validateRelease(tag, { ...release, name: '1.2.2' }, packages), /title/);
  assert.throws(() => validatePackageVersions(tag, [packages[0]]), /Missing workspace package lekalo-core/);
  assert.throws(() => validatePackageVersions(tag, [packages[1]]), /Missing workspace package lekalo-cli/);
});

test('the CI command checks the real Cargo workspace and rejects a mismatched tag', () => {
  const root = dirname(dirname(fileURLToPath(import.meta.url)));
  const version = readFileSync(join(root, 'Cargo.toml'), 'utf8').match(/^version = "([^"]+)"$/m)[1];
  const check = releaseTag => spawnSync(process.execPath, [join(root, 'scripts/release.mjs'), 'check-version'], {
    encoding: 'utf8', cwd: root,
    env: { ...process.env, RELEASE_TAG: releaseTag, RELEASE_SOURCE_DIR: root },
  });
  const matching = check(`v${version}`);
  assert.equal(matching.status, 0, matching.stderr);
  assert.match(matching.stdout, /Cargo package versions match/);
  const mismatched = check('v0.0.0');
  assert.equal(mismatched.status, 1, mismatched.stderr);
  assert.match(mismatched.stderr, /version differs from the release tag/);
});

test('prerelease tags work and malformed or unsafe tags fail', () => {
  assert.equal(versionForTag('v1.2.3-rc.1'), '1.2.3-rc.1');
  for (const invalid of ['1.2.3', 'v01.2.3', 'v1.2', '../v1.2.3', 'v1.2.3\nsource-commit=injected', '--latest']) {
    assert.throws(() => versionForTag(invalid));
  }
  assert.throws(() => assetNames(tag, '../unsupported'), /Unsupported/);
});

test('all five targets are required and every published file receives a checksum', () => {
  const directory = releaseFixture();
  const names = collectAssets(directory, tag, commit);
  assert.equal(names.length, 11);
  const checksums = readFileSync(join(directory, 'SHA256SUMS.txt'), 'utf8');
  assert.equal(checksums.split('\n').filter(Boolean).length, 10);
  for (const name of names.filter(name => name !== 'SHA256SUMS.txt')) {
    assert.ok(checksums.includes(`${sha256(join(directory, name))}  ${name}\n`));
  }
  assert.deepEqual(collectAssets(directory, tag, commit), names, 'Collection is repeatable');
});

test('missing platform, corrupt archive and extra files block publication', () => {
  const incomplete = temp();
  assert.throws(() => collectAssets(incomplete, tag, commit), /ENOENT/);
  const corrupt = releaseFixture();
  writeFileSync(join(corrupt, assetNames(tag, Object.keys(targets)[0]).archive), 'changed');
  assert.throws(() => collectAssets(corrupt, tag, commit), /checksum mismatch/);
  const extra = releaseFixture();
  writeFileSync(join(extra, 'credentials.txt'), 'must never be published');
  assert.throws(() => collectAssets(extra, tag, commit), /exactly all five/);
});

test('mixed commits, versions, tags, targets and archive names block publication', () => {
  for (const [field, value] of Object.entries({
    sourceCommit: 'c'.repeat(40), version: '1.2.2', tag: 'v1.2.2', target: 'unsupported', archive: '../other', binarySha256: 'bad',
  })) {
    const directory = releaseFixture();
    const manifest = join(directory, assetNames(tag, Object.keys(targets)[0]).manifest);
    writeFileSync(manifest, JSON.stringify({ ...JSON.parse(readFileSync(manifest, 'utf8')), [field]: value }));
    assert.throws(() => collectAssets(directory, tag, commit), undefined, field);
  }
});

test('an archive round trip preserves the executable bytes and contains only the selected public files', () => {
  const target = process.platform === 'win32' ? 'x86_64-pc-windows-msvc' :
    process.platform === 'darwin' ? `${process.arch === 'arm64' ? 'aarch64' : 'x86_64'}-apple-darwin` :
      `${process.arch === 'arm64' ? 'aarch64' : 'x86_64'}-unknown-linux-gnu`;
  const stage = temp();
  const output = temp();
  const { stem } = assetNames(tag, target);
  mkdirSync(join(stage, stem));
  const binary = process.platform === 'win32' ? 'lekalo.exe' : 'lekalo';
  writeFileSync(join(stage, stem, binary), 'executable fixture\0');
  writeFileSync(join(stage, stem, 'README.md'), 'public readme');
  writeFileSync(join(stage, 'secret.env'), 'outside archive');
  const archive = createArchive(stage, output, tag, target);
  const extracted = temp();
  extractArchive(archive, extracted, target);
  assert.deepEqual(readdirSync(extracted), [stem]);
  assert.deepEqual(readdirSync(join(extracted, stem)).sort(), ['README.md', binary].sort());
  assert.equal(sha256(join(extracted, stem, binary)), sha256(join(stage, stem, binary)));
});

test('verification detects missing or changed files after upload', () => {
  const local = releaseFixture();
  const downloaded = releaseFixture();
  const names = collectAssets(local, tag, commit);
  collectAssets(downloaded, tag, commit);
  verifyDownloadedAssets(local, downloaded, names);
  writeFileSync(join(downloaded, names[0]), 'corrupted upload');
  assert.throws(() => verifyDownloadedAssets(local, downloaded, names), /Uploaded asset differs/);
  assert.throws(() => verifyDownloadedAssets(local, temp(), names), /ENOENT/);
});
