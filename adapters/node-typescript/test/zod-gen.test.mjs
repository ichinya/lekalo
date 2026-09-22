/**
 * #45 zod generator suite: the extension driven through the real kernel
 * dispatch boundary — describe honesty, generate dry-run plans, applies
 * honoring the plan id, byte stability, drift verification, and the
 * partial refusal on out-of-subset IR. Every case runs in a hermetic
 * temp project; no package manager, no shell, no project scripts.
 */
import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

import { createKernel, validateExtensionDescriptor, VERSION } from "../main.mjs";
import { descriptor } from "../src/zod-gen.mjs";
import { ZOD_DIR } from "../src/zod-map.mjs";
import { sha256 } from "../src/zod-emit.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const MATRIX_IR = join(repoRoot, "tests", "fixtures", "ir", "valid-zod-matrix", "ir.json");
const UNSUPPORTED_IR = join(repoRoot, "tests", "fixtures", "ir", "zod-unsupported", "ir.json");

const REQUEST_ID = `req-${"a".repeat(64)}`;

/** One hermetic project with the IR evidence under the cache home. */
function sandbox(tag, irSource) {
  const dir = mkdtempSync(join(tmpdir(), `lekalo-zod-gen-${tag}-`));
  const evidenceDir = join(dir, ".lekalo", "cache", "ir");
  mkdirSync(join(dir, ZOD_DIR), { recursive: true });
  mkdirSync(evidenceDir, { recursive: true });
  const bytes = readFileSync(irSource);
  const evidence = join(evidenceDir, "planner.json");
  writeFileSync(evidence, bytes);
  return {
    dir,
    evidenceBytes: bytes,
    close: () => rmSync(dir, { recursive: true, force: true }),
  };
}

function profileFor(dir) {
  return {
    id: "standalone",
    mode: "observed",
    target: "node-typescript",
    readRoots: [
      { path: ".lekalo/cache/ir", kind: "tree" },
      { path: ZOD_DIR, kind: "tree" },
    ],
    exclusions: [],
    provenance: {
      origin: "declared",
      revision: "zod-gen-suite-0001",
      disposition: "public-fixture",
    },
    // Kept verbatim by the kernel; the absolute root never reaches the wire.
    localReference: { permittedRoot: dir },
  };
}

function kernelFor(dir) {
  return createKernel({
    resolvedProjectProfile: profileFor(dir),
    extensionRegistry: [descriptor],
  });
}

function request(operation, overrides = {}) {
  return {
    protocol: "lekalo.target/v1",
    protocol_version: VERSION,
    operation,
    request_id: REQUEST_ID,
    project_root: ".",
    target: "node-typescript",
    profile: "standalone",
    ir_path: ".lekalo/cache/ir/planner.json",
    ...overrides,
  };
}

function dispatch(kernel, request) {
  const outcome = kernel.dispatch(request, {
    permittedProjectRoot: kernel.profile.localReference.permittedRoot,
  });
  return outcome;
}

// ---------------------------------------------------------------------------
// Descriptor and describe honesty.
// ---------------------------------------------------------------------------

test("the descriptor validates and declares the full zod capability", () => {
  const validated = validateExtensionDescriptor(descriptor);
  assert.equal(validated.id, "zod-schema-generator");
  assert.deepEqual(validated.operations, ["generate", "verify"]);
  assert.deepEqual(validated.namedCapabilities, { "generate.zod": "full" });
  assert.deepEqual(validated.acceptedIrVersions, ["0.2.16"]);
  assert.deepEqual(validated.writeScopes, [`${ZOD_DIR}/**`]);
});

test("describe with the bound profile advertises generate, verify, and the write scope", () => {
  const box = sandbox("describe", MATRIX_IR);
  try {
    const kernel = kernelFor(box.dir);
    const capabilities = kernel.describe();
    assert.ok(capabilities.operations.includes("generate"));
    assert.ok(capabilities.operations.includes("verify"));
    assert.equal(capabilities.capabilities["generate.zod"], "full");
    assert.deepEqual(capabilities.write_scopes, [`${ZOD_DIR}/**`]);
    assert.ok(capabilities.ir_versions.includes("0.2.16"));
  } finally {
    box.close();
  }
});

// ---------------------------------------------------------------------------
// Generate: dry-run plan, apply, byte stability, determinism.
// ---------------------------------------------------------------------------

