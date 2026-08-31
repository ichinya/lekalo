#!/usr/bin/env node

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { evaluateDecision, loadTrustedContext } from "./check-privacy.mjs";
import { authorizingEvidence, refreshEvidenceBindings, setProvenanceEvidence } from "./privacy-test-helpers.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const context = await loadTrustedContext();
const base = JSON.parse(await readFile(`${root}/tests/fixtures/privacy/allowed.json`, "utf8"))[0].decision;
const clone = (value) => structuredClone(value);
const REPO_ONE = `repo-sha256:${"1".repeat(64)}`;
const REPO_TWO = `repo-sha256:${"2".repeat(64)}`;
let vectorCount = 0;
const digest = (character) => `sha256:${character.repeat(64)}`;
const audit = (id, character = "a") => ({ id, version: "1.0.0", evidenceDigest: digest(character) });

function expect(input, decision, reason = null, malformed = false) {
  refreshEvidenceBindings(input);
  const result = evaluateDecision(input, context);
  assert.equal(result.malformed, malformed, `malformed mismatch for ${reason ?? decision}`);
  assert.equal(result.output.decision, decision, `decision mismatch for ${reason ?? decision}`);
  if (reason) assert.equal(result.output.reasonCodes[0], reason);
  assert.ok(result.output.reasonCodes.length > 0);
  assert.deepEqual(result.output.effectiveRefs.policyRef, context.policy.policyRef);
  assert.deepEqual(result.output.effectiveRefs.authorityRef, context.policy.authorityRef);
  assert.deepEqual(result.output.effectiveRefs.classificationContractRef, context.policy.classificationContractRef);
  assert.deepEqual(result.output.effectiveRefs.authorizingEvidenceContractRef, context.policy.authorizingEvidenceContractRef);
  assert.deepEqual(result.output.effectiveRefs.authorizationSubjectProfileRef, context.policy.authorizationSubjectProfileRef);
  if (decision === "transform-required") {
    assert.equal(result.output.sourceTransferAllowed, false);
    assert.ok(result.output.requiredTransforms.length > 0);
    assert.ok(result.output.derivedArtifactRequirements.includes("reevaluate-derived-output"));
  }
  vectorCount += 1;
  return result.output;
}

function destination(input, trustBoundary) {
  const profiles = {
    "same-local-workspace": ["local-use", "local-workspace", null, "not-applicable", "same-tenant", "operator-only"],
    "same-repository": ["repository-store", "consumer-repository", REPO_ONE, "same-origin", "same-tenant", "repository-collaborators"],
    "same-tenant": ["transfer", "external-repository", REPO_TWO, "different-repository", "same-tenant", "tenant-members"],
    "cross-repository": ["transfer", "external-repository", REPO_TWO, "different-repository", "same-tenant", "named-external"],
    "cross-tenant": ["transfer", "external-repository", REPO_TWO, "different-repository", "cross-tenant", "named-external"],
    public: ["publish", "public-channel", null, "not-applicable", "not-applicable", "public"],
  };
  const [operation, role, repositoryRef, repositoryRelation, tenantRelation, audience] = profiles[trustBoundary];
  if (!["same-local-workspace", "public"].includes(trustBoundary)) {
    input.source = { repositoryRole: "consumer-repository", repositoryRef: REPO_ONE };
    input.provenance.repositoryRole = "consumer-repository";
    input.provenance.repositoryRef = REPO_ONE;
    input.provenance.origin = "consumer-repository";
    input.provenance.synthetic = false;
    input.provenance.derived = false;
  }
  input.operation.id = operation;
  input.destination = { repositoryRole: role, repositoryRef, trustBoundary, repositoryRelation, tenantRelation };
  input.audience = audience;
  for (const field of ["publicFixturePermissionRef", "publicFixtureLicenseRef", "publicFixtureConsentRef",
    "consumerAclPermissionRef", "consumerRepositoryConsentRef", "exportTransferConsentRef"]) input.provenance[field] = null;
  if (input.exportDisposition === "public-fixture" && input.provenance.origin !== "synthetic"
    && ["repository-store", "transfer", "publish"].includes(operation)) {
    setProvenanceEvidence(input, "publicFixturePermissionRef", "4");
    setProvenanceEvidence(input, "publicFixtureLicenseRef", "5");
    setProvenanceEvidence(input, "publicFixtureConsentRef", "6");
  } else if (input.exportDisposition === "consumer-repository-only" && operation === "repository-store") {
    setProvenanceEvidence(input, "consumerAclPermissionRef", "7");
    setProvenanceEvidence(input, "consumerRepositoryConsentRef", "8");
  } else if (["repository-store", "transfer", "publish"].includes(operation)
    && input.exportDisposition !== "public-fixture") {
    setProvenanceEvidence(input, "exportTransferConsentRef", "9");
  }
  return input;
}

