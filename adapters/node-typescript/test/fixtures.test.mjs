/**
 * #43 kernel suite: closed synthetic fixture custody, repeatable process
 * output, injection of fake scanner/runner outcomes from the committed
 * fixture documents, and public-fixture privacy (no private consumer
 * identity, no absolute paths, no canary leakage into public bytes).
 */
import assert from "node:assert/strict";
import { test } from "node:test";

import { readFileSync } from "node:fs";

import {
  buildResponse,
  canonicalJson,
  createKernel,
  decodeJsonDocument,
  validateRequestObject,
} from "../main.mjs";
import {
  adapterPath,
  deterministicDescribeRequest,
  dispose,
  expectedNormalization,
  extensionResults,
  fixtureRoot,
  materializeFixtureProject,
  projectProfile,
  runAdapter,
  sha256Bytes,
  snapshotProject,
} from "./helpers.mjs";

const REQUEST_ID = "req-1b2c9180d660f980e22741574e778fa6dcd11fd7a960b9ad89f7a48485e5988c";

/** The committed request fixture decodes and validates exactly. */
test("the committed describe request fixture is byte-valid protocol input", () => {
  const bytes = readFileSync(fixtureRoot + "/requests/describe.json");
  const request = validateRequestObject(decodeJsonDocument(bytes));
  assert.equal(request.operation, "describe");
  assert.equal(request.request_id, REQUEST_ID);
});

test("every committed invalid request vector is refused by the strict decoder", () => {
  const vectors = JSON.parse(readFileSync(fixtureRoot + "/requests/invalid.json", "utf8")).cases;
  for (const [name, vector] of Object.entries(vectors)) {
    const bytes = typeof vector === "string"
      ? Buffer.from(vector, "utf8")
      : Buffer.from(vector.bytesBase64, "base64");
    assert.throws(
      () => validateRequestObject(decodeJsonDocument(bytes)),
      (error) => error instanceof Error && error.name === "RequestRefusal",
      `vector ${name} must be refused`,
    );
  }
});

function kernelWithFixtureOutcome(kind, sink) {
  const scannerOutcome = extensionResults().scanner[kind];
  return createKernel({
    resolvedProjectProfile: projectProfile(),
    extensionRegistry: [{
      id: "fixture-scanner",
      version: "0.1.0",
      operations: ["scan"],
      invoke: () => scannerOutcome,
    }],
    localEvidenceSink: sink,
  });
}

const scanRequest = {
  protocol: "lekalo.target/v1",
  protocol_version: "0.2.16",
  operation: "scan",
  request_id: REQUEST_ID,
  project_root: ".",
  profile: "standalone",
};

test("fixture outcomes: complete succeeds; every uncertain state stays honest", () => {
  const { project, root } = materializeFixtureProject("fx-outcomes");
  for (const kind of ["complete", "partial", "unknown", "ambiguous", "malformed", "error", "localOnlyCanary"]) {
    const sink = [];
    const kernel = kernelWithFixtureOutcome(kind, sink);
    const dispatched = kernel.dispatch(scanRequest, { permittedProjectRoot: project });
    const expected = expectedNormalization().outcomes[kind];
    assert.equal(dispatched.response.status, expected.publicOutcome.status, kind);
    if (expected.publicOutcome.status === "error") {
      assert.equal(dispatched.response.error.class, expected.publicOutcome.class ?? dispatched.response.error.class, kind);
      assert.equal("result" in dispatched.response, false, `${kind}: no success is claimed`);
    }
    // The local sink holds the complete outcome including any canary.
    assert.equal(sink.length, 1, kind);
    assert.equal(sink[0].state, expected.internalState, kind);
  }
  dispose(root);
});

test("the local-only canary stays out of the public response bytes", () => {
  const { project, root } = materializeFixtureProject("fx-canary");
  const sink = [];
  const kernel = kernelWithFixtureOutcome("localOnlyCanary", sink);
  const dispatched = kernel.dispatch(scanRequest, { permittedProjectRoot: project });
  const publicBytes = canonicalJson(dispatched.response);
  const canary = expectedNormalization().outcomes.localOnlyCanary.canary;
  assert.equal(canary, "LOCAL-ONLY-CANARY-4f2a");
  assert.equal(publicBytes.includes(canary), false, "canary must never be public");
  assert.equal(canonicalJson(sink).includes(canary), true, "canary is preserved locally");
  dispose(root);
});

