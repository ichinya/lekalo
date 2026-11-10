#!/usr/bin/env node
// Issue #109: real HTTP + PostgreSQL assertions, never equality of unsupported rows.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { resolve, join, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (path) => readFileSync(join(root, path), 'utf8');
const corpus = JSON.parse(read('examples/planner-shared/scenarios.json'));
const sha = (bytes) => `sha256:${createHash('sha256').update(bytes).digest('hex')}`;
const files = (home) => readdirSync(home, { withFileTypes: true }).flatMap((entry) => {
  if (['node_modules', 'vendor', '.gitignore'].includes(entry.name)) return [];
  const path = join(home, entry.name); return entry.isDirectory() ? files(path) : [path];
});
const manifest = JSON.parse(read('examples/reference-projects.lock.json'));
const matrix = JSON.parse(read('examples/planner-shared/reference-matrix.json'));
assert.equal(manifest.formatVersion, 1);
assert.equal(manifest.revision, corpus.revision);
assert.equal(matrix.revision, corpus.revision);
assert.deepEqual(Object.keys(matrix.nativeCoverage).map((id) => id.replace('planner.', '')).sort(), [...matrix.implementedHttpOperations].sort());
assert.ok(Object.values(matrix.nativeCoverage).every((name) => corpus.cases.includes(name)), 'every native coverage reference names a real shared case');
const definitions = ['module', 'entities', 'commands', 'queries', 'bindings'].flatMap((file) => JSON.parse(read(`examples/planner-shared/lekalo/modules/planner/${file}.yaml`)).definitions);
assert.deepEqual(definitions.filter((d) => ['command', 'query'].includes(d.kind)).map((d) => d.id.replace('planner.', '')).sort(), [...matrix.implementedHttpOperations].sort(), 'the canonical model and reference profile name the same operation surface');
for (const [path, digest] of Object.entries(manifest.files)) assert.equal(sha(readFileSync(join(root, path))), digest, `reference source drift: ${path}`);
for (const home of ['planner-shared', 'planner-node', 'planner-laravel']) for (const path of files(join(root, 'examples', home))) {
  assert.ok(Object.hasOwn(manifest.files, relative(root, path).replaceAll('\\', '/')), `unlocked reference file: ${path}`);
}
for (const [path, digest] of Object.entries(manifest.adapters)) assert.equal(sha(readFileSync(join(root, path))), digest, `adapter lock drift: ${path}`);
assert.equal(new Set(corpus.cases).size, corpus.cases.length);
if (process.argv.slice(2).join(' ') === '--static') {
  console.log(JSON.stringify({ ok: true, gate: 'reference-projects-static', files: Object.keys(manifest.files).length, cases: corpus.cases.length }));
  process.exit(0);
}
const extracted = process.argv.slice(2).join(' ') === '--extraction';
assert.ok(process.argv.length === 2 || extracted, 'usage: node scripts/test-reference-projects.mjs [--static|--extraction]');
const { default: pg } = await import('../examples/planner-node/node_modules/pg/lib/index.js');
const databaseUrl = process.env.REFERENCE_DATABASE_URL ?? 'postgres://postgres@127.0.0.1:56329/lekalo_reference';
const database = new URL(databaseUrl);
assert.ok(['127.0.0.1', 'localhost', '[::1]'].includes(database.hostname) && database.pathname === '/lekalo_reference', 'gate admits only the dedicated loopback lekalo_reference database');
for (const url of [process.env.REFERENCE_NODE_URL, process.env.REFERENCE_LARAVEL_URL, process.env.REFERENCE_LARAVEL_PEER_URL, process.env.REFERENCE_READER_URL].filter(Boolean)) {
  assert.ok(['127.0.0.1', 'localhost', '[::1]'].includes(new URL(url).hostname), 'gate admits only loopback HTTP applications');
}
const pool = new pg.Pool({ connectionString: databaseUrl, max: 4 });
const task = (n) => `00000000-0000-4000-8000-${String(n).padStart(12, '0')}`;
const day = '2026-01-02';
let base; let peer; let serial = 0;
const request = async (path, { method = 'GET', body, key, headers = {}, status = 200, baseUrl = base } = {}) => {
  const response = await fetch(`${baseUrl}${path}`, { method, headers: { 'X-Actor': 'user-a', 'X-Tenant': 'workspace-a',
    'Content-Type': 'application/json', ...(method !== 'GET' ? { 'Idempotency-Key': key ?? `request-${++serial}` } : {}), ...headers },
    ...(body === undefined ? {} : { body: typeof body === 'string' ? body : JSON.stringify(body) }), signal: AbortSignal.timeout(15000) });
  const json = await response.json(); assert.equal(response.status, status, `${method} ${path}: ${JSON.stringify(json)}`); return json;
};
const plan = (n, date = day, options = {}) => request('/planning', { method: 'POST', body: { task_id: task(n), planned_for: date }, ...options });
const list = () => request('/planning/today');
const focus = (n, options = {}) => request(`/tasks/${task(n)}/focus`, { method: 'POST', ...options });
const reset = () => pool.query('TRUNCATE reference_planning,reference_receipts');
const wait = (ms) => new Promise((accept) => setTimeout(accept, ms));
async function ready(url) {
  for (let i = 0; i < 100; i++) {
    try { if ((await fetch(`${url}/health`, { signal: AbortSignal.timeout(1000) })).ok) return; } catch {}
    await wait(100);
  }
  throw new Error(`reference server unavailable: ${url}`);
}
async function concurrent(actions) {
  const blocker = await pool.connect(); let pending;
  try {
    await blocker.query('BEGIN');
    await blocker.query("SELECT 1 FROM reference_actors WHERE tenant='workspace-a' AND user_id='user-a' FOR UPDATE");
    pending = Promise.all(actions.map((action, index) => action(index === 1 && peer ? peer : base)));
    // Attach a handler immediately; the assertion below reports early failures.
    pending.catch(() => {});
    let overlap = false;
    for (let i = 0; i < 100; i++) {
      const result = await pool.query("SELECT count(*)::int n FROM pg_stat_activity WHERE application_name IN ('reference-node','reference-laravel') AND wait_event_type='Lock'");
      if (result.rows[0].n >= 2) { overlap = true; break; }
      await wait(25);
    }
    assert.ok(overlap, 'two independent HTTP transactions must overlap at the PostgreSQL lock barrier');
  } finally { await blocker.query('ROLLBACK'); blocker.release(); }
  return pending;
}
const cases = {
  'plan-and-unplan': async () => { assert.equal((await request('/planning/backlog')).items.length, 3); await plan(1); assert.deepEqual((await request('/planning/backlog')).items.map((x) => x.task_id), [task(2), task(3)]); const row = (await list()).items[0]; assert.equal(row.task_id, task(1)); assert.equal(row.planning_id, 'f16aeb12-d13b-57d4-9ca5-5f17e6d8b90e', 'independent Python uuid.uuid5 DNS vector'); await request(`/planning/${row.planning_id}`, { method: 'DELETE' }); assert.equal((await list()).items.length, 0); assert.equal((await request('/planning/backlog')).items.length, 3); },
  'focus-and-pause': async () => { await plan(1); await plan(2); await focus(1); const row = (await list()).items[0]; assert.equal(row.focused, true); await request(`/planning/${row.planning_id}/pause`, { method: 'POST' }); const paused = (await list()).items[0]; assert.equal(paused.focused, false); assert.equal(paused.paused_at, corpus.clock); await focus(2); assert.deepEqual((await list()).items.filter((x) => x.focused).map((x) => x.task_id), [task(2)]); },
  'today-and-carry-over': async () => { await plan(1); await plan(2, '2026-01-01'); assert.deepEqual((await list()).items.map((x) => x.task_id), [task(1)]); const old = (await request('/planning/carry-over')).items; assert.equal(old[0].planned_for, '2026-01-01'); assert.equal(old[0].task_id, task(2)); },
  'timezone-boundary': async () => { await plan(1); await plan(2, '2026-01-01'); const west = await request('/planning/today', { headers: { 'X-Timezone': 'America/Los_Angeles' } }); assert.deepEqual(west.items.map((x) => x.task_id), [task(2)]); },
  'tenant-and-owner-isolation': async () => { await plan(1); const row = (await list()).items[0]; for (const headers of [{ 'X-Actor': 'user-b' }, { 'X-Tenant': 'workspace-b' }]) { assert.equal((await request('/planning/today', { headers })).items.length, 0); await request(`/planning/${row.planning_id}`, { method: 'DELETE', headers, status: 404 }); await plan(1, day, { headers, status: 404 }); } assert.equal((await list()).items.length, 1); },
  authorization: async () => { assert.equal((await request('/planning/today', { headers: { 'X-Actor': '' }, status: 401 })).error, 'actor_required'); await request('/planning/today', { headers: { 'X-Actor': 'unknown' }, status: 403 }); },
  'idempotent-replay': async () => { const a = await plan(1, day, { key: 'replay' }); const b = await plan(1, day, { key: 'replay' }); assert.deepEqual(a, b); assert.equal((await list()).items.length, 1); },
  'idempotency-conflict': async () => { await plan(1, day, { key: 'conflict' }); assert.equal((await plan(2, day, { key: 'conflict', status: 409 })).error, 'idempotency_conflict'); assert.equal((await list()).items.length, 1); },
  'duplicate-planning': async () => { await plan(1); assert.equal((await plan(1, day, { status: 409 })).error, 'planning_conflict'); },
  'reorder-and-stale-version': async () => { await plan(1); await plan(2); const ids = (await list()).items.map((x) => x.planning_id).reverse(); const body = { planned_for: day, expected_version: 1, order: ids.join(',') }; await request('/planning/reorder', { method: 'POST', key: 'numeric-order', body: JSON.stringify(body).replace('"expected_version":1', '"expected_version":1.0') }); await request('/planning/reorder', { method: 'POST', key: 'numeric-order', body }); assert.deepEqual((await list()).items.map((x) => x.planning_id), ids); assert.equal((await request('/planning/reorder', { method: 'POST', body, status: 409 })).error, 'reorder_stale'); assert.ok((await list()).items.every((x) => x.version === 2)); },
  'reorder-must-be-a-permutation': async () => { await plan(1); await plan(2); const id = (await list()).items[0].planning_id; for (const order of [id, `${id},${id}`]) await request('/planning/reorder', { method: 'POST', body: { planned_for: day, expected_version: 1, order }, status: 422 }); assert.ok((await list()).items.every((x) => x.version === 1)); },
  'invalid-input': async () => { for (const body of [null, [], '{', '{"planned_for":1e400,"task_id":"00000000-0000-4000-8000-000000000001"}', { task_id: task(1), planned_for: '2026-02-30' }, { task_id: task(1), planned_for: day, extra: true }]) await request('/planning', { method: 'POST', body, status: 422 }); await request('/planning/today', { headers: { 'X-Timezone': 'not/a/zone' }, status: 422 }); await plan(1, day, { headers: { 'Idempotency-Key': '' }, status: 422 }); },
  'rollback-and-retry': async () => { assert.equal((await plan(1, day, { key: 'rollback', headers: { 'X-Reference-Fault': 'rollback' }, status: 503 })).error, 'store_unavailable'); assert.equal((await list()).items.length, 0); await plan(1, day, { key: 'rollback' }); assert.equal((await list()).items.length, 1); },
  'concurrent-focus': async () => { await plan(1); await plan(2); const calls = [1, 2].map((n) => async (url) => { const r = await fetch(`${url}/tasks/${task(n)}/focus`, { method: 'POST', headers: { 'X-Actor': 'user-a', 'X-Tenant': 'workspace-a', 'Idempotency-Key': `race-${n}` }, signal: AbortSignal.timeout(15000) }); const body = await r.json(); return { status: r.status, body }; }); const rows = await concurrent(calls); assert.deepEqual(rows.map((r) => r.status).sort(), [200, 409]); assert.equal(rows.find((r) => r.status === 409).body.error, 'focus_conflict'); assert.equal((await list()).items.filter((r) => r.focused).length, 1); await assert.rejects(pool.query('UPDATE reference_planning SET focused=true'), (error) => error.code === '23505'); },
  'concurrent-idempotency': async () => { await concurrent([(url) => plan(1, day, { key: 'parallel-replay', baseUrl: url }), (url) => plan(1, day, { key: 'parallel-replay', baseUrl: url })]); assert.equal((await list()).items.length, 1); },
};
assert.deepEqual(Object.keys(cases).sort(), [...corpus.cases].sort(), 'every shared scenario has an implementation');
const results = [];
try {
  const targets = [['node', process.env.REFERENCE_NODE_URL ?? 'http://127.0.0.1:56330'], ...(!extracted ? [['laravel', process.env.REFERENCE_LARAVEL_URL ?? 'http://127.0.0.1:56331']] : [])];
  for (const [target, url] of targets) {
    base = url; peer = target === 'laravel' ? (process.env.REFERENCE_LARAVEL_PEER_URL ?? 'http://127.0.0.1:56334') : undefined; await ready(base); if (peer) await ready(peer);
    for (const name of corpus.cases) { await reset(); await cases[name](); const testId = `reference.${target}.${name}`; results.push({ target, scenario: name, testId, outcome: 'pass' }); console.log(`ok - ${testId}`); }
  }
  if (extracted) {
    const reader = process.env.REFERENCE_READER_URL ?? 'http://127.0.0.1:56333';
    await ready(reader);
    const response = await fetch(`${reader}/planning`, { method: 'POST', headers: { 'X-Actor': 'user-a', 'X-Tenant': 'workspace-a' } });
    assert.equal(response.status, 403);
    const connection = new URL(pool.options.connectionString); connection.username = 'reference_reader';
    const readOnly = new pg.Pool({ connectionString: connection.toString() });
    try { await assert.rejects(readOnly.query('DELETE FROM reference_planning'), (error) => error.code === '42501'); } finally { await readOnly.end(); }
    console.log('ok - extracted module refuses writes at HTTP and database boundaries');
  }
  // A genuine changed application runs over HTTP against the same database.
  // Its successful focus response without a focused row must fail the corpus.
  const home = mkdtempSync(join(root, 'examples/planner-node/node_modules/.reference-mutant-'));
  let child;
  try {
    const source = read('examples/planner-node/src/server.mjs');
    assert.ok(source.includes('SET focused=true,paused_at=NULL'));
    writeFileSync(join(home, 'server.mjs'), source.replace('SET focused=true,paused_at=NULL', 'SET focused=false,paused_at=NULL'));
    const port = process.env.REFERENCE_MUTANT_PORT ?? '56332';
    child = spawn(process.execPath, [join(home, 'server.mjs')], { env: { ...process.env, DATABASE_URL: pool.options.connectionString, PORT: port, REFERENCE_NOW: corpus.clock }, stdio: 'ignore', windowsHide: true });
    base = `http://127.0.0.1:${port}`; await ready(base); await reset();
    await assert.rejects(cases['focus-and-pause'], /Expected values to be strictly equal/);
    console.log('ok - deliberate application behavior drift detected');
  } finally { if (child && child.exitCode === null) { const exited = new Promise((accept) => child.once('exit', accept)); child.kill(); await exited; } rmSync(home, { recursive: true, force: true }); }
  console.log(JSON.stringify({ ok: true, gate: 'reference-projects-http-postgres', revision: corpus.revision, inputDigest: sha(read('examples/reference-projects.lock.json')), scenarios: corpus.cases.length, targets: targets.length, assertions: results.length, testIds: results.map((r) => r.testId), concurrentTransactionsProven: true, negativeControl: 'real-mutant-rejected', extracted }));
} finally { await pool.end(); }