function localFor(kind, disposition, labels = ["public"]) {
  const input = destination(clone(base), "same-local-workspace");
  input.artifactKind = kind;
  input.exportDisposition = disposition;
  input.dataSensitivity = labels;
  return input;
}

// Every destination/trust boundary and its matching audience/operation.
for (const boundary of context.policy.vocabularies.trustBoundary) {
  const input = destination(clone(base), boundary);
  expect(input, "allow", "policy.allow");
}

// Every operation independently.
for (const operation of context.policy.vocabularies.operation) {
  const input = clone(base);
  if (operation === "local-use" || operation === "derive") destination(input, "same-local-workspace");
  else if (operation === "repository-store") destination(input, "same-repository");
  else if (operation === "transfer") destination(input, "same-tenant");
  else destination(input, "public");
  input.operation.id = operation;
  expect(input, "allow", "policy.allow");
}

// All five audiences are independently exercised by the six destination vectors above.
const exercisedAudiences = new Set(context.policy.vocabularies.trustBoundary.map((boundary) => destination(clone(base), boundary).audience));
assert.deepEqual(exercisedAudiences, new Set(context.policy.vocabularies.audience));

// Every sensitivity label is legal locally; restrictive labels deny broader destinations by intersection.
for (const label of context.policy.vocabularies.dataSensitivity) {
  expect(localFor("fixture", "public-fixture", [label]), "allow", "policy.allow");
}
for (const [label, boundary] of [["credential-secret", "public"], ["personal-pii", "public"], ["tenant-scoped", "cross-tenant"], ["internal", "cross-repository"]]) {
  const input = destination(clone(base), boundary);
  input.dataSensitivity = ["public", label];
  expect(input, "deny", `sensitivity.${label}.denied`);
}

// Multi-label ordering is deterministic, while duplicates are malformed.
const labelsA = destination(clone(base), "public");
labelsA.dataSensitivity = ["tenant-scoped", "public"];
const labelsB = clone(labelsA);
labelsB.dataSensitivity.reverse();
assert.deepEqual(expect(labelsA, "deny", "sensitivity.tenant-scoped.denied"), expect(labelsB, "deny", "sensitivity.tenant-scoped.denied"));
const duplicate = clone(base);
duplicate.dataSensitivity = ["public", "public"];
expect(duplicate, "deny", "input.data-sensitivity", true);

// All six dispositions have independent semantic vectors.
expect(localFor("context.capsule", "local-private"), "allow", "policy.allow");
const localPrivateRepo = destination(localFor("context.capsule", "local-private"), "same-repository");
expect(localPrivateRepo, "deny", "disposition.local-private.operation-denied");
const consumerStore = destination(localFor("consumer.model", "consumer-repository-only", ["internal"]), "same-repository");
consumerStore.source.repositoryRole = "consumer-repository";
consumerStore.source.repositoryRef = REPO_ONE;
consumerStore.provenance.repositoryRole = "consumer-repository";
consumerStore.provenance.repositoryRef = REPO_ONE;
consumerStore.provenance.origin = "consumer-repository";
consumerStore.provenance.synthetic = false;
expect(consumerStore, "allow", "policy.allow");
const consumerWrongOrigin = clone(consumerStore);
consumerWrongOrigin.source.repositoryRole = "lekalo-repository";
consumerWrongOrigin.provenance.repositoryRole = "lekalo-repository";
consumerWrongOrigin.provenance.origin = "lekalo-repository";
consumerWrongOrigin.destination.repositoryRole = "lekalo-repository";
refreshEvidenceBindings(consumerWrongOrigin);
expect(consumerWrongOrigin, "deny", "disposition.consumer-origin-acl-only");
const consumerCrossRepo = destination(clone(consumerStore), "cross-repository");
expect(consumerCrossRepo, "deny", "disposition.consumer-repository-only.operation-denied");

