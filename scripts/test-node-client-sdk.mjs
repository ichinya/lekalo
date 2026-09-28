#!/usr/bin/env node
// Issue #72 client-SDK generator gate: the generation composite claims
// the `generate.client-sdk` capability, the render over the committed
// SDK evidence is byte-stable across repeats, the write plan carries
// the client module plus the compatibility and ownership sidecars, the
// apply publishes exactly the declared plan, and the verify posture
// recomputes without drift and reports a maintained edit. Driven
// through the production kernel. Node built-ins only.
import assert from "node:assert/strict";
import { test } from "node:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { createKernel, validateResolvedProjectProfile } from "../adapters/node-typescript/src/kernel.mjs";
import { descriptor as generationComposite } from "../adapters/node-typescript/src/generation-composite.mjs";
import { CLIENT_SDK_CAPABILITY } from "../adapters/node-typescript/src/client-sdk-gen.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const goldenBytes = readFileSync(
  join(repoRoot, "tests/fixtures/client-sdk/golden/planner.expect.json"),
  "utf8",
);

const PROFILE = {
  id: "standalone",
  mode: "observed",
  target: "node-typescript",
  readRoots: [
    { kind: "tree", path: ".lekalo/cache/client-sdk" },
    { kind: "tree", path: "src" },
  ],
  exclusions: [],
  provenance: { origin: "declared", revision: "test", disposition: "public-fixture" },
};

const REQUEST = {
  protocol: "lekalo.target/v1",
  protocol_version: "0.3.2",
  operation: "generate",
  request_id: "req-client-sdk-gate-1",
  project_root: ".",
  target: "node-typescript",
  profile: "standalone",
  dry_run: true,
  ir_path: ".lekalo/cache/client-sdk/planner.json",
};

function evidenceProject() {
  const root = realpathSync(mkdtempSync(join(tmpdir(), "lekalo-client-sdk-gate-")));
  const sdkDir = join(root, ".lekalo", "cache", "client-sdk");
  mkdirSync(sdkDir, { recursive: true });
  writeFileSync(join(sdkDir, "planner.json"), goldenBytes, "utf8");
  // The declared `src` read root must physically exist at kernel
  // creation.
  mkdirSync(join(root, "src"), { recursive: true });
  return root;
}

