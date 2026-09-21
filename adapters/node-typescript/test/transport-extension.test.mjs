/**
 * Issue #70 transport-extension tests: the deterministic route-layer
 * plan derived from the canonical transport evidence, the explicit
 * unsupported-capability notes, the dry-run/apply discipline, and the
 * kernel descriptor/scan-surface integration (write roots, the
 * generate capability declaration, and the writes projection).
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { createKernel, createReadView, validateResolvedProjectProfile } from "../src/kernel.mjs";
import {
  EXTENSION_VERSION,
  ROUTE_WRITE_ROOT,
  TRANSPORT_CAPABILITY,
  planRouteLayer,
  transportExtensionDescriptor,
  transportGenerateOperation,
} from "../src/transport-extension.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const plannerEvidence = readFileSync(
  join(repoRoot, "tests/fixtures/adapter-conformance/inputs/transport-minimal.json"),
  "utf8",
);
const plannerIr = readFileSync(
  join(repoRoot, "tests/fixtures/adapter-conformance/inputs/ir-minimal.json"),
  "utf8",
);
const sha256Text = (text) =>
  "sha256:" + createHash("sha256").update(text, "utf8").digest("hex");

const PROFILE = {
  id: "standalone",
  mode: "observed",
  target: "node-typescript",
  readRoots: [
    { kind: "tree", path: ".lekalo/cache/transport" },
    { kind: "tree", path: ".lekalo/cache/ir" },
  ],
  exclusions: [],
  provenance: { origin: "declared", revision: "test", disposition: "public-fixture" },
};

/** A temp project with the planner evidence and IR under the cache homes. */
function evidenceProject(irText = plannerIr) {
  const root = mkdtempSync(join(tmpdir(), "lekalo-transport-ext-"));
  const evidenceDir = join(root, ".lekalo", "cache", "transport");
  mkdirSync(evidenceDir, { recursive: true });
  writeFileSync(join(evidenceDir, "planner.json"), plannerEvidence, "utf8");
  const irDir = join(root, ".lekalo", "cache", "ir");
  mkdirSync(irDir, { recursive: true });
  writeFileSync(join(irDir, "planner.json"), irText, "utf8");
  return root;
}

/** The Model endpoint joins of the fixture IR evidence. */
function fixtureJoins(document = JSON.parse(plannerIr)) {
  const joins = new Map();
  for (const definition of document.definitions) {
    if (definition.kind === "endpoint") {
      joins.set(definition.id, {
        method: definition.method,
        path: definition.path,
        invokes: definition.invokes,
      });
    }
  }
  return joins;
}

const REQUEST_BASE = {
  protocol: "lekalo.target/v1",
  protocol_version: "0.3.2",
  operation: "generate",
  request_id: "req-transport-1",
  project_root: ".",
  target: "node-typescript",
  profile: "standalone",
  dry_run: true,
  ir_path: ".lekalo/ir/planner.json",
};

function kernelFor(root) {
  return createKernel({
    resolvedProjectProfile: validateResolvedProjectProfile({
      ...PROFILE,
      readRoots: PROFILE.readRoots,
    }),
    extensionRegistry: [transportExtensionDescriptor()],
  });
}

test("the descriptor declares the transport capability and write root", () => {
  const descriptor = transportExtensionDescriptor();
  assert.equal(descriptor.id, "http-transport-generator");
  assert.equal(descriptor.version, EXTENSION_VERSION);
  assert.deepEqual(descriptor.operations, ["generate"]);
  assert.equal(descriptor.namedCapabilities[TRANSPORT_CAPABILITY], "partial");
  assert.equal(descriptor.namedCapabilities["generate.openapi"], "unsupported");
  assert.deepEqual(descriptor.acceptedIrVersions, ["0.2.16"]);
  assert.deepEqual(descriptor.writeRoots, [ROUTE_WRITE_ROOT]);
  assert.deepEqual(descriptor.readRoots, [".lekalo/cache/transport", ".lekalo/cache/ir"]);
});

test("the route plan is deterministic and derived from the evidence", () => {
  const evidence = JSON.parse(plannerEvidence);
  const joins = fixtureJoins();
  const first = planRouteLayer(evidence, "planner", joins);
  const second = planRouteLayer(evidence, "planner", joins);
  assert.deepEqual(first, second, "repeated plans are identical");
  assert.equal(first.writes.length, 1, "one route module per model module");
  const write = first.writes[0];
  assert.equal(write.path, "src/routes/planner.routes.ts");
  assert.equal(write.action, "create");
  assert.equal(write.sha256, sha256Text(first.bodies[0].bytes));
  // The route module pins the operation id and the error envelope.
  const text = first.bodies[0].bytes;
  assert.match(text, /plannerApiFocus/);
  assert.match(text, /"errorEnvelope":"canonical-v1"/);
  // The Model join fills the wire surface: never a null-bearing route.
  assert.match(text, /"method":"POST"/);
  assert.match(text, /"path":"\/tasks\/\{task_id\}\/focus"/);
  assert.match(text, /"invokes":"planner.focus_task"/);
  assert.doesNotMatch(text, /"method":null/);
  // Every declared wire member is carried through, never dropped:
  // policy surface (rateLimit, cache, apiVersion) and declaration
  // data (tags, summary, scenarios) all survive the projection.
  for (const member of ["rateLimit", "cache", "apiVersion", "tags", "summary", "scenarios"]) {
    assert.match(text, new RegExp(`"${member}":`), `${member} is projected`);
  }
  // Members the endpoint does not declare stay absent, never null.
  assert.doesNotMatch(text, /"pagination":null/);
});

