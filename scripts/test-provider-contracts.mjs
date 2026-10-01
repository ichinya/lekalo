#!/usr/bin/env node
// Issue #34 release gate: the workflow-provider discovery boundary.
// Validates the committed schema (contracts/provider-capabilities
// .schema.v0.6.3.json), the committed golden manifest fixture, and the
// live `lekalo provider describe --json` receipt (when the built binary
// is present) against the same pinned third-party Draft 2020-12
// implementation as the other contract gates. Cross-checks the closed
// operation vocabulary, the schema pins, the canonical digest domain,
// and the side-effect-free discovery guarantee against the CLI's own
// Rust contract tests. Ajv 8.17.1 is provisioned outside this checkout
// (CI does the same on Node 18 and 24) and exposed through NODE_PATH /
// LEKALO_AJV_NODE_PATH.
//
// The live-binary checks skip cleanly (with an explicit skip reason)
// when `cargo build -p lekalo-cli` has not run; the committed fixtures
// keep the gate meaningful either way.

import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  Ajv2020 = require("ajv/dist/2020.js").default;
  ajvVersion = require("ajv/package.json").version;
} catch {
  try {
    const fallback = join(process.env.LEKALO_AJV_NODE_PATH ?? "", "ajv");
    Ajv2020 = require(join(fallback, "dist/2020.js")).default;
    ajvVersion = JSON.parse(readFileSync(join(fallback, "package.json"), "utf8")).version;
  } catch (error) {
    process.stderr.write(`ajv-8.17.1-unavailable: ${error?.message ?? error}\n`);
    process.exit(1);
  }
}
if (ajvVersion !== "8.17.1") {
  process.stderr.write(`ajv-version-mismatch: ${ajvVersion}\n`);
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));

const schema = read("contracts/provider-capabilities.schema.v0.6.3.json");
const golden = read("tests/fixtures/provider/describe.golden.json");
const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateManifestReceipt = ajv.compile(schema);

let checks = 0;

// 1. The committed golden fixture is a valid discovery receipt.
if (!validateManifestReceipt(golden)) {
  fail("golden-schema", JSON.stringify(validateManifestReceipt.errors, null, 1));
}
checks += 1;

// 2. The golden fixture carries exactly the reviewed identity triple.
const manifest = golden.manifest;
if (manifest.schemaVersion !== "lekalo/workflow-provider/v0.6.3") {
  fail("golden-schema-version", manifest.schemaVersion);
}
if (manifest.identity !== "dev.lekalo.workflow-provider@0.6.3") {
  fail("golden-identity", manifest.identity);
}
if (manifest.productVersion !== "0.6.3") {
  fail("golden-product-version", manifest.productVersion);
}
checks += 1;

// 3. The closed operation vocabulary, canonical order, and effect
// classes. Detection is not an operation and no lifecycle operation is
// ever advertised.
const expectedOperations = [
  "context", "doctor", "generate", "impact", "readiness", "status",
  "trace.export", "validate", "verify",
];
const operationIds = manifest.operations.map((operation) => operation.id);
if (JSON.stringify(operationIds) !== JSON.stringify(expectedOperations)) {
  fail("operation-vocabulary", JSON.stringify(operationIds));
}
for (const forbidden of ["init", "install", "update", "sync", "cleanup", "migrate", "detect"]) {
  if (operationIds.includes(forbidden)) fail("hidden-lifecycle-operation", forbidden);
}
for (const operation of manifest.operations) {
  if (operation.id === "generate") {
    if (operation.effect !== "generated-artifacts" || operation.requiresAdapter !== true) {
      fail("generate-effect", JSON.stringify(operation));
    }
  } else if (operation.effect !== "read-only" || operation.requiresAdapter !== false) {
    fail("non-generate-effect", JSON.stringify(operation));
  }
}
checks += 1;

// 4. One pinned output schema per operation, matching the CLI's own
// Rust contract tests.
const schemaOf = (id) =>
  manifest.operations.find((operation) => operation.id === id)?.outputSchema;
