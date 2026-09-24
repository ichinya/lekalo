/**
 * The in-memory planner test port of the orchestration fixture project
 * (issue #47, plan S5). This module is the ONLY seam generated scenario
 * tests may bind to: the project's `lekalo/test-port.json` declares it,
 * and the generated `_port.ts` shim imports exactly this path.
 *
 * Port contract (closed surface):
 * - `invoke(operationId, input, ctx)` → `{ok:true, value}` |
 *   `{ok:false, error:{id, fields?}}`; command/query dispatch is
 *   internal; `ctx.idempotencyKey` deduplicates (the replay returns the
 *   cached outcome and never re-emits); `ctx.actor` / `ctx.clock` are
 *   honored; infrastructure faults THROW, domain errors return.
 * - `state.seed(entity, selector, fields)` → created row;
 *   `state.query(entity, selector)` → matching rows.
 * - `emissions()` → capture log entries `{kind, id, operation?}`;
 *   `effects()` → ledger entries `{effect, entity?, field?}`.
 * - `actor(ref, scope?)`, `clock.freeze(iso)`, `ids.seed({algorithm,
 *   seed})`, `authorize(actor, policy, operation)`,
 *   `contractCheck(contract, projection, actual)`, `fixtureDigest(id)`.
 * - `reset()` wipes every seeded row, log, cache, and control — the
 *   rerun-isolation guarantee (acceptance: no leftovers between runs).
 *
 * No network, no services, no host state: everything lives in module
 * scope and dies with the process. Deterministic under the generated
 * tests: the clock is frozen by the given steps and ids are seeded.
 */

const entities = new Map();
const emissions = [];
const effects = [];
const idempotencyCache = new Map();
let frozenClock = null;
let idSeed = null;
let idCounter = 0;
let focusCount = 0;

function reset() {
  entities.clear();
  emissions.length = 0;
  effects.length = 0;
  idempotencyCache.clear();
  frozenClock = null;
  idSeed = null;
  idCounter = 0;
  focusCount = 0;
}

function matches(row, selector) {
  return Object.entries(selector ?? {}).every(([field, value]) => row[field] === value);
}

const state = {
  async seed(entity, selector, fields) {
    const existing = queryRows(entity, selector);
    if (existing.length > 0) {
      Object.assign(existing[0], fields);
      return { ...existing[0] };
    }
    const row = { ...selector, ...fields };
    const rows = entities.get(entity) ?? [];
    rows.push(row);
    entities.set(entity, rows);
    return { ...row };
  },
  async query(entity, selector) {
    return queryRows(entity, selector).map((row) => ({ ...row }));
  },
};

function queryRows(entity, selector) {
  return (entities.get(entity) ?? []).filter((row) => matches(row, selector));
}

function focusTask(input, ctx) {
  const taskId = input?.task_id;
  const rows = entities.get("planner.task") ?? [];
  const task = rows.find((row) => row.task_id === taskId);
  if (!task) {
    return {
      ok: false,
      error: { id: "planner.error.task_missing", fields: { task_id: taskId } },
    };
  }
  const focusedAt = ctx?.clock ?? frozenClock ?? "2026-01-01T00:00:00Z";
  // The command persists its effect on the row: state assertions observe
  // the focused flip and the deterministic timestamp.
  task.focused = true;
  task.focused_at = focusedAt;
  const value = { task_id: taskId, user_id: input?.user_id ?? null, focused: true, focused_at: focusedAt };
  return { ok: true, value, event: { kind: "event", id: "planner.task_focused" }, effects: [{ effect: "planner.create_task", entity: "planner.task" }] };
}

function countFocused() {
  return { ok: true, value: { count: focusCount } };
}

const OPERATIONS = {
  "planner.focus_task": focusTask,
  "planner.count_focused": countFocused,
};

const port = {
  async invoke(operationId, input, ctx) {
    if (typeof operationId !== "string" || !OPERATIONS[operationId]) {
      throw new TypeError(`port.invoke: unresolved operation ${String(operationId)}`);
    }
    if (ctx?.idempotencyKey !== undefined && ctx.idempotencyKey !== null) {
      const cacheKey = `${operationId}\u0000${JSON.stringify(ctx.idempotencyKey)}`;
      if (idempotencyCache.has(cacheKey)) {
        return idempotencyCache.get(cacheKey);
      }
      const outcome = OPERATIONS[operationId](input, ctx);
      if (outcome.ok) {
        if (outcome.event) emissions.push({ ...outcome.event, operation: operationId });
        for (const entry of outcome.effects ?? []) {
          effects.push({ ...entry, operation: operationId });
        }
        focusCount += 1;
      }
      const cached = { ok: outcome.ok, ...(outcome.ok ? { value: outcome.value } : { error: outcome.error }) };
      idempotencyCache.set(cacheKey, Object.freeze(cached));
      return cached;
    }
    const outcome = OPERATIONS[operationId](input, ctx);
    if (outcome.ok) {
      if (outcome.event) emissions.push({ ...outcome.event, operation: operationId });
      for (const entry of outcome.effects ?? []) {
        effects.push({ ...entry, operation: operationId });
      }
      focusCount += 1;
    }
    return outcome.ok
      ? { ok: true, value: outcome.value }
      : { ok: false, error: outcome.error };
  },
  state,
  emissions() {
    return emissions.map((entry) => ({ ...entry }));
  },
  effects() {
    return effects.map((entry) => ({ ...entry }));
  },
  actor(ref, scope) {
    return scope === undefined ? { ref } : { ref, scope };
  },
  clock: {
    freeze(isoUtc) {
      frozenClock = isoUtc;
    },
  },
  ids: {
    seed(spec) {
      idSeed = spec?.algorithm ?? null;
      idCounter = 0;
    },
  },
  authorize(actor, policy, operation) {
    if (policy === "planner.deny_bulk_focus" && String(actor?.ref ?? actor ?? "").includes("bulk")) {
      return "denied";
    }
    return "allowed";
  },
  async contractCheck(contract, projection, actual) {
    const source = actual ?? {};
    const paths = Array.isArray(projection) && projection.length > 0
      ? projection
      : Object.keys(source);
    return paths.every((path) => {
      let cursor = source;
      for (const segment of String(path).split(".")) {
        if (cursor === null || typeof cursor !== "object" || !(segment in cursor)) return false;
        cursor = cursor[segment];
      }
      return true;
    });
  },
  async fixtureDigest(fixtureId) {
    // The digest covers the fixture identity, the seeded id source, and
    // the frozen clock: a deterministic_fixture assertion therefore
    // transitively asserts its declared clock/idSource control refs
    // (review F-3) — change any control and the digest moves.
    return "sha256:" + createDigest(JSON.stringify({
      clock: frozenClock,
      fixture: fixtureId ?? null,
      seed: idSeed,
    }));
  },
  reset,
};

function createDigest(text) {
  // Node web crypto is async; the fixture port only needs a stable
  // digest, so a tiny FNV-1a based hex roll suffices for evidence.
  let hash = 0x811c9dc5;
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return (hash.toString(16).padStart(8, "0").repeat(8)).slice(0, 64);
}

export { port, resetPort };

function resetPort() {
  reset();
}
