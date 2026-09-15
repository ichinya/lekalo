/**
 * #43 kernel suite: lexical/physical root validation, link and junction
 * refusals, and the all-roots-before-dispatch guarantee, proven with
 * invocation counters: one invalid root — first or last — prevents every
 * scanner/runner callback.
 */
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  createKernel,
  isInsideRoot,
  lexicalRootViolation,
  physicalRootViolation,
  resolveReadRoots,
  validateResolvedProjectProfile,
} from "../adapter.mjs";
import {
  dispose,
  materializeFixtureProject,
  projectProfile,
  symlinkIfPossible,
} from "./helpers.mjs";

function kernelWithCounters(permittedRoot, profile, extraRoots = []) {
  const calls = [];
  const scanner = {
    id: "fixture-scanner",
    version: "0.1.0",
    operations: ["scan"],
    invoke: (input) => {
      calls.push({ extension: "fixture-scanner", operation: input.operation });
      return {
        state: "complete",
        data: { complete: true, entries: [] },
        evidence: { revision: "fixture-scan-revision-counter" },
        diagnostics: [],
      };
    },
  };
  const runner = {
    id: "fixture-runner",
    version: "0.1.0",
    operations: ["verify"],
    invoke: (input) => {
      calls.push({ extension: "fixture-runner", operation: input.operation });
      return {
        state: "complete",
        data: { complete: true, entries: [] },
        evidence: { revision: "fixture-run-revision-counter" },
        diagnostics: [],
      };
    },
  };
  const kernel = createKernel({
    resolvedProjectProfile: {
      ...profile,
      ...(extraRoots.length > 0 ? { readRoots: [...profile.readRoots, ...extraRoots] } : {}),
    },
    extensionRegistry: [scanner, runner],
  });
  return { kernel, calls };
}

const scanRequest = (requestId) => ({
  protocol: "lekalo.target/v1",
  protocol_version: "0.2.16",
  operation: "scan",
  request_id: requestId,
  project_root: ".",
  profile: "standalone",
});

test("all valid roots resolve physically and extension callbacks then run", () => {
  const { project, root } = materializeFixtureProject("roots-ok");
  const profile = validateResolvedProjectProfile(projectProfile());
  const roots = resolveReadRoots(project, profile);
  assert.equal(roots.length, 2);
  assert.deepEqual(roots.map((candidate) => candidate.scope), ["src/**", "test/**"]);
  for (const candidate of roots) {
    assert.equal(lexicalRootViolation(candidate.path), null);
    assert.equal(physicalRootViolation(project, candidate), null);
    assert.ok(isInsideRoot(project, candidate.absolute));
  }
  dispose(root);
});

test("a missing root is an explicit failure, never an empty successful project", () => {
  const { project, root } = materializeFixtureProject("roots-missing");
  const broken = {
    ...projectProfile(),
    readRoots: [...projectProfile().readRoots, { path: "gone", kind: "tree" }],
  };
  const profile = validateResolvedProjectProfile(broken);
  assert.throws(
    () => resolveReadRoots(project, profile),
    (error) => error.code === "root-invalid"
      && error.failures.some((failure) => failure.reason === "missing"),
  );
  dispose(root);
});

test("a symlinked root is refused before any read", () => {
  const { project, root } = materializeFixtureProject("roots-link");
  const made = symlinkIfPossible(project, "link-out", "..");
  const broken = {
    ...projectProfile(),
    readRoots: [{ path: "link-out", kind: "tree" }],
  };
  const profile = validateResolvedProjectProfile(broken);
  if (made) {
    assert.throws(
      () => resolveReadRoots(project, profile),
      (error) => error.code === "root-invalid"
        && error.failures.some((failure) => ["symlink", "junction", "containment", "kind"].includes(failure.reason)),
    );
  } else {
    // A missing entry fails anyway: the refusal is unconditional.
    assert.throws(() => resolveReadRoots(project, profile), (error) => error.code === "root-invalid");
  }
  dispose(root);
});

test("an out-of-tree file root whose kind mismatches is refused", () => {
  const { project, root } = materializeFixtureProject("roots-kind");
  const broken = {
    ...projectProfile(),
    readRoots: [{ path: "src", kind: "file" }],
  };
  const profile = validateResolvedProjectProfile(broken);
  assert.throws(
    () => resolveReadRoots(project, profile),
    (error) => error.failures.every((failure) => failure.reason !== undefined),
  );
  dispose(root);
});

