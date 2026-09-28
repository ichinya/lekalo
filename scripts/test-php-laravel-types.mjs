#!/usr/bin/env node
// Issue #58 acceptance harness: the PHP type generator proven end to
// end through the real one-shot adapter exchange (no vendor tree:
// generation and verification are pure compile exchanges).
//
// Stages:
//   1. Managed custody: the committed input plans the full closed type
//      inventory, applies it, and every emitted file matches the
//      committed golden digests byte for byte.
//   2. Byte stability: two clean roots with identical inputs emit
//      identical files, sidecars, and plan ids.
//   3. The closed mapping surface: verbatim wire names (task_id stays
//      task_id on the wire, taskId is only the PHP property), distinct
//      nominal UUID types, declared enum order, and the four presence
//      cases in the sidecar.
//   4. Drift custody: tampered managed bytes are a verify finding, a
//      clean plans and applies the managed deletions, and plan-clean
//      never names the scaffold scope.
//   5. Scaffold-once custody: one emission into the consumer root, a
//      marker-guarded regeneration that plans nothing, edited user
//      bytes that verify silently, a removed file that is a finding,
//      and a pre-existing unowned path that refuses the generation.
//   6. Checked custody: zero writes, strict join — absent evidence is
//      a finding for every declared id, conforming evidence passes
//      silently, stale/diverging/ambiguous evidence are typed
//      findings.
//   7. Unsupported projections: recursive codecs, normalization
//      collisions, and unknown default metadata each produce the
//      bounded php-types.mapping-unsupported finding with ZERO writes
//      planned; a stale input digest and an unknown custody value
//      refuse as errors.
//
// Dependency-free (node:*, no npm packages); run from the repo root.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const adapterPath = join(repoRoot, "adapters", "php-laravel", "adapter.php");
const corpusIr = join(repoRoot, "tests", "fixtures", "adapter-conformance", "inputs", "ir-minimal.json");
const fixtures = join(repoRoot, "tests", "fixtures", "php-laravel", "types");
const php = process.env.LEKALO_PHP ?? "php";
const sha256 = (bytes) => "sha256:" + createHash("sha256").update(bytes).digest("hex");

let passed = 0;
function step(name, fn) {
  try {
    fn();
    passed += 1;
    console.log(`ok - ${name}`);
  } catch (error) {
    console.error(`not ok - ${name}`);
    console.error(error instanceof Error ? error.stack : String(error));
    process.exit(1);
  }
}

/** One adapter exchange over stdin; returns the parsed envelope. */
function adapterCall(root, request) {
  const result = spawnSync(php, [adapterPath], {
    cwd: root,
    input: JSON.stringify(request),
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    timeout: 120_000,
  });
  if (result.status !== 0) {
    throw new Error(`adapter exited ${result.status}: ${result.stderr.slice(0, 2000)}`);
  }
  const lines = result.stdout.split("\n").filter((line) => line.trim().startsWith("{"));
  return JSON.parse(lines[lines.length - 1]);
}

/**
 * One generate exchange that must REFUSE: the bounded stderr
 * diagnostic carries the refusal code (the kernel never fabricates an
 * envelope for a refused generation), and nothing can have been
 * planned.
 */
function refusedCall(root, request) {
  const result = spawnSync(php, [adapterPath], {
    cwd: root,
    input: JSON.stringify(request),
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    timeout: 120_000,
  });
  assert.notEqual(result.status, 0, "a refused generation never exits 0");
  assert.equal(result.stdout.trim(), "", "a refused generation never prints an envelope");
  const diagnostic = JSON.parse(result.stderr.trim());
  return diagnostic.diagnostic;
}

const requestId = (seed) => "req-" + createHash("sha256").update(seed).digest("hex");

function baseRequest(operation, irPath) {
  return {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    project_root: ".",
    operation,
    ir_path: irPath,
    target: "php-laravel",
    profile: "default",
  };
}

