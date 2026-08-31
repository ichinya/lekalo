#!/usr/bin/env node

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  AUTHORITY_REF,
  AUTHORIZATION_SUBJECT_PROFILE_REF,
  CLASSIFICATION_CONTRACT_REF,
  POLICY_REF,
  authorizationSubjectDigest,
  evaluateDecision,
  loadTrustedContext,
  validateDecisionInput,
} from "./check-privacy.mjs";
import {
  AUTHORIZATION_SUBJECT_PROFILE,
  authorizingEvidence,
  refreshEvidenceBindings,
  setProvenanceEvidence,
} from "./privacy-test-helpers.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const checker = fileURLToPath(new URL("check-privacy.mjs", import.meta.url));
const context = await loadTrustedContext();
const temp = await mkdtemp(join(tmpdir(), "lekalo-privacy-subject-"));
const clone = (value) => structuredClone(value);
const digest = (character) => `sha256:${character.repeat(64)}`;
const repo = (character) => `repo-sha256:${character.repeat(64)}`;
const sourceRef = (character) => `source-sha256:${character.repeat(64)}`;
const audit = (id, character) => ({ id, version: "1.0.0", evidenceDigest: digest(character) });
const classification = (character) => ({
  ...CLASSIFICATION_CONTRACT_REF,
  decisionId: `classification-sha256:${character.repeat(64)}`,
  evidenceDigest: digest(character),
});
const base = JSON.parse(await readFile(join(root, "tests/fixtures/privacy/allowed.json"), "utf8"))[0].decision;

function reverseObjectKeys(value) {
  if (Array.isArray(value)) return value.map(reverseObjectKeys);
  if (value !== null && typeof value === "object") {
    return Object.fromEntries(Object.entries(value).reverse().map(([key, child]) => [key, reverseObjectKeys(child)]));
  }
  return value;
}

function maximalAggregate() {
  const input = clone(base);
  input.artifactKind = "aggregate.artifact";
  input.artifactRef = `artifact-sha256:${"a".repeat(64)}`;
  input.operation.id = "publish";
  input.source = { repositoryRole: "consumer-repository", repositoryRef: repo("1") };
  input.destination = {
    repositoryRole: "public-channel", repositoryRef: null, trustBoundary: "public",
    repositoryRelation: "not-applicable", tenantRelation: "not-applicable",
  };
  input.audience = "public";
  input.dataSensitivity = ["public"];
  input.exportDisposition = "public-aggregate";
  input.provenance.origin = "derived";
  input.provenance.repositoryRole = "consumer-repository";
  input.provenance.repositoryRef = repo("1");
  input.provenance.synthetic = false;
  input.provenance.derived = true;
  input.provenance.classificationRef = classification("1");
  input.resourcePath = { state: "known", value: "reports/aggregate.json" };
  input.valueState = { state: "known", value: 0 };
  input.conflictResolution = { state: "resolved-allow", decisionRef: null };
  input.constraints = [
    { scope: "operation", constraintRef: audit("constraint.operation", "1"), allowedOperations: ["publish"], allowedTrustBoundaries: ["public"], allowedAudiences: ["public"] },
    { scope: "profile", constraintRef: audit("constraint.profile", "2"), allowedOperations: ["publish"], allowedTrustBoundaries: ["public"], allowedAudiences: ["public"] },
  ];
  input.derivedArtifact = {
    sourceArtifacts: [
      {
        sourceRef: sourceRef("1"), artifactKind: "metrics.evaluation-evidence", authorityRef: clone(AUTHORITY_REF),
        policyRef: clone(POLICY_REF), classificationRef: classification("2"), dataSensitivity: ["internal", "retention-limited"],
        exportDisposition: "shareable-with-redaction",
      },
      {
        sourceRef: sourceRef("2"), artifactKind: "metrics.evaluation-evidence", authorityRef: clone(AUTHORITY_REF),
        policyRef: clone(POLICY_REF), classificationRef: classification("3"), dataSensitivity: ["retention-limited", "internal"],
        exportDisposition: "shareable-with-redaction",
      },
    ],
    appliedTransforms: [
      { transformId: "aggregate-no-source-rows", version: "1.0.0", evidenceDigest: digest("4") },
      { transformId: "replace-repository-identity", version: "1.0.0", evidenceDigest: digest("5") },
    ],
    declassificationDecision: {
      policyRef: clone(POLICY_REF), decisionRef: null, version: "1.0.0", outcome: "approved",
      removedSensitivities: ["retention-limited", "internal"],
    },
    aggregationDecision: {
      policyRef: clone(POLICY_REF), decisionRef: null, version: "1.0.0", outcome: "approved",
      removesSourceRows: true, removesSourceIdentities: true,
    },
    containsSourceRows: false,
    containsSourceIdentities: false,
    reevaluated: true,
  };
  input.conflictResolution.decisionRef = authorizingEvidence(input, "conflictResolution", "6");
  input.derivedArtifact.declassificationDecision.decisionRef = authorizingEvidence(input, "declassification", "7");
  input.derivedArtifact.aggregationDecision.decisionRef = authorizingEvidence(input, "aggregation", "8");
  setProvenanceEvidence(input, "exportTransferConsentRef", "9");
  return refreshEvidenceBindings(input);
}

