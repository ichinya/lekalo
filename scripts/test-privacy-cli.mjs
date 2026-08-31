#!/usr/bin/env node

import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { AUTHORITY_REF, CLASSIFICATION_CONTRACT_REF, POLICY_REF } from "./check-privacy.mjs";
import { authorizingEvidence, refreshEvidenceBindings, setProvenanceEvidence } from "./privacy-test-helpers.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const checker = fileURLToPath(new URL("check-privacy.mjs", import.meta.url));
const temp = await mkdtemp(join(tmpdir(), "lekalo-privacy-cli-"));
let subprocessCases = 0;
const clone = (value) => structuredClone(value);
const digest = (character) => `sha256:${character.repeat(64)}`;
const audit = (id, character) => ({ id, version: "1.0.0", evidenceDigest: digest(character) });
const classification = (character) => ({
  ...CLASSIFICATION_CONTRACT_REF,
  decisionId: `classification-sha256:${character.repeat(64)}`,
  evidenceDigest: digest(character),
});
const sourceRef = (character) => `source-sha256:${character.repeat(64)}`;

function invoke(args) {
  return spawnSync(process.execPath, [checker, ...args], { cwd: root, encoding: "utf8", windowsHide: true });
}

function assertDecisionRun(result, exitCode, decision, reasonCode) {
  assert.equal(result.error, undefined);
  assert.equal(result.signal, null);
  assert.equal(result.status, exitCode, result.stderr || result.stdout);
  assert.equal(result.stderr, "");
  const output = JSON.parse(result.stdout);
  assert.equal(output.decision, decision);
  assert.equal(output.reasonCodes[0], reasonCode);
  subprocessCases += 1;
}

async function invokeDecision(name, decision) {
  refreshEvidenceBindings(decision);
  const path = join(temp, `${name}.json`);
  await writeFile(path, JSON.stringify(decision));
  return invoke(["--decision", path]);
}

