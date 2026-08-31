#!/usr/bin/env node

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { evaluateDecision, loadTrustedContext } from "./check-privacy.mjs";
import { setProvenanceEvidence } from "./privacy-test-helpers.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const context = await loadTrustedContext();
const base = JSON.parse(await readFile(`${root}/tests/fixtures/privacy/allowed.json`, "utf8"))[0].decision;
const clone = (value) => structuredClone(value);
const digest = (character) => `sha256:${character.repeat(64)}`;
const REPO_ONE = `repo-sha256:${"1".repeat(64)}`;
const REPO_TWO = `repo-sha256:${"2".repeat(64)}`;

function setOrigin(input, origin, synthetic, derived) {
  const roleByOrigin = {
    synthetic: "local-workspace",
    "consumer-repository": "consumer-repository",
    "lekalo-repository": "lekalo-repository",
    "tool-runtime": "consumer-repository",
    derived: "consumer-repository",
    external: "external-repository",
  };
  const role = roleByOrigin[origin];
  const ref = role === "local-workspace" ? null : REPO_ONE;
  input.source = { repositoryRole: role, repositoryRef: ref };
  input.provenance.origin = origin;
  input.provenance.repositoryRole = role;
  input.provenance.repositoryRef = ref;
  input.provenance.synthetic = synthetic;
  input.provenance.derived = derived;
}

function setDestination(input, operation) {
  const { repositoryRole, repositoryRef } = input.source;
  input.operation.id = operation;
  if (["local-use", "derive"].includes(operation)) {
    input.destination = {
      repositoryRole: "local-workspace", repositoryRef: null, trustBoundary: "same-local-workspace",
      repositoryRelation: "not-applicable", tenantRelation: "same-tenant",
    };
    input.audience = "operator-only";
    return;
  }
  if (operation === "repository-store") {
    input.destination = {
      repositoryRole, repositoryRef, trustBoundary: "same-repository",
      repositoryRelation: "same-origin", tenantRelation: "same-tenant",
    };
    input.audience = "repository-collaborators";
    return;
  }
  if (operation === "transfer") {
    input.destination = {
      repositoryRole: "external-repository", repositoryRef: REPO_TWO, trustBoundary: "same-tenant",
      repositoryRelation: "different-repository", tenantRelation: "same-tenant",
    };
    input.audience = "tenant-members";
    return;
  }
  input.destination = {
    repositoryRole: "public-channel", repositoryRef: null, trustBoundary: "public",
    repositoryRelation: "not-applicable", tenantRelation: "not-applicable",
  };
  input.audience = "public";
}

const ORIGIN_TRUTH = new Map([
  ["synthetic", new Set(["true,false"])],
  ["derived", new Set(["false,true"])],
  ["consumer-repository", new Set(["false,false"])],
  ["lekalo-repository", new Set(["false,false"])],
  ["tool-runtime", new Set(["false,false"])],
  ["external", new Set(["false,false"])],
]);

let provenanceTruthCases = 0;
for (const [origin, acceptedStates] of ORIGIN_TRUTH) {
  for (const synthetic of [false, true]) {
    for (const derived of [false, true]) {
      const input = clone(base);
      input.operation.id = "local-use";
      setOrigin(input, origin, synthetic, derived);
      setDestination(input, "local-use");
      const actual = evaluateDecision(input, context);
      const coherent = acceptedStates.has(`${synthetic},${derived}`);
      if (coherent) {
        assert.equal(actual.malformed, false, `${origin}/${synthetic}/${derived}: shaped`);
        assert.notEqual(actual.output.reasonCodes[0], "provenance.origin-boolean-conflict", `${origin}/${synthetic}/${derived}: coherent mode`);
      } else {
        assert.equal(actual.malformed, false, `${origin}/${synthetic}/${derived}: semantic conflict`);
        assert.equal(actual.output.decision, "deny", `${origin}/${synthetic}/${derived}: deny`);
        assert.equal(actual.output.reasonCodes[0], "provenance.origin-boolean-conflict", `${origin}/${synthetic}/${derived}: exclusive truth table`);
        assert.equal(actual.output.sourceTransferAllowed, false);
      }
      provenanceTruthCases += 1;
    }
  }
}
assert.equal(provenanceTruthCases, 24);

