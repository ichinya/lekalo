// One-off issue #48 section-9 regeneration recipe (plan section 9).
// Repeats the accepted #43/#44 reserve meaning for the 0.3.2 candidate:
// asserts the freshly built binary, regenerates the contract-only lock
// golden plus the two digest constants and the orchestration receipts,
// all inside new external disposable directories. Run only during
// authorized implementation, from the repository root. At the reserve
// stage the target-protocol contract is still 0.3.1; the feature
// contract commit moves it to 0.3.2 and regenerates again.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, mkdtempSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const repo = realpathSync('.');
const bin = process.env.LEKALO_BIN ?? join(repo, 'target', 'debug',
  process.platform === 'win32' ? 'lekalo.exe' : 'lekalo');
assert.equal(JSON.parse(execFileSync(bin, ['--version', '--json'],
  { encoding: 'utf8', cwd: repo })).version, '0.3.2');
const work = realpathSync(mkdtempSync(join(tmpdir(), 'lekalo-reserve-032-')));
const run = (cwd, args) => execFileSync(bin, args,
  { cwd, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
const copyFixture = (name, relative) => {
  const dest = join(work, name);
  cpSync(resolve(repo, relative), dest, { recursive: true });
  return realpathSync(dest);
};
const lockRoot = copyFixture('lock', 'tests/fixtures/lockfile/project');
run(lockRoot, ['lock']);
const bytes = readFileSync(join(lockRoot, 'lekalo.lock'));
assert.equal(bytes.at(-1), 10);
assert.notEqual(bytes.at(-2), 10);
assert.equal(bytes.includes(13), false);
const lock = JSON.parse(bytes.toString('utf8'));
assert.equal(lock.core.version, '0.3.2');
assert.equal(lock.contracts.target_protocol.version, '0.3.1');
assert.equal(lock.resolver.version, '0.2.16');
const hash = createHash('sha256').update(bytes.subarray(0, -1)).digest('hex');
const digest = `sha256:${hash}`;
writeFileSync(join(repo, 'tests/fixtures/lockfile/valid/contract-only.lock.json'), bytes);
writeFileSync(join(repo, 'tests/fixtures/lockfile/valid/contract-only.expect.json'),
  JSON.stringify({ lockDigest: digest, payloadSha256: hash }) + '\n');
for (const relative of ['crates/lekalo-core/tests/lockfile.rs',
  'crates/lekalo-cli/tests/lock.rs']) {
  const path = join(repo, relative);
  const text = readFileSync(path, 'utf8');
  const pattern = /(const GOLDEN_DIGEST: &str =\s*\n\s*")[^"]+(";)/;
  assert.equal((text.match(new RegExp(pattern.source, 'g')) ?? []).length, 1);
  writeFileSync(path, text.replace(pattern, (_, a, b) => a + digest + b));
}
const orchRoot = copyFixture('orchestration', 'tests/fixtures/orchestration/project');
const adapter = ['--', 'node', 'adapters/node-typescript/node-adapter.mjs'];
run(orchRoot, ['lock', ...adapter]);
const receipts = [
  ['generate.dry-run.golden.json',
   run(orchRoot, ['--json', 'generate', '--target', 'node-typescript', '--dry-run', ...adapter])],
  ['generate.apply.golden.json',
   run(orchRoot, ['--json', 'generate', '--target', 'node-typescript', ...adapter])],
  ['verify.full.golden.json',
   run(orchRoot, ['--json', 'verify', '--target', 'node-typescript', ...adapter])]
];
for (const [name, text] of receipts) {
  const receipt = JSON.parse(text);
  assert.equal(receipt.verdict, 'ready');
  for (const target of receipt.targets ?? []) {
    if (Object.hasOwn(target, 'planId')) target.planId = 'plan-normalized';
  }
  writeFileSync(join(repo, 'tests/fixtures/orchestration', name),
    JSON.stringify(receipt, null, 2) + '\n');
}
console.log(JSON.stringify({ work, lockDigest: digest,
  requestDigest: lock.resolver.request_digest }));
