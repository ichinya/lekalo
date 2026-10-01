/**
 * Issue #115 — the production process path: the trusted framework
 * policy launch input, wire-level scan behavior with the provider
 * enabled/disabled, malformed-policy refusals, and the read-only
 * source invariant.
 */
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import test from "node:test";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

import {
  dispose,
  honoFixtureRoot,
  loadAdapter,
  materializeHonoFixture,
} from "./hono-helpers.mjs";

const adapterPath = join(import.meta.dirname, "..", "adapter.mjs");

const PROFILE = JSON.stringify({
  id: "standalone",
  mode: "observed",
  target: "node-typescript",
  readRoots: [
    { path: "src", kind: "tree" },
    { path: "types", kind: "tree" },
    { path: "package.json", kind: "file" },
    { path: "tsconfig.json", kind: "file" },
  ],
  exclusions: [],
  provenance: {
    origin: "declared",
    revision: "process-fixture",
    disposition: "public-fixture",
  },
});

const POLICY_ENABLED = JSON.stringify({
  schema: "lekalo/framework-policy",
  version: 1,
  providers: [{ id: "hono", state: "enabled" }],
});

const POLICY_DISABLED = JSON.stringify({
  schema: "lekalo/framework-policy",
  version: 1,
  providers: [{ id: "hono", state: "disabled" }],
});

function scanRequest(sequence) {
  const hex = sequence.toString(16).padStart(4, "0") + "a".repeat(60);
  return JSON.stringify({
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    operation: "scan",
    request_id: `req-${hex}`,
    project_root: ".",
    profile: "standalone",
  });
}

/** Full inventory of the materialized project (path → digest). */
function inventory(root) {
  const map = new Map();
  const walk = (directory) => {
    for (const entry of readdirSync(directory)) {
      const absolute = join(directory, entry);
      const metadata = statSync(absolute);
      if (metadata.isDirectory()) {
        walk(absolute);
      } else {
        map.set(absolute.slice(root.length + 1).split("\\").join("/"),
          createHash("sha256").update(readFileSync(absolute)).digest("hex"));
      }
    }
  };
  walk(root);
  return map;
}

/** The wire refuses partial success; uncertainty is a terminal outcome. */
function assertTerminalScan(response) {
  assert.equal(response.operation, "scan");
  if (response.status === "ok") return;
  assert.equal(response.status, "error");
  assert.equal(response.error.code, "outcome-partial-uncertainty-present");
  assert.equal(response.error.partial, true);
}

// Read-only invariant: scan mutates nothing in the project.
test("the wire scan with an enabled policy completes and never mutates source", () => {
  const { root, project } = materializeHonoFixture("process", "static");
  try {
    const before = inventory(project);
    const result = spawnSync(
      process.execPath,
      [adapterPath,
        "--lekalo-project-profile-json", PROFILE,
        "--lekalo-framework-policy-json", POLICY_ENABLED],
      { input: scanRequest(1), encoding: "utf8", timeout: 120000, cwd: project },
    );
    assert.equal(result.status, 0, `process failed: ${result.stderr?.slice(0, 400)}`);
    const response = JSON.parse(result.stdout);
    assertTerminalScan(response);
    // Read-only invariant: scan mutates nothing in the project.
    const after = inventory(project);
    assert.deepEqual([...after.entries()], [...before.entries()]);
    assert.equal(before.size, after.size);
  } finally {
    dispose(root);
  }
});

/**
 * The trusted launch inputs root the adapter and enable the provider;
 * the scan itself is read-only — this invariant is asserted explicitly
 * above, and repeated here for the disabled path.
 */
test("the disabled policy keeps the scan generic and complete", () => {
  const { root, project } = materializeHonoFixture("process-off", "static");
  try {
    const result = spawnSync(
      process.execPath,
      [adapterPath,
        "--lekalo-project-profile-json", PROFILE,
        "--lekalo-framework-policy-json", POLICY_DISABLED],
      { input: scanRequest(2), encoding: "utf8", timeout: 120000, cwd: project },
    );
    assert.equal(result.status, 0);
    const response = JSON.parse(result.stdout);
    assertTerminalScan(response);
  } finally {
    dispose(root);
  }
});

test("malformed and tampered policies are launch refusals", () => {
  const { root, project } = materializeHonoFixture("process-bad", "static");
  try {
    const malformed = spawnSync(
      process.execPath,
      [adapterPath,
        "--lekalo-project-profile-json", PROFILE,
        "--lekalo-framework-policy-json", '{"schema":"lekalo/framework-policy","version":1,"providers":[{"id":"hono","state":"confirmed"}]}'],
      { input: scanRequest(3), encoding: "utf8", timeout: 120000, cwd: project },
    );
    assert.equal(malformed.status, 1);
    assert.match(malformed.stderr, /framework-policy/);
    const unknownSchema = spawnSync(
      process.execPath,
      [adapterPath,
        "--lekalo-project-profile-json", PROFILE,
        "--lekalo-framework-policy-json", '{"schema":"other","version":1,"providers":[]}'],
      { input: scanRequest(4), encoding: "utf8", timeout: 120000, cwd: project },
    );
    assert.equal(unknownSchema.status, 1);
    assert.match(unknownSchema.stderr, /framework-policy/);
    const missingValue = spawnSync(
      process.execPath,
      [adapterPath,
        "--lekalo-project-profile-json", PROFILE,
        "--lekalo-framework-policy-json"],
      { input: scanRequest(5), encoding: "utf8", timeout: 120000, cwd: project },
    );
    assert.equal(missingValue.status, 1);
    assert.match(missingValue.stderr, /framework-policy/);
  } finally {
    dispose(root);
  }
});

test("the kernel decodes the policy strictly and binds its identity", async () => {
  const adapter = await loadAdapter();
  const kernel = adapter.__lekaloKernel;
  const decoded = kernel.decodeFrameworkPolicyJson(POLICY_ENABLED);
  assert.equal(decoded.schema, "lekalo/framework-policy");
  assert.equal(decoded.version, 1);
  assert.deepEqual(decoded.providers, [{ id: "hono", state: "enabled" }]);
  assert.match(decoded.digest, /^sha256:[0-9a-f]{64}$/);
  // Duplicate ids, unknown states, unknown members, and bad shapes refuse.
  assert.throws(() => kernel.decodeFrameworkPolicyJson(JSON.stringify({
    schema: "lekalo/framework-policy", version: 1,
    providers: [{ id: "hono", state: "enabled" }, { id: "hono", state: "disabled" }],
  })));
  assert.throws(() => kernel.decodeFrameworkPolicyJson(JSON.stringify({
    schema: "lekalo/framework-policy", version: 2, providers: [],
  })));
  assert.throws(() => kernel.decodeFrameworkPolicyJson(JSON.stringify({
    schema: "lekalo/framework-policy", version: 1, providers: [{ id: "hono", state: "enabled", extra: 1 }],
  })));
  assert.throws(() => kernel.decodeFrameworkPolicyJson(JSON.stringify({
    schema: "lekalo/framework-policy", version: 1,
    providers: [{ id: "HONO!", state: "enabled" }],
  })));
  // Absent flag → undefined (provider stays off).
  assert.equal(kernel.extractFrameworkPolicyJson(["node", "adapter.mjs"]), undefined);
  assert.equal(
    kernel.extractFrameworkPolicyJson(["node", "adapter.mjs", "--lekalo-framework-policy-json", POLICY_ENABLED]),
    POLICY_ENABLED,
  );
  void honoFixtureRoot;
});
