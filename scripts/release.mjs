import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { appendFileSync, chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export const targets = {
  'x86_64-unknown-linux-gnu': { platform: 'linux', arch: 'x64', extension: '.tar.gz' },
  'aarch64-unknown-linux-gnu': { platform: 'linux', arch: 'arm64', extension: '.tar.gz' },
  'x86_64-apple-darwin': { platform: 'darwin', arch: 'x64', extension: '.tar.gz' },
  'aarch64-apple-darwin': { platform: 'darwin', arch: 'arm64', extension: '.tar.gz' },
  'x86_64-pc-windows-msvc': { platform: 'win32', arch: 'x64', extension: '.zip' },
};

const run = (command, args, options = {}) => execFileSync(command, args, {
  encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], ...options,
}).trim();
export const sha256 = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const json = path => JSON.parse(readFileSync(path, 'utf8'));

export function versionForTag(tag) {
  assert.match(tag ?? '', /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/, 'Expected a v-prefixed release version');
  return tag.slice(1);
}

export function validatePackageVersions(tag, packages) {
  const version = versionForTag(tag);
  for (const name of ['lekalo-cli', 'lekalo-core']) {
    const pkg = packages.find(pkg => pkg.name === name);
    assert.ok(pkg, `Missing workspace package ${name}`);
    assert.equal(pkg.version, version, `${pkg.name} version differs from the release tag`);
  }
  return version;
}

export function validateRelease(tag, release, packages) {
  const version = versionForTag(tag);
  assert.equal(release.tag_name, tag, 'Release tag mismatch');
  assert.equal(release.draft, false, 'Release must be published');
  assert.ok(release.name === version || release.name === tag, 'Release title must equal the version or tag');
  if (packages !== undefined) validatePackageVersions(tag, packages);
  return version;
}

export function assetNames(tag, target) {
  versionForTag(tag);
  assert.ok(Object.hasOwn(targets, target), 'Unsupported release target');
  const stem = `lekalo-${tag}-${target}`;
  return { stem, archive: stem + targets[target].extension, manifest: stem + '.build.json' };
}

function sourceCommit(source) {
  assert.equal(run('git', ['status', '--porcelain', '--untracked-files=all'], { cwd: source }), '', 'Release source must be clean');
  return run('git', ['rev-parse', 'HEAD'], { cwd: source });
}

function sourcePackages(source) {
  const metadata = JSON.parse(run('cargo', ['metadata', '--locked', '--no-deps', '--format-version', '1'], { cwd: source }));
  return ['lekalo-cli', 'lekalo-core'].map(name => {
    const pkg = metadata.packages.find(pkg => pkg.name === name);
    assert.ok(pkg, `Missing workspace package ${name}`);
    return pkg;
  });
}

function releaseInfo(tag) {
  versionForTag(tag);
  return JSON.parse(run('gh', ['api', `repos/${process.env.GITHUB_REPOSITORY}/releases/tags/${tag}`]));
}

export function createArchive(stage, output, tag, target) {
  const { stem, archive } = assetNames(tag, target);
  const archivePath = resolve(output, archive);
  mkdirSync(output, { recursive: true });
  if (targets[target].platform === 'win32') {
    assert.equal(process.platform, 'win32', 'Windows archives must be built on Windows');
    run('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
      '$ErrorActionPreference = "Stop"; Compress-Archive -LiteralPath $env:LEKALO_ARCHIVE_SOURCE -DestinationPath $env:LEKALO_ARCHIVE_DESTINATION'], {
      env: { ...process.env, LEKALO_ARCHIVE_SOURCE: resolve(stage, stem), LEKALO_ARCHIVE_DESTINATION: archivePath },
    });
  } else {
    run('tar', ['-czf', archivePath, '-C', stage, stem]);
  }
  return archivePath;
}

export function extractArchive(archive, destination, target) {
  mkdirSync(destination, { recursive: true });
  if (targets[target].platform === 'win32') {
    run('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
      '$ErrorActionPreference = "Stop"; Expand-Archive -LiteralPath $env:LEKALO_ARCHIVE_SOURCE -DestinationPath $env:LEKALO_ARCHIVE_DESTINATION'], {
      env: { ...process.env, LEKALO_ARCHIVE_SOURCE: resolve(archive), LEKALO_ARCHIVE_DESTINATION: resolve(destination) },
    });
  } else {
    run('tar', ['-xzf', archive, '-C', destination]);
  }
}

export function collectAssets(directory, tag, commit) {
  assert.match(commit ?? '', /^[0-9a-f]{40}$/, 'Expected the tagged commit SHA');
  const version = versionForTag(tag);
  const expected = Object.keys(targets).flatMap(target => {
    const { archive, manifest } = assetNames(tag, target);
    const record = json(join(directory, manifest));
    assert.equal(record.tag, tag, 'Build tag mismatch');
    assert.equal(record.version, version, 'Build version mismatch');
    assert.equal(record.sourceCommit, commit, 'Build source commit mismatch');
    assert.equal(record.target, target, 'Build target mismatch');
    assert.equal(record.archive, archive, 'Build archive name mismatch');
    assert.equal(record.archiveSha256, sha256(join(directory, archive)), 'Archive checksum mismatch');
    assert.match(record.binarySha256, /^[0-9a-f]{64}$/, 'Missing executable checksum');
    return [archive, manifest];
  }).sort();
  assert.deepEqual(readdirSync(directory).filter(name => name !== 'SHA256SUMS.txt').sort(), expected, 'Release must contain exactly all five archives and their build records');
  const checksums = expected.map(name => `${sha256(join(directory, name))}  ${name}\n`).join('');
  writeFileSync(join(directory, 'SHA256SUMS.txt'), checksums);
  return [...expected, 'SHA256SUMS.txt'];
}

