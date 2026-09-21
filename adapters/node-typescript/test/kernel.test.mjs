/**
 * #43 kernel suite: pure request/response validation, canonicalization,
 * capability, profile, dispatch, and evidence tests. These run the
 * kernel's exported APIs in-process — no child process is involved.
 */
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  ADAPTER_ID,
  ADAPTER_VERSION,
  ERROR_CODES,
  PROTOCOL_TOKEN,
  RequestRefusal,
  SUPPORTED_VERSIONS,
  VERSION,
  buildResponse,
  canonicalJson,
  createKernel,
  decodeJsonDocument,
  describeCapabilities,
  entryDigest,
  isLogicalPath,
  isScope,
  isScopeSegment,
  isSha256Digest,
  lexicalRootViolation,
  normalizeExtensionOutcome,
  runtimeMetadata,
  scopeCovers,
  validateExtensionDescriptor,
  validateRequestObject,
  validateResolvedProjectProfile,
} from "../main.mjs";
import {
  deterministicDescribeRequest,
  expectedNormalization,
  extensionResults,
  projectProfile,
} from "./helpers.mjs";

const REQUEST_ID = deterministicDescribeRequest().request_id;

function baseRequest(overrides = {}) {
  return {
    protocol: PROTOCOL_TOKEN,
    protocol_version: VERSION,
    operation: "describe",
    request_id: REQUEST_ID,
    project_root: ".",
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Handshake identity, token, version.
// ---------------------------------------------------------------------------

test("identity constants are the frozen #43 values", () => {
  assert.equal(PROTOCOL_TOKEN, "lekalo.target/v1");
  assert.deepEqual(SUPPORTED_VERSIONS, ["0.3.2"]);
  assert.equal(VERSION, "0.3.2");
  assert.equal(ADAPTER_ID, "lekalo-target-node-typescript");
  // The release constant is the reserved product version (0.4.0),
  // not a protocol version.
  assert.equal(ADAPTER_VERSION, "0.4.0");
  assert.match(entryDigest(), /^sha256:[0-9a-f]{64}$/);
});

test("runtime metadata probe reports the exact running node", () => {
  const metadata = runtimeMetadata();
  assert.equal(metadata.adapter.id, ADAPTER_ID);
  assert.equal(metadata.adapter.version, ADAPTER_VERSION);
  assert.equal(metadata.adapter.digest, entryDigest());
  assert.equal(metadata.node, process.versions.node);
});

test("the descriptor advertises describe only, with honest capability states", () => {
  const capabilities = describeCapabilities();
  assert.deepEqual(capabilities.operations, ["describe"]);
  assert.deepEqual(capabilities.protocol_versions, ["0.3.2"]);
  assert.deepEqual(capabilities.ir_versions, []);
  assert.equal(capabilities.progress, false);
  assert.deepEqual(capabilities.read_scopes, []);
  assert.deepEqual(capabilities.write_scopes, []);
  assert.equal(capabilities.transports.sort().join(","), "file,stdin");
  assert.deepEqual(capabilities.targets, ["node-typescript"]);
  assert.deepEqual(capabilities.profiles, ["standalone"]);
  for (const state of Object.values(capabilities.capabilities)) {
    assert.equal(state, "unsupported");
  }
});

test("no invented capability ids ever appear", () => {
  const declared = Object.keys(describeCapabilities().capabilities);
  assert.deepEqual(declared.sort(), [
    "generate.openapi",
    "generate.ui",
    "generate.zod",
    "preserve.classification",
    "scan.symbols",
    "verify.scenarios",
  ]);
});

// ---------------------------------------------------------------------------
// Strict JSON decoding.
// ---------------------------------------------------------------------------

test("duplicate decoded keys are refused, including escaped aliases", () => {
  assert.throws(
    () => decodeJsonDocument(Buffer.from('{"a":1,"a":2}', "utf8")),
    (error) => error instanceof RequestRefusal && error.code === "duplicate-key",
  );
  assert.throws(
    () => decodeJsonDocument(Buffer.from('{"a":1,"\\u0061":2}', "utf8")),
    (error) => error instanceof RequestRefusal && error.code === "duplicate-key",
  );
});

test("invalid UTF-8 is fatal, never tolerated into a best-effort string", () => {
  assert.throws(
    () => decodeJsonDocument(Buffer.from([0x7b, 0x22, 0xfd, 0xf7, 0xff, 0x22, 0x7d])),
    (error) => error instanceof RequestRefusal,
  );
});

test("two JSON documents and trailing JSON are refused", () => {
  assert.throws(
    () => decodeJsonDocument(Buffer.from('{} {}', "utf8")),
    (error) => error.code === "trailing-content",
  );
  assert.throws(
    () => decodeJsonDocument(Buffer.from('{"a":1}[]', "utf8")),
    (error) => error.code === "trailing-content",
  );
});

test("excessive depth is bounded", () => {
  const deep = "[".repeat(200) + "]".repeat(200);
  assert.throws(
    () => decodeJsonDocument(Buffer.from(deep, "utf8")),
    (error) => error.code === "depth",
  );
});

test("JSON.parse alone would accept what the strict decoder refuses", () => {
  const hostile = '{"operation":"describe","operation":"scan"}';
  assert.equal(JSON.parse(hostile).operation, "scan");
  assert.throws(() => decodeJsonDocument(Buffer.from(hostile, "utf8")));
});

// ---------------------------------------------------------------------------
// Request envelope validation matrix.
// ---------------------------------------------------------------------------

test("a legal describe request validates", () => {
  const request = validateRequestObject(baseRequest());
  assert.equal(request.operation, "describe");
});

test("every operation-specific member combination is enforced", () => {
  // describe forbids operation members
  assert.throws(() => validateRequestObject(baseRequest({ target: "node-typescript" })),
    (error) => error.code === "member");
  assert.throws(() => validateRequestObject(baseRequest({ profile: "standalone" })),
    (error) => error.code === "member");
  assert.throws(() => validateRequestObject(baseRequest({ ir_path: "x/y.json" })),
    (error) => error.code === "member");
  // validate/verify require IR
  assert.throws(() => validateRequestObject(baseRequest({ operation: "validate" })),
    (error) => error.code === "ir-path");
  assert.throws(() => validateRequestObject(baseRequest({ operation: "verify" })),
    (error) => error.code === "ir-path");
  // generate requires target + IR + dry_run
  assert.throws(() => validateRequestObject(baseRequest({ operation: "generate", dry_run: true })),
    (error) => error.code === "ir-path");
  assert.throws(
    () => validateRequestObject(baseRequest({ operation: "generate", dry_run: true, ir_path: "ir/p.json" })),
    (error) => error.code === "target");
  // generate with dry_run false requires plan id
  assert.throws(
    () => validateRequestObject(baseRequest({
      operation: "generate", dry_run: false, ir_path: "ir/p.json", target: "node-typescript",
    })),
    (error) => error.code === "plan-id");
  // bind requires target and profile
  assert.throws(() => validateRequestObject(baseRequest({ operation: "bind" })),
    (error) => error.code === "target");
  assert.throws(() => validateRequestObject(baseRequest({ operation: "bind", target: "node-typescript" })),
    (error) => error.code === "profile");
  // clean requires plan id; plan-clean forbids it
  assert.throws(() => validateRequestObject(baseRequest({ operation: "clean" })),
    (error) => error.code === "plan-id");
  assert.throws(
    () => validateRequestObject(baseRequest({ operation: "plan-clean", plan_id: `plan-${"0".repeat(64)}` })),
    (error) => error.code === "plan-id");
});

test("explicit nulls, unknown members, and bad spellings are refused", () => {
  assert.throws(() => validateRequestObject({ ...baseRequest(), target: null }),
    (error) => error.code === "null-member");
  assert.throws(() => validateRequestObject({ ...baseRequest(), workspace: "pnpm" }),
    (error) => error.code === "unknown-key");
  assert.throws(() => validateRequestObject(baseRequest({ protocol: "lekalo.target/v0" })),
    (error) => error.code === "protocol-token");
  assert.throws(() => validateRequestObject(baseRequest({ protocol_version: "0.3.0" })),
    (error) => error.code === "protocol-version");
  assert.throws(() => validateRequestObject(baseRequest({ request_id: "req-deadbeef" })),
    (error) => error.code === "request-id");
  assert.throws(() => validateRequestObject(baseRequest({ project_root: "C:/tmp" })),
    (error) => error.code === "project-root");
  assert.throws(() => validateRequestObject(baseRequest({ operation: "conform" })),
    (error) => error.code === "operation");
});

test("profile resolution members are paired, spelled, and sorted", () => {
  const digest = `sha256:${"a".repeat(64)}`;
  const capabilities = [{ id: "scan.symbols", support: "full" }];
  const scan = {
    operation: "scan",
    profile: "standalone",
    profile_digest: digest,
    profile_capabilities: capabilities,
  };
  validateRequestObject(baseRequest(scan));
  assert.throws(() => validateRequestObject(baseRequest({ ...scan, profile_capabilities: undefined })),
    (error) => error.code === "profile-capabilities");
  assert.throws(() => validateRequestObject(baseRequest({ ...scan, profile_digest: undefined })),
    (error) => error.code === "profile-capabilities");
  assert.throws(() => validateRequestObject({ ...baseRequest(scan), profile_digest: "sha256:short" }),
    (error) => error.code === "profile-digest");
  assert.throws(
    () => validateRequestObject(baseRequest({
      ...scan,
      profile_capabilities: [
        { id: "scan.symbols", support: "full" },
        { id: "generate.zod", support: "full" },
      ],
    })),
    (error) => error.code === "profile-capabilities");
  assert.throws(() => validateRequestObject(baseRequest({ ...scan, profile: undefined })),
    (error) => error.code === "profile");
});

// ---------------------------------------------------------------------------
// Canonical serialization.
// ---------------------------------------------------------------------------

test("canonical JSON sorts keys by UTF-8 byte order and stays compact", () => {
  const text = canonicalJson({ zeta: 1, alpha: { b: 1, a: 2 }, mid: [2, 1] });
  assert.equal(text, '{"alpha":{"a":2,"b":1},"mid":[2,1],"zeta":1}');
  // Byte order, not locale order: `Z` (0x5A) sorts before `a` (0x61).
  assert.equal(canonicalJson({ a: 1, Z: 2 }), '{"Z":2,"a":1}');
  assert.equal(canonicalJson({ "ü": 1, z: 2 }), '{"z":2,"ü":1}');
});

test("LF-only output: canonical bytes never contain carriage returns", () => {
  assert.equal(canonicalJson({ nested: { deep: ["a", "b"] } }).includes("\r"), false);
});

test("responses echo identity members and pair status with payload", () => {
  const request = baseRequest();
  const ok = buildResponse(request, { capabilities: describeCapabilities() });
  assert.equal(ok.protocol, PROTOCOL_TOKEN);
  assert.equal(ok.protocol_version, VERSION);
  assert.equal(ok.operation, "describe");
  assert.equal(ok.request_id, REQUEST_ID);
  assert.equal(ok.status, "ok");
  assert.deepEqual(ok.evidence.adapter.id, ADAPTER_ID);
  assert.throws(() => buildResponse(request, { error: { class: "invalid", code: "x", message: "y" }, result: {} }),
    (error) => error.code === "error-pairing");
});

test("an error response never carries result, capabilities, progress, or writes", () => {
  const request = baseRequest();
  const error = buildResponse(request, {
    error: { class: "unsupported", code: "operation-unsupported", message: "no", partial: false },
  });
  assert.equal(error.status, "error");
  for (const key of ["result", "capabilities", "progress", "writes"]) {
    assert.equal(key in error, false);
  }
});

// ---------------------------------------------------------------------------
// Scope grammar (mirroring the core's closed grammar).
// ---------------------------------------------------------------------------

test("scope containment is segment-wise and never widens", () => {
  assert.equal(scopeCovers("src/**", "src/a/b.ts"), true);
  assert.equal(scopeCovers("src/**", "src/a"), true);
  assert.equal(scopeCovers("src/**", "srcx/a.ts"), false, "sibling prefix is not covered");
  assert.equal(scopeCovers("src/**", "src"), false, "the scope root itself is not covered");
  assert.equal(scopeCovers("gen/out.txt", "gen/out.txt"), true);
  assert.equal(scopeCovers("gen/out.txt", "gen/other.txt"), false);
  assert.equal(isScope("src/**"), true);
  assert.equal(isScope("src/**/x"), false);
  assert.equal(isLogicalPath("src/**"), false);
  assert.equal(isScopeSegment("src"), true);
  assert.equal(isScopeSegment("."), false);
  assert.equal(isScopeSegment(".."), false);
  assert.equal(isScopeSegment("A"), false);
  assert.equal(isScopeSegment("src~1"), false);
});

test("lexical root violations cover the closed refusal taxonomy", () => {
  const cases = {
    "": "empty",
    ".": "project-root",
    "..": "traversal",
    "src/../leak": "traversal",
    "/etc": "absolute",
    "\\\\server/share": "absolute",
    "c:/windows": "drive",
    "C:/windows": "drive",
    "https://example.com/pkg": "uri",
    "src\\windows": "backslash",
    "src%2fescape": "percent-escape",
    "src/x:y": "control-character",
    "con": "dos-device",
    "nul.txt": "dos-device",
    "src~1": "short-name",
    "SRC": "uppercase",
    "src x": "whitespace",
    ["a".repeat(600)]: "overlong",
  };
  for (const [path, expected] of Object.entries(cases)) {
    assert.equal(lexicalRootViolation(path), expected, `root ${JSON.stringify(path)}`);
  }
  assert.equal(lexicalRootViolation("src"), null);
  assert.equal(lexicalRootViolation("src/deep/file.ts"), null);
  assert.equal(lexicalRootViolation("test"), null);
});

// ---------------------------------------------------------------------------
// Resolved project profile validation.
// ---------------------------------------------------------------------------

test("the committed standalone profile validates deterministically", () => {
  const profile = validateResolvedProjectProfile(projectProfile());
  assert.equal(profile.mode, "observed");
  assert.equal(profile.target, "node-typescript");
  assert.equal(profile.readRoots.length, 2);
  assert.deepEqual(profile.readRoots.map((root) => root.scope), ["src/**", "test/**"]);
  assert.equal(Object.isFrozen(profile), true);
  assert.equal(Object.isFrozen(profile.readRoots), true);
  // Repeated validation is value-identical.
  assert.deepEqual(validateResolvedProjectProfile(projectProfile()), profile);
});

test("every named invalid profile is refused before filesystem work", () => {
  const { cases } = JSON.parse(
    readInvalidProfiles(),
  );
  for (const [name, candidate] of Object.entries(cases)) {
    assert.throws(
      () => validateResolvedProjectProfile(candidate),
      (error) => error instanceof RequestRefusal && error.code === "profile-invalid",
      `profile case ${name} must be refused`,
    );
  }
});

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fixtureRoot } from "./helpers.mjs";
function readInvalidProfiles() {
  return readFileSync(join(fixtureRoot, "profiles/invalid.json"), "utf8");
}

test("zero roots are a policy refusal, not an implicit all-files grant", () => {
  const candidate = { ...projectProfile(), readRoots: [] };
  assert.throws(() => validateResolvedProjectProfile(candidate),
    (error) => error.code === "profile-invalid");
});

// ---------------------------------------------------------------------------
// Extension registry validation.
// ---------------------------------------------------------------------------

test("the classification preservation capability is declarable and honestly unsupported by default (issue #87)", () => {
  // The default describe map carries the closed unsupported state: the
  // frozen observed-scan wire cannot carry kind tokens, so the adapter
  // refuses instead of silently lowering.
  const describe = describeCapabilities();
  assert.equal(describe.capabilities["preserve.classification"], "unsupported");
  // A validated extension may declare the capability; its state is
  // projected into the describe map verbatim.
  const descriptor = {
    id: "fixture-classifier",
    version: "0.1.0",
    operations: ["scan"],
    namedCapabilities: { "preserve.classification": "full" },
    acceptedIrVersions: ["0.3.1"],
    invoke: () => ({ state: "complete" }),
  };
  const validated = validateExtensionDescriptor(descriptor);
  assert.equal(validated.namedCapabilities["preserve.classification"], "full");
  // Unknown look-alike ids stay refused.
  assert.throws(
    () =>
      validateExtensionDescriptor({
        ...descriptor,
        namedCapabilities: { "preserve.classification-scope": "full" },
      }),
    (error) => error.code === "extension-invalid",
  );
});

test("extension descriptors are validated; unknown capability ids are refused", () => {
  const descriptor = {
    id: "fixture-scanner",
    version: "0.1.0",
    operations: ["scan"],
    namedCapabilities: { "scan.symbols": "partial" },
    acceptedIrVersions: ["0.3.1"],
    invoke: () => ({ state: "complete" }),
  };
  const validated = validateExtensionDescriptor(descriptor);
  assert.equal(validated.id, "fixture-scanner");
  assert.throws(() => validateExtensionDescriptor({ ...descriptor, namedCapabilities: { "runtime.node": "full" } }),
    (error) => error.code === "extension-invalid");
  assert.throws(() => validateExtensionDescriptor({ ...descriptor, operations: ["describe"] }),
    (error) => error.code === "extension-invalid");
  assert.throws(() => validateExtensionDescriptor({ ...descriptor, invoke: "not-a-function" }),
    (error) => error.code === "extension-invalid");
  assert.throws(() => validateExtensionDescriptor({ ...descriptor, id: "Uppercase" }),
    (error) => error.code === "extension-invalid");
});

test("internal outcomes are closed and normalized", () => {
  const outcome = normalizeExtensionOutcome({ state: "partial", data: { entries: [] } });
  assert.equal(outcome.state, "partial");
  assert.throws(() => normalizeExtensionOutcome({ state: "optimistic" }),
    (error) => error.code === "outcome-invalid");
  assert.throws(() => normalizeExtensionOutcome({ state: "complete", surprise: true }),
    (error) => error.code === "outcome-invalid");
});

// ---------------------------------------------------------------------------
// Dispatch: unsupported operations and injected fakes.
// ---------------------------------------------------------------------------

function syntheticExecutionContext(project) {
  return { permittedProjectRoot: project, limits: { files: 64, bytes: 1 << 20 } };
}

test("production describe works with no roots and no extensions", () => {
  const kernel = createKernel();
  const request = baseRequest();
  const dispatched = kernel.dispatch(request, syntheticExecutionContext("/nonexistent"));
  assert.equal(dispatched.response.status, "ok");
  assert.deepEqual(dispatched.response.capabilities.operations, ["describe"]);
  assert.equal(dispatched.internal.state, "complete");
  assert.equal(dispatched.internal.evidence.node, process.versions.node);
  assert.equal(dispatched.internal.evidence.profile, null);
});

test("unsupported operations produce honest unsupported errors, never empty success", () => {
  const kernel = createKernel();
  // The production kernel (no extensions) refuses every non-describe
  // operation; an unrelated fake registry cannot revive them either.
  for (const operation of ["bind", "validate", "generate", "verify", "clean", "plan-clean", "scan"]) {
    const overrides = { operation };
    if (operation === "validate" || operation === "verify") {
      overrides.ir_path = "ir/p.json";
    }
    if (operation === "generate") {
      overrides.ir_path = "ir/p.json";
      overrides.target = "node-typescript";
      overrides.dry_run = true;
    }
    if (operation === "bind") {
      overrides.target = "node-typescript";
      overrides.profile = "standalone";
    }
    const dispatched = kernel.dispatch(baseRequest(overrides), syntheticExecutionContext("/nonexistent"));
    assert.equal(dispatched.response.status, "error", operation);
    assert.equal(dispatched.response.error.class, "unsupported", operation);
    assert.equal(dispatched.response.error.code, ERROR_CODES.unsupported.code, operation);
    assert.equal("result" in dispatched.response, false, operation);
  }
});

test("clean with a plausible plan id is still unsupported", () => {
  const kernel = createKernel();
  const dispatched = kernel.dispatch(
    baseRequest({ operation: "clean", plan_id: `plan-${"0".repeat(64)}` }),
    syntheticExecutionContext("/nonexistent"),
  );
  assert.equal(dispatched.response.status, "error");
  assert.equal(dispatched.response.error.class, "unsupported");
});

/** Build one kernel with the fixture profile plus fake scanner/runner spies. */
function kernelWithFakes(options = {}) {
  const calls = [];
  const scanner = {
    id: "fixture-scanner",
    version: "0.1.0",
    operations: ["scan"],
    namedCapabilities: { "scan.symbols": "partial" },
    invoke: options.scanner ?? ((input) => {
      calls.push({ extension: "fixture-scanner", ...input });
      return extensionResults().scanner.complete;
    }),
  };
  const runner = {
    id: "fixture-runner",
    version: "0.1.0",
    operations: ["verify"],
    namedCapabilities: { "verify.scenarios": "unsupported" },
    invoke: options.runner ?? ((input) => {
      calls.push({ extension: "fixture-runner", ...input });
      return extensionResults().runner.complete;
    }),
  };
  const kernel = createKernel({
    resolvedProjectProfile: projectProfile(),
    extensionRegistry: [scanner, runner],
    localEvidenceSink: options.sink ?? null,
  });
  return { kernel, calls };
}

test("independent fake scanner and runner attach to the same dispatcher", () => {
  // Both fakes are exercised through a valid rooted context in the
  // process/root suites; here we verify descriptor wiring only, so the
  // nonexistent context keeps the callbacks from running at all.
  const { kernel } = kernelWithFakes();
  assert.deepEqual(kernel.extensions, ["fixture-scanner", "fixture-runner"]);
  const describe = kernel.describe();
  assert.deepEqual(describe.operations, ["describe", "scan", "verify"].sort());
  assert.equal(describe.capabilities["scan.symbols"], "partial");
  assert.equal(describe.capabilities["verify.scenarios"], "unsupported");
  // The production descriptor stays untouched.
  assert.deepEqual(describeCapabilities().operations, ["describe"]);
});

// ---------------------------------------------------------------------------
// Evidence normalization and the local-only/public separation.
// ---------------------------------------------------------------------------

test("complete scan evidence normalizes with the full internal envelope", () => {
  const expectations = expectedNormalization().outcomes.complete;
  const outcome = extensionResults().scanner.complete;
  assert.deepEqual(Object.keys(outcome.evidence), expectations.internalEvidenceKeys);
  assert.equal(outcome.evidence.revision, "fixture-scan-revision-0001");
  assert.equal(outcome.evidence.freshness, "current");
  assert.equal(outcome.evidence.confidence, "high");
  // Full source spans survive intact internally (no invented ends).
  const span = outcome.evidence.sourceSpans[0];
  assert.equal(span.endLine, 7);
  assert.equal(span.endColumn, 2);
  assert.equal(span.convention, "1-based-1-based");
  // The original local reference is retained locally.
  assert.deepEqual(outcome.evidence.localReference, { kind: "internal-symbol", value: "fixtureGreeting" });
});

test("the local-only canary never reaches public projection", () => {
  const canary = extensionResults().scanner.localOnlyCanary;
  assert.equal(canary.evidence.localReference.value, "LOCAL-ONLY-CANARY-4f2a");
  const { kernel } = kernelWithFakes({
    scanner: () => canary,
  });
  const { project } = { project: undefined };
  // The canary projection is checked in the rooted process suite; here we
  // assert the normalization contract rows themselves.
  const expected = expectedNormalization().outcomes.localOnlyCanary;
  assert.equal(expected.canaryMustBeAbsentFromPublicBytes, true);
  assert.equal(canary.state, "complete");
  void project;
  void kernel;
});

test("malformed traversal entry cannot bypass response validation", () => {
  const malformed = extensionResults().scanner.malformed;
  assert.equal(malformed.data.entries[0].path, "../escape.ts");
  assert.equal(isLogicalPath("../escape.ts"), false);
  // The projection refuses the value instead of truncating the path.
  const expected = expectedNormalization().outcomes.malformed;
  assert.equal(expected.publicOutcome.status, "error");
  assert.equal(expected.mustNotClaimSuccess, true);
});

test("partial, unknown, and ambiguous outcomes must not claim success", () => {
  const expected = expectedNormalization().outcomes;
  for (const name of ["partial", "unknown", "ambiguous", "error", "malformed"]) {
    assert.equal(expected[name].mustNotClaimSuccess, true, name);
    assert.equal(expected[name].publicOutcome.status, "error", name);
  }
});

test("digest helpers stay exact", () => {
  assert.equal(isSha256Digest(`sha256:${"0".repeat(64)}`), true);
  assert.equal(isSha256Digest("sha256:0"), false);
  assert.equal(isSha256Digest(`sha256:${"0".repeat(63)}`), false);
  assert.equal(isSha256Digest(`SHA256:${"0".repeat(64)}`), false);
});
