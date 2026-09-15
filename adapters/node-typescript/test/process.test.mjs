/**
 * #43 kernel suite: real one-shot child process tests — stdin and file
 * transports, runtime metadata, malformed bytes, refusals, and
 * no-side-effect guarantees. The only program this suite spawns is the
 * Node interpreter running the kernel script (or the lekalo binary in
 * the dedicated confined-runtime tests). Package managers, shells, and
 * project scripts are never executed.
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";

import {
  canonicalJson,
  createKernel,
  decodeJsonDocument,
  validateRequestObject,
} from "../adapter.mjs";
import {
  canonical,
  deterministicDescribeRequest,
  dispose,
  materializeFixtureProject,
  projectProfile,
  runAdapter,
  snapshotProject,
} from "./helpers.mjs";

test("one-shot stdin describe produces exactly one valid response", () => {
  const request = deterministicDescribeRequest();
  const result = runAdapter(Buffer.from(JSON.stringify(request), "utf8"));
  assert.equal(result.status, 0, `stderr=${result.stderr}`);
  const envelope = JSON.parse(result.stdout.toString("utf8"));
  assert.equal(envelope.status, "ok");
  assert.equal(envelope.operation, "describe");
  assert.equal(envelope.request_id, request.request_id);
  assert.equal(envelope.protocol_version, "0.2.16");
  // The kernel writes the canonical bytes only: no trailing whitespace noise.
  assert.equal(result.stdout.toString("utf8"), canonicalJson(envelope));
  assert.equal(result.stderr.toString("utf8"), "");
});

test("two separate invocations prove no stale handshake state in the child", () => {
  const request = deterministicDescribeRequest();
  const first = runAdapter(Buffer.from(JSON.stringify(request), "utf8"));
  const second = runAdapter(Buffer.from(JSON.stringify(request), "utf8"));
  assert.equal(first.status, 0);
  assert.equal(second.status, 0);
  assert.equal(first.stdout.toString("utf8"), second.stdout.toString("utf8"),
    "byte-identical responses for identical requests");
});

test("the file transport works: --lekalo-request-file PATH", () => {
  const dir = mkdtempSync(tmpdir() + "/lekalo-k43-file-");
  const requestPath = dir + "/request.json";
  writeFileSync(requestPath, JSON.stringify(deterministicDescribeRequest()));
  const result = runAdapter(Buffer.alloc(0), ["--lekalo-request-file", requestPath]);
  assert.equal(result.status, 0, `stderr=${result.stderr}`);
  const envelope = JSON.parse(result.stdout.toString("utf8"));
  assert.equal(envelope.status, "ok");
  rmSync(dir, { recursive: true, force: true });
});

test("malformed bytes exit nonzero with bounded stderr and no fabricated envelope", () => {
  const cases = [
    Buffer.from("not json at all", "utf8"),
    Buffer.from([0x7b, 0xfd, 0xf7, 0xff, 0x7d]),
    Buffer.from('{"protocol":"lekalo.target/v1"}', "utf8"),
    Buffer.from('{"a":1}{"b":2}', "utf8"),
  ];
  for (const bytes of cases) {
    const result = runAdapter(bytes);
    assert.notEqual(result.status, 0, "malformed input must not exit zero");
    assert.equal(result.stdout.toString("utf8"), "",
      "no synthetic envelope may be emitted for input without a valid echo identity");
    const stderr = result.stderr.toString("utf8");
    assert.ok(stderr.length <= 512, "stderr stays bounded");
    assert.match(stderr, /lekalo-target-node-typescript/);
  }
});

test("duplicate decoded keys exit nonzero, never collapse", () => {
  const bytes = Buffer.from(
    '{"protocol":"lekalo.target/v1","protocol_version":"0.2.16","operation":"describe",'
    + '"request_id":"req-1b2c9180d660f980e22741574e778fa6dcd11fd7a960b9ad89f7a48485e5988c",'
    + '"project_root":".","operation":"scan"}',
    "utf8",
  );
  const result = runAdapter(bytes);
  assert.notEqual(result.status, 0);
  assert.equal(result.stdout.toString("utf8"), "");
});

test("unknown transport arguments are refused", () => {
  const result = runAdapter(
    Buffer.from(JSON.stringify(deterministicDescribeRequest()), "utf8"),
    ["--lekalo-request-file"],
  );
  assert.notEqual(result.status, 0, "a dangling request-file flag is a transport refusal");
});

test("the metadata probe prints exact runtime fields on stdout", () => {
  const result = runAdapter(Buffer.alloc(0), ["--version-json"]);
  assert.equal(result.status, 0);
  const probe = JSON.parse(result.stdout.toString("utf8"));
  assert.equal(probe.adapter.id, "lekalo-target-node-typescript");
  assert.equal(probe.adapter.version, "0.3.0");
  assert.match(probe.adapter.digest, /^sha256:[0-9a-f]{64}$/);
  assert.equal(probe.node, process.versions.node);
});

test("an unsupported operation through the wire yields one valid error envelope", () => {
  const request = {
    protocol: "lekalo.target/v1",
    protocol_version: "0.2.16",
    operation: "scan",
    request_id: "req-1b2c9180d660f980e22741574e778fa6dcd11fd7a960b9ad89f7a48485e5988c",
    project_root: ".",
    profile: "standalone",
  };
  const result = runAdapter(Buffer.from(JSON.stringify(request), "utf8"));
  assert.equal(result.status, 0, `stderr=${result.stderr}`);
  const envelope = JSON.parse(result.stdout.toString("utf8"));
  assert.equal(envelope.status, "error");
  assert.equal(envelope.error.class, "unsupported");
  assert.equal(envelope.error.code, "operation-unsupported");
  assert.equal(envelope.error.partial, false);
  assert.equal("result" in envelope, false);
});

test("operation framing never changes source/config: byte snapshots around every outcome", () => {
  const { project, root } = materializeFixtureProject("noeffect");
  const before = snapshotProject(project);
  // describe framing
  const describe = runAdapter(
    Buffer.from(JSON.stringify(deterministicDescribeRequest()), "utf8"),
    [],
    { cwd: project },
  );
  assert.equal(describe.status, 0);
  // unsupported scan framing
  const scan = runAdapter(
    Buffer.from(JSON.stringify({
      ...deterministicDescribeRequest(),
      operation: "scan",
      profile: "standalone",
    }), "utf8"),
    [],
    { cwd: project },
  );
  assert.equal(scan.status, 0);
  const after = snapshotProject(project);
  assert.deepEqual(after, before, "no file changed under any framing");
  // A poison sentinel directory would only appear if package scripts ran.
  assert.equal(before["node_modules/.package-lock.json"], undefined);
  dispose(root);
});

test("the production kernel never emits a progress member", () => {
  const request = deterministicDescribeRequest();
  const result = runAdapter(Buffer.from(JSON.stringify(request), "utf8"));
  const envelope = JSON.parse(result.stdout.toString("utf8"));
  assert.equal("progress" in envelope, false);
  assert.equal(envelope.capabilities.progress, false);
});
