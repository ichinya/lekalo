/**
 * #47 scenario compiler extension suite: the composite join, the
 * document-identity routing, the generate dry-run/apply exchange over
 * the fixture project, the verify drift gate, and the capability
 * advertisement. Pure in-process dispatch through the real kernel
 * boundary — no vendored compiler bundle required.
 */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

import { descriptor as generationComposite } from "../src/generation-composite.mjs";
import {
  SCENARIO_DRIFT,
  SCENARIO_EXTENSION_VERSION,
  scenarioPlanIdOf,
} from "../src/scenario-gen.mjs";
import { createKernel } from "../src/kernel.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..", "..", "..");

function load(path) {
  return JSON.parse(readFileSync(join(repoRoot, path), "utf8"));
}

const irDigest = "sha256:" + createHash("sha256").update(
  readFileSync(join(repoRoot, "tests/fixtures/adapter-conformance/inputs/ir-minimal.json")),
).digest("hex");

const PORT_DOC = load("tests/fixtures/orchestration/project/lekalo/test-port.json");

const SCENARIO_DOCS = [
  "planner.scenario.focus_happy",
  "planner.scenario.focus_error",
  "planner.scenario.focus_idempotent",
  "planner.scenario.focus_concurrent",
].map((id) => ({
  id,
  text: readFileSync(
    join(repoRoot, "tests/fixtures/orchestration/project/lekalo/scenarios", `${id}.json`),
    "utf8",
  ),
}));

const PROFILE = {
  id: "scenario-fixture",
  mode: "observed",
  target: "node-typescript",
  readRoots: [
    { path: "lekalo", kind: "tree" },
    { path: ".lekalo", kind: "tree" },
    { path: "src", kind: "tree" },
  ],
  exclusions: [],
  provenance: {
    origin: "declared",
    revision: "issue-47-scenario-0001",
    disposition: "public-fixture",
  },
};

/** Materialize the fixture project files physically under the root. */
function materialize(root, scenarioId = "planner.scenario.focus_happy") {
  const files = new Map();
  files.set("lekalo/test-port.json", JSON.stringify(PORT_DOC));
  files.set(".lekalo/cache/ir/planner.json", readFileSync(
    join(repoRoot, "tests/fixtures/adapter-conformance/inputs/ir-minimal.json"),
    "utf8",
  ));
  const scenario = SCENARIO_DOCS.find((doc) => doc.id === scenarioId);
  files.set(".lekalo/ir/scenario.json", scenario.text);
  // The generated home's parent must physically exist: the kernel
  // validates every read root (including src) before dispatch.
  mkdirSync(join(root, "src"), { recursive: true });
  for (const [path, text] of files) {
    const target = join(root, ...path.split("/"));
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, text);
  }
  return files;
}

function kernelWith(root) {
  const kernel = createKernel({
    resolvedProjectProfile: PROFILE,
    extensionRegistry: [generationComposite],
  });
  const dispatch = (request) => kernel.dispatch(
    request,
    {
      permittedProjectRoot: root,
      cancellation: null,
      limits: { files: 4096, bytes: 4 * 1024 * 1024 },
    },
  );
  return { kernel, dispatch };
}

/** Read one file the pipeline wrote under the project root. */
function written(root, path) {
  return readFileSync(join(root, ...path.split("/")), "utf8");
}

function scenarioRequest(operation, extra = {}) {
  return {
    id: "req-" + "0".repeat(8),
    operation,
    protocol_version: "0.3.2",
    ir_path: ".lekalo/ir/scenario.json",
    target: "node-typescript",
    profile: "scenario-fixture",
    dry_run: operation === "generate",
    ...extra,
  };
}