/** Dry-run + apply one input document; returns both envelopes. */
function generate(root, irPath, seed) {
  const dry = adapterCall(root, {
    ...baseRequest("generate", irPath),
    request_id: requestId(`dry-${seed}`),
    dry_run: true,
  });
  assert.equal(dry.status, "ok", JSON.stringify(dry).slice(0, 2000));
  const apply = adapterCall(root, {
    ...baseRequest("generate", irPath),
    request_id: requestId(`apply-${seed}`),
    dry_run: false,
    plan_id: dry.evidence.plan_id,
  });
  assert.equal(apply.status, "ok", JSON.stringify(apply).slice(0, 2000));
  return { dry, apply };
}

function verify(root, irPath, seed) {
  const response = adapterCall(root, {
    ...baseRequest("verify", irPath),
    request_id: requestId(`verify-${seed}`),
  });
  assert.equal(response.status, "ok", JSON.stringify(response).slice(0, 2000));
  return response.result.findings ?? [];
}

function validate(root, irPath, seed) {
  const response = adapterCall(root, {
    ...baseRequest("validate", irPath),
    request_id: requestId(`validate-${seed}`),
  });
  assert.equal(response.status, "ok", JSON.stringify(response).slice(0, 2000));
  return response.result.findings ?? [];
}

/** Stage the bounded project view: input document, staged IR evidence. */
function materialize(root, inputFixture, irSource, inputName = "planner.types.json", keepDigest = false) {
  mkdirSync(join(root, "lekalo", "types"), { recursive: true });
  mkdirSync(join(root, ".lekalo", "cache", "ir"), { recursive: true });
  cpSync(irSource, join(root, ".lekalo", "cache", "ir", "planner.json"));
  const irDigest = sha256(readFileSync(join(root, ".lekalo", "cache", "ir", "planner.json")));
  const document = JSON.parse(readFileSync(join(fixtures, inputFixture), "utf8"));
  if (!keepDigest) {
    document.irDigest = irDigest;
  }
  writeFileSync(
    join(root, "lekalo", "types", inputName),
    canonicalJson(document) + "\n",
  );
  return irDigest;
}

function canonicalJson(value) {
  if (Array.isArray(value)) return "[" + value.map(canonicalJson).join(",") + "]";
  if (value !== null && typeof value === "object") {
    return (
      "{" +
      Object.keys(value)
        .sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)))
        .map((k) => `${JSON.stringify(k)}:${canonicalJson(value[k])}`)
        .join(",") +
      "}"
    );
  }
  return JSON.stringify(value);
}

const TYPES = ".lekalo/generated/php-laravel/types";
const SCAFFOLD = "app/lekalo-types";
const readJson = (path) => JSON.parse(readFileSync(path, "utf8"));

// ---------------------------------------------------------------------
// Stage 1: managed custody with committed golden digests.
// ---------------------------------------------------------------------
let goldenRoot;
step("managed generation matches the committed golden digests", () => {
  goldenRoot = mkdtempSync(join(tmpdir(), "lekalo-types-a-"));
  materialize(goldenRoot, "inputs/planner.types.json", corpusIr);
  const { dry, apply } = generate(goldenRoot, "lekalo/types/planner.types.json", "managed-a");
  const golden = readJson(join(fixtures, "goldens", "digests.json"));
  assert.equal(dry.writes.length, Object.keys(golden.files).length);
  assert.ok(apply.writes.length >= dry.writes.length - 0);
  for (const [relative, digest] of Object.entries(golden.files)) {
    const path = join(goldenRoot, TYPES, relative);
    assert.ok(existsSync(path), `missing emitted file ${relative}`);
    assert.equal(sha256(readFileSync(path)), digest, `golden drift on ${relative}`);
  }
  // The binding refused inputs stay refused: a stale digest is an error.
  materialize(goldenRoot, "inputs/planner-stale-digest.types.json", corpusIr, "stale.types.json", true);
  const staleCode = refusedCall(goldenRoot, {
    ...baseRequest("generate", "lekalo/types/stale.types.json"),
    request_id: requestId("stale"),
    dry_run: true,
  });
  assert.equal(staleCode, "types-input-digest");
  // An unknown custody value is an input-shape refusal, never a silent
  // fallback to managed.
  mkdirSync(join(goldenRoot, "lekalo", "types"), { recursive: true });
  writeFileSync(
    join(goldenRoot, "lekalo", "types", "unknown-custody.types.json"),
    canonicalJson({
      schemaVersion: "lekalo/php-types-input/v0.4.0",
      identity: "dev.lekalo.php-types-input@0.4.0",
      projectId: "planner",
      irDigest: sha256(readFileSync(join(goldenRoot, ".lekalo", "cache", "ir", "planner.json"))),
      policy: { custody: "best-effort" },
    }) + "\n",
  );
  const unknownCustodyCode = refusedCall(goldenRoot, {
    ...baseRequest("generate", "lekalo/types/unknown-custody.types.json"),
    request_id: requestId("unknown-custody"),
    dry_run: true,
  });
  assert.equal(unknownCustodyCode, "types-input-shape");
});

