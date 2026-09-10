#!/usr/bin/env node
// Issue #12 release gate: the validation-profile schema, the two embedded
// profile instances, and their cross-contract coherence with the embedded
// diagnostic registry, validated with the same pinned third-party Draft
// 2020-12 implementation as the other contract gates. Exact Ajv 8.17.1 is
// provisioned outside this checkout (CI does the same on Node 18 and 24)
// and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.

import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
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

const schema = read("contracts/validation-profile.schema.v1.0.0.json");
const registry = read("contracts/diagnostic-registry.v1.19.0.json");
const profiles = [
  ["contracts/validation-profile.default.v1.0.0.json", "default"],
  ["contracts/validation-profile.strict.v1.0.0.json", "strict"],
].map(([file, id]) => [read(file), id]);

const ajv = new Ajv2020({ strict: true, allErrors: true });
const validateProfile = ajv.compile(schema);

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

// The registry instance feeding the cross-check must itself be the pinned
// successor validated by test-diagnostic-contracts.mjs.
if (registry.registry_version !== "1.19.0") fail("registry-version", registry.registry_version);
const registryRules = new Map(registry.entries.map((entry) => [entry.id, entry]));

// 1. Each embedded profile instance must satisfy the closed schema.
for (const [profile, id] of profiles) {
  if (!validateProfile(profile)) fail(`${id}-profile-invalid`, validateProfile.errors);
  if (profile.profile_id !== id) fail(`${id}-identity`, profile.profile_id);
  if (profile.diagnostic_registry_version !== registry.registry_version) {
    fail(`${id}-registry-pin`, profile.diagnostic_registry_version);
  }
}

// 2. Profile invariants JSON Schema cannot express.
const severityRank = { info: 0, warning: 1, error: 2 };
for (const [profile, id] of profiles) {
  const ids = profile.rules.map((rule) => rule.id);
  const sorted = [...ids].sort();
  if (ids.length !== new Set(ids).size) fail(`${id}-duplicate-rule-ids`);
  if (ids.some((ruleId, index) => ruleId !== sorted[index])) fail(`${id}-rules-not-sorted-by-id`);
  for (const rule of profile.rules) {
    const entry = registryRules.get(rule.id);
    if (!entry) fail(`${id}-unregistered-rule`, rule.id);
    if (entry.lifecycle !== "active") fail(`${id}-inactive-rule`, rule.id);
    if (rule.severity_override === undefined) continue;
    const fallback = entry.default_severity;
    if (fallback === "error") fail(`${id}-overrides-error-default`, rule.id);
    if (severityRank[rule.severity_override] >= severityRank[fallback]) {
      fail(`${id}-override-not-a-downgrade`, rule.id);
    }
  }
}

// 3. The two built-ins cover exactly the rule families issue #12 owns
//    (semantic.* plus validate.*) and differ only in the recorded
//    portability downgrade.
const owned = new Set([...registryRules.keys()].filter((ruleId) =>
  ruleId.startsWith("semantic.") || ruleId.startsWith("validate.")));
const defaultRules = new Set(profiles[0][0].rules.map((rule) => rule.id));
const strictRules = new Set(profiles[1][0].rules.map((rule) => rule.id));
for (const ruleId of owned) {
  if (!defaultRules.has(ruleId)) fail("default-missing-rule", ruleId);
  if (!strictRules.has(ruleId)) fail("strict-missing-rule", ruleId);
}
for (const ruleId of defaultRules) {
  if (!owned.has(ruleId)) fail("default-foreign-rule", ruleId);
}
for (const ruleId of strictRules) {
  if (!owned.has(ruleId)) fail("strict-foreign-rule", ruleId);
}
const defaultOverrides = profiles[0][0].rules
  .filter((rule) => rule.severity_override !== undefined);
if (defaultOverrides.length !== 1
  || defaultOverrides[0].id !== "semantic.portable-target-reference"
  || defaultOverrides[0].severity_override !== "info") {
  fail("default-override-inventory", defaultOverrides);
}
const strictOverrides = profiles[1][0].rules
  .filter((rule) => rule.severity_override !== undefined);
if (strictOverrides.length !== 0) fail("strict-has-overrides", strictOverrides);

process.stdout.write(`${JSON.stringify({
  ok: true,
  gate: "validation-profile-contracts",
  registry: registry.registry_version,
  profiles: profiles.map(([profile]) => profile.profile_id),
  owned: owned.size,
}, null, 2)}\n`);
