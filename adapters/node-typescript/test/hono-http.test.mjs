/**
 * Issue #115 — request validation, response mapping, error handling,
 * OpenAPI operation links, handler→service joins, endpoint-contract
 * joins, test bindings, and the API/SSR distinction.
 */
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  dispose,
  recordsOfRelation,
  scanHonoFixture,
} from "./hono-helpers.mjs";

test("validation: validator targets bind to routes with resolved schema symbols", async () => {
  const context = await scanHonoFixture("http-validation", "http");
  try {
    const validations = recordsOfRelation(context, "validates-request");
    assert.equal(validations.length, 3);
    const byTarget = new Map(validations.map((row) => [row.note, row]));
    const body = byTarget.get("zValidator:json");
    assert.ok(body, "zValidator json target bound");
    assert.equal(body.to.name, "CreateUserSchema");
    assert.equal(body.to.indexed, true);
    assert.equal(body.method, "POST");
    assert.equal(body.path, "/users");
    assert.equal(body.ordinal, 0);
    const query = byTarget.get("zValidator:query");
    assert.equal(query.to.name, "ListQuerySchema");
    const header = byTarget.get("validator:header");
    assert.ok(header, "builtin validator bound");
    assert.equal(header.path, "/users");
    assert.equal(header.ordinal, 1);
  } finally {
    dispose(context.root);
  }
});

test("responses and errors: statuses, HTTPException, onError, notFound", async () => {
  const context = await scanHonoFixture("http-responses", "http");
  try {
    const responses = recordsOfRelation(context, "returns-response")
      .filter((row) => row.note !== "route-classification");
    const created = responses.find((row) => row.path === "/users" && row.method === "POST");
    assert.equal(created.httpStatus, 201);
    assert.equal(created.to.name, "created");
    // The explicit 200 status is exact evidence, not a default.
    const listed = responses.find((row) => row.path === "/users" && row.method === "GET");
    assert.equal(listed.httpStatus, 200);
    assert.equal(listed.note, "json");
    const errors = recordsOfRelation(context, "handles-error");
    const notFound = errors.find((row) => row.path === "/missing");
    assert.equal(notFound.httpStatus, 404);
    assert.equal(notFound.confidence, "exact");
    const boom = errors.find((row) => row.path === "/boom");
    assert.equal(boom.status, "incomplete");
    assert.ok(boom.reasons.includes("dynamic-status"));
    const onError = errors.find((row) => row.note === "onError");
    assert.ok(onError, "onError handler recorded");
    const notFoundHandler = errors.find((row) => row.note === "notFound");
    assert.ok(notFoundHandler, "notFound handler recorded");
    // Handlers that only throw have no response site: classified unknown.
    const classifications = recordsOfRelation(context, "returns-response")
      .filter((row) => row.note === "route-classification");
    const thrower = classifications.find((row) => row.path === "/missing");
    assert.equal(thrower.facet, "unknown");
    assert.ok(thrower.reasons.includes("no-response-evidence"));
    const page = classifications.find((row) => row.path === "/page");
    assert.equal(page.facet, "html");
  } finally {
    dispose(context.root);
  }
});

test("openapi: createRoute definitions link operationIds to handlers", async () => {
  const context = await scanHonoFixture("openapi", "openapi");
  try {
    const operations = recordsOfRelation(context, "openapi-operation");
    // The join record (to: the bound handler) is distinct from the
    // declaration-side definition record (to: null).
    const linked = operations.find((row) => row.note === "users.show" && row.to !== null);
    assert.ok(linked, "operationId link recorded");
    assert.equal(linked.method, "GET");
    assert.equal(linked.path, "/users/{id}");
    assert.equal(linked.to.name, "getUserHandler");
    assert.equal(linked.confidence, "exact");
    assert.equal(linked.status, "complete");
    // The dynamic-method definition stays incomplete with its span —
    // and the reason names the failed AXIS: the method is dynamic, the
    // path resolved fine (it was mislabeled dynamic-path before).
    const dynamic = operations.find((row) => row.note === "definition-incomplete");
    assert.ok(dynamic, "incomplete definition recorded");
    assert.equal(dynamic.status, "incomplete");
    assert.equal(dynamic.method, null);
    assert.ok(dynamic.reasons.includes("dynamic-method"));
    assert.equal(dynamic.reasons.includes("dynamic-path"), false);
    // The declaration side also emits its own reasoned record — the
    // missing operationId is its own axis, never silent, never folded
    // into dynamic-path.
    const declared = operations.find((row) => row.note === "operationid-unknown" && row.to === null);
    assert.ok(declared, "declaration-side record present");
    assert.equal(declared.status, "incomplete");
    assert.ok(declared.reasons.includes("dynamic-method"));
    assert.ok(declared.reasons.includes("operationid-unknown"));
  } finally {
    dispose(context.root);
  }
});

