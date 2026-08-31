#!/usr/bin/env node

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const checker = join(root, "scripts/check-privacy.mjs");
const outputSchema = JSON.parse(readFileSync(join(root, "contracts/privacy-export.schema.v2.5.output.json"), "utf8"));
const cliErrorSchema = JSON.parse(readFileSync(join(root, "contracts/privacy-cli-error.schema.v1.0.0.json"), "utf8"));
const temp = mkdtempSync(join(tmpdir(), "lekalo-output-schema-"));

function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

function localRef(rootSchema, ref) {
  assert.match(ref, /^#\//, `only local JSON pointers are supported: ${ref}`);
  return ref.slice(2).split("/").reduce((value, token) => value[token.replace(/~1/g, "/").replace(/~0/g, "~")], rootSchema);
}

function typeMatches(value, type) {
  if (type === "null") return value === null;
  if (type === "array") return Array.isArray(value);
  if (type === "object") return value !== null && typeof value === "object" && !Array.isArray(value);
  if (type === "integer") return typeof value === "number" && Number.isInteger(value);
  return typeof value === type;
}

function validate(schema, value, rootSchema = schema, path = "$", errors = []) {
  if (schema === true) return errors;
  if (schema === false) return [...errors, `${path}: false schema`];
  if (schema.$ref) return validate(localRef(rootSchema, schema.$ref), value, rootSchema, path, errors);

  if (Object.hasOwn(schema, "const") && canonical(value) !== canonical(schema.const)) errors.push(`${path}: const`);
  if (schema.enum && !schema.enum.some((entry) => canonical(entry) === canonical(value))) errors.push(`${path}: enum`);
  if (schema.type && !typeMatches(value, schema.type)) {
    errors.push(`${path}: type ${schema.type}`);
    return errors;
  }
  if (schema.oneOf) {
    const matches = schema.oneOf.filter((candidate) => validate(candidate, value, rootSchema, path, []).length === 0).length;
    if (matches !== 1) errors.push(`${path}: oneOf (${matches} matches)`);
  }
  if (schema.anyOf && !schema.anyOf.some((candidate) => validate(candidate, value, rootSchema, path, []).length === 0)) {
    errors.push(`${path}: anyOf`);
  }
  if (schema.allOf) for (const candidate of schema.allOf) validate(candidate, value, rootSchema, path, errors);
  if (schema.not && validate(schema.not, value, rootSchema, path, []).length === 0) errors.push(`${path}: not`);
  if (schema.if) {
    const branch = validate(schema.if, value, rootSchema, path, []).length === 0 ? schema.then : schema.else;
    if (branch) validate(branch, value, rootSchema, path, errors);
  }

  if (typeof value === "string") {
    if (schema.minLength !== undefined && value.length < schema.minLength) errors.push(`${path}: minLength`);
    if (schema.pattern && !(new RegExp(schema.pattern, "u")).test(value)) errors.push(`${path}: pattern`);
  }
  if (Array.isArray(value)) {
    if (schema.minItems !== undefined && value.length < schema.minItems) errors.push(`${path}: minItems`);
    if (schema.maxItems !== undefined && value.length > schema.maxItems) errors.push(`${path}: maxItems`);
    if (schema.uniqueItems && new Set(value.map(canonical)).size !== value.length) errors.push(`${path}: uniqueItems`);
    if (schema.items) value.forEach((item, index) => validate(schema.items, item, rootSchema, `${path}[${index}]`, errors));
  }
  if (value !== null && typeof value === "object" && !Array.isArray(value)) {
    for (const required of schema.required ?? []) if (!Object.hasOwn(value, required)) errors.push(`${path}: missing ${required}`);
    for (const [key, child] of Object.entries(value)) {
      if (schema.properties?.[key]) validate(schema.properties[key], child, rootSchema, `${path}.${key}`, errors);
      else if (schema.additionalProperties === false) errors.push(`${path}: additional ${key}`);
      else if (schema.additionalProperties && typeof schema.additionalProperties === "object") {
        validate(schema.additionalProperties, child, rootSchema, `${path}.${key}`, errors);
      }
    }
  }
  return errors;
}

function invoke(args) {
  return spawnSync(process.execPath, [checker, ...args], { cwd: root, encoding: "utf8" });
}

function invokeDecision(decision, id) {
  const path = join(temp, `${id}.json`);
  writeFileSync(path, `${JSON.stringify(decision, null, 2)}\n`);
  return invoke(["--decision", path]);
}

function parsed(stream, label) {
  assert.notEqual(stream.trim(), "", `${label}: expected JSON`);
  return JSON.parse(stream);
}

function assertValid(schema, value, label) {
  assert.deepEqual(validate(schema, value), [], label);
}

function assertInvalid(schema, value, label) {
  assert.ok(validate(schema, value).length > 0, `${label}: expected schema rejection`);
}

function clone(value) {
  return structuredClone(value);
}

let negativeSchemaCases = 0;
function mutate(base, label, mutation) {
  const value = clone(base);
  mutation(value);
  assertInvalid(outputSchema, value, label);
  negativeSchemaCases += 1;
}

const branchSamples = new Map();
const fixtureResults = [];
for (const group of ["allowed", "forbidden", "ambiguous", "malformed", "transform-required"]) {
  const fixtures = JSON.parse(readFileSync(join(root, `tests/fixtures/privacy/${group}.json`), "utf8"));
  for (const fixture of fixtures) {
    const run = invokeDecision(fixture.decision, `${group}-${fixture.id}`);
    assert.equal(run.status, fixture.expected.exitCode, `${group}/${fixture.id}: exit`);
    assert.equal(run.stderr, "", `${group}/${fixture.id}: no startup error`);
    const output = parsed(run.stdout, `${group}/${fixture.id}`);
    assertValid(outputSchema, output, `${group}/${fixture.id}: ExportDecisionOutput`);
    assert.equal(output.decision, fixture.expected.decision, `${group}/${fixture.id}: branch`);
    assert.ok(output.reasonCodes.includes(fixture.expected.reasonCode), `${group}/${fixture.id}: reason`);
    branchSamples.set(output.decision, branchSamples.get(output.decision) ?? output);
    fixtureResults.push({ id: `${group}/${fixture.id}`, exit: run.status, branch: output.decision });
  }
}
assert.deepEqual([...branchSamples.keys()].sort(), ["allow", "deny", "transform-required"]);
assert.equal(fixtureResults.length, 7);

const allow = branchSamples.get("allow");
const deny = branchSamples.get("deny");
const transform = branchSamples.get("transform-required");

for (const field of outputSchema.required) mutate(allow, `missing root ${field}`, (value) => { delete value[field]; });
for (const field of outputSchema.properties.effectiveRefs.required) {
  mutate(allow, `missing effective ref ${field}`, (value) => { delete value.effectiveRefs[field]; });
}
mutate(allow, "root additional property", (value) => { value.extra = true; });
mutate(allow, "effectiveRefs additional property", (value) => { value.effectiveRefs.extra = true; });
mutate(allow, "nested ref additional property", (value) => { value.effectiveRefs.policyRef.extra = true; });
mutate(allow, "empty reasonCodes", (value) => { value.reasonCodes = []; });
mutate(allow, "duplicate reasonCodes", (value) => { value.reasonCodes.push(value.reasonCodes[0]); });
mutate(allow, "non-string reasonCode", (value) => { value.reasonCodes = [1]; });
mutate(allow, "unknown transform", (value) => { value.requiredTransforms = ["caller-transform"]; });
mutate(transform, "duplicate transform", (value) => { value.requiredTransforms.push(value.requiredTransforms[0]); });
mutate(transform, "unknown derived requirement", (value) => { value.derivedArtifactRequirements[0] = "caller-requirement"; });
mutate(transform, "duplicate derived requirement", (value) => { value.derivedArtifactRequirements.push(value.derivedArtifactRequirements[0]); });
mutate(allow, "sourceTransferAllowed non-boolean", (value) => { value.sourceTransferAllowed = "false"; });

mutate(allow, "reviewer swapped classification version", (value) => { value.effectiveRefs.classificationContractRef.version = "1.1.0"; });
mutate(allow, "reviewer swapped evidence version", (value) => { value.effectiveRefs.authorizingEvidenceContractRef.version = "1.0.0"; });
mutate(allow, "stale classification digest", (value) => { value.effectiveRefs.classificationContractRef.digest = `sha256:${"0".repeat(64)}`; });
mutate(allow, "stale evidence digest", (value) => { value.effectiveRefs.authorizingEvidenceContractRef.digest = `sha256:${"0".repeat(64)}`; });
mutate(allow, "stale policy", (value) => { value.effectiveRefs.policyRef.version = "1.0.5"; });
mutate(allow, "stale decision", (value) => { value.effectiveRefs.decisionContractRef.version = "1.4.0"; });
mutate(allow, "stale input schema", (value) => { value.effectiveRefs.inputSchemaRef.version = "2.4.0"; });
mutate(allow, "stale output schema", (value) => { value.effectiveRefs.outputSchemaRef.version = "1.4.0"; });

mutate(allow, "allow with transforms", (value) => { value.requiredTransforms = ["redact-content"]; });
mutate(allow, "allow with derived requirements", (value) => { value.derivedArtifactRequirements = ["new-artifact"]; });
mutate(deny, "deny with source transfer", (value) => { value.sourceTransferAllowed = true; });
mutate(deny, "deny with transforms", (value) => { value.requiredTransforms = ["redact-content"]; });
mutate(transform, "transform without transforms", (value) => { value.requiredTransforms = []; });
mutate(transform, "transform without derived requirements", (value) => { value.derivedArtifactRequirements = []; });
mutate(transform, "transform with source transfer", (value) => { value.sourceTransferAllowed = true; });

const missingDecision = invoke(["--decision", join(temp, "missing-decision.json")]);
assert.equal(missingDecision.status, 1);
assert.equal(missingDecision.stdout, "");
const missingDecisionError = parsed(missingDecision.stderr, "missing decision startup error");
assertValid(cliErrorSchema, missingDecisionError, "missing decision exact CLI error protocol");
assertInvalid(outputSchema, missingDecisionError, "CLI startup error is not ExportDecisionOutput");

const oldSelection = invoke([
  "--manifest", join(root, "contracts/privacy-policy.v1.0.5.manifest.json"),
  "--manifest-sidecar", join(root, "contracts/privacy-policy.v1.0.5.manifest.sha256"),
  "--policy", join(root, "contracts/privacy-policy.v1.0.5.json"),
  "--policy-sidecar", join(root, "contracts/privacy-policy.v1.0.5.sha256"),
  "--input-schema", join(root, "contracts/privacy-export.schema.v2.4.json"),
  "--output-schema", join(root, "contracts/privacy-export.schema.v2.4.output.json"),
]);
assert.equal(oldSelection.status, 1);
assert.equal(oldSelection.stdout, "");
const oldSelectionError = parsed(oldSelection.stderr, "old 1.0.5 selection");
assertValid(cliErrorSchema, oldSelectionError, "old selection CLI error protocol");
assert.equal(oldSelectionError.reasonCodes[0], "custody.manifest-untrusted");

const mutatedOutput = clone(outputSchema);
mutatedOutput.properties.effectiveRefs.properties.classificationContractRef.properties.version.const = "1.1.0";
const mutatedOutputPath = join(temp, "mutated-output-schema.json");
writeFileSync(mutatedOutputPath, `${JSON.stringify(mutatedOutput, null, 2)}\n`);
const outputCustody = invoke(["--output-schema", mutatedOutputPath]);
assert.equal(outputCustody.status, 1);
assert.equal(parsed(outputCustody.stderr, "output custody").reasonCodes[0], "custody.output-schema-bytes-mismatch");

const mutatedCliError = clone(cliErrorSchema);
mutatedCliError.additionalProperties = true;
const mutatedCliErrorPath = join(temp, "mutated-cli-error-schema.json");
writeFileSync(mutatedCliErrorPath, `${JSON.stringify(mutatedCliError, null, 2)}\n`);
const cliErrorCustody = invoke(["--cli-error-schema", mutatedCliErrorPath]);
assert.equal(cliErrorCustody.status, 1);
assert.equal(parsed(cliErrorCustody.stderr, "CLI error schema custody").reasonCodes[0], "custody.cli-error-schema-bytes-mismatch");

const manifest = JSON.parse(readFileSync(join(root, "contracts/privacy-policy.v1.0.6.manifest.json"), "utf8"));
manifest.acceptedContracts[0].outputSchemaRef.digest = `sha256:${"0".repeat(64)}`;
const mutatedManifestBytes = Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`);
const mutatedManifestPath = join(temp, "mutated-manifest.json");
const mutatedManifestSidecarPath = join(temp, "mutated-manifest.sha256");
writeFileSync(mutatedManifestPath, mutatedManifestBytes);
writeFileSync(mutatedManifestSidecarPath,
  `${createHash("sha256").update(mutatedManifestBytes).digest("hex")}  privacy-policy.v1.0.6.manifest.json\n`);
const manifestCustody = invoke(["--manifest", mutatedManifestPath, "--manifest-sidecar", mutatedManifestSidecarPath]);
assert.equal(manifestCustody.status, 1);
assert.equal(parsed(manifestCustody.stderr, "manifest custody").reasonCodes[0], "custody.manifest-untrusted");

console.log(`privacy output-schema conformance: ${fixtureResults.length} subprocess fixtures; allow/deny/transform-required; exit-1 CLI error protocol; ${negativeSchemaCases} schema-negative cases; 5 startup/lifecycle/custody cases`);
