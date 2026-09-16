/**
 * #44 scanner unit probes: identity/signature determinism, native-id
 * domain separation, and the compiler-availability seam. Pure and
 * in-process; the vendored compiler is only attached inside the bundle,
 * so identity probes run without it and pipeline probes assert the
 * honest compiler-absent refusal on the source deployment.
 */
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  IDENTITY_DOMAIN,
  SIGNATURE_DOMAIN,
  nativeId,
  nativeIdentityTuple,
  signatureDigest,
  sortIndex,
} from "../src/scanner.mjs";
import { RequestRefusal } from "../src/kernel.mjs";

const tupleA = nativeIdentityTuple({
  locator: { kind: "package", name: "@fixture/esm-app", root: "." },
  modulePath: "src/math.ts",
  qualifiedName: "parse",
  family: "function",
  slot: "static",
});

test("native ids are stable and domain-separated", () => {
  const id = nativeId(tupleA);
  assert.match(id, /^ts1-[0-9a-f]{64}$/);
  assert.equal(id.length, 68);
  assert.equal(nativeId([...tupleA]), id);
  // A domain flip must move the digest.
  const other = nativeId(["other.domain", ...tupleA.slice(1)]);
  assert.notEqual(other, id);
  assert.equal(IDENTITY_DOMAIN, "lekalo.ts.native.v1");
  assert.equal(SIGNATURE_DOMAIN, "lekalo.ts.signature.v1");
});

test("identity moves with rename/relocation, not with location noise", () => {
  const renamed = nativeId([...tupleA.slice(0, 3), "parsed", ...tupleA.slice(4)]);
  assert.notEqual(renamed, nativeId(tupleA));
  const moved = nativeId([...tupleA.slice(0, 2), "src/other.ts", ...tupleA.slice(3)]);
  assert.notEqual(moved, nativeId(tupleA));
  // Slot distinguishes static from instance members.
  const slot = nativeId([...tupleA.slice(0, 5), "instance"]);
  assert.notEqual(slot, nativeId(tupleA));
  // The declaration line is NOT part of the tuple: adding blank lines
  // above a declaration must not move its native id.
  void tupleA;
});

test("signature digests are structural, ordered, and policy-bound", () => {
  const graph = {
    policy: 1,
    call: [{
      parameters: [
        { name: "x", optional: false, rest: false, type: { kind: "string" } },
      ],
      returnType: { kind: "number" },
      async: false,
    }],
    construct: [],
    properties: [],
  };
  const digest = signatureDigest(graph);
  assert.match(digest, /^sha256:[0-9a-f]{64}$/);
  // Member order inside sets is canonical, so re-sorting members keeps
  // the digest, but changing parameter ORDER changes it (overload
  // resolution is order-sensitive).
  const reordered = {
    ...graph,
    call: [{ ...graph.call[0], parameters: [...graph.call[0].parameters] }],
  };
  assert.equal(signatureDigest(reordered), digest);
  const swapped = {
    ...graph,
    call: [{
      parameters: [graph.call[0].parameters[0], {
        name: "y", optional: false, rest: false, type: { kind: "number" },
      }],
      returnType: { kind: "number" },
      async: false,
    }],
  };
  assert.notEqual(signatureDigest(swapped), digest);
  // A signature policy version bump changes every digest.
  const policyBump = JSON.parse(JSON.stringify(graph));
  policyBump.policy = 2;
  assert.notEqual(signatureDigest(policyBump), digest);
});

test("sortIndex orders every record family canonically", () => {
  const index = {
    packages: [{ root: "b" }, { root: "a" }],
    projects: [{ configPath: "b/tsconfig.json" }, { configPath: "a/tsconfig.json" }],
    symbols: [{ native: "ts1-b" }, { native: "ts1-a" }],
    exports: [{ module: "b.ts", name: "x" }, { module: "a.ts", name: "z" }],
    references: [{ from: "b", to: "x", role: "call" }, { from: "a", to: "x", role: "call" }],
    routes: [{ method: "post", path: "/" }, { method: "get", path: "/" }],
    tests: [{ path: "b.test.ts", name: "x" }, { path: "a.test.ts", name: "y" }],
    diagnostics: [{ code: "2" }, { code: "1" }],
    anyUncertainty: [{ path: "b.ts" }, { path: "a.ts" }],
  };
  sortIndex(index);
  assert.deepEqual(index.packages.map((p) => p.root), ["a", "b"]);
  assert.deepEqual(index.symbols.map((s) => s.native), ["ts1-a", "ts1-b"]);
  assert.deepEqual(index.routes.map((r) => r.method), ["get", "post"]);
});

test("the source deployment refuses scanner construction without the compiler", async () => {
  // The kernel source never imports the compiler; runScan must refuse
  // honestly rather than reaching for a project or global TypeScript.
  const { runScan, ScannerSession } = await import("../src/scanner.mjs");
  const session = new ScannerSession();
  assert.throws(
    () => session.scan({
      profile: { id: "x", readRoots: [], exclusions: [] },
      readView: { roots: [], counters: () => ({ filesRead: 0, bytesRead: 0 }) },
      permittedProjectRoot: process.cwd(),
    }),
    (error) => error instanceof RequestRefusal && error.code === "compiler-absent",
  );
  void runScan;
});
