#!/usr/bin/env node
// Regenerate the issue #113 module-api goldens after a digest-domain
// change. Runs the freshly built CLI against the fixture projects and
// rewrites the tracked goldens in both fixture trees (the workspace tree
// of the Node gate and the crate tree of the core integration tests).
import { spawnSync, execFileSync } from 'node:child_process';
import { writeFileSync, readFileSync, mkdirSync, copyFileSync, cpSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const bin = join(root, 'target/debug', process.platform === 'win32' ? 'lekalo.exe' : 'lekalo');
const fixture = 'tests/fixtures/module-api';
const run = (args) => {
  const result = spawnSync(bin, ['--json', '--no-cache', ...args], { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  if (result.status !== 0) throw new Error(`command failed: ${args.join(' ')}\n${result.stdout}\n${result.stderr}`);
  return JSON.parse(result.stdout);
};
const write = (path, args, key) => {
  const payload = run(args);
  const document = payload[key];
  if (!document) throw new Error(`missing payload ${key}`);
  writeFileSync(join(root, path), JSON.stringify(document, null, 2) + '\n');
  return document;
};

// 1. reference project: bundle, consumers, report, contexts.
write(`${fixture}/standalone/golden-bundle.json`, ['module', 'api', 'export', 'tasks', '--project', `${fixture}/reference/tasks`, '--api', 'module-api.json'], 'modulePublicContract');
write(`${fixture}/standalone/golden-consumers.json`, ['module', 'api', 'consumers', 'tasks', '--project', `${fixture}/reference/tasks`, '--api', 'module-api.json'], 'moduleApiConsumers');
write(`${fixture}/valid/golden-report.json`, ['module', 'api', 'check', '--project', `${fixture}/reference/tasks`, '--api', 'module-api.json'], 'moduleApiReport');
write(`${fixture}/standalone/golden-context.json`, ['module', 'api', 'context', `${fixture}/standalone/golden-bundle.json`, '--consumer', 'planner', '--uses', 'tasks.task_view,tasks.get_task', '--budget', '4000'], 'moduleConsumerContext');
write(`${fixture}/standalone/golden-context-tiny.json`, ['module', 'api', 'context', `${fixture}/standalone/golden-bundle.json`, '--consumer', 'planner', '--uses', 'tasks.task_view,tasks.get_task', '--budget', '16'], 'moduleConsumerContext');

// 2. diff pair projects: bundles, snapshots, diff receipt.
write(`${fixture}/diff/golden-bundle-before.json`, ['module', 'api', 'export', 'tasks', '--project', `${fixture}/diff/before`, '--api', 'module-api.json'], 'modulePublicContract');
write(`${fixture}/diff/golden-bundle-after.json`, ['module', 'api', 'export', 'tasks', '--project', `${fixture}/diff/after`, '--api', 'module-api.json'], 'modulePublicContract');
write(`${fixture}/diff/golden-consumers-before.json`, ['module', 'api', 'consumers', 'tasks', '--project', `${fixture}/diff/before`, '--api', 'module-api.json'], 'moduleApiConsumers');
write(`${fixture}/diff/golden-consumers-after.json`, ['module', 'api', 'consumers', 'tasks', '--project', `${fixture}/diff/after`, '--api', 'module-api.json'], 'moduleApiConsumers');
const diff = run(['module', 'api', 'diff', `${fixture}/diff/golden-bundle-before.json`, `${fixture}/diff/golden-bundle-after.json`, '--consumers-before', `${fixture}/diff/golden-consumers-before.json`, '--consumers-after', `${fixture}/diff/golden-consumers-after.json`]);
writeFileSync(join(root, `${fixture}/diff/golden-diff.json`), JSON.stringify({ status: 'valid', moduleApiDiff: diff.moduleApiDiff }, null, 2) + '\n');

// 3. lock fixture.
write(`${fixture}/lock/golden-lock.json`, ['module', 'api', 'lock', '--project', `${fixture}/lock`, '--api', 'module-api.json', '--module', 'tasks', '--consumer', 'planner'], 'moduleApiLock');

// 4. populated public dependency: shared scalar -> nested Task view ->
//    planner lock and standalone context. Foreign shapes remain explicit
//    gaps; their exact public bundle pins must survive every projection.
const dependencies = `${fixture}/public-dependencies`;
write(`${dependencies}/golden-bundle-shared.json`, ['module', 'api', 'export', 'shared', '--project', dependencies, '--api', 'module-api.json'], 'modulePublicContract');
write(`${dependencies}/golden-bundle.json`, ['module', 'api', 'export', 'tasks', '--project', dependencies, '--api', 'module-api.json'], 'modulePublicContract');
write(`${dependencies}/golden-lock.json`, ['module', 'api', 'lock', '--project', dependencies, '--api', 'module-api.json', '--module', 'tasks', '--consumer', 'planner'], 'moduleApiLock');
write(`${dependencies}/golden-context.json`, ['module', 'api', 'context', `${dependencies}/golden-bundle.json`, '--consumer', 'planner', '--uses', 'tasks.task_envelope', '--budget', '4000'], 'moduleConsumerContext');

// 5. the core integration-test fixture tree keeps its own tracked copy
//    of the standalone/diff goldens and the populated dependency project.
cpSync(join(root, dependencies), join(root, 'crates/lekalo-core/tests/fixtures/module-api/public-dependencies'), { recursive: true });
mkdirSync(join(root, 'crates/lekalo-core/tests/fixtures/module-api/standalone'), { recursive: true });
mkdirSync(join(root, 'crates/lekalo-core/tests/fixtures/module-api/diff'), { recursive: true });
for (const name of ['golden-bundle.json', 'golden-consumers.json', 'golden-context.json', 'golden-context-tiny.json']) {
  copyFileSync(join(root, `${fixture}/standalone/${name}`), join(root, `crates/lekalo-core/tests/fixtures/module-api/standalone/${name}`));
}
for (const name of ['golden-bundle-before.json', 'golden-bundle-after.json', 'golden-consumers-before.json', 'golden-consumers-after.json', 'golden-diff.json']) {
  copyFileSync(join(root, `${fixture}/diff/${name}`), join(root, `crates/lekalo-core/tests/fixtures/module-api/diff/${name}`));
}

console.log(JSON.stringify({ ok: true, regenerated: 'module-api goldens' }));