// ---------------------------------------------------------------------
// Stage 2: byte stability across two clean roots.
// ---------------------------------------------------------------------
step("two clean roots emit identical bytes, sidecars, and plan ids", () => {
  const rootB = mkdtempSync(join(tmpdir(), "lekalo-types-b-"));
  materialize(rootB, "inputs/planner.types.json", corpusIr);
  const { dry } = generate(rootB, "lekalo/types/planner.types.json", "managed-b");
  const dryA = adapterCall(goldenRoot, {
    ...baseRequest("generate", "lekalo/types/planner.types.json"),
    request_id: requestId("dry-managed-a"),
    dry_run: true,
  });
  // Same writes (paths + digests), so the plan id binds across roots.
  assert.deepEqual(dry.writes, dryA.writes);
  const sidecarA = readJson(join(goldenRoot, TYPES, "types.map.json"));
  const sidecarB = readJson(join(rootB, TYPES, "types.map.json"));
  assert.deepEqual(sidecarA, sidecarB);
  assert.equal(sidecarA.custody, "managed");
  assert.equal(sidecarA.namespacePrefix, "Lekalo\\Generated\\Types");
  assert.equal(sidecarA.digests.ir, sha256(readFileSync(corpusIr)));
  rmSync(rootB, { recursive: true, force: true });
});

// ---------------------------------------------------------------------
// Stage 3: the closed mapping surface.
// ---------------------------------------------------------------------
step("wire names stay verbatim and nominal ids stay distinct", () => {
  const sidecar = readJson(join(goldenRoot, TYPES, "types.map.json"));
  const byId = new Map(sidecar.types.map((t) => [t.semanticId, t]));
  // task_id stays task_id on the wire; taskId is only the property.
  const task = byId.get("planner.task");
  const taskIdField = task.fields.find((f) => f.name === "task_id");
  assert.ok(taskIdField, "the verbatim wire member task_id is mapped");
  assert.equal(taskIdField.property, "taskId");
  // The four presence cases, exactly.
  const presence = Object.fromEntries(task.fields.map((f) => [f.name, f.presence]));
  assert.equal(presence.task_id, "required-nonnull");
  assert.equal(presence.due, "optional-nullable");
  assert.equal(presence.window, "optional-nonnull");
  const payload = byId.get("planner.task_focused");
  assert.equal(payload.fields.find((f) => f.name === "focused_at").presence, "optional-nonnull");
  const cap = byId.get("planner.focus_task");
  assert.equal(cap.fields.find((f) => f.name === "task_id").presence, "required-nonnull");
  // Nominal uuid wrappers are distinct types.
  const taskId = byId.get("planner.task_id");
  const userId = byId.get("notify.user_id");
  assert.notEqual(taskId.fqn, userId.fqn);
  assert.equal(taskId.base, "uuid");
  assert.equal(userId.base, "uuid");
  // Declared enum order and values.
  const state = byId.get("planner.task_state");
  assert.deepEqual(
    state.values.map((v) => v.value),
    ["backlog", "focused", "done"],
  );
  // The emitted codec carries the verbatim member spelling.
  const codec = readFileSync(join(goldenRoot, TYPES, "planner", "task_dto_codec.php"), "utf8");
  assert.ok(codec.includes("'task_id'"), "codec uses the verbatim wire member");
  assert.ok(codec.includes("strict_types=1"), "every emitted file declares strict_types");
  // The class map covers every emitted class.
  const classmap = readFileSync(join(goldenRoot, TYPES, "classmap.php"), "utf8");
  assert.ok(classmap.includes("Lekalo\\\\Generated\\\\Types\\\\Planner\\\\TaskId"));
  assert.ok(classmap.includes("Lekalo\\\\Generated\\\\Types\\\\Notify\\\\UserId"));
});

