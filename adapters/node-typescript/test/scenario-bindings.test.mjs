/**
 * #47 checked-binding suite (plan S8): the `lekalo:<id>` title
 * convention rides the scanner's observed `t` slot, and the scenario
 * verify joins native checked bindings against the observed index —
 * reporting missing, ambiguous, and stale bindings as typed findings,
 * never silently resolving them.
 */
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  BINDING_AMBIGUOUS,
  BINDING_MISSING,
  BINDING_MISMATCH,
  OBSERVED_INDEX_PATH,
  joinCheckedBindings,
} from "../src/scenario-gen.mjs";
import { lekaloTestIdsByModule } from "../src/scanner.mjs";

const FINGERPRINT = "sha256:" + "a".repeat(64);
const STALE = "sha256:" + "b".repeat(64);

function binding(test, extra = {}) {
  return {
    backend: "native",
    runner: "node:test",
    runnerVersion: "24.0.0",
    capabilities: [],
    capabilityDigest: "sha256:" + "0".repeat(64),
    test,
    mode: "checked",
    ...extra,
  };
}

function indexDocument(records) {
  return {
    schema_version: "lekalo/observed-index/v0.2.16",
    test_bindings: records,
  };
}

test("a checked binding joins a matching scanned test with no findings", () => {
  const scenario = {
    bindings: [binding("planner.scenario.focus_happy", { evidenceDigest: FINGERPRINT })],
  };
  const index = indexDocument([
    {
      id: "src/tests/focus.test.ts#lekalo:planner.scenario.focus_happy",
      symbol: "planner.focus_task",
      path: "src/tests/focus.test.ts",
      fingerprint: FINGERPRINT,
      state: "current",
    },
  ]);
  assert.deepEqual(joinCheckedBindings(scenario, index), []);
});

test("a binding with no scanned test is missing", () => {
  const scenario = { bindings: [binding("planner.scenario.mystery")] };
  const index = indexDocument([
    {
      id: "src/tests/focus.test.ts#lekalo:planner.scenario.focus_happy",
      symbol: "planner.focus_task",
      path: "src/tests/focus.test.ts",
      fingerprint: FINGERPRINT,
    },
  ]);
  const findings = joinCheckedBindings(scenario, index);
  assert.equal(findings.length, 1);
  assert.equal(findings[0].code, BINDING_MISSING);
  assert.equal(findings[0].symbol, "planner.scenario.mystery");
});

test("an id claimed by two tests is ambiguous, never silently resolved", () => {
  const scenario = { bindings: [binding("planner.scenario.focus_happy")] };
  const index = indexDocument([
    {
      id: "src/tests/a.test.ts#lekalo:planner.scenario.focus_happy",
      symbol: "planner.focus_task",
      path: "src/tests/a.test.ts",
      fingerprint: FINGERPRINT,
    },
    {
      id: "src/tests/b.test.ts#lekalo:planner.scenario.focus_happy",
      symbol: "planner.focus_task",
      path: "src/tests/b.test.ts",
      fingerprint: STALE,
    },
  ]);
  const findings = joinCheckedBindings(scenario, index);
  assert.equal(findings[0].code, BINDING_AMBIGUOUS);
  assert.equal(findings[0].detail, "claimed-by-2-tests");
});

test("a shared native test file claiming several ids joins cleanly (F-5)", () => {
  // One native test file may legitimately cover several scenarios; the
  // join accepts a record whose claimed set CONTAINS the bound id
  // (review F-5). Ambiguity remains only when several different files
  // claim the same id.
  const scenario = { bindings: [binding("planner.scenario.focus_happy")] };
  const index = indexDocument([
    {
      id: "src/tests/both.test.ts#lekalo:planner.scenario.focus_happy,lekalo:planner.scenario.focus_error",
      symbol: "planner.focus_task",
      path: "src/tests/both.test.ts",
      fingerprint: FINGERPRINT,
    },
  ]);
  assert.deepEqual(joinCheckedBindings(scenario, index), []);
});

test("a stale evidence digest is a mismatch, never a silent rewrite", () => {
  const scenario = {
    bindings: [binding("planner.scenario.focus_happy", { evidenceDigest: STALE })],
  };
  const index = indexDocument([
    {
      id: "src/tests/focus.test.ts#lekalo:planner.scenario.focus_happy",
      symbol: "planner.focus_task",
      path: "src/tests/focus.test.ts",
      fingerprint: FINGERPRINT,
    },
  ]);
  const findings = joinCheckedBindings(scenario, index);
  assert.equal(findings.length, 1);
  assert.equal(findings[0].code, BINDING_MISMATCH);
  assert.equal(findings[0].detail, "stale-evidence-digest");
});