test("handler → service calls bind exact native identities through the checker", async () => {
  const context = await scanHonoFixture("calls", "static");
  try {
    const calls = recordsOfRelation(context, "handler-call");
    assert.equal(calls.length, 3);
    const query = calls.find((row) => row.to.name === "queryUsers");
    assert.equal(query.from.name, "listUsers");
    assert.match(query.from.native, /^ts1-/);
    assert.match(query.to.native, /^ts1-/);
    assert.equal(query.confidence, "exact");
    assert.equal(query.status, "complete");
    assert.ok(calls.find((row) => row.to.name === "findUser"));
    assert.ok(calls.find((row) => row.to.name === "saveUser"));
  } finally {
    dispose(context.root);
  }
});

test("endpoint contracts: unique joins bind, ambiguity and gaps stay reasoned", async () => {
  const context = await scanHonoFixture("contracts", "static");
  try {
    const joins = recordsOfRelation(context, "endpoint-contract");
    const byPath = new Map(joins.map((row) => [`${row.method} ${row.path}`, row]));
    const create = byPath.get("POST /users");
    assert.equal(create.status, "complete");
    assert.equal(create.note, "users.create");
    assert.equal(create.provenance, "inferred", "shape joins are candidates until confirmed");
    assert.equal(create.confidence, "medium");
    const show = byPath.get("GET /users/:id");
    assert.equal(show.note, "users.show");
    // The duplicated contract entry makes GET /users ambiguous.
    const list = byPath.get("GET /users");
    assert.equal(list.status, "incomplete");
    assert.ok(list.reasons.includes("ambiguous-endpoint-join"));
    assert.equal(list.note, "2-candidates");
    // No contract for /v1/status: reasoned gap, never a silent drop.
    const status = byPath.get("GET /v1/status");
    assert.ok(status.reasons.includes("missing-endpoint-join"));
    // The multi-method on() route joins the POST contract: the record
    // is labeled with the JOINED contract method, never methods[0].
    const multi = joins.find((row) => row.path === "/multi");
    assert.ok(multi, "multi-method route joined");
    assert.equal(multi.status, "complete");
    assert.equal(multi.method, "POST", "label carries the joined contract method");
    assert.equal(multi.note, "multi.any");
  } finally {
    dispose(context.root);
  }
});

test("tests: app.request and testClient flows bind to resolved routes", async () => {
  const context = await scanHonoFixture("tests", "tests");
  try {
    const bindings = recordsOfRelation(context, "route-test");
    const byNote = new Map();
    for (const row of bindings) {
      if (!byNote.has(`${row.method} ${row.path}`)) byNote.set(`${row.method} ${row.path}`, row);
    }
    const list = bindings.find((row) => row.method === "GET" && row.path === "/items" && row.from.name.startsWith("items>"));
    assert.ok(list, "app.request GET /items bound");
    assert.equal(list.to.name, "listHandler");
    assert.equal(list.status, "complete");
    const created = byNote.get("POST /items");
    assert.ok(created && created.to.indexed === false, "inline handler target");
    assert.match(created.from.native, /^hono-inline-/);
    // Unknown path: reasoned gap.
    const missing = bindings.find((row) => row.path === "/nope");
    assert.ok(missing.reasons.includes("missing-endpoint-join"));
    // Dynamic URL: never guessed.
    assert.ok(context.hono.uncertainty.some((row) => row.kind === "hono-dynamic-test-target"));
    // A bare request(...) helper call is not Hono evidence: no record
    // and no unknown-app uncertainty may be fabricated for it.
    assert.equal(
      context.hono.uncertainty.filter((row) => row.kind === "hono-unknown-test-app").length,
      0,
      "bare request() helpers must not produce unknown-app uncertainty",
    );
    // Mounted routes bind through the root app: GET /sub/other resolves
    // through app.route('/sub', other) and binds listHandler.
    const mounted = byNote.get("GET /sub/other");
    assert.ok(mounted, "mounted route bound through the root app");
    assert.equal(mounted.to.name, "listHandler");
    assert.equal(mounted.status, "complete");
    // testClient binds the app identity.
    const client = bindings.find((row) => row.note === "test-client");
    assert.ok(client, "testClient app identity recorded");
    const clientGet = bindings.find((row) => row.path === "/items" && row.from.name.startsWith("client>"));
    assert.ok(clientGet, "client.get binds through the app identity");
    assert.equal(clientGet.to.name, "listHandler");
  } finally {
    dispose(context.root);
  }
});

test("SSR: api, html, ssr, and mixed facets stay distinct; tsx alone is not SSR", async () => {
  const context = await scanHonoFixture("ssr", "ssr");
  try {
    const classifications = recordsOfRelation(context, "returns-response")
      .filter((row) => row.note === "route-classification");
    const byPath = new Map(classifications.map((row) => [row.path, row]));
    assert.equal(byPath.get("/api/ping").facet, "api");
    assert.equal(byPath.get("/page").facet, "ssr");
    assert.equal(byPath.get("/render").facet, "ssr");
    assert.equal(byPath.get("/mixed").facet, "mixed");
    // A .tsx file whose handler returns JSON only is API, not SSR.
    assert.equal(byPath.get("/tsx-but-api").facet, "api");
    // The plain-string c.html route is HTML, not necessarily SSR (http fixture covers html).
  } finally {
    dispose(context.root);
  }
});