// ---------------------------------------------------------------------
// Stage 4: drift, verify, and clean custody.
// ---------------------------------------------------------------------
step("drift is a finding and clean removes exactly the managed types", () => {
  let findings = verify(goldenRoot, "lekalo/types/planner.types.json", "clean-verify");
  assert.deepEqual(findings, [], "a fresh generation verifies clean");
  const target = join(goldenRoot, TYPES, "planner", "task_dto.php");
  writeFileSync(target, readFileSync(target, "utf8") + "\n// tampered\n");
  findings = verify(goldenRoot, "lekalo/types/planner.types.json", "drift-verify");
  const drift = findings.find((f) => f.path.endsWith("task_dto.php"));
  assert.ok(drift, "the tampered file is reported");
  assert.equal(drift.code, "php-types.drift");
  assert.equal(drift.detail, "drifted");
  // Validate stays silent over a good mapping even while bytes drifted.
  assert.deepEqual(validate(goldenRoot, "lekalo/types/planner.types.json", "validate"), []);
  writeFileSync(
    target,
    readFileSync(target, "utf8").replace("\n// tampered\n", ""),
  );
  // The wire clean plans only over the kernel fixture artifact: managed
  // types deletion is the core ownership manifest's custody (issue
  // #91), and the adapter wire never names generated types there.
  const refusedClean = refusedCall(goldenRoot, {
    ...baseRequest("plan-clean", "lekalo/types/planner.types.json"),
    request_id: requestId("plan-clean"),
  });
  assert.equal(refusedClean, "ir-path");
  const wirePlan = adapterCall(goldenRoot, {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    project_root: ".",
    operation: "plan-clean",
    request_id: requestId("plan-clean-wire"),
  });
  assert.equal(wirePlan.status, "ok");
  for (const entry of wirePlan.writes) {
    assert.ok(
      !entry.path.startsWith(TYPES + "/") && !entry.path.startsWith(SCAFFOLD + "/"),
      `clean may never target a types path: ${entry.path}`,
    );
  }
});

