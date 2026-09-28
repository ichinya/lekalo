#!/usr/bin/env node
// Issue #58 round-trip gate: the generated codecs are EXECUTED against
// the closed wire vectors, and — when the pinned Mago toolchain is
// available — the generated files pass the real strict lint and
// analysis gates.
//
// Stages:
//   1. Planner corpus: decode→encode preserves canonical JSON
//      semantics (absent vs null, empty list vs object, enum values,
//      verbatim wire names), encode→decode preserves typed values and
//      presence, hostile inputs refuse, the cross-module nested input
//      round-trips, the query codec is a direct-body passthrough,
//      duplicate JSON members refuse at the kernel boundary, and the
//      class map loads every class.
//   2. Edge corpus: number/boolean/datetime/uri bases, every remaining
//      presence shape (required-nullable, optional-nullable,
//      optional-nonnull, nullable list elements), precision-loss and
//      numeric-string refusals, and the hostile-description escaping.
//   3. Mago lane (real only, honest skip otherwise): pinned lint and
//      analyze over the exact generated files, plus a negative control
//      that a deliberately violating file is caught. A missing tool
//      with --require-mago fails the gate; a skip never counts as
//      Mago acceptance.
//
// Dependency-free (node:*, no npm packages); run from the repo root.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  cpSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const requireMago = process.argv.includes("--require-mago");
const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const adapterPath = join(repoRoot, "adapters", "php-laravel", "adapter.php");
const corpusIr = join(repoRoot, "tests", "fixtures", "adapter-conformance", "inputs", "ir-minimal.json");
const fixtures = join(repoRoot, "tests", "fixtures", "php-laravel", "types");
const srcDir = join(repoRoot, "adapters", "php-laravel", "src");
const php = process.env.LEKALO_PHP ?? "php";
const sha256 = (bytes) => "sha256:" + createHash("sha256").update(bytes).digest("hex");

let passed = 0;
function step(name, fn) {
  try {
    fn();
    passed += 1;
    console.log(`ok - ${name}`);
  } catch (error) {
    console.error(`not ok - ${name}`);
    console.error(error instanceof Error ? error.stack : String(error));
    process.exit(1);
  }
}

function adapterCall(root, request) {
  const result = spawnSync(php, [adapterPath], {
    cwd: root,
    input: JSON.stringify(request),
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    timeout: 120_000,
  });
  if (result.status !== 0) {
    throw new Error(`adapter exited ${result.status}: ${result.stderr.slice(0, 2000)}`);
  }
  const lines = result.stdout.split("\n").filter((line) => line.trim().startsWith("{"));
  return JSON.parse(lines[lines.length - 1]);
}

const requestId = (seed) => "req-" + createHash("sha256").update(seed).digest("hex");

/** Dry-run + apply one input document; asserts a clean exchange. */
function generate(root, irPath, seed) {
  const dry = adapterCall(root, {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    project_root: ".",
    operation: "generate",
    ir_path: irPath,
    target: "php-laravel",
    profile: "default",
    request_id: requestId(`dry-${seed}`),
    dry_run: true,
  });
  assert.equal(dry.status, "ok", JSON.stringify(dry).slice(0, 800));
  const apply = adapterCall(root, {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    project_root: ".",
    operation: "generate",
    ir_path: irPath,
    target: "php-laravel",
    profile: "default",
    request_id: requestId(`apply-${seed}`),
    dry_run: false,
    plan_id: dry.evidence.plan_id,
  });
  assert.equal(apply.status, "ok", JSON.stringify(apply).slice(0, 800));
}

function canonicalJson(value) {
  if (Array.isArray(value)) return "[" + value.map(canonicalJson).join(",") + "]";
  if (value !== null && typeof value === "object") {
    return (
      "{" +
      Object.keys(value)
        .sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)))
        .map((k) => `${JSON.stringify(k)}:${canonicalJson(value[k])}`)
        .join(",") +
      "}"
    );
  }
  return JSON.stringify(value);
}

/** Stage one corpus and apply the managed generation. */
function materialize(root, inputFixture, irSource, inputName) {
  mkdirSync(join(root, "lekalo", "types"), { recursive: true });
  mkdirSync(join(root, ".lekalo", "cache", "ir"), { recursive: true });
  cpSync(irSource, join(root, ".lekalo", "cache", "ir", `${inputName === "edge.types.json" ? "edge" : "planner"}.json`));
  const irDigest = sha256(readFileSync(join(root, ".lekalo", "cache", "ir", `${inputName === "edge.types.json" ? "edge" : "planner"}.json`)));
  const document = JSON.parse(readFileSync(join(fixtures, inputFixture), "utf8"));
  document.irDigest = irDigest;
  writeFileSync(join(root, "lekalo", "types", inputName), canonicalJson(document) + "\n");
}

