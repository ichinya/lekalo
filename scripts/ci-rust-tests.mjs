#!/usr/bin/env node
// The root workspace's normal CI test groups. Qualification stays in its
// existing workflow steps with their platform conditions and time limits.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const unix = platform => platform === 'linux' || platform === 'darwin';
const testArgs = pkg => ['test', '--locked', '-p', pkg, '--no-fail-fast'];
const windowsWatch = 'watch::targets::tests::';

export function buildGroups(metadata, platform = process.platform) {
  assert(['linux', 'darwin', 'win32'].includes(platform), 'unsupported CI platform');
  const members = metadata.workspace_members.map(id => {
    const pkg = metadata.packages.find(p => p.id === id);
    assert(pkg, `missing workspace package ${id}`);
    return pkg;
  });
  assert.deepEqual(members.map(p => p.name).sort(), ['lekalo-cli', 'lekalo-core'], 'unclassified workspace members');
  for (const pkg of members) {
    const seen = new Set();
    for (const target of pkg.targets) {
      assert.equal(target.kind.length, 1, `unclassified target ${pkg.name}/${target.name}`);
      const kind = target.kind[0];
      assert(['lib', 'bin', 'test', 'example', 'custom-build'].includes(kind), `unclassified target kind ${kind}`);
      const key = `${kind}/${target.name}`;
      assert(!seen.has(key), `duplicate target ${pkg.name}/${key}`);
      seen.add(key);
      assert(!(target['required-features']?.length), `conditional target needs an explicit policy: ${pkg.name}/${key}`);
    }
  }
  const core = members.find(p => p.name === 'lekalo-core');
  const cli = members.find(p => p.name === 'lekalo-cli');
  const libs = core.targets.filter(t => t.kind[0] === 'lib');
  assert.equal(libs.length, 1, 'core must have one library');
  assert(libs[0].test && libs[0].doctest, 'core unit and documentation tests must remain enabled');
  const integration = core.targets.filter(t => t.kind[0] === 'test' && t.test);
  for (const name of ['target_protocol_security', 'target_transport_unix']) {
    assert(integration.some(t => t.name === name), `missing qualification target ${name}`);
  }
  const groups = [];
  const add = (id, args, targets, delegated = false) => groups.push({ id, args, targets, delegated });
  // A package-default invocation preserves CLI bins, tests, doctests and
  // compile-only examples, including newly discovered default targets.
  add('cli', testArgs(cli.name), cli.targets.filter(t => t.kind[0] !== 'custom-build').map(t => `cli/${t.kind[0]}/${t.name}`));
  if (unix(platform)) {
    add('unix-confinement', ['test', '-p', core.name, '--lib', 'unix_confined_', '--locked', '--', '--nocapture'], ['core/lib/unix_confined_'], true);
  }
  // These tests launch deadline-bound Node fixtures. Keep Windows process
  // startup out of contention with the rest of the parallel library suite.
  // Bound their own concurrency without serializing all deadline probes;
  // preserve their real deadlines and run each case exactly once.
  if (platform === 'win32') {
    add('windows-watch', [...testArgs(core.name), '--lib', windowsWatch, '--', '--test-threads=2'], ['core/lib/watch-targets']);
    assert(integration.some(t => t.name === 'target_protocol_boundaries'), 'missing Windows boundary target');
    add('windows-boundaries', [...testArgs(core.name), '--test', 'target_protocol_boundaries', '--', '--test-threads=2'], ['core/test/target_protocol_boundaries']);
  }
  add('core-lib', [...testArgs(core.name), '--lib', '--', '--skip', unix(platform) ? 'unix_confined_' : windowsWatch], ['core/lib/rest']);
  add('containment', ['test', '-p', core.name, '--test', 'target_protocol_security', '--locked', '--', '--test-threads=1'], ['core/test/target_protocol_security'], true);
  if (unix(platform)) {
    add('unix-transport', ['test', '-p', core.name, '--test', 'target_transport_unix', '--locked', '--', '--test-threads=1'], ['core/test/target_transport_unix'], true);
  }
  const ownedTests = integration.filter(t => t.name !== 'target_protocol_security' && (!unix(platform) || t.name !== 'target_transport_unix') && (platform !== 'win32' || t.name !== 'target_protocol_boundaries')).sort((a, b) => a.name.localeCompare(b.name));
  if (ownedTests.length) {
    add('core-integration', [...testArgs(core.name), ...ownedTests.flatMap(t => ['--test', t.name])], ownedTests.map(t => `core/test/${t.name}`));
  }
  const bins = core.targets.filter(t => t.kind[0] === 'bin' && t.test);
  if (bins.length) add('core-bins', [...testArgs(core.name), ...bins.flatMap(t => ['--bin', t.name])], bins.map(t => `core/bin/${t.name}`));
  add('core-doc', [...testArgs(core.name), '--doc'], ['core/doc']);
  const examples = core.targets.filter(t => t.kind[0] === 'example');
  const exampleTests = examples.filter(t => t.test);
  const exampleBuilds = examples.filter(t => !t.test);
  if (exampleTests.length) add('core-example-tests', [...testArgs(core.name), ...exampleTests.flatMap(t => ['--example', t.name])], exampleTests.map(t => `core/example/${t.name}`));
  if (exampleBuilds.length) add('core-examples', ['build', '--locked', '-p', core.name, ...exampleBuilds.flatMap(t => ['--example', t.name])], exampleBuilds.map(t => `core/example/${t.name}`));
  const targetKeys = groups.flatMap(g => g.targets);
  assert.equal(new Set(targetKeys).size, targetKeys.length, 'overlapping target ownership');
  assert.equal(groups.flatMap(g => g.targets).filter(t => t.startsWith('core/test/')).length, integration.length, 'uncovered core integration target');
  return groups;
}

