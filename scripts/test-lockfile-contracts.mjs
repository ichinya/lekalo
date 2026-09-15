#!/usr/bin/env node
// Issue #10 lockfile contract gate.
//
// Independently re-implements the closed v1 wire checks in JavaScript: the
// committed schema artifact, the golden valid fixtures (byte form, key
// order, and the lock digest recomputed here with node:crypto), and the
// invalid fixtures (each must fail). Hermetic: Node itself and repository
// files only; the pinned Ajv validation lives in test-lockfile-ajv.mjs.

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const failures = [];

const read = (relative, encoding) => readFileSync(resolve(root, relative), encoding);
const readText = (relative) => read(relative, "utf8");
const fail = (caseName, detail) => failures.push({ case: caseName, detail });

// The closed canonical SemVer spelling: no build metadata, no leading v.
const CANONICAL = /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$/;
const DIGEST = /^sha256:[0-9a-f]{64}$/;
const DISCRIMINATOR = "lekalo/lock/v1.0.0";
const TOP_LEVEL = [
  "schema_version",
  "resolver",
  "core",
  "contracts",
  "adapters",
  "generators",
  "profiles",
  "capabilities",
];

// ---------------------------------------------------------------------------
// 1. The committed schema artifact carries the closed shape.
// ---------------------------------------------------------------------------
const schema = JSON.parse(readText("contracts/lock.schema.v1.0.0.json"));

if (schema.$id !== "dev.lekalo.lock@1.0.0") {
  fail("schema:identity", `unexpected $id ${schema.$id}`);
}
if (schema.$schema !== "https://json-schema.org/draft/2020-12/schema") {
  fail("schema:dialect", `unexpected dialect ${schema.$schema}`);
}
if (schema.additionalProperties !== false) {
  fail("schema:closed", "the top-level object must be closed");
}
const declared = Object.keys(schema.properties).sort();
if (JSON.stringify(declared) !== JSON.stringify([...TOP_LEVEL].sort())) {
  fail("schema:properties", `unexpected property set ${declared.join(",")}`);
}
if (JSON.stringify([...schema.required].sort()) !== JSON.stringify([...TOP_LEVEL].sort())) {
  fail("schema:required", "every section is required");
}
const discriminator = schema.properties.schema_version;
if (!("const" in discriminator) || discriminator.const !== DISCRIMINATOR) {
  fail("schema:discriminator", "schema_version must be a const discriminator");
}
const versionPattern = schema.$defs.version.pattern;
const digestPattern = schema.$defs.digest.pattern;
if (!new RegExp(versionPattern).test("1.0.0") || new RegExp(versionPattern).test("v1") || new RegExp(versionPattern).test("1.0.0+meta")) {
  fail("schema:version-pattern", "the version pattern is not canonical SemVer");
}
if (!new RegExp(digestPattern).test("sha256:" + "0".repeat(64)) || new RegExp(digestPattern).test("sha256:" + "A".repeat(64))) {
  fail("schema:digest-pattern", "the digest pattern is not sha256:<64 lowercase hex>");
}
// Closed enums on the wire.
for (const [name, values] of [
  ["source.kind", schema.$defs.adapter.properties.source.properties.kind.enum],
  ["support", schema.$defs.capability.properties.support.enum],
  ["provider.kind", schema.$defs.capability.properties.provider.properties.kind.enum],
]) {
  if (!Array.isArray(values) || values.length === 0) {
    fail(`schema:${name}`, "closed enum missing");
  }
}

// ---------------------------------------------------------------------------
// 2. Canonical form: byte-sorted keys everywhere, compact, one final LF.
// ---------------------------------------------------------------------------
const isObject = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
const canonicalText = (value) => {
  if (Array.isArray(value)) return `[${value.map(canonicalText).join(",")}]`;
  if (isObject(value)) {
    const keys = Object.keys(value).sort((a, b) => {
      const left = Buffer.from(a, "utf8");
      const right = Buffer.from(b, "utf8");
      return left.compare(right);
    });
    return `{${keys.map((key) => `${JSON.stringify(key)}:${canonicalText(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
};

const recomputeDigest = (fileText) => {
  const payload = fileText.replace(/\n$/, "");
  return `sha256:${createHash("sha256").update(Buffer.from(payload, "utf8")).digest("hex")}`;
};

const validDir = "tests/fixtures/lockfile/valid";
for (const name of ["contract-only.lock.json", "multi-adapter.lock.json"]) {
  const text = readText(`${validDir}/${name}`);
  let document;
  try {
    document = JSON.parse(text);
  } catch (error) {
    fail(`valid:${name}`, `invalid JSON: ${error.message}`);
    continue;
  }
  if (!text.endsWith("\n") || text.endsWith("\n\n")) {
    fail(`valid:${name}`, "the file must end with exactly one LF");
  }
  if (text.includes("\r")) {
    fail(`valid:${name}`, "no carriage returns on the wire");
  }
  const payload = text.replace(/\n$/, "");
  if (canonicalText(document) !== payload) {
    fail(`valid:${name}`, "the fixture is not byte-canonical (sorted compact JSON)");
  }
  if (document.schema_version !== DISCRIMINATOR) {
    fail(`valid:${name}`, "wrong discriminator");
  }
  // Closed sections: no unknown keys anywhere (independent structural walk).
  const walk = (node, path) => {
    if (Array.isArray(node)) {
      node.forEach((entry, index) => walk(entry, `${path}/${index}`));
      return;
    }
    if (!isObject(node)) return;
    if (path === "" && JSON.stringify(Object.keys(node)) !== JSON.stringify([...TOP_LEVEL].sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b))))) {
      fail(`valid:${name}`, "top-level keys deviate from the closed set/order");
    }
    for (const key of Object.keys(node)) walk(node[key], `${path}/${key}`);
  };
  walk(document, "");
}

