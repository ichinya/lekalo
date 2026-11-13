import assert from 'node:assert/strict';
import test from 'node:test';
import { buildGroups, checkCoverage, listedCases, runGroups } from './ci-rust-tests.mjs';

const target = (name, kind, options = {}) => ({ name, kind: [kind], test: kind !== 'example', doctest: kind === 'lib', ...options });
const metadata = () => ({
  workspace_members: ['core', 'cli'],
  packages: [
    { id: 'core', name: 'lekalo-core', targets: [target('lekalo_core', 'lib'), target('format', 'test'), target('target_protocol_security', 'test'), target('target_transport_unix', 'test'), target('generate', 'example')] },
    { id: 'cli', name: 'lekalo-cli', targets: [target('lekalo', 'bin'), target('adapter-sdk-oracle', 'bin'), target('format', 'test')] },
  ],
});

test('new integration targets are included once, without a maintained list', () => {
  const input = metadata();
  input.packages[0].targets.push(target('new_regression', 'test'));
  const groups = buildGroups(input, 'linux');
  const integration = groups.find(g => g.id === 'core-integration');
  assert(integration.args.includes('new_regression'));
  const keys = groups.flatMap(g => g.targets);
  assert.equal(keys.filter(k => k === 'core/test/new_regression').length, 1);
  assert.equal(new Set(keys).size, keys.length);
});

test('Unix qualifications retain serial invocations; Windows owns its empty Unix target', () => {
  for (const platform of ['linux', 'darwin']) {
    const groups = buildGroups(metadata(), platform);
    assert.deepEqual(groups.filter(g => g.delegated).map(g => g.id), ['unix-confinement', 'containment', 'unix-transport']);
    assert(groups.find(g => g.id === 'core-lib').args.includes('--skip'));
    for (const id of ['containment', 'unix-transport']) assert(groups.find(g => g.id === id).args.includes('--test-threads=1'));
  }
  const groups = buildGroups(metadata(), 'win32');
  assert.deepEqual(groups.filter(g => g.delegated).map(g => g.id), ['containment']);
  assert(groups.find(g => g.id === 'core-lib').args.includes('--skip'));
  assert(groups.find(g => g.id === 'core-integration').args.includes('target_transport_unix'));
});

test('doctests and compile-only examples stay distinct from executable example tests', () => {
  const input = metadata();
  input.packages[0].targets.push(target('tested_example', 'example', { test: true }));
  const groups = buildGroups(input, 'linux');
  assert(groups.find(g => g.id === 'core-doc').args.includes('--doc'));
  assert.equal(groups.find(g => g.id === 'core-examples').args[0], 'build');
  assert(!groups.find(g => g.id === 'core-examples').args.includes('tested_example'));
  assert(groups.find(g => g.id === 'core-example-tests').args.includes('tested_example'));
});

test('unknown members, unsupported kinds, duplicates and conditional targets fail closed', () => {
  const member = metadata();
  member.workspace_members.push('extra');
  member.packages.push({ id: 'extra', name: 'extra', targets: [] });
  assert.throws(() => buildGroups(member), /unclassified workspace members/);
  for (const extra of [target('bench', 'bench'), target('format', 'test'), target('feature_only', 'test', { 'required-features': ['extra'] })]) {
    const input = metadata();
    input.packages[0].targets.push(extra);
    assert.throws(() => buildGroups(input), /unclassified|duplicate|conditional/);
  }
  assert.throws(() => buildGroups(metadata(), 'freebsd'), /unsupported CI platform/);
});

test('missing qualification targets and disabled core tests do not silently drop coverage', () => {
  const missing = metadata();
  missing.packages[0].targets = missing.packages[0].targets.filter(t => t.name !== 'target_protocol_security');
  assert.throws(() => buildGroups(missing), /missing qualification target/);
  const disabled = metadata();
  disabled.packages[0].targets[0].doctest = false;
  assert.throws(() => buildGroups(disabled), /documentation tests must remain enabled/);
});

