import { serve } from '@hono/node-server';
import { Hono } from 'hono';
import { bodyLimit } from 'hono/body-limit';
import pg from 'pg';
import { drizzle } from 'drizzle-orm/node-postgres';
import { sql } from 'drizzle-orm';
import { createHash } from 'node:crypto';

const pool = new pg.Pool({ connectionString: process.env.DATABASE_URL, application_name: 'reference-node', max: 8 });
const db = drizzle(pool);
const app = new Hono();
const now = process.env.REFERENCE_NOW ?? new Date().toISOString();
const canonical = (value) => JSON.stringify(value && !Array.isArray(value) && typeof value === 'object'
  ? Object.fromEntries(Object.keys(value).sort().map((key) => [key, value[key]])) : value);
class Refusal extends Error { constructor(status, code) { super(code); this.status = status; } }
const refuse = (status, code) => { throw new Refusal(status, code); };
const id = (value) => typeof value === 'string' && /^[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$/.test(value);
const date = (value) => typeof value === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(value)
  && !value.startsWith('0000-')
  && !Number.isNaN(Date.parse(value)) && new Date(value).toISOString().slice(0, 10) === value;
const columns = sql.raw('planning_id, task_id, planned_for::text, position, version, focused, paused_at');
app.use('*', bodyLimit({ maxSize: 16384, onError: (c) => c.json({ error: 'body_too_large' }, 413) }));
app.get('/health', async (c) => { await db.execute(sql`SELECT 1`); return c.json({ ok: true }); });
app.all('*', async (c) => {
  const actor = c.req.header('X-Actor'); const tenant = c.req.header('X-Tenant');
  if (!actor || !tenant) refuse(401, 'actor_required');
  const identity = await db.execute(sql`SELECT 1 FROM reference_actors WHERE tenant=${tenant} AND user_id=${actor}`);
  if (!identity.rows.length) refuse(403, 'actor_denied');
  const zone = c.req.header('X-Timezone') ?? 'UTC';
  let today;
  try { today = new Intl.DateTimeFormat('en-CA', { timeZone: zone, year: 'numeric', month: '2-digit', day: '2-digit' }).format(new Date(now)); }
  catch { refuse(422, 'invalid_timezone'); }
  const path = new URL(c.req.url).pathname; const method = c.req.method;
  if (method === 'GET') {
    if (process.env.REFERENCE_QUERY_SERVICE_URL) {
      const response = await fetch(`${process.env.REFERENCE_QUERY_SERVICE_URL}${path}`, {
        headers: { 'X-Actor': actor, 'X-Tenant': tenant, 'X-Timezone': zone }, signal: AbortSignal.timeout(5000),
      });
      return c.json(await response.json(), response.status);
    }
    if (path === '/planning/backlog') {
      const result = await db.execute(sql`SELECT task_id,title FROM reference_tasks t WHERE tenant=${tenant} AND user_id=${actor}
        AND NOT EXISTS (SELECT 1 FROM reference_planning p WHERE p.tenant=t.tenant AND p.user_id=t.user_id AND p.task_id=t.task_id) ORDER BY task_id`);
      return c.json({ items: result.rows });
    }
    if (!['/planning/today', '/planning/carry-over'].includes(path)) refuse(404, 'not_found');
    const condition = path.endsWith('/today') ? sql`planned_for=${today}::date` : sql`planned_for<${today}::date`;
    const result = await db.execute(sql`SELECT ${columns} FROM reference_planning WHERE tenant=${tenant} AND user_id=${actor} AND ${condition} ORDER BY planned_for,position,planning_id`);
    return c.json({ items: result.rows });
  }
  if (process.env.REFERENCE_READ_ONLY === '1') refuse(403, 'read_only');
  if (!['POST', 'DELETE'].includes(method)) refuse(404, 'not_found');
  const key = c.req.header('Idempotency-Key');
  if (!key || !/^[A-Za-z0-9._-]{1,80}$/.test(key)) refuse(422, 'idempotency_key_required');
  let body = {};
  try { const text = await c.req.text(); body = text ? JSON.parse(text) : {}; } catch { refuse(422, 'invalid_request'); }
  if (!body || Array.isArray(body) || typeof body !== 'object') refuse(422, 'invalid_request');
  const digest = createHash('sha256').update(`${method}|${path}|${canonical(body)}`).digest('hex');
  const response = await db.transaction(async (tx) => {
    await tx.execute(sql`SELECT 1 FROM reference_actors WHERE tenant=${tenant} AND user_id=${actor} FOR UPDATE`);
    const old = await tx.execute(sql`SELECT digest,response FROM reference_receipts WHERE tenant=${tenant} AND user_id=${actor} AND key=${key}`);
    if (old.rows.length) {
      if (old.rows[0].digest !== digest) refuse(409, 'idempotency_conflict');
      return old.rows[0].response;
    }
    const ok = { ok: true };
    if (path === '/planning' && method === 'POST') {
      if (Object.keys(body).sort().join(',') !== 'planned_for,task_id' || !id(body.task_id) || !date(body.planned_for)) refuse(422, 'invalid_request');
      const task = await tx.execute(sql`SELECT 1 FROM reference_tasks WHERE tenant=${tenant} AND user_id=${actor} AND task_id=${body.task_id}`);
      if (!task.rows.length) refuse(404, 'not_found');
      // Standard UUIDv5: DNS namespace and a synthetic actor/task/day name.
      const hash = createHash('sha1').update(Buffer.from('6ba7b8109dad11d180b400c04fd430c8', 'hex')).update(`${tenant}|${actor}|${body.task_id}|${body.planned_for}`).digest('hex');
      const planningId = `${hash.slice(0, 8)}-${hash.slice(8, 12)}-5${hash.slice(13, 16)}-${((parseInt(hash[16], 16) & 3) | 8).toString(16)}${hash.slice(17, 20)}-${hash.slice(20, 32)}`;
      const duplicate = await tx.execute(sql`SELECT 1 FROM reference_planning WHERE tenant=${tenant} AND user_id=${actor} AND task_id=${body.task_id} AND planned_for=${body.planned_for}::date`);
      if (duplicate.rows.length) refuse(409, 'planning_conflict');
      await tx.execute(sql`INSERT INTO reference_planning (tenant,user_id,planning_id,task_id,planned_for,position)
        SELECT ${tenant},${actor},${planningId},${body.task_id},${body.planned_for}::date,COALESCE(MAX(position),0)+1
        FROM reference_planning WHERE tenant=${tenant} AND user_id=${actor} AND planned_for=${body.planned_for}::date`);
    } else if (path === '/planning/reorder' && method === 'POST') {
      if (Object.keys(body).sort().join(',') !== 'expected_version,order,planned_for' || !date(body.planned_for)
        || !Number.isSafeInteger(body.expected_version) || body.expected_version < 1 || body.expected_version >= 2147483647 || typeof body.order !== 'string') refuse(422, 'invalid_request');
      const day = await tx.execute(sql`SELECT planning_id,version FROM reference_planning WHERE tenant=${tenant} AND user_id=${actor} AND planned_for=${body.planned_for}::date ORDER BY planning_id`);
      const order = body.order.split(',');
      if (!day.rows.length || order.length !== day.rows.length || new Set(order).size !== order.length || order.some((value) => !day.rows.some((row) => row.planning_id === value))) refuse(422, 'invalid_order');
      if (day.rows.some((row) => row.version !== body.expected_version)) refuse(409, 'reorder_stale');
      for (const [index, planningId] of order.entries()) await tx.execute(sql`UPDATE reference_planning SET position=${index + 1},version=version+1 WHERE tenant=${tenant} AND user_id=${actor} AND planning_id=${planningId}`);
    } else if (/^\/tasks\/[^/]+\/focus$/.test(path) && method === 'POST') {
      if (Object.keys(body).length) refuse(422, 'invalid_request');
      const taskId = path.split('/')[2];
      const target = await tx.execute(sql`SELECT planning_id FROM reference_planning WHERE tenant=${tenant} AND user_id=${actor} AND task_id=${taskId} AND planned_for=${today}::date`);
      if (!target.rows.length) refuse(404, 'not_found');
      const planningId = target.rows[0].planning_id;
      const active = await tx.execute(sql`SELECT 1 FROM reference_planning WHERE tenant=${tenant} AND user_id=${actor} AND focused AND planning_id<>${planningId}`);
      if (active.rows.length) refuse(409, 'focus_conflict');
      await tx.execute(sql`UPDATE reference_planning SET focused=true,paused_at=NULL WHERE tenant=${tenant} AND user_id=${actor} AND planning_id=${planningId}`);
    } else {
      const match = path.match(/^\/planning\/([a-f0-9-]{36})(\/pause)?$/);
      if (!match || !((method === 'DELETE' && !match[2]) || (method === 'POST' && match[2])) || Object.keys(body).length) refuse(404, 'not_found');
      const found = await tx.execute(sql`SELECT 1 FROM reference_planning WHERE tenant=${tenant} AND user_id=${actor} AND planning_id=${match[1]}`);
      if (!found.rows.length) refuse(404, 'not_found');
      if (method === 'DELETE') await tx.execute(sql`DELETE FROM reference_planning WHERE tenant=${tenant} AND user_id=${actor} AND planning_id=${match[1]}`);
      else await tx.execute(sql`UPDATE reference_planning SET focused=false,paused_at=${now} WHERE tenant=${tenant} AND user_id=${actor} AND planning_id=${match[1]}`);
    }
    if (process.env.REFERENCE_TEST_FAULTS === '1' && c.req.header('X-Reference-Fault') === 'rollback') refuse(503, 'store_unavailable');
    await tx.execute(sql`INSERT INTO reference_receipts VALUES (${tenant},${actor},${key},${digest},${JSON.stringify(ok)}::jsonb)`);
    return ok;
  });
  return c.json(response);
});
app.onError((error, c) => c.json({ error: error instanceof Refusal ? error.message : 'store_unavailable' }, error instanceof Refusal ? error.status : 503));
const server = serve({ fetch: app.fetch, hostname: process.env.HOST ?? '127.0.0.1', port: Number(process.env.PORT ?? 8080) });
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => server.close(async () => { await pool.end(); process.exit(0); }));
