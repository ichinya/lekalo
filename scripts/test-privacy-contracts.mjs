#!/usr/bin/env node

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  AUTHORITY_REF,
  AUTHORIZING_EVIDENCE_CONTRACT_REF,
  AUTHORIZATION_SUBJECT_PROFILE_REF,
  CLASSIFICATION_CONTRACT_REF,
  POLICY_REF,
  evaluateDecision,
  loadTrustedContext,
} from "./check-privacy.mjs";
import { authorizingEvidence, refreshEvidenceBindings, setProvenanceEvidence } from "./privacy-test-helpers.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const temp = await mkdtemp(join(tmpdir(), "lekalo-privacy-contracts-"));
const context = await loadTrustedContext();
const clone = (value) => structuredClone(value);
const hex = (character) => `sha256:${character.repeat(64)}`;
const audit = (id, character = "b") => ({ id, version: "1.0.0", evidenceDigest: hex(character) });
const classification = (character = "c") => ({
  ...CLASSIFICATION_CONTRACT_REF,
  decisionId: `classification-sha256:${character.repeat(64)}`,
  evidenceDigest: hex(character),
});
const sourceRef = (character = "a") => `source-sha256:${character.repeat(64)}`;

function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
  return JSON.stringify(value);
}

function identityDigest(policy) {
  const projected = clone(policy);
  delete projected.policyRef.digest;
  return `sha256:${createHash("sha256").update(canonical(projected)).digest("hex")}`;
}

async function expectCustodyFailure(options, code) {
  await assert.rejects(() => loadTrustedContext(options), (error) => {
    assert.match(error.message, new RegExp(code.replaceAll(".", "\\.")));
    return true;
  });
}

let policyMutationCount = 0;
async function expectPolicyMutation(name, mutator) {
  const mutated = clone(context.policy);
  mutator(mutated);
  mutated.policyRef.digest = identityDigest(mutated);
  const path = join(temp, `mutated-policy-${name}.json`);
  await writeFile(path, `${JSON.stringify(mutated, null, 2)}\n`);
  await expectCustodyFailure({ policy: path }, "custody.policy-bytes-mismatch");
  policyMutationCount += 1;
}

function evaluate(input, decision, code, malformed = false) {
  refreshEvidenceBindings(input);
  const actual = evaluateDecision(input, context);
  assert.equal(actual.malformed, malformed, `${code}: malformed class`);
  assert.equal(actual.output.decision, decision, `${code}: decision`);
  assert.equal(actual.output.reasonCodes[0], code, `${code}: reason`);
  return actual.output;
}