function bareProject() {
  const root = realpathSync(mkdtempSync(join(tmpdir(), "lekalo-client-sdk-gate-")));
  mkdirSync(join(root, ".lekalo", "cache", "client-sdk"), { recursive: true });
  mkdirSync(join(root, "src"), { recursive: true });
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

test("gate: the composite claims the client-SDK capability honestly", () => {
  assert.equal(generationComposite.namedCapabilities[CLIENT_SDK_CAPABILITY], "partial");
  assert.ok(
    generationComposite.writeScopes.includes("src/generated/node-typescript/clients/**"),
  );
});

test("gate: the dry-run plan is deterministic across repeats", () => {
  const root = evidenceProject();
  try {
    const kernel = kernelFor();
    const first = dispatch(kernel, root, REQUEST);
    assert.equal(first.status, "ok", JSON.stringify(first.error ?? {}));
    const writes = first.writes ?? [];
    assert.ok(writes.length >= 3, "the client module plus the sidecars");
    for (let index = 0; index < 2; index += 1) {
      const repeat = dispatch(kernel, root, {
        ...REQUEST,
        request_id: `req-client-sdk-gate-repeat-${index}`,
      });
      assert.equal(repeat.status, "ok");
      assert.deepEqual(repeat.writes, writes, "generation is deterministic");
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("gate: the plan carries the client module and the sidecars", () => {
  const root = evidenceProject();
  try {
    const kernel = kernelFor();
    const response = dispatch(kernel, root, REQUEST);
    const writes = response.writes ?? [];
    const paths = writes.map((write) => write.path).sort();
    assert.ok(
      paths.some((path) => path.endsWith("planner.client.ts")),
      `client module in plan: ${JSON.stringify(paths)}`,
    );
    assert.ok(
      paths.some((path) => path.endsWith("planner.client.go")),
      `the Go backend module is in the plan: ${JSON.stringify(paths)}`,
    );
    assert.ok(paths.some((path) => path.endsWith("planner.compatibility.json")));
    assert.ok(paths.some((path) => path.endsWith("planner.map.json")));
    // Every write stays inside the declared client write root.
    for (const path of paths) {
      assert.ok(path.startsWith("src/generated/node-typescript/clients/"), path);
    }
    // The plan is the dry-run authority: no file exists yet.
    assert.ok(
      !existsSync(join(root, "src/generated/node-typescript/clients/planner.client.ts")),
      "a dry run writes nothing",
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("gate: apply publishes the plan and verify recomputes without drift", () => {
  const root = evidenceProject();
  try {
    const kernel = kernelFor();
    const plan = dispatch(kernel, root, REQUEST);
    assert.equal(plan.status, "ok", JSON.stringify(plan.error ?? {}));
    const apply = dispatch(kernel, root, {
      ...REQUEST,
      request_id: "req-client-sdk-gate-apply",
      dry_run: false,
      plan_id: plan.evidence?.plan_id,
    });
    assert.equal(apply.status, "ok", JSON.stringify(apply.error ?? {}));
    const clientPath = join(root, "src/generated/node-typescript/clients/planner.client.ts");
    assert.ok(existsSync(clientPath), "the applied client module exists");
    // The emitted module injects transport and base URL; it never
    // hard-codes either.
    const text = readFileSync(clientPath, "utf8");
    assert.match(text, /export interface LekaloTransport/);
    assert.match(text, /export class LekaloClient/);
    assert.ok(!text.includes("http://") && !text.includes("https://"), "no hardcoded URL");
    // The applied compatibility sidecar binds the exact evidence
    // digest and never carries a timestamp or a path.
    const compatibility = JSON.parse(
      readFileSync(
        join(root, "src/generated/node-typescript/clients/planner.compatibility.json"),
        "utf8",
      ),
    );
    assert.equal(compatibility.projectId, "planner");
    assert.match(compatibility.sdkContract.digest, /^sha256:[0-9a-f]{64}$/);
    assert.ok(!/\d{4}-\d{2}-\d{2}T/.test(JSON.stringify(compatibility)), "no timestamps");
    // Verify recomputes without drift.
    const verify = dispatch(kernel, root, {
      ...REQUEST,
      request_id: "req-client-sdk-gate-verify",
      operation: "verify",
    });
    assert.equal(verify.status, "ok", JSON.stringify(verify.error ?? {}));
    const findings = verify.result?.findings ?? verify.findings ?? [];
    assert.deepEqual(
      findings.filter((finding) => finding.code === "client.drift"),
      [],
      "no drift over the applied bytes",
    );
    // A maintained edit drifts.
    const maintained = readFileSync(clientPath, "utf8").replace(
      "never edit",
      "maintained edit",
    );
    writeFileSync(clientPath, maintained, "utf8");
    const drifted = dispatch(kernel, root, {
      ...REQUEST,
      request_id: "req-client-sdk-gate-drift",
      operation: "verify",
    });
    const driftFindings = (drifted.result?.findings ?? drifted.findings ?? []).filter(
      (finding) => finding.code === "client.drift",
    );
    assert.ok(driftFindings.length > 0, "the maintained edit drifts");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("gate: absent SDK evidence fails honestly when the profile reads the home", () => {
  const root = bareProject();
  try {
    const kernel = kernelFor();
    const response = dispatch(kernel, root, REQUEST);
    assert.equal(response.status, "error");
    // The failure is in-envelope and named: the SDK home is readable
    // but the evidence file is absent (or the sibling IR path is), and
    // every owner refuses explicitly instead of planning an empty
    // client.
    assert.match(
      JSON.stringify(response.error),
      /client-sdk-evidence-absent|ir-unreadable|ir-evidence-absent/,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
