#!/usr/bin/env node

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import {
  AUTHORITY_REF,
  CLASSIFICATION_CONTRACT_REF,
  POLICY_REF,
  evaluateDecision,
  loadTrustedContext,
} from "./check-privacy.mjs";
import { authorizingEvidence, refreshEvidenceBindings, setProvenanceEvidence } from "./privacy-test-helpers.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const context = await loadTrustedContext();
const schema = context.inputSchema;
const clone = (value) => structuredClone(value);
const digest = (character) => `sha256:${character.repeat(64)}`;
const audit = (id, character) => ({ id, version: "1.0.0", evidenceDigest: digest(character) });
const classification = (character) => ({
  ...CLASSIFICATION_CONTRACT_REF,
  decisionId: `classification-sha256:${character.repeat(64)}`,
  evidenceDigest: digest(character),
});
const sourceRef = (character) => `source-sha256:${character.repeat(64)}`;

function resolveRef(ref) {
  assert.match(ref, /^#\/\$defs\//);
  return schema.$defs[ref.slice("#/$defs/".length)];
}

function typeMatches(value, type) {
  if (Array.isArray(type)) return type.some((entry) => typeMatches(value, entry));
  if (type === "null") return value === null;
  if (type === "array") return Array.isArray(value);
  if (type === "object") return value !== null && typeof value === "object" && !Array.isArray(value);
  if (type === "number") return typeof value === "number" && Number.isFinite(value);
  return typeof value === type;
}

function schemaAccepts(value, rule) {
  if (rule.$ref) return schemaAccepts(value, resolveRef(rule.$ref));
  if (rule.oneOf) return rule.oneOf.filter((entry) => schemaAccepts(value, entry)).length === 1;
  if (Object.hasOwn(rule, "const") && value !== rule.const) return false;
  if (rule.enum && !rule.enum.some((entry) => Object.is(entry, value))) return false;
  if (rule.type && !typeMatches(value, rule.type)) return false;
  if (typeof value === "string") {
    if (rule.minLength !== undefined && value.length < rule.minLength) return false;
    if (rule.pattern && !new RegExp(rule.pattern).test(value)) return false;
  }
  if (Array.isArray(value)) {
    if (rule.minItems !== undefined && value.length < rule.minItems) return false;
    if (rule.uniqueItems && new Set(value.map((entry) => JSON.stringify(entry))).size !== value.length) return false;
    if (rule.items && value.some((entry) => !schemaAccepts(entry, rule.items))) return false;
  }
  if (value !== null && typeof value === "object" && !Array.isArray(value)) {
    if (rule.required && rule.required.some((key) => !Object.hasOwn(value, key))) return false;
    if (rule.additionalProperties === false && Object.keys(value).some((key) => !Object.hasOwn(rule.properties ?? {}, key))) return false;
    for (const [key, child] of Object.entries(rule.properties ?? {})) {
      if (Object.hasOwn(value, key) && !schemaAccepts(value[key], child)) return false;
    }
  }
  return true;
}

const base = JSON.parse(await readFile(`${root}/tests/fixtures/privacy/allowed.json`, "utf8"))[0].decision;
const aggregate = clone(base);
aggregate.artifactKind = "aggregate.artifact";
aggregate.exportDisposition = "public-aggregate";
aggregate.provenance.origin = "derived";
aggregate.provenance.derived = true;
aggregate.provenance.synthetic = false;
aggregate.derivedArtifact = {
  sourceArtifacts: [{
    sourceRef: sourceRef("a"), artifactKind: "metrics.evaluation-evidence",
    authorityRef: clone(AUTHORITY_REF), policyRef: clone(POLICY_REF), classificationRef: classification("c"),
    dataSensitivity: ["internal"], exportDisposition: "shareable-with-redaction",
  }],
  appliedTransforms: [{ transformId: "aggregate-no-source-rows", version: "1.0.0", evidenceDigest: digest("d") }],
  declassificationDecision: {
    policyRef: clone(POLICY_REF), decisionRef: null, version: "1.0.0", outcome: "approved", removedSensitivities: ["internal"],
  },
  aggregationDecision: {
    policyRef: clone(POLICY_REF), decisionRef: null, version: "1.0.0", outcome: "approved", removesSourceRows: true, removesSourceIdentities: true,
  },
  containsSourceRows: false, containsSourceIdentities: false, reevaluated: true,
};
aggregate.derivedArtifact.declassificationDecision.decisionRef = authorizingEvidence(aggregate, "declassification", "e");
aggregate.derivedArtifact.aggregationDecision.decisionRef = authorizingEvidence(aggregate, "aggregation", "f");
setProvenanceEvidence(aggregate, "exportTransferConsentRef", "6");

let vectorCount = 0;
function expectParity(name, input, expectedSchema, expectedMalformed, expectedReason = null) {
  const schemaResult = schemaAccepts(input, schema);
  const runtimeResult = evaluateDecision(input, context);
  assert.equal(schemaResult, expectedSchema, `${name}: schema`);
  assert.equal(runtimeResult.malformed, expectedMalformed, `${name}: runtime malformed`);
  assert.equal(schemaResult, !runtimeResult.malformed, `${name}: schema/runtime closure`);
  if (expectedReason !== null) assert.equal(runtimeResult.output.reasonCodes[0], expectedReason, `${name}: reason`);
  vectorCount += 1;
}

function mutated(seed, mutate) {
  const input = clone(seed);
  mutate(input);
  return input;
}

expectParity("valid-main", base, true, false);
expectParity("valid-derived", aggregate, true, false);

for (const [name, mutate] of [
  ["unknown-main-label", (input) => { input.dataSensitivity = ["caller-label"]; }],
  ["unknown-main-disposition", (input) => { input.exportDisposition = "caller-disposition"; }],
  ["unknown-nested-label", (input) => { input.derivedArtifact.sourceArtifacts[0].dataSensitivity = ["caller-label"]; }],
  ["unknown-nested-disposition", (input) => { input.derivedArtifact.sourceArtifacts[0].exportDisposition = "caller-disposition"; }],
  ["stale-classification-version", (input) => { input.derivedArtifact.sourceArtifacts[0].classificationRef.version = "99.99.99"; }],
  ["unknown-classification-digest", (input) => { input.derivedArtifact.sourceArtifacts[0].classificationRef.digest = digest("0"); }],
  ["invalid-source-ref", (input) => { input.derivedArtifact.sourceArtifacts[0].sourceRef = "source:private-name"; }],
  ["nested-extra-field", (input) => { input.derivedArtifact.sourceArtifacts[0].privateRepository = "forbidden"; }],
  ["invalid-repository-ref", (input) => { input.source.repositoryRef = "private/repository/name"; }],
  ["root-extra-field", (input) => { input.localOverride = true; }],
]) {
  const input = clone(name.includes("nested") || name.includes("classification") || name.includes("source-ref") ? aggregate : base);
  mutate(input);
  expectParity(name, input, false, true);
}

const provenanceEvidenceFields = [
  "publicFixturePermissionRef",
  "publicFixtureLicenseRef",
  "publicFixtureConsentRef",
  "consumerAclPermissionRef",
  "consumerRepositoryConsentRef",
  "exportTransferConsentRef",
];
for (const [fieldIndex, field] of provenanceEvidenceFields.entries()) {
  for (const [variant, mutateEvidence] of [
    ["unknown-contract", (evidence) => { evidence.contractId = "caller.generic"; }],
    ["stale-contract-version", (evidence) => { evidence.version = "99.99.99"; }],
    ["wrong-purpose", (evidence) => { evidence.purpose = "authorize-caller-purpose"; }],
    ["extra-field", (evidence) => { evidence.auditRef = "caller.says.allow"; }],
  ]) {
    const input = clone(base);
    setProvenanceEvidence(input, field, String(fieldIndex + 1));
    mutateEvidence(input.provenance[field]);
    expectParity(`evidence-${field}-${variant}`, input, false, true);
  }
}

for (const [variant, mutateEvidence] of [
  ["unverified", (evidence) => { evidence.verificationState = "unverified"; }],
  ["stale", (evidence) => { evidence.freshnessState = "stale"; }],
  ["expired", (evidence) => { evidence.freshnessState = "expired"; }],
  ["wrong-outcome", (evidence) => { evidence.outcome = "denied"; }],
  ["binding-mismatch", (evidence) => { evidence.binding.subjectDigest = `subject-sha256:${"9".repeat(64)}`; }],
]) {
  const input = clone(base);
  setProvenanceEvidence(input, "publicFixturePermissionRef", "7");
  mutateEvidence(input.provenance.publicFixturePermissionRef);
  expectParity(`evidence-semantic-${variant}`, input, true, false);
}

for (const [name, seed, locate] of [
  ["conflict", (() => { const input = clone(base); input.conflictResolution = { state: "resolved-allow", decisionRef: authorizingEvidence(input, "conflictResolution", "8") }; return input; })(),
    (input) => input.conflictResolution.decisionRef],
  ["declassification", aggregate, (input) => input.derivedArtifact.declassificationDecision.decisionRef],
  ["aggregation", aggregate, (input) => input.derivedArtifact.aggregationDecision.decisionRef],
]) {
  for (const [variant, mutateEvidence] of [
    ["unknown-contract", (evidence) => { evidence.contractId = "caller.says.allow"; }],
    ["stale-contract-version", (evidence) => { evidence.version = "99.99.99"; }],
    ["wrong-purpose", (evidence) => { evidence.purpose = "authorize-unrelated-purpose"; }],
  ]) {
    const input = clone(seed);
    mutateEvidence(locate(input));
    expectParity(`${name}-evidence-${variant}`, input, false, true);
  }
}

const duplicate = clone(aggregate);
duplicate.derivedArtifact.sourceArtifacts.push(clone(duplicate.derivedArtifact.sourceArtifacts[0]));
expectParity("exact-duplicate-source", duplicate, false, true);

const conflicting = clone(aggregate);
const conflictEntry = clone(conflicting.derivedArtifact.sourceArtifacts[0]);
conflictEntry.classificationRef = classification("d");
  conflicting.derivedArtifact.sourceArtifacts.push(conflictEntry);
  refreshEvidenceBindings(conflicting);
assert.equal(schemaAccepts(conflicting, schema), true, "cross-entry conflict is schema-shaped");
const conflictRuntime = evaluateDecision(conflicting, context);
assert.equal(conflictRuntime.malformed, false);
assert.equal(conflictRuntime.output.reasonCodes[0], "derived.source-ref-conflict");
vectorCount += 1;

const validConstraint = {
  scope: "operation",
  constraintRef: audit("constraint.operation", "7"),
  allowedOperations: ["publish"],
  allowedTrustBoundaries: ["public"],
  allowedAudiences: ["public"],
};
for (const [name, seed, mutateInput] of [
  ["root-authority-extra", base, (input) => { input.authorityRef.extra = true; }],
  ["root-policy-extra", base, (input) => { input.policyRef.extra = true; }],
  ["operation-extra", base, (input) => { input.operation.extra = true; }],
  ["source-extra", base, (input) => { input.source.extra = true; }],
  ["destination-extra", base, (input) => { input.destination.extra = true; }],
  ["provenance-extra", base, (input) => { input.provenance.extra = true; }],
  ["classification-extra", base, (input) => { input.provenance.classificationRef.extra = true; }],
  ["permission-ref-extra", base, (input) => { setProvenanceEvidence(input, "publicFixturePermissionRef", "1"); input.provenance.publicFixturePermissionRef.extra = true; }],
  ["license-ref-extra", base, (input) => { setProvenanceEvidence(input, "publicFixtureLicenseRef", "2"); input.provenance.publicFixtureLicenseRef.extra = true; }],
  ["consent-ref-extra", base, (input) => { setProvenanceEvidence(input, "publicFixtureConsentRef", "3"); input.provenance.publicFixtureConsentRef.extra = true; }],
  ["path-extra", base, (input) => { input.resourcePath.extra = true; }],
  ["value-state-extra", base, (input) => { input.valueState.extra = true; }],
  ["conflict-extra", base, (input) => { input.conflictResolution.extra = true; }],
  ["nested-authority-extra", aggregate, (input) => { input.derivedArtifact.sourceArtifacts[0].authorityRef.extra = true; }],
  ["nested-policy-extra", aggregate, (input) => { input.derivedArtifact.sourceArtifacts[0].policyRef.extra = true; }],
  ["nested-classification-extra", aggregate, (input) => { input.derivedArtifact.sourceArtifacts[0].classificationRef.extra = true; }],
  ["transform-extra", aggregate, (input) => { input.derivedArtifact.appliedTransforms[0].extra = true; }],
  ["derived-extra", aggregate, (input) => { input.derivedArtifact.extra = true; }],
  ["declassification-extra", aggregate, (input) => { input.derivedArtifact.declassificationDecision.extra = true; }],
  ["declassification-policy-extra", aggregate, (input) => { input.derivedArtifact.declassificationDecision.policyRef.extra = true; }],
  ["declassification-decision-ref-extra", aggregate, (input) => { input.derivedArtifact.declassificationDecision.decisionRef.extra = true; }],
  ["aggregation-extra", aggregate, (input) => { input.derivedArtifact.aggregationDecision.extra = true; }],
  ["aggregation-policy-extra", aggregate, (input) => { input.derivedArtifact.aggregationDecision.policyRef.extra = true; }],
  ["aggregation-decision-ref-extra", aggregate, (input) => { input.derivedArtifact.aggregationDecision.decisionRef.extra = true; }],
  ["declassification-number-label", aggregate, (input) => { input.derivedArtifact.declassificationDecision.removedSensitivities = [1]; }],
  ["declassification-unknown-label", aggregate, (input) => { input.derivedArtifact.declassificationDecision.removedSensitivities = ["caller-label"]; }],
  ["declassification-stale-version", aggregate, (input) => { input.derivedArtifact.declassificationDecision.version = "0.9.0"; }],
  ["aggregation-stale-version", aggregate, (input) => { input.derivedArtifact.aggregationDecision.version = "0.9.0"; }],
]) {
  expectParity(name, mutated(seed, mutateInput), false, true);
}

for (const [state, hasRef, outcome] of [
  ["none", false, null],
  ["none", true, "allow"],
  ["unresolved", false, null],
  ["unresolved", true, "allow"],
  ["resolved-allow", true, "allow"],
  ["resolved-allow", false, null],
  ["resolved-deny", true, "deny"],
  ["resolved-deny", false, null],
]) {
  const input = clone(base);
  input.conflictResolution = { state, decisionRef: null };
  if (hasRef) input.conflictResolution.decisionRef = authorizingEvidence(input, "conflictResolution", "3", { outcome });
  expectParity(`conflict-coupling-${state}-${hasRef ? "ref" : "null"}`, input, true, false);
}

for (const [name, grant] of [
  ["grant-generic-audit", { id: "grant.reviewed", version: "1.0.0", evidenceDigest: digest("8") }],
  ["grant-empty-object", {}],
  ["grant-string", "caller-grant"],
]) {
  const input = clone(base);
  input.broadeningGrant = clone(grant);
  expectParity(name, input, false, true);
}

const duplicateMainLabels = clone(base);
duplicateMainLabels.dataSensitivity.push(duplicateMainLabels.dataSensitivity[0]);
expectParity("duplicate-main-sensitivity", duplicateMainLabels, false, true);

const duplicateConstraints = clone(base);
duplicateConstraints.constraints = [clone(validConstraint), clone(validConstraint)];
expectParity("duplicate-constraints", duplicateConstraints, false, true);

for (const [name, property] of [
  ["duplicate-constraint-operation", "allowedOperations"],
  ["duplicate-constraint-boundary", "allowedTrustBoundaries"],
  ["duplicate-constraint-audience", "allowedAudiences"],
]) {
  const input = clone(base);
  input.constraints = [clone(validConstraint)];
  input.constraints[0][property].push(input.constraints[0][property][0]);
  expectParity(name, input, false, true);
}

const duplicateTransforms = clone(aggregate);
duplicateTransforms.derivedArtifact.appliedTransforms.push(clone(duplicateTransforms.derivedArtifact.appliedTransforms[0]));
expectParity("duplicate-transforms", duplicateTransforms, false, true);

const conflictingTransformIdentity = clone(aggregate);
const secondTransform = clone(conflictingTransformIdentity.derivedArtifact.appliedTransforms[0]);
secondTransform.evidenceDigest = digest("9");
conflictingTransformIdentity.derivedArtifact.appliedTransforms.push(secondTransform);
refreshEvidenceBindings(conflictingTransformIdentity);
expectParity("conflicting-transform-identity", conflictingTransformIdentity, true, false, "derived.transform-id-conflict");

const duplicateRemoved = clone(aggregate);
duplicateRemoved.derivedArtifact.declassificationDecision.removedSensitivities.push("internal");
expectParity("duplicate-removed-sensitivities", duplicateRemoved, false, true);

const constraintExact = clone(base);
constraintExact.constraints = [clone(validConstraint)];
expectParity("valid-exact-constraint", constraintExact, true, false);

assert.ok(vectorCount >= 90, `expected expanded parity matrix, got ${vectorCount}`);
console.log(JSON.stringify({
  ok: true,
  vectorCount,
  nestedVocabulariesClosed: true,
  independentSchemaSubsetValidator: true,
  schemaInvalidAlwaysMalformed: true,
  schemaValidNeverShapeRejected: true,
}));
