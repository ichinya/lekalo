#!/usr/bin/env node
// Issue #62 release gate: the error-contract and error-registry schemas,
// the published registry instance, and every golden/invalid fixture,
// validated with the same pinned third-party Draft 2020-12 implementation
// as the other contract gates. Exact Ajv 8.17.1 is provisioned outside
// this checkout (CI does the same on Node 18 and 24) and exposed through
// NODE_PATH / LEKALO_AJV_NODE_PATH.
//
// The gate checks both closed wire schemas plus the invariants JSON
// Schema cannot express: canonical ordering of every array, unique ids
// and codes, tombstones that are never reused, the closed
// retry/idempotency/effect agreement table, the public/private message
// and payload exposure rules, closed unions that resolve to declared
// errors, and canonical byte form of the published registry.

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

const registrySchema = read("contracts/error-registry.schema.v1.0.0.json");
const bindingSchema = read("contracts/error-contract.schema.v1.0.0.json");
const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateRegistry = ajv.compile(registrySchema);
const validateBinding = ajv.compile(bindingSchema);

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

// Invariant violations throw so the invalid-fixture checker can treat
// them as the expected rejection instead of a gate failure.
class InvariantViolation extends Error {}
const bad = (reason, detail) => {
  throw new InvariantViolation(reason + (detail === undefined ? "" : " " + JSON.stringify(detail)));
};

// The closed vocabularies mirrored from the contract (redundant with the
// schema enums by intent: a schema edit alone cannot widen these gates).
const CATEGORIES = new Set([
  "validation",
  "auth",
  "conflict",
  "not-found",
  "domain",
  "infrastructure",
]);
const IDEMPOTENCY = new Set([
  "guaranteed",
  "key-required",
  "not-guaranteed",
  "not-applicable",
]);
const EFFECTS = new Set(["none", "read", "write", "destructive", "external"]);
const OBSERVABILITY = new Set(["info", "warning", "error", "critical"]);
const CODE = /^LEK-ERR-[0-9]{3}$/;
const ID = /^[a-z][a-z0-9_]{0,62}(\.[a-z][a-z0-9_]{0,62}){1,2}$/;

const sortedIds = (items, key) => {
  for (let index = 1; index < items.length; index += 1) {
    if (key(items[index - 1]) >= key(items[index])) return false;
  }
  return true;
};

const typeLeaves = (expr, leaves) => {
  if (expr.ref !== undefined) leaves.push(expr.ref);
  else if (expr.list !== undefined) typeLeaves(expr.list, leaves);
  else if (expr.optional !== undefined) typeLeaves(expr.optional, leaves);
};

const checkRegistryInvariants = (where_, registry) => {
  if (registry.schema_version !== "lekalo/error-registry/v1.0.0") bad("registry-schema-version", where_);
  if (registry.identity !== "dev.lekalo.error-registry@1.0.0") bad("registry-identity", where_);
  if (registry.closed !== true) bad("registry-closed", where_);
  if (!sortedIds(registry.errors, (error) => error.id)) bad("errors-unsorted", where_);
  if (!sortedIds(registry.bindings, (binding) => binding.operation)) bad("bindings-unsorted", where_);
  if (!sortedIds(registry.tombstones, (tombstone) => tombstone.code)) bad("tombstones-unsorted", where_);

  const ids = new Set();
  const codes = new Set();
  for (const error of registry.errors) {
    if (!ID.test(error.id)) bad("error-id", { where_, id: error.id });
    if (!CODE.test(error.code)) bad("error-code", { where_, code: error.code });
    if (ids.has(error.id)) bad("duplicate-id", { where_, id: error.id });
    if (codes.has(error.code)) bad("duplicate-code", { where_, code: error.code });
    ids.add(error.id);
    codes.add(error.code);

    if (!CATEGORIES.has(error.category)) bad("unknown-category", { where_, category: error.category });
    if (!IDEMPOTENCY.has(error.idempotency)) bad("unknown-idempotency", { where_, id: error.id });
    if (!EFFECTS.has(error.effect)) bad("unknown-effect", { where_, id: error.id });
    if (!OBSERVABILITY.has(error.observability)) bad("unknown-observability", { where_, id: error.id });

    // Closed payload fields, sorted and unique.
    const fieldNames = error.payload.fields.map((field) => field.name);
    if (!sortedIds(error.payload.fields, (field) => field.name)) bad("fields-unsorted", { where_, id: error.id });
    if (new Set(fieldNames).size !== fieldNames.length) bad("duplicate-field", { where_, id: error.id });
    const publicNames = new Set(
      error.payload.fields.filter((field) => field.exposure === "public").map((field) => field.name),
    );

    // Public templates may reference only public payload fields.
    const templates = [["public", error.messages.public], error.messages.private ? ["private", error.messages.private] : null]
      .filter(Boolean);
    for (const [form, template] of templates) {
      if (!sortedIds(template.fields, (field) => field)) bad("template-fields-unsorted", { where_, id: error.id, form });
      for (const field of template.fields) {
        if (!fieldNames.includes(field)) bad("template-unknown-field", { where_, id: error.id, field });
        if (form === "public" && !publicNames.has(field)) {
          bad("template-private-field", { where_, id: error.id, field });
        }
      }
    }

    // The closed retry/idempotency/effect agreement table (ADR-0022).
    const effect = error.effect;
    const mode = error.idempotency;
    const retry = error.retry.policy;
    if (effect === "none" || effect === "read") {
      if (mode !== "not-applicable") bad("effect-idempotency-conflict", { where_, id: error.id });
      if (retry === "conditional") bad("read-conditional-conflict", { where_, id: error.id });
    } else if (effect === "write") {
      if (retry === "safe" && mode !== "guaranteed") bad("safe-write-conflict", { where_, id: error.id });
      if (
        retry === "conditional" &&
        mode !== "key-required" &&
        mode !== "guaranteed"
      ) {
        bad("conditional-write-conflict", { where_, id: error.id });
      }
    } else if (effect === "destructive") {
      if (retry === "safe") bad("destructive-safe-conflict", { where_, id: error.id });
      if (mode === "guaranteed") bad("destructive-guaranteed-conflict", { where_, id: error.id });
      if (
        retry === "conditional" &&
        error.retry.condition === "idempotency-key" &&
        mode !== "key-required"
      ) {
        bad("key-retry-requires-key-mode", { where_, id: error.id });
      }
    } else if (effect === "external") {
      if (retry === "safe" && mode !== "guaranteed") bad("safe-external-conflict", { where_, id: error.id });
      if (
        retry === "conditional" &&
        error.retry.condition === "idempotency-key" &&
        mode !== "key-required"
      ) {
        bad("key-retry-requires-key-mode", { where_, id: error.id });
      }
    }
    if (retry === "conditional" && error.retry.condition === undefined) {
      bad("conditional-requires-condition", { where_, id: error.id });
    }
    if (retry !== "conditional" && error.retry.condition !== undefined) {
      bad("unconditional-has-condition", { where_, id: error.id });
    }
  }

  const tombstoneCodes = new Set(registry.tombstones.map((tombstone) => tombstone.code));
  for (const tombstone of registry.tombstones) {
    if (codes.has(tombstone.code)) bad("tombstone-collision", { where_, code: tombstone.code });
    if (tombstoneCodes.has(tombstone.code) === false) bad("tombstone-set", { where_, code: tombstone.code });
  }

  for (const binding of registry.bindings) {
    if (binding.errors.length === 0) bad("empty-union", { where_, operation: binding.operation });
    if (!sortedIds(binding.errors, (member) => member)) bad("union-unsorted", { where_, operation: binding.operation });
    if (new Set(binding.errors).size !== binding.errors.length) {
      bad("duplicate-union-member", { where_, operation: binding.operation });
    }
    for (const member of binding.errors) {
      if (!ids.has(member)) bad("unresolved-union-member", { where_, operation: binding.operation, member });
    }
  }
};

