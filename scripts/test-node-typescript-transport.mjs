#!/usr/bin/env node
/**
 * Issue #70 transport-extension gate: the committed transport
 * extension's descriptor, deterministic route-layer plan, explicit
 * unsupported-capability notes, and dry-run/apply discipline over the
 * committed conformance evidence fixture, driven through the
 * production kernel. Node built-ins only — no Ajv, no packages.
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { createKernel, createReadView, validateResolvedProjectProfile } from "../adapters/node-typescript/src/kernel.mjs";
import {
  ROUTE_WRITE_ROOT,
  TRANSPORT_CAPABILITY,
  planRouteLayer,
  transportExtensionDescriptor,
  transportGenerateOperation,
} from "../adapters/node-typescript/src/transport-extension.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const evidenceBytes = readFileSync(
  join(repoRoot, "tests/fixtures/adapter-conformance/inputs/transport-minimal.json"),
  "utf8",
);
const irBytes = readFileSync(
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

const REQUEST = {
  protocol: "lekalo.target/v1",
  protocol_version: "0.3.2",
  operation: "generate",
  request_id: "req-transport-gate-1",
  project_root: ".",
  target: "node-typescript",
  profile: "standalone",
  dry_run: true,
  ir_path: ".lekalo/ir/planner.json",
};

function evidenceProject() {
  const root = realpathSync(mkdtempSync(join(tmpdir(), "lekalo-transport-gate-")));
  const dir = join(root, ".lekalo", "cache", "transport");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "planner.json"), evidenceBytes, "utf8");
  const irDir = join(root, ".lekalo", "cache", "ir");
  mkdirSync(irDir, { recursive: true });
  writeFileSync(join(irDir, "planner.json"), irBytes, "utf8");
  return root;
}

/** The Model endpoint joins of the fixture IR evidence. */
function fixtureJoins(document = JSON.parse(irBytes)) {
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

test("gate: the descriptor claims the transport capability honestly", () => {
  const descriptor = transportExtensionDescriptor();
  assert.equal(descriptor.namedCapabilities[TRANSPORT_CAPABILITY], "partial");
  assert.equal(descriptor.namedCapabilities["generate.openapi"], "unsupported");
  assert.deepEqual(descriptor.writeRoots, [ROUTE_WRITE_ROOT]);
});

test("gate: the plan is deterministic and byte-pinned across repeats", () => {
  const evidence = JSON.parse(evidenceBytes);
  const joins = fixtureJoins();
  const first = planRouteLayer(evidence, "planner", joins);
  for (let index = 0; index < 3; index += 1) {
    assert.deepEqual(planRouteLayer(evidence, "planner", joins), first);
  }
  assert.equal(first.writes[0].path, "src/routes/planner.routes.ts");
  assert.equal(first.writes[0].sha256, sha256Text(first.bodies[0].bytes));
  // Every route carries its joined Model surface: never a null.
  const module = JSON.parse(
    first.bodies[0].bytes.slice(first.bodies[0].bytes.indexOf("=") + 1, -2),
  );
  for (const route of module.routes) {
    assert.equal(route.method, "POST");
    assert.equal(route.path, "/tasks/{task_id}/focus");
    assert.equal(route.invokes, "planner.focus_task");
    // The full declared policy surface survives the projection.
    assert.deepEqual(route.rateLimit, { limit: 120, windowSeconds: 60, scope: "actor" });
    assert.deepEqual(route.cache, { policy: "no-store", maxAgeSeconds: 0, etag: false });
    assert.deepEqual(route.apiVersion, { in: "header", name: "v1" });
    assert.deepEqual(route.tags, ["planner"]);
    assert.equal(route.summary, "Focus one task over HTTP");
    assert.deepEqual(route.scenarios, ["planner.focus_flow"]);
  }
});

test("gate: dry run never writes and apply publishes exactly the plan", () => {
  const root = evidenceProject();
  try {
    const kernel = createKernel({
      resolvedProjectProfile: validateResolvedProjectProfile({ ...PROFILE }),
      extensionRegistry: [transportExtensionDescriptor()],
    });
    const dry = kernel.dispatch({ ...REQUEST }, { permittedProjectRoot: root });
    assert.equal(dry.response.status, "ok", JSON.stringify(dry.response.error ?? {}));
    assert.equal(dry.response.writes.length, 1);
    assert.equal(existsSync(join(root, "src/routes/planner.routes.ts")), false);
    const apply = kernel.dispatch(
      { ...REQUEST, dry_run: false, plan_id: "plan-transport-gate-1" },
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

test("gate: unsupported capabilities are explicit notes, never silent", () => {
  const evidence = JSON.parse(evidenceBytes);
  const plan = planRouteLayer(evidence, "planner", fixtureJoins());
  assert.equal(plan.notes.length, 1);
  assert.equal(plan.notes[0].capability, "streaming");
  assert.equal(plan.notes[0].state, "unsupported");
});

test("gate: an unjoined endpoint refuses instead of planning nulls", () => {
  const evidence = JSON.parse(evidenceBytes);
  assert.throws(
    () => planRouteLayer(evidence, "planner", new Map()),
    /transport-endpoint-unjoined/,
  );
});

test("gate: an absent evidence file refuses honestly", () => {
  const root = realpathSync(mkdtempSync(join(tmpdir(), "lekalo-transport-none-")));
  try {
    const profile = validateResolvedProjectProfile({ ...PROFILE });
    const view = createReadView(root, profile.readRoots, profile);
    view.permittedProjectRoot = root;
    const outcome = transportGenerateOperation({ request: { ...REQUEST }, readView: view });
    assert.equal(outcome.state, "failed");
    assert.equal(outcome.diagnostics[0].reason, "transport-evidence-absent");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
