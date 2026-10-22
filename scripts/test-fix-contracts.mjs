#!/usr/bin/env node
// Issue #74: closed versioned recipes/results, actual additive registry union,
// and the deterministic safe preview plus safe/stale/conflict/rollback goldens.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { registryUnion, assertRegistryUnion, ACCEPTED_M8_REGISTRY_FAMILIES } from "./lib/registry-union.mjs";

const require = createRequire(import.meta.url);
const Ajv2020 = require("ajv/dist/2020").default;
assert.equal(require("ajv/package.json").version, "8.20.0", "exact gate dependency required");
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = path => JSON.parse(readFileSync(resolve(root, path), "utf8"));
const ajv = new Ajv2020({ strict: true, allErrors: true });
for (const name of ["change-request", "change-plan", "change-receipt"]) ajv.addSchema(read(`contracts/${name}.schema.v0.8.0.json`));
ajv.addSchema(read("contracts/diagnostic.schema.v0.2.16.json"), "https://dev.lekalo/diagnostic.schema.v0.2.16.json");
for (const family of ["fix-recipe", "fix-result"]) {
  const schema = read(`contracts/${family}.schema.v0.8.0.json`);
  assert.equal(schema.title, `lekalo/${family}/v0.8.0`);
  assert.equal(schema.$id, `https://dev.lekalo/${family}.schema.v0.8.0.json`);
  assert.equal(schema.properties.schemaVersion.const, `lekalo/${family}/v0.8.0`);
  assert.equal(schema.properties.identity.const, `dev.lekalo.${family}@0.8.0`);
  ajv.addSchema(schema);
}
const recipe = ajv.getSchema("https://dev.lekalo/fix-recipe.schema.v0.8.0.json");
const result = ajv.getSchema("https://dev.lekalo/fix-result.schema.v0.8.0.json");
const valid = (validator, value, name) => assert(validator(value), `${name}: ${JSON.stringify(validator.errors)}`);
const preview = read("tests/fixtures/fix/golden/safe-preview.json");
valid(result, preview, "safe-preview");
valid(recipe, preview.recipes[0], "bound recipe");
assert.equal(preview.state, "preview");
assert.equal(preview.dryRun, true);
assert.equal(preview.receipt, null);
assert.equal(preview.plan.complete, true);
assert.equal(preview.plan.semanticDiff.equal, true);
assert.equal(preview.plan.fileEdits.length, 3);
assert.match(preview.recipes[0].preconditions.sourceFingerprint, /^sha256:[0-9a-f]{64}$/);
assert.equal(preview.diagnostics[0].fixes[0].fix_id, "core.canonical-format");
for (const [path, value] of [["class", "automatic"], ["version", "latest"], ["schemaVersion", "lekalo/fix-recipe/v9"], ["patch", "arbitrary source patch"]]) {
  const bad = structuredClone(preview.recipes[0]); bad[path] = value;
  assert.equal(recipe(bad), false, `closed/versioned recipe must reject ${path}`);
}
const unsafeSafe = structuredClone(preview.recipes[0]);
unsafeSafe.owner = "adapter.node-typescript"; unsafeSafe.id = "adapter.node-typescript.fake-safe";
assert.equal(recipe(unsafeSafe), false, "adapter cannot claim core safe execution");
for (const name of ["safe", "stale", "conflict", "rollback"]) {
  const golden = read(`tests/fixtures/fix/golden/${name}.json`);
  assert(Object.keys(golden).length >= 3, `${name} golden must carry observable invariants`);
  assert(!JSON.stringify(golden).includes("false"), `${name} golden invariants are positive proofs`);
}
const predecessor = read("contracts/diagnostic-registry.v0.6.5.json");
const registry = read("contracts/diagnostic-registry.v0.8.0.json");
registryUnion(predecessor, registry);
assertRegistryUnion(predecessor, registry, { families: ACCEPTED_M8_REGISTRY_FAMILIES, ownPrefix: "fix.", ownCount: 10 });
const own = registry.entries.filter(entry => entry.id.startsWith("fix."));
assert.deepEqual(own.map(entry => [entry.id, entry.code]), [
  ["fix.class-denied", "LEK-FIX-010"], ["fix.confirmation-required", "LEK-FIX-005"],
  ["fix.conflict", "LEK-FIX-004"], ["fix.format-required", "LEK-FIX-009"],
  ["fix.manual-only", "LEK-FIX-008"], ["fix.post-validation-failed", "LEK-FIX-007"],
  ["fix.precondition-failed", "LEK-FIX-003"], ["fix.recipe-invalid", "LEK-FIX-001"],
  ["fix.recipe-unavailable", "LEK-FIX-002"], ["fix.scope-denied", "LEK-FIX-006"],
]);
valid(ajv.compile(read("contracts/diagnostic-registry.schema.v0.8.0.json")), registry, "registry");
assert.deepEqual(own.find(entry => entry.id === "fix.format-required").allowed_fix_ids, ["core.canonical-format"]);
const replaced = structuredClone(registry);
replaced.entries.find(entry => entry.id === predecessor.entries[0].id).id = "zz-regression.missing";
replaced.entries.sort((a,b) => a.id < b.id ? -1 : 1);
assert.throws(() => registryUnion(predecessor, replaced), /registry-predecessor-missing/);
console.log(JSON.stringify({ ok: true, gate: "test-fix-contracts", contracts: 2, fixRules: own.length, registryEntries: registry.entries.length, goldens: 5 }));
