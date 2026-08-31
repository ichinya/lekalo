#!/usr/bin/env node

import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { isAbsolute } from "node:path";
import { pathToFileURL } from "node:url";

const BASE = new URL("../", import.meta.url);
const DEFAULTS = {
  manifest: new URL("contracts/privacy-policy.v1.0.6.manifest.json", BASE),
  manifestSidecar: new URL("contracts/privacy-policy.v1.0.6.manifest.sha256", BASE),
  policy: new URL("contracts/privacy-policy.v1.0.6.json", BASE),
  policySidecar: new URL("contracts/privacy-policy.v1.0.6.sha256", BASE),
  classificationContract: new URL("contracts/privacy-policy.v1.0.2.classification.json", BASE),
  classificationSidecar: new URL("contracts/privacy-policy.v1.0.2.classification.sha256", BASE),
  authorizingEvidenceContract: new URL("contracts/privacy-authorizing-evidence.v1.1.0.json", BASE),
  authorizingEvidenceSidecar: new URL("contracts/privacy-authorizing-evidence.v1.1.0.sha256", BASE),
  authorizationSubjectProfile: new URL("contracts/privacy-authorization-subject-profile.v1.0.0.json", BASE),
  authorizationSubjectProfileSidecar: new URL("contracts/privacy-authorization-subject-profile.v1.0.0.sha256", BASE),
  inputSchema: new URL("contracts/privacy-export.schema.v2.5.json", BASE),
  outputSchema: new URL("contracts/privacy-export.schema.v2.5.output.json", BASE),
  cliErrorSchema: new URL("contracts/privacy-cli-error.schema.v1.0.0.json", BASE),
  classificationSchema: new URL("contracts/privacy-export.schema.v2.classification.json", BASE),
  authority: new URL("contracts/authority-matrix.v1.3.1.json", BASE),
};

const TRUSTED_MANIFEST_SHA256 = "3cbc9b428d47218872c62124c36fb15eeda5df2fe565ca8d3afbf477b5e16747";
const POLICY_RAW_SHA256 = "29bf9a669a775442bf393b359a92c219eb1365014ad915b312768ff24414fbe6";
const CLASSIFICATION_CONTRACT_RAW_SHA256 = "58626d1889990bf6120874fcd194c9fc68f2d81f05af1f3b8f7d04336fb3aa9e";
const AUTHORIZING_EVIDENCE_RAW_SHA256 = "6402f7918f23edc06d68101341d0ebf44039c2e17351da97f506f688d4a6e4f4";
const AUTHORIZATION_SUBJECT_PROFILE_RAW_SHA256 = "825546e4a4df4551e122c2cd548ae81c1fa2063d0bd8f256d728bffc089676a1";
const INPUT_SCHEMA_RAW_SHA256 = "1f44df586b020267ceb1bcf982981ce1a7f7ab7fb0c0aa99a88dce29af1a2150";
const OUTPUT_SCHEMA_RAW_SHA256 = "973786421855c93484492b589012606fd81481631735731a5b59c7d21bff1aac";
const CLI_ERROR_SCHEMA_RAW_SHA256 = "8b9525ff8c5431a09546c06baa36e1afe29ecedd14b80c99275509b7b0e13cfd";
const CLASSIFICATION_SCHEMA_RAW_SHA256 = "b996f62f23eb3341518ba3c4d917757814d27200c2ab410f2c127b4f161d50cb";
const AUTHORITY_RAW_SHA256 = "5c96ed68fe27956512b6de37e0fa23d223e4430d391b6a5869b12d2a8cb522d3";

export const POLICY_REF = Object.freeze({
  policyId: "dev.lekalo.privacy-export-policy",
  version: "1.0.6",
  digest: "sha256:99a813a89efbdf336340390c9589a4f05d0dbbc8805748708b455a3d7a329ca7",
});
export const AUTHORITY_REF = Object.freeze({
  contractId: "dev.lekalo.authority-matrix",
  version: "1.3.1",
  digest: `sha256:${AUTHORITY_RAW_SHA256}`,
});
export const CLASSIFICATION_CONTRACT_REF = Object.freeze({
  contractId: "dev.lekalo.privacy-classification-decision",
  version: "1.0.0",
  digest: `sha256:${CLASSIFICATION_CONTRACT_RAW_SHA256}`,
});
export const AUTHORIZING_EVIDENCE_CONTRACT_REF = Object.freeze({
  contractId: "dev.lekalo.privacy-authorizing-evidence",
  version: "1.1.0",
  digest: `sha256:${AUTHORIZING_EVIDENCE_RAW_SHA256}`,
});
export const AUTHORIZATION_SUBJECT_PROFILE_REF = Object.freeze({
  profileId: "dev.lekalo.privacy-authorization-subject-profile",
  version: "1.0.0",
  digest: `sha256:${AUTHORIZATION_SUBJECT_PROFILE_RAW_SHA256}`,
});
export const DECISION_REF = Object.freeze({ contractId: "dev.lekalo.privacy-export-decision", version: "1.5.0" });
export const INPUT_SCHEMA_REF = Object.freeze({ schemaId: "dev.lekalo.privacy-export-input-schema", version: "2.5.0" });
export const OUTPUT_SCHEMA_REF = Object.freeze({ schemaId: "dev.lekalo.privacy-export-output-schema", version: "1.5.0" });
export const CLI_ERROR_SCHEMA_REF = Object.freeze({ schemaId: "dev.lekalo.privacy-cli-error-schema", version: "1.0.0" });
export const CLASSIFICATION_SCHEMA_REF = Object.freeze({ schemaId: "dev.lekalo.privacy-classification-decision-schema", version: "1.0.0" });

const TRANSFORM_REQUIREMENTS = Object.freeze([
  "new-artifact",
  "new-provenance",
  "retain-exact-source-authority-and-policy-refs",
  "retain-source-classification-refs",
  "record-applied-transforms",
  "record-declassification-or-aggregation-decision",
  "reevaluate-derived-output",
]);
const VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const DIGEST = /^sha256:[0-9a-f]{64}$/;
const DOS_DEVICE = /^(con|prn|aux|nul|com[1-9]|lpt[1-9]|conin\$|conout\$|clock\$)(?:\..*)?$/i;
const REPOSITORY_REF = /^repo-sha256:[0-9a-f]{64}$/;
const SOURCE_REF = /^source-sha256:[0-9a-f]{64}$/;
const ARTIFACT_REF = /^artifact-sha256:[0-9a-f]{64}$/;
const EVIDENCE_REF = /^evidence-sha256:[0-9a-f]{64}$/;
const SUBJECT_DIGEST = /^subject-sha256:[0-9a-f]{64}$/;
const CLASSIFICATION_DECISION_ID = /^classification-sha256:[0-9a-f]{64}$/;

const AUTHORIZING_EVIDENCE_SPECS = Object.freeze({
  publicFixturePermissionRef: Object.freeze({ evidenceKind: "public-fixture-permission", purpose: "authorize-public-fixture-permission", outcome: "granted", outcomes: ["granted", "denied"] }),
  publicFixtureLicenseRef: Object.freeze({ evidenceKind: "public-fixture-license", purpose: "authorize-public-fixture-license", outcome: "licensed", outcomes: ["licensed", "unlicensed"] }),
  publicFixtureConsentRef: Object.freeze({ evidenceKind: "public-fixture-consent", purpose: "authorize-public-fixture-consent", outcome: "granted", outcomes: ["granted", "denied"] }),
  consumerAclPermissionRef: Object.freeze({ evidenceKind: "consumer-acl-permission", purpose: "authorize-consumer-repository-acl", outcome: "granted", outcomes: ["granted", "denied"] }),
  consumerRepositoryConsentRef: Object.freeze({ evidenceKind: "consumer-repository-consent", purpose: "authorize-consumer-repository-consent", outcome: "granted", outcomes: ["granted", "denied"] }),
  exportTransferConsentRef: Object.freeze({ evidenceKind: "export-transfer-consent", purpose: "authorize-export-transfer-or-storage", outcome: "granted", outcomes: ["granted", "denied"] }),
  conflictResolution: Object.freeze({ evidenceKind: "conflict-resolution-decision", purpose: "resolve-export-conflict", outcome: null, outcomes: ["allow", "deny"] }),
  declassification: Object.freeze({ evidenceKind: "declassification-decision", purpose: "authorize-declassification", outcome: "approved", outcomes: ["approved", "rejected"] }),
  aggregation: Object.freeze({ evidenceKind: "aggregation-decision", purpose: "authorize-public-aggregation", outcome: "approved", outcomes: ["approved", "rejected"] }),
});