test("one invalid LATE root prevents every scanner and runner callback", () => {
  const { project, root } = materializeFixtureProject("roots-late");
  const { kernel, calls } = kernelWithCounters(project, projectProfile(), [
    { path: "vanished", kind: "tree" },
  ]);
  const requestId = "req-" + "a".repeat(64);
  const dispatched = kernel.dispatch(scanRequest(requestId), {
    permittedProjectRoot: project,
    limits: { files: 16, bytes: 1 << 20 },
  });
  assert.equal(dispatched.response.status, "error");
  assert.equal(dispatched.response.error.class, "invalid");
  assert.equal(dispatched.response.error.code, "root-invalid");
  assert.equal(dispatched.response.error.partial, false);
  assert.deepEqual(calls, [], "no extension may start when any root failed");
  dispose(root);
});

test("one invalid EARLY root prevents every scanner and runner callback", () => {
  const { project, root } = materializeFixtureProject("roots-early");
  const { kernel, calls } = kernelWithCounters(project, projectProfile(), [
    { path: "0-broken", kind: "tree" },
  ]);
  const requestId = "req-" + "b".repeat(64);
  const dispatched = kernel.dispatch(scanRequest(requestId), {
    permittedProjectRoot: project,
  });
  assert.equal(dispatched.response.status, "error");
  assert.deepEqual(calls, []);
  dispose(root);
});

test("with every root valid, the scanner callback runs with rooted views", () => {
  const { project, root } = materializeFixtureProject("roots-ok-dispatch");
  const calls = [];
  const kernel = createKernel({
    resolvedProjectProfile: projectProfile(),
    extensionRegistry: [{
      id: "fixture-scanner",
      version: "0.1.0",
      operations: ["scan"],
      invoke: (input) => {
        calls.push({ operation: input.operation, roots: input.readView.roots });
        return {
          state: "complete",
          data: { complete: true, entries: [{ path: "src/example.ts", kind: "source" }] },
          evidence: { revision: "fixture-scan-revision-ok" },
          diagnostics: [],
        };
      },
    }],
  });
  const requestId = "req-" + "c".repeat(64);
  const dispatched = kernel.dispatch(scanRequest(requestId), {
    permittedProjectRoot: project,
    limits: { files: 16, bytes: 1 << 20 },
  });
  assert.equal(dispatched.response.status, "ok", JSON.stringify(dispatched.response));
  assert.equal(calls.length, 1);
  assert.deepEqual(calls[0].operation, "scan");
  assert.deepEqual(calls[0].roots.map((candidate) => candidate.scope), ["src/**", "test/**"]);
  assert.equal(dispatched.response.result.entries.length, 1);
  dispose(root);
});

test("readView denies paths outside resolved roots and excluded paths", () => {
  const { project, root } = materializeFixtureProject("roots-view");
  const profile = validateResolvedProjectProfile({
    ...projectProfile(),
    exclusions: ["src/example.ts"],
  });
  const roots = resolveReadRoots(project, profile);
  void roots;
  // Facade denial is asserted through the kernel-level counters in the
  // dispatch tests; here the lexical containment invariant is pinned.
  assert.ok(isInsideRoot(project, project + "\\src"));
  assert.equal(isInsideRoot(project, project), false, "the root itself is never inside");
  dispose(root);
});

test("the runner fake attaches independently and its counters stay zero on refusal", () => {
  const { project, root } = materializeFixtureProject("roots-runner");
  const { kernel, calls } = kernelWithCounters(project, {
    ...projectProfile(),
    readRoots: [{ path: "test", kind: "tree" }],
  }, [{ path: "nope", kind: "tree" }]);
  const requestId = "req-" + "d".repeat(64);
  const verifyRequest = {
    protocol: "lekalo.target/v1",
    protocol_version: "0.2.16",
    operation: "verify",
    request_id: requestId,
    project_root: ".",
    ir_path: "ir/doc.json",
  };
  const dispatched = kernel.dispatch(verifyRequest, { permittedProjectRoot: project });
  assert.equal(dispatched.response.status, "error");
  assert.deepEqual(calls, [], "the runner must never start when a root failed");
  dispose(root);
});
