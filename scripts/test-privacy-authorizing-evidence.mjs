#!/usr/bin/env node

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  AUTHORITY_REF,
  CLASSIFICATION_CONTRACT_REF,
  POLICY_REF,
} from "./check-privacy.mjs";
import {
  authorizingEvidence,
  refreshEvidenceBindings,
  setProvenanceEvidence,
} from "./privacy-test-helpers.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const checker = fileURLToPath(new URL("check-privacy.mjs", import.meta.url));
const temp = await mkdtemp(join(tmpdir(), "lekalo-privacy-authorizing-evidence-"));
const clone = (value) => structuredClone(value);
const sha = (character) => `sha256:${character.repeat(64)}`;
const repo = (character) => `repo-sha256:${character.repeat(64)}`;
const sourceRef = (character) => `source-sha256:${character.repeat(64)}`;
const classification = (character) => ({
  ...CLASSIFICATION_CONTRACT_REF,
  decisionId: `classification-sha256:${character.repeat(64)}`,
  evidenceDigest: sha(character),
});
const genericAudit = Object.freeze({
  id: "caller.generic",
  version: "99.99.99",
  evidenceDigest: sha("9"),
});
const REPO_ONE = repo("1");
const REPO_TWO = repo("2");

let subprocessCases = 0;

function invoke(args) {
  return spawnSync(process.execPath, [checker, ...args], {
    cwd: root,
    encoding: "utf8",
    windowsHide: true,
  });
}

async function runDecision(name, input) {
  const path = join(temp, `${name}.json`);
  await writeFile(path, `${JSON.stringify(input, null, 2)}\n`);
  return invoke(["--decision", path]);
}

function assertProtocol(result, exitCode, decision, reason = null) {
  assert.equal(result.error, undefined);
  assert.equal(result.signal, null);
  assert.equal(result.status, exitCode, result.stderr || result.stdout);
  const payload = JSON.parse(result.stdout || result.stderr);
  assert.equal(payload.decision, decision);
  if (reason !== null) assert.equal(payload.reasonCodes[0], reason);
  if (decision !== "allow") assert.equal(payload.sourceTransferAllowed, false);
  subprocessCases += 1;
  return payload;
}

function setDestination(input, operation) {
  input.operation.id = operation;
  if (["local-use", "derive"].includes(operation)) {
    input.destination = {
      repositoryRole: "local-workspace",
      repositoryRef: null,
      trustBoundary: "same-local-workspace",
      repositoryRelation: "not-applicable",
      tenantRelation: "same-tenant",
    };
    input.audience = "operator-only";
  } else if (operation === "repository-store") {
    input.destination = {
      repositoryRole: input.source.repositoryRole,
      repositoryRef: input.source.repositoryRef,
      trustBoundary: "same-repository",
      repositoryRelation: "same-origin",
      tenantRelation: "same-tenant",
    };
    input.audience = "repository-collaborators";
  } else if (operation === "transfer") {
    input.destination = {
      repositoryRole: "external-repository",
      repositoryRef: REPO_TWO,
      trustBoundary: "same-tenant",
      repositoryRelation: "different-repository",
      tenantRelation: "same-tenant",
    };
    input.audience = "tenant-members";
  } else {
    input.destination = {
      repositoryRole: "public-channel",
      repositoryRef: null,
      trustBoundary: "public",
      repositoryRelation: "not-applicable",
      tenantRelation: "not-applicable",
    };
    input.audience = "public";
  }
}

function publicFixture(seed, operation, synthetic) {
  const input = clone(seed);
  input.artifactKind = "fixture";
  input.exportDisposition = "public-fixture";
  input.dataSensitivity = ["public"];
  input.source = {
    repositoryRole: synthetic ? "local-workspace" : "consumer-repository",
    repositoryRef: synthetic ? null : REPO_ONE,
  };
  input.provenance.origin = synthetic ? "synthetic" : "consumer-repository";
  input.provenance.repositoryRole = input.source.repositoryRole;
  input.provenance.repositoryRef = input.source.repositoryRef;
  input.provenance.synthetic = synthetic;
  input.provenance.derived = false;
  setDestination(input, operation);
  return input;
}