const expectedSchemas = new Map([
  ["status", "lekalo/doctor/v0.3.2"],
  ["doctor", "lekalo/doctor/v0.3.2"],
  ["readiness", "lekalo/doctor/v0.3.2"],
  ["impact", "lekalo/impact/v0.2.16"],
  ["context", "lekalo/context/v0.2.16"],
  ["validate", "lekalo/validation-profile/v0.4.0"],
  ["verify", "lekalo/orchestration/v0.2.16"],
  ["generate", "lekalo/orchestration/v0.2.16"],
  ["trace.export", "lekalo/trace-manifest/v0.2.16"],
]);
for (const [id, expected] of expectedSchemas) {
  if (schemaOf(id) !== expected) fail("operation-schema-pin", `${id}: ${schemaOf(id)} != ${expected}`);
}
checks += 1;

// 5. The schema pins cover exactly the seven referenced families.
const expectedPins = [
  "lekalo/context/v0.2.16",
  "lekalo/diagnostic/v0.2.16",
  "lekalo/doctor/v0.3.2",
  "lekalo/impact/v0.2.16",
  "lekalo/orchestration/v0.2.16",
  "lekalo/trace-manifest/v0.2.16",
  "lekalo/validation-profile/v0.4.0",
];
const pinned = manifest.schemaPins.map((pin) => pin.schemaVersion).sort();
if (JSON.stringify(pinned) !== JSON.stringify(expectedPins)) {
  fail("schema-pins", JSON.stringify(pinned));
}
checks += 1;

// 6. The digest domain: canonical JSON (sorted keys, no whitespace)
// with manifestDigest removed.
function canonicalize(value) {
  if (Array.isArray(value)) {
    return `[${value.map(canonicalize).join(",")}]`;
  }
  if (value !== null && typeof value === "object") {
    const body = Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalize(value[key])}`)
      .join(",");
    return `{${body}}`;
  }
  return JSON.stringify(value);
}
function sha256Hex(bytes) {
  return createRequire(import.meta.url)("node:crypto")
    .createHash("sha256")
    .update(bytes)
    .digest("hex");
}
const { manifestDigest, ...withoutDigest } = manifest;
if (`sha256:${sha256Hex(canonicalize(withoutDigest))}` !== manifestDigest) {
  fail("manifest-digest", manifestDigest);
}
checks += 1;

// 7. No host identity in the committed bytes.
const goldenText = readFileSync(resolve(root, "tests/fixtures/provider/describe.golden.json"), "utf8");
if (/\\/u.test(goldenText) || /timestamp/u.test(goldenText.toLowerCase())) {
  fail("host-identity-leak", "golden carries a path or timestamp");
}
checks += 1;

// 8. The live binary (when built) projects exactly the committed
// golden bytes and discovery writes nothing beside the process.
const binary =
  process.env.LEKALO_BIN ??
  join(root, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
let binaryAvailable = false;
try {
  binaryAvailable = statSync(binary).isFile();
} catch {
  binaryAvailable = false;
}
if (binaryAvailable) {
  const workRoot = join(root, "target", "provider-describe-probe");
  const { mkdirSync, rmSync, readdirSync } = await import("node:fs");
  rmSync(workRoot, { recursive: true, force: true });
  mkdirSync(workRoot, { recursive: true });
  const probe = join(workRoot, "empty");
  mkdirSync(probe, { recursive: true });
  const before = readdirSync(probe).length;
  const run = spawnSync(binary, ["provider", "describe", "--json"], {
    cwd: probe,
    encoding: "utf8",
    windowsHide: true,
  });
  const afterEntries = readdirSync(probe);
  const live = run.status === 0 ? JSON.parse(run.stdout.trim()) : undefined;
  if (run.status !== 0) fail("live-exit", String(run.status));
  if (run.stderr && run.stderr.length > 0) fail("live-stderr", run.stderr);
  if (JSON.stringify(live) !== JSON.stringify(golden)) {
    fail("live-golden", "the live manifest is not the committed golden fixture");
  }
  if (afterEntries.length !== before) {
    fail("live-side-effect", JSON.stringify(afterEntries));
  }
  rmSync(workRoot, { recursive: true, force: true });
  checks += 2;
} else if (!existsSync(binary)) {
  process.stdout.write(
    `${JSON.stringify({ skipped: "live-binary", detail: `run: cargo build -p lekalo-cli (${binary})` })}\n`,
  );
}

process.stdout.write(
  `${JSON.stringify({ ok: true, gate: "provider-capabilities", checks })}\n`,
);