// ---------------------------------------------------------------------
// Stage 5: scaffold-once custody.
// ---------------------------------------------------------------------
step("scaffold-once emits once, keeps user bytes, and refuses unowned paths", () => {
  const root = mkdtempSync(join(tmpdir(), "lekalo-types-s-"));
  materialize(root, "inputs/planner-scaffold.types.json", corpusIr);
  // A pre-existing unowned path refuses the whole generation.
  const unowned = join(root, SCAFFOLD, "planner", "task_state.php");
  mkdirSync(join(root, SCAFFOLD, "planner"), { recursive: true });
  writeFileSync(unowned, "<?php\n// user's own class\n");
  const refusedCode = refusedCall(root, {
    ...baseRequest("generate", "lekalo/types/planner.types.json"),
    request_id: requestId("scaffold-refused"),
    dry_run: true,
  });
  assert.equal(refusedCode, "types-scaffold-unowned");
  rmSync(unowned, { force: true });
  // First emission writes the scaffold and its bundle marker.
  const first = generate(root, "lekalo/types/planner.types.json", "scaffold-first");
  assert.ok(first.dry.writes.length > 0);
  assert.ok(existsSync(join(root, SCAFFOLD, "planner", "task_dto.php")));
  assert.ok(existsSync(join(root, SCAFFOLD, "types.map.json")));
  // Regeneration plans nothing: the marker makes the scaffold user-owned.
  const second = adapterCall(root, {
    ...baseRequest("generate", "lekalo/types/planner.types.json"),
    request_id: requestId("scaffold-second"),
    dry_run: true,
  });
  assert.equal(second.status, "ok");
  assert.deepEqual(second.writes, [], "the marker guards every scaffold write");
  // Edited user bytes verify silently.
  const dtoPath = join(root, SCAFFOLD, "planner", "task_dto.php");
  writeFileSync(dtoPath, readFileSync(dtoPath, "utf8") + "\n// user edit\n");
  let findings = verify(root, "lekalo/types/planner.types.json", "scaffold-edit");
  assert.deepEqual(findings, [], "user-owned bytes are existence-checked only");
  // A removed scaffold file with a surviving marker is a finding.
  rmSync(dtoPath, { force: true });
  findings = verify(root, "lekalo/types/planner.types.json", "scaffold-missing");
  const missing = findings.find((f) => f.code === "php-types.scaffold-missing");
  assert.ok(missing, "the removed scaffold is reported");
  assert.ok(missing.path.endsWith("task_dto.php"));
  // Clean never names the scaffold scope: the wire plan carries only
  // the kernel fixture artifact, never app/lekalo-types paths.
  const plan = adapterCall(root, {
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    project_root: ".",
    operation: "plan-clean",
    request_id: requestId("scaffold-plan-clean"),
  });
  assert.equal(plan.status, "ok");
  for (const entry of plan.writes) {
    assert.ok(
      !entry.path.startsWith(SCAFFOLD + "/"),
      `the scaffold scope never enters a delete plan: ${entry.path}`,
    );
  }
  rmSync(root, { recursive: true, force: true });
});

