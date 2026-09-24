#!/usr/bin/env node
// Issue #46 OpenAPI generator gate: the generation composite claims
// the `generate.openapi` capability, the document render over the
// committed conformance evidence is byte-stable across repeats, the
// write plan carries the document plus the ownership and pointer
// sidecars, the apply publishes exactly the declared plan, and the
// verify posture recomputes without drift and reports a maintained
// edit. Driven through the production kernel. Node built-ins only.
import assert from "node:assert/strict";
import { test } from "node:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { createKernel, validateResolvedProjectProfile } from "../adapters/node-typescript/src/kernel.mjs";
import { descriptor as generationComposite } from "../adapters/node-typescript/src/generation-composite.mjs";
import { OPENAPI_CAPABILITY } from "../adapters/node-typescript/src/transport-extension.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const evidenceBytes = readFileSync(
  join(repoRoot, "tests/fixtures/adapter-conformance/inputs/transport-minimal.json"),
  "utf8",
);
const irBytes = readFileSync(
  join(repoRoot, "tests/fixtures/adapter-conformance/inputs/ir-minimal.json"),
  "utf8",
);

const PROFILE = {
  id: "standalone",
  mode: "observed",
  target: "node-typescript",
  readRoots: [
    { kind: "tree", path: ".lekalo/cache/transport" },
    { kind: "tree", path: ".lekalo/cache/ir" },
    { kind: "tree", path: "docs" },
  ],
  exclusions: [],
  provenance: { origin: "declared", revision: "test", disposition: "public-fixture" },
};

const REQUEST = {
  protocol: "lekalo.target/v1",
  protocol_version: "0.3.2",
  operation: "generate",
  request_id: "req-openapi-gate-1",
  project_root: ".",
  target: "node-typescript",
  profile: "standalone",
  dry_run: true,
  ir_path: ".lekalo/cache/ir/planner.json",
};

function evidenceProject() {
  const root = mkdtempSync(join(tmpdir(), "lekalo-openapi-gate-"));
  const evidenceDir = join(root, ".lekalo", "cache", "transport");
  const irDir = join(root, ".lekalo", "cache", "ir");
  mkdirSync(join(root, "docs"), { recursive: true });
  mkdirSync(evidenceDir, { recursive: true });
  mkdirSync(irDir, { recursive: true });
  writeFileSync(join(evidenceDir, "planner.json"), evidenceBytes, "utf8");
  writeFileSync(join(irDir, "planner.json"), irBytes, "utf8");
  return root;
}

function kernelFor() {
  return createKernel({
    resolvedProjectProfile: validateResolvedProjectProfile({ ...PROFILE }),
    extensionRegistry: [generationComposite],
  });
}

function dispatch(kernel, root, request) {
  const { response } = kernel.dispatch(request, { permittedProjectRoot: root });
  return response;
}

test("gate: the composite claims generate.openapi as a partial capability", () => {
  assert.equal(generationComposite.operations.includes("generate"), true);
  assert.equal(generationComposite.namedCapabilities[OPENAPI_CAPABILITY], "partial");
});

test("gate: the dry-run plan is deterministic across repeats", () => {
  const root = evidenceProject();
  try {
    const kernel = kernelFor();
    const first = dispatch(kernel, root, REQUEST);
    const second = dispatch(kernel, root, {
      ...REQUEST,
      request_id: "req-openapi-gate-2",
    });
    assert.equal(first.status, "ok", JSON.stringify(first.error ?? {}));
    const writes = first.writes ?? [];
    assert.ok(writes.length >= 3, "document plus two sidecars");
    assert.deepEqual(writes, second.writes ?? []);
    assert.match(first.evidence?.plan_id ?? "", /^plan-[0-9a-f]{64}$/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("gate: the plan carries the document and the ownership/pointer sidecars", () => {
  const root = evidenceProject();
  try {
    const kernel = kernelFor();
    const response = dispatch(kernel, root, REQUEST);
    const writes = response.writes ?? [];
    const paths = writes.map((write) => write.path);
    assert.ok(paths.includes("docs/openapi.yaml"), "the document");
    assert.ok(
      paths.includes("docs/openapi.ownership.json"),
      "the ownership manifest",
    );
    assert.ok(paths.includes("docs/openapi.map.json"), "the pointer sidecar");
    for (const write of writes) {
      assert.match(write.sha256, /^sha256:[0-9a-f]{64}$/);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("gate: apply publishes the plan and verify recomputes without drift", () => {
  const root = evidenceProject();
  try {
    const kernel = kernelFor();
    const dry = dispatch(kernel, root, REQUEST);
    const apply = dispatch(kernel, root, {
      ...REQUEST,
      request_id: "req-openapi-gate-3",
      dry_run: false,
      plan_id: dry.evidence?.plan_id,
    });
    assert.equal(apply.status, "ok", JSON.stringify(apply.error ?? {}));
    assert.deepEqual(apply.writes, dry.writes, "the apply echo binds the plan");
    const documentPath = join(root, "docs", "openapi.yaml");
    assert.ok(existsSync(documentPath), "the applied document exists");
    const verify = dispatch(kernel, root, {
      ...REQUEST,
      request_id: "req-openapi-gate-4",
      operation: "verify",
    });
    assert.equal(verify.status, "ok", JSON.stringify(verify.error ?? {}));
    const findings = verify.result?.findings ?? verify.findings ?? [];
    assert.deepEqual(
      findings.filter((finding) => finding.code === "openapi.drift"),
      [],
      "no drift over the applied bytes",
    );
    // A maintained edit drifts.
    const maintained = readFileSync(documentPath, "utf8").replace(
      "Success response.",
      "Maintained response.",
    );
    writeFileSync(documentPath, maintained, "utf8");
    const drifted = dispatch(kernel, root, {
      ...REQUEST,
      request_id: "req-openapi-gate-5",
      operation: "verify",
    });
    const driftFindings = (drifted.result?.findings ?? drifted.findings ?? []).filter(
      (finding) => finding.code === "openapi.drift",
    );
    assert.ok(driftFindings.length > 0, "the maintained edit drifts");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