function publicFixture() {
  const input = clone(base);
  input.artifactKind = "fixture";
  input.artifactRef = `artifact-sha256:${"b".repeat(64)}`;
  input.operation.id = "publish";
  input.source = { repositoryRole: "consumer-repository", repositoryRef: repo("3") };
  input.destination = { repositoryRole: "public-channel", repositoryRef: null, trustBoundary: "public", repositoryRelation: "not-applicable", tenantRelation: "not-applicable" };
  input.audience = "public";
  input.dataSensitivity = ["public"];
  input.exportDisposition = "public-fixture";
  input.provenance.origin = "consumer-repository";
  input.provenance.repositoryRole = "consumer-repository";
  input.provenance.repositoryRef = repo("3");
  input.provenance.synthetic = false;
  input.provenance.derived = false;
  input.provenance.classificationRef = classification("4");
  input.resourcePath = { state: "known", value: "fixtures/example.json" };
  input.valueState = { state: "known", value: 0 };
  setProvenanceEvidence(input, "publicFixturePermissionRef", "a");
  setProvenanceEvidence(input, "publicFixtureLicenseRef", "b");
  setProvenanceEvidence(input, "publicFixtureConsentRef", "c");
  return refreshEvidenceBindings(input);
}

function consumerStore() {
  const input = clone(base);
  input.artifactKind = "consumer.model";
  input.artifactRef = `artifact-sha256:${"c".repeat(64)}`;
  input.operation.id = "repository-store";
  input.source = { repositoryRole: "consumer-repository", repositoryRef: repo("4") };
  input.destination = { repositoryRole: "consumer-repository", repositoryRef: repo("4"), trustBoundary: "same-repository", repositoryRelation: "same-origin", tenantRelation: "same-tenant" };
  input.audience = "repository-collaborators";
  input.dataSensitivity = ["public"];
  input.exportDisposition = "consumer-repository-only";
  input.provenance.origin = "consumer-repository";
  input.provenance.repositoryRole = "consumer-repository";
  input.provenance.repositoryRef = repo("4");
  input.provenance.synthetic = false;
  input.provenance.derived = false;
  setProvenanceEvidence(input, "consumerAclPermissionRef", "d");
  setProvenanceEvidence(input, "consumerRepositoryConsentRef", "e");
  return refreshEvidenceBindings(input);
}

function resolveInventoryTarget(input, path) {
  const segments = path.split(".").map((part) => ({ key: part.endsWith("[]") ? part.slice(0, -2) : part, each: part.endsWith("[]") }));
  let current = input;
  for (let index = 0; index < segments.length - 1; index += 1) {
    const segment = segments[index];
    current = current[segment.key];
    if (segment.each) current = current[0];
  }
  const last = segments.at(-1);
  return { parent: current, key: last.key, each: last.each };
}

function mutateInventoryPath(input, path) {
  const { parent, key, each } = resolveInventoryTarget(input, path);
  const target = each ? parent[key][0] : parent[key];
  if (typeof target === "boolean") {
    if (each) parent[key][0] = !target; else parent[key] = !target;
  } else if (typeof target === "number") {
    if (each) parent[key][0] = target + 1; else parent[key] = target + 1;
  } else if (typeof target === "string") {
    let replacement;
    if (/^(?:sha256|subject-sha256|repo-sha256|source-sha256|artifact-sha256|classification-sha256):[0-9a-f]{64}$/.test(target)) {
      replacement = `${target.slice(0, -1)}${target.endsWith("0") ? "1" : "0"}`;
    } else if (target === "internal") replacement = "confidential";
    else if (target === "retention-limited") replacement = "confidential";
    else if (target === "public") replacement = "internal";
    else if (target === "known") replacement = "unknown";
    else replacement = `${target}-mutation`;
    if (each) parent[key][0] = replacement; else parent[key] = replacement;
  } else if (target === null) {
    if (each) parent[key][0] = {}; else parent[key] = {};
  } else if (target !== undefined && typeof target === "object") {
    target.unexpectedMutationField = true;
  } else {
    throw new Error(`inventory path not represented: ${path}`);
  }
}

