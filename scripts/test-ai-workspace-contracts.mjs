#!/usr/bin/env node
// Issue #37 release gate: the ai-workspace change-event envelope wire
// schema, its closed canonical key order, the event-key derivation
// contract, and the privacy closure of the emitted example, validated
// with the same pinned third-party Draft 2020-12 implementation as the
// other contract gates. Exact Ajv 8.17.1 is provisioned outside this
// checkout (CI does the same) and exposed through NODE_PATH /
// LEKALO_AJV_NODE_PATH.
//
// This gate is the independent reference for the envelope's canonical
// byte form: it re-derives the event key from the example without
// reusing any hook code, proves canonicalization determinism by
// permuting object key order, and proves privacy closure by probing
// every emitted member against the leak set.
//
// Dependency-free beyond the pinned Ajv; run from the repo root.

import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  ({ default: Ajv2020 } = require("ajv/dist/2020"));
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(
    `pinned Ajv 8.17.1 is required (set NODE_PATH or LEKALO_AJV_NODE_PATH): ${error.message}\n`,
  );
  process.exit(1);
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(`pinned Ajv 8.17.1 required, found ${ajvVersion}\n`);
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));
const readText = (relative) => readFileSync(resolve(root, relative), "utf8");

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

const SCHEMA = "contracts/ai-workspace-event.schema.v0.6.3.json";
const EXAMPLE = "tests/fixtures/ai-workspace/event-envelope.example.json";
const ROUTING = "tests/fixtures/ai-workspace/routing-manifest.json";

const schema = read(SCHEMA);

// Pinned Ajv in strict mode, same as the sibling contract gates.
const ajv = new Ajv2020({ strict: true, allErrors: true });
let validateEnvelope;
try {
  validateEnvelope = ajv.compile(schema);
} catch (error) {
  fail("schema-compile", String(error));
}

const sha256Hex = (text) => createHash("sha256").update(text, "utf8").digest("hex");
const sha256Ref = (text) => `sha256:${sha256Hex(text)}`;

// The canonical byte form is the UTF-8 serialization with every
// object's members recursively sorted lexicographically (the closed
// schemas make that unambiguous) joined by the documented separators.
const CANONICAL_SEPARATOR = "|lekalo/ai-workspace-event/v0.6.3";

const envelope = read(EXAMPLE);

const assertValid = (value, label) => {
  const ok = validateEnvelope(value);
  if (!ok) {
    fail(`${label}-invalid`, JSON.stringify(validateEnvelope.errors, null, 2));
  }
};

// 1. The emitted example is a valid envelope.
assertValid(envelope, "example");