let publicFixtureEvidenceCases = 0;
for (const operation of ["repository-store", "transfer", "publish"]) {
  for (const syntheticMode of [false, true]) {
    for (const permission of [false, true]) {
      for (const license of [false, true]) {
        for (const consent of [false, true]) {
          const input = clone(base);
          input.artifactKind = "fixture";
          input.exportDisposition = "public-fixture";
          input.dataSensitivity = ["public"];
          setOrigin(input, syntheticMode ? "synthetic" : "consumer-repository", syntheticMode, false);
          setDestination(input, operation);
          if (permission) setProvenanceEvidence(input, "publicFixturePermissionRef", "4");
          if (license) setProvenanceEvidence(input, "publicFixtureLicenseRef", "5");
          if (consent) setProvenanceEvidence(input, "publicFixtureConsentRef", "6");
          const actual = evaluateDecision(input, context);
          const coherentSyntheticOperation = !syntheticMode || operation === "publish";
          const expectedAllow = coherentSyntheticOperation
            && (syntheticMode ? !permission && !license && !consent : permission && license && consent);
          assert.equal(actual.malformed, false, `${operation}/${syntheticMode}/${permission}/${license}/${consent}: shaped`);
          assert.equal(actual.output.decision, expectedAllow ? "allow" : "deny",
            `${operation}/${syntheticMode}/${permission}/${license}/${consent}: evidence truth table`);
          if (!expectedAllow) assert.equal(actual.output.sourceTransferAllowed, false);
          publicFixtureEvidenceCases += 1;
        }
      }
    }
  }
}
assert.equal(publicFixtureEvidenceCases, 48);

const CONSUMER_CONTEXT_VARIANTS = [
  { id: "exact-consumer", sourceRole: "consumer-repository", sourceRef: REPO_ONE, provenanceRole: "consumer-repository", provenanceRef: REPO_ONE, origin: "consumer-repository" },
  { id: "wrong-origin", sourceRole: "consumer-repository", sourceRef: REPO_ONE, provenanceRole: "consumer-repository", provenanceRef: REPO_ONE, origin: "lekalo-repository" },
  { id: "wrong-role", sourceRole: "lekalo-repository", sourceRef: REPO_ONE, provenanceRole: "lekalo-repository", provenanceRef: REPO_ONE, origin: "lekalo-repository" },
  { id: "missing-ref", sourceRole: "consumer-repository", sourceRef: null, provenanceRole: "consumer-repository", provenanceRef: null, origin: "consumer-repository" },
  { id: "mismatched-ref", sourceRole: "consumer-repository", sourceRef: REPO_ONE, provenanceRole: "consumer-repository", provenanceRef: REPO_TWO, origin: "consumer-repository" },
];

let consumerOriginCases = 0;
for (const operation of ["local-use", "repository-store", "derive", "transfer", "publish"]) {
  for (const variant of CONSUMER_CONTEXT_VARIANTS) {
    const input = clone(base);
    input.artifactKind = "consumer.model";
    input.exportDisposition = "consumer-repository-only";
    input.dataSensitivity = ["public"];
    input.source = { repositoryRole: variant.sourceRole, repositoryRef: variant.sourceRef };
    input.provenance.origin = variant.origin;
    input.provenance.repositoryRole = variant.provenanceRole;
    input.provenance.repositoryRef = variant.provenanceRef;
    input.provenance.synthetic = false;
    input.provenance.derived = false;
    setDestination(input, operation);
    if (operation === "repository-store") {
      setProvenanceEvidence(input, "consumerAclPermissionRef", "7");
      setProvenanceEvidence(input, "consumerRepositoryConsentRef", "8");
    }
    const actual = evaluateDecision(input, context);
    const expectedAllow = variant.id === "exact-consumer" && ["local-use", "repository-store", "derive"].includes(operation);
    assert.equal(actual.malformed, false, `${operation}/${variant.id}: shaped`);
    assert.equal(actual.output.decision, expectedAllow ? "allow" : "deny", `${operation}/${variant.id}: consumer origin matrix`);
    if (!expectedAllow) assert.equal(actual.output.sourceTransferAllowed, false);
    consumerOriginCases += 1;
  }
}
assert.equal(consumerOriginCases, 25);

console.log(JSON.stringify({
  ok: true,
  provenanceTruthCases,
  publicFixtureEvidenceCases,
  consumerOriginCases,
  independentExpectationTables: true,
}));