test("generate dry run plans create writes without touching the disk", () => {
  const box = sandbox("dryrun", MATRIX_IR);
  try {
    const kernel = kernelFor(box.dir);
    const { response } = dispatch(kernel, request("generate", { dry_run: true }));
    assert.equal(response.status, "ok");
    const writes = response.writes;
    assert.ok(Array.isArray(writes) && writes.length >= 4, "all emitted files planned");
    assert.deepEqual(writes.map((write) => write.action), writes.map(() => "create"));
    const paths = writes.map((write) => write.path);
    assert.deepEqual(paths, [...paths].sort(), "writes are sorted by path");
    assert.ok(paths.includes(`${ZOD_DIR}/runtime.ts`));
    assert.ok(paths.includes(`${ZOD_DIR}/index.ts`));
    assert.ok(paths.includes(`${ZOD_DIR}/alpha.ts`));
    assert.ok(paths.includes(`${ZOD_DIR}/alpha.map.json`));
    // Nothing exists yet: a dry run writes nothing.
    assert.ok(!existsSync(join(box.dir, ZOD_DIR, "alpha.ts")));
  } finally {
    box.close();
  }
});

test("generate apply writes the exact planned bytes and honors the plan id", () => {
  const box = sandbox("apply", MATRIX_IR);
  try {
    const kernel = kernelFor(box.dir);
    const planned = dispatch(kernel, request("generate", { dry_run: true })).response;
    const planId = planned.evidence.plan_id;
    assert.match(planId, /^plan-[0-9a-f]{64}$/);
    const applied = dispatch(
      kernel,
      request("generate", { dry_run: false, plan_id: planId }),
    ).response;
    assert.equal(applied.status, "ok", JSON.stringify(applied.error ?? {}));
    assert.equal(applied.evidence.plan_id, planId, "the apply echoes the plan id");
    for (const write of planned.writes) {
      const physical = join(box.dir, write.path);
      assert.ok(existsSync(physical), `${write.path} applied`);
      assert.equal(
        sha256(readFileSync(physical).toString("utf8")),
        write.sha256,
        `${write.path} bytes match the plan digest`,
      );
    }
    // The applied sidecar is canonical JSON with a final LF.
    const sidecar = readFileSync(join(box.dir, `${ZOD_DIR}/alpha.map.json`), "utf8");
    assert.ok(sidecar.endsWith("\n") && !sidecar.endsWith("\n\n"));
  } finally {
    box.close();
  }
});

test("regenerating an applied project plans byte-identical replaces", () => {
  const box = sandbox("stable", MATRIX_IR);
  try {
    const kernel = kernelFor(box.dir);
    const firstPlan = dispatch(kernel, request("generate", { dry_run: true })).response;
    const planId = firstPlan.evidence.plan_id;
    dispatch(kernel, request("generate", { dry_run: false, plan_id: planId }));
    const before = firstPlan.writes.map((write) => [write.path, write.sha256]);
    const secondPlan = dispatch(kernel, request("generate", { dry_run: true })).response;
    const after = secondPlan.writes.map((write) => [write.path, write.sha256]);
    assert.deepEqual(after, before, "same IR produces byte-identical output");
    assert.deepEqual(
      secondPlan.writes.map((write) => write.action),
      secondPlan.writes.map(() => "replace"),
      "the second generation plans replaces",
    );
  } finally {
    box.close();
  }
});

test("an apply without a plan id refuses", () => {
  const box = sandbox("noplan", MATRIX_IR);
  try {
    const kernel = kernelFor(box.dir);
    const outcome = dispatch(kernel, request("generate", { dry_run: false }));
    assert.equal(outcome.response.status, "error");
    assert.equal(outcome.response.error.code, "outcome-failed-missing-plan-id");
  } finally {
    box.close();
  }
});

// ---------------------------------------------------------------------------
// Unsupported constructs: honest partial refusal on generate (AC-4).
// ---------------------------------------------------------------------------

