import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { chmodSync, lstatSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const binaryName = 'lekalo';
const manifestName = 'contracts-cli.json';
const digest = bytes => createHash('sha256').update(bytes).digest('hex');

function requireSourceSha(sourceSha) {
  if (!/^[a-f0-9]{40}$/.test(sourceSha ?? '')) throw Error('Expected a full source SHA');
}

function plainFile(path) {
  const info = lstatSync(path);
  if (!info.isFile() || info.isSymbolicLink()) throw Error(`Expected a plain file: ${path}`);
  return readFileSync(path);
}

// This artifact belongs only to the Linux/x64 Contracts producer and consumers.
// SDK oracle and fresh-target builds must not use it.
export function prepare({ binary, destination, sourceSha, stripSymbols = false, platform = process.platform, arch = process.arch }) {
  requireSourceSha(sourceSha);
  if (platform !== 'linux' || arch !== 'x64') throw Error('Contracts CLI requires linux/x64');
  const bytes = plainFile(binary);
  if (bytes.length === 0) throw Error('Contracts CLI is empty');
  // Refuse an existing destination; never clean another job's files.
  mkdirSync(dirname(destination), { recursive: true });
  mkdirSync(destination);
  const packagedBinary = join(destination, binaryName);
  writeFileSync(packagedBinary, bytes, { flag: 'wx', mode: 0o755 });
  // Strip only the artifact copy. Keep Cargo's output and fingerprints intact.
  // A failed strip aborts preparation before a usable manifest is written.
  if (stripSymbols) execFileSync('strip', ['--strip-unneeded', packagedBinary], { stdio: 'pipe' });
  const packagedBytes = plainFile(packagedBinary);
  const manifest = { schemaVersion: 1, sourceSha, binary: binaryName, sha256: digest(packagedBytes), bytes: packagedBytes.length, platform, arch };
  writeFileSync(join(destination, manifestName), `${JSON.stringify(manifest, null, 2)}\n`, { flag: 'wx' });
  return manifest;
}

export function verify({ directory, sourceSha, platform = process.platform, arch = process.arch }) {
  requireSourceSha(sourceSha);
  const info = lstatSync(directory);
  if (!info.isDirectory() || info.isSymbolicLink()) throw Error('Expected a plain artifact directory');
  const manifest = JSON.parse(plainFile(join(directory, manifestName)).toString('utf8'));
  if (!manifest || manifest.schemaVersion !== 1 || manifest.binary !== binaryName
      || manifest.sourceSha !== sourceSha || manifest.platform !== 'linux' || manifest.arch !== 'x64'
      || platform !== manifest.platform || arch !== manifest.arch
      || !Number.isSafeInteger(manifest.bytes) || manifest.bytes <= 0
      || !/^[a-f0-9]{64}$/.test(manifest.sha256 ?? '')) {
    throw Error('Contracts CLI manifest does not match this checkout or runtime');
  }
  const binary = join(directory, binaryName);
  const bytes = plainFile(binary);
  if (bytes.length !== manifest.bytes || digest(bytes) !== manifest.sha256) throw Error('Contracts CLI digest or size mismatch');
  // Actions artifact downloads normalize permissions. Restore execution only
  // after validating provenance and content, before any contract probe runs.
  chmodSync(binary, 0o755);
  return manifest;
}

function main() {
  const [mode, location, ...extra] = process.argv.slice(2);
  const stripSymbols = mode === 'prepare' && extra.length === 1 && extra[0] === '--strip';
  if (!['prepare', 'verify'].includes(mode) || !location || (extra.length && !stripSymbols)) {
    throw Error('Usage: node scripts/ci-contracts-cli.mjs prepare <artifact-directory> [--strip] | verify <artifact-directory>');
  }
  const sourceSha = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
  requireSourceSha(sourceSha);
  if (process.env.GITHUB_SHA && process.env.GITHUB_SHA !== sourceSha) throw Error('Checkout SHA differs from GITHUB_SHA');
  const directory = resolve(location);
  const manifest = mode === 'prepare'
    ? prepare({ binary: resolve(process.env.CARGO_TARGET_DIR || 'target', 'debug', binaryName), destination: directory, sourceSha, stripSymbols })
    : verify({ directory, sourceSha });
  console.log(`Contracts CLI ${mode}: ${manifest.sourceSha} sha256:${manifest.sha256}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(); } catch (error) { console.error(error.message); process.exitCode = 1; }
}