try {
  assert.equal(context.kindIds.length, 49);
  assert.deepEqual(Object.keys(context.defaults), context.kindIds, "defaults must be the exact authority registry in exact order");
  assert.deepEqual(context.policy.authorityRef, AUTHORITY_REF);
  assert.deepEqual(context.policy.policyRef, POLICY_REF);
  assert.deepEqual(context.policy.authorizationSubjectProfileRef, AUTHORIZATION_SUBJECT_PROFILE_REF);
  assert.equal(context.policy.lifecycle.status, "accepted");
  assert.equal(context.policy.lifecycle.accepted, true);
  assert.equal(context.policy.lifecycle.currentProductRelease, "0.0.1");
  assert.equal(context.policy.lifecycle.targetProductReleaseAfterAcceptance, "0.0.2");
  assert.equal(identityDigest(context.policy), POLICY_REF.digest, "semantic policy identity must reproduce exactly");
  assert.deepEqual(context.policy.consumptionSeam.map((entry) => entry.issue), [87, 119, 121, 102]);
  assert.equal(context.policy.repositoryIdentityPolicy.declaredTokenCoherenceOnly, true);
  assert.equal(context.policy.repositoryIdentityPolicy.callerFabricatedEqualityProvesPhysicalIdentity, false);
  assert.equal(context.policy.repositoryIdentityPolicy.bindingAttestationInDecisionInput, false);
  assert.deepEqual(context.policy.repositoryIdentityPolicy.trustedBinding.ownedByIssues, [119, 89]);
  assert.equal(context.policy.repositoryIdentityPolicy.trustedBinding.missingStaleOrUnverified,
    "fail-closed-before-evaluator-invocation-or-decision-acceptance");
  assert.equal(context.policy.constraintPolicy.localVocabularyOverridesAllowed, false);
  assert.deepEqual(context.policy.constraintPolicy.reviewedBroadeningGrants, []);
  assert.equal(context.inputSchema.additionalProperties, false);
  assert.equal(context.outputSchema.additionalProperties, false);
  assert.deepEqual(context.inputSchema.$defs.authorityRef.properties.digest.const, AUTHORITY_REF.digest);
  assert.deepEqual(context.inputSchema.$defs.policyRef.properties.digest.const, POLICY_REF.digest);
  assert.deepEqual(context.policy.classificationContractRef, CLASSIFICATION_CONTRACT_REF);
  assert.deepEqual(context.policy.authorizingEvidenceContractRef, AUTHORIZING_EVIDENCE_CONTRACT_REF);
  assert.equal(context.authorizingEvidenceContract.registry.length, 9);
  assert.equal(context.authorizingEvidenceContract.trustedVerificationBoundary.callerDeclarationProvesAuthenticity, false);
  const evidenceSchemaDefs = [
    "publicFixturePermissionEvidence",
    "publicFixtureLicenseEvidence",
    "publicFixtureConsentEvidence",
    "consumerAclPermissionEvidence",
    "consumerRepositoryConsentEvidence",
    "exportTransferConsentEvidence",
    "conflictResolutionEvidence",
    "declassificationEvidence",
    "aggregationEvidence",
  ];
  const evidenceRegistryByField = new Map(context.authorizingEvidenceContract.registry
    .map((entry) => [entry.authorizingField, entry]));
  const evidenceFields = [
    "provenance.publicFixturePermissionRef",
    "provenance.publicFixtureLicenseRef",
    "provenance.publicFixtureConsentRef",
    "provenance.consumerAclPermissionRef",
    "provenance.consumerRepositoryConsentRef",
    "provenance.exportTransferConsentRef",
    "conflictResolution.decisionRef",
    "derivedArtifact.declassificationDecision.decisionRef",
    "derivedArtifact.aggregationDecision.decisionRef",
  ];
  for (const [index, field] of evidenceFields.entries()) {
    const registryEntry = evidenceRegistryByField.get(field);
    assert.ok(registryEntry, `missing evidence registry field ${field}`);
    const schemaDef = context.inputSchema.$defs[evidenceSchemaDefs[index]];
    assert.equal(schemaDef.properties.evidenceKind.const, registryEntry.evidenceKind);
    assert.equal(schemaDef.properties.purpose.const, registryEntry.purpose);
    assert.deepEqual(schemaDef.properties.outcome.enum, registryEntry.allowedOutcomes);
  }
  assert.equal(context.manifest.acceptedContracts.length, 1);
  assert.deepEqual(context.manifest.currentAcceptedRef, POLICY_REF);
  assert.equal(context.manifest.yankedCandidates.length, 5);
  assert.equal(context.manifest.yankedCandidates[0].policyRef.version, "1.0.1");
  assert.equal(context.manifest.yankedCandidates[0].accepted, false);
  assert.equal(context.manifest.yankedCandidates[1].policyRef.version, "1.0.2");
  assert.equal(context.manifest.yankedCandidates[1].accepted, false);
  assert.equal(context.manifest.yankedCandidates[2].policyRef.version, "1.0.3");
  assert.equal(context.manifest.yankedCandidates[2].accepted, false);
  assert.equal(context.manifest.yankedCandidates[3].policyRef.version, "1.0.4");
  assert.equal(context.manifest.yankedCandidates[3].accepted, false);
  assert.equal(context.manifest.yankedCandidates[4].policyRef.version, "1.0.5");
  assert.equal(context.manifest.yankedCandidates[4].accepted, false);
  assert.deepEqual(context.inputSchema.properties.operation.properties.id.enum, context.policy.vocabularies.operation);
  assert.deepEqual(context.inputSchema.properties.dataSensitivity.items.enum, context.policy.vocabularies.dataSensitivity);
  assert.deepEqual(context.inputSchema.properties.exportDisposition.enum, context.policy.vocabularies.exportDisposition);
  assert.deepEqual(context.inputSchema.$defs.repositoryRole.enum, context.policy.vocabularies.repositoryRole);
  assert.deepEqual(context.inputSchema.$defs.sourceArtifact.properties.dataSensitivity.items.enum, context.policy.vocabularies.dataSensitivity);
  assert.deepEqual(context.inputSchema.$defs.sourceArtifact.properties.exportDisposition.enum, context.policy.vocabularies.exportDisposition);
  assert.equal(context.inputSchema.$defs.sourceArtifact.additionalProperties, false);
  assert.equal(context.inputSchema.$defs.classificationRef.additionalProperties, false);
  assert.deepEqual(context.inputSchema.$defs.valueState.oneOf[1].properties.state.enum, ["unknown", "withheld", "unsupported"]);
  assert.deepEqual(context.outputSchema.properties.decision.enum, context.policy.outputContract.decisions);
  assert.deepEqual(context.outputSchema.required, ["decision", "reasonCodes", "requiredTransforms", "effectiveRefs", "derivedArtifactRequirements", "sourceTransferAllowed"]);
  assert.ok(context.outputSchema.properties.effectiveRefs.required.includes("classificationContractRef"));
  assert.ok(context.outputSchema.properties.effectiveRefs.required.includes("authorizingEvidenceContractRef"));
  assert.ok(context.outputSchema.properties.effectiveRefs.required.includes("authorizationSubjectProfileRef"));
  const privacyDocs = await readFile(join(root, "docs/privacy.md"), "utf8");
  const privacyAdr = await readFile(join(root, "docs/adr/0002-privacy-export-policy.md"), "utf8");
  for (const document of [privacyDocs, privacyAdr]) {
    assert.match(document, /caller-fabricated equality/i);
    assert.match(document, /#119/);
    assert.match(document, /#89/);
    assert.match(document, /physically resolved repository context/i);
  }

  let fixtureCount = 0;
  for (const group of ["allowed", "forbidden", "ambiguous", "malformed", "transform-required"]) {
    const fixtures = JSON.parse(await readFile(join(root, `tests/fixtures/privacy/${group}.json`), "utf8"));
    for (const fixture of fixtures) {
      const actual = evaluateDecision(fixture.decision, context);
      assert.equal(actual.output.decision, fixture.expected.decision, `${group}/${fixture.id}: decision`);
      assert.equal(actual.output.reasonCodes[0], fixture.expected.reasonCode, `${group}/${fixture.id}: reason`);
      assert.equal(actual.malformed ? 1 : actual.output.decision === "allow" ? 0 : 3, fixture.expected.exitCode, `${group}/${fixture.id}: exit class`);
      fixtureCount += 1;
    }
  }

  await expectCustodyFailure({ manifest: join(temp, "missing-manifest.json") }, "custody.required-file-missing");
  await expectCustodyFailure({ manifestSidecar: join(temp, "missing-manifest.sha256") }, "custody.required-file-missing");
  await expectCustodyFailure({ policySidecar: join(temp, "missing-policy.sha256") }, "custody.required-file-missing");

  const weakenedPolicy = clone(context.policy);
  weakenedPolicy.artifactDefaults.find((entry) => entry.artifactKind === "ai.prompt").exportDisposition = "public-fixture";
  weakenedPolicy.policyRef.digest = identityDigest(weakenedPolicy);
  const weakenedPolicyPath = join(temp, "weakened-policy.json");
  await writeFile(weakenedPolicyPath, `${JSON.stringify(weakenedPolicy, null, 2)}\n`);
  await expectCustodyFailure({ policy: weakenedPolicyPath }, "custody.policy-bytes-mismatch");

  const alternateAuthority = clone(context.authority);
  alternateAuthority.artifactKinds[0].id = "shadow.alternate-taxonomy";
  const alternateAuthorityPath = join(temp, "alternate-authority.json");
  await writeFile(alternateAuthorityPath, `${JSON.stringify(alternateAuthority, null, 2)}\n`);
  await expectCustodyFailure({ authority: alternateAuthorityPath }, "custody.authority-bytes-mismatch");

  const substitutedManifest = clone(context.manifest);
  substitutedManifest.currentAcceptedRef.digest = POLICY_REF.digest;
  substitutedManifest.note = "caller-recomputed";
  const manifestPath = join(temp, "manifest.json");
  const manifestSidecarPath = join(temp, "manifest.sha256");
  const manifestBytes = Buffer.from(`${JSON.stringify(substitutedManifest, null, 2)}\n`);
  await writeFile(manifestPath, manifestBytes);
  await writeFile(manifestSidecarPath, `${createHash("sha256").update(manifestBytes).digest("hex")}  privacy-policy.v1.0.3.manifest.json\n`);
  await expectCustodyFailure({ manifest: manifestPath, manifestSidecar: manifestSidecarPath }, "custody.manifest-untrusted");

  const weakenedBindingManifest = clone(context.manifest);
  weakenedBindingManifest.repositoryBindingSeam = { callerEqualityTrusted: true };
  const weakenedBindingManifestPath = join(temp, "manifest-binding-weakened.json");
  const weakenedBindingManifestSidecarPath = join(temp, "manifest-binding-weakened.sha256");
  const weakenedBindingManifestBytes = Buffer.from(`${JSON.stringify(weakenedBindingManifest, null, 2)}\n`);
  await writeFile(weakenedBindingManifestPath, weakenedBindingManifestBytes);
  await writeFile(weakenedBindingManifestSidecarPath,
    `${createHash("sha256").update(weakenedBindingManifestBytes).digest("hex")}  privacy-policy.v1.0.3.manifest.json\n`);
  await expectCustodyFailure({ manifest: weakenedBindingManifestPath, manifestSidecar: weakenedBindingManifestSidecarPath }, "custody.manifest-untrusted");

  const weakenedSchema = clone(context.inputSchema);
  weakenedSchema.additionalProperties = true;
  const weakenedSchemaPath = join(temp, "input-schema.json");
  await writeFile(weakenedSchemaPath, `${JSON.stringify(weakenedSchema, null, 2)}\n`);
  await expectCustodyFailure({ inputSchema: weakenedSchemaPath }, "custody.input-schema-bytes-mismatch");

  await expectPolicyMutation("kind-coverage-removed", (policy) => { policy.artifactDefaults.pop(); });
  await expectPolicyMutation("kind-coverage-duplicate", (policy) => { policy.artifactDefaults[48] = clone(policy.artifactDefaults[47]); });
  await expectPolicyMutation("disposition-uniqueness", (policy) => { policy.vocabularies.exportDisposition[5] = "public-fixture"; });
  await expectPolicyMutation("precedence-removal", (policy) => { policy.precedence.splice(6, 1); });
  await expectPolicyMutation("consent-weakened", (policy) => { policy.provenancePolicy.transferConsentRequired = false; });
  await expectPolicyMutation("derivation-weakened", (policy) => { policy.derivedArtifactPolicy.sourcePolicyRefMustMatch = false; });
  await expectPolicyMutation("role-alias-added", (policy) => { policy.vocabularies.repositoryRole.push("caller-private-repository"); });
  await expectPolicyMutation("issue-102-seam-removed", (policy) => { policy.consumptionSeam = policy.consumptionSeam.filter((entry) => entry.issue !== 102); });
  await expectPolicyMutation("accepted-lifecycle-flipped", (policy) => { policy.lifecycle.accepted = false; policy.lifecycle.status = "candidate-successor"; });
  await expectPolicyMutation("repository-binding-seam-weakened", (policy) => {
    policy.repositoryIdentityPolicy.trustedBinding.missingStaleOrUnverified = "caller-declared-equality-accepted";
  });

  const weakenedClassificationContract = clone(context.classificationContract);
  weakenedClassificationContract.rules.contractRefExact = false;
  const weakenedClassificationPath = join(temp, "classification-contract.json");
  await writeFile(weakenedClassificationPath, `${JSON.stringify(weakenedClassificationContract, null, 2)}\n`);
  await expectCustodyFailure({ classificationContract: weakenedClassificationPath }, "custody.classification-contract-bytes-mismatch");

  const weakenedEvidenceContract = clone(context.authorizingEvidenceContract);
  weakenedEvidenceContract.registry[0].expectedOutcome = "denied";
  const weakenedEvidencePath = join(temp, "authorizing-evidence-contract.json");
  await writeFile(weakenedEvidencePath, `${JSON.stringify(weakenedEvidenceContract, null, 2)}\n`);
  await expectCustodyFailure({ authorizingEvidenceContract: weakenedEvidencePath }, "custody.authorizing-evidence-bytes-mismatch");
  const missingEvidenceSidecar = join(temp, "missing-evidence.sha256");
  await expectCustodyFailure({ authorizingEvidenceSidecar: missingEvidenceSidecar }, "custody.required-file-missing");
  const mutatedEvidenceSidecar = join(temp, "mutated-evidence.sha256");
  await writeFile(mutatedEvidenceSidecar, `${"0".repeat(64)}  privacy-authorizing-evidence.v1.1.0.json\n`);
  await expectCustodyFailure({ authorizingEvidenceSidecar: mutatedEvidenceSidecar }, "custody.authorizing-evidence-sidecar-mismatch");

  const weakenedSubjectProfile = clone(context.authorizationSubjectProfile);
  weakenedSubjectProfile.projection.excludedPaths.push("artifactKind");
  const weakenedSubjectProfilePath = join(temp, "authorization-subject-profile.json");
  await writeFile(weakenedSubjectProfilePath, `${JSON.stringify(weakenedSubjectProfile, null, 2)}\n`);
  await expectCustodyFailure({ authorizationSubjectProfile: weakenedSubjectProfilePath }, "custody.subject-profile-bytes-mismatch");
  await expectCustodyFailure({ authorizationSubjectProfileSidecar: join(temp, "missing-subject-profile.sha256") }, "custody.required-file-missing");
  const mutatedSubjectProfileSidecar = join(temp, "mutated-subject-profile.sha256");
  await writeFile(mutatedSubjectProfileSidecar, `${"0".repeat(64)}  privacy-authorization-subject-profile.v1.0.0.json\n`);
  await expectCustodyFailure({ authorizationSubjectProfileSidecar: mutatedSubjectProfileSidecar }, "custody.subject-profile-sidecar-mismatch");
  const mutatedPolicySidecar = join(temp, "mutated-policy.sha256");
  await writeFile(mutatedPolicySidecar, `${"0".repeat(64)}  privacy-policy.v1.0.6.json\n`);
  await expectCustodyFailure({ policySidecar: mutatedPolicySidecar }, "custody.policy-sidecar-mismatch");

  const allowedFixtures = JSON.parse(await readFile(join(root, "tests/fixtures/privacy/allowed.json"), "utf8"));
  const publicFixture = allowedFixtures[0].decision;
  const tenantPublic = clone(publicFixture);
  tenantPublic.dataSensitivity = ["public", "tenant-scoped"];
  evaluate(tenantPublic, "deny", "sensitivity.tenant-scoped.denied");

  const aggregate = clone(publicFixture);
  aggregate.artifactKind = "aggregate.artifact";
  aggregate.exportDisposition = "public-aggregate";
  aggregate.provenance.origin = "derived";
  aggregate.provenance.derived = true;
  aggregate.provenance.synthetic = false;
  aggregate.derivedArtifact = {
    sourceArtifacts: [{
      sourceRef: sourceRef("a"),
      artifactKind: "metrics.evaluation-evidence",
      authorityRef: clone(AUTHORITY_REF),
      policyRef: clone(POLICY_REF),
      classificationRef: classification("c"),
      dataSensitivity: ["internal"],
      exportDisposition: "shareable-with-redaction",
    }],
    appliedTransforms: [{ transformId: "aggregate-no-source-rows", version: "1.0.0", evidenceDigest: hex("d") }],
    declassificationDecision: {
      policyRef: clone(POLICY_REF), decisionRef: null, version: "1.0.0", outcome: "approved", removedSensitivities: ["internal"],
    },
    aggregationDecision: {
      policyRef: clone(POLICY_REF), decisionRef: null, version: "1.0.0", outcome: "approved", removesSourceRows: true, removesSourceIdentities: true,
    },
    containsSourceRows: false,
    containsSourceIdentities: false,
    reevaluated: true,
  };
  aggregate.derivedArtifact.declassificationDecision.decisionRef = authorizingEvidence(aggregate, "declassification", "e");
  aggregate.derivedArtifact.aggregationDecision.decisionRef = authorizingEvidence(aggregate, "aggregation", "f");
  setProvenanceEvidence(aggregate, "exportTransferConsentRef", "6");
  evaluate(aggregate, "allow", "policy.allow");

  const staleSourcePolicy = clone(aggregate);
  staleSourcePolicy.derivedArtifact.sourceArtifacts[0].policyRef.version = "1.0.0";
  evaluate(staleSourcePolicy, "deny", "input.derived-artifact", true);
  const staleSourceAuthority = clone(aggregate);
  staleSourceAuthority.derivedArtifact.sourceArtifacts[0].authorityRef.version = "1.3.0";
  evaluate(staleSourceAuthority, "deny", "input.derived-artifact", true);
  const unknownSourceKind = clone(aggregate);
  unknownSourceKind.derivedArtifact.sourceArtifacts[0].artifactKind = "local.shadow-kind";
  evaluate(unknownSourceKind, "deny", "derived.source-kind-unknown");
  const staleDeclassification = clone(aggregate);
  staleDeclassification.derivedArtifact.declassificationDecision.version = "0.9.0";
  evaluate(staleDeclassification, "deny", "input.derived-artifact", true);
  const staleClassification = clone(aggregate);
  staleClassification.derivedArtifact.sourceArtifacts[0].classificationRef.version = "99.99.99";
  evaluate(staleClassification, "deny", "input.derived-artifact", true);
  const unknownClassification = clone(aggregate);
  unknownClassification.derivedArtifact.sourceArtifacts[0].classificationRef.contractId = "caller.classification";
  evaluate(unknownClassification, "deny", "input.derived-artifact", true);
  const duplicateSource = clone(aggregate);
  duplicateSource.derivedArtifact.sourceArtifacts.push(clone(duplicateSource.derivedArtifact.sourceArtifacts[0]));
  evaluate(duplicateSource, "deny", "input.derived-artifact", true);
  const conflictingSources = clone(aggregate);
  const conflicting = clone(conflictingSources.derivedArtifact.sourceArtifacts[0]);
  conflicting.classificationRef = classification("d");
  conflictingSources.derivedArtifact.sourceArtifacts.push(conflicting);
  refreshEvidenceBindings(conflictingSources);
  evaluate(conflictingSources, "deny", "derived.source-ref-conflict");

  const twoSources = clone(aggregate);
  const secondSource = clone(twoSources.derivedArtifact.sourceArtifacts[0]);
  secondSource.sourceRef = sourceRef("b");
  secondSource.classificationRef = classification("d");
  twoSources.derivedArtifact.sourceArtifacts.push(secondSource);
  refreshEvidenceBindings(twoSources);
  const permutedSources = clone(twoSources);
  permutedSources.derivedArtifact.sourceArtifacts.reverse();
  assert.deepEqual(evaluate(twoSources, "allow", "policy.allow"), evaluate(permutedSources, "allow", "policy.allow"),
    "source-set evaluation must be permutation invariant");

  const nestedUnknownLabel = clone(aggregate);
  nestedUnknownLabel.derivedArtifact.sourceArtifacts[0].dataSensitivity = ["caller-label"];
  evaluate(nestedUnknownLabel, "deny", "input.derived-artifact", true);
  const nestedUnknownDisposition = clone(aggregate);
  nestedUnknownDisposition.derivedArtifact.sourceArtifacts[0].exportDisposition = "caller-disposition";
  evaluate(nestedUnknownDisposition, "deny", "input.derived-artifact", true);

  console.log(JSON.stringify({
    ok: true,
    fixtureCount,
    authorityKindCount: context.kindIds.length,
    custodyMutations: 16 + policyMutationCount,
    namedPolicyMutations: policyMutationCount,
    headlineAdversarialProbes: 13,
    policyRef: POLICY_REF,
  }));
} finally {
  await rm(temp, { recursive: true, force: true });
}