const checkBindingInvariants = (where_, binding) => {
  if (binding.schema_version !== "lekalo/error-contract/v1.0.0") bad("binding-schema-version", where_);
  if (binding.errors.length === 0) bad("empty-union", where_);
  if (!sortedIds(binding.errors, (member) => member)) bad("union-unsorted", where_);
  if (new Set(binding.errors).size !== binding.errors.length) bad("duplicate-union-member", where_);
};

const isRegistryLike = (document) => document.errors !== undefined && document.bindings !== undefined;

const checkValid = (where_, document) => {
  if (isRegistryLike(document)) {
    if (!validateRegistry(document)) fail("schema-invalid", { where_, errors: validateRegistry.errors });
    checkRegistryInvariants(where_, document);
  } else {
    if (!validateBinding(document)) fail("schema-invalid", { where_, errors: validateBinding.errors });
    checkBindingInvariants(where_, document);
  }
};

const checkInvalid = (where_, document) => {
  const schemaHolds = isRegistryLike(document) ? validateRegistry(document) : validateBinding(document);
  if (schemaHolds) {
    // The schema alone may accept it; every invariant must still refuse.
    try {
      if (isRegistryLike(document)) checkRegistryInvariants(where_, document);
      else checkBindingInvariants(where_, document);
    } catch {
      return;
    }
    fail("invalid-fixture-accepted", where_);
  }
};

// The published registry must be canonical bytes: compact JSON with
// byte-sorted keys, no final line feed.
const registryBytes = readFileSync(resolve(root, "contracts/error-registry.v1.0.0.json"), "utf8");
const canonical = (value) => {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object") {
    const out = {};
    for (const key of Object.keys(value).sort()) out[key] = canonical(value[key]);
    return out;
  }
  return value;
};
if (registryBytes.endsWith("\n")) fail("registry-trailing-newline", "canonical bytes hash no final LF");
if (JSON.stringify(canonical(JSON.parse(registryBytes))) !== registryBytes) {
  fail("registry-noncanonical-bytes", "keys must be byte-sorted, compact");
}

const validRoot = resolve(root, "tests/fixtures/error-contract/valid");
const invalidRoot = resolve(root, "tests/fixtures/error-contract/invalid");
const validNames = readdirSync(validRoot).sort();
const invalidNames = readdirSync(invalidRoot).sort();
if (validNames.length === 0) fail("no-valid-fixtures", validRoot);
if (invalidNames.length === 0) fail("no-invalid-fixtures", invalidRoot);

let documents = 0;
checkValid("contracts/error-registry.v1.0.0.json", JSON.parse(registryBytes));
documents += 1;
for (const name of validNames) {
  checkValid(`tests/fixtures/error-contract/valid/${name}`, read(`tests/fixtures/error-contract/valid/${name}`));
  documents += 1;
}
for (const name of invalidNames) {
  checkInvalid(`tests/fixtures/error-contract/invalid/${name}`, read(`tests/fixtures/error-contract/invalid/${name}`));
  documents += 1;
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  gate: "error-contracts",
  ajv: ajvVersion,
  documents,
  valid: validNames.length + 1,
  invalid: invalidNames.length,
}, null, 2)}\n`);
