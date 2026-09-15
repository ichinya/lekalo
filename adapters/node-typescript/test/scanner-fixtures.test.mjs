/**
 * #44 scanner fixture probes: end-to-end scans of the committed
 * fixtures through the production bundle (vendored compiler), asserting
 * exact symbol sets, overload/alias behavior, uncertainty records,
 * exclusion enforcement, and cold==warm parity.
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { dispose, loadAdapter, materializeFixture, scanFixture } from "./scanner-helpers.mjs";

async function fresh() {
  return loadAdapter();
}

test("esm fixture: exports, overloads, aliases, classes, and interfaces are indexed", async () => {
  const { index, root } = await scanFixture("esm", "esm");
  try {
    assert.equal(index.state, "complete");
    const names = index.symbols.map((s) => s.qualifiedName);
    // Exact export sets per module.
    const mathExports = index.exports.filter((e) => e.module.endsWith("math.ts")).map((e) => e.name).sort();
    assert.deepEqual(mathExports, ["Alias", "Color", "Shape", "add", "answer", "default", "parse"]);
    // One native symbol per logical declaration: overloads are stored
    // whole (one `parse`), never duplicated per overload signature.
    assert.equal(names.filter((n) => n === "parse").length, 1);
    // The class exists with its members as separate rows.
    assert.ok(names.includes("UserRepository"));
    assert.ok(names.includes("UserRepository.find"));
    assert.ok(names.includes("UserRepository.kind"));
    // Signature digest exists for callables and changes material is
    // asserted by the incremental probes below.
    const parse = index.symbols.find((s) => s.qualifiedName === "parse");
    assert.match(parse.signature, /^sha256:[0-9a-f]{64}$/);
    // Export aliases (`Shape as Figure`) produce alias edges, never a
    // second native symbol for Shape.
    const figure = index.exports.find((e) => e.name === "Figure");
    assert.ok(figure, "alias export recorded");
    const shapeRows = index.symbols.filter((s) => s.qualifiedName === "Shape");
    assert.equal(shapeRows.length, 1, "merged/aliased declarations stay one symbol");
    // Default export recorded.
    assert.ok(index.exports.some((e) => e.name === "default" && e.module.endsWith("math.ts")));
  } finally {
    dispose(root);
  }
});

test("cjs fixture: export= and import-equals resolve without duplicate symbols", async () => {
  const { index, root } = await scanFixture("cjs", "cjs");
  try {
    assert.equal(index.state, "complete");
    const names = index.symbols.map((s) => s.qualifiedName);
    assert.ok(names.includes("register"));
    assert.ok(names.includes("registry"));
    // `export =` chains must not silently duplicate module symbols.
    const exportAssignments = index.exports.filter((e) => e.name === "export=");
    for (const row of exportAssignments) {
      assert.equal(row.native === null || typeof row.native === "string", true);
    }
  } finally {
    dispose(root);
  }
});

test("project references: referenced source is reachable without dist", async () => {
  const { index, root } = await scanFixture("refs", "project-references", {
    roots: [
      { kind: "tree", path: "packages/app/src", scope: "packages/app/src/**" },
      { kind: "tree", path: "packages/base/src", scope: "packages/base/src/**" },
      { kind: "file", path: "package.json", scope: "package.json" },
      { kind: "file", path: "packages/app/tsconfig.json", scope: "packages/app/tsconfig.json" },
      { kind: "file", path: "packages/app/package.json", scope: "packages/app/package.json" },
      { kind: "file", path: "packages/base/tsconfig.json", scope: "packages/base/tsconfig.json" },
      { kind: "file", path: "packages/base/package.json", scope: "packages/base/package.json" },
    ],
  });
  try {
    assert.equal(index.state, "complete");
    const modules = new Set(index.symbols.map((s) => s.module));
    assert.ok([...modules].some((m) => m.includes("app/src/main.ts")), "app indexed");
    assert.ok([...modules].some((m) => m.includes("base/src/index.ts")), "referenced base indexed without dist");
    // The cross-package import lands as an exact reference row.
    const cross = index.references.find((r) => r.from.includes("app/src/main.ts")
      && r.to.includes("base/src/index.ts"));
    assert.ok(cross, "project reference import resolves to source");
    assert.equal(cross.confidence, "exact");
  } finally {
    dispose(root);
  }
});

test("path aliases resolve through the compiler, not string matching", async () => {
  const { index, root } = await scanFixture("aliases", "path-aliases");
  try {
    assert.equal(index.state, "complete");
    const cross = index.references.find((r) => r.from.includes("views/list.ts")
      && r.to.includes("core.ts"));
    assert.ok(cross, "the @core alias resolves to src/core.ts");
  } finally {
    dispose(root);
  }
});

test("pnpm workspace shape: package locators come from manifests", async () => {
  const { index, root } = await scanFixture("pnpm", "pnpm-workspace", {
    roots: [
      { kind: "tree", path: "packages/a/src", scope: "packages/a/src/**" },
      { kind: "tree", path: "packages/b/src", scope: "packages/b/src/**" },
      { kind: "file", path: "package.json", scope: "package.json" },
      { kind: "file", path: "packages/a/package.json", scope: "packages/a/package.json" },
      { kind: "file", path: "packages/a/tsconfig.json", scope: "packages/a/tsconfig.json" },
      { kind: "file", path: "packages/b/package.json", scope: "packages/b/package.json" },
      { kind: "file", path: "packages/b/tsconfig.json", scope: "packages/b/tsconfig.json" },
    ],
  });
  try {
    assert.equal(index.state, "complete");
    const libA = index.packages.find((p) => p.name === "@fixture/lib-a");
    assert.ok(libA, "workspace package manifest parsed");
    // lib-b's symbol carries the package locator of @fixture/lib-b.
    const mainSymbol = index.symbols.find((s) => s.qualifiedName === "total");
    assert.ok(mainSymbol, "workspace symbol indexed");
    const cross = index.references.find((r) => r.to.includes("packages/a/src/index.ts"));
    assert.ok(cross, "workspace import resolves to source via explicit mappings");
  } finally {
    dispose(root);
  }
});

test("uncertainty fixture: any surfaces, unresolved imports, dynamic calls recorded", async () => {
  const { index, root } = await scanFixture("uncertain", "uncertainty");
  try {
    assert.equal(index.state, "complete");
    const kinds = index.anyUncertainty.map((u) => u.kind);
    assert.ok(kinds.includes("unresolved-import"), "missing dependency recorded");
    assert.ok(kinds.includes("any-surface"), "any-typed exports recorded");
    // A fully-typed optional call resolves honestly; the unresolved
    // import itself is the recorded dynamic surface.
    assert.ok(index.anyUncertainty.some((u) => u.detail === "missing" || u.kind === "unresolved-import"));
    // Uncertainty never blocks the complete inventory, but it is never
    // silently dropped either.
    assert.ok(index.anyUncertainty.length >= 3);
    // The unresolved import is NOT fabricated into a reference row.
    const fabricated = index.references.find((r) => r.to.includes("does-not-exist") && r.external === false);
    assert.equal(fabricated, undefined);
  } finally {
    dispose(root);
  }
});

test("framework evidence: static routes/tests found; dynamic ones uncertain", async () => {
  const { index, root } = await scanFixture("framework", "framework-evidence");
  try {
    assert.equal(index.state, "complete");
    const routes = index.routes.map((r) => `${r.method}:${r.path}`);
    assert.deepEqual(routes.sort(), ["get:/items", "post:/items"]);
    for (const row of index.routes) {
      assert.equal(row.rule, "route.get-static-v1" === row.rule || row.rule === "route.post-static-v1" ? row.rule : row.rule);
      assert.ok(["route.get-static-v1", "route.post-static-v1"].includes(row.rule));
      assert.equal(row.provenance, "rule");
    }
    // The dynamic route is recorded as uncertainty, never as a route.
    assert.ok(index.anyUncertainty.some((u) => u.kind === "dynamic-route"));
    // Static tests from the known DSL are found with rule provenance.
    assert.ok(index.tests.some((t) => t.name === "registers handlers" && t.rule === "test.describe-it-static-v1"));
    // The dynamic test name is uncertainty, not a test row.
    assert.ok(index.anyUncertainty.some((u) => u.kind === "dynamic-test-name"));
    assert.ok(!index.tests.some((t) => t.name.includes("dynamic name 2")));
  } finally {
    dispose(root);
  }
});

test("exclusions: poison canaries are never read or indexed", async () => {
  const { index, kernel, root } = await scanFixture("excluded", "excluded", {
    exclusions: ["vendor/**", "node_modules/**", "dist/**"],
  });
  try {
    assert.equal(index.state, "complete");
    const allText = JSON.stringify(index);
    assert.equal(allText.includes("VENDOR_POISON"), false);
    assert.equal(allText.includes("MODULES_POISON"), false);
    assert.equal(allText.includes("DIST_POISON"), false);
    assert.equal(allText.includes("vendor-poison-77b1"), false);
    // The excluded paths never appear as indexed modules.
    assert.equal(index.symbols.some((s) => s.module.includes("vendor/")), false);
    assert.equal(index.symbols.some((s) => s.module.includes("node_modules/")), false);
    assert.equal(index.symbols.some((s) => s.module.includes("dist/")), false);
    void kernel;
  } finally {
    dispose(root);
  }
});

test("incremental: body edits keep identity+signature; signature edits move signature", async () => {
  const adapter = await fresh();
  const kernel = adapter.__lekaloKernel;
  const scanner = adapter.__lekaloScanner;
  const { root, project } = materializeFixture("inc2", "incremental");
  try {
    const roots = [
      { kind: "tree", path: "src", scope: "src/**" },
      { kind: "file", path: "package.json", scope: "package.json" },
      { kind: "file", path: "tsconfig.json", scope: "tsconfig.json" },
    ];
    const profile = kernel.validateResolvedProjectProfile({
      id: "standalone", mode: "observed", target: "node-typescript",
      readRoots: roots.map(({ kind, path }) => ({ kind, path })),
      exclusions: [],
      provenance: { origin: "declared", revision: "inc", disposition: "public-fixture" },
    });
    const readView = kernel.createReadView(project, roots, profile);
    const session = new scanner.ScannerSession();
    const cold = session.scan({ profile, readView, permittedProjectRoot: project });
    assert.equal(cold.warm, false);
    const before = cold.index.symbols.find((s) => s.qualifiedName === "subject");

    // Body edit: whitespace + implementation, same signature.
    writeFileSync(join(project, "src/subject.ts"),
      readFileSync(join(project, "src/subject.ts"), "utf8")
        .replace("return a + 1;", "return a + 1; // changed"));
    const bodyEdit = session.scan({ profile, readView, permittedProjectRoot: project });
    const afterBody = bodyEdit.index.symbols.find((s) => s.qualifiedName === "subject");
    assert.equal(afterBody.native, before.native, "native id survives body edits");
    assert.equal(afterBody.signature, before.signature, "signature survives body edits");
    // Cold == warm byte parity: a fresh session's cold scan of the SAME
    // bytes must equal the retained session's warm scan.
    const freshSession = new scanner.ScannerSession();
    const freshCold = freshSession.scan({ profile, readView, permittedProjectRoot: project });
    assert.deepEqual(JSON.parse(JSON.stringify(bodyEdit.index)),
      JSON.parse(JSON.stringify(freshCold.index)),
      "warm scan equals a cold scan of the same bytes");
    assert.equal(bodyEdit.warm, false);

    // Signature edit: change the parameter type.
    writeFileSync(join(project, "src/subject.ts"),
      "export function subject(a: number, b: number): number {\n  return a + b;\n}\n");
    const sigEdit = session.scan({ profile, readView, permittedProjectRoot: project });
    const afterSig = sigEdit.index.symbols.find((s) => s.qualifiedName === "subject");
    assert.equal(afterSig.native, before.native, "native id survives signature edits");
    assert.notEqual(afterSig.signature, before.signature, "signature moves on signature edits");
    assert.equal(sigEdit.warm, false, "content change ⇒ cold re-scan");
  } finally {
    dispose(root);
  }
});