async function subprocess(name, input, expectedCode, expectedReason) {
  const path = join(temp, `${name}.json`);
  await writeFile(path, JSON.stringify(input));
  const run = spawnSync(process.execPath, [checker, "--decision", path], { cwd: root, encoding: "utf8", windowsHide: true });
  assert.equal(run.status, expectedCode, run.stderr || run.stdout);
  const output = JSON.parse(run.stdout);
  assert.equal(output.reasonCodes[0], expectedReason);
  return run;
}

try {
  assert.deepEqual(AUTHORIZATION_SUBJECT_PROFILE_REF, context.policy.authorizationSubjectProfileRef);
  assert.deepEqual(AUTHORIZATION_SUBJECT_PROFILE, context.authorizationSubjectProfile);
  const baseline = maximalAggregate();
  assert.equal(validateDecisionInput(baseline, context), null);
  const baselineResult = evaluateDecision(baseline, context);
  assert.equal(baselineResult.output.decision, "allow", JSON.stringify(baselineResult.output));

  const declaredInventory = context.authorizationSubjectProfile.decisionSemanticInventory;
  let malformedMutations = 0;
  let bindingDenials = 0;
  for (const entry of declaredInventory) {
    const input = maximalAggregate();
    const before = authorizationSubjectDigest(input, context.authorizationSubjectProfile);
    mutateInventoryPath(input, entry.path);
    const after = authorizationSubjectDigest(input, context.authorizationSubjectProfile);
    assert.notEqual(after, before, `${entry.id}: semantic mutation must change subject digest`);
    const result = evaluateDecision(input, context);
    if (result.malformed) malformedMutations += 1;
    else {
      assert.equal(result.output.reasonCodes[0], "evidence.binding-mismatch", `${entry.id}: well-formed stale binding`);
      bindingDenials += 1;
    }
  }
  assert.equal(malformedMutations + bindingDenials, declaredInventory.length);

  const typed = publicFixture();
  for (const [name, mutate] of [
    ["typed-classification-decision", (input) => { input.provenance.classificationRef.decisionId = `classification-sha256:${"f".repeat(64)}`; }],
    ["typed-classification-evidence", (input) => { input.provenance.classificationRef.evidenceDigest = digest("f"); }],
    ["typed-project-relative-path", (input) => { input.resourcePath.value = "fixtures/changed.json"; }],
    ["typed-value-state", (input) => { input.valueState = { state: "unknown" }; }],
    ["typed-value-zero-to-one", (input) => { input.valueState.value = 1; }],
  ]) {
    const input = clone(typed);
    mutate(input);
    await subprocess(name, input, 3, "evidence.binding-mismatch");
  }

  for (const [name, mutate] of [
    ["aggregate-classification-decision", (input) => { input.derivedArtifact.sourceArtifacts[0].classificationRef.decisionId = `classification-sha256:${"f".repeat(64)}`; }],
    ["aggregate-classification-evidence", (input) => { input.derivedArtifact.sourceArtifacts[0].classificationRef.evidenceDigest = digest("f"); }],
    ["aggregate-internal-to-confidential", (input) => { input.derivedArtifact.sourceArtifacts[0].dataSensitivity[0] = "confidential"; }],
    ["aggregate-removed-sensitivity", (input) => { input.derivedArtifact.declassificationDecision.removedSensitivities[0] = "confidential"; }],
    ["aggregate-transform-evidence", (input) => { input.derivedArtifact.appliedTransforms[0].evidenceDigest = digest("f"); }],
    ["aggregate-declassification-outcome", (input) => { input.derivedArtifact.declassificationDecision.outcome = "rejected"; }],
    ["aggregate-aggregation-outcome", (input) => { input.derivedArtifact.aggregationDecision.outcome = "rejected"; }],
  ]) {
    const input = maximalAggregate();
    mutate(input);
    await subprocess(name, input, 3, "evidence.binding-mismatch");
  }
  const staleClassificationVersion = maximalAggregate();
  staleClassificationVersion.derivedArtifact.sourceArtifacts[0].classificationRef.version = "99.99.99";
  await subprocess("aggregate-classification-version", staleClassificationVersion, 1, "input.derived-artifact");

  for (const [name, mutate] of [
    ["source-set-add", (input) => {
      const added = clone(input.derivedArtifact.sourceArtifacts[0]);
      added.sourceRef = sourceRef("f");
      added.classificationRef = classification("f");
      input.derivedArtifact.sourceArtifacts.push(added);
    }],
    ["source-set-remove", (input) => { input.derivedArtifact.sourceArtifacts.pop(); }],
    ["source-set-change", (input) => { input.derivedArtifact.sourceArtifacts[0].sourceRef = sourceRef("f"); }],
  ]) {
    const input = maximalAggregate();
    mutate(input);
    await subprocess(name, input, 3, "evidence.binding-mismatch");
  }

  const equivalent = maximalAggregate();
  const baselineDigest = authorizationSubjectDigest(equivalent, context.authorizationSubjectProfile);
  const permutations = [];
  const keysReversed = reverseObjectKeys(equivalent);
  permutations.push(["object-key-order", keysReversed]);
  for (const [name, mutate] of [
    ["source-order", (input) => { input.derivedArtifact.sourceArtifacts.reverse(); }],
    ["source-label-order", (input) => { input.derivedArtifact.sourceArtifacts[0].dataSensitivity.reverse(); }],
    ["removed-label-order", (input) => { input.derivedArtifact.declassificationDecision.removedSensitivities.reverse(); }],
    ["transform-order", (input) => { input.derivedArtifact.appliedTransforms.reverse(); }],
    ["constraint-order", (input) => { input.constraints.reverse(); }],
  ]) {
    const input = clone(equivalent);
    mutate(input);
    permutations.push([name, input]);
  }
  for (const [name, input] of permutations) {
    assert.equal(authorizationSubjectDigest(input, context.authorizationSubjectProfile), baselineDigest, `${name}: canonical equivalence`);
    assert.equal(evaluateDecision(input, context).output.reasonCodes[0], "policy.allow", `${name}: existing evidence remains valid`);
  }
  const traceOnly = clone(equivalent);
  traceOnly.constraints[0].constraintRef.evidenceDigest = digest("f");
  assert.equal(authorizationSubjectDigest(traceOnly, context.authorizationSubjectProfile), baselineDigest, "non-authorizing trace ref excluded");
  assert.equal(evaluateDecision(traceOnly, context).output.reasonCodes[0], "policy.allow");

  const duplicateLabels = maximalAggregate();
  duplicateLabels.dataSensitivity.push(duplicateLabels.dataSensitivity[0]);
  assert.equal(evaluateDecision(duplicateLabels, context).malformed, true);
  const duplicateSources = maximalAggregate();
  duplicateSources.derivedArtifact.sourceArtifacts.push(clone(duplicateSources.derivedArtifact.sourceArtifacts[0]));
  assert.equal(evaluateDecision(duplicateSources, context).malformed, true);

  const purposeCases = [
    ["public-fixture-permission", publicFixture(), (input) => input.provenance.publicFixturePermissionRef],
    ["public-fixture-license", publicFixture(), (input) => input.provenance.publicFixtureLicenseRef],
    ["public-fixture-consent", publicFixture(), (input) => input.provenance.publicFixtureConsentRef],
    ["consumer-acl", consumerStore(), (input) => input.provenance.consumerAclPermissionRef],
    ["consumer-consent", consumerStore(), (input) => input.provenance.consumerRepositoryConsentRef],
    ["export-consent", maximalAggregate(), (input) => input.provenance.exportTransferConsentRef],
    ["conflict", maximalAggregate(), (input) => input.conflictResolution.decisionRef],
    ["declassification", maximalAggregate(), (input) => input.derivedArtifact.declassificationDecision.decisionRef],
    ["aggregation", maximalAggregate(), (input) => input.derivedArtifact.aggregationDecision.decisionRef],
  ];
  for (const [name, input, locate] of purposeCases) {
    locate(input).binding.subjectDigest = `subject-sha256:${"f".repeat(64)}`;
    await subprocess(`purpose-${name}-stale-subject`, input, 3, "evidence.binding-mismatch");
  }

  const oldManifest = spawnSync(process.execPath, [checker,
    "--manifest", join(root, "contracts/privacy-policy.v1.0.4.manifest.json"),
    "--manifest-sidecar", join(root, "contracts/privacy-policy.v1.0.4.manifest.sha256"),
    "--policy", join(root, "contracts/privacy-policy.v1.0.4.json"),
    "--policy-sidecar", join(root, "contracts/privacy-policy.v1.0.4.sha256"),
  ], { cwd: root, encoding: "utf8", windowsHide: true });
  assert.equal(oldManifest.status, 1);
  assert.equal(JSON.parse(oldManifest.stderr).reasonCodes[0], "custody.manifest-untrusted");

  console.log(JSON.stringify({
    ok: true,
    semanticInventoryCases: declaredInventory.length,
    malformedMutations,
    bindingDenials,
    exactReviewerSubprocessCases: 16,
    authorizingPurposeSubprocessCases: purposeCases.length,
    nonSemanticEquivalenceCases: permutations.length + 1,
    profileRef: AUTHORIZATION_SUBJECT_PROFILE_REF,
  }));
} finally {
  await rm(temp, { recursive: true, force: true });
}
