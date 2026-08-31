import { readFileSync } from "node:fs";
import {
  AUTHORIZING_EVIDENCE_CONTRACT_REF,
  AUTHORIZATION_SUBJECT_PROFILE_REF,
  authorizationSubjectDigest,
} from "./check-privacy.mjs";

export const AUTHORIZATION_SUBJECT_PROFILE = Object.freeze(JSON.parse(readFileSync(
  new URL("../contracts/privacy-authorization-subject-profile.v1.0.0.json", import.meta.url), "utf8",
)));

const SPECS = Object.freeze({
  publicFixturePermissionRef: { evidenceKind: "public-fixture-permission", purpose: "authorize-public-fixture-permission", outcome: "granted" },
  publicFixtureLicenseRef: { evidenceKind: "public-fixture-license", purpose: "authorize-public-fixture-license", outcome: "licensed" },
  publicFixtureConsentRef: { evidenceKind: "public-fixture-consent", purpose: "authorize-public-fixture-consent", outcome: "granted" },
  consumerAclPermissionRef: { evidenceKind: "consumer-acl-permission", purpose: "authorize-consumer-repository-acl", outcome: "granted" },
  consumerRepositoryConsentRef: { evidenceKind: "consumer-repository-consent", purpose: "authorize-consumer-repository-consent", outcome: "granted" },
  exportTransferConsentRef: { evidenceKind: "export-transfer-consent", purpose: "authorize-export-transfer-or-storage", outcome: "granted" },
  conflictResolution: { evidenceKind: "conflict-resolution-decision", purpose: "resolve-export-conflict", outcome: "allow" },
  declassification: { evidenceKind: "declassification-decision", purpose: "authorize-declassification", outcome: "approved" },
  aggregation: { evidenceKind: "aggregation-decision", purpose: "authorize-public-aggregation", outcome: "approved" },
});

export function evidenceBinding(input) {
  return {
    subjectProfileRef: { ...AUTHORIZATION_SUBJECT_PROFILE_REF },
    subjectDigest: authorizationSubjectDigest(input, AUTHORIZATION_SUBJECT_PROFILE),
  };
}

export function authorizingEvidence(input, field, character, overrides = {}) {
  const spec = SPECS[field];
  if (!spec) throw new Error(`unknown authorizing evidence field: ${field}`);
  return {
    ...AUTHORIZING_EVIDENCE_CONTRACT_REF,
    evidenceKind: spec.evidenceKind,
    purpose: spec.purpose,
    outcome: spec.outcome,
    evidenceId: `evidence-sha256:${character.repeat(64)}`,
    verificationState: "verified",
    freshnessState: "current",
    binding: evidenceBinding(input),
    ...overrides,
  };
}

export function refreshEvidenceBindings(input) {
  const binding = evidenceBinding(input);
  for (const field of [
    "publicFixturePermissionRef",
    "publicFixtureLicenseRef",
    "publicFixtureConsentRef",
    "consumerAclPermissionRef",
    "consumerRepositoryConsentRef",
    "exportTransferConsentRef",
  ]) {
    if (input.provenance[field]) input.provenance[field].binding = structuredClone(binding);
  }
  if (input.conflictResolution.decisionRef) input.conflictResolution.decisionRef.binding = structuredClone(binding);
  if (input.derivedArtifact?.declassificationDecision?.decisionRef) {
    input.derivedArtifact.declassificationDecision.decisionRef.binding = structuredClone(binding);
  }
  if (input.derivedArtifact?.aggregationDecision?.decisionRef) {
    input.derivedArtifact.aggregationDecision.decisionRef.binding = structuredClone(binding);
  }
  return input;
}

export function setProvenanceEvidence(input, field, character, overrides = {}) {
  input.provenance[field] = authorizingEvidence(input, field, character, overrides);
  return input;
}
