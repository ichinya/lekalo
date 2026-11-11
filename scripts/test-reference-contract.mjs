#!/usr/bin/env node
// File-level OWNER declarations for the actual maintained reference handlers.
// Contract checks prove declared signatures/effects + fingerprints + coverage
// presence. The separate real HTTP/PostgreSQL gate proves runtime behavior.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdtempSync, rmSync, realpathSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const binary = process.env.LEKALO_BIN ?? join(root, 'target/debug', process.platform === 'win32' ? 'lekalo.exe' : 'lekalo');
const sha = (bytes) => `sha256:${createHash('sha256').update(bytes).digest('hex')}`;
const type = (value) => typeof value === 'string' ? value : value.ref ?? `${Object.keys(value)[0]}(${type(Object.values(value)[0])})`;
const definitions = ['commands', 'queries'].flatMap((file) => JSON.parse(readFileSync(join(root, `examples/planner-shared/lekalo/modules/planner/${file}.yaml`), 'utf8')).definitions);
const operations = definitions.filter((d) => ['command', 'query'].includes(d.kind));
const coverage = JSON.parse(readFileSync(join(root, 'examples/planner-shared/reference-matrix.json'), 'utf8')).nativeCoverage;
const call = (command, args, cwd, expected = 0) => {
  const result = spawnSync(command, args, { cwd, encoding: 'utf8', windowsHide: true, timeout: 60000 });
  assert.equal(result.status, expected, `${args.join(' ')}\n${result.stdout}\n${result.stderr}`);
  return JSON.parse(result.stdout || result.stderr);
};
const home = realpathSync(mkdtempSync(join(tmpdir(), 'lekalo-reference-contract-')));
try {
  for (const target of ['node', 'laravel']) {
    const output = join(home, target);
    call(process.execPath, [join(root, 'scripts/materialize-reference-project.mjs'), target, output], root);
    const source = target === 'node' ? 'src/server.mjs' : 'src/planner.php';
    const bytes = readFileSync(join(output, source));
    const manifest = JSON.parse(readFileSync(join(root, `adapters/${target === 'node' ? 'node-typescript' : 'php-laravel'}/adapter.manifest.json`), 'utf8'));
    const declaration = {
      schemaVersion: 'lekalo/contracted-declaration/v0.4.0',
      adapter: { id: manifest.adapter.id, version: manifest.adapter.version, digest: null },
      project: 'planner', revision: sha(readFileSync(join(root, 'examples/reference-projects.lock.json'))),
      symbols: operations.map((d) => ({ id: d.id, kind: d.kind, source: { path: source, line: 1 }, fingerprint: sha(bytes),
        signature: { inputs: (d.input ?? []).map((f) => ({ name: f.name, type: type(f.type), required: f.required ?? true })), output: d.kind === 'query' ? type(d.returns) : null, reads: d.reads ?? [] },
        effects: (d.effects ?? []).map((id) => { const effect = definitions.find((e) => e.id === id); return { kind: effect.operation, subject: effect.entity }; }),
      })), artifacts: [],
    };
    writeFileSync(join(output, 'reference-declaration.json'), `${JSON.stringify(declaration, null, 2)}\n`);
    call(binary, ['--json', 'contract', 'update', '--declaration', 'reference-declaration.json'], output);
    const missing = call(binary, ['--no-cache', '--json', 'contract', 'check', '--module', 'planner'], output, 1);
    assert.equal(missing.diagnostics.length, operations.length);
    assert.ok(missing.diagnostics.every((d) => d.id === 'contracted.coverage-missing'));
    for (const d of operations) {
      assert.ok(coverage[d.id]);
      call(binary, ['--json', 'contract', 'attach', d.id, '--native-test', `reference.${target}.${coverage[d.id]}`], output);
    }
    call(binary, ['--no-cache', '--json', 'contract', 'check', '--module', 'planner'], output);
    assert.deepEqual(readFileSync(join(output, source)), bytes, 'contracting must not rewrite maintained source');
    writeFileSync(join(output, source), Buffer.concat([bytes, Buffer.from('\n// deliberate source drift\n')]));
    const stale = call(binary, ['--no-cache', '--json', 'contract', 'check', '--module', 'planner'], output, 1);
    assert.equal(stale.diagnostics.length, operations.length);
    assert.ok(stale.diagnostics.every((d) => d.id === 'contracted.binding-drift' && d.data.detail === 'fingerprint-mismatch'));
    console.log(`ok - ${target}: ${operations.length} actual reference bindings, missing coverage refused, attached conformance and source drift refusal`);
  }
  console.log(JSON.stringify({ ok: true, gate: 'reference-contracted', targets: 2, bindings: operations.length * 2, runtimeQualification: 'separate-http-postgres-gate' }));
} finally { rmSync(home, { recursive: true, force: true }); }
