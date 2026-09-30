/**
 * Issue #115 — the Hono framework-evidence envelope: closed vocabularies,
 * record validation, freshness fingerprints, canonical ordering, and the
 * trusted framework policy decode.
 */
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  HONO_CONFIDENCE,
  HONO_PROVENANCE,
  HONO_RELATIONS,
  HONO_RELATION_NAMESPACE,
  HONO_REASONS,
  HONO_RULES_REVISION,
  HONO_SPECIFIERS,
  HONO_STATUS,
  canonicalHonoText,
  fingerprintOf,
  honoCompare,
  makeRecord,
  makeUncertainty,
  sortHonoRecords,
  validateHonoRecords,
} from "../src/hono-evidence.mjs";
import { resolveEndpointContracts } from "../src/hono-bindings.mjs";

const baseRecord = {
  relation: "dev.lekalo.hono/route-handler",
  from: { module: "src/app.ts", native: "ts1-abc", name: "app", indexed: true },
  to: { module: "src/handlers.ts", native: "ts1-def", name: "listUsers", indexed: true },
  method: "GET",
  path: "/users",
  provenance: "detected",
  confidence: "exact",
  status: "complete",
  reasons: [],
  span: { path: "src/app.ts", startLine: 8, startColumn: 1, endLine: 8, endColumn: 24 },
  revision: "fixture-revision-0001",
  adapterVersion: "0.4.0",
};