// Cross-field repository and provenance coherence closes relabelled same-origin writes.
const contradictoryConsumerLekalo = clone(consumerStore);
contradictoryConsumerLekalo.destination.repositoryRole = "lekalo-repository";
expect(contradictoryConsumerLekalo, "deny", "repository.same-origin-contradiction");
const contradictorySameRepoRef = clone(consumerStore);
contradictorySameRepoRef.destination.repositoryRef = REPO_TWO;
expect(contradictorySameRepoRef, "deny", "repository.same-origin-contradiction");
const contradictorySameRepoRelation = clone(consumerStore);
contradictorySameRepoRelation.destination.repositoryRelation = "different-repository";
expect(contradictorySameRepoRelation, "deny", "repository.same-origin-contradiction");
const contradictoryDifferentRepoRef = destination(clone(base), "cross-repository");
contradictoryDifferentRepoRef.destination.repositoryRef = contradictoryDifferentRepoRef.source.repositoryRef;
expect(contradictoryDifferentRepoRef, "deny", "repository.different-origin-contradiction");
const contradictoryDifferentRepoRelation = destination(clone(base), "cross-repository");
contradictoryDifferentRepoRelation.destination.repositoryRelation = "same-origin";
expect(contradictoryDifferentRepoRelation, "deny", "repository.different-origin-contradiction");
const contradictoryTenant = destination(clone(base), "cross-tenant");
contradictoryTenant.destination.tenantRelation = "same-tenant";
expect(contradictoryTenant, "deny", "repository.cross-tenant-contradiction");
const contradictoryLocalRef = destination(clone(base), "same-local-workspace");
contradictoryLocalRef.destination.repositoryRef = REPO_TWO;
expect(contradictoryLocalRef, "deny", "repository.destination-ref-role-mismatch");
const contradictoryPublicRef = destination(clone(base), "public");
contradictoryPublicRef.destination.repositoryRef = REPO_TWO;
expect(contradictoryPublicRef, "deny", "repository.destination-ref-role-mismatch");
const provenanceRoleMismatch = clone(consumerStore);
provenanceRoleMismatch.provenance.repositoryRole = "lekalo-repository";
expect(provenanceRoleMismatch, "deny", "provenance.source-repository-mismatch");
const provenanceRefMismatch = clone(consumerStore);
provenanceRefMismatch.provenance.repositoryRef = REPO_TWO;
expect(provenanceRefMismatch, "deny", "provenance.source-repository-mismatch");
const syntheticBooleanConflict = clone(base);
syntheticBooleanConflict.provenance.synthetic = false;
expect(syntheticBooleanConflict, "deny", "provenance.origin-boolean-conflict");
const derivedBooleanConflict = clone(base);
derivedBooleanConflict.provenance.origin = "derived";
expect(derivedBooleanConflict, "deny", "provenance.origin-boolean-conflict");
const originRoleConflict = clone(consumerStore);
originRoleConflict.provenance.origin = "lekalo-repository";
expect(originRoleConflict, "deny", "provenance.origin-role-conflict");
expect(localFor("generated.summary", "shareable-with-redaction"), "allow", "policy.allow");
const shareablePublish = destination(localFor("generated.summary", "shareable-with-redaction"), "public");
expect(shareablePublish, "transform-required", "disposition.transform-required");
expect(destination(clone(base), "public"), "allow", "policy.allow");
expect(localFor("ai.prompt", "forbidden-to-export", ["confidential"]), "allow", "policy.allow");
const forbiddenPublish = destination(localFor("ai.prompt", "forbidden-to-export"), "public");
expect(forbiddenPublish, "deny", "disposition.forbidden-dominates");
const aggregateMissing = destination(localFor("aggregate.artifact", "public-aggregate"), "public");
expect(aggregateMissing, "deny", "derived.public-aggregate-required");

// Conflict, consent, fixture permission, disposition and exact-ref cases.
for (const state of ["unresolved", "resolved-deny"]) {
  const input = clone(base);
  input.conflictResolution = { state, decisionRef: null };
  if (state === "resolved-deny") input.conflictResolution.decisionRef = authorizingEvidence(input, "conflictResolution", "a", { outcome: "deny" });
  expect(input, "deny", "conflict.deny");
}
const missingConsent = clone(shareablePublish);
missingConsent.provenance.exportTransferConsentRef = null;
expect(missingConsent, "deny", "provenance.consent-required");
const nonSynthetic = clone(base);
nonSynthetic.provenance.synthetic = false;
nonSynthetic.provenance.origin = "external";
nonSynthetic.source = { repositoryRole: "external-repository", repositoryRef: REPO_ONE };
nonSynthetic.provenance.repositoryRole = "external-repository";
nonSynthetic.provenance.repositoryRef = REPO_ONE;
refreshEvidenceBindings(nonSynthetic);
expect(nonSynthetic, "deny", "fixture.permission-license-consent-required");
const dispositionMismatch = clone(base);
dispositionMismatch.exportDisposition = "local-private";
expect(dispositionMismatch, "deny", "classification.disposition-default-mismatch");
for (const refName of ["policyRef", "authorityRef", "decisionContractRef"]) {
  const input = clone(base);
  input[refName].version = "0.0.0";
  expect(input, "deny", `input.${refName === "decisionContractRef" ? "decision-ref" : refName === "authorityRef" ? "authority-ref" : "policy-ref"}`, true);
}