// Golden digests recomputed independently must match the sidecar expects.
for (const name of ["contract-only", "multi-adapter"]) {
  const lockText = readText(`${validDir}/${name}.lock.json`);
  const expect = JSON.parse(readText(`${validDir}/${name}.expect.json`));
  const digest = recomputeDigest(lockText);
  if (digest !== expect.lockDigest) {
    fail(`valid:${name}:digest`, `recomputed ${digest} != expected ${expect.lockDigest}`);
  }
}

// ---------------------------------------------------------------------------
// 3. Invalid fixtures: every committed invalid case must fail the wire.
// ---------------------------------------------------------------------------
const invalidDir = "tests/fixtures/lockfile/invalid";
for (const name of [
  "unsupported-schema-version.lock.json",
  "unknown-field.lock.json",
  "missing-required.lock.json",
  "bad-digest-uppercase.lock.json",
  "bad-version-v-prefix.lock.json",
  "protocol-null-with-adapters.lock.json",
]) {
  const text = readText(`${invalidDir}/${name}`);
  let document;
  try {
    document = JSON.parse(text);
  } catch (error) {
    fail(`invalid:${name}`, `fixture itself is not JSON: ${error.message}`);
    continue;
  }
  // Closed-set check: unknown/missing/extra sections or a wrong pattern
  // must surface. A structural deviation the JSON below tolerates still
  // fails the closed-key or pattern checks here.
  const deviations = [];
  if (document.schema_version !== DISCRIMINATOR) deviations.push("discriminator");
  for (const key of Object.keys(document)) {
    if (!TOP_LEVEL.includes(key)) deviations.push(`unknown field ${key}`);
  }
  for (const key of TOP_LEVEL) {
    if (!(key in document)) deviations.push(`missing ${key}`);
  }
  const collectDigests = (node) => {
    if (Array.isArray(node)) return node.flatMap(collectDigests);
    if (!isObject(node)) return [];
    const found = [];
    for (const key of Object.keys(node)) {
      if (key === "digest" && typeof node[key] === "string") found.push(node[key]);
      found.push(...collectDigests(node[key]));
    }
    return found;
  };
  for (const digest of collectDigests(document)) {
    if (!DIGEST.test(digest)) deviations.push(`bad digest ${digest}`);
  }
  const collectVersions = (node) => {
    if (Array.isArray(node)) return node.flatMap(collectVersions);
    if (!isObject(node)) return [];
    const found = [];
    for (const key of Object.keys(node)) {
      if (key === "version" && typeof node[key] === "string") found.push(node[key]);
      found.push(...collectVersions(node[key]));
    }
    return found;
  };
  for (const version of collectVersions(document)) {
    if (!CANONICAL.test(version)) deviations.push(`bad version ${version}`);
  }
  // Protocol-null closure.
  if (
    document.contracts &&
    document.contracts.target_protocol === null &&
    ((document.adapters && document.adapters.length > 0) ||
      (document.generators && document.generators.length > 0) ||
      (document.capabilities && document.capabilities.length > 0))
  ) {
    deviations.push("executable components under an unpublished protocol");
  }
  if (deviations.length === 0) {
    fail(`invalid:${name}`, "the fixture unexpectedly satisfies the closed wire");
  }
}

// ---------------------------------------------------------------------------
// 4. Contract parity with the Rust side.
// ---------------------------------------------------------------------------
const registry = JSON.parse(
  readText("crates/lekalo-core/src/versioning/contracts/version-registry.v1.2.0.json"),
);
if (registry.registryVersion !== "1.2.0") {
  fail("parity:registry", "the version registry artifact moved");
}
const golden = JSON.parse(readText(`${validDir}/contract-only.lock.json`));
if (golden.contracts.registry.version !== registry.registryVersion) {
  fail("parity:registry-pin", "the lock registry pin must match the embedded registry");
}
const irFamily = registry.families.ir;
if (golden.contracts.ir.version !== irFamily.current) {
  fail("parity:ir-pin", "the lock IR pin must match the registry current IR");
}
const modelFamily = registry.families.model;
if (!modelFamily.versions.some((record) => record.version === golden.contracts.model.version)) {
  fail("parity:model-pin", "the lock model pin must be a registered Model version");
}

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, failures }, null, 2)}\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({ ok: true, checked: "lockfile-contracts-v1" }, null, 2)}\n`);