test("generated and fake-reference bindings never join", () => {
  const scenario = {
    bindings: [
      binding("planner.scenario.focus_happy", { mode: "generated" }),
      {
        backend: "fake-reference",
        runner: "node:test",
        runnerVersion: "24.0.0",
        capabilities: [],
        capabilityDigest: "sha256:" + "0".repeat(64),
        test: "planner.scenario.focus_happy",
        mode: "checked",
      },
    ],
  };
  assert.deepEqual(joinCheckedBindings(scenario, indexDocument([])), []);
});

test("an absent observed index is legal absence: the join says nothing", () => {
  const scenario = { bindings: [binding("planner.scenario.focus_happy")] };
  assert.deepEqual(joinCheckedBindings(scenario, null), []);
  assert.deepEqual(joinCheckedBindings(scenario, {}), []);
});

test("the scanner groups lekalo test titles per module with bounds", () => {
  const { byModule, truncated } = lekaloTestIdsByModule([
    { path: "src/tests/focus.test.ts", name: "lekalo:planner.scenario.focus_happy" },
    { path: "src/tests/focus.test.ts", name: "lekalo:planner.scenario.focus_error" },
    { path: "src/tests/focus.test.ts", name: "lekalo:planner.scenario.focus_happy" },
    { path: "src/tests/other.test.ts", name: "not-a-lekalo-title" },
    { path: "src/tests/edge.test.ts", name: "lekalo:" },
  ]);
  assert.deepEqual(byModule.get("src/tests/focus.test.ts"), [
    "planner.scenario.focus_error",
    "planner.scenario.focus_happy",
  ]);
  assert.equal(byModule.has("src/tests/other.test.ts"), false);
  assert.equal(byModule.has("src/tests/edge.test.ts"), false);
  assert.deepEqual(truncated, [], "nothing clips in this vector");
});

test("the observed index path pin holds", () => {
  assert.equal(OBSERVED_INDEX_PATH, ".lekalo/import/observed/index.json");
});

/**
 * Review F-2: the only end-to-end walk of the checked-binding custody
 * chain — a fixture test file with `lekalo:<id>` titles goes through
 * the production `scanOperation`, the entries promote into the exact
 * observed-index `test_bindings` shape the core's scan service writes
 * (`id = "<path>#<t>"`, `symbol` = the first entry's semantic proposal,
 * `fingerprint` = the file digest), and `joinCheckedBindings` answers
 * over that document. The e2e gate never scans a project with
 * `lekalo:`-titled tests before verifying; this vector does.
 */
async function scanAndJoin(scanBody, { unrelatedFirstSymbol = false } = {}) {
  const { createHash } = await import("node:crypto");
  const fs = await import("node:fs");
  const path = await import("node:path");
  const os = await import("node:os");
  const { loadAdapter, dispose } = await import("./scanner-helpers.mjs");
  const adapter = await loadAdapter();
  const kernel = adapter.__lekaloKernel;
  // The wire scan entry point of the production bundle (the vendored
  // compiler rides the built artifact, never the src deployment).
  const { scanOperation } = adapter.__lekaloScanner;
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "lekalo-s47-join-")));
  const project = path.join(root, "project");
  fs.mkdirSync(path.join(project, "src"), { recursive: true });
  const testFile = [
    'import { test } from "./dsl";',
    ...(unrelatedFirstSymbol
      ? ["export function sharedHelper(): number { return 1; }", ""]
      : []),
    // The exported const mirrors the emitter's top-level shape: the
    // generated test files always carry one top-level export whose
    // scanned entry is the claim carrier.
    'export const lekalo_planner_scenario_focus = { scenario: "planner.scenario.focus_happy" };',
    'test("lekalo:planner.scenario.focus_happy", () => {});',
    'test("lekalo:planner.scenario.focus_error", () => {});',
  ].join("\n");
  fs.writeFileSync(path.join(project, "src", "focus.test.ts"), testFile);
  // The known DSL vocabulary is statically imported describe/it/test:
  // the fixture declares it locally exactly like the committed fixtures
  // do, so module resolution stays complete.
  fs.writeFileSync(
    path.join(project, "src", "dsl.ts"),
    "export function test(name: string, fn: () => void): void {}\n",
  );
  try {
    const roots = [{ kind: "tree", path: "src", scope: "src/**" }];
    const profile = kernel.validateResolvedProjectProfile({
      id: "join-e2e",
      mode: "observed",
      target: "node-typescript",
      readRoots: roots.map(({ kind, path: p }) => ({ kind, path: p })),
      exclusions: [],
      provenance: { origin: "declared", revision: "join-e2e-0001", disposition: "public-fixture" },
    });
    const readView = kernel.createReadView(project, roots, profile);
    // The kernel dispatch stamps the permitted root onto the view after
    // creation; the scan entry point requires it.
    readView.permittedProjectRoot = project;
    const outcome = scanOperation({
      operation: "scan",
      profile,
      readView,
      permittedProjectRoot: project,
      limits: undefined,
    });
    assert.equal(outcome.state, "complete", JSON.stringify(outcome.diagnostics ?? null));
    scanBody({
      entries: outcome.data.entries,
      readFile: (relative) => readView.readFile(relative),
      joinCheckedBindings,
      createHash,
      unrelatedFirstSymbol,
    });
  } finally {
    dispose(root);
  }
}