// Path representation: valid POSIX project-relative, withheld-local, and adversarial Windows/URI/traversal forms.
const withheldLocal = localFor("fixture", "public-fixture");
withheldLocal.resourcePath = { state: "withheld" };
expect(withheldLocal, "allow", "policy.allow");
for (const state of ["unknown", "unsupported"]) {
  const input = localFor("fixture", "public-fixture");
  input.resourcePath = { state };
  expect(input, "deny", `path.${state}`);
}
for (const path of [
  "C:/private/file.txt", "//server/share/file.txt", "/absolute/file.txt", "~/secret", "file:///private/file.txt",
  "a/../secret", "a\\secret", "a/%2e%2e/secret", "a/./b", "a//b", "a/trailing. ", "a/file:stream",
  "a/CON.txt", "a/PROGRA~1/file", `a/${"e\u0301"}.txt`,
  "a/COM¹.txt", "a/com²", "a/CoM³.bin", "a/LPT¹.txt", "a/lpt²", "a/LpT³.bin",
  "a/CONIN$", "a/conin$.txt", "a/CONOUT$", "a/conout$.log", "a/CLOCK$", "a/clock$.txt",
]) {
  const input = clone(base);
  input.resourcePath = { state: "known", value: path };
  expect(input, "deny", "path.not-normalized-project-relative");
}
const privateIdentity = clone(base);
privateIdentity.source = { repositoryRole: "consumer-repository", repositoryUrl: "https://private.example/repo" };
expect(privateIdentity, "deny", "input.source", true);
const localAlias = clone(base);
localAlias.source.repositoryRole = "my-consumer-repo";
expect(localAlias, "deny", "input.source", true);

// Known numeric zero is data, not a missing state. Non-known measured states deny distinctly.
const zero = clone(base);
zero.valueState = { state: "known", value: 0 };
expect(zero, "allow", "policy.allow");
for (const state of ["unknown", "withheld", "unsupported"]) {
  const input = clone(base);
  input.valueState = { state };
  expect(input, "deny", `value-state.${state}`);
}
const illegalUnknownValue = clone(base);
illegalUnknownValue.valueState = { state: "unknown", value: 0 };
expect(illegalUnknownValue, "deny", "input.value-state", true);

// Project/profile/operation constraints only intersect the baseline; no caller grant can broaden it.
const narrowedAllow = clone(base);
narrowedAllow.constraints = [{
  scope: "operation",
  constraintRef: audit("constraint.operation"),
  allowedOperations: ["publish"],
  allowedTrustBoundaries: ["public"],
  allowedAudiences: ["public"],
}];
expect(narrowedAllow, "allow", "policy.allow");
const narrowedDeny = clone(narrowedAllow);
narrowedDeny.constraints[0].allowedOperations = [];
expect(narrowedDeny, "deny", "constraint.narrowed-deny");
const localBroadening = clone(narrowedAllow);
localBroadening.constraints[0].allowedTrustBoundaries.push("same-repository");
expect(localBroadening, "deny", "constraint.broadening-forbidden");
const callerGrant = clone(base);
callerGrant.broadeningGrant = {
  grantId: "caller.local-grant", version: "1.0.0", policyRef: clone(context.policy.policyRef), reviewRef: audit("caller.review"),
};
expect(callerGrant, "deny", "input.broadening-grant", true);

console.log(JSON.stringify({
  ok: true,
  vectorCount,
  covered: {
    dispositions: context.policy.vocabularies.exportDisposition.length,
    operations: context.policy.vocabularies.operation.length,
    trustBoundaries: context.policy.vocabularies.trustBoundary.length,
    audiences: context.policy.vocabularies.audience.length,
    sensitivityLabels: context.policy.vocabularies.dataSensitivity.length,
    outputBranches: 3,
    valueStates: context.policy.vocabularies.valueState.length,
  },
}));
