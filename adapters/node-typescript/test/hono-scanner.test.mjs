/**
 * Issue #115 — Hono app discovery, route bindings, nested-router
 * composition, uncertainty honesty, and provider disablement. All
 * scans run through the production bundle against materialized
 * fixture projects.
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import {
  dispose,
  loadAdapter,
  recordsOfRelation,
  scanHonoFixture,
} from "./hono-helpers.mjs";

test("static fixture: apps, routes, and exact handler bindings", async () => {
  const context = await scanHonoFixture("static", "static");
  try {
    assert.equal(context.index.state, "complete");
    assert.equal(context.hono.provider.state, "complete");
    // One app instance, discovered through the reserved specifier and
    // the inventoried declaration — not by name.
    const apps = recordsOfRelation(context, "app-discovered");
    assert.equal(apps.length, 1);
    assert.equal(apps[0].from.name, "app");
    assert.match(apps[0].from.native, /^(ts1-|hono-inline-)/);
    // Five top-level routes plus the multi-method on() route (one
    // record per resolved method) — all bound to exact handlers.
    const routes = recordsOfRelation(context, "route-handler");
    assert.equal(routes.length, 7);
    const byPath = new Map(routes.map((record) => [`${record.method} ${record.path}`, record]));
    const list = byPath.get("GET /users");
    assert.ok(list, "GET /users bound");
    assert.equal(list.to.name, "listUsers");
    assert.equal(list.to.indexed, true);
    assert.match(list.to.native, /^ts1-/);
    assert.equal(list.confidence, "exact");
    assert.equal(list.span.path, "src/app.ts");
    assert.ok(list.span.startLine >= 3);
    assert.equal(list.revision, "fixture-revision-0001");
    // The template route with a const alias resolves deterministically.
    assert.ok(byPath.get("GET /v1/status"), "const-alias template path resolved");
    // Inline handlers keep stable occurrence keys and content digests.
    const health = byPath.get("GET /health");
    assert.match(health.to.native, /^hono-inline-/);
    assert.match(health.to.digest, /^sha256:[0-9a-f]{64}$/);
    // The multi-method on() route emits one record per resolved method.
    assert.ok(byPath.get("GET /multi"), "on() GET leg bound");
    assert.ok(byPath.get("POST /multi"), "on() POST leg bound");
  } finally {
    dispose(context.root);
  }
});

test("static fixture: cold and warm scans are byte-identical", async () => {
  const context = await scanHonoFixture("static", "static");
  try {
    const first = JSON.stringify(context.hono);
    const second = context.session.scan({
      profile: context.profile,
      readView: context.readView,
      permittedProjectRoot: context.project,
      frameworks: ["hono"],
    });
    assert.equal(JSON.stringify(second.index.frameworks.hono), first);
  } finally {
    dispose(context.root);
  }
});

test("composition: nested routers, shared children, and proven ordering", async () => {
  const context = await scanHonoFixture("composition", "composition");
  try {
    assert.equal(context.hono.provider.state, "complete");
    const routes = recordsOfRelation(context, "route-handler");
    const byPath = new Map(routes.map((record) => [`${record.method} ${record.path}`, record]));
    // Nested mounts compose every ancestor prefix: app mounts /api
    // which mounts /v1 twice (shared child) and /deep (depth 3).
    assert.ok(byPath.get("GET /api/ping"), "parent+mount path");
    assert.ok(byPath.get("GET /api/v1/users"), "nested v1 mount keeps the /api prefix");
    assert.ok(byPath.get("GET /api/v2/users"), "second mount of the shared child keeps the /api prefix");
    assert.ok(byPath.get("GET /api/deep/leaf"), "depth-3 mount composes root+parent+child prefixes");
    assert.ok(byPath.get("GET /shared/a"), "shared router mounted at /shared");
    assert.ok(byPath.get("GET /v1direct/users"), "basePath view surface");
    // A shared parent with its own nested mount, mounted twice: every
    // ancestor chain resolves independently (no dedupe-key collision),
    // and the nested mounts-router fact is recorded exactly once.
    assert.ok(byPath.get("GET /hub1/own"), "first hub chain");
    assert.ok(byPath.get("GET /hub2/own"), "second hub chain");
    assert.ok(byPath.get("GET /hub1/h/users"), "nested mount under first hub chain");
    assert.ok(byPath.get("GET /hub2/h/users"), "nested mount under second hub chain");
    assert.ok(byPath.get("GET /hub2/h/posts"), "second hub chain covers every child route");
    // Deterministic ordering: v1.ts provably initializes before the
    // mounting module (import edge), so its pre-mount routes are
    // complete; the same-module post-mount registration stays incomplete.
    const v1Users = byPath.get("GET /api/v1/users");
    assert.equal(v1Users.status, "complete");
    assert.deepEqual(v1Users.reasons, []);
    const late = byPath.get("GET /api/v1/late");
    assert.equal(late.status, "incomplete");
    assert.ok(late.reasons.includes("post-mount-registration"));
    // Mount relations are recorded per occurrence with the child identity
    // (relative mount paths; full prefix resolution is on the routes).
    // /h is emitted once although the hub is reached through two chains.
    const mounts = recordsOfRelation(context, "mounts-router");
    assert.equal(mounts.length, 8);
    const mountPaths = mounts.map((mount) => mount.path).sort();
    assert.deepEqual(mountPaths, ["/api", "/deep", "/h", "/hub1", "/hub2", "/shared", "/v1", "/v2"]);
    // No duplicate/invalid records: the validator runs inside the scan
    // and any violation would surface as hono-invalid-record uncertainty.
    assert.ok(
      !context.hono.uncertainty.some((row) => row.kind === "hono-invalid-record"),
      "no duplicate or invalid records",
    );
    // The base-path view is its own relation.
    const basePaths = recordsOfRelation(context, "base-path");
    assert.equal(basePaths.length, 1);
    assert.equal(basePaths[0].path, "/v1direct");
  } finally {
    dispose(context.root);
  }
});

test("reachability: conditional/deferred/unreachable registrations are never complete facts", async () => {
  const context = await scanHonoFixture("reachability", "reachability");
  try {
    const routes = recordsOfRelation(context, "route-handler");
    const byPath = new Map(routes.map((record) => [`${record.method} ${record.path}`, record]));
    // Top-level straight-line: proven, complete, no reasons.
    const top = byPath.get("GET /top");
    assert.ok(top, "top-level route present");
    assert.equal(top.status, "complete");
    assert.deepEqual(top.reasons, []);
    // Conditional: resolved fully, but never a complete fact.
    const conditional = byPath.get("GET /conditional");
    assert.ok(conditional, "conditional route present");
    assert.equal(conditional.status, "incomplete");
    assert.ok(conditional.reasons.includes("conditional-registration"));
    // Deferred: inside a function with no proven top-level call.
    const deferred = byPath.get("GET /deferred");
    assert.ok(deferred, "deferred route present");
    assert.equal(deferred.status, "incomplete");
    assert.ok(deferred.reasons.includes("deferred-registration"));
    // Proven-called: the module calls the wrapper straight-line at top
    // level, so the registration provably runs — complete.
    const called = byPath.get("GET /called");
    assert.ok(called, "proven-called route present");
    assert.equal(called.status, "complete");
    assert.deepEqual(called.reasons, []);
    // Unreachable: after a top-level return — unknown, never complete.
    const unreachable = byPath.get("GET /unreachable");
    assert.ok(unreachable, "unreachable route present");
    assert.equal(unreachable.status, "unknown");
    assert.ok(unreachable.reasons.includes("unreachable-registration"));
    // A deferred `use` passes its reachability into middleware claims:
    // both /plain records stay incomplete with the same reason.
    const middleware = recordsOfRelation(context, "uses-middleware")
      .filter((record) => record.to.name === "deferredMiddleware");
    assert.ok(middleware.length >= 1, "deferred middleware still recorded");
    for (const record of middleware) {
      assert.ok(record.reasons.includes("deferred-registration"));
      assert.notEqual(record.status, "complete");
    }
    // Spans carry half-open UTF-8 byte offsets converted from the
    // compiler's UTF-16 positions: the multibyte comment above the
    // /umlauf registration makes the two diverge, so exact byte
    // equality against the file bytes proves the conversion.
    const umlauf = byPath.get("GET /umlauf");
    assert.ok(umlauf, "multibyte-anchored route present");
    const source = readFileSync(join(context.project, "src", "app.ts"), "utf8");
    const utf16Index = source.indexOf('app.get("/umlauf"');
    const expectedByte = Buffer.byteLength(source.slice(0, utf16Index), "utf8");
    assert.notEqual(utf16Index, expectedByte, "fixture must contain multibyte bytes before the route");
    assert.equal(umlauf.span.startByte, expectedByte);
    assert.ok(umlauf.span.endByte > umlauf.span.startByte);
  } finally {
    dispose(context.root);
  }
});

test("mount reachability: conditional/deferred mounts constrain every derived relation", async () => {
  const context = await scanHonoFixture("mount-reach", "mount-reach");
  try {
    // The mount occurrences themselves carry their site reachability.
    const mounts = recordsOfRelation(context, "mounts-router");
    const byPath = new Map(mounts.map((row) => [row.path, row]));
    assert.equal(byPath.get("/ok").status, "complete");
    const conditionalMount = byPath.get("/cm");
    assert.ok(conditionalMount, "conditional mount recorded");
    assert.equal(conditionalMount.status, "incomplete");
    assert.ok(conditionalMount.reasons.includes("conditional-registration"));
    const deferredMount = byPath.get("/late");
    assert.ok(deferredMount, "deferred mount recorded");
    assert.equal(deferredMount.status, "incomplete");
    assert.ok(deferredMount.reasons.includes("deferred-registration"));
    // Ordering alone must not resurrect a mount whose site may never
    // run: the child registration is snapshot-included, but the route
    // record inherits the mount occurrence's scope.
    const routes = recordsOfRelation(context, "route-handler");
    const routeOf = (path) => routes.find((row) => row.path === path);
    assert.equal(routeOf("/ok/direct").status, "complete");
    for (const path of ["/cm/kid", "/late/kid"]) {
      const route = routeOf(path);
      assert.ok(route, `route ${path} recorded`);
      assert.equal(route.status, "incomplete");
      assert.ok(
        route.reasons.includes("conditional-registration") || route.reasons.includes("deferred-registration"),
        `${path} carries its mount's reachability reason`,
      );
    }
    // EVERY derived relation beneath an incomplete mount inherits the
    // same scope: middleware links, context writes and reads, response
    // sites, route classification, and handler→service calls.
    const constrained = ["uses-middleware", "context-write", "context-read", "returns-response", "handler-call"]
      .flatMap((name) => recordsOfRelation(context, name))
      .filter((row) => typeof row.path === "string" && (row.path.startsWith("/cm") || row.path.startsWith("/late")));
    assert.ok(constrained.length >= 10, "all derived relations are present under the mounts");
    for (const row of constrained) {
      assert.notEqual(row.status, "complete", `${row.relation} ${row.path} must not claim complete`);
      assert.ok(
        row.reasons.includes("conditional-registration") || row.reasons.includes("deferred-registration"),
        `${row.relation} ${row.path} carries the mount reason`,
      );
    }
    // The unconditional control mount keeps complete derived evidence.
    const control = ["uses-middleware", "context-write", "context-read", "returns-response", "handler-call"]
      .flatMap((name) => recordsOfRelation(context, name))
      .filter((row) => typeof row.path === "string" && row.path === "/ok/direct");
    for (const row of control) {
      if (row.relation.endsWith("uses-middleware")) continue; // the control route has no middleware
      assert.equal(row.status, "complete", `${row.relation} /ok/direct stays complete`);
    }
    assert.ok(control.some((row) => row.relation.endsWith("returns-response")));
    // Well-formed: no duplicates, no invalid records, no uncertainty.
    assert.equal(context.hono.provider.state, "complete");
  } finally {
    dispose(context.root);
  }
});

test("composition cycle: mounts are refused as unknown, never invented", async () => {
  const context = await scanHonoFixture("cycle", "composition-cycle");
  try {
    assert.equal(context.hono.provider.state, "partial");
    // The cyclic mount cannot resolve its (circularly-imported) target:
    // recorded unknown, no fabricated routes through it.
    const mounts = recordsOfRelation(context, "mounts-router");
    const refused = mounts.find((mount) => mount.path === "/a");
    assert.ok(refused, "the cyclic mount is still recorded");
    assert.equal(refused.status, "unknown");
    assert.equal(refused.to, null);
    // The resolvable mount of the cycle still binds its child.
    const resolved = mounts.find((mount) => mount.path === "/b");
    assert.ok(resolved && resolved.status === "complete");
  } finally {
    dispose(context.root);
  }
});

test("lookalikes: local Hono classes and router-shaped modules are never apps", async () => {
  const context = await scanHonoFixture("lookalike", "lookalike");
  try {
    assert.equal(context.hono.provider.counts.apps, 0);
    assert.equal(context.hono.provider.counts.routes, 0);
    assert.equal(recordsOfRelation(context, "route-handler").length, 0);
    // The local class *type* is Hono-shaped: the lost receiver is
    // reported as uncertainty, not silently dropped.
    assert.ok(context.hono.uncertainty.some((row) => row.kind === "hono-unsupported-receiver"));
  } finally {
    dispose(context.root);
  }
});

test("uncertainty: dynamic paths, methods, receivers stay unknown with reasons", async () => {
  const context = await scanHonoFixture("uncertainty", "uncertainty");
  try {
    assert.equal(context.hono.provider.state, "partial");
    const routes = recordsOfRelation(context, "route-handler");
    const dynamic = routes.find((record) => record.status === "unknown"
      && record.reasons.includes("dynamic-path"));
    assert.ok(dynamic, "dynamic concatenation stays unknown");
    assert.equal(dynamic.path, "/");
    const dynamicMethod = routes.find((record) => record.reasons.includes("dynamic-method"));
    assert.ok(dynamicMethod, "dynamic `on` method array stays unknown");
    // The resolvable template route is complete.
    const templated = routes.find((record) => record.path === "/templated/ok");
    assert.ok(templated && templated.status === "complete");
    // Factory-produced receivers are reported, never bound.
    assert.ok(context.hono.uncertainty.some((row) => row.kind === "hono-unsupported-receiver"));
    // Uncertainty rides the generic scan surface too.
    assert.ok(context.index.anyUncertainty.some((row) => row.kind === "hono-dynamic-path"));
  } finally {
    dispose(context.root);
  }
});

test("framework data reads are bounded per file (64 KiB) and per call", async () => {
  const context = await scanHonoFixture("contracts-bound", "static");
  try {
    // Baseline: the contract file joins (POST /users → users.create).
    const baseline = recordsOfRelation(context, "endpoint-contract");
    assert.ok(baseline.some((row) => row.note === "users.create"), "baseline join present");
    // Grow the copied contract file beyond the 64 KiB per-file data
    // bound: the read must be refused, the join phase must report
    // missing evidence instead of parsing an unbounded file, and the
    // scan itself must still complete.
    const contractPath = join(context.project, "lekalo", "endpoints.json");
    const document = JSON.parse(readFileSync(contractPath, "utf8"));
    document.endpoints.push({ id: "huge", method: "GET", path: "/" + "x".repeat(80 * 1024) });
    writeFileSync(contractPath, JSON.stringify(document));
    const rescanned = context.session.scan({
      profile: context.profile,
      readView: context.readView,
      permittedProjectRoot: context.project,
      frameworks: ["hono"],
    });
    const joins = (rescanned.index.frameworks?.hono?.records ?? [])
      .filter((row) => row.relation === "dev.lekalo.hono/endpoint-contract");
    assert.equal(joins.length, 0, "an over-bound contract file contributes nothing");
    assert.equal(rescanned.index.state, "complete", "the bounded refusal never breaks the scan");
  } finally {
    dispose(context.root);
  }
});

test("route signature/source edits invalidate freshness fingerprints", async () => {
  const context = await scanHonoFixture("freshness", "static");
  try {
    const routeOf = (hono) => hono.records.find((record) =>
      record.relation === "dev.lekalo.hono/route-handler" && record.path === "/users");
    const before = routeOf(context.hono);
    const appBefore = context.hono.records.find((record) =>
      record.relation === "dev.lekalo.hono/app-discovered");
    // Edit the handler BODY only: the handler digest changes, so the
    // route-handler fingerprint changes; app discovery is untouched.
    const handlersPath = join(context.project, "src", "handlers.ts");
    const original = readFileSync(handlersPath, "utf8");
    writeFileSync(handlersPath, original.replace(
      "return c.json(queryUsers());",
      "return c.json(queryUsers().slice(0, 10));",
    ));
    const after = context.session.scan({
      profile: context.profile,
      readView: context.readView,
      permittedProjectRoot: context.project,
      frameworks: ["hono"],
    });
    const routeAfter = routeOf(after.index.frameworks.hono);
    assert.notEqual(routeAfter.fingerprint, before.fingerprint, "body edit invalidates the route fingerprint");
    assert.notEqual(routeAfter.to.digest, before.to.digest);
    const appAfter = after.index.frameworks.hono.records.find((record) =>
      record.relation === "dev.lekalo.hono/app-discovered");
    assert.equal(appAfter.fingerprint, appBefore.fingerprint, "unrelated records stay stable");
  } finally {
    dispose(context.root);
  }
});

test("provider disabled or absent: the generic index is byte-identical", async () => {
  const adapter = await loadAdapter();
  const kernel = adapter.__lekaloKernel;
  const scanner = adapter.__lekaloScanner;
  const enabled = await scanHonoFixture("disabled-on", "static");
  try {
    const disabled = await scanHonoFixture("disabled-off", "static", { frameworks: [] });
    const enabledGeneric = { ...enabled.index, frameworks: undefined };
    assert.equal(
      JSON.stringify({ ...disabled.index }),
      JSON.stringify(enabledGeneric),
      "disabled output equals enabled output minus the frameworks family",
    );
    assert.equal(disabled.index.frameworks, undefined);
    // Absent policy (the frameworks key not requested at all) behaves
    // exactly like the disabled policy: no family, generic bytes.
    const absent = disabled.session.scan({
      profile: disabled.profile,
      readView: disabled.readView,
      permittedProjectRoot: disabled.project,
    });
    assert.equal(absent.index.frameworks, undefined);
    // The policy helper: unknown ids are surfaced, never silently run.
    assert.deepEqual(
      scanner.enabledFrameworkProviders({ providers: [{ id: "hono", state: "enabled" }] }),
      ["hono"],
    );
    assert.deepEqual(
      scanner.enabledFrameworkProviders({ providers: [{ id: "other", state: "enabled" }] }),
      ["other"],
    );
    assert.deepEqual(scanner.enabledFrameworkProviders(null), []);
    void kernel;
  } finally {
    dispose(enabled.root);
  }
});

test("fluent verb chains: every statically-known link keeps its record", async () => {
  const context = await scanHonoFixture("fluent-chains", "fluent");
  try {
    assert.equal(context.hono.provider.state, "complete");
    const routes = recordsOfRelation(context, "route-handler");
    const byKey = new Map(routes.map((row) => [`${row.method} ${row.path}`, row]));
    // Each link of app.get(p, h).post(p, h2).put(p, h3) resolves to its
    // own complete route record with the right handler — no asymmetric
    // loss into a bare receiver uncertainty.
    assert.equal(byKey.get("GET /fluent").to.name, "getHandler");
    assert.equal(byKey.get("POST /fluent").to.name, "postHandler");
    assert.equal(byKey.get("PUT /fluent").to.name, "putHandler");
    for (const key of ["GET /fluent", "POST /fluent", "PUT /fluent", "GET /plain"]) {
      assert.equal(byKey.get(key).status, "complete");
      assert.deepEqual(byKey.get(key).reasons, []);
    }
    // The chain is not a lost receiver: no unsupported-receiver row.
    assert.equal(
      context.hono.uncertainty.filter((row) => row.kind === "hono-unsupported-receiver").length,
      0,
    );
  } finally {
    dispose(context.root);
  }
});

test("inline mount targets: fluent-chain children mount under their prefix, never orphan", async () => {
  const context = await scanHonoFixture("inline-mount", "inline-mount");
  try {
    assert.equal(context.hono.provider.state, "complete");
    // The mount binds the inline-constructed child instead of refusing.
    const mounts = recordsOfRelation(context, "mounts-router");
    const byPath = new Map(mounts.map((row) => [row.path, row]));
    for (const path of ["/sub", "/deep", "/n", "/maybe"]) {
      const mount = byPath.get(path);
      assert.ok(mount, `mount ${path} recorded`);
      assert.ok(mount.to, `mount ${path} binds its inline child`);
      assert.equal(mount.to.name, "hono");
    }
    assert.equal(byPath.get("/maybe").status, "incomplete");
    assert.ok(byPath.get("/maybe").reasons.includes("conditional-registration"));
    // The inline child's registrations resolve UNDER the mount prefix —
    // no standalone orphan /inside route, no unknown-handler refusal.
    const routes = recordsOfRelation(context, "route-handler");
    const byKey = new Map(routes.map((row) => [`${row.method} ${row.path}`, row]));
    assert.equal(byKey.get("GET /sub/inside").to.name, "insideHandler");
    assert.equal(byKey.get("GET /sub/inside").status, "complete");
    assert.equal(byKey.get("GET /deep/a").to.name, "insideHandler");
    assert.equal(byKey.get("POST /deep/b").to.name, "deepHandler");
    assert.equal(byKey.get("GET /deep/n/c").to.name, "deepHandler");
    for (const key of ["GET /deep/a", "POST /deep/b", "GET /deep/n/c"]) {
      assert.equal(byKey.get(key).status, "complete");
    }
    // A conditional inline mount keeps its whole subtree constrained.
    const maybe = byKey.get("GET /maybe/x");
    assert.ok(maybe, "conditional inline route recorded");
    assert.equal(maybe.status, "incomplete");
    assert.ok(maybe.reasons.includes("conditional-registration"));
    // No orphan routes at unprefixed paths.
    assert.equal(byKey.get("GET /inside"), undefined);
    assert.equal(byKey.get("GET /a"), undefined);
    assert.equal(
      context.hono.uncertainty.filter((row) => row.kind === "hono-unknown-handler").length,
      0,
      "no unknown-handler uncertainty for provable inline targets",
    );
  } finally {
    dispose(context.root);
  }
});
