/**
 * #45 zod emitter suite: deterministic text emission, golden byte
 * comparisons against the committed matrix fixture, and the runtime
 * execution of the generated output (acceptance criteria 5 and 6). Node
 * built-ins only at suite level; the emitted modules are executed after a
 * type-only transform (the `export type` lines are erased, exactly what a
 * TS-to-JS transform drops) and resolve the pinned zod package through
 * plain node_modules resolution in a hermetic temp root.
 */
import assert from "node:assert/strict";
import {
  copyFileSync,
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { pathToFileURL, fileURLToPath } from "node:url";
import { test } from "node:test";

import { canonicalJson, emitFiles, orderDeclarations, sha256 } from "../src/zod-emit.mjs";
import { mapProject } from "../src/zod-map.mjs";

const fileURL = import.meta.url;
const repoRoot = resolve(fileURLToPath(fileURL), "..", "..", "..", "..");
const ADAPTER_ROOT = join(repoRoot, "adapters", "node-typescript");
const FIXTURE_DIR = join(repoRoot, "tests", "fixtures", "ir", "valid-zod-matrix");

const CONTEXT = {
  inputDigest:
    "sha256:1111111111111111111111111111111111111111111111111111111111111111",
  adapterVersion: "0.4.0",
  irIdentity: "dev.lekalo.ir@0.2.16",
};

function matrixIr() {
  return JSON.parse(readFileSync(join(FIXTURE_DIR, "ir.json"), "utf8"));
}

function matrixFiles(context = CONTEXT) {
  return emitFiles({ modules: mapProject(matrixIr()).modules, ...context });
}

// ---------------------------------------------------------------------------
// Pure emission properties.
// ---------------------------------------------------------------------------

test("emitted text has LF endings, no trailing whitespace, one final newline", () => {
  for (const file of matrixFiles()) {
    assert.ok(!file.text.includes("\r"), `${file.path} carries CR`);
    assert.ok(
      !file.text.split("\n").some((line) => /\s$/.test(line)),
      `${file.path} has trailing whitespace`,
    );
    assert.ok(file.text.endsWith("\n"), `${file.path} misses the final newline`);
    assert.ok(!file.text.endsWith("\n\n"), `${file.path} ends in a blank line`);
  }
});

test("header carries identity and the input digest, never paths or timestamps", () => {
  for (const file of matrixFiles()) {
    // Only per-module files carry the full header; the runtime and the
    // barrel have their own fixed comments.
    if (!/zod\/[a-z][a-z0-9_]*\.ts$/.test(file.path)
      || file.path.endsWith("runtime.ts")
      || file.path.endsWith("index.ts")) {
      continue;
    }
    assert.ok(file.text.includes(CONTEXT.adapterVersion), file.path);
    assert.ok(file.text.includes(CONTEXT.irIdentity), file.path);
    assert.ok(file.text.includes(CONTEXT.inputDigest), file.path);
    assert.ok(!/[A-Za-z]:[\\/]/.test(file.text), `${file.path} leaks a host path`);
    assert.ok(!/\d{4}-\d{2}-\d{2}T/.test(file.text), `${file.path} leaks a timestamp`);
  }
});

test("declarations emit in topological order with semantic-id tie-breaks", () => {
  const declarations = [
    { semanticId: "m.b", module: "m", exportName: "BSchema", typeName: "B", kind: "alias", expr: { k: "ref", name: "ASchema", module: "m" } },
    { semanticId: "m.a", module: "m", exportName: "ASchema", typeName: "A", kind: "scalar", expr: { k: "string" } },
    { semanticId: "m.c", module: "m", exportName: "CSchema", typeName: "C", kind: "alias", expr: { k: "array", item: { k: "ref", name: "BSchema", module: "m" } } },
  ];
  const ordered = orderDeclarations(declarations);
  assert.deepEqual(
    ordered.map((declaration) => declaration.exportName),
    ["ASchema", "BSchema", "CSchema"],
  );
});

test("object rendering nests with two-space indentation and closes strictly", () => {
  const ir = {
    contract: "dev.lekalo.ir@0.2.16",
    definitions: [
      { id: "m.s", kind: "scalar", base: "string" },
      { id: "m.inner", kind: "value-object", fields: [{ name: "x", type: { ref: "m.s" }, required: true }] },
      {
        id: "m.outer",
        kind: "value-object",
        fields: [{ name: "inner", type: { ref: "m.inner" }, required: true }],
      },
    ],
  };
  const files = emitFiles({ modules: mapProject(ir).modules, ...CONTEXT });
  const moduleFile = files.find((file) => file.path.endsWith("/m.ts"));
  assert.ok(
    moduleFile.text.includes(
      [
        `export const MOuterSchema = z.object({`,
        `  "inner": MInnerSchema,`,
        `}).strict();`,
      ].join("\n"),
    ),
    moduleFile.text,
  );
  // Named refs resolve topologically before use; nesting is expressed by
  // the referenced schema, not inlined.
  const order = moduleFile.text.indexOf("export const MInnerSchema =");
  const use = moduleFile.text.indexOf("MInnerSchema,");
  assert.ok(order > 0 && use > order, "dependency declared before use");
});

// ---------------------------------------------------------------------------
// Golden byte comparison against the committed matrix fixture (AC-6).
// ---------------------------------------------------------------------------

test("the matrix fixture emits byte-identical goldens and a stable double run", () => {
  const irBytes = readFileSync(join(FIXTURE_DIR, "ir.json"));
  const context = { ...CONTEXT, inputDigest: sha256(irBytes.toString("utf8")) };
  const first = matrixFiles(context);
  const second = matrixFiles(context);
  for (let index = 0; index < first.length; index += 1) {
    assert.equal(first[index].text, second[index].text, "repeat emission drifts");
  }
  const expectedDir = join(FIXTURE_DIR, "expected");
  for (const file of first) {
    const goldenPath = join(expectedDir, file.path.split("/").pop());
    assert.ok(existsSync(goldenPath), `missing golden ${file.path}`);
    assert.equal(
      file.text,
      readFileSync(goldenPath, "utf8"),
      `golden drift in ${file.path}`,
    );
  }
  assert.deepEqual(
    first.map((file) => file.path.split("/").pop()).sort(),
    ["alpha.map.json", "alpha.ts", "index.ts", "runtime.ts"],
  );
});

// ---------------------------------------------------------------------------
// Sidecar map integrity (AC-7 evidence base).
// ---------------------------------------------------------------------------

test("sidecars are canonical JSON with in-bounds declaration ranges", () => {
  const irBytes = readFileSync(join(FIXTURE_DIR, "ir.json"));
  const files = matrixFiles({ ...CONTEXT, inputDigest: sha256(irBytes.toString("utf8")) });
  for (const sidecar of files.filter((file) => file.map)) {
    assert.equal(
      sidecar.text,
      `${canonicalJson(sidecar.map)}\n`,
      `${sidecar.path} is not canonical`,
    );
    const moduleFile = files.find(
      (file) => file.path === sidecar.path.replace(".map.json", ".ts"),
    );
    const moduleBytes = byteLength(moduleFile.text);
    let previous = -1;
    for (const declaration of sidecar.map.declarations) {
      assert.ok(declaration.start >= previous, "declaration ranges overlap or regress");
      assert.ok(declaration.end <= moduleBytes, "range escapes the file");
      assert.ok(declaration.end > declaration.start, "empty range");
      const slice = moduleFile.text.slice(declaration.start, declaration.end);
      assert.ok(
        slice.includes(`export const ${declaration.export} =`),
        `range of ${declaration.export} does not cover its declaration`,
      );
      previous = declaration.end;
    }
  }
});

function byteLength(text) {
  return Buffer.byteLength(text, "utf8");
}

test("merged groups attribute colliding paths first-wins and owner agrees with the root entry", () => {
  // alpha + beta merge into one emission group (beta references alpha),
  // and both modules carry colliding leaf names.
  const ir = {
    contract: "dev.lekalo.ir@0.2.16",
    modelVersion: "0.2.16",
    definitions: [
      { id: "alpha.text", kind: "scalar", base: "string" },
      { id: "alpha.aa_row", kind: "value-object", fields: [{ name: "title", type: { ref: "alpha.text" }, required: true }] },
      { id: "alpha.task", kind: "value-object", fields: [{ name: "title", type: { ref: "alpha.text" }, required: true }] },
      { id: "beta.report", kind: "value-object", fields: [{ name: "title", type: { ref: "alpha.text" }, required: true }] },
      { id: "beta.bridge", kind: "value-object", fields: [{ name: "row", type: { ref: "alpha.aa_row" }, required: true }] },
    ],
    modules: [
      { id: "alpha", kind: "module", version: 1 },
      { id: "beta", kind: "module", version: 1, imports: ["alpha"] },
    ],
  };
  const files = emitFiles({ modules: mapProject(ir).modules, ...CONTEXT });
  const sidecar = files.find((file) => file.path.endsWith("/alpha.map.json"));
  const map = sidecar.map;
  // One merged group: the group id names the head module, but the owner
  // is the mapped root symbol (fields[""]), not blindly the head id.
  const smallestObject = "alpha.aa_row"; // byte-order-first object root
  assert.equal(map.fields[""], smallestObject);
  assert.equal(map.owner, smallestObject);
  // Colliding bare leaves keep their first deterministic owner
  // (alpha.aa_row wins byte order over alpha.task and beta.report).
  assert.equal(map.fields["title"], smallestObject);
});

// ---------------------------------------------------------------------------
// Runtime execution of the generated output (AC-5).
// ---------------------------------------------------------------------------

test("generated modules parse, validate, and attribute issues to semantic ids", async () => {
  const dir = materialize();
  try {
    const index = await import(
      pathToFileURL(join(dir, "src", "generated", "node-typescript", "zod", "index.js")).href
    );
    // AC-3 runtime proof: the four presence × nullability combinations
    // behave exactly as mapped.
    const ok = index.AlphaMatrixSchema.safeParse({
      plain: "x",
      nullable: null,
      optionalKey: "y",
    });
    assert.equal(ok.success, true, JSON.stringify(ok.error?.issues ?? []));
    // required + nullable wrapper: the key is still mandatory.
    const missingNullable = index.AlphaMatrixSchema.safeParse({ plain: "x" });
    assert.equal(missingNullable.success, false);
    // optional + nullable wrapper accepts undefined and explicit null.
    const bothAbsent = index.AlphaMatrixSchema.safeParse({
      plain: "x",
      nullable: "v",
      optionalKey: "y",
      optionalNullable: null,
    });
    assert.equal(bothAbsent.success, true, JSON.stringify(bothAbsent.error?.issues ?? []));
    // Closed objects: unknown keys are refused (strict default policy).
    const unknownKey = index.AlphaMatrixSchema.safeParse({
      plain: "x",
      nullable: "v",
      ghost: 1,
    });
    assert.equal(unknownKey.success, false);
    // AC-7: field-level errors attribute to Lekalo semantic ids through
    // the sidecar map.
    const bad = index.AlphaMatrixSchema.safeParse({ plain: 42, nullable: "v" });
    assert.equal(bad.success, false);
    const sidecar = JSON.parse(
      readFileSync(
        join(dir, "src", "generated", "node-typescript", "zod", "alpha.map.json"),
        "utf8",
      ),
    );
    const normalized = index.normalizeIssues(
      bad.error.issues,
      sidecar.fields,
      sidecar.owner,
    );
    assert.ok(normalized.length > 0);
    assert.equal(normalized[0].path, "plain");
    assert.equal(normalized[0].semanticId, "alpha.matrix");
    // Branding: the branded identity scalar refuses a non-uuid raw string.
    assert.equal(index.AlphaTaskIdSchema.safeParse("not-a-uuid").success, false);
    const parsed = index.AlphaTaskIdSchema.parse(
      "3f2bd13e-8b6d-4d8a-9ba7-2f4a5f4b9c10",
    );
    assert.equal(typeof parsed, "string");
  // F-1/F-5: a string-base identity scalar keeps its base validation —
  // plain strings parse (a slug is a slug), non-strings refuse, and
  // branding only seals the type.
  assert.equal(index.AlphaTagSchema.safeParse("acme-7").success, true);
  assert.equal(index.AlphaTagSchema.safeParse(42).success, false);
  // F-1 end to end: a BRANDED string-base identity scalar keeps its base
  // validation too (the exact review repro — branding never narrows a
  // non-uuid identity to uuid).
  assert.equal(index.AlphaSlugSchema.safeParse("acme-7").success, true);
  assert.equal(index.AlphaSlugSchema.safeParse(42).success, false);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

/**
 * Materialize a runnable hermetic project: the emitted files plus a
 * recursive copy of the pinned zod package (JS files only) under a temp
 * node_modules, so plain node_modules resolution (never NODE_PATH, never
 * a package manager) resolves the generated `import * as z from "zod"`.
 */
function materialize(keepTypeScript = false) {
  const dir = mkdtempSync(join(tmpdir(), "lekalo-zod-emit-"));
  const zodDir = join(dir, "src", "generated", "node-typescript", "zod");
  mkdirSync(zodDir, { recursive: true });
  const files = matrixFiles({ ...CONTEXT, inputDigest: sha256(readFileSync(join(FIXTURE_DIR, "ir.json")).toString("utf8")) });
  for (const file of files) {
    const target = join(
      dir,
      keepTypeScript ? file.path : file.path.replace(/\.ts$/, ".js"),
    );
    mkdirSync(dirname(target), { recursive: true });
    // The runnable copy erases type-only lines; the typecheck copy is
    // the exact emitted TypeScript bytes.
    writeFileSync(target, keepTypeScript ? file.text : toRunnable(file.text));
  }
  copyPackage(join(ADAPTER_ROOT, "node_modules", "zod"), join(dir, "node_modules", "zod"));
  return dir;
}

/** Recursively copy JavaScript package files, skipping declarations/docs. */
function copyPackage(fromRoot, toRoot) {
  for (const entry of readdirSync(fromRoot)) {
    const from = join(fromRoot, entry);
    if (lstatSync(from).isDirectory()) {
      copyPackage(from, join(toRoot, entry));
      continue;
    }
    if (/\.(d\.ts|d\.cts)$/.test(entry) || entry === "README.md") continue;
    mkdirSync(toRoot, { recursive: true });
    copyFileSync(from, join(toRoot, entry));
  }
}

/**
 * Erase type-only lines (the exact transform a TS-to-JS pipeline applies
 * to these files: every line is plain JS except `export type ...`) and
 * give relative import specifiers their `.js` extension, which Node ESM
 * resolution requires of the runnable copies. The committed `.ts` goldens
 * stay extensionless — the TypeScript convention consumer toolchains
 * resolve themselves.
 */
function toRunnable(text) {
  return text
    .split("\n")
    .filter((line) => !line.startsWith("export type "))
    .map((line) =>
      line
        .replace(
          /^(import \{[^}]*\} from "(\.\/[a-z0-9_]+))";$/,
          (_match, head, specifier) => `${head}.js";`,
        )
        .replace(
          /^(export \* from "(\.\/[a-z0-9_]+))";$/,
          (_match, head, specifier) => `${head}.js";`,
        ),
    )
    .join("\n");
}

test("normalizeIssues attributes nested paths through the sidecar map", async () => {
  const dir = materialize();
  try {
    const index = await import(
      pathToFileURL(join(dir, "src", "generated", "node-typescript", "zod", "index.js")).href
    );
    const sidecar = JSON.parse(
      readFileSync(
        join(dir, "src", "generated", "node-typescript", "zod", "alpha.map.json"),
        "utf8",
      ),
    );
    // A nested array-element error path resolves to the owning symbol of
    // the flattened leaf (deep.0.0 → alpha.deep).
    const normalized = index.normalizeIssues(
      [{ path: ["deep", 0, 0], code: "invalid_type" }],
      sidecar.fields,
      sidecar.owner,
    );
    assert.deepEqual(normalized, [
      { path: "deep.0.0", semanticId: "alpha.deep", code: "invalid_type" },
    ]);
    // A nested unknown leaf resolves to its closest enclosing mapped
    // path (the closest-enclosing-path fallback).
    const enclosing = index.normalizeIssues(
      [{ path: ["deep", 5, 9, "beyond"], code: "invalid_type" }],
      sidecar.fields,
      sidecar.owner,
    );
    assert.equal(enclosing[0].semanticId, "alpha.deep");
    // A completely unknown path falls back to the module owner.
    const fallback = index.normalizeIssues(
      [{ path: ["ghost", "deep", "x"], code: "unrecognized_keys" }],
      sidecar.fields,
      sidecar.owner,
    );
    assert.equal(fallback[0].semanticId, sidecar.owner);
    // The object root ("" path) attributes to the module's root symbol.
    const root = index.normalizeIssues(
      [{ path: [], code: "invalid_type" }],
      sidecar.fields,
      sidecar.owner,
    );
    assert.ok(root[0].semanticId.length > 0);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

// ---------------------------------------------------------------------------
// Typecheck of the generated TypeScript (AC-5, typecheck half).
// ---------------------------------------------------------------------------

test("generated modules typecheck cleanly under the pinned typescript", async () => {
  // The exact compiler pin the adapter bundles, resolved from the
  // adapter's build-time node_modules — never ambient resolution.
  const { createRequire } = await import("node:module");
  const require = createRequire(join(ADAPTER_ROOT, "package.json"));
  const ts = require("typescript/lib/typescript.js");
  assert.equal(ts.version, "5.9.3", "compiler pin drift");
  const dir = materialize(/* keepTypeScript */ true);
  try {
    const zodDir = join(dir, "src", "generated", "node-typescript", "zod");
    // The hermetic temp root carries only the trimmed JavaScript copy of
    // zod (the runtime suites strip declarations on copy), so the
    // typecheck resolves zod through paths to the declaration entry of
    // the exact same pin inside the adapter's build-time node_modules.
    const program = ts.createProgram(
      ["alpha.ts", "runtime.ts", "index.ts"].map((name) => join(zodDir, name)),
      {
        noEmit: true,
        strict: true,
        noImplicitAny: false, // the runtime helper is JSDoc-typed plain JS by contract
        target: ts.ScriptTarget.ES2022,
        module: ts.ModuleKind.ESNext,
        moduleResolution: ts.ModuleResolutionKind.Bundler,
        skipLibCheck: true,
        types: [],
        baseUrl: dir,
        paths: {
          zod: [join(ADAPTER_ROOT, "node_modules", "zod", "index.d.ts")],
        },
      },
      ts.createCompilerHost({}, /*setParentNodes*/ true),
    );
    const diagnostics = ts.getPreEmitDiagnostics(program).filter(
      (diagnostic) => diagnostic.category === ts.DiagnosticCategory.Error,
    );
    const report = ts.formatDiagnostics(diagnostics, {
      getCurrentDirectory: () => dir,
      getCanonicalFileName: (name) => name,
      getNewLine: () => "\n",
    });
    assert.equal(diagnostics.length, 0, report);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
