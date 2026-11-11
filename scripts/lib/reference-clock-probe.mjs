// Real HTTP regression for the default Node clock, without waiting for midnight
// or introducing a production clock-control endpoint/environment variable.
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createServer } from 'node:net';
import { spawn } from 'node:child_process';

export async function runReferenceClockProbe(root, databaseUrl) {
  const home = mkdtempSync(join(root, 'examples/planner-node/node_modules/.reference-clock-'));
  let child;
  try {
    const clock = join(home, 'instant.txt');
    const loader = join(home, 'clock.mjs');
    const before = '2026-01-01T23:59:00Z'; const after = '2026-01-02T00:01:00Z';
    writeFileSync(clock, before);
    writeFileSync(loader, `import { readFileSync } from 'node:fs';
const NativeDate = globalThis.Date;
const instant = () => readFileSync(process.env.REFERENCE_CLOCK_PROBE_FILE, 'utf8');
globalThis.Date = class extends NativeDate {
  constructor(...args) { super(...(args.length ? args : [instant()])); }
  static now() { return new NativeDate(instant()).getTime(); }
};\n`);
    const socket = createServer();
    await new Promise((accept, reject) => { socket.once('error', reject); socket.listen(0, '127.0.0.1', accept); });
    const port = socket.address().port;
    await new Promise((accept) => socket.close(accept));
    const env = { ...process.env, HOST: '127.0.0.1', PORT: String(port), DATABASE_URL: databaseUrl,
      REFERENCE_CLOCK_PROBE_FILE: clock, REFERENCE_QUERY_SERVICE_URL: '', REFERENCE_READ_ONLY: '0' };
    delete env.REFERENCE_NOW;
    child = spawn(process.execPath, ['--import', pathToFileURL(loader).href, join(root, 'examples/planner-node/src/server.mjs')], { env, stdio: 'ignore', windowsHide: true });
    const url = `http://127.0.0.1:${port}`;
    let ready = false;
    for (let i = 0; i < 100; i++) {
      try { const response = await fetch(`${url}/health`, { signal: AbortSignal.timeout(1000) }); await response.text(); if (response.ok) { ready = true; break; } } catch {}
      await new Promise((accept) => setTimeout(accept, 100));
    }
    assert.ok(ready, 'clock probe application unavailable');
    let serial = 0;
    const call = async (path, method = 'GET', body) => {
      const response = await fetch(`${url}${path}`, { method, headers: { 'X-Actor': 'user-a', 'X-Tenant': 'workspace-a',
        'Content-Type': 'application/json', 'Idempotency-Key': `clock-${++serial}` },
        ...(body ? { body: JSON.stringify(body) } : {}), signal: AbortSignal.timeout(15000) });
      const json = await response.json(); assert.equal(response.status, 200, JSON.stringify(json)); return json;
    };
    const task = (n) => `00000000-0000-4000-8000-${String(n).padStart(12, '0')}`;
    await call('/planning', 'POST', { task_id: task(1), planned_for: '2026-01-01' });
    await call('/planning', 'POST', { task_id: task(2), planned_for: '2026-01-02' });
    const first = (await call('/planning/today')).items;
    assert.deepEqual(first.map((row) => row.task_id), [task(1)]);
    await call(`/tasks/${task(1)}/focus`, 'POST');
    await call(`/planning/${first[0].planning_id}/pause`, 'POST');
    assert.equal((await call('/planning/today')).items[0].paused_at, new Date(before).toISOString());
    writeFileSync(clock, after);
    const next = (await call('/planning/today')).items;
    assert.deepEqual(next.map((row) => row.task_id), [task(2)], 'Today must advance after local midnight without restarting Node');
    assert.deepEqual((await call('/planning/carry-over')).items.map((row) => row.task_id), [task(1)]);
    await call(`/tasks/${task(2)}/focus`, 'POST');
    await call(`/planning/${next[0].planning_id}/pause`, 'POST');
    assert.equal((await call('/planning/today')).items[0].paused_at, new Date(after).toISOString(), 'pause must use the current request clock');
    console.log('ok - reference.node.default-clock-rollover');
  } finally {
    if (child && child.exitCode === null) { const exited = new Promise((accept) => child.once('exit', accept)); child.kill(); await exited; }
    rmSync(home, { recursive: true, force: true });
  }
}