function setFixtureEvidence(input) {
  setProvenanceEvidence(input, "publicFixturePermissionRef", "1");
  setProvenanceEvidence(input, "publicFixtureLicenseRef", "2");
  setProvenanceEvidence(input, "publicFixtureConsentRef", "3");
  return input;
}

function aggregate(seed) {
  const input = clone(seed);
  input.artifactKind = "aggregate.artifact";
  input.exportDisposition = "public-aggregate";
  input.dataSensitivity = ["public"];
  input.provenance.origin = "derived";
  input.provenance.synthetic = false;
  input.provenance.derived = true;
  input.derivedArtifact = {
    sourceArtifacts: [{
      sourceRef: sourceRef("a"),
      artifactKind: "metrics.evaluation-evidence",
      authorityRef: clone(AUTHORITY_REF),
      policyRef: clone(POLICY_REF),
      classificationRef: classification("c"),
      dataSensitivity: ["internal"],
      exportDisposition: "shareable-with-redaction",
    }],
    appliedTransforms: [{ transformId: "aggregate-no-source-rows", version: "1.0.0", evidenceDigest: sha("d") }],
    declassificationDecision: {
      policyRef: clone(POLICY_REF), decisionRef: null, version: "1.0.0", outcome: "approved", removedSensitivities: ["internal"],
    },
    aggregationDecision: {
      policyRef: clone(POLICY_REF), decisionRef: null, version: "1.0.0", outcome: "approved",
      removesSourceRows: true, removesSourceIdentities: true,
    },
    containsSourceRows: false,
    containsSourceIdentities: false,
    reevaluated: true,
  };
  input.derivedArtifact.declassificationDecision.decisionRef = authorizingEvidence(input, "declassification", "4");
  input.derivedArtifact.aggregationDecision.decisionRef = authorizingEvidence(input, "aggregation", "5");
  setProvenanceEvidence(input, "exportTransferConsentRef", "6");
  return input;
}