// Both independent jobs use this partition. Qualification stays delegated to
// the core lane; process fixtures run sequentially on their own runner.
export function selectGroups(groups, lane = 'all') {
  assert(['all', 'core', 'process'].includes(lane), 'unknown Rust lane');
  if (lane === 'all') return groups;
  const processGroups = new Set(['cli', 'windows-watch', 'windows-boundaries']);
  const selected = groups.filter(g => processGroups.has(g.id) === (lane === 'process'));
  assert(selected.length > 0, `empty Rust lane: ${lane}`);
  return selected;
}

export function parseOptions(argv) {
  let mode, lane;
  for (const arg of argv) {
    if (['--plan', '--check-coverage'].includes(arg)) {
      assert(!mode, 'duplicate Rust mode');
      mode = arg;
    } else {
      assert(/^--lane=(all|core|process)$/.test(arg) && !lane, 'usage: ci-rust-tests.mjs [--plan|--check-coverage] [--lane=all|core|process]');
      lane = arg.slice('--lane='.length);
    }
  }
  assert(mode !== '--check-coverage' || !lane || lane === 'all', 'coverage must include all Rust lanes');
  return { mode, lane: lane ?? 'all' };
}

function invoke(args, capture = false) {
  return spawnSync('cargo', args, { cwd: root, windowsHide: true, ...(capture ? { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 } : { stdio: 'inherit' }) });
}

export function runGroups(groups, { run = invoke, report = value => console.log(JSON.stringify(value)) } = {}) {
  let exitCode = 0;
  for (const group of groups.filter(g => !g.delegated)) {
    const start = Date.now();
    report({ group: group.id, command: ['cargo', ...group.args] });
    let result;
    try { result = run(group.args); }
    catch (error) { result = { status: null, error }; }
    const status = result.error || result.signal || !Number.isInteger(result.status) ? 1 : result.status;
    if (status !== 0 && exitCode === 0) exitCode = status;
    report({ group: group.id, status, milliseconds: Date.now() - start, ...(result.error ? { error: String(result.error) } : {}) });
  }
  return exitCode;
}

export function listedCases(output) {
  return output.split(/\r?\n/).filter(line => /: (test|benchmark)$/.test(line)).sort();
}

export function checkCoverage(groups, run = invoke) {
  const list = args => {
    const result = run([...args, ...(args.includes('--') ? [] : ['--']), '--list'], true);
    assert(!result.error && !result.signal && result.status === 0, `test listing failed: cargo ${args.join(' ')}\n${result.stderr ?? result.error ?? ''}`);
    return listedCases(result.stdout ?? '');
  };
  const baseline = list(['test', '--workspace', '--locked', '--no-fail-fast']);
  assert(baseline.length > 0, 'empty workspace test inventory');
  const selected = groups.filter(g => g.args[0] === 'test').map(g => ({ group: g.id, cases: list(g.args) }));
  const qualification = selected.find(g => g.group === 'unix-confinement');
  if (qualification) assert(qualification.cases.length > 0, 'Unix qualification filter selects no tests');
  const watch = selected.find(g => g.group === 'windows-watch');
  if (watch) assert(watch.cases.length > 0, 'Windows watch filter selects no tests');
  const boundaries = selected.find(g => g.group === 'windows-boundaries');
  if (boundaries) assert(boundaries.cases.length > 0, 'Windows boundary filter selects no tests');
  const combined = selected.flatMap(g => g.cases).sort();
  assert.deepEqual(combined, baseline, 'test inventory differs: missing or repeated cases');
  return { cases: baseline.length, groups: selected.map(g => ({ group: g.group, cases: g.cases.length })) };
}

export function main(argv = process.argv.slice(2)) {
  const { mode, lane } = parseOptions(argv);
  const result = invoke(['metadata', '--locked', '--no-deps', '--format-version', '1'], true);
  assert(!result.error && result.status === 0, `cargo metadata failed: ${result.stderr ?? result.error ?? ''}`);
  const groups = selectGroups(buildGroups(JSON.parse(result.stdout)), lane);
  if (mode === '--plan') {
    console.log(JSON.stringify({ platform: process.platform, lane, groups }, null, 2));
    return 0;
  }
  if (mode === '--check-coverage') {
    console.log(JSON.stringify({ ok: true, ...checkCoverage(groups) }));
    return 0;
  }
  return runGroups(groups);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { process.exitCode = main(); }
  catch (error) { console.error(String(error)); process.exitCode = 1; }
}