test("the composite advertises verify.scenarios full and the scenario write scope", () => {
  const root = mkdtempSync(join(tmpdir(), "lekalo-scenario-ext-"));
  try {
    materialize(root);
    const { kernel } = kernelWith(root);
    const capabilities = kernel.describe();
    assert.equal(capabilities.capabilities["verify.scenarios"], "full");
    assert.ok(
      capabilities.write_scopes.includes("src/generated/node-typescript/scenario-tests/**"),
      "the scenario write scope is advertised",
    );
    assert.ok(capabilities.operations.includes("generate"));
    assert.ok(capabilities.operations.includes("verify"));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the kernel default map keeps verify.scenarios unsupported", () => {
  // The extension-free describe (no roots, no extensions) is the honest
  // default; the composite join upgrades exactly that entry.
  const bare = createKernel({});
  const capabilities = bare.describe();
  assert.equal(capabilities.capabilities["verify.scenarios"], "unsupported");
});

test("generate dry-run over a scenario document returns the byte-stable plan", () => {
  const root = mkdtempSync(join(tmpdir(), "lekalo-scenario-ext-"));
  try {
    materialize(root);
    const { dispatch } = kernelWith(root);
    const { response } = dispatch(scenarioRequest("generate", { dry_run: true }));
    assert.equal(response.status, "ok");
    const writes = response.writes;
    const paths = writes.map((entry) => entry.path);
    assert.deepEqual(paths, [...paths].sort());
    assert.ok(paths.includes("src/generated/node-typescript/scenario-tests/testkit.ts"));
    assert.ok(paths.includes("src/generated/node-typescript/scenario-tests/planner/planner.scenario.focus_happy.test.ts"));
    assert.ok(paths.every((path) => path.startsWith("src/generated/node-typescript/scenario-tests/")));
    assert.match(response.evidence.plan_id, /^plan-[0-9a-f]{64}$/);
    // The plan id is the shared domain over the sorted write entries.
    assert.equal(response.evidence.plan_id, scenarioPlanIdOf(writes));
    // A repeat dispatch is byte-identical (the dry-run determinism).
    const again = dispatch(scenarioRequest("generate", { dry_run: true }));
    assert.deepEqual(again.response.writes, writes);
    assert.equal(again.response.evidence.plan_id, response.evidence.plan_id);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("generate apply echoes the plan id and writes the exact bytes", () => {
  const root = mkdtempSync(join(tmpdir(), "lekalo-scenario-ext-"));
  try {
    materialize(root);
    const { dispatch } = kernelWith(root);
    const dry = dispatch(scenarioRequest("generate", { dry_run: true }));
    const plan = { writes: dry.response.writes, plan_id: dry.response.evidence.plan_id };
    const apply = dispatch(
      scenarioRequest("generate", { dry_run: false, plan_id: plan.plan_id }),
    );
    assert.equal(apply.response.status, "ok");
    assert.deepEqual(apply.response.writes, plan.writes);
    for (const entry of plan.writes) {
      const writtenBytes = readFileSync(join(root, ...entry.path.split("/")));
      const digest = "sha256:" + createHash("sha256").update(writtenBytes).digest("hex");
      assert.equal(digest, entry.sha256, `${entry.path} write receipt digest`);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("apply without the echoed plan id is refused", () => {
  const root = mkdtempSync(join(tmpdir(), "lekalo-scenario-ext-"));
  try {
    materialize(root);
    const { dispatch } = kernelWith(root);
    const apply = dispatch(scenarioRequest("generate", { dry_run: false }));
    assert.equal(apply.response.status, "error");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("verify is clean after generation and reports scenario.drift after edits", () => {
  const root = mkdtempSync(join(tmpdir(), "lekalo-scenario-ext-"));
  try {
    materialize(root);
    const { dispatch } = kernelWith(root);
    const dry = dispatch(scenarioRequest("generate", { dry_run: true }));
    dispatch(scenarioRequest("generate", { dry_run: false, plan_id: dry.response.evidence.plan_id }));
    const clean = dispatch(scenarioRequest("verify"));
    assert.equal(clean.response.status, "ok");
    assert.equal(clean.response.result.ok, true);
    assert.deepEqual(clean.response.result.findings ?? [], []);
    // Manual drift: one edited byte in one generated file.
    const drifted = "src/generated/node-typescript/scenario-tests/testkit.ts";
    const driftedPath = join(root, ...drifted.split("/"));
    writeFileSync(driftedPath, readFileSync(driftedPath, "utf8").replace("typedEqual", "typedEq"));
    const dirty = dispatch(scenarioRequest("verify"));
    // The wire keeps ok=true (the operation completed); the drift signal
    // rides the typed findings exactly like the zod pipeline.
    assert.equal(dirty.response.result.ok, true);
    const finding = dirty.response.result.findings.find((entry) => entry.path === drifted);
    assert.equal(finding.code, SCENARIO_DRIFT);
    assert.match(finding.detail, /expected:[0-9a-f]{12} observed:[0-9a-f]{12}/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a scenario document without the port declaration vetoes generation", () => {
  const root = mkdtempSync(join(tmpdir(), "lekalo-scenario-ext-"));
  try {
    materialize(root);
    rmSync(join(root, "lekalo", "test-port.json"));
    const { dispatch } = kernelWith(root);
    const { response } = dispatch(scenarioRequest("generate", { dry_run: true }));
    assert.equal(response.status, "error");
    assert.equal(response.error.code, "outcome-partial-unsupported-constructs");
    assert.equal(response.error.detail[0], "scenario.port-missing-declaration-absent");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the concurrency scenario compiles but the serial pipeline stays honest", () => {
  // The concurrency-marked document emits a test file whose rows are
  // recorded unsupported and whose runner outcome is skip — never a
  // pass. The extension itself only refuses to claim more than it did.
  const root = mkdtempSync(join(tmpdir(), "lekalo-scenario-ext-"));
  try {
    materialize(root, "planner.scenario.focus_concurrent");
    const { dispatch } = kernelWith(root);
    const dry = dispatch(scenarioRequest("generate", { dry_run: true }));
    dispatch(scenarioRequest("generate", { dry_run: false, plan_id: dry.response.evidence.plan_id }));
    const testPath = "src/generated/node-typescript/scenario-tests/planner/planner.scenario.focus_concurrent.test.ts";
    const text = written(root, testPath);
    assert.match(text, /outcome: "unsupported"/);
    assert.match(text, /t\.skip\("scenario\.unsupported-capability"\)/);
    assert.doesNotMatch(text, /port\.invoke/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("project-IR documents keep routing to the zod pipeline", () => {
  const root = mkdtempSync(join(tmpdir(), "lekalo-scenario-ext-"));
  try {
    materialize(root);
    writeFileSync(
      join(root, ".lekalo", "ir", "scenario.json"),
      readFileSync(
        join(repoRoot, "tests/fixtures/adapter-conformance/inputs/ir-minimal.json"),
        "utf8",
      ),
    );
    const { dispatch } = kernelWith(root);
    const { response } = dispatch(scenarioRequest("generate", { dry_run: true }));
    assert.equal(response.status, "ok");
    // Zod writes under its own home, never the scenario-tests home.
    for (const entry of response.writes ?? []) {
      assert.ok(!entry.path.includes("scenario-tests"), entry.path);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the extension version and digest pins hold", () => {
  assert.equal(SCENARIO_EXTENSION_VERSION, "0.4.0");
  assert.equal(SCENARIO_DRIFT, "scenario.drift");
  assert.match(scenarioPlanIdOf([]), /^plan-[0-9a-f]{64}$/);
});
