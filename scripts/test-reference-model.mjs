#!/usr/bin/env node
// Actual public exporter and production CLI; no package or runtime fallback.
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, existsSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const binary = process.env.LEKALO_BIN ?? join(root, 'target/debug', process.platform === 'win32' ? 'lekalo.exe' : 'lekalo');
assert.ok(existsSync(binary), 'build the current lekalo CLI before model verification');
const call = (command, args, expected = 0, cwd = root) => {
  const result = spawnSync(command, args, { cwd, encoding: 'utf8', timeout: 60000, windowsHide: true });
  assert.equal(result.status, expected, `${command}: ${result.stdout}\n${result.stderr}`);
  return result.stdout;
};
const home = realpathSync(mkdtempSync(join(tmpdir(), 'lekalo-reference-export-')));
try {
  const ir = [];
  for (const target of ['node', 'laravel', 'brownfield']) {
    const output = join(home, target);
    call(process.execPath, ['scripts/materialize-reference-project.mjs', target, output]);
    call(process.execPath, ['scripts/materialize-reference-project.mjs', target, output], 1);
    if (target === 'brownfield') {
      assert.ok(existsSync(join(output, 'migrations/0000_tasks.sql')));
      assert.ok(!existsSync(join(output, 'schema.sql')), 'MySQL export must not contain the PostgreSQL planner schema');
    } else {
      const result = JSON.parse(call(binary, ['--no-cache', '--json', 'validate', '--project', '.'], 0, output));
      assert.equal(result.status, 'valid');
      ir.push(call(binary, ['--no-cache', '--json', 'load', '--ir', '--project', '.'], 0, output));
    }
  }
  assert.equal(ir[0], ir[1], 'standalone Node/Laravel projects compile the same canonical model');
  console.log(JSON.stringify({ ok: true, gate: 'reference-model-export', exports: 3, identicalModel: true, overwriteRefusals: 3 }));
} finally { rmSync(home, { recursive: true, force: true }); }
