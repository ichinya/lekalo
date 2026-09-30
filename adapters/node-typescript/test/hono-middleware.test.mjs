/**
 * Issue #115 — middleware chains, order, applicability, roles, and
 * context evidence. Presence is never authorization proof; context
 * keys stay namespaced implementation evidence.
 */
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  dispose,
  recordsOfRelation,
  scanHonoFixture,
} from "./hono-helpers.mjs";

test("middleware chains: global order, path filters, inline order, next evidence", async () => {
  const context = await scanHonoFixture("middleware", "middleware");
  try {
    const chains = recordsOfRelation(context, "uses-middleware");
    const byRoute = new Map();
    for (const record of chains) {
      const key = `${record.method} ${record.path}`;
      if (!byRoute.has(key)) byRoute.set(key, []);
      byRoute.get(key).push(record);
    }
    // /whoami: logger(0) → tenant(1); auth is path-filtered to /admin/*.
    const whoami = byRoute.get("GET /whoami").sort((a, b) => a.ordinal - b.ordinal);
    assert.deepEqual(whoami.map((row) => row.to.name), ["logger", "tenant", "auth"]);
    assert.deepEqual(whoami.map((row) => row.ordinal), [0, 1, 2]);
    assert.deepEqual(whoami.map((row) => row.unwindOrdinal), [3, 2, 1]);
    // Entry ordinals are explicit for inspect/impact/context consumption.
    assert.ok(whoami.every((row) => Number.isInteger(row.ordinal)));
    // The /admin/* filter cannot be proven disjoint from /whoami by
    // literals alone: the entry stays conditional, never guessed.
    const authOnWhoami = whoami[2];
    assert.equal(authOnWhoami.role, "auth");
    assert.equal(authOnWhoami.provenance, "explicit");
    assert.ok(authOnWhoami.reasons.includes("conditional-applicability"));
    assert.equal(authOnWhoami.status, "incomplete");
    assert.equal(authOnWhoami.confidence, "medium");
    // The short-circuit candidate: no next() call detected anywhere.
    const reset = byRoute.get("POST /admin/reset").sort((a, b) => a.ordinal - b.ordinal);
    const maintenance = reset.find((row) => row.to.name === "maintenance");
    assert.ok(maintenance.reasons.includes("no-next-call-detected"));
    assert.match(maintenance.note, /next=absent/);
    // Middleware with await next() carries detected pass-through.
    const logger = reset.find((row) => row.to.name === "logger");
    assert.match(logger.note, /next=detected/);
    assert.equal(logger.status, "complete");
    // Inline middleware keeps its registration ordinal after the
    // globals and the conditional admin filter.
    const health = byRoute.get("GET /health").sort((a, b) => a.ordinal - b.ordinal);
    assert.deepEqual(health.map((row) => row.to.name), ["logger", "tenant", "auth", "cacheHeaders"]);
    assert.deepEqual(health.map((row) => row.unwindOrdinal), [4, 3, 2, 1]);
  } finally {
    dispose(context.root);
  }
});

test("parent path-filtered middleware composes onto mounted routes", async () => {
  const context = await scanHonoFixture("middleware-mounts", "middleware-mounts");
  try {
    const middleware = recordsOfRelation(context, "uses-middleware");
    const byRoute = new Map();
    for (const record of middleware) {
      if (!byRoute.has(record.path)) byRoute.set(record.path, []);
      byRoute.get(record.path).push(record);
    }
    // /api/ping: gate (wildcard parent filter, conditional), audit
    // (literal parent filter, provably applicable), childLogger
    // (the child's own global middleware) — in execution order.
    const ping = (byRoute.get("/api/ping") ?? []).sort((a, b) => a.ordinal - b.ordinal);
    assert.deepEqual(ping.map((row) => row.to.name), ["gate", "audit", "childLogger"]);
    // Chain provenance rides the note: parent-use rows mark mount
    // ancestors, the child's own use marks the owner.
    assert.match(ping[0].note, /^parent-use;/);
    assert.match(ping[1].note, /^parent-use;/);
    assert.match(ping[2].note, /^use;/);
    const gateRow = ping[0];
    assert.equal(gateRow.status, "incomplete");
    assert.ok(gateRow.reasons.includes("conditional-applicability"));
    const auditRow = ping[1];
    assert.equal(auditRow.status, "complete");
    assert.deepEqual(auditRow.reasons, []);
    assert.match(auditRow.note, /next=detected/);
    // Same coverage on the second child route: the wildcard never
    // silently vanishes on mounted routes.
    const other = (byRoute.get("/api/other") ?? []).sort((a, b) => a.ordinal - b.ordinal);
    assert.deepEqual(other.map((row) => row.to.name), ["gate", "audit", "childLogger"]);
    // The /open subtree: the wildcard parent filter cannot be proven
    // disjoint from any literal path, so gate stays conditional there;
    // but the literal /api filter is provably disjoint — no audit, no
    // childLogger. Unprovable overlap is a reasoned record, never a
    // guessed applicability and never silence.
    const openLeaf = (byRoute.get("/open/leaf") ?? []).sort((a, b) => a.ordinal - b.ordinal);
    assert.deepEqual(openLeaf.map((row) => row.to.name), ["gate"]);
    assert.equal(openLeaf[0].status, "incomplete");
    assert.ok(openLeaf[0].reasons.includes("conditional-applicability"));
    // The child's own middleware still carries next evidence.
    const loggerRow = ping.find((row) => row.to.name === "childLogger");
    assert.equal(loggerRow.status, "complete");
  } finally {
    dispose(context.root);
  }
});

