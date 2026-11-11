#!/usr/bin/env node
// Explicit authoring operation. Verification never rewrites this lock.
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const sha = (path) => `sha256:${createHash('sha256').update(readFileSync(join(root, path))).digest('hex')}`;
function files(home) {
  return readdirSync(join(root, home), { withFileTypes: true }).flatMap((entry) => {
    if (['node_modules', 'vendor', '.gitignore'].includes(entry.name)) return [];
    const path = `${home}/${entry.name}`; return entry.isDirectory() ? files(path) : [path];
  });
}
const paths = ['planner-shared', 'planner-node', 'planner-laravel', 'brownfield-hono-drizzle-mysql'].flatMap((name) => files(`examples/${name}`));
paths.push(...files('tests/fixtures/pilot/brownfield-mysql'), 'examples/compose.yaml', 'examples/extraction.compose.yaml',
  'scripts/test-reference-projects.mjs', 'scripts/materialize-reference-project.mjs', 'scripts/test-reference-contract.mjs', 'scripts/lib/reference-clock-probe.mjs');
const lock = {
  formatVersion: 1,
  revision: JSON.parse(readFileSync(join(root, 'examples/planner-shared/scenarios.json'), 'utf8')).revision,
  source: 'Synthetic public reference projects; model derived from the public issue-60 routes corpus. Historical test fixtures remain independent regression inputs.',
  files: Object.fromEntries(paths.sort().map((path) => [path, sha(path)])),
  adapters: Object.fromEntries(['adapters/node-typescript/adapter.manifest.json', 'adapters/php-laravel/adapter.manifest.json'].map((path) => [path, sha(path)])),
};
writeFileSync(join(root, 'examples/reference-projects.lock.json'), `${JSON.stringify(lock, null, 2)}\n`);
console.log(`Updated reference lock: ${paths.length} files, 2 exact adapter manifests`);