test("a full source span and local reference survive internally, never truncated", () => {
  const outcome = extensionResults().scanner.complete;
  const sink = [];
  const kernel = createKernel({
    resolvedProjectProfile: projectProfile(),
    extensionRegistry: [{
      id: "fixture-scanner",
      version: "0.1.0",
      operations: ["scan"],
      invoke: () => outcome,
    }],
    localEvidenceSink: sink,
  });
  const { project, root } = materializeFixtureProject("fx-spans");
  const dispatched = kernel.dispatch(scanRequest, { permittedProjectRoot: project });
  assert.equal(dispatched.internal.evidence.sourceSpans[0].endColumn, 2);
  assert.equal(dispatched.internal.evidence.localReference.value, "fixtureGreeting");
  assert.equal(canonicalJson(dispatched.response).includes("fixtureGreeting"), false);
  void sink;
  dispose(root);
});

test("repeated process requests over the fixture produce byte-identical output", () => {
  const request = deterministicDescribeRequest();
  const outputs = new Set();
  for (let index = 0; index < 3; index += 1) {
    const result = runAdapter(Buffer.from(JSON.stringify(request), "utf8"));
    assert.equal(result.status, 0);
    outputs.add(result.stdout.toString("utf8"));
  }
  assert.equal(outputs.size, 1, "the same request identity and bytes repeat byte for byte");
});

test("fixture source/config bytes are preserved across success and refusal", () => {
  const { project, root } = materializeFixtureProject("fx-custody");
  const before = snapshotProject(project);
  // success path
  const ok = kernelWithFixtureOutcome("complete", null)
    .dispatch(scanRequest, { permittedProjectRoot: project });
  assert.equal(ok.response.status, "ok");
  // refusal path
  const broken = createKernel({
    resolvedProjectProfile: {
      ...projectProfile(),
      readRoots: [{ path: "src", kind: "tree" }, { path: "missing-late", kind: "tree" }],
    },
    extensionRegistry: [{
      id: "fixture-scanner",
      version: "0.1.0",
      operations: ["scan"],
      invoke: () => extensionResults().scanner.complete,
    }],
  });
  const refused = broken.dispatch(scanRequest, { permittedProjectRoot: project });
  assert.equal(refused.response.status, "error");
  const after = snapshotProject(project);
  assert.deepEqual(after, before);
  dispose(root);
});

test("fixtures carry no private-consumer markers, absolute paths, or credentials", () => {
  const files = [
    "profiles/standalone.valid.json",
    "profiles/invalid.json",
    "requests/describe.json",
    "requests/invalid.json",
    "extensions/results.json",
    "expected/normalization.json",
  ];
  const needles = [
    "C:\\", "C:/", "/home/", "/Users/", "file://",
    "ghp_", "sk-", "-----BEGIN",
    "company", "internal-client", "production",
  ];
  for (const relative of files) {
    const text = readFileSync(fixtureRoot + "/" + relative, "utf8");
    for (const needle of needles) {
      assert.equal(text.includes(needle), false, `${relative} contains ${needle}`);
    }
  }
});

test("adapter entry digest is stable across reads within one process", () => {
  const first = JSON.parse(
    runAdapter(Buffer.alloc(0), ["--version-json"]).stdout.toString("utf8"),
  );
  const second = JSON.parse(
    runAdapter(Buffer.alloc(0), ["--version-json"]).stdout.toString("utf8"),
  );
  assert.equal(first.adapter.digest, second.adapter.digest);
  // The generated artifact IS the launched entry: its digest is the
  // sha256 of its own bytes (holds for the interim generation and the
  // final compiler bundle alike).
  assert.equal(first.adapter.digest, entryDigestOfArtifactBytes());
});

import { createHash } from "node:crypto";
function entryDigestOfArtifactBytes() {
  return "sha256:" + createHash("sha256").update(readFileSync(adapterPath)).digest("hex");
}