function record(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function exactKeys(value, required, optional = []) {
  if (!record(value)) return false;
  const keys = Object.keys(value);
  const allowed = new Set([...required, ...optional]);
  return required.every((key) => Object.hasOwn(value, key)) && keys.every((key) => allowed.has(key));
}

function sameObject(actual, expected) {
  return record(actual)
    && Object.keys(actual).length === Object.keys(expected).length
    && Object.entries(expected).every(([key, value]) => actual[key] === value);
}

function uniqueKnown(values, vocabulary, { nonempty = true } = {}) {
  return Array.isArray(values)
    && (!nonempty || values.length > 0)
    && values.every((value) => typeof value === "string" && vocabulary.includes(value))
    && new Set(values).size === values.length;
}

function rawSha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function compareUnicodeCodePoints(left, right) {
  const a = Array.from(left);
  const b = Array.from(right);
  for (let index = 0; index < Math.min(a.length, b.length); index += 1) {
    const delta = a[index].codePointAt(0) - b[index].codePointAt(0);
    if (delta !== 0) return delta;
  }
  return a.length - b.length;
}

function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (record(value)) {
    return `{${Object.keys(value).sort(compareUnicodeCodePoints).map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

function pathSegments(path) {
  return path.split(".").map((part) => ({ key: part.endsWith("[]") ? part.slice(0, -2) : part, each: part.endsWith("[]") }));
}

function visitPath(root, path, visitor) {
  const segments = pathSegments(path);
  function visit(current, index) {
    if (!record(current)) return;
    const segment = segments[index];
    if (!Object.hasOwn(current, segment.key)) return;
    if (index === segments.length - 1) {
      visitor(current, segment.key, current[segment.key]);
      return;
    }
    const next = current[segment.key];
    if (segment.each) {
      if (Array.isArray(next)) for (const item of next) visit(item, index + 1);
    } else {
      visit(next, index + 1);
    }
  }
  visit(root, 0);
}

function authorizationSubjectProjection(input, profile) {
  const projected = structuredClone(input);
  for (const path of profile.projection.excludedPaths) visitPath(projected, path, (parent, key) => { delete parent[key]; });
  for (const rule of profile.projection.setCanonicalizationRules) {
    visitPath(projected, rule.path, (parent, key, value) => {
      if (!Array.isArray(value)) return;
      if (rule.strategy === "unicode-code-point-lexicographic") value.sort(compareUnicodeCodePoints);
      else if (rule.strategy === "object-key-then-canonical") {
        value.sort((left, right) => compareUnicodeCodePoints(left[rule.key], right[rule.key]) || compareUnicodeCodePoints(canonical(left), canonical(right)));
      } else if (rule.strategy === "canonical-json-lexicographic") {
        value.sort((left, right) => compareUnicodeCodePoints(canonical(left), canonical(right)));
      } else throw new Error("custody.subject-profile-unknown-canonicalization");
      parent[key] = value;
    });
  }
  return projected;
}

export function authorizationSubjectDigest(input, profile) {
  if (!record(profile) || profile.profileId !== AUTHORIZATION_SUBJECT_PROFILE_REF.profileId
    || profile.version !== AUTHORIZATION_SUBJECT_PROFILE_REF.version) throw new Error("custody.subject-profile-ref-mismatch");
  return `subject-sha256:${rawSha256(canonical(authorizationSubjectProjection(input, profile)))}`;
}

function policyIdentity(policy) {
  const projected = structuredClone(policy);
  if (!record(projected.policyRef)) return "";
  delete projected.policyRef.digest;
  return `sha256:${rawSha256(canonical(projected))}`;
}

function parseJson(bytes, name) {
  try {
    return JSON.parse(bytes.toString("utf8"));
  } catch (error) {
    throw new Error(`custody.${name}.json: ${error.message}`);
  }
}

function sidecarMatches(bytes, expectedDigest, filename) {
  return bytes.toString("utf8").trim() === `${expectedDigest}  ${filename}`;
}

function requireCondition(condition, code) {
  if (!condition) throw new Error(code);
}

function deepFreeze(value) {
  if (value && typeof value === "object" && !Object.isFrozen(value)) {
    Object.freeze(value);
    for (const child of Object.values(value)) deepFreeze(child);
  }
  return value;
}

function resolveOption(value, fallback) {
  if (!value) return fallback;
  return isAbsolute(value) ? pathToFileURL(value) : new URL(value, pathToFileURL(`${process.cwd()}/`));
}

export async function loadTrustedContext(options = {}) {
  const locations = {
    manifest: resolveOption(options.manifest, DEFAULTS.manifest),
    manifestSidecar: resolveOption(options.manifestSidecar, DEFAULTS.manifestSidecar),
    policy: resolveOption(options.policy, DEFAULTS.policy),
    policySidecar: resolveOption(options.policySidecar, DEFAULTS.policySidecar),
    classificationContract: resolveOption(options.classificationContract, DEFAULTS.classificationContract),
    classificationSidecar: resolveOption(options.classificationSidecar, DEFAULTS.classificationSidecar),
    authorizingEvidenceContract: resolveOption(options.authorizingEvidenceContract, DEFAULTS.authorizingEvidenceContract),
    authorizingEvidenceSidecar: resolveOption(options.authorizingEvidenceSidecar, DEFAULTS.authorizingEvidenceSidecar),
    authorizationSubjectProfile: resolveOption(options.authorizationSubjectProfile, DEFAULTS.authorizationSubjectProfile),
    authorizationSubjectProfileSidecar: resolveOption(options.authorizationSubjectProfileSidecar, DEFAULTS.authorizationSubjectProfileSidecar),
    inputSchema: resolveOption(options.inputSchema, DEFAULTS.inputSchema),
    outputSchema: resolveOption(options.outputSchema, DEFAULTS.outputSchema),
    cliErrorSchema: resolveOption(options.cliErrorSchema, DEFAULTS.cliErrorSchema),
    classificationSchema: resolveOption(options.classificationSchema, DEFAULTS.classificationSchema),
    authority: resolveOption(options.authority, DEFAULTS.authority),
  };
  let bytes;
  try {
    bytes = await Promise.all(Object.values(locations).map((location) => readFile(location)));
  } catch (error) {
    throw new Error(`custody.required-file-missing: ${error.message}`);
  }
  const [manifestBytes, manifestSidecarBytes, policyBytes, policySidecarBytes, classificationBytes, classificationSidecarBytes,
    authorizingEvidenceBytes, authorizingEvidenceSidecarBytes, authorizationSubjectProfileBytes, authorizationSubjectProfileSidecarBytes,
    inputBytes, outputBytes, cliErrorSchemaBytes, classificationSchemaBytes, authorityBytes] = bytes;
  requireCondition(rawSha256(manifestBytes) === TRUSTED_MANIFEST_SHA256, "custody.manifest-untrusted");
  requireCondition(sidecarMatches(manifestSidecarBytes, TRUSTED_MANIFEST_SHA256, "privacy-policy.v1.0.6.manifest.json"), "custody.manifest-sidecar-mismatch");
  requireCondition(rawSha256(policyBytes) === POLICY_RAW_SHA256, "custody.policy-bytes-mismatch");
  requireCondition(sidecarMatches(policySidecarBytes, POLICY_RAW_SHA256, "privacy-policy.v1.0.6.json"), "custody.policy-sidecar-mismatch");
  requireCondition(rawSha256(classificationBytes) === CLASSIFICATION_CONTRACT_RAW_SHA256, "custody.classification-contract-bytes-mismatch");
  requireCondition(sidecarMatches(classificationSidecarBytes, CLASSIFICATION_CONTRACT_RAW_SHA256, "privacy-policy.v1.0.2.classification.json"), "custody.classification-sidecar-mismatch");
  requireCondition(rawSha256(authorizingEvidenceBytes) === AUTHORIZING_EVIDENCE_RAW_SHA256, "custody.authorizing-evidence-bytes-mismatch");
  requireCondition(sidecarMatches(authorizingEvidenceSidecarBytes, AUTHORIZING_EVIDENCE_RAW_SHA256, "privacy-authorizing-evidence.v1.1.0.json"),
    "custody.authorizing-evidence-sidecar-mismatch");
  requireCondition(rawSha256(authorizationSubjectProfileBytes) === AUTHORIZATION_SUBJECT_PROFILE_RAW_SHA256,
    "custody.subject-profile-bytes-mismatch");
  requireCondition(sidecarMatches(authorizationSubjectProfileSidecarBytes, AUTHORIZATION_SUBJECT_PROFILE_RAW_SHA256,
    "privacy-authorization-subject-profile.v1.0.0.json"), "custody.subject-profile-sidecar-mismatch");
  requireCondition(rawSha256(inputBytes) === INPUT_SCHEMA_RAW_SHA256, "custody.input-schema-bytes-mismatch");
  requireCondition(rawSha256(outputBytes) === OUTPUT_SCHEMA_RAW_SHA256, "custody.output-schema-bytes-mismatch");
  requireCondition(rawSha256(cliErrorSchemaBytes) === CLI_ERROR_SCHEMA_RAW_SHA256, "custody.cli-error-schema-bytes-mismatch");
  requireCondition(rawSha256(classificationSchemaBytes) === CLASSIFICATION_SCHEMA_RAW_SHA256, "custody.classification-schema-bytes-mismatch");
  requireCondition(rawSha256(authorityBytes) === AUTHORITY_RAW_SHA256, "custody.authority-bytes-mismatch");

  const manifest = parseJson(manifestBytes, "manifest");
  const policy = parseJson(policyBytes, "policy");
  const classificationContract = parseJson(classificationBytes, "classification-contract");
  const authorizingEvidenceContract = parseJson(authorizingEvidenceBytes, "authorizing-evidence-contract");
  const authorizationSubjectProfile = parseJson(authorizationSubjectProfileBytes, "authorization-subject-profile");
  const inputSchema = parseJson(inputBytes, "input-schema");
  const outputSchema = parseJson(outputBytes, "output-schema");
  const cliErrorSchema = parseJson(cliErrorSchemaBytes, "cli-error-schema");
  const classificationSchema = parseJson(classificationSchemaBytes, "classification-schema");
  const authority = parseJson(authorityBytes, "authority");
  requireCondition(sameObject(policy.policyRef, POLICY_REF), "custody.policy-ref-mismatch");
  requireCondition(policyIdentity(policy) === POLICY_REF.digest, "custody.policy-identity-mismatch");
  requireCondition(policy.lifecycle?.status === "accepted" && policy.lifecycle?.accepted === true, "custody.policy-not-accepted");
  requireCondition(sameObject(policy.authorityRef, AUTHORITY_REF), "custody.policy-authority-ref-mismatch");
  requireCondition(sameObject(policy.classificationContractRef, CLASSIFICATION_CONTRACT_REF), "custody.policy-classification-ref-mismatch");
  requireCondition(sameObject(policy.authorizingEvidenceContractRef, AUTHORIZING_EVIDENCE_CONTRACT_REF), "custody.policy-authorizing-evidence-ref-mismatch");
  requireCondition(sameObject(policy.authorizationSubjectProfileRef, AUTHORIZATION_SUBJECT_PROFILE_REF), "custody.policy-subject-profile-ref-mismatch");
  requireCondition(classificationContract.contractId === CLASSIFICATION_CONTRACT_REF.contractId
    && classificationContract.version === CLASSIFICATION_CONTRACT_REF.version && classificationContract.status === "accepted", "custody.classification-contract-ref-mismatch");
  requireCondition(authorizingEvidenceContract.contractId === AUTHORIZING_EVIDENCE_CONTRACT_REF.contractId
    && authorizingEvidenceContract.version === AUTHORIZING_EVIDENCE_CONTRACT_REF.version
    && authorizingEvidenceContract.status === "accepted" && authorizingEvidenceContract.accepted === true
    && Array.isArray(authorizingEvidenceContract.registry) && authorizingEvidenceContract.registry.length === 9,
  "custody.authorizing-evidence-contract-ref-mismatch");
  requireCondition(authorizationSubjectProfile.profileId === AUTHORIZATION_SUBJECT_PROFILE_REF.profileId
    && authorizationSubjectProfile.version === AUTHORIZATION_SUBJECT_PROFILE_REF.version
    && authorizationSubjectProfile.status === "accepted" && authorizationSubjectProfile.accepted === true,
  "custody.subject-profile-ref-mismatch");
  requireCondition(authority.contractId === AUTHORITY_REF.contractId && authority.version === AUTHORITY_REF.version, "custody.authority-ref-mismatch");
  const accepted = manifest.acceptedContracts?.[0];
  requireCondition(manifest.status === "accepted" && manifest.accepted === true
    && manifest.acceptedContracts?.length === 1 && accepted?.status === "accepted" && accepted?.accepted === true
    && sameObject(accepted?.policyRef, POLICY_REF) && accepted?.policyFileDigest === `sha256:${POLICY_RAW_SHA256}`
    && sameObject(manifest.currentAcceptedRef, POLICY_REF), "custody.manifest-policy-mismatch");
  requireCondition(manifest.yankedCandidates?.length === 5 && manifest.yankedCandidates[0]?.accepted === false
    && manifest.yankedCandidates[0]?.policyRef?.version === "1.0.1"
    && manifest.yankedCandidates[0]?.policyRef?.digest === "sha256:f8faab0908fb1bc2da8c173969921000d6424a1f900ab0b04f2b57fa058d0ffb"
    && manifest.yankedCandidates[0]?.policyFileDigest === "sha256:462df7a5c92b676deda7d315e9304695f3563295ffa41775246341fb99970622"
    && manifest.yankedCandidates[1]?.accepted === false
    && manifest.yankedCandidates[1]?.policyRef?.version === "1.0.2"
    && manifest.yankedCandidates[1]?.policyRef?.digest === "sha256:207117a6a064c2341d95087b208b8dbc7f0953be08eb8c59b5da7eb905e25be1"
    && manifest.yankedCandidates[1]?.policyFileDigest === "sha256:de8f7495087d8b2890ed00efddc448f99563f32c68e753b43dc73646ab7e5719"
    && manifest.yankedCandidates[1]?.manifestFileDigest === "sha256:5739d80bde351b85c6ba8b7eedff1ef25f562c6649a7a1364027d4a308b4d530"
    && manifest.yankedCandidates[2]?.accepted === false
    && manifest.yankedCandidates[2]?.policyRef?.version === "1.0.3"
    && manifest.yankedCandidates[2]?.policyRef?.digest === "sha256:868ced73748caa4ad80a74df0e5d6ad8d3c5463a6593caa4ead09d872160280f"
    && manifest.yankedCandidates[2]?.policyFileDigest === "sha256:9179ced3d5d9c07f2fb9ed5bb7c40c5c5c1e072bb65eb077229fb040c55cac1c"
    && manifest.yankedCandidates[2]?.manifestFileDigest === "sha256:fe58cfc9fe3323b5fdb0d9be9f1610c50681e376e7fe9529812f117e80b5f112"
    && manifest.yankedCandidates[3]?.accepted === false
    && manifest.yankedCandidates[3]?.policyRef?.version === "1.0.4"
    && manifest.yankedCandidates[3]?.policyRef?.digest === "sha256:259cf596fcdc38423fa45d1df0937e85591569621c481197f3940f894bbc4ce5"
    && manifest.yankedCandidates[3]?.policyFileDigest === "sha256:76702466ddd1d54f1c542e73b63995de7dcc81861bc641914e4bf862623bbed1"
    && manifest.yankedCandidates[3]?.manifestFileDigest === "sha256:82ad6a16080dbe7fe8555d6726312f7123a6e0682d50d4ed30d0ce95d4fdc026"
    && manifest.yankedCandidates[4]?.accepted === false
    && manifest.yankedCandidates[4]?.policyRef?.version === "1.0.5"
    && manifest.yankedCandidates[4]?.policyRef?.digest === "sha256:bebd0631c2b978cd2a8264877525f5047354781e26cfe60cbc3b9d9f2aefa87a"
    && manifest.yankedCandidates[4]?.policyFileDigest === "sha256:59052a176eb61d6a4dd7676f0f73938404a69d37825f83bcc3566f6f9330e9e3"
    && manifest.yankedCandidates[4]?.manifestFileDigest === "sha256:7db5aecafce8d13966299d8071bf9c5eac17d02069cf208cceee160ef6b9768a",
  "custody.yanked-candidate-mismatch");
  requireCondition(sameObject(manifest.authorityRef, AUTHORITY_REF), "custody.manifest-authority-mismatch");
  requireCondition(accepted?.classificationContractRef?.digest === CLASSIFICATION_CONTRACT_REF.digest
    && accepted?.authorizingEvidenceContractRef?.digest === AUTHORIZING_EVIDENCE_CONTRACT_REF.digest
    && accepted?.authorizingEvidenceContractRef?.contractId === AUTHORIZING_EVIDENCE_CONTRACT_REF.contractId
    && accepted?.authorizingEvidenceContractRef?.version === AUTHORIZING_EVIDENCE_CONTRACT_REF.version
    && accepted?.authorizationSubjectProfileRef?.digest === AUTHORIZATION_SUBJECT_PROFILE_REF.digest
    && accepted?.authorizationSubjectProfileRef?.profileId === AUTHORIZATION_SUBJECT_PROFILE_REF.profileId
    && accepted?.authorizationSubjectProfileRef?.version === AUTHORIZATION_SUBJECT_PROFILE_REF.version
    && accepted?.inputSchemaRef?.digest === `sha256:${INPUT_SCHEMA_RAW_SHA256}`
    && accepted?.outputSchemaRef?.digest === `sha256:${OUTPUT_SCHEMA_RAW_SHA256}`
    && accepted?.cliErrorSchemaRef?.digest === `sha256:${CLI_ERROR_SCHEMA_RAW_SHA256}`
    && accepted?.classificationSchemaRef?.digest === `sha256:${CLASSIFICATION_SCHEMA_RAW_SHA256}`
    && accepted?.inputSchemaRef?.schemaId === INPUT_SCHEMA_REF.schemaId && accepted?.inputSchemaRef?.version === INPUT_SCHEMA_REF.version
    && accepted?.outputSchemaRef?.schemaId === OUTPUT_SCHEMA_REF.schemaId && accepted?.outputSchemaRef?.version === OUTPUT_SCHEMA_REF.version
    && accepted?.cliErrorSchemaRef?.schemaId === CLI_ERROR_SCHEMA_REF.schemaId && accepted?.cliErrorSchemaRef?.version === CLI_ERROR_SCHEMA_REF.version
    && accepted?.classificationSchemaRef?.schemaId === CLASSIFICATION_SCHEMA_REF.schemaId && accepted?.classificationSchemaRef?.version === CLASSIFICATION_SCHEMA_REF.version
    && sameObject(accepted?.decisionContractRef, DECISION_REF),
  "custody.manifest-schema-mismatch");
  requireCondition(inputSchema.schemaId === INPUT_SCHEMA_REF.schemaId && inputSchema.version === INPUT_SCHEMA_REF.version, "custody.input-schema-ref-mismatch");
  requireCondition(outputSchema.schemaId === OUTPUT_SCHEMA_REF.schemaId && outputSchema.version === OUTPUT_SCHEMA_REF.version, "custody.output-schema-ref-mismatch");
  requireCondition(cliErrorSchema.schemaId === CLI_ERROR_SCHEMA_REF.schemaId && cliErrorSchema.version === CLI_ERROR_SCHEMA_REF.version,
    "custody.cli-error-schema-ref-mismatch");
  requireCondition(outputSchema.properties?.effectiveRefs?.properties?.classificationContractRef?.properties?.version?.const
      === CLASSIFICATION_CONTRACT_REF.version
    && outputSchema.properties?.effectiveRefs?.properties?.classificationContractRef?.properties?.digest?.const
      === CLASSIFICATION_CONTRACT_REF.digest
    && outputSchema.properties?.effectiveRefs?.properties?.authorizingEvidenceContractRef?.properties?.version?.const
      === AUTHORIZING_EVIDENCE_CONTRACT_REF.version
    && outputSchema.properties?.effectiveRefs?.properties?.authorizingEvidenceContractRef?.properties?.digest?.const
      === AUTHORIZING_EVIDENCE_CONTRACT_REF.digest,
  "custody.output-schema-effective-ref-mismatch");
  requireCondition(classificationSchema.schemaId === CLASSIFICATION_SCHEMA_REF.schemaId && classificationSchema.version === CLASSIFICATION_SCHEMA_REF.version,
    "custody.classification-schema-ref-mismatch");
  requireCondition(inputSchema.lifecycle?.accepted === true && outputSchema.lifecycle?.accepted === true
    && cliErrorSchema.lifecycle?.accepted === true && cliErrorSchema.lifecycle?.exportDecisionOutput === false
    && classificationSchema.lifecycle?.accepted === true, "custody.schema-not-accepted");
  requireCondition(sameObject(authorizingEvidenceContract.authorizationSubjectProfileRef, AUTHORIZATION_SUBJECT_PROFILE_REF),
    "custody.evidence-subject-profile-ref-mismatch");
  const schemaRootProperties = Object.keys(inputSchema.properties ?? {}).sort(compareUnicodeCodePoints);
  const profileRootProperties = [...(authorizationSubjectProfile.inputSchemaRootProperties ?? [])].sort(compareUnicodeCodePoints);
  requireCondition(canonical(schemaRootProperties) === canonical(profileRootProperties)
    && schemaRootProperties.length === new Set(schemaRootProperties).size
    && profileRootProperties.length === new Set(profileRootProperties).size,
  "custody.subject-profile-root-inventory-mismatch");
  requireCondition(Array.isArray(authorizationSubjectProfile.decisionSemanticInventory)
    && authorizationSubjectProfile.decisionSemanticInventory.length > 0
    && new Set(authorizationSubjectProfile.decisionSemanticInventory.map((entry) => entry.id)).size
      === authorizationSubjectProfile.decisionSemanticInventory.length,
  "custody.subject-profile-semantic-inventory-mismatch");

  const authorityKinds = authority.artifactKinds?.map((entry) => entry.id);
  requireCondition(Array.isArray(authorityKinds) && authorityKinds.length === 49 && new Set(authorityKinds).size === 49, "custody.authority-registry-not-closed-exact");
  const defaults = policy.artifactDefaults;
  requireCondition(Array.isArray(defaults) && defaults.length === 49, "custody.policy-default-count");
  requireCondition(defaults.every((entry, index) => exactKeys(entry, ["artifactKind", "exportDisposition"])
    && entry.artifactKind === authorityKinds[index]
    && policy.vocabularies.exportDisposition.includes(entry.exportDisposition)), "custody.policy-default-registry-mismatch");
  requireCondition(new Set(defaults.map((entry) => entry.artifactKind)).size === 49, "custody.policy-default-duplicate");
  requireCondition(policy.constraintPolicy?.localVocabularyOverridesAllowed === false
    && Array.isArray(policy.constraintPolicy?.reviewedBroadeningGrants)
    && policy.constraintPolicy.reviewedBroadeningGrants.length === 0, "custody.local-override-or-grant-present");
  requireCondition(policy.repositoryIdentityPolicy?.declaredTokenCoherenceOnly === true
    && policy.repositoryIdentityPolicy?.callerFabricatedEqualityProvesPhysicalIdentity === false
    && policy.repositoryIdentityPolicy?.bindingAttestationInDecisionInput === false
    && canonical(policy.repositoryIdentityPolicy?.trustedBinding) === canonical({
      ownedByIssues: [119, 89],
      actors: ["runtime-envelope", "integration-adapter"],
      source: "physically-resolved-repository-context",
      requirements: ["mint", "bind", "verify-freshness"],
      missingStaleOrUnverified: "fail-closed-before-evaluator-invocation-or-decision-acceptance",
      physicalChecks: ["root-containment", "symlink", "junction", "reparse-point", "real-8.3-alias", "toctou"],
    }), "custody.repository-binding-seam-mismatch");
  requireCondition(policy.authorizingEvidencePolicy?.genericAuditRefAuthorizingUse === "forbidden"
    && policy.authorizingEvidencePolicy?.broadeningGrantInput === "null-only"
    && policy.authorizingEvidencePolicy?.trustedVerificationBoundary?.callerDeclarationProvesAuthenticity === false
    && policy.authorizingEvidencePolicy?.trustedVerificationBoundary?.physicalOrCryptographicAuthenticityCheckedByPolicy120 === false,
  "custody.authorizing-evidence-policy-mismatch");
  return deepFreeze({
    manifest,
    policy,
    classificationContract,
    authorizingEvidenceContract,
    authorizationSubjectProfile,
    inputSchema,
    outputSchema,
    cliErrorSchema,
    classificationSchema,
    authority,
    kindIds: authorityKinds,
    defaults: Object.fromEntries(defaults.map((entry) => [entry.artifactKind, entry.exportDisposition])),
  });
}

function auditRef(value) {
  return exactKeys(value, ["id", "version", "evidenceDigest"])
    && typeof value.id === "string" && value.id.length > 0
    && VERSION.test(value.version) && DIGEST.test(value.evidenceDigest);
}

function nullableAuditRef(value) {
  return value === null || auditRef(value);
}

function authorizingEvidenceBindingShape(value) {
  return exactKeys(value, ["subjectProfileRef", "subjectDigest"])
    && sameObject(value.subjectProfileRef, AUTHORIZATION_SUBJECT_PROFILE_REF)
    && typeof value.subjectDigest === "string" && SUBJECT_DIGEST.test(value.subjectDigest);
}

function authorizingEvidenceShape(value, spec, context) {
  const contract = context.authorizingEvidenceContract;
  return exactKeys(value, ["contractId", "version", "digest", "evidenceKind", "purpose", "outcome", "evidenceId", "verificationState", "freshnessState", "binding"])
    && value.contractId === AUTHORIZING_EVIDENCE_CONTRACT_REF.contractId
    && value.version === AUTHORIZING_EVIDENCE_CONTRACT_REF.version
    && value.digest === AUTHORIZING_EVIDENCE_CONTRACT_REF.digest
    && value.evidenceKind === spec.evidenceKind
    && value.purpose === spec.purpose
    && spec.outcomes.includes(value.outcome)
    && typeof value.evidenceId === "string" && EVIDENCE_REF.test(value.evidenceId)
    && contract.vocabularies.verificationState.includes(value.verificationState)
    && contract.vocabularies.freshnessState.includes(value.freshnessState)
    && authorizingEvidenceBindingShape(value.binding);
}

function nullableAuthorizingEvidence(value, spec, context) {
  return value === null || authorizingEvidenceShape(value, spec, context);
}

function refOrNull(value, expected) {
  return sameObject(value, expected);
}

function pathShape(value) {
  if (!record(value) || !["known", "unknown", "withheld", "unsupported"].includes(value.state)) return false;
  return value.state === "known"
    ? exactKeys(value, ["state", "value"]) && typeof value.value === "string" && value.value.length > 0
    : exactKeys(value, ["state"]);
}

function valueStateShape(value) {
  if (!record(value) || !["known", "unknown", "withheld", "unsupported"].includes(value.state)) return false;
  if (value.state !== "known") return exactKeys(value, ["state"]);
  return exactKeys(value, ["state", "value"])
    && (value.value === null || ["number", "string", "boolean"].includes(typeof value.value))
    && !(typeof value.value === "number" && !Number.isFinite(value.value));
}

function classificationShape(value, policy) {
  return Array.isArray(value) && value.length > 0 && uniqueKnown(value, policy.vocabularies.dataSensitivity);
}

function repositoryRefShape(value) {
  return value === null || (typeof value === "string" && REPOSITORY_REF.test(value));
}

function exactClassificationRef(value) {
  return exactKeys(value, ["contractId", "version", "digest", "decisionId", "evidenceDigest"])
    && value.contractId === CLASSIFICATION_CONTRACT_REF.contractId
    && value.version === CLASSIFICATION_CONTRACT_REF.version
    && value.digest === CLASSIFICATION_CONTRACT_REF.digest
    && CLASSIFICATION_DECISION_ID.test(value.decisionId)
    && DIGEST.test(value.evidenceDigest);
}

function sourceArtifactShape(value, context) {
  const { policy } = context;
  return exactKeys(value, ["sourceRef", "artifactKind", "authorityRef", "policyRef", "classificationRef", "dataSensitivity", "exportDisposition"])
    && typeof value.sourceRef === "string" && SOURCE_REF.test(value.sourceRef)
    && typeof value.artifactKind === "string" && value.artifactKind.length > 0
    && sameObject(value.authorityRef, AUTHORITY_REF) && sameObject(value.policyRef, POLICY_REF) && exactClassificationRef(value.classificationRef)
    && classificationShape(value.dataSensitivity, policy)
    && policy.vocabularies.exportDisposition.includes(value.exportDisposition);
}

function transformShape(value, policy) {
  return exactKeys(value, ["transformId", "version", "evidenceDigest"])
    && policy.vocabularies.transform.includes(value.transformId)
    && value.version === policy.contractVersions.transformVocabulary
    && DIGEST.test(value.evidenceDigest);
}

function decisionShape(value, type, context) {
  const { policy } = context;
  const required = type === "aggregation"
    ? ["policyRef", "decisionRef", "version", "outcome", "removesSourceRows", "removesSourceIdentities"]
    : ["policyRef", "decisionRef", "version", "outcome", "removedSensitivities"];
  return exactKeys(value, required) && sameObject(value.policyRef, POLICY_REF)
    && authorizingEvidenceShape(value.decisionRef, AUTHORIZING_EVIDENCE_SPECS[type], context)
    && value.version === "1.0.0"
    && ["approved", "rejected"].includes(value.outcome)
    && (type !== "aggregation" || (value.removesSourceRows === true && value.removesSourceIdentities === true))
    && (type !== "declassification" || uniqueKnown(value.removedSensitivities, policy.vocabularies.dataSensitivity));
}

function derivedShape(value, context) {
  if (!exactKeys(value, ["sourceArtifacts", "appliedTransforms", "declassificationDecision", "aggregationDecision", "containsSourceRows", "containsSourceIdentities", "reevaluated"])) return false;
  return Array.isArray(value.sourceArtifacts) && value.sourceArtifacts.length > 0 && value.sourceArtifacts.every((source) => sourceArtifactShape(source, context))
    && new Set(value.sourceArtifacts.map((source) => canonical(source))).size === value.sourceArtifacts.length
    && Array.isArray(value.appliedTransforms) && value.appliedTransforms.length > 0 && value.appliedTransforms.every((transform) => transformShape(transform, context.policy))
    && new Set(value.appliedTransforms.map((transform) => canonical(transform))).size === value.appliedTransforms.length
    && (value.declassificationDecision === null || decisionShape(value.declassificationDecision, "declassification", context))
    && (value.aggregationDecision === null || decisionShape(value.aggregationDecision, "aggregation", context))
    && typeof value.containsSourceRows === "boolean" && typeof value.containsSourceIdentities === "boolean" && typeof value.reevaluated === "boolean";
}

export function validateDecisionInput(input, context) {
  const { policy } = context;
  const required = ["decisionContractRef", "authorityRef", "policyRef", "authorizingEvidenceContractRef", "authorizationSubjectProfileRef", "artifactKind", "artifactRef", "operation", "source", "destination", "audience", "dataSensitivity", "exportDisposition", "provenance", "resourcePath", "conflictResolution", "constraints", "broadeningGrant"];
  if (!exactKeys(input, required, ["valueState", "derivedArtifact"])) return "input.shape";
  if (!refOrNull(input.decisionContractRef, DECISION_REF)) return "input.decision-ref";
  if (!refOrNull(input.authorityRef, AUTHORITY_REF)) return "input.authority-ref";
  if (!refOrNull(input.policyRef, POLICY_REF)) return "input.policy-ref";
  if (!refOrNull(input.authorizingEvidenceContractRef, AUTHORIZING_EVIDENCE_CONTRACT_REF)) return "input.authorizing-evidence-ref";
  if (!refOrNull(input.authorizationSubjectProfileRef, AUTHORIZATION_SUBJECT_PROFILE_REF)) return "input.authorization-subject-profile-ref";
  if (typeof input.artifactKind !== "string" || input.artifactKind.length === 0) return "input.artifact-kind";
  if (typeof input.artifactRef !== "string" || !ARTIFACT_REF.test(input.artifactRef)) return "input.artifact-ref";
  if (!exactKeys(input.operation, ["id", "version"]) || !policy.vocabularies.operation.includes(input.operation.id) || input.operation.version !== policy.contractVersions.operationVocabulary) return "input.operation";
  if (!exactKeys(input.source, ["repositoryRole", "repositoryRef"]) || !policy.vocabularies.repositoryRole.includes(input.source.repositoryRole)
    || !repositoryRefShape(input.source.repositoryRef)) return "input.source";
  if (!exactKeys(input.destination, ["repositoryRole", "repositoryRef", "trustBoundary", "repositoryRelation", "tenantRelation"])
    || !policy.vocabularies.repositoryRole.includes(input.destination.repositoryRole)
    || !repositoryRefShape(input.destination.repositoryRef)
    || !policy.vocabularies.trustBoundary.includes(input.destination.trustBoundary)
    || !policy.vocabularies.repositoryRelation.includes(input.destination.repositoryRelation)
    || !policy.vocabularies.tenantRelation.includes(input.destination.tenantRelation)) return "input.destination";
  if (!policy.vocabularies.audience.includes(input.audience)) return "input.audience";
  if (!classificationShape(input.dataSensitivity, policy)) return "input.data-sensitivity";
  if (!policy.vocabularies.exportDisposition.includes(input.exportDisposition)) return "input.export-disposition";
  if (!exactKeys(input.provenance, ["origin", "repositoryRole", "repositoryRef", "synthetic", "derived", "classificationRef", "publicFixturePermissionRef", "publicFixtureLicenseRef", "publicFixtureConsentRef", "consumerAclPermissionRef", "consumerRepositoryConsentRef", "exportTransferConsentRef"])
    || !["synthetic", "consumer-repository", "lekalo-repository", "tool-runtime", "derived", "external"].includes(input.provenance.origin)
    || !policy.vocabularies.repositoryRole.includes(input.provenance.repositoryRole)
    || !repositoryRefShape(input.provenance.repositoryRef)
    || typeof input.provenance.synthetic !== "boolean" || typeof input.provenance.derived !== "boolean"
    || !exactClassificationRef(input.provenance.classificationRef)
    || !nullableAuthorizingEvidence(input.provenance.publicFixturePermissionRef, AUTHORIZING_EVIDENCE_SPECS.publicFixturePermissionRef, context)
    || !nullableAuthorizingEvidence(input.provenance.publicFixtureLicenseRef, AUTHORIZING_EVIDENCE_SPECS.publicFixtureLicenseRef, context)
    || !nullableAuthorizingEvidence(input.provenance.publicFixtureConsentRef, AUTHORIZING_EVIDENCE_SPECS.publicFixtureConsentRef, context)
    || !nullableAuthorizingEvidence(input.provenance.consumerAclPermissionRef, AUTHORIZING_EVIDENCE_SPECS.consumerAclPermissionRef, context)
    || !nullableAuthorizingEvidence(input.provenance.consumerRepositoryConsentRef, AUTHORIZING_EVIDENCE_SPECS.consumerRepositoryConsentRef, context)
    || !nullableAuthorizingEvidence(input.provenance.exportTransferConsentRef, AUTHORIZING_EVIDENCE_SPECS.exportTransferConsentRef, context)) return "input.provenance";
  if (!pathShape(input.resourcePath)) return "input.resource-path";
  if (Object.hasOwn(input, "valueState") && !valueStateShape(input.valueState)) return "input.value-state";
  if (!exactKeys(input.conflictResolution, ["state", "decisionRef"])
    || !policy.vocabularies.conflictState.includes(input.conflictResolution.state)
    || !nullableAuthorizingEvidence(input.conflictResolution.decisionRef, AUTHORIZING_EVIDENCE_SPECS.conflictResolution, context)) return "input.conflict-resolution";
  if (!Array.isArray(input.constraints) || input.constraints.some((constraint) => !exactKeys(constraint, ["scope", "constraintRef", "allowedOperations", "allowedTrustBoundaries", "allowedAudiences"])
    || !policy.vocabularies.constraintScope.includes(constraint.scope) || !auditRef(constraint.constraintRef)
    || !uniqueKnown(constraint.allowedOperations, policy.vocabularies.operation, { nonempty: false })
    || !uniqueKnown(constraint.allowedTrustBoundaries, policy.vocabularies.trustBoundary, { nonempty: false })
    || !uniqueKnown(constraint.allowedAudiences, policy.vocabularies.audience, { nonempty: false }))
    || new Set(input.constraints.map((constraint) => canonical(constraint))).size !== input.constraints.length) return "input.constraints";
  if (input.broadeningGrant !== null) return "input.broadening-grant";
  if (Object.hasOwn(input, "derivedArtifact") && !derivedShape(input.derivedArtifact, context)) return "input.derived-artifact";
  return null;
}

function result(decision, reasonCodes, requiredTransforms = [], derivedArtifactRequirements = [], sourceTransferAllowed = false) {
  return {
    decision,
    reasonCodes,
    requiredTransforms,
    effectiveRefs: {
      decisionContractRef: { ...DECISION_REF },
      authorityRef: { ...AUTHORITY_REF },
      policyRef: { ...POLICY_REF },
      classificationContractRef: { ...CLASSIFICATION_CONTRACT_REF },
      authorizingEvidenceContractRef: { ...AUTHORIZING_EVIDENCE_CONTRACT_REF },
      authorizationSubjectProfileRef: { ...AUTHORIZATION_SUBJECT_PROFILE_REF },
      inputSchemaRef: { ...INPUT_SCHEMA_REF },
      outputSchemaRef: { ...OUTPUT_SCHEMA_REF },
    },
    derivedArtifactRequirements,
    sourceTransferAllowed,
  };
}

function deny(code) {
  return result("deny", [code]);
}

function invalidPath(path) {
  if (path.normalize("NFC") !== path || /[\u0000-\u001f\u007f]/.test(path)) return true;
  if (path.startsWith("/") || path.startsWith("//") || path.startsWith("~") || path.includes("\\") || /^[A-Za-z]:/.test(path) || /^[A-Za-z][A-Za-z0-9+.-]*:/.test(path) || /%[0-9A-Fa-f]{2}/.test(path)) return true;
  const segments = path.split("/");
  return segments.some((segment) => segment === "" || segment === "." || segment === ".." || /[ .]$/.test(segment)
    || segment.includes(":") || DOS_DEVICE.test(segment.normalize("NFKC")) || /~\d/.test(segment));
}

function dispositionBaseline(rule, operation) {
  return rule.allowOperations.includes(operation) || rule.transformOperations.includes(operation);
}

function repositoryBackedRole(role) {
  return !["local-workspace", "public-channel"].includes(role);
}

function contextCoherence(input) {
  const { source, destination, provenance, audience } = input;
  if (provenance.repositoryRole !== source.repositoryRole || provenance.repositoryRef !== source.repositoryRef) {
    return "provenance.source-repository-mismatch";
  }
  if (source.repositoryRole === "public-channel"
    || (repositoryBackedRole(source.repositoryRole) !== (source.repositoryRef !== null))) return "repository.source-ref-role-mismatch";
  if (repositoryBackedRole(destination.repositoryRole) !== (destination.repositoryRef !== null)) return "repository.destination-ref-role-mismatch";

  const ordinaryOrigin = provenance.synthetic === false && provenance.derived === false;
  if (provenance.origin === "synthetic" && !(provenance.synthetic === true && provenance.derived === false
    && source.repositoryRole === "local-workspace" && source.repositoryRef === null)) return "provenance.origin-boolean-conflict";
  if (provenance.origin === "derived" && !(provenance.derived === true && provenance.synthetic === false)) {
    return "provenance.origin-boolean-conflict";
  }
  if (!["synthetic", "derived"].includes(provenance.origin) && !ordinaryOrigin) return "provenance.origin-boolean-conflict";
  if (provenance.origin === "consumer-repository" && source.repositoryRole !== "consumer-repository") return "provenance.origin-role-conflict";
  if (provenance.origin === "lekalo-repository" && source.repositoryRole !== "lekalo-repository") return "provenance.origin-role-conflict";
  if (provenance.origin === "external" && source.repositoryRole !== "external-repository") return "provenance.origin-role-conflict";

  switch (destination.trustBoundary) {
    case "same-local-workspace":
      if (destination.repositoryRole !== "local-workspace" || destination.repositoryRef !== null
        || destination.repositoryRelation !== "not-applicable" || destination.tenantRelation !== "same-tenant" || audience !== "operator-only") {
        return "repository.local-context-contradiction";
      }
      break;
    case "same-repository":
      if (!repositoryBackedRole(source.repositoryRole) || source.repositoryRef === null
        || destination.repositoryRole !== source.repositoryRole || destination.repositoryRef !== source.repositoryRef
        || destination.repositoryRelation !== "same-origin" || destination.tenantRelation !== "same-tenant") {
        return "repository.same-origin-contradiction";
      }
      break;
    case "same-tenant":
    case "cross-repository":
      if (source.repositoryRef === null || destination.repositoryRef === null || source.repositoryRef === destination.repositoryRef
        || destination.repositoryRole !== "external-repository" || destination.repositoryRelation !== "different-repository"
        || destination.tenantRelation !== "same-tenant") return "repository.different-origin-contradiction";
      break;
    case "cross-tenant":
      if (source.repositoryRef === null || destination.repositoryRef === null || source.repositoryRef === destination.repositoryRef
        || destination.repositoryRole !== "external-repository" || destination.repositoryRelation !== "different-repository"
        || destination.tenantRelation !== "cross-tenant") return "repository.cross-tenant-contradiction";
      break;
    case "public":
      if (destination.repositoryRole !== "public-channel" || destination.repositoryRef !== null
        || destination.repositoryRelation !== "not-applicable" || destination.tenantRelation !== "not-applicable" || audience !== "public") {
        return "repository.public-context-contradiction";
      }
      break;
    default:
      return "repository.unknown-context";
  }
  return null;
}

function authorizingEvidenceEntries(input) {
  const entries = Object.entries(AUTHORIZING_EVIDENCE_SPECS)
    .filter(([name]) => Object.hasOwn(input.provenance, name))
    .map(([name, spec]) => ({ name: `provenance.${name}`, evidence: input.provenance[name], spec }));
  entries.push({ name: "conflictResolution.decisionRef", evidence: input.conflictResolution.decisionRef,
    spec: AUTHORIZING_EVIDENCE_SPECS.conflictResolution });
  if (input.derivedArtifact?.declassificationDecision) entries.push({
    name: "derivedArtifact.declassificationDecision.decisionRef",
    evidence: input.derivedArtifact.declassificationDecision.decisionRef,
    spec: AUTHORIZING_EVIDENCE_SPECS.declassification,
  });
  if (input.derivedArtifact?.aggregationDecision) entries.push({
    name: "derivedArtifact.aggregationDecision.decisionRef",
    evidence: input.derivedArtifact.aggregationDecision.decisionRef,
    spec: AUTHORIZING_EVIDENCE_SPECS.aggregation,
  });
  return entries;
}

function authorizingEvidenceSemanticError(input, context) {
  const entries = authorizingEvidenceEntries(input).filter((entry) => entry.evidence !== null);
  const evidenceIds = new Set();
  const expectedSubjectDigest = authorizationSubjectDigest(input, context.authorizationSubjectProfile);
  for (const entry of entries) {
    if (evidenceIds.has(entry.evidence.evidenceId)) return "evidence.identity-reused";
    evidenceIds.add(entry.evidence.evidenceId);
    if (entry.evidence.verificationState !== "verified") return "evidence.unverified";
    if (entry.evidence.freshnessState !== "current") return `evidence.${entry.evidence.freshnessState}`;
    if (!sameObject(entry.evidence.binding.subjectProfileRef, AUTHORIZATION_SUBJECT_PROFILE_REF)
      || entry.evidence.binding.subjectDigest !== expectedSubjectDigest) return "evidence.binding-mismatch";
    const expectedOutcome = entry.name === "conflictResolution.decisionRef"
      ? input.conflictResolution.state === "resolved-allow" ? "allow"
        : input.conflictResolution.state === "resolved-deny" ? "deny" : null
      : entry.spec.outcome;
    if (expectedOutcome !== null && entry.evidence.outcome !== expectedOutcome) return "evidence.outcome-mismatch";
  }
  return null;
}

function validateDerived(input, context) {
  const { derivedArtifact: derived, dataSensitivity, exportDisposition } = input;
  if (input.provenance.derived !== Boolean(derived)) return "derived.provenance-flag-mismatch";
  if (!derived) return exportDisposition === "public-aggregate" ? "derived.public-aggregate-required" : null;
  const transformIds = new Set();
  for (const transform of derived.appliedTransforms) {
    if (transformIds.has(transform.transformId)) return "derived.transform-id-conflict";
    transformIds.add(transform.transformId);
  }
  const sourceEntries = new Map();
  for (const source of derived.sourceArtifacts) {
    if (sourceEntries.has(source.sourceRef)) return "derived.source-ref-conflict";
    sourceEntries.set(source.sourceRef, source);
  }
  const sourceLabels = new Set();
  for (const source of [...derived.sourceArtifacts].sort((left, right) => left.sourceRef.localeCompare(right.sourceRef))) {
    if (!context.kindIds.includes(source.artifactKind)) return "derived.source-kind-unknown";
    if (!sameObject(source.authorityRef, AUTHORITY_REF)) return "derived.source-authority-ref-mismatch";
    if (!sameObject(source.policyRef, POLICY_REF)) return "derived.source-policy-ref-mismatch";
    if (source.exportDisposition !== context.defaults[source.artifactKind]) return "derived.source-disposition-mismatch";
    for (const label of source.dataSensitivity) sourceLabels.add(label);
  }
  const removed = [...sourceLabels].filter((label) => !dataSensitivity.includes(label));
  if (removed.length > 0) {
    const decision = derived.declassificationDecision;
    if (!decision) return "derived.declassification-required";
    if (!sameObject(decision.policyRef, POLICY_REF)) return "derived.declassification-policy-ref-mismatch";
    if (decision.version !== "1.0.0") return "derived.declassification-version-mismatch";
    if (decision.outcome !== "approved") return "derived.declassification-outcome-denied";
    const actual = [...decision.removedSensitivities].sort();
    if (actual.join("\n") !== removed.sort().join("\n")) return "derived.declassification-labels-mismatch";
  } else if (derived.declassificationDecision !== null) {
    return "derived.unnecessary-declassification";
  }
  if (derived.reevaluated !== true) return "derived.not-reevaluated";
  if (exportDisposition === "public-aggregate") {
    if (derived.containsSourceRows || derived.containsSourceIdentities) return "derived.aggregate-contains-source-data";
    if (!derived.appliedTransforms.some((entry) => entry.transformId === "aggregate-no-source-rows")) return "derived.aggregate-transform-missing";
    if (!derived.aggregationDecision) return "derived.aggregation-decision-required";
    if (!sameObject(derived.aggregationDecision.policyRef, POLICY_REF)) return "derived.aggregation-policy-ref-mismatch";
    if (derived.aggregationDecision.version !== "1.0.0") return "derived.aggregation-version-mismatch";
    if (derived.aggregationDecision.outcome !== "approved") return "derived.aggregation-outcome-denied";
  } else if (derived.aggregationDecision !== null) {
    return "derived.unexpected-aggregation-decision";
  }
  return null;
}

export function evaluateDecision(input, context) {
  const malformed = validateDecisionInput(input, context);
  if (malformed) return { malformed: true, output: deny(malformed) };
  const { policy } = context;
  const evidenceError = authorizingEvidenceSemanticError(input, context);
  if (evidenceError) return { malformed: false, output: deny(evidenceError) };
  const operation = input.operation.id;
  const destination = input.destination;
  const expectedDisposition = context.defaults[input.artifactKind];
  if (!expectedDisposition) return { malformed: false, output: deny("artifact-kind.unknown") };
  if (input.exportDisposition !== expectedDisposition) return { malformed: false, output: deny("classification.disposition-default-mismatch") };
  const coherenceError = contextCoherence(input);
  if (coherenceError) return { malformed: false, output: deny(coherenceError) };
  if (["resolved-allow", "resolved-deny"].includes(input.conflictResolution.state) && input.conflictResolution.decisionRef === null) {
    return { malformed: false, output: deny("conflict.missing-decision") };
  }
  if (["none", "unresolved"].includes(input.conflictResolution.state) && input.conflictResolution.decisionRef !== null) {
    return { malformed: false, output: deny("conflict.unexpected-decision") };
  }
  if (input.conflictResolution.state === "unresolved" || input.conflictResolution.state === "resolved-deny") return { malformed: false, output: deny("conflict.deny") };
  if (Object.hasOwn(input, "valueState") && input.valueState.state !== "known") return { malformed: false, output: deny(`value-state.${input.valueState.state}`) };
  if (input.resourcePath.state !== "known") {
    if (operation !== "local-use" || input.resourcePath.state !== "withheld") return { malformed: false, output: deny(`path.${input.resourcePath.state}`) };
  } else if (invalidPath(input.resourcePath.value)) return { malformed: false, output: deny("path.not-normalized-project-relative") };

  const operationProfile = policy.operationProfiles.find((entry) => entry.operation === operation);
  if (!operationProfile.allowedTrustBoundaries.includes(destination.trustBoundary)) return { malformed: false, output: deny("destination.operation-boundary-mismatch") };
  const destinationProfile = policy.destinationProfiles.find((entry) => entry.trustBoundary === destination.trustBoundary);
  if (!destinationProfile || !destinationProfile.repositoryRoles.includes(destination.repositoryRole)
    || !destinationProfile.repositoryRelations.includes(destination.repositoryRelation)
    || !destinationProfile.tenantRelations.includes(destination.tenantRelation)
    || !destinationProfile.audiences.includes(input.audience)) return { malformed: false, output: deny("destination.profile-mismatch") };

  const dispositionRule = policy.dispositionRules.find((entry) => entry.disposition === input.exportDisposition);
  if (input.exportDisposition === "forbidden-to-export" && operation !== "local-use") return { malformed: false, output: deny("disposition.forbidden-dominates") };
  if (!dispositionBaseline(dispositionRule, operation)) return { malformed: false, output: deny(`disposition.${input.exportDisposition}.operation-denied`) };
  if (input.exportDisposition === "local-private" && destination.trustBoundary !== "same-local-workspace") return { malformed: false, output: deny("disposition.local-private-boundary") };
  if (input.exportDisposition === "consumer-repository-only"
    && (input.source.repositoryRole !== "consumer-repository" || input.source.repositoryRef === null
      || input.provenance.origin !== "consumer-repository" || input.provenance.repositoryRole !== "consumer-repository"
      || input.provenance.repositoryRef !== input.source.repositoryRef)) {
    return { malformed: false, output: deny("disposition.consumer-origin-acl-only") };
  }
  if (input.exportDisposition === "consumer-repository-only" && operation === "repository-store"
    && (destination.trustBoundary !== "same-repository" || destination.repositoryRelation !== "same-origin" || destination.tenantRelation !== "same-tenant"
      || destination.repositoryRole !== "consumer-repository" || input.source.repositoryRef !== destination.repositoryRef
      || input.provenance.consumerAclPermissionRef === null || input.provenance.consumerRepositoryConsentRef === null)) {
    return { malformed: false, output: deny("disposition.consumer-origin-acl-only") };
  }

  const orderedLabels = [...input.dataSensitivity].sort((a, b) => policy.vocabularies.dataSensitivity.indexOf(a) - policy.vocabularies.dataSensitivity.indexOf(b));
  for (const label of orderedLabels) {
    const rule = policy.sensitivityRules.find((entry) => entry.label === label);
    if (!rule.allowedOperations.includes(operation) || !rule.allowedTrustBoundaries.includes(destination.trustBoundary)
      || !rule.allowedAudiences.includes(input.audience) || !rule.allowedRepositoryRelations.includes(destination.repositoryRelation)
      || !rule.allowedTenantRelations.includes(destination.tenantRelation)) return { malformed: false, output: deny(`sensitivity.${label}.denied`) };
  }

  const baselineOperations = new Set(dispositionRule.allowOperations.concat(dispositionRule.transformOperations));
  const baselineBoundaries = new Set(operationProfile.allowedTrustBoundaries);
  const baselineAudiences = new Set(destinationProfile.audiences);
  for (const constraint of input.constraints) {
    if (constraint.allowedOperations.some((value) => !baselineOperations.has(value))
      || constraint.allowedTrustBoundaries.some((value) => !baselineBoundaries.has(value))
      || constraint.allowedAudiences.some((value) => !baselineAudiences.has(value))) return { malformed: false, output: deny("constraint.broadening-forbidden") };
    if (!constraint.allowedOperations.includes(operation) || !constraint.allowedTrustBoundaries.includes(destination.trustBoundary)
      || !constraint.allowedAudiences.includes(input.audience)) return { malformed: false, output: deny("constraint.narrowed-deny") };
  }

  const derivedError = validateDerived(input, context);
  if (derivedError) return { malformed: false, output: deny(derivedError) };
  const exportLike = ["repository-store", "transfer", "publish"].includes(operation);
  const publicFixtureRefs = [input.provenance.publicFixturePermissionRef, input.provenance.publicFixtureLicenseRef,
    input.provenance.publicFixtureConsentRef];
  const consumerRefs = [input.provenance.consumerAclPermissionRef, input.provenance.consumerRepositoryConsentRef];
  if (input.exportDisposition !== "public-fixture" && publicFixtureRefs.some((value) => value !== null)) {
    return { malformed: false, output: deny("evidence.unexpected-public-fixture") };
  }
  if (!(input.exportDisposition === "consumer-repository-only" && operation === "repository-store")
    && consumerRefs.some((value) => value !== null)) return { malformed: false, output: deny("evidence.unexpected-consumer-repository") };
  const usesGeneralTransferConsent = exportLike
    && input.exportDisposition !== "public-fixture"
    && !(input.exportDisposition === "consumer-repository-only" && operation === "repository-store");
  if (usesGeneralTransferConsent && input.provenance.exportTransferConsentRef === null) {
    return { malformed: false, output: deny("provenance.consent-required") };
  }
  if (!usesGeneralTransferConsent && input.provenance.exportTransferConsentRef !== null) {
    return { malformed: false, output: deny("evidence.unexpected-export-consent") };
  }
  if (input.exportDisposition === "public-fixture" && input.provenance.origin === "synthetic"
    && publicFixtureRefs.some((value) => value !== null)) return { malformed: false, output: deny("evidence.unexpected-public-fixture") };
  if (input.exportDisposition === "public-fixture" && exportLike && input.provenance.origin !== "synthetic"
    && publicFixtureRefs.some((value) => value === null)) return { malformed: false, output: deny("fixture.permission-license-consent-required") };

  if (dispositionRule.transformOperations.includes(operation)) {
    const transforms = ["redact-content"];
    if (orderedLabels.includes("credential-secret")) transforms.push("redact-secrets");
    if (orderedLabels.includes("personal-pii")) transforms.push("redact-pii");
    return { malformed: false, output: result("transform-required", ["disposition.transform-required"], transforms, [...TRANSFORM_REQUIREMENTS], false) };
  }
  const sourceTransferAllowed = operationProfile.sourceTransfer === true && dispositionRule.sourceTransferAllowed === true;
  return { malformed: false, output: result("allow", ["policy.allow"], [], [], sourceTransferAllowed) };
}

function parseArgs(argv) {
  const result = {};
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    if (!["--decision", "--manifest", "--manifest-sidecar", "--policy", "--policy-sidecar", "--classification-contract", "--classification-sidecar",
      "--authorizing-evidence-contract", "--authorizing-evidence-sidecar",
      "--authorization-subject-profile", "--authorization-subject-profile-sidecar",
      "--input-schema", "--output-schema", "--cli-error-schema", "--classification-schema", "--authority"].includes(token) || !argv[index + 1]) {
      throw new Error(`usage.unknown-or-missing-option:${token}`);
    }
    result[token.slice(2).replace(/-([a-z])/g, (_, letter) => letter.toUpperCase())] = argv[index + 1];
    index += 1;
  }
  return result;
}

export async function main(argv = process.argv.slice(2)) {
  let options;
  try {
    options = parseArgs(argv);
    const context = await loadTrustedContext(options);
    if (!options.decision) {
      console.log(JSON.stringify({
        status: "valid",
        policyRef: POLICY_REF,
        authorityRef: AUTHORITY_REF,
        classificationContractRef: CLASSIFICATION_CONTRACT_REF,
        authorizingEvidenceContractRef: AUTHORIZING_EVIDENCE_CONTRACT_REF,
        authorizationSubjectProfileRef: AUTHORIZATION_SUBJECT_PROFILE_REF,
        decisionContractRef: DECISION_REF,
        inputSchemaRef: INPUT_SCHEMA_REF,
        outputSchemaRef: OUTPUT_SCHEMA_REF,
        cliErrorSchemaRef: CLI_ERROR_SCHEMA_REF,
        artifactKindCount: context.kindIds.length,
        policyLifecycle: context.policy.lifecycle.status,
        accepted: context.policy.lifecycle.accepted,
      }, null, 2));
      return 0;
    }
    const decisionBytes = await readFile(resolveOption(options.decision, null));
    const input = parseJson(decisionBytes, "decision");
    const evaluated = evaluateDecision(input, context);
    console.log(JSON.stringify(evaluated.output, null, 2));
    if (evaluated.malformed) return 1;
    return evaluated.output.decision === "allow" ? 0 : 3;
  } catch (error) {
    console.error(JSON.stringify({ status: "invalid", reasonCodes: [String(error.message)] }, null, 2));
    return 1;
  }
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.exitCode = await main();
}
