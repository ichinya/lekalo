#!/usr/bin/env node
// Export one standalone example with the exact shared model revision.
// Authoring only: no package installation, adapter launch or overwrite.
import assert from 'node:assert/strict';
import { existsSync, cpSync, readFileSync, writeFileSync, lstatSync, realpathSync } from 'node:fs';
import { resolve, join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const [target, destination, ...extra] = process.argv.slice(2);
assert.ok(['node', 'laravel', 'brownfield'].includes(target) && destination && !extra.length, 'usage: materialize-reference-project.mjs node|laravel|brownfield <new-directory>');
const output = resolve(destination);
assert.ok(!existsSync(output), 'destination must not exist; nothing is overwritten');
assert.ok(existsSync(dirname(output)) && lstatSync(dirname(output)).isDirectory(), 'parent must be an existing directory');
const physical = (path) => process.platform === 'win32' ? path.toLowerCase() : path;
assert.equal(physical(realpathSync(dirname(output))), physical(dirname(output)), 'parent must not alias another directory');
const source = target === 'brownfield' ? 'tests/fixtures/pilot/brownfield-mysql' : `examples/planner-${target}`;
// Read-only integrity verification runs before creating output.
const lock = JSON.parse(readFileSync(join(root, 'examples/reference-projects.lock.json'), 'utf8'));
for (const [path, digest] of Object.entries({ ...lock.files, ...lock.adapters })) {
  assert.equal(`sha256:${createHash('sha256').update(readFileSync(join(root, path))).digest('hex')}`, digest, `source drift: ${path}`);
}
cpSync(join(root, source), output, { recursive: true, filter: (path) => !['vendor', 'node_modules', '.gitignore'].includes(path.split(/[\\/]/).at(-1)) });
if (target !== 'brownfield') {
  cpSync(join(root, 'examples/planner-shared/lekalo'), join(output, 'lekalo'), { recursive: true });
  cpSync(join(root, 'examples/planner-shared/schema.sql'), join(output, 'schema.sql'));
  cpSync(join(root, 'examples/planner-shared/seed.sql'), join(output, 'seed.sql'));
  cpSync(join(root, 'examples/planner-shared/openspec'), join(output, 'openspec'), { recursive: true });
  cpSync(join(root, 'examples/planner-shared/ai-factory-extension.json'), join(output, 'ai-factory-extension.json'));
  cpSync(join(root, 'examples/planner-shared/reference-matrix.json'), join(output, 'reference-profile.json'));
  writeFileSync(join(output, 'README.md'), `# Standalone ${target} planner reference\n\nRevision: ${lock.revision}. Model: lekalo/. Profile: reference-profile.json. Exact origin pins: reference-revision.json.\n\nUse an isolated PostgreSQL 16 database named lekalo_reference. Apply schema.sql then seed.sql. All identities/data are fictional. The supplied trust-authenticated database recipe is for loopback-only development. X-Actor and X-Tenant are synthetic authentication headers. Every mutation requires Idempotency-Key.\n\n${target === 'node' ? 'Run npm ci --ignore-scripts --no-audit --no-fund. Set DATABASE_URL, REFERENCE_NOW=2026-01-02T00:30:00Z and optionally PORT, then npm start (loopback by default).' : 'Run composer install --no-interaction --prefer-dist --no-scripts. PHP >=8.3 with pdo_pgsql and mbstring is required. Set DB_HOST, DB_PORT and REFERENCE_NOW=2026-01-02T00:30:00Z, then php -S 127.0.0.1:8080 router.php. A single PHP worker does not qualify concurrent execution.'}\n\nWith the matching Lekalo executable provisioned, run lekalo --no-cache --json validate --project . and lekalo --no-cache --json load --ir --project . from this directory. No adapter is installed by exporting or validating the model. OpenSpec requirements are under openspec/; ai-factory-extension.json is an optional read-only MCP descriptor, not an installed extension or a gate result. Whole-application managed generation is not claimed.\n`);
}
writeFileSync(join(output, 'reference-revision.json'), `${JSON.stringify({ revision: lock.revision, target, source, inputs: lock.files, adapters: lock.adapters }, null, 2)}\n`);
writeFileSync(join(output, '.gitignore'), 'node_modules/\nvendor/\n.env\n.lekalo/\n');
console.log(JSON.stringify({ ok: true, target, directory: output, revision: lock.revision }));