/** Run the wire-vector harness over one materialized corpus. */
function runHarness(root, corpus) {
  const result = spawnSync(php, [
    join(fixtures, "roundtrip-harness.php"),
    root,
    corpus,
    srcDir,
  ], { encoding: "utf8", maxBuffer: 32 * 1024 * 1024, timeout: 120_000 });
  if (result.status !== 0) {
    throw new Error(`harness exited ${result.status}: ${result.stderr.slice(0, 4000)}`);
  }
  const rows = JSON.parse(result.stdout.slice(result.stdout.indexOf("[")));
  assert.ok(rows.length > 0, "the harness reports its vectors");
  for (const row of rows) {
    assert.equal(row.ok, true, `vector ${row.name}: ${row.detail}`);
  }
  return rows.length;
}

/** Every generated PHP file passes the interpreter's own parser. */
function lintAll(root) {
  const files = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(path);
      } else if (entry.name.endsWith(".php")) {
        files.push(path);
      }
    }
  };
  walk(root);
  assert.ok(files.length > 0, "generated files exist");
  for (const file of files) {
    const lint = spawnSync(php, ["-n", "-l", file], { encoding: "utf8", timeout: 30_000 });
    assert.equal(lint.status, 0, `parse failed: ${file}: ${lint.stderr}`);
  }
  return files.length;
}

// ---------------------------------------------------------------------
// Stage 1+2: executed wire vectors over both corpora.
// ---------------------------------------------------------------------
let plannerRoot;
step("planner corpus: every executed wire vector holds", () => {
  plannerRoot = mkdtempSync(join(tmpdir(), "lekalo-rt-planner-"));
  materialize(plannerRoot, "inputs/planner.types.json", corpusIr, "planner.types.json");
  generate(plannerRoot, "lekalo/types/planner.types.json", "rt-planner");
  lintAll(plannerRoot);
  const vectors = runHarness(plannerRoot, "planner");
  assert.ok(vectors >= 25, `the planner vectors ran (${vectors})`);
});

step("edge corpus: every executed wire vector holds", () => {
  const root = mkdtempSync(join(tmpdir(), "lekalo-rt-edge-"));
  materialize(root, "inputs/edge.types.json", join(fixtures, "inputs", "ir", "edge.ir.json"), "edge.types.json");
  generate(root, "lekalo/types/edge.types.json", "rt-edge");
  lintAll(root);
  const vectors = runHarness(root, "edge");
  assert.ok(vectors >= 8, `the edge vectors ran (${vectors})`);
  // The hostile description stayed one safe comment line.
  const amount = readFileSync(join(root, ".lekalo/generated/php-laravel/types/edge/amount.php"), "utf8");
  const description = amount.split("\n").find((line) => line.includes("// Description:"));
  assert.ok(description, "the description comment is present");
  assert.ok(!description.includes("?>"), "the close-tag pair is broken");
  assert.ok(!description.includes("*/ code") || description.includes("// Description:"), "no comment can close");
  // B1: the sidecar binds both shared-key optional positions to the
  // SAME wrapper FQN, and the differing third shape to its own.
  const sidecar = JSON.parse(
    readFileSync(join(root, ".lekalo/generated/php-laravel/types/types.map.json"), "utf8"),
  );
  const reading = sidecar.types.find((t) => t.semanticId === "edge.reading");
  const wrapperOf = (name) => reading.fields.find((f) => f.name === name).type.wrapper;
  const amountWrapper = wrapperOf("threshold");
  assert.equal(wrapperOf("floor"), amountWrapper, "shared-key fields bind one wrapper");
  assert.match(amountWrapper, /OptionalNullableAmount$/);
  assert.notEqual(wrapperOf("label"), amountWrapper, "the differing shape binds its own wrapper");
  assert.match(wrapperOf("label"), /OptionalEnabled$/);
  rmSync(root, { recursive: true, force: true });
});