test('a failed group does not suppress later groups or become success', () => {
  const groups = buildGroups(metadata(), 'linux');
  const called = [];
  const result = runGroups(groups, { run(args) { called.push(args); return { status: called.length === 1 ? 17 : 0 }; }, report() {} });
  assert.equal(result, 17);
  assert.equal(called.length, groups.filter(g => !g.delegated).length);
});

test('spawn errors and signals produce a failure while later groups still execute', () => {
  for (const failure of [{ status: null, error: Error('cargo missing') }, { status: null, signal: 'SIGTERM' }]) {
    let called = 0;
    const result = runGroups(buildGroups(metadata(), 'win32'), { run() { return ++called === 1 ? failure : { status: 0 }; }, report() {} });
    assert.equal(result, 1);
    assert(called > 1);
  }
  let called = 0;
  assert.equal(runGroups(buildGroups(metadata(), 'win32'), {
    run() { if (++called === 1) throw Error('spawn threw'); return { status: 0 }; }, report() {},
  }), 1);
  assert(called > 1);
});

const casesFor = args => {
  const core = ['core_case', 'unix_confined_case', 'watch::targets::tests::startup_case'];
  if (args.includes('--workspace')) return ['cli_case', ...core, 'format_case', 'security_case', 'transport_case', 'doc_case'];
  if (args.includes('lekalo-cli')) return ['cli_case'];
  if (args.includes('--doc')) return ['doc_case'];
  if (args.includes('--lib')) {
    const separator = args.indexOf('--');
    const filter = args.slice(0, separator < 0 ? args.length : separator).find(a => ['unix_confined_', 'watch::targets::tests::'].includes(a));
    const skipped = args.flatMap((a, i) => a === '--skip' ? [args[i + 1]] : []);
    return core.filter(name => (!filter || name.includes(filter)) && !skipped.some(s => name.includes(s)));
  }
  return [['format', 'format_case'], ['target_protocol_security', 'security_case'], ['target_transport_unix', 'transport_case']].filter(([target]) => args.includes(target)).map(([, name]) => name);
};
const listing = transform => args => ({ status: 0, stdout: transform(casesFor(args), args).map(name => `${name}: test`).join('\n'), stderr: '' });

test('coverage listing compares the complete multiset including delegated suites', () => {
  const result = checkCoverage(buildGroups(metadata(), 'linux'), listing(names => names));
  assert.equal(result.cases, 8);
  assert.equal(result.groups.length, 7);
  assert.deepEqual(listedCases('Running tests\r\nfixture: test\r\n0 tests, 0 benchmarks\r\n'), ['fixture: test']);
});

test('coverage listing detects omissions, duplicate executions and empty qualification', () => {
  for (const transform of [
    (names, args) => args.includes('--skip') ? [] : names,
    (names, args) => args.includes('--skip') ? [...names, ...names] : names,
    (names, args) => args.includes('--lib') && !args.includes('--skip') ? [] : names,
  ]) assert.throws(() => checkCoverage(buildGroups(metadata(), 'linux'), listing(transform)), /inventory differs|selects no tests/);
});

test('a failed listing is not accepted as an empty successful suite', () => {
  assert.throws(() => checkCoverage(buildGroups(metadata(), 'linux'), () => ({ status: 101, stderr: 'compile failed' })), /test listing failed/);
});

test('Windows watch fixtures have bounded concurrency and complete nonoverlapping library coverage', () => {
  const groups = buildGroups(metadata(), 'win32');
  const watch = groups.find(g => g.id === 'windows-watch');
  assert(watch.args.includes('--test-threads=2'));
  assert(!watch.delegated);
  assert.equal(groups.find(g => g.id === 'core-lib').args.at(-1), 'watch::targets::tests::');
  assert.equal(checkCoverage(groups, listing(names => names)).cases, 8);
  for (const transform of [
    (names, args) => args.includes('watch::targets::tests::') && !args.includes('--skip') ? [] : names,
    (names, args) => args.includes('--skip') ? [...names, 'watch::targets::tests::startup_case'] : names,
  ]) assert.throws(() => checkCoverage(groups, listing(transform)), /inventory differs|Windows watch filter selects no tests/);
});