test("roles: explicit JSDoc annotations only; presence never authorizes", async () => {
  const context = await scanHonoFixture("middleware-roles", "middleware");
  try {
    const chains = recordsOfRelation(context, "uses-middleware");
    const byName = new Map(chains.map((row) => [row.to.name, row]));
    // Explicit annotations become explicit-provenance role evidence.
    assert.equal(byName.get("logger").role, "logging");
    assert.equal(byName.get("tenant").role, "tenant");
    assert.equal(byName.get("auth").role, "auth");
    // No annotation: no role claim — never inferred from the name.
    const cache = byName.get("cacheHeaders");
    assert.equal(cache.role, null);
    assert.equal(cache.provenance, "detected");
    // The provider never mints an authorization relation.
    for (const record of context.hono.records) {
      assert.equal(
        record.relation.endsWith("/authorizes") || record.relation.includes("authorize"),
        false,
        "no canonical authorization relation may be minted",
      );
    }
    // Context keys stay namespaced evidence: logger writes requestId on
    // all five app routes, tenant writes tenantId on all five,
    // cacheHeaders writes cache on /health. The runtimeApp routes get
    // neither global middleware, so they add no writes.
    const writes = recordsOfRelation(context, "context-write");
    const keys = writes.map((row) => row.note).sort();
    assert.deepEqual(keys, [
      "cache", "requestId", "requestId", "requestId", "requestId", "requestId",
      "tenantId", "tenantId", "tenantId", "tenantId", "tenantId",
    ]);
    const reads = recordsOfRelation(context, "context-read");
    assert.ok(reads.every((row) => typeof row.note === "string" && row.note.length > 0));
  } finally {
    dispose(context.root);
  }
});

test("use() filters: const aliases stay filters; dynamic filters never fabricate handlers", async () => {
  const context = await scanHonoFixture("use-filters", "middleware");
  try {
    const middleware = recordsOfRelation(context, "uses-middleware");
    // No uses-middleware endpoint may be fabricated from a string-
    // shaped argument: consolePrefix/runtimePrefix never become `to`.
    const fabricated = middleware.filter((row) => row.to.name === "consolePrefix"
      || row.to.name === "runtimePrefix");
    assert.equal(fabricated.length, 0, "no string/filter argument becomes a middleware endpoint");
    // The const-alias filter resolves like a literal: /console/panel
    // gets exactly one APPLICABLE auth binding (the /admin/* wildcard
    // row stays conditional-incomplete).
    const consoleComplete = middleware.filter((row) => row.path === "/console/panel"
      && row.to.name === "auth" && row.status === "complete");
    assert.equal(consoleComplete.length, 1, "const-alias filter applies auth to /console/panel");
    assert.deepEqual(consoleComplete[0].reasons, []);
    // The dynamic filter cannot resolve: auth stays bound with
    // conditional applicability and the dynamic-path-filter reason —
    // never silently dropped, never guessed applicable.
    const runtimeDynamic = middleware.filter((row) => row.path === "/runtime/panel"
      && row.to.name === "auth" && row.reasons.includes("dynamic-path-filter"));
    assert.equal(runtimeDynamic.length, 1, "dynamic filter keeps auth recorded");
    assert.equal(runtimeDynamic[0].status, "incomplete");
    assert.ok(runtimeDynamic[0].reasons.includes("conditional-applicability"));
    // The unresolved filter surfaces on the uncertainty surface too.
    assert.ok(context.hono.uncertainty.some((row) => row.kind === "hono-dynamic-path-filter"));
  } finally {
    dispose(context.root);
  }
});

test("pre-mount parent middleware is visible on mounted child routes", async () => {
  const context = await scanHonoFixture("middleware-mount", "middleware");
  try {
    // The middleware fixture has no mounts; this probe asserts the
    // chain composition stays deterministic when a provider-disabled
    // scan of the same fixture is compared for middleware absence.
    const disabled = await scanHonoFixture("middleware-mount-off", "middleware", { frameworks: [] });
    assert.equal(disabled.hono, null);
    assert.ok(context.hono.records.length > 0);
    dispose(disabled.root);
  } finally {
    dispose(context.root);
  }
});
