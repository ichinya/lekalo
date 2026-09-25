#!/usr/bin/env node
// Parity gate for the Rust privacy evaluator (issue #119, plan S3).
//
// The port of `evaluateDecision` in `crates/lekalo-core/src/privacy/`
// is faithful only if, for every decision vector the `test-privacy-*`
// gates pin, the Rust evaluator and this reference evaluator produce
// identical outcomes. This gate generates that corpus — the committed
// fixture files plus the same programmatic families as
// `test-privacy-decisions.mjs` (every trust boundary, operation,
// disposition, sensitivity intersection, conflict state, value state,
// path representation, derived-artifact rule, constraint narrowing,
// and the evidence attack families) — runs each vector through both
// engines, and requires identical `malformed`, `decision`,
// `reasonCodes`, `requiredTransforms`,
// `derivedArtifactRequirements`, `sourceTransferAllowed`, and
// `effectiveRefs`, plus the identical CLI exit contract.
//
// Fail-closed: any Rust invocation failure, missing binary, or
// mismatch fails the gate. No payload, secret, or absolute host path
// is embedded in any vector: the corpus is metadata-only.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, writeFileSync } from "node:fs";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { evaluateDecision, loadTrustedContext } from "./check-privacy.mjs";
import { authorizingEvidence, refreshEvidenceBindings, setProvenanceEvidence } from "./privacy-test-helpers.mjs";
const root = fileURLToPath(new URL("../", import.meta.url));
const clone = (value) => structuredClone(value);
const REPO_ONE = `repo-sha256:${"1".repeat(64)}`;
const REPO_TWO = `repo-sha256:${"2".repeat(64)}`;

// ---------------------------------------------------------------------------
// Corpus generation (the same families the JS gates pin).
// ---------------------------------------------------------------------------