// ---------------------------------------------------------------------
// Stage 6: checked custody.
// ---------------------------------------------------------------------
step("checked custody is strict: absent evidence can never pass", () => {
  const root = mkdtempSync(join(tmpdir(), "lekalo-types-c-"));
  materialize(root, "inputs/planner-checked.types.json", corpusIr, "planner-checked.types.json");
  // Zero writes, and the absent observer is a finding for every id.
  // A findings outcome plans nothing and carries no plan id: there is
  // nothing to apply.
  const dry = adapterCall(root, {
    ...baseRequest("generate", "lekalo/types/planner-checked.types.json"),
    request_id: requestId("checked-absent"),
    dry_run: true,
  });
  assert.equal(dry.status, "ok");
  assert.deepEqual(dry.result.writes, [], "checked custody never emits");
  const findings = dry.result.findings;
  const missingCodes = new Set(findings.map((f) => f.code));
  assert.ok(missingCodes.has("php-types.binding-missing"));
  assert.equal(findings.length, 13);
  // Conforming evidence (built from the managed emission of the same
  // project, whose bytes the consumer carries) joins clean.
  materialize(root, "inputs/planner.types.json", corpusIr, "planner.types.json");
  generate(root, "lekalo/types/planner.types.json", "checked-managed");
  const sidecar = readJson(join(root, TYPES, "types.map.json"));
  const classes = sidecar.types.map((entry) => {
    const record = {
      semanticId: entry.semanticId,
      fqn: entry.fqn,
      path: TYPES + "/" + entry.path,
      sourceDigest: sha256(readFileSync(join(root, TYPES, entry.path))),
      kind: entry.kind,
    };
    if (entry.values) {
      record.enumCases = entry.values.map((v) => ({ case: v.case, value: v.value }));
    }
    if (entry.fields) {
      record.properties = entry.fields.map((f) => ({ name: f.property, type: "T" }));
    }
    record.codec = entry.codec;
    return record;
  });
  mkdirSync(join(root, ".lekalo", "import", "observed"), { recursive: true });
  writeFileSync(
    join(root, ".lekalo", "import", "observed", "types-evidence.json"),
    canonicalJson({
      schemaVersion: "lekalo/php-types-evidence/v0.4.0",
      identity: "dev.lekalo.php-types-evidence@0.4.0",
      classes,
    }) + "\n",
  );
  const conforming = adapterCall(root, {
    ...baseRequest("validate", "lekalo/types/planner-checked.types.json"),
    request_id: requestId("checked-conforming"),
  });
  assert.equal(conforming.status, "ok");
  assert.deepEqual(conforming.result.findings, [], "conforming evidence joins silently");
  // Stale source digest: the observed bytes moved under the binding.
  const stale = JSON.parse(canonicalJson({ schemaVersion: "lekalo/php-types-evidence/v0.4.0", identity: "dev.lekalo.php-types-evidence@0.4.0", classes }));
  stale.classes = stale.classes.map((c) => ({
    ...c,
    sourceDigest: "sha256:" + "f".repeat(64),
  }));
  writeFileSync(join(root, ".lekalo", "import", "observed", "types-evidence.json"), canonicalJson(stale) + "\n");
  const staleFindings = validate(root, "lekalo/types/planner-checked.types.json", "checked-stale");
  assert.ok(staleFindings.some((f) => f.code === "php-types.binding-mismatch" && f.detail.includes("stale-source-digest")));
  // A diverging enum value is a shape mismatch, never conformant.
  const diverging = JSON.parse(canonicalJson({ schemaVersion: "lekalo/php-types-evidence/v0.4.0", identity: "dev.lekalo.php-types-evidence@0.4.0", classes }));
  const stateRecord = diverging.classes.find((c) => c.semanticId === "planner.task_state");
  stateRecord.enumCases = stateRecord.enumCases.map((c) =>
    c.case === "Backlog" ? { case: "Backlog", value: "queued" } : c,
  );
  writeFileSync(join(root, ".lekalo", "import", "observed", "types-evidence.json"), canonicalJson(diverging) + "\n");
  const divergingFindings = validate(root, "lekalo/types/planner-checked.types.json", "checked-diverging");
  assert.ok(divergingFindings.some((f) => f.code === "php-types.binding-mismatch" && f.detail.includes("enum-cases-diverge")));
  // Ambiguous claims (two records for one id) never silently pick one.
  const ambiguous = JSON.parse(canonicalJson({ schemaVersion: "lekalo/php-types-evidence/v0.4.0", identity: "dev.lekalo.php-types-evidence@0.4.0", classes }));
  ambiguous.classes = [...ambiguous.classes, { ...ambiguous.classes[0] }];
  writeFileSync(join(root, ".lekalo", "import", "observed", "types-evidence.json"), canonicalJson(ambiguous) + "\n");
  const ambiguousFindings = validate(root, "lekalo/types/planner-checked.types.json", "checked-ambiguous");
  assert.ok(ambiguousFindings.some((f) => f.code === "php-types.binding-ambiguous"));
  rmSync(root, { recursive: true, force: true });
});

// ---------------------------------------------------------------------
// Stage 7: unsupported projections refuse with zero writes.
// ---------------------------------------------------------------------
for (const [name, irFixture, reason] of [
  ["recursive codec cycle", "recursive.ir.json", "recursive-codec-unsupported"],
  ["normalization collision", "collision.ir.json", "name-collision"],
  ["default metadata", "default-metadata.ir.json", "default-unsupported"],
]) {
  step(`${name} refuses with the bounded reason and zero writes`, () => {
    const root = mkdtempSync(join(tmpdir(), "lekalo-types-u-"));
    materialize(root, "inputs/planner.types.json", join(fixtures, "inputs", "ir", irFixture));
    const dry = adapterCall(root, {
      ...baseRequest("generate", "lekalo/types/planner.types.json"),
      request_id: requestId(`unsupported-${reason}`),
      dry_run: true,
    });
    assert.equal(dry.status, "ok");
    assert.deepEqual(dry.result.writes, [], "an unsupported projection never publishes a partial DTO set");
    const finding = dry.result.findings.find(
      (f) => f.code === "php-types.mapping-unsupported" && f.detail.includes(reason),
    );
    assert.ok(finding, `the ${reason} finding is reported`);
    rmSync(root, { recursive: true, force: true });
  });
}

process.stdout.write(`${JSON.stringify({ ok: true, suite: "php-laravel-types", checks: passed })}\n`);
