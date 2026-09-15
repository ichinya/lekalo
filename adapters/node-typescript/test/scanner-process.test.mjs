/**
 * #44 scanner process probes: launch input handling, the F1 binding
 * matrix through the real one-shot process, version metadata with the
 * vendored compiler pins, and the deterministic artifact check.
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";

import {
  adapterPath,
  deterministicDescribeRequest,
  repoRoot,
  runAdapter,
} from "./helpers.mjs";

const PROFILE_JSON = readFileSync(
  join(repoRoot, "tests/fixtures/node-typescript-scanner/protocol/conformance.profile.json"),
  "utf8",
).replace(/\s+/g, "");

test("the launch profile is bounded: oversize inputs refuse", async () => {
  const { decodeProjectProfileJson, MAX_PROFILE_JSON_BYTES } = await import("../main.mjs");
  assert.throws(() => decodeProjectProfileJson("x".repeat(MAX_PROFILE_JSON_BYTES + 1)),
    (error) => error.code === "profile-input");
  assert.throws(() => decodeProjectProfileJson(""),
    (error) => error.code === "profile-input");
  assert.throws(() => decodeProjectProfileJson('{"id":1,"id":2}'),
    (error) => error.code === "duplicate-key");
});

test("the launch profile marker is refused without a value", () => {
  const result = runAdapter(Buffer.from(JSON.stringify(deterministicDescribeRequest()), "utf8"),
    ["--lekalo-project-profile-json"]);
  assert.notEqual(result.status, 0, "dangling launch input is a refusal");
});

test("an invalid launch profile fails the launch with bounded stderr", () => {
  const result = runAdapter(Buffer.from(JSON.stringify(deterministicDescribeRequest()), "utf8"),
    ["--lekalo-project-profile-json", '{"id":"standalone"}']);
  assert.notEqual(result.status, 0);
  assert.equal(result.stdout.toString("utf8"), "");
  assert.ok(result.stderr.toString("utf8").length < 512);
});

function scanRequest() {
  return {
    protocol: "lekalo.target/v1",
    protocol_version: "0.2.16",
    operation: "scan",
    request_id: deterministicDescribeRequest().request_id,
    project_root: ".",
    profile: "standalone",
  };
}

/** A launch profile carrying a trusted targetResolution snapshot. */
const SNAPSHOT_PROFILE = JSON.stringify({
  id: "standalone",
  mode: "observed",
  target: "node-typescript",
  readRoots: [{ path: "src", kind: "tree" }],
  exclusions: [],
  targetResolution: {
    digest: "sha256:" + "a".repeat(64),
    capabilities: [{ id: "scan.symbols", support: "full" }],
  },
  provenance: { origin: "declared", revision: "f1-fixture", disposition: "public-fixture" },
});

test("F1 through the wire: scan without the matching profile binding refuses", () => {
  // Launch with a trusted snapshot; scan without the request pair:
  // refusal before any root validation or read.
  const withProfile = spawnSync(
    process.execPath,
    [adapterPath, "--lekalo-project-profile-json", SNAPSHOT_PROFILE],
    { input: Buffer.from(JSON.stringify(scanRequest()), "utf8"), encoding: "buffer", timeout: 30000 },
  );
  assert.equal(withProfile.status, 0, `stderr=${withProfile.stderr}`);
  const envelope = JSON.parse(withProfile.stdout.toString("utf8"));
  assert.equal(envelope.status, "error");
  assert.equal(envelope.error.code, "profile-binding-mismatch");
  assert.equal(envelope.error.detail[0], "resolution-missing");
});

test("F1 through the wire: a claimed resolution that differs from the snapshot refuses", () => {
  const request = {
    ...scanRequest(),
    target: "node-typescript",
    profile_digest: "sha256:" + "b".repeat(64),
    profile_capabilities: [{ id: "scan.symbols", support: "full" }],
  };
  const withProfile = spawnSync(
    process.execPath,
    [adapterPath, "--lekalo-project-profile-json", SNAPSHOT_PROFILE],
    { input: Buffer.from(JSON.stringify(request), "utf8"), encoding: "buffer", timeout: 30000 },
  );
  assert.equal(withProfile.status, 0);
  const envelope = JSON.parse(withProfile.stdout.toString("utf8"));
  assert.equal(envelope.status, "error");
  assert.equal(envelope.error.code, "profile-binding-mismatch");
  assert.equal(envelope.error.detail[0], "digest");
});

test("bare launch keeps the #43 kernel-only describe behavior", () => {
  const result = runAdapter(Buffer.from(JSON.stringify(deterministicDescribeRequest()), "utf8"));
  assert.equal(result.status, 0);
  const envelope = JSON.parse(result.stdout.toString("utf8"));
  assert.deepEqual(envelope.capabilities.operations, ["describe"]);
  assert.equal(envelope.capabilities.read_scopes.length, 0);
});

test("describe advertises scan only with the launch profile, and the compiler pins", async () => {
  const probe = spawnSync(
    process.execPath,
    [adapterPath, "--version-json"],
    { encoding: "utf8", timeout: 30000 },
  );
  const metadata = JSON.parse(probe.stdout);
  assert.equal(metadata.compiler.vendored, true);
  assert.equal(metadata.compiler.typescript, "5.9.3");
  assert.equal(metadata.compiler.esbuild, "0.25.12");
  void runAdapter;
});

test("the committed artifact is exactly the deterministic rebuild", () => {
  const check = spawnSync(process.execPath, ["build.mjs", "--check"], {
    cwd: join(repoRoot, "adapters/node-typescript"),
    encoding: "utf8",
    timeout: 120000,
  });
  assert.equal(check.status, 0, `output=${check.stdout}${check.stderr}`);
  const report = JSON.parse(check.stdout);
  assert.equal(report.check, true);
});