async function fixtureVectors() {
  const vectors = [];
  for (const name of ["allowed", "forbidden", "transform-required", "ambiguous", "malformed"]) {
    const entries = JSON.parse(await readFile(`${root}/tests/fixtures/privacy/${name}.json`, "utf8"));
    for (const entry of entries) {
      vectors.push({ family: `fixture:${name}`, id: entry.id, decision: clone(entry.decision) });
    }
  }
  return vectors;
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

function localFor(base, kind, disposition, labels = ["public"]) {
  const input = destination(clone(base), "same-local-workspace");
  input.artifactKind = kind;
  input.exportDisposition = disposition;
  input.dataSensitivity = labels;
  return input;
}

async function programmaticVectors(context) {
  const base = JSON.parse(await readFile(`${root}/tests/fixtures/privacy/allowed.json`, "utf8"))[0].decision;
  const vectors = [];
  const add = (family, id, decision) => vectors.push({ family, id, decision });

  // Every trust boundary, every operation, every audience.
  for (const boundary of context.policy.vocabularies.trustBoundary) {
    add("boundaries", boundary, destination(clone(base), boundary));
  }
  for (const operation of context.policy.vocabularies.operation) {
    const input = clone(base);
    if (operation === "local-use" || operation === "derive") destination(input, "same-local-workspace");
    else if (operation === "repository-store") destination(input, "same-repository");
    else if (operation === "transfer") destination(input, "same-tenant");
    else destination(input, "public");
    input.operation.id = operation;
    add("operations", operation, input);
  }

  // Every sensitivity label locally; restrictive intersections deny.
  for (const label of context.policy.vocabularies.dataSensitivity) {
    add("sensitivities", label, localFor(base, "fixture", "public-fixture", [label]));
  }
  for (const [label, boundary] of [["credential-secret", "public"], ["personal-pii", "public"], ["tenant-scoped", "cross-tenant"], ["internal", "cross-repository"]]) {
    const input = destination(clone(base), boundary);
    input.dataSensitivity = ["public", label];
    add("sensitivity-intersections", `${label}-${boundary}`, input);
  }
  const labelsA = destination(clone(base), "public");
  labelsA.dataSensitivity = ["tenant-scoped", "public"];
  add("sensitivity-order", "ordered", labelsA);
  const duplicate = clone(base);
  duplicate.dataSensitivity = ["public", "public"];
  add("sensitivity-duplicates", "duplicate", duplicate);

  // Every disposition with its independent semantic vectors.
  add("dispositions", "local-private", localFor(base, "context.capsule", "local-private"));
  const localPrivateRepo = destination(localFor(base, "context.capsule", "local-private"), "same-repository");
  add("dispositions", "local-private-repository", localPrivateRepo);
  const consumerStore = destination(localFor(base, "consumer.model", "consumer-repository-only", ["internal"]), "same-repository");
  consumerStore.source.repositoryRole = "consumer-repository";
  consumerStore.source.repositoryRef = REPO_ONE;
  consumerStore.provenance.repositoryRole = "consumer-repository";
  consumerStore.provenance.repositoryRef = REPO_ONE;
  consumerStore.provenance.origin = "consumer-repository";
  consumerStore.provenance.synthetic = false;
  add("dispositions", "consumer-store", consumerStore);
  const consumerWrongOrigin = clone(consumerStore);
  consumerWrongOrigin.source.repositoryRole = "lekalo-repository";
  consumerWrongOrigin.provenance.repositoryRole = "lekalo-repository";
  consumerWrongOrigin.provenance.origin = "lekalo-repository";
  consumerWrongOrigin.destination.repositoryRole = "lekalo-repository";
  refreshEvidenceBindings(consumerWrongOrigin);
  add("dispositions", "consumer-wrong-origin", consumerWrongOrigin);
  const consumerCrossRepo = destination(clone(consumerStore), "cross-repository");
  add("dispositions", "consumer-cross-repository", consumerCrossRepo);
  add("dispositions", "shareable-local", localFor(base, "generated.summary", "shareable-with-redaction"));
  add("dispositions", "shareable-publish", destination(localFor(base, "generated.summary", "shareable-with-redaction"), "public"));
  add("dispositions", "forbidden-local", localFor(base, "ai.prompt", "forbidden-to-export", ["confidential"]));
  add("dispositions", "forbidden-publish", destination(localFor(base, "ai.prompt", "forbidden-to-export"), "public"));
  add("dispositions", "aggregate-missing-derived", destination(localFor(base, "aggregate.artifact", "public-aggregate"), "public"));

  // Repository/provenance coherence families.
  const contradictoryConsumerLekalo = clone(consumerStore);
  contradictoryConsumerLekalo.destination.repositoryRole = "lekalo-repository";
  add("coherence", "same-origin-role", contradictoryConsumerLekalo);
  const contradictorySameRepoRef = clone(consumerStore);
  contradictorySameRepoRef.destination.repositoryRef = REPO_TWO;
  add("coherence", "same-origin-ref", contradictorySameRepoRef);
  const contradictorySameRepoRelation = clone(consumerStore);
  contradictorySameRepoRelation.destination.repositoryRelation = "different-repository";
  add("coherence", "same-origin-relation", contradictorySameRepoRelation);
  const contradictoryDifferentRepoRef = destination(clone(base), "cross-repository");
  contradictoryDifferentRepoRef.destination.repositoryRef = contradictoryDifferentRepoRef.source.repositoryRef;
  add("coherence", "different-origin-ref", contradictoryDifferentRepoRef);
  const contradictoryTenant = destination(clone(base), "cross-tenant");
  contradictoryTenant.destination.tenantRelation = "same-tenant";
  add("coherence", "cross-tenant-relation", contradictoryTenant);
  const contradictoryLocalRef = destination(clone(base), "same-local-workspace");
  contradictoryLocalRef.destination.repositoryRef = REPO_TWO;
  add("coherence", "local-ref", contradictoryLocalRef);
  const provenanceRoleMismatch = clone(consumerStore);
  provenanceRoleMismatch.provenance.repositoryRole = "lekalo-repository";
  add("coherence", "provenance-role", provenanceRoleMismatch);
  const provenanceRefMismatch = clone(consumerStore);
  provenanceRefMismatch.provenance.repositoryRef = REPO_TWO;
  add("coherence", "provenance-ref", provenanceRefMismatch);
  const syntheticBooleanConflict = clone(base);
  syntheticBooleanConflict.provenance.synthetic = false;
  add("coherence", "synthetic-boolean", syntheticBooleanConflict);
  const derivedBooleanConflict = clone(base);
  derivedBooleanConflict.provenance.origin = "derived";
  add("coherence", "derived-boolean", derivedBooleanConflict);
  const originRoleConflict = clone(consumerStore);
  originRoleConflict.provenance.origin = "lekalo-repository";
  add("coherence", "origin-role", originRoleConflict);

  // Conflict, consent, fixture permission, and exact-ref families.
  const unresolved = clone(base);
  unresolved.conflictResolution = { state: "unresolved", decisionRef: null };
  add("conflicts", "unresolved", unresolved);
  const resolvedDeny = clone(base);
  resolvedDeny.conflictResolution = { state: "resolved-deny", decisionRef: authorizingEvidence(resolvedDeny, "conflictResolution", "a", { outcome: "deny" }) };
  refreshEvidenceBindings(resolvedDeny);
  add("conflicts", "resolved-deny", resolvedDeny);
  const missingConsent = destination(localFor(base, "generated.summary", "shareable-with-redaction"), "public");
  missingConsent.provenance.exportTransferConsentRef = null;
  add("consent", "missing-transfer-consent", missingConsent);
  const unexpectedConsent = clone(base);
  unexpectedConsent.provenance.exportTransferConsentRef = authorizingEvidence(unexpectedConsent, "exportTransferConsentRef", "9");
  refreshEvidenceBindings(unexpectedConsent);
  add("consent", "unexpected-transfer-consent", unexpectedConsent);
  const nonSynthetic = clone(base);
  nonSynthetic.provenance.synthetic = false;
  nonSynthetic.provenance.origin = "external";
  nonSynthetic.source = { repositoryRole: "external-repository", repositoryRef: REPO_ONE };
  nonSynthetic.provenance.repositoryRole = "external-repository";
  nonSynthetic.provenance.repositoryRef = REPO_ONE;
  refreshEvidenceBindings(nonSynthetic);
  add("fixtures", "non-synthetic-without-evidence", nonSynthetic);
  const dispositionMismatch = clone(base);
  dispositionMismatch.exportDisposition = "local-private";
  add("fixtures", "disposition-mismatch", dispositionMismatch);
  for (const [refName, code] of [
    ["policyRef", "input.policy-ref"],
    ["authorityRef", "input.authority-ref"],
    ["decisionContractRef", "input.decision-ref"],
  ]) {
    const input = clone(base);
    input[refName].version = "0.0.0";
    add("exact-refs", refName, input);
  }

  // Evidence semantic attacks on the public-fixture publish path.
  const nonSyntheticPublish = destination(clone(base), "public");
  nonSyntheticPublish.provenance.synthetic = false;
  nonSyntheticPublish.provenance.origin = "external";
  nonSyntheticPublish.source = { repositoryRole: "external-repository", repositoryRef: REPO_ONE };
  nonSyntheticPublish.provenance.repositoryRole = "external-repository";
  nonSyntheticPublish.provenance.repositoryRef = REPO_ONE;
  setProvenanceEvidence(nonSyntheticPublish, "publicFixturePermissionRef", "4");
  setProvenanceEvidence(nonSyntheticPublish, "publicFixtureLicenseRef", "5");
  setProvenanceEvidence(nonSyntheticPublish, "publicFixtureConsentRef", "6");
  add("evidence", "non-synthetic-with-evidence", nonSyntheticPublish);
  const stale = clone(nonSyntheticPublish);
  stale.provenance.publicFixturePermissionRef.freshnessState = "stale";
  add("evidence", "stale", stale);
  const expired = clone(nonSyntheticPublish);
  expired.provenance.publicFixtureConsentRef.freshnessState = "expired";
  add("evidence", "expired", expired);
  const wrongOutcome = clone(nonSyntheticPublish);
  wrongOutcome.provenance.publicFixtureLicenseRef.outcome = "unlicensed";
  add("evidence", "wrong-outcome", wrongOutcome);
  const reusedIdentity = clone(nonSyntheticPublish);
  reusedIdentity.provenance.publicFixtureConsentRef.evidenceId = reusedIdentity.provenance.publicFixturePermissionRef.evidenceId;
  add("evidence", "identity-reused", reusedIdentity);
  const wrongPurpose = clone(nonSyntheticPublish);
  wrongPurpose.provenance.publicFixturePermissionRef.purpose = "authorize-public-fixture-consent";
  add("evidence", "wrong-purpose", wrongPurpose);
  const wrongKind = clone(nonSyntheticPublish);
  wrongKind.provenance.publicFixturePermissionRef.evidenceKind = "consumer-acl-permission";
  add("evidence", "wrong-kind", wrongKind);
  const syntheticWithEvidence = clone(base);
  for (const [field, character] of [["publicFixturePermissionRef", "4"], ["publicFixtureLicenseRef", "5"], ["publicFixtureConsentRef", "6"]]) {
    setProvenanceEvidence(syntheticWithEvidence, field, character);
  }
  add("evidence", "synthetic-with-evidence", syntheticWithEvidence);
  const unexpectedAcl = clone(base);
  unexpectedAcl.provenance.consumerAclPermissionRef = authorizingEvidence(unexpectedAcl, "consumerAclPermissionRef", "7");
  refreshEvidenceBindings(unexpectedAcl);
  add("evidence", "unexpected-acl", unexpectedAcl);

  // Path representation: valid POSIX, withheld-local, and the
  // adversarial Windows/URI/traversal/DOS-device forms.
  const withheldLocal = localFor(base, "fixture", "public-fixture");
  withheldLocal.resourcePath = { state: "withheld" };
  add("paths", "withheld-local", withheldLocal);
  for (const state of ["unknown", "unsupported"]) {
    const input = localFor(base, "fixture", "public-fixture");
    input.resourcePath = { state };
    add("paths", state, input);
  }
  const unsafePaths = [
    "C:/private/file.txt", "//server/share/file.txt", "/absolute/file.txt", "~/secret", "file:///private/file.txt",
    "a/../secret", "a\\secret", "a/%2e%2e/secret", "a/./b", "a//b", "a/trailing. ", "a/file:stream",
    "a/CON.txt", "a/PROGRA~1/file", `a/${"e\u0301"}.txt`,
    "a/COM¹.txt", "a/com²", "a/CoM³.bin", "a/LPT¹.txt", "a/lpt²", "a/LpT³.bin",
    "a/CONIN$", "a/conin$.txt", "a/CONOUT$", "a/conout$.log", "a/CLOCK$", "a/clock$.txt",
  ];
  for (const [index, path] of unsafePaths.entries()) {
    const input = clone(base);
    input.resourcePath = { state: "known", value: path };
    add("paths", `unsafe-${index}`, input);
  }
  const privateIdentity = clone(base);
  privateIdentity.source = { repositoryRole: "consumer-repository", repositoryUrl: "https://private.example/repo" };
  add("paths", "private-repository-url", privateIdentity);
  const localAlias = clone(base);
  localAlias.source.repositoryRole = "my-consumer-repo";
  add("paths", "local-alias-role", localAlias);

  // Value states: known zero is data; the state-only alternatives deny.
  const zero = clone(base);
  zero.valueState = { state: "known", value: 0 };
  add("value-states", "known-zero", zero);
  const zeroString = clone(base);
  zeroString.valueState = { state: "known", value: "0" };
  add("value-states", "known-string", zeroString);
  const zeroNull = clone(base);
  zeroNull.valueState = { state: "known", value: null };
  add("value-states", "known-null", zeroNull);
  for (const state of ["unknown", "withheld", "unsupported"]) {
    const input = clone(base);
    input.valueState = { state };
    add("value-states", state, input);
  }
  const illegalUnknownValue = clone(base);
  illegalUnknownValue.valueState = { state: "unknown", value: 0 };
  add("value-states", "unknown-with-value", illegalUnknownValue);

  // Constraints: exact narrowing passes; narrowing to nothing denies;
  // no caller-supplied broadening or grant survives.
  const narrowedAllow = clone(base);
  narrowedAllow.constraints = [{
    scope: "operation",
    constraintRef: { id: "constraint.operation", version: "0.2.16", evidenceDigest: `sha256:${"a".repeat(64)}` },
    allowedOperations: ["publish"],
    allowedTrustBoundaries: ["public"],
    allowedAudiences: ["public"],
  }];
  add("constraints", "narrowed-allow", narrowedAllow);
  const narrowedDeny = clone(narrowedAllow);
  narrowedDeny.constraints[0].allowedOperations = [];
  add("constraints", "narrowed-deny", narrowedDeny);
  const localBroadening = clone(narrowedAllow);
  localBroadening.constraints[0].allowedTrustBoundaries.push("same-repository");
  add("constraints", "local-broadening", localBroadening);
  const sensitivityBroadening = localFor(base, "metrics.evaluation-evidence", "shareable-with-redaction", ["internal"]);
  sensitivityBroadening.constraints = [{
    scope: "profile",
    constraintRef: { id: "constraint.sensitivity", version: "0.2.16", evidenceDigest: `sha256:${"b".repeat(64)}` },
    allowedOperations: ["local-use", "publish"],
    allowedTrustBoundaries: ["same-local-workspace"],
    allowedAudiences: ["operator-only"],
  }];
  add("constraints", "sensitivity-broadening", sensitivityBroadening);
  const callerGrant = clone(base);
  callerGrant.broadeningGrant = {
    grantId: "caller.local-grant", version: "0.2.16", policyRef: clone(context.policy.policyRef), reviewRef: { id: "caller.review", version: "0.2.16", evidenceDigest: `sha256:${"c".repeat(64)}` },
  };
  add("constraints", "caller-grant", callerGrant);

  // Shape attacks: unknown member, missing member, duplicate
  // constraints, and a wrong-typed member.
  const extraMember = clone(base);
  extraMember.unexpected = true;
  add("shape", "extra-member", extraMember);
  const missingMember = clone(base);
  delete missingMember.resourcePath;
  add("shape", "missing-member", missingMember);
  const duplicatedConstraint = clone(narrowedAllow);
  duplicatedConstraint.constraints.push(clone(duplicatedConstraint.constraints[0]));
  add("shape", "duplicate-constraint", duplicatedConstraint);
  const wrongTyped = clone(base);
  wrongTyped.audience = 42;
  add("shape", "wrong-typed-audience", wrongTyped);

  return vectors;
}

// ---------------------------------------------------------------------------
// The two engines.
// ---------------------------------------------------------------------------

const BINARY_CANDIDATES = [
  join(root, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo"),
  join(root, "target", "release", process.platform === "win32" ? "lekalo.exe" : "lekalo"),
];

function findBinary() {
  for (const candidate of BINARY_CANDIDATES) {
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

function buildBinary() {
  const build = spawnSync("cargo", ["build", "-p", "lekalo-cli", "--locked"], {
    cwd: root, encoding: "utf8", windowsHide: true,
  });
  if (build.status !== 0) {
    console.error(build.stderr || build.stdout);
  }
  return build.status === 0 ? findBinary() : null;
}

function rustEvaluate(binary, temp, index, decision) {
  const path = join(temp, `vector-${index}.json`);
  // Synchronous write is fine here: the corpus is metadata-only and
  // small, and the temp directory is gate-private.
  writeFileSync(path, JSON.stringify(decision));
  const run = spawnSync(binary, ["privacy", "evaluate", "--decision", path], {
    cwd: root, encoding: "utf8", windowsHide: true, maxBuffer: 16 * 1024 * 1024,
  });
  return { run, path };
}

function jsExitCode(malformed, decision) {
  if (malformed) return 1;
  if (decision === "allow") return 0;
  return 3;
}

// ---------------------------------------------------------------------------
// The gate.
// ---------------------------------------------------------------------------

const context = await loadTrustedContext();
const vectors = [
  ...(await fixtureVectors()),
  ...(await programmaticVectors(context)),
];
const binary = findBinary() ?? buildBinary();
assert.ok(binary, "the lekalo CLI binary must be buildable for the parity gate");

const temp = await mkdtemp(join(tmpdir(), "lekalo-privacy-parity-"));
let compared = 0;
try {
  for (const [index, vector] of vectors.entries()) {
    const input = refreshEvidenceBindings(vector.decision);
    const reference = evaluateDecision(input, context);
    const { run } = rustEvaluate(binary, temp, index, input);

    assert.equal(run.error, undefined, `${vector.family}/${vector.id}: the Rust evaluator must spawn`);
    assert.equal(run.signal, null, `${vector.family}/${vector.id}: the Rust evaluator must not crash`);
    assert.ok(run.stdout.trim().length > 0, `${vector.family}/${vector.id}: stdout must carry the decision`);

    const rustOutput = JSON.parse(run.stdout);
    assert.equal(rustOutput.decision, reference.output.decision,
      `${vector.family}/${vector.id}: decision`);
    assert.deepEqual(rustOutput.reasonCodes, reference.output.reasonCodes,
      `${vector.family}/${vector.id}: reasonCodes`);
    assert.deepEqual(rustOutput.requiredTransforms, reference.output.requiredTransforms,
      `${vector.family}/${vector.id}: requiredTransforms`);
    assert.deepEqual(rustOutput.derivedArtifactRequirements, reference.output.derivedArtifactRequirements,
      `${vector.family}/${vector.id}: derivedArtifactRequirements`);
    assert.equal(rustOutput.sourceTransferAllowed, reference.output.sourceTransferAllowed,
      `${vector.family}/${vector.id}: sourceTransferAllowed`);
    // The effective refs must be the exact frozen references.
    assert.equal(rustOutput.effectiveRefs.policyRef.policyId, context.policy.policyRef.policyId);
    assert.equal(rustOutput.effectiveRefs.policyRef.version, context.policy.policyRef.version);
    assert.equal(rustOutput.effectiveRefs.policyRef.digest, context.policy.policyRef.digest);
    assert.equal(rustOutput.effectiveRefs.authorityRef.digest, context.policy.authorityRef.digest);
    assert.equal(
      rustOutput.effectiveRefs.classificationContractRef.digest,
      context.policy.classificationContractRef.digest,
    );
    assert.equal(
      rustOutput.effectiveRefs.authorizingEvidenceContractRef.digest,
      context.policy.authorizingEvidenceContractRef.digest,
    );
    assert.equal(
      rustOutput.effectiveRefs.authorizationSubjectProfileRef.digest,
      context.policy.authorizationSubjectProfileRef.digest,
    );

    // The exit contract must match the reference protocol exactly.
    const expectedExit = jsExitCode(reference.malformed, reference.output.decision);
    assert.equal(run.status, expectedExit,
      `${vector.family}/${vector.id}: exit code (stderr: ${run.stderr})`);

    compared += 1;
  }
} finally {
  await rm(temp, { recursive: true, force: true });
}

console.log(JSON.stringify({
  ok: true,
  compared,
  binary: process.platform === "win32" ? "target/[debug|release]/lekalo.exe" : "target/[debug|release]/lekalo",
}));