test("every relation is namespaced adapter evidence, never core vocabulary", () => {
  for (const relation of HONO_RELATIONS) {
    assert.match(relation, /^dev\.lekalo\.hono\//);
  }
  assert.equal(HONO_RELATION_NAMESPACE, "dev.lekalo.hono/");
  // The closed vocabularies carry no canonical core relations.
  for (const forbidden of ["authorizes", "entity", "owns"]) {
    assert.equal(
      HONO_RELATIONS.some((relation) => relation.endsWith(`/${forbidden}`)),
      false,
      `no canonical ${forbidden} relation may exist`,
    );
  }
});

test("provenance never includes confirmed: a scanner cannot mint confirmation", () => {
  assert.deepEqual([...HONO_PROVENANCE], ["explicit", "detected", "inferred"]);
  assert.equal(HONO_PROVENANCE.includes("confirmed"), false);
  assert.deepEqual([...HONO_CONFIDENCE], ["exact", "high", "medium", "low", "unknown"]);
  assert.deepEqual([...HONO_STATUS], ["complete", "incomplete", "unknown"]);
});

test("makeRecord builds a closed record with every issue-required fact", () => {
  const record = makeRecord(baseRecord);
  assert.equal(record.relation, "dev.lekalo.hono/route-handler");
  assert.equal(record.from.native, "ts1-abc");
  assert.equal(record.to.name, "listUsers");
  assert.equal(record.span.startLine, 8);
  assert.equal(record.revision, "fixture-revision-0001");
  assert.equal(record.provenance, "detected");
  assert.equal(record.confidence, "exact");
  assert.equal(record.adapterVersion, "0.4.0");
  assert.equal(record.framework.name, "hono");
  assert.match(record.fingerprint, /^sha256:[0-9a-f]{64}$/);
  assert.equal(record.rulesRevision, HONO_RULES_REVISION);
});

test("makeRecord refuses closed-vocabulary violations", () => {
  assert.throws(() => makeRecord({ ...baseRecord, relation: "canonical.authorizes" }));
  assert.throws(() => makeRecord({ ...baseRecord, provenance: "confirmed" }));
  assert.throws(() => makeRecord({ ...baseRecord, confidence: "0.93" }));
  assert.throws(() => makeRecord({ ...baseRecord, status: "guessed" }));
  assert.throws(() => makeRecord({ ...baseRecord, reasons: ["made-up-reason"] }));
  assert.throws(() => makeRecord({ ...baseRecord, facet: "widget" }));
  assert.throws(() => makeRecord({ ...baseRecord, role: "boss" }));
});

test("fingerprints are stable for identical facts and sensitive to edits", () => {
  const first = makeRecord(baseRecord);
  const second = makeRecord(baseRecord);
  assert.equal(first.fingerprint, second.fingerprint);
  // Path edit
  const movedPath = makeRecord({ ...baseRecord, path: "/users/:id" });
  // Handler signature edit
  const editedHandler = makeRecord({
    ...baseRecord,
    to: { ...baseRecord.to, signature: "sha256:" + "1".repeat(64) },
  });
  // Source body digest edit
  const editedBody = makeRecord({
    ...baseRecord,
    to: { ...baseRecord.to, digest: "sha256:" + "2".repeat(64) },
  });
  // Ordinal (middleware order) edit
  const reordered = makeRecord({ ...baseRecord, ordinal: 2 });
  // Span edit
  const movedSpan = makeRecord({
    ...baseRecord,
    span: { ...baseRecord.span, startLine: 9 },
  });
  const distinct = new Set([
    first.fingerprint, movedPath.fingerprint, editedHandler.fingerprint,
    editedBody.fingerprint, reordered.fingerprint, movedSpan.fingerprint,
  ]);
  assert.equal(distinct.size, 6, "every dependency change must invalidate the fingerprint");
});

test("fingerprintOf is domain-separated and deterministic", () => {
  const value = { a: 1 };
  assert.equal(fingerprintOf(value), fingerprintOf({ a: 1 }));
  assert.notEqual(fingerprintOf(value), fingerprintOf({ a: 2 }));
  assert.match(fingerprintOf(value), /^sha256:[0-9a-f]{64}$/);
});

test("records sort canonically and identically across runs", () => {
  const records = [
    makeRecord({ ...baseRecord, path: "/b" }),
    makeRecord({ ...baseRecord, path: "/a" }),
    makeRecord({ ...baseRecord, method: "POST", path: "/a" }),
  ];
  const once = sortHonoRecords([...records].sort((left, right) => honoCompare(right, left)));
  const twice = sortHonoRecords([...records].sort((left, right) => honoCompare(right, left)));
  assert.deepEqual(once.map((record) => record.path), ["/a", "/a", "/b"]);
  assert.equal(canonicalHonoText(once), canonicalHonoText(twice));
});

test("validateHonoRecords accepts a well-formed set and rejects tampering", () => {
  const record = makeRecord(baseRecord);
  assert.deepEqual(validateHonoRecords([record]), []);
  const tampered = { ...record, fingerprint: "sha256:" + "0".repeat(64) };
  const violations = validateHonoRecords([tampered]);
  assert.equal(violations.length, 1);
  assert.equal(violations[0].code, "fingerprint-mismatch");
  const broken = { ...record, relation: "canonical.owns", fingerprint: record.fingerprint };
  assert.equal(validateHonoRecords([broken]).length, 1);
  const duplicate = validateHonoRecords([record, { ...record }]);
  assert.equal(duplicate.filter((violation) => violation.code === "duplicate-record").length, 1);
});

test("uncertainty rows carry the hono prefix and bounded detail", () => {
  const row = makeUncertainty("src/app.ts", "dynamic-path", "x".repeat(200), 12);
  assert.equal(row.kind, "hono-dynamic-path");
  assert.equal(row.detail.length, 128);
  assert.equal(row.line, 12);
});

test("the closed reason vocabulary covers the uncertainty classes in use", () => {
  for (const reason of ["dynamic-path", "post-mount-registration", "cross-module-registration-order",
    "composition-cycle", "conditional-applicability", "ambiguous-endpoint-join", "ssr-api-conflict",
    "no-response-evidence", "missing-endpoint-join"]) {
    assert.ok(HONO_REASONS.includes(reason), `missing reason ${reason}`);
  }
});

test("the recognized Hono specifier vocabulary is the closed pin set", () => {
  assert.deepEqual([...HONO_SPECIFIERS], [
    "hono", "hono/validator", "hono/http-exception", "hono/testing",
    "@hono/zod-openapi", "@hono/zod-validator",
  ]);
});

test("endpoint contract parsing is bounded, strict, and rejects malformed data", () => {
  const good = resolveEndpointContracts(
    () => JSON.stringify({ endpoints: [
      { id: "users.list", method: "get", path: "/users" },
      { id: "users.create", method: "POST", path: "/users" },
      { bad: true },
    ] }),
    ["lekalo/endpoints.json"],
  );
  assert.equal(good.length, 2);
  assert.equal(good[0].method, "GET");
  // Malformed JSON contributes nothing.
  assert.deepEqual(resolveEndpointContracts(() => "{not json", ["lekalo/endpoints.json"]), []);
  // Missing reader: no contracts, no fabrication.
  assert.deepEqual(resolveEndpointContracts(undefined, ["lekalo/endpoints.json"]), []);
});
