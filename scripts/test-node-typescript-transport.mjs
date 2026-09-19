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
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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
const sha256Text = (text) =>
  "sha256:" + createHash("sha256").update(text, "utf8").digest("hex");

const PROFILE = {
  id: "standalone",
  mode: "observed",
  target: "node-typescript",
  readRoots: [{ kind: "tree", path: ".lekalo/cache/transport" }],
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
  const root = mkdtempSync(join(tmpdir(), "lekalo-transport-gate-"));
  const dir = join(root, ".lekalo", "cache", "transport");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "planner.json"), evidenceBytes, "utf8");
  return root;
}

test("gate: the descriptor claims the transport capability honestly", () => {
  const descriptor = transportExtensionDescriptor();
  assert.equal(descriptor.namedCapabilities[TRANSPORT_CAPABILITY], "partial");
  assert.equal(descriptor.namedCapabilities["generate.openapi"], "unsupported");
  assert.deepEqual(descriptor.writeRoots, [ROUTE_WRITE_ROOT]);
});

test("gate: the plan is deterministic and byte-pinned across repeats", () => {
  const evidence = JSON.parse(evidenceBytes);
  const first = planRouteLayer(evidence, "planner");
  for (let index = 0; index < 3; index += 1) {
    assert.deepEqual(planRouteLayer(evidence, "planner"), first);
  }
  assert.equal(first.writes[0].path, "src/routes/planner.routes.ts");
  assert.equal(first.writes[0].sha256, sha256Text(first.bodies[0].bytes));
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
  const plan = planRouteLayer(evidence, "planner");
  assert.equal(plan.notes.length, 1);
  assert.equal(plan.notes[0].capability, "streaming");
  assert.equal(plan.notes[0].state, "unsupported");
});

test("gate: an absent evidence file refuses honestly", () => {
  const root = mkdtempSync(join(tmpdir(), "lekalo-transport-none-"));
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