test("the scan to observed-index to join chain is clean end-to-end (F-2)", () => {
  return scanAndJoin(({ entries, readFile, joinCheckedBindings, createHash }) => {
    // The core's promotion: every entry detail that carries the bounded
    // `t` slot becomes one test_bindings record — the id is the verbatim
    // `t` spelling, the symbol is that entry's semantic proposal, and
    // the fingerprint is the real file digest.
    const records = [];
    for (const entry of entries) {
      const detail = JSON.parse(entry.detail);
      if (typeof detail.t !== "string") continue;
      records.push({
        id: detail.t,
        symbol: detail.s,
        path: entry.path,
        fingerprint: createHash("sha256").update(readFile(entry.path)).digest("hex"),
      });
    }
    assert.equal(records.length, 1, "one claiming file, one record");
    assert.match(records[0].id, /#lekalo:planner\.scenario\.focus_(happy|error)/);
    assert.match(records[0].fingerprint, /^[0-9a-f]{64}$/);

    // The join answers over the promoted document exactly as it does in
    // production. One binding pins the real file fingerprint (equality
    // path), one binds without evidence (no freshness claim), and a
    // stale digest is the typed mismatch finding.
    const scenario = {
      bindings: [
        binding("planner.scenario.focus_happy"),
        binding("planner.scenario.focus_error", { evidenceDigest: records[0].fingerprint }),
      ],
    };
    const findings = joinCheckedBindings(scenario, indexDocument(records));
    assert.deepEqual(findings, [], "both claims join cleanly");
    const stale = joinCheckedBindings(
      { bindings: [binding("planner.scenario.focus_error", { evidenceDigest: FINGERPRINT })] },
      indexDocument(records),
    );
    assert.equal(stale.length, 1);
    assert.equal(stale[0].code, BINDING_MISMATCH);

    // The honest negative over the same chain: an id nobody claims is
    // still a typed missing finding.
    const missing = joinCheckedBindings(
      { bindings: [binding("planner.scenario.mystery")] },
      indexDocument(records),
    );
    assert.equal(missing.length, 1);
    assert.equal(missing[0].code, BINDING_MISSING);
  });
});

test("an unrelated first top-level symbol still owns the claim deterministically (F-2)", () => {
  return scanAndJoin(
    ({ entries, readFile, joinCheckedBindings, createHash, unrelatedFirstSymbol }) => {
      assert.ok(unrelatedFirstSymbol);
      const claiming = [];
      for (const entry of entries) {
        const detail = JSON.parse(entry.detail);
        if (typeof detail.t === "string") claiming.push(detail);
      }
      assert.equal(claiming.length, 1, "the claim rides exactly one entry");
      // The claim lands on the FIRST top-level entry of the file — here
      // the unrelated helper — deterministically, per file, never lost:
      // the join still resolves because attribution is per record, not
      // per test function.
      const records = claiming.map((detail) => ({
        id: detail.t,
        symbol: detail.s,
        path: "src/focus.test.ts",
        fingerprint: createHash("sha256").update(readFile("src/focus.test.ts")).digest("hex"),
      }));
      const scenario = { bindings: [binding("planner.scenario.focus_happy")] };
      assert.deepEqual(joinCheckedBindings(scenario, indexDocument(records)), []);
    },
    { unrelatedFirstSymbol: true },
  );
});test('a shared native test file claiming several ids joins cleanly (F-5)', () => {
  const scenario = { bindings: [binding('planner.scenario.focus_happy')] };
  const index = indexDocument([
    {
      id: 'src/tests/both.test.ts#lekalo:planner.scenario.focus_happy,lekalo:planner.scenario.focus_error',
      symbol: 'planner.focus_task',
      path: 'src/tests/both.test.ts',
      fingerprint: FINGERPRINT,
    },
  ]);
  assert.deepEqual(joinCheckedBindings(scenario, index), []);
});

test('the scanner surfaces truncated id budgets as uncertainty (F-5)', () => {
  const { byModule, truncated } = lekaloTestIdsByModule(
    Array.from({ length: 10 }, (_, index) => ({
      path: 'src/tests/crowded.test.ts',
      name: `lekalo:planner.s.c${index}`,
    })),
  );
  assert.equal(byModule.get('src/tests/crowded.test.ts').length, 8, 'the budget caps at 8');
  assert.deepEqual(truncated, ['src/tests/crowded.test.ts'], 'the clip is named, not silent');
});