try {
  const defaultRun = invoke([]);
  assert.equal(defaultRun.status, 0, defaultRun.stderr);
  const defaultOutput = JSON.parse(defaultRun.stdout);
  assert.equal(defaultOutput.status, "valid");
  assert.equal(defaultOutput.policyLifecycle, "accepted");
  assert.equal(defaultOutput.accepted, true);
  assert.equal(defaultOutput.policyRef.version, "1.0.6");
  subprocessCases += 1;

  for (const group of ["allowed", "ambiguous", "malformed", "forbidden", "transform-required"]) {
    const fixtures = JSON.parse(await readFile(join(root, `tests/fixtures/privacy/${group}.json`), "utf8"));
    for (const [index, fixture] of fixtures.entries()) {
      const path = join(temp, `${group}-${index}.json`);
      await writeFile(path, JSON.stringify(fixture.decision));
      assertDecisionRun(invoke(["--decision", path]), fixture.expected.exitCode, fixture.expected.decision, fixture.expected.reasonCode);
    }
  }

  const malformedJson = join(temp, "malformed-json.json");
  await writeFile(malformedJson, "{not-json");
  const malformedJsonRun = invoke(["--decision", malformedJson]);
  assert.equal(malformedJsonRun.status, 1);
  assert.match(JSON.parse(malformedJsonRun.stderr).reasonCodes[0], /^custody\.decision\.json:/);
  subprocessCases += 1;

  const missingManifest = invoke(["--manifest", join(temp, "missing.json")]);
  assert.equal(missingManifest.status, 1);
  assert.match(JSON.parse(missingManifest.stderr).reasonCodes[0], /^custody\.required-file-missing:/);
  subprocessCases += 1;

  const unknownArgument = invoke(["--caller-policy-digest", "sha256:deadbeef"]);
  assert.equal(unknownArgument.status, 1);
  assert.match(JSON.parse(unknownArgument.stderr).reasonCodes[0], /^usage\.unknown-or-missing-option:/);
  subprocessCases += 1;

  const base = JSON.parse(await readFile(join(root, "tests/fixtures/privacy/allowed.json"), "utf8"))[0].decision;
  const repoOne = `repo-sha256:${"1".repeat(64)}`;
  const contradictory = clone(base);
  contradictory.artifactKind = "consumer.model";
  contradictory.exportDisposition = "consumer-repository-only";
  contradictory.operation.id = "repository-store";
  contradictory.source = { repositoryRole: "consumer-repository", repositoryRef: repoOne };
  contradictory.destination = {
    repositoryRole: "lekalo-repository", repositoryRef: repoOne, trustBoundary: "same-repository",
    repositoryRelation: "same-origin", tenantRelation: "same-tenant",
  };
  contradictory.audience = "repository-collaborators";
  contradictory.provenance.origin = "consumer-repository";
  contradictory.provenance.repositoryRole = "consumer-repository";
  contradictory.provenance.repositoryRef = repoOne;
  contradictory.provenance.synthetic = false;
  assertDecisionRun(await invokeDecision("contradictory-consumer-lekalo", contradictory), 3, "deny", "repository.same-origin-contradiction");

  for (const [name, value] of [
    ["com-superscript-1", "paths/COM¹.txt"], ["lpt-superscript-2", "paths/LPT²"],
    ["conin", "paths/CONIN$"], ["conout", "paths/CONOUT$"], ["clock", "paths/CLOCK$"],
  ]) {
    const decision = clone(base);
    decision.resourcePath.value = value;
    assertDecisionRun(await invokeDecision(name, decision), 3, "deny", "path.not-normalized-project-relative");
  }

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
  assertDecisionRun(await invokeDecision("aggregate-current-classification", aggregate), 0, "allow", "policy.allow");

  const derivedAndSynthetic = clone(aggregate);
  derivedAndSynthetic.provenance.synthetic = true;
  assertDecisionRun(await invokeDecision("aggregate-derived-and-synthetic", derivedAndSynthetic), 3, "deny", "provenance.origin-boolean-conflict");

  const staleClassification = clone(aggregate);
  staleClassification.derivedArtifact.sourceArtifacts[0].classificationRef.version = "99.99.99";
  assertDecisionRun(await invokeDecision("aggregate-stale-classification", staleClassification), 1, "deny", "input.derived-artifact");
  const arbitraryClassification = clone(aggregate);
  arbitraryClassification.derivedArtifact.sourceArtifacts[0].classificationRef.contractId = "caller.classification";
  assertDecisionRun(await invokeDecision("aggregate-arbitrary-classification", arbitraryClassification), 1, "deny", "input.derived-artifact");
  const duplicateSource = clone(aggregate);
  duplicateSource.derivedArtifact.sourceArtifacts.push(clone(duplicateSource.derivedArtifact.sourceArtifacts[0]));
  assertDecisionRun(await invokeDecision("aggregate-duplicate-source", duplicateSource), 1, "deny", "input.derived-artifact");
  const conflictingSource = clone(aggregate);
  const conflict = clone(conflictingSource.derivedArtifact.sourceArtifacts[0]);
  conflict.classificationRef = classification("d");
  conflictingSource.derivedArtifact.sourceArtifacts.push(conflict);
  refreshEvidenceBindings(conflictingSource);
  const conflictFirst = await invokeDecision("aggregate-conflicting-source", conflictingSource);
  assertDecisionRun(conflictFirst, 3, "deny", "derived.source-ref-conflict");
  const conflictingReverse = clone(conflictingSource);
  conflictingReverse.derivedArtifact.sourceArtifacts.reverse();
  const conflictSecond = await invokeDecision("aggregate-conflicting-source-reverse", conflictingReverse);
  assertDecisionRun(conflictSecond, 3, "deny", "derived.source-ref-conflict");
  assert.deepEqual(JSON.parse(conflictFirst.stdout), JSON.parse(conflictSecond.stdout));
  const twoSources = clone(aggregate);
  const second = clone(twoSources.derivedArtifact.sourceArtifacts[0]);
  second.sourceRef = sourceRef("b");
  second.classificationRef = classification("d");
  twoSources.derivedArtifact.sourceArtifacts.push(second);
  refreshEvidenceBindings(twoSources);
  const firstOrder = await invokeDecision("aggregate-two-sources-a", twoSources);
  const reverse = clone(twoSources);
  reverse.derivedArtifact.sourceArtifacts.reverse();
  const secondOrder = await invokeDecision("aggregate-two-sources-b", reverse);
  assertDecisionRun(firstOrder, 0, "allow", "policy.allow");
  assertDecisionRun(secondOrder, 0, "allow", "policy.allow");
  assert.deepEqual(JSON.parse(firstOrder.stdout), JSON.parse(secondOrder.stdout));
  const mixedStale = clone(twoSources);
  mixedStale.derivedArtifact.sourceArtifacts[1].classificationRef.version = "99.99.99";
  assertDecisionRun(await invokeDecision("aggregate-mixed-stale", mixedStale), 1, "deny", "input.derived-artifact");
  mixedStale.derivedArtifact.sourceArtifacts.reverse();
  assertDecisionRun(await invokeDecision("aggregate-mixed-stale-reverse", mixedStale), 1, "deny", "input.derived-artifact");

  const nestedUnknownLabel = clone(aggregate);
  nestedUnknownLabel.derivedArtifact.sourceArtifacts[0].dataSensitivity = ["caller-label"];
  assertDecisionRun(await invokeDecision("nested-unknown-label", nestedUnknownLabel), 1, "deny", "input.derived-artifact");
  const nestedUnknownDisposition = clone(aggregate);
  nestedUnknownDisposition.derivedArtifact.sourceArtifacts[0].exportDisposition = "caller-disposition";
  assertDecisionRun(await invokeDecision("nested-unknown-disposition", nestedUnknownDisposition), 1, "deny", "input.derived-artifact");

  const publicFixtureStore = clone(base);
  publicFixtureStore.artifactKind = "fixture";
  publicFixtureStore.exportDisposition = "public-fixture";
  publicFixtureStore.operation.id = "repository-store";
  publicFixtureStore.source = { repositoryRole: "consumer-repository", repositoryRef: repoOne };
  publicFixtureStore.destination = {
    repositoryRole: "consumer-repository", repositoryRef: repoOne, trustBoundary: "same-repository",
    repositoryRelation: "same-origin", tenantRelation: "same-tenant",
  };
  publicFixtureStore.audience = "repository-collaborators";
  publicFixtureStore.provenance.origin = "consumer-repository";
  publicFixtureStore.provenance.repositoryRole = "consumer-repository";
  publicFixtureStore.provenance.repositoryRef = repoOne;
  publicFixtureStore.provenance.synthetic = false;
  publicFixtureStore.provenance.derived = false;
  for (const [name, permission, license] of [
    ["consent-only", false, false],
    ["permission-and-consent", true, false],
    ["license-and-consent", false, true],
  ]) {
    const decision = clone(publicFixtureStore);
    if (permission) setProvenanceEvidence(decision, "publicFixturePermissionRef", "7");
    if (license) setProvenanceEvidence(decision, "publicFixtureLicenseRef", "8");
    setProvenanceEvidence(decision, "publicFixtureConsentRef", "9");
    assertDecisionRun(await invokeDecision(`fixture-store-${name}`, decision), 3, "deny", "fixture.permission-license-consent-required");
  }

  for (const operation of ["local-use", "derive"]) {
    const wrongConsumerOrigin = clone(base);
    wrongConsumerOrigin.artifactKind = "consumer.model";
    wrongConsumerOrigin.exportDisposition = "consumer-repository-only";
    wrongConsumerOrigin.operation.id = operation;
    wrongConsumerOrigin.source = { repositoryRole: "lekalo-repository", repositoryRef: repoOne };
    wrongConsumerOrigin.destination = {
      repositoryRole: "local-workspace", repositoryRef: null, trustBoundary: "same-local-workspace",
      repositoryRelation: "not-applicable", tenantRelation: "same-tenant",
    };
    wrongConsumerOrigin.audience = "operator-only";
    wrongConsumerOrigin.provenance.origin = "lekalo-repository";
    wrongConsumerOrigin.provenance.repositoryRole = "lekalo-repository";
    wrongConsumerOrigin.provenance.repositoryRef = repoOne;
    wrongConsumerOrigin.provenance.synthetic = false;
    wrongConsumerOrigin.provenance.derived = false;
    assertDecisionRun(await invokeDecision(`consumer-wrong-origin-${operation}`, wrongConsumerOrigin), 3, "deny", "disposition.consumer-origin-acl-only");
  }

  for (const [name, seed, mutate, reasonCode] of [
    ["nested-authority-extra", aggregate, (decision) => { decision.derivedArtifact.sourceArtifacts[0].authorityRef.extra = true; }, "input.derived-artifact"],
    ["nested-policy-extra", aggregate, (decision) => { decision.derivedArtifact.sourceArtifacts[0].policyRef.extra = true; }, "input.derived-artifact"],
    ["declassification-number", aggregate, (decision) => { decision.derivedArtifact.declassificationDecision.removedSensitivities = [1]; }, "input.derived-artifact"],
    ["declassification-ref-extra", aggregate, (decision) => { decision.derivedArtifact.declassificationDecision.decisionRef.extra = true; }, "input.derived-artifact"],
    ["duplicate-main-sensitivity", base, (decision) => { decision.dataSensitivity.push(decision.dataSensitivity[0]); }, "input.data-sensitivity"],
    ["duplicate-transforms", aggregate, (decision) => { decision.derivedArtifact.appliedTransforms.push(clone(decision.derivedArtifact.appliedTransforms[0])); }, "input.derived-artifact"],
    ["duplicate-removed", aggregate, (decision) => { decision.derivedArtifact.declassificationDecision.removedSensitivities.push("internal"); }, "input.derived-artifact"],
  ]) {
    const decision = clone(seed);
    mutate(decision);
    assertDecisionRun(await invokeDecision(`shape-${name}`, decision), 1, "deny", reasonCode);
  }

  const conflictAllowMissing = clone(base);
  conflictAllowMissing.conflictResolution = { state: "resolved-allow", decisionRef: null };
  assertDecisionRun(await invokeDecision("conflict-allow-missing", conflictAllowMissing), 3, "deny", "conflict.missing-decision");

  const grantPolicyExtra = clone(base);
  grantPolicyExtra.broadeningGrant = {
    grantId: "grant.reviewed", version: "1.0.0", policyRef: clone(POLICY_REF), reviewRef: audit("review.grant", "a"),
  };
  grantPolicyExtra.broadeningGrant.policyRef.extra = true;
  assertDecisionRun(await invokeDecision("shape-grant-policy-extra", grantPolicyExtra), 1, "deny", "input.broadening-grant");

  const duplicateConstraints = clone(base);
  const constraint = {
    scope: "operation", constraintRef: audit("constraint.operation", "b"), allowedOperations: ["publish"],
    allowedTrustBoundaries: ["public"], allowedAudiences: ["public"],
  };
  duplicateConstraints.constraints = [constraint, clone(constraint)];
  assertDecisionRun(await invokeDecision("shape-duplicate-constraints", duplicateConstraints), 1, "deny", "input.constraints");

  const duplicateConstraintMember = clone(base);
  duplicateConstraintMember.constraints = [clone(constraint)];
  duplicateConstraintMember.constraints[0].allowedOperations.push("publish");
  assertDecisionRun(await invokeDecision("shape-duplicate-constraint-operation", duplicateConstraintMember), 1, "deny", "input.constraints");

  const rejectedAccepted102 = invoke([
    "--manifest", join(root, "contracts/privacy-policy.v1.0.2.manifest.json"),
    "--manifest-sidecar", join(root, "contracts/privacy-policy.v1.0.2.manifest.sha256"),
    "--policy", join(root, "contracts/privacy-policy.v1.0.2.json"),
    "--policy-sidecar", join(root, "contracts/privacy-policy.v1.0.2.sha256"),
  ]);
  assert.equal(rejectedAccepted102.status, 1);
  assert.equal(JSON.parse(rejectedAccepted102.stderr).reasonCodes[0], "custody.manifest-untrusted");
  subprocessCases += 1;

  const rejectedCandidate = invoke([
    "--manifest", join(root, "contracts/privacy-policy.v1.manifest.json"),
    "--manifest-sidecar", join(root, "contracts/privacy-policy.v1.manifest.sha256"),
    "--policy", join(root, "contracts/privacy-policy.v1.json"),
    "--policy-sidecar", join(root, "contracts/privacy-policy.v1.sha256"),
  ]);
  assert.equal(rejectedCandidate.status, 1);
  assert.equal(JSON.parse(rejectedCandidate.stderr).reasonCodes[0], "custody.manifest-untrusted");
  subprocessCases += 1;

  console.log(JSON.stringify({ ok: true, subprocessCases, exitCodes: [0, 1, 3], realProcessBoundary: true }));
} finally {
  await rm(temp, { recursive: true, force: true });
}