try {
  const seed = JSON.parse(await readFile(join(root, "tests/fixtures/privacy/allowed.json"), "utf8"))[0].decision;

  const conflictGeneric = clone(seed);
  conflictGeneric.conflictResolution = {
    state: "resolved-allow",
    decisionRef: { id: "caller.says.allow", version: "99.99.99", evidenceDigest: sha("9") },
  };
  assertProtocol(await runDecision("conflict-caller-says-allow", conflictGeneric), 1, "deny", "input.conflict-resolution");

  const genericTriple = publicFixture(seed, "publish", false);
  genericTriple.provenance.publicFixturePermissionRef = clone(genericAudit);
  genericTriple.provenance.publicFixtureLicenseRef = clone(genericAudit);
  genericTriple.provenance.publicFixtureConsentRef = clone(genericAudit);
  assertProtocol(await runDecision("generic-object-reused-for-triple", genericTriple), 1, "deny", "input.provenance");

  for (const operation of ["repository-store", "transfer", "publish"]) {
    const staleVersion = setFixtureEvidence(publicFixture(seed, operation, false));
    staleVersion.provenance.publicFixturePermissionRef.version = "99.99.99";
    assertProtocol(await runDecision(`stale-contract-version-${operation}`, staleVersion), 1, "deny", "input.provenance");
  }

  for (const [name, left, right] of [
    ["permission-license", "publicFixturePermissionRef", "publicFixtureLicenseRef"],
    ["license-consent", "publicFixtureLicenseRef", "publicFixtureConsentRef"],
    ["consent-permission", "publicFixtureConsentRef", "publicFixturePermissionRef"],
  ]) {
    const swapped = setFixtureEvidence(publicFixture(seed, "publish", false));
    swapped.provenance[left].purpose = swapped.provenance[right].purpose;
    assertProtocol(await runDecision(`wrong-purpose-${name}`, swapped), 1, "deny", "input.provenance");
  }

  for (const [name, mutate, reason] of [
    ["unverified", (evidence) => { evidence.verificationState = "unverified"; }, "evidence.unverified"],
    ["stale", (evidence) => { evidence.freshnessState = "stale"; }, "evidence.stale"],
    ["expired", (evidence) => { evidence.freshnessState = "expired"; }, "evidence.expired"],
    ["wrong-outcome", (evidence) => { evidence.outcome = "denied"; }, "evidence.outcome-mismatch"],
    ["binding-mismatch", (evidence) => { evidence.binding.subjectDigest = `subject-sha256:${"8".repeat(64)}`; }, "evidence.binding-mismatch"],
  ]) {
    const input = setFixtureEvidence(publicFixture(seed, "publish", false));
    mutate(input.provenance.publicFixturePermissionRef);
    assertProtocol(await runDecision(`semantic-${name}`, input), 3, "deny", reason);
  }

  const missingEvidence = publicFixture(seed, "publish", false);
  assertProtocol(await runDecision("missing-fixture-evidence", missingEvidence), 3, "deny", "fixture.permission-license-consent-required");

  const reusedIdentity = setFixtureEvidence(publicFixture(seed, "publish", false));
  reusedIdentity.provenance.publicFixtureLicenseRef.evidenceId = reusedIdentity.provenance.publicFixturePermissionRef.evidenceId;
  assertProtocol(await runDecision("typed-evidence-identity-reused", reusedIdentity), 3, "deny", "evidence.identity-reused");

  const consumerStore = clone(seed);
  consumerStore.artifactKind = "consumer.model";
  consumerStore.exportDisposition = "consumer-repository-only";
  consumerStore.dataSensitivity = ["public"];
  consumerStore.source = { repositoryRole: "consumer-repository", repositoryRef: REPO_ONE };
  consumerStore.provenance.origin = "consumer-repository";
  consumerStore.provenance.repositoryRole = "consumer-repository";
  consumerStore.provenance.repositoryRef = REPO_ONE;
  consumerStore.provenance.synthetic = false;
  consumerStore.provenance.derived = false;
  setDestination(consumerStore, "repository-store");
  setProvenanceEvidence(consumerStore, "consumerAclPermissionRef", "a");
  setProvenanceEvidence(consumerStore, "consumerRepositoryConsentRef", "b");
  assertProtocol(await runDecision("consumer-store-exact-acl-consent", consumerStore), 0, "allow", "policy.allow");
  for (const [name, mutate, exitCode, reason] of [
    ["stale-contract-version", (input) => { input.provenance.consumerAclPermissionRef.version = "99.99.99"; }, 1, "input.provenance"],
    ["wrong-purpose", (input) => { input.provenance.consumerAclPermissionRef.purpose = input.provenance.consumerRepositoryConsentRef.purpose; }, 1, "input.provenance"],
    ["unverified", (input) => { input.provenance.consumerAclPermissionRef.verificationState = "unverified"; }, 3, "evidence.unverified"],
    ["wrong-outcome", (input) => { input.provenance.consumerRepositoryConsentRef.outcome = "denied"; }, 3, "evidence.outcome-mismatch"],
    ["reused-identity", (input) => { input.provenance.consumerRepositoryConsentRef.evidenceId = input.provenance.consumerAclPermissionRef.evidenceId; }, 3, "evidence.identity-reused"],
  ]) {
    const input = clone(consumerStore);
    mutate(input);
    assertProtocol(await runDecision(`consumer-store-${name}`, input), exitCode, "deny", reason);
  }

  const shareablePublish = clone(seed);
  shareablePublish.artifactKind = "ai-factory.provider-evidence-envelope";
  shareablePublish.exportDisposition = "shareable-with-redaction";
  shareablePublish.dataSensitivity = ["public"];
  setProvenanceEvidence(shareablePublish, "exportTransferConsentRef", "c");
  assertProtocol(await runDecision("general-transfer-consent-exact", shareablePublish), 3, "transform-required", "disposition.transform-required");
  for (const [name, mutate, exitCode, reason] of [
    ["missing", (input) => { input.provenance.exportTransferConsentRef = null; }, 3, "provenance.consent-required"],
    ["stale-contract-version", (input) => { input.provenance.exportTransferConsentRef.version = "99.99.99"; }, 1, "input.provenance"],
    ["wrong-purpose", (input) => { input.provenance.exportTransferConsentRef.purpose = "authorize-public-fixture-consent"; }, 1, "input.provenance"],
    ["unverified", (input) => { input.provenance.exportTransferConsentRef.verificationState = "unverified"; }, 3, "evidence.unverified"],
    ["wrong-outcome", (input) => { input.provenance.exportTransferConsentRef.outcome = "denied"; }, 3, "evidence.outcome-mismatch"],
  ]) {
    const input = clone(shareablePublish);
    mutate(input);
    assertProtocol(await runDecision(`general-transfer-consent-${name}`, input), exitCode, "deny", reason);
  }

  const conflictExact = clone(seed);
  conflictExact.conflictResolution = { state: "resolved-allow", decisionRef: null };
  conflictExact.conflictResolution.decisionRef = authorizingEvidence(conflictExact, "conflictResolution", "7");
  assertProtocol(await runDecision("conflict-exact-current", conflictExact), 0, "allow", "policy.allow");
  for (const [name, mutate, exitCode, reason] of [
    ["unknown-contract", (evidence) => { evidence.contractId = "caller.says.allow"; }, 1, "input.conflict-resolution"],
    ["stale-contract-version", (evidence) => { evidence.version = "99.99.99"; }, 1, "input.conflict-resolution"],
    ["wrong-purpose", (evidence) => { evidence.purpose = "authorize-public-fixture-consent"; }, 1, "input.conflict-resolution"],
    ["unverified", (evidence) => { evidence.verificationState = "unverified"; }, 3, "evidence.unverified"],
    ["expired", (evidence) => { evidence.freshnessState = "expired"; }, 3, "evidence.expired"],
    ["wrong-outcome", (evidence) => { evidence.outcome = "deny"; }, 3, "evidence.outcome-mismatch"],
  ]) {
    const input = clone(conflictExact);
    mutate(input.conflictResolution.decisionRef);
    assertProtocol(await runDecision(`conflict-${name}`, input), exitCode, "deny", reason);
  }

  const exactAggregate = aggregate(seed);
  assertProtocol(await runDecision("aggregate-exact-evidence", exactAggregate), 0, "allow", "policy.allow");
  const missingDeclassification = clone(exactAggregate);
  missingDeclassification.derivedArtifact.declassificationDecision = null;
  refreshEvidenceBindings(missingDeclassification);
  assertProtocol(await runDecision("aggregate-missing-declassification-evidence", missingDeclassification),
    3, "deny", "derived.declassification-required");
  const missingAggregation = clone(exactAggregate);
  missingAggregation.derivedArtifact.aggregationDecision = null;
  refreshEvidenceBindings(missingAggregation);
  assertProtocol(await runDecision("aggregate-missing-aggregation-evidence", missingAggregation),
    3, "deny", "derived.aggregation-decision-required");
  for (const [field, selector] of [
    ["declassification", (input) => input.derivedArtifact.declassificationDecision.decisionRef],
    ["aggregation", (input) => input.derivedArtifact.aggregationDecision.decisionRef],
  ]) {
    for (const [name, mutate] of [
      ["stale-contract-version", (evidence) => { evidence.version = "99.99.99"; }],
      ["unknown-contract", (evidence) => { evidence.contractId = "caller.unrelated"; }],
      ["wrong-purpose", (evidence) => { evidence.purpose = "authorize-public-fixture-permission"; }],
    ]) {
      const input = clone(exactAggregate);
      mutate(selector(input));
      assertProtocol(await runDecision(`${field}-${name}`, input), 1, "deny", "input.derived-artifact");
    }
  }

  const matrixStates = ["missing", "exact", "stale", "unverified", "expired", "wrong-purpose", "reused"];
  const syntheticMissingAllow = new Set(["local-use", "derive", "publish"]);
  let matrixCases = 0;
  for (const operation of ["local-use", "repository-store", "derive", "transfer", "publish"]) {
    for (const synthetic of [false, true]) {
      for (const state of matrixStates) {
        const input = publicFixture(seed, operation, synthetic);
        if (state !== "missing") setFixtureEvidence(input);
        if (state === "stale") input.provenance.publicFixturePermissionRef.freshnessState = "stale";
        if (state === "unverified") input.provenance.publicFixturePermissionRef.verificationState = "unverified";
        if (state === "expired") input.provenance.publicFixturePermissionRef.freshnessState = "expired";
        if (state === "wrong-purpose") input.provenance.publicFixturePermissionRef.purpose = "authorize-public-fixture-license";
        if (state === "reused") input.provenance.publicFixtureLicenseRef.evidenceId = input.provenance.publicFixturePermissionRef.evidenceId;

        const malformed = state === "wrong-purpose";
        const nonSyntheticAllow = !synthetic
          && (["local-use", "derive"].includes(operation) ? ["missing", "exact"].includes(state) : state === "exact");
        const syntheticAllow = synthetic && state === "missing" && syntheticMissingAllow.has(operation);
        const expectedAllow = nonSyntheticAllow || syntheticAllow;
        const expectedExit = malformed ? 1 : expectedAllow ? 0 : 3;
        const expectedDecision = expectedAllow ? "allow" : "deny";
        assertProtocol(await runDecision(`matrix-${operation}-${synthetic ? "synthetic" : "origin"}-${state}`, input),
          expectedExit, expectedDecision);
        matrixCases += 1;
      }
    }
  }
  assert.equal(matrixCases, 70);

  const oldManifest = invoke([
    "--manifest", join(root, "contracts/privacy-policy.v1.0.3.manifest.json"),
    "--manifest-sidecar", join(root, "contracts/privacy-policy.v1.0.3.manifest.sha256"),
  ]);
  assert.equal(oldManifest.status, 1);
  assert.equal(JSON.parse(oldManifest.stderr).reasonCodes[0], "custody.manifest-untrusted");
  subprocessCases += 1;

  const evidenceContract = JSON.parse(await readFile(join(root, "contracts/privacy-authorizing-evidence.v1.0.0.json"), "utf8"));
  evidenceContract.registry[0].expectedOutcome = "denied";
  const evidenceContractPath = join(temp, "mutated-evidence-contract.json");
  const evidenceContractBytes = Buffer.from(`${JSON.stringify(evidenceContract, null, 2)}\n`);
  const evidenceSidecarPath = join(temp, "mutated-evidence-contract.sha256");
  await writeFile(evidenceContractPath, evidenceContractBytes);
  await writeFile(evidenceSidecarPath,
    `${createHash("sha256").update(evidenceContractBytes).digest("hex")}  privacy-authorizing-evidence.v1.0.0.json\n`);
  const recomputedEvidence = invoke([
    "--authorizing-evidence-contract", evidenceContractPath,
    "--authorizing-evidence-sidecar", evidenceSidecarPath,
  ]);
  assert.equal(recomputedEvidence.status, 1);
  assert.equal(JSON.parse(recomputedEvidence.stderr).reasonCodes[0], "custody.authorizing-evidence-bytes-mismatch");
  subprocessCases += 1;

  const currentManifest = JSON.parse(await readFile(join(root, "contracts/privacy-policy.v1.0.4.manifest.json"), "utf8"));
  currentManifest.acceptedContracts[0].authorizingEvidenceContractRef.digest = sha("0");
  const mutatedManifestPath = join(temp, "mutated-manifest.json");
  const mutatedManifestBytes = Buffer.from(`${JSON.stringify(currentManifest, null, 2)}\n`);
  const mutatedManifestSidecarPath = join(temp, "mutated-manifest.sha256");
  await writeFile(mutatedManifestPath, mutatedManifestBytes);
  await writeFile(mutatedManifestSidecarPath,
    `${createHash("sha256").update(mutatedManifestBytes).digest("hex")}  privacy-policy.v1.0.4.manifest.json\n`);
  const recomputedManifest = invoke(["--manifest", mutatedManifestPath, "--manifest-sidecar", mutatedManifestSidecarPath]);
  assert.equal(recomputedManifest.status, 1);
  assert.equal(JSON.parse(recomputedManifest.stderr).reasonCodes[0], "custody.manifest-untrusted");
  subprocessCases += 1;

  const aggregateWithSecondSource = clone(exactAggregate);
  const second = clone(aggregateWithSecondSource.derivedArtifact.sourceArtifacts[0]);
  second.sourceRef = sourceRef("b");
  second.classificationRef = classification("d");
  aggregateWithSecondSource.derivedArtifact.sourceArtifacts.push(second);
  refreshEvidenceBindings(aggregateWithSecondSource);
  assertProtocol(await runDecision("aggregate-evidence-binding-two-sources", aggregateWithSecondSource), 0, "allow", "policy.allow");

  console.log(JSON.stringify({
    ok: true,
    subprocessCases,
    matrixCases,
    exactPurposeBoundFields: 9,
    genericAuditAuthorizes: false,
    exitCodes: [0, 1, 3],
  }));
} finally {
  await rm(temp, { recursive: true, force: true });
}