test("generate over out-of-subset IR refuses partially and writes nothing", () => {
  const box = sandbox("unsupported", UNSUPPORTED_IR);
  try {
    const kernel = kernelFor(box.dir);
    const outcome = dispatch(kernel, request("generate", { dry_run: true }));
    assert.equal(outcome.response.status, "error");
    assert.equal(outcome.response.error.partial, true);
    assert.equal(outcome.response.error.code, "outcome-partial-unsupported-constructs");
    assert.ok(
      outcome.response.error.detail.some((detail) => detail.includes("planner.count")),
      JSON.stringify(outcome.response.error.detail),
    );
    assert.ok(!existsSync(join(box.dir, ZOD_DIR, "alpha.ts")));
  } finally {
    box.close();
  }
});

// ---------------------------------------------------------------------------
// Verify: drift findings and clean runs.
// ---------------------------------------------------------------------------

test("verify is clean on applied bytes and reports drift after tampering", () => {
  const box = sandbox("verify", MATRIX_IR);
  try {
    const kernel = kernelFor(box.dir);
    const planned = dispatch(kernel, request("generate", { dry_run: true })).response;
    const planId = planned.evidence.plan_id;
    dispatch(kernel, request("generate", { dry_run: false, plan_id: planId }));
    const clean = dispatch(kernel, request("verify"));
    assert.equal(clean.response.status, "ok");
    const cleanFindings = clean.response.result?.findings ?? [];
    assert.equal(cleanFindings.length, 0, JSON.stringify(cleanFindings));

    const generated = join(box.dir, `${ZOD_DIR}/alpha.ts`);
    writeFileSync(
      generated,
      readFileSync(generated, "utf8").replace("AlphaTextSchema", "AlphaTextSchemaTampered"),
    );
    const drifted = dispatch(kernel, request("verify"));
    assert.equal(drifted.response.status, "ok");
    const findings = drifted.response.result.findings;
    assert.equal(findings.length, 1, JSON.stringify(findings));
    assert.equal(findings[0].code, "zod.drift");
    assert.equal(findings[0].path, `${ZOD_DIR}/alpha.ts`);
  } finally {
    box.close();
  }
});

// ---------------------------------------------------------------------------
// Kernel boundary honesty.
// ---------------------------------------------------------------------------

test("policy documents outside the read roots resolve to defaults silently", () => {
  // The sandbox profile grants only the IR cache; the policy path is
  // unreadable, so defaults apply (no refusal).
  const box = sandbox("nopolicy", MATRIX_IR);
  try {
    const kernel = kernelFor(box.dir);
    const { response } = dispatch(kernel, request("generate", { dry_run: true }));
    assert.equal(response.status, "ok");
  } finally {
    box.close();
  }
});

test("a malformed readable policy document fails the operation in-envelope", () => {
  const box = sandbox("badpolicy", MATRIX_IR);
  try {
    // The malformed document sits at the policy path; the read roots
    // cover it, so the refusal must be in-envelope — never a silent
    // fallback to defaults.
    const dir = box.dir;
    const lekaloDir = join(dir, "lekalo", "targets");
    mkdirSync(lekaloDir, { recursive: true });
    writeFileSync(join(lekaloDir, "node-typescript.yaml"), "zod:\n  date: weekly\n");
    const profile = profileFor(dir);
    profile.readRoots = [
      { path: ".lekalo/cache/ir", kind: "tree" },
      { path: "lekalo/targets", kind: "tree" },
    ];
    const kernel = createKernel({
      resolvedProjectProfile: profile,
      extensionRegistry: [descriptor],
    });
    const { response } = dispatch(kernel, request("generate", { dry_run: true }));
    assert.equal(response.status, "error");
    assert.equal(response.error.code, "outcome-failed-policy-date-value");
    assert.ok(!existsSync(join(dir, ZOD_DIR, "alpha.ts")), "nothing written");
  } finally {
    box.close();
  }
});

test("a malformed policy document outside the read roots resolves to defaults", () => {
  // The unreadable-policy fallback: a malformed file the profile cannot
  // see is invisible, exactly like an absent one. The in-envelope
  // refusal for a readable malformed policy is the vector above.
  const box = sandbox("hiddenpolicy", MATRIX_IR);
  try {
    const dir = box.dir;
    const lekaloDir = join(dir, "lekalo", "targets");
    mkdirSync(lekaloDir, { recursive: true });
    writeFileSync(join(lekaloDir, "node-typescript.yaml"), "zod:\n  date: weekly\n");
    const kernel = kernelFor(box.dir);
    const { response } = dispatch(kernel, request("generate", { dry_run: true }));
    assert.equal(response.status, "ok");
  } finally {
    box.close();
  }
});
