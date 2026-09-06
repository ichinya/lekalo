#!/usr/bin/env node
// Issue #25 release gate: every authorization golden fixture, validated
// with the same pinned third-party Draft 2020-12 implementation as the
// other contract gates. Exact Ajv 8.17.1 is provisioned outside this
// checkout (CI does the same on Node 18 and 24) and exposed through
// NODE_PATH / LEKALO_AJV_NODE_PATH.
//
// The gate checks the closed wire schema plus the invariants JSON Schema
// cannot express: canonical ordering of every repeated collection,
// document-local id uniqueness, the closed actor vocabulary with its
// system.job/job pairing, sorted scope dimensions, error-ref grammar,
// the evaluation-vector expectation vocabulary, and coherent vector
// coverage of the required planner matrix.

import { createRequire } from "node:module";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  Ajv2020 = require("ajv/dist/2020").default;
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "ajv-unavailable",
    detail: error?.code ?? error?.message,
  }, null, 2)}\n`);
  process.exit(1);
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "ajv-version",
    detail: `expected 8.17.1, found ${ajvVersion}`,
  }, null, 2)}\n`);
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));

const schema = read("contracts/authorization.schema.v1.0.0.json");
const ajv = new Ajv2020({ strict: true, allErrors: true });
const validate = ajv.compile(schema);

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

// The closed vocabularies mirrored from the contract (redundant with the
// schema enums by intent: a schema edit alone cannot widen these gates).
const ACTORS = new Set([
  "public",
  "identity.user",
  "identity.service",
  "system.job",
  "internal",
]);
const DECISIONS = new Set(["allow", "deny"]);
const SCOPE_DIMENSIONS = new Set(["tenant", "workspace", "user"]);
const SYMBOL_ID = /^[a-z][a-z0-9_]{0,62}(\.[a-z][a-z0-9_]{0,62}){1,2}$/;
const SEGMENT = /^[a-z][a-z0-9_]{0,62}$/;

const sortedUnique = (values) => {
  const sorted = [...values].sort();
  return sorted.length === new Set(sorted).size;
};

const checkDocument = (name, document) => {
  if (document.schema_version !== "lekalo/authorization/v1.0.0") fail("schema-version", name);
  if (document.identity !== "dev.lekalo.authorization@1.0.0") fail("identity", name);
  if (document.profile_ref !== "dev.lekalo.authorization-profile@1.0.0") fail("profile-ref", name);

  for (const section of ["capabilities", "roles"]) {
    const decls = document[section] ?? [];
    if (!sortedUnique(decls.map((decl) => decl.id))) fail(`${section}-order`, name);
    for (const decl of decls) {
      if (!sortedUnique(decl.implies ?? [])) fail(`${section}-implies-order`, `${name}:${decl.id}`);
    }
  }
  const declared = new Set([
    ...(document.capabilities ?? []).map((decl) => decl.id),
    ...(document.roles ?? []).map((decl) => decl.id),
  ]);

  const policies = document.policies ?? [];
  if (!sortedUnique(policies.map((policy) => policy.id))) fail("policy-order", name);
  const policyIds = new Set(policies.map((policy) => policy.id));
  for (const policy of policies) {
    if (!ACTORS.has(policy.actor)) fail("actor", `${name}:${policy.id}`);
    if (!DECISIONS.has(policy.decision)) fail("decision", `${name}:${policy.id}`);
    if (!SYMBOL_ID.test(policy.error_ref)) fail("error-ref", `${name}:${policy.id}`);
    if (!sortedUnique(policy.applies_to)) fail("applies-to-order", `${name}:${policy.id}`);
    if ((policy.actor === "system.job") !== (typeof policy.job === "string")) {
      fail("job-pairing", `${name}:${policy.id}`);
    }
    const scope = policy.scope ?? {};
    const dimensions = Object.keys(scope);
    if (dimensions.length > SCOPE_DIMENSIONS.size) fail("scope-count", `${name}:${policy.id}`);
    for (const dimension of dimensions) {
      if (!SCOPE_DIMENSIONS.has(dimension)) fail("scope-dimension", `${name}:${policy.id}`);
    }
    for (const section of ["capabilities", "roles"]) {
      if (policy[section] !== undefined && !sortedUnique(policy[section])) {
        fail(`policy-${section}-order`, `${name}:${policy.id}`);
      }
      for (const reference of policy[section] ?? []) {
        if (!declared.has(reference)) fail(`${section}-reference`, `${name}:${policy.id}`);
      }
    }
    if (policy.composition !== undefined && !SYMBOL_ID.test(policy.composition)) {
      fail("composition-reference", `${name}:${policy.id}`);
    }
  }
  for (const composition of document.compositions ?? []) {
    if (!SYMBOL_ID.test(composition.id)) fail("composition-id", name);
    if (policyIds.has(composition.id)) fail("composition-id-collision", name);
    const members = [...(composition.all_of ?? []), ...(composition.any_of ?? [])];
    if (!sortedUnique(members.filter((m, i, all) => all.indexOf(m) === i))) {
      fail("composition-members", `${name}:${composition.id}`);
    }
  }
};

const goldenDir = resolve(root, "tests/fixtures/authorization/golden");
const goldens = readdirSync(goldenDir).filter((name) => name.endsWith(".canonical.json")).sort();
if (goldens.length === 0) fail("no-goldens", goldenDir);

let documents = 0;
let policies = 0;
for (const name of goldens) {
  const document = read(join("tests/fixtures/authorization/golden", name));
  if (!validate(document)) fail("golden-schema", `${name}: ${JSON.stringify(validate.errors)}`);
  checkDocument(name, document);
  documents += 1;
  policies += document.policies.length;
}

// The evaluation vectors: expectations stay closed and every required
// matrix row from the live issue stays present.
const vectors = read("tests/fixtures/authorization/vectors.json").vectors;
if (!Array.isArray(vectors) || vectors.length === 0) fail("no-vectors", "vectors.json");
const NAMES = new Set(vectors.map((vector) => vector.name));
if (NAMES.size !== vectors.length) fail("duplicate-vector", "vectors.json");
const REQUIRED = [
  "same-tenant-workspace-owner-allowed",
  "same-tenant-wrong-user-denied",
  "cross-tenant-denied-with-role-and-capability",
  "missing-authentication-denied",
  "missing-scope-denied",
  "capability-absent-denied",
  "ownership-mismatch-denied",
  "private-field-read-denied-for-member",
  "system-job-declared-grant-allowed",
  "system-job-wrong-job-denied",
  "deny-overrides-allow-for-archived",
  "public-title-read-denied",
];
for (const required of REQUIRED) {
  if (!NAMES.has(required)) fail("missing-vector", required);
}
for (const vector of vectors) {
  if (vector.expect !== "allowed" && vector.expect !== "denied") {
    fail("vector-outcome", vector.name);
  }
  if (vector.expect === "allowed" && !vector.expect_policy) {
    fail("vector-policy", vector.name);
  }
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  goldens: documents,
  policies,
  vectors: vectors.length,
}, null, 2)}\n`);