test("an unjoined endpoint throws instead of planning a null route", () => {
  const evidence = JSON.parse(plannerEvidence);
  assert.throws(
    () => planRouteLayer(evidence, "planner", new Map()),
    /transport-endpoint-unjoined/,
  );
});

test("declared capabilities are reported unsupported, never silent", () => {
  const evidence = JSON.parse(plannerEvidence);
  const plan = planRouteLayer(evidence, "planner", fixtureJoins());
  assert.equal(plan.notes.length, 1, "the streaming declaration is noted");
  assert.deepEqual(plan.notes[0], {
    capability: "streaming",
    detail: "sse",
    minimumSupport: "partial",
    state: "unsupported",
  });
});

test("a dry run plans without writing; an apply publishes the plan", () => {
  const root = evidenceProject();
  try {
    const kernel = kernelFor(root);
    const dry = kernel.dispatch(
      { ...REQUEST_BASE, dry_run: true },
      { permittedProjectRoot: root },
    );
    assert.equal(dry.response.status, "ok");
    assert.equal(dry.response.writes.length, 1);
    assert.equal(
      existsSync(join(root, "src/routes/planner.routes.ts")),
      false,
      "the dry run never writes",
    );
    // The plan bytes are identical across repeats.
    const again = kernel.dispatch(
      { ...REQUEST_BASE, dry_run: true },
      { permittedProjectRoot: root },
    );
    assert.equal(
      JSON.stringify(again.response.writes),
      JSON.stringify(dry.response.writes),
    );
    // The apply publishes exactly the plan.
    const apply = kernel.dispatch(
      { ...REQUEST_BASE, dry_run: false, plan_id: "plan-transport-1" },
      { permittedProjectRoot: root },
    );
    assert.equal(apply.response.status, "ok");
    assert.deepEqual(apply.response.writes, dry.response.writes);
    const published = readFileSync(join(root, "src/routes/planner.routes.ts"), "utf8");
    assert.equal(sha256Text(published), dry.response.writes[0].sha256);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a missing evidence file is an honest failure, never a silent plan", () => {
  const root = mkdtempSync(join(tmpdir(), "lekalo-transport-empty-"));
  try {
    const outcome = transportGenerateOperation({
      request: { ...REQUEST_BASE },
      readView: createScopedReadView(root),
    });
    assert.equal(outcome.state, "failed");
    assert.equal(outcome.diagnostics[0].reason, "transport-evidence-absent");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a missing compiled-IR evidence refuses instead of planning nulls", () => {
  const root = evidenceProject();
  try {
    rmSync(join(root, ".lekalo", "cache", "ir", "planner.json"));
    const outcome = transportGenerateOperation({
      request: { ...REQUEST_BASE },
      readView: createScopedReadView(root),
    });
    assert.equal(outcome.state, "failed");
    assert.equal(outcome.diagnostics[0].reason, "ir-evidence-absent");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a foreign project in either evidence document refuses the plan", () => {
  // The IR carries a different project id than the transport evidence.
  const foreignIr = JSON.parse(plannerIr);
  foreignIr.project.id = "other";
  const root = evidenceProject(JSON.stringify(foreignIr));
  try {
    const outcome = transportGenerateOperation({
      request: { ...REQUEST_BASE },
      readView: createScopedReadView(root),
    });
    assert.equal(outcome.state, "failed");
    assert.equal(outcome.diagnostics[0].reason, "transport-project-mismatch");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("an endpoint the IR cannot resolve refuses the plan", () => {
  // The transport evidence references an endpoint symbol the IR lacks.
  const missingJoin = JSON.parse(plannerEvidence);
  missingJoin.endpoints[0].endpoint = "planner.endpoint_missing";
  const foreignEvidence = JSON.stringify(missingJoin);
  const root = mkdtempSync(join(tmpdir(), "lekalo-transport-unjoined-"));
  try {
    mkdirSync(join(root, ".lekalo", "cache", "transport"), { recursive: true });
    writeFileSync(join(root, ".lekalo", "cache", "transport", "planner.json"), foreignEvidence, "utf8");
    mkdirSync(join(root, ".lekalo", "cache", "ir"), { recursive: true });
    writeFileSync(join(root, ".lekalo", "cache", "ir", "planner.json"), plannerIr, "utf8");
    const outcome = transportGenerateOperation({
      request: { ...REQUEST_BASE },
      readView: createScopedReadView(root),
    });
    assert.equal(outcome.state, "failed");
    assert.equal(outcome.diagnostics[0].reason, "transport-endpoint-unjoined");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

function createScopedReadView(root) {
  const profile = validateResolvedProjectProfile({ ...PROFILE });
  const view = createReadView(root, profile.readRoots, profile);
  view.permittedProjectRoot = root;
  return view;
}

test("the kernel advertises generate with the transport capability and write scope", () => {
  const root = evidenceProject();
  try {
    const kernel = kernelFor(root);
    const described = kernel.describe();
    assert.ok(described.operations.includes("generate"));
    assert.equal(described.capabilities[TRANSPORT_CAPABILITY], "partial");
    assert.deepEqual(described.write_scopes, [ROUTE_WRITE_ROOT]);
    assert.ok(described.read_scopes.includes(".lekalo/cache/transport/**"));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