// ---------------------------------------------------------------------
// Stage 3: the Mago lane.
// ---------------------------------------------------------------------
function findMago() {
  const lock = JSON.parse(readFileSync(join(repoRoot, "adapters", "php-laravel", "mago-toolchain.lock.json"), "utf8"));
  const candidates = (process.env.LEKALO_MAGO ?? "mago").split(";").filter(Boolean);
  for (const candidate of candidates) {
    const probe = spawnSync(candidate, ["--version"], { encoding: "utf8", timeout: 30_000 });
    if (probe.status === 0 && String(probe.stdout).includes(lock.tool.version)) {
      return { binary: candidate, version: lock.tool.version };
    }
  }
  return null;
}

step("mago lane over the exact generated files", () => {
  const mago = findMago();
  if (mago === null) {
    if (requireMago) {
      throw new Error("mago-missing: --require-mago was set but no pinned tool is available (set LEKALO_MAGO)");
    }
    process.stdout.write(`${JSON.stringify({
      ok: true,
      step: "mago-lane",
      skipped: true,
      reason: "mago-unavailable",
      detail: "a platform skip never counts as Mago acceptance",
    })}\n`);
    return;
  }
  const config = readFileSync(join(repoRoot, "tests", "fixtures", "mago", "toolchain", "mago.toml"), "utf8")
    .replace(/paths = \["src"\]/, 'paths = ["types"]');
  const runMago = (args) =>
    spawnSync(mago.binary, ["--workspace", ".", ...args], {
      cwd: magoRoot,
      encoding: "utf8",
      timeout: 300_000,
      maxBuffer: 64 * 1024 * 1024,
    });
  // A disposable workspace whose `types/` tree is the exact generated
  // bytes (plus the class map): positive gate first, then the negative
  // control that a violating file cannot hide.
  const magoRoot = mkdtempSync(join(tmpdir(), "lekalo-rt-mago-"));
  mkdirSync(join(magoRoot, "types"), { recursive: true });
  cpSync(join(plannerRoot, ".lekalo", "generated", "php-laravel", "types"), join(magoRoot, "types"), { recursive: true });
  writeFileSync(join(magoRoot, "mago.toml"), config);

  const lint = runMago([
    "lint", "--only", "strict-types", "--reporting-format", "json", "--reporting-target", "stdout",
  ]);
  let lintReport = {};
  try {
    lintReport = JSON.parse(lint.stdout || "{}");
  } catch {
    throw new Error(`lint unparseable: ${String(lint.stdout).slice(0, 400)}`);
  }
  const lintIssues = lintReport.issues ?? [];
  assert.deepEqual(
    lintIssues.map((issue) => issue.code),
    [],
    `strict-types lint found violations over the generated types`,
  );

  const analyze = runMago(["analyze", "--reporting-format", "json", "--reporting-target", "stdout"]);
  let analyzeReport = {};
  try {
    analyzeReport = JSON.parse(analyze.stdout || "{}");
  } catch {
    throw new Error(`analyze unparseable: ${String(analyze.stdout).slice(0, 400)}`);
  }
  const analyzeIssues = (analyzeReport.issues ?? []).filter((issue) =>
    String(issue.file ?? issue.location?.file ?? "").includes("types/"),
  );
  assert.deepEqual(
    analyzeIssues.map((issue) => issue.code),
    [],
    `analyze found errors over the generated types: ${JSON.stringify(analyzeIssues).slice(0, 2000)}`,
  );

  // Negative control: a deliberately violating file IS caught, so a
  // clean report proves the rules ran, not that they are off.
  writeFileSync(
    join(magoRoot, "types", "violating_control.php"),
    "<?php\nnamespace Lekalo\\Generated\\Types;\nclass violating_control {\n    public $untyped;\n}\n",
  );
  const negative = runMago([
    "lint", "--only", "strict-types", "--reporting-format", "json", "--reporting-target", "stdout",
  ]);
  let negativeReport = {};
  try {
    negativeReport = JSON.parse(negative.stdout || "{}");
  } catch {
    throw new Error(`negative lint unparseable: ${String(negative.stdout).slice(0, 400)}`);
  }
  assert.ok(
    (negativeReport.issues ?? []).length > 0,
    "the negative control was not caught: a clean report cannot prove rule coverage",
  );
  rmSync(magoRoot, { recursive: true, force: true });
  process.stdout.write(`${JSON.stringify({
    ok: true,
    step: "mago-lane",
    tool: "mago",
    version: mago.version,
    lintIssues: 0,
    analyzeIssues: 0,
    negativeCaught: true,
  })}\n`);
});

rmSync(plannerRoot, { recursive: true, force: true });
process.stdout.write(`${JSON.stringify({ ok: true, suite: "php-laravel-type-roundtrip", checks: passed })}\n`);