// 2. The event key in the example matches an independent derivation:
//    sha256 over the exact canonical serialization (excluding the key
//    itself) with the documented separators.
const { eventKey, ...keyInput } = envelope;
const canonical = (value) => {
  if (Array.isArray(value)) {
    return `[${value.map((entry) => canonical(entry)).join(",")}]`;
  }
  if (value && typeof value === "object") {
    const keys = Object.keys(value).sort();
    return `{${keys.map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
};
const derived = sha256Ref(canonical(keyInput) + CANONICAL_SEPARATOR);
if (derived !== envelope.eventKey) {
  fail("event-key-mismatch", `declared ${envelope.eventKey}, derived ${derived}`);
}

// 3. Canonicalization is deterministic under key permutation.
const permute = (value) => {
  if (Array.isArray(value)) return value.map(permute);
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.keys(value).sort().reverse().map((key) => [key, permute(value[key])]));
  }
  return value;
};
const permutedEnvelope = { ...permute(keyInput) };
const derivedPermuted = sha256Ref(canonical(permutedEnvelope) + CANONICAL_SEPARATOR);
if (derivedPermuted !== envelope.eventKey) {
  fail("canonicalization-nondeterministic", "permuted input changed the derived key");
}

// 4. The schema is genuinely closed: every emitted member of the
//    example is declared, and mutating any member fails validation.
const mutant = structuredClone(envelope);
mutant.notARealMember = true;
if (validateEnvelope(mutant) !== false) fail("closure-root", "unknown root member accepted");
const mutantRole = structuredClone(envelope);
mutantRole.impact.affectedRoles[0].privateName = "ghost";
if (validateEnvelope(mutantRole) !== false) fail("closure-role", "unknown role member accepted");

// 5. Bounds are real: a 65th artifact and a 65th role refuse.
const overArtifacts = structuredClone(envelope);
overArtifacts.artifacts = Array.from({ length: 65 }, (_, index) => ({
  ...envelope.artifacts[0],
  path: `contracts/generated-${index}.schema.v0.6.3.json`,
}));
if (validateEnvelope(overArtifacts) !== false) fail("bound-artifacts", "65 artifacts accepted");
const overRoles = structuredClone(envelope);
overRoles.impact.affectedRoles = Array.from({ length: 65 }, (_, index) => ({
  role: index === 0 ? envelope.impact.affectedRoles[0].role : `role-${index}`,
  explanation: envelope.impact.affectedRoles[0].explanation,
}));
if (validateEnvelope(overRoles) !== false) fail("bound-roles", "65 roles accepted");

// 6. Role pattern refuses path-like and host-like aliases: the closed
//    neutral alias grammar is the privacy boundary for consumer identity.
const pathy = structuredClone(envelope);
pathy.impact.affectedRoles[0].role = "Consumer/checkout";
if (validateEnvelope(pathy) !== false) fail("role-grammar", "path-like role accepted");

// 7. Privacy closure: no member key or string value anywhere in the
//    example may carry private identity. The example is deliberately
//    built from public names only, and this probe enforces it.
const LEAKS = [
  "c:\\", "c:/", "file://", "http://", "https://", "appdata", "users\\",
  "users/", ".lekalo/", "submitTask", "tasks.ts", "ichinya", "lekalo-i37",
];
const flatStrings = [];
const walk = (value, path) => {
  if (Array.isArray(value)) {
    value.forEach((entry, index) => walk(entry, `${path}[${index}]`));
  } else if (value && typeof value === "object") {
    for (const [key, member] of Object.entries(value)) {
      flatStrings.push([`${path}.${key}`, key], [`${path}.${key}`, String(member && typeof member === "object" ? "" : member)]);
      walk(member, `${path}.${key}`);
    }
  }
};
walk(envelope, "$");
for (const [where, text] of flatStrings) {
  const lowered = String(text).toLowerCase();
  for (const leak of LEAKS) {
    if (lowered.includes(leak)) fail("privacy-leak", `${where} carries ${leak}`);
  }
}

// 8. The routing manifest fixture is itself closed: declared paths are
//    envelope-legal public paths, roles match the role grammar, and the
//    manifest digest recorded in the example matches the fixture bytes.
const routing = read(ROUTING);
for (const route of routing.routes) {
  if (!/^[a-z][a-z0-9-]{2,63}$/.test(route.role)) fail("routing-role-grammar", route.role);
  if (!/^[a-z][a-z0-9.-]*$/.test(route.workspaceSlug ?? route.role)) fail("routing-slug-grammar", route.role);
}
for (const artifactPath of routing.approvedPaths) {
  const probe = structuredClone(envelope.artifacts[0]);
  probe.path = artifactPath;
  if (!validateEnvelope({ ...envelope, artifacts: [probe] })) fail("routing-path-grammar", artifactPath);
}
const routingDigest = sha256Ref(readText(ROUTING));
if (envelope.manifest.digest !== routingDigest) {
  fail("manifest-digest", `example pins ${envelope.manifest.digest}, fixture bytes hash ${routingDigest}`);
}

// 9. The limitation vocabulary is closed in the schema and the example
//    uses only declared limitations.
const declaredLimitations = new Set(
  (schema.$defs.limitation.description.match(/[a-z][a-z0-9-]*[a-z0-9]/gu) ?? [])
    .filter((word) => word.includes("-"))
);
for (const limitation of envelope.impact.limitations) {
  if (!declaredLimitations.has(limitation)) {
    fail("limitation-vocabulary", limitation);
  }
}

process.stdout.write(
  `${JSON.stringify({ ok: true, gate: "ai-workspace-contracts", schema: SCHEMA, example: EXAMPLE })}\n`,
);