export function verifyDownloadedAssets(local, downloaded, names) {
  for (const name of names) {
    assert.equal(sha256(join(downloaded, name)), sha256(join(local, name)), `Uploaded asset differs: ${name}`);
  }
}

function packageBinary(source, output, tag, target, commit) {
  const version = versionForTag(tag);
  const { stem, archive, manifest } = assetNames(tag, target);
  assert.equal(process.platform, targets[target].platform, 'Build must run on the target OS');
  assert.equal(process.arch, targets[target].arch, 'Build must run on the target architecture');
  assert.equal(sourceCommit(source), commit, 'Source commit changed');
  validatePackageVersions(tag, sourcePackages(source));
  const binaryName = process.platform === 'win32' ? 'lekalo.exe' : 'lekalo';
  const binary = join(source, 'target', target, 'release', binaryName);
  assert.equal(run(binary, ['--version']), `lekalo ${version}`, 'Compiled executable version mismatch');
  run(binary, ['--help']);

  const stage = mkdtempSync(join(tmpdir(), 'lekalo-release-stage-'));
  const contents = join(stage, stem);
  mkdirSync(contents);
  copyFileSync(binary, join(contents, binaryName));
  if (process.platform !== 'win32') chmodSync(join(contents, binaryName), 0o755);
  // Ship only the executable and public readme; no checkout, cache or credentials.
  copyFileSync(join(source, 'README.md'), join(contents, 'README.md'));
  const archivePath = createArchive(stage, output, tag, target);
  const extracted = mkdtempSync(join(tmpdir(), 'lekalo-release-smoke-'));
  extractArchive(archivePath, extracted, target);
  const extractedBinary = join(extracted, stem, binaryName);
  assert.equal(sha256(extractedBinary), sha256(binary), 'Extracted executable differs');
  assert.equal(run(extractedBinary, ['--version'], { cwd: extracted }), `lekalo ${version}`, 'Extracted executable version mismatch');
  run(extractedBinary, ['--help'], { cwd: extracted });
  run(extractedBinary, ['compatibility'], { cwd: extracted });
  assert.equal(sourceCommit(source), commit, 'Build modified release source');

  writeFileSync(join(output, manifest), JSON.stringify({
    tag, version, target, sourceCommit: commit,
    workflowCommit: process.env.GITHUB_WORKFLOW_SHA ?? null,
    buildRun: process.env.GITHUB_RUN_ID ? `${process.env.GITHUB_SERVER_URL}/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}` : null,
    rustc: run('rustc', ['--version', '--verbose']),
    cargoLockSha256: sha256(join(source, 'Cargo.lock')),
    binarySha256: sha256(binary), archive, archiveSha256: sha256(archivePath),
  }, null, 2) + '\n');
  console.log(`Verified ${archive}`);
}

function main(mode) {
  const tag = process.env.RELEASE_TAG;
  versionForTag(tag);
  const source = resolve(process.env.RELEASE_SOURCE_DIR ?? 'source');
  const output = resolve('dist');
  const commit = process.env.SOURCE_COMMIT;
  switch (mode) {
    case 'check-version':
      validatePackageVersions(tag, sourcePackages(source));
      console.log(`Cargo package versions match ${tag}`);
      break;
    case 'prepare': {
      const sha = sourceCommit(source);
      validateRelease(tag, releaseInfo(tag), sourcePackages(source));
      appendFileSync(process.env.GITHUB_OUTPUT, `source-commit=${sha}\n`);
      console.log(`Release ${tag} will build source commit ${sha}`);
      break;
    }
    case 'package':
      packageBinary(source, output, tag, process.env.RELEASE_TARGET, commit);
      break;
    case 'collect':
      collectAssets(output, tag, commit);
      break;
    case 'publish': {
      assert.equal(sourceCommit(source), commit, 'Release tag moved during the build');
      validateRelease(tag, releaseInfo(tag));
      const names = collectAssets(output, tag, commit);
      run('gh', ['release', 'upload', tag, '--repo', process.env.GITHUB_REPOSITORY, '--clobber', ...names.map(name => join(output, name))]);
      const downloaded = mkdtempSync(join(tmpdir(), 'lekalo-release-download-'));
      run('gh', ['release', 'download', tag, '--repo', process.env.GITHUB_REPOSITORY, '--dir', downloaded,
        ...names.flatMap(name => ['--pattern', name])]);
      verifyDownloadedAssets(output, downloaded, names);
      console.log(`Uploaded and downloaded all ${names.length} verified assets for ${tag}`);
      break;
    }
    default:
      throw new Error('Usage: node scripts/release.mjs check-version|prepare|package|collect|publish');
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try { main(process.argv[2]); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
