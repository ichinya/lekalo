#!/usr/bin/env node

import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const checker = resolve(root, "scripts/check-authority.mjs");
const baselinePath = resolve(root, "contracts/authority-matrix.v1.2.0.json");
const successorPath = resolve(root, "contracts/authority-matrix.v1.3.1.json");
const yankedPath = resolve(root, "contracts/authority-matrix.v1.3.0.json");
const baselineBytes = readFileSync(baselinePath);
const successorBytes = readFileSync(successorPath);
const yankedBytes = readFileSync(yankedPath);
const baseline = JSON.parse(baselineBytes.toString("utf8"));
const successor = JSON.parse(successorBytes.toString("utf8"));
const profiles = {
  "1.2.0": {
    path: baselinePath,
    digest: "3446ce25ce33397f8c49426144a4c4dbf8c8ea23e4c759ceca70557606ffeda2"
  },
  "1.3.1": {
    path: successorPath,
    digest: "5c96ed68fe27956512b6de37e0fa23d223e4430d391b6a5869b12d2a8cb522d3"
  }
};
const yankedProfile = {
  version: "1.3.0",
  path: yankedPath,
  digest: "50a4b9c8533644bd61841e695fc042e4ac52307952dcccb37bf6f2d3ab3a3211"
};
const requiredAddedKindIds = [
  "authority.contract",
  "authority.contract-manifest",
  "privacy.policy",
  "privacy.export-schema",
  "diagnostics.raw-tool-output",
  "context.capsule",
  "trace.manifest",
  "ai.prompt",
  "ai.response",
  "ai.tool-transcript",
  "metrics.evaluation-evidence",
  "fixture",
  "repository.identity",
  "native.symbol-identity",
  "native.source-map",
  "consumer.model",
  "consumer.target-bindings",
  "export.artifact",
  "export.decision",
  "redaction.artifact",
  "redaction.decision",
  "aggregate.artifact",
  "aggregate.decision"
];
const probeOperation = {
  action: "write",
  actor: "source-native",
  target: {
    artifactKind: "source-native.source-code",
    path: "openspec/changes/mutation-probe/implementation.ts"
  }
};
const tempRoot = mkdtempSync(resolve(tmpdir(), "lekalo-authority-contracts-"));

function fail(message) {
  throw new Error(message);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function ref(version, digest = profiles[version]?.digest) {
  return `dev.lekalo.authority-matrix@${version}@sha256:${digest}`;
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function writeBytes(name, bytes) {
  const path = resolve(tempRoot, name);
  writeFileSync(path, bytes);
  return path;
}

function writeJson(name, value) {
  return writeBytes(name, Buffer.from(`${JSON.stringify(value, null, 2)}\n`, "utf8"));
}

function run(args) {
  return spawnSync(process.execPath, [checker, ...args], {
    cwd: root,
    encoding: "utf8",
    windowsHide: true
  });
}

function runOperation(version, operation) {
  const operationPath = writeJson(`operation-${version.replaceAll(".", "-")}-${Math.random()}.json`, operation);
  return run(["--contract-version", version, "--operation", operationPath]);
}

function assertDecision(id, result, expectedExit, expectedCode) {
  if (result.status !== expectedExit) {
    fail(`${id}: expected exit ${expectedExit}, got ${result.status}; stderr=${result.stderr.trim()}; stdout=${result.stdout.trim()}`);
  }
  const output = JSON.parse(result.stdout);
  if (output.code !== expectedCode) fail(`${id}: expected ${expectedCode}, got ${result.stdout.trim()}`);
  process.stdout.write(`PASS ${id}: exit ${expectedExit}, ${expectedCode}\n`);
}

function assertContractFailure(id, result) {
  if (result.status !== 1) {
    fail(`${id}: expected contract exit 1, got ${result.status}; stderr=${result.stderr.trim()}; stdout=${result.stdout.trim()}`);
  }
  if (!result.stderr.includes("authority contract: FAIL:")) {
    fail(`${id}: missing contract diagnostic: ${result.stderr.trim()}`);
  }
  if (result.stdout.trim() !== "") fail(`${id}: operation was evaluated: ${result.stdout.trim()}`);
  process.stdout.write(`PASS ${id}: exit 1 before operation evaluation\n`);
}

function runCustomContract(id, bytes, authorityRef) {
  const contractPath = writeBytes(`${id}.json`, bytes);
  const operationPath = writeJson(`${id}-operation.json`, probeOperation);
  return run(["--contract", contractPath, "--authority-ref", authorityRef, "--operation", operationPath]);
}

function verifyRegistry() {
  const baselineIds = baseline.artifactKinds.map((kind) => kind.id);
  const successorIds = successor.artifactKinds.map((kind) => kind.id);
  if (new Set(baselineIds).size !== baselineIds.length) fail("baseline registry IDs are not unique");
  if (new Set(successorIds).size !== successorIds.length) fail("successor registry IDs are not unique");
  if (successorIds.length !== 49) fail(`successor registry expected 49 kinds, got ${successorIds.length}`);
  if (JSON.stringify(successorIds.slice(0, baselineIds.length)) !== JSON.stringify(baselineIds)) {
    fail("successor did not preserve all baseline stable IDs in order");
  }
  if (JSON.stringify(successor.migration.privacyHandoffAddedKindIdsFromAcceptedBaseline) !== JSON.stringify(requiredAddedKindIds)) {
    fail("successor added-kind migration registry is incomplete or reordered");
  }
  if (JSON.stringify(successorIds.slice(baselineIds.length)) !== JSON.stringify(requiredAddedKindIds)) {
    fail("successor registry additions do not match migration metadata");
  }

  const requiredFields = ["id", "canonicalOwner", "classification", "allowedPaths", "allowedReaders", "allowedWriters"];
  for (const kind of successor.artifactKinds) {
    for (const field of requiredFields) {
      if (!(field in kind)) fail(`${kind.id} is missing ${field}`);
    }
    if (typeof kind.canonicalOwner !== "string") fail(`${kind.id} has no scalar owner`);
    for (const field of ["allowedPaths", "allowedReaders", "allowedWriters"]) {
      if (!Array.isArray(kind[field]) || kind[field].length === 0) fail(`${kind.id} has incomplete ${field}`);
      if (new Set(kind[field]).size !== kind[field].length) fail(`${kind.id} duplicates ${field}`);
    }
    if ("dataSensitivity" in kind || "exportDisposition" in kind) {
      fail(`${kind.id} improperly contains privacy classification`);
    }
  }

  for (let index = 0; index < baseline.artifactKinds.length; index += 1) {
    const oldKind = baseline.artifactKinds[index];
    const newKind = successor.artifactKinds[index];
    for (const field of ["id", "canonicalOwner", "classification", "allowedPaths", "allowedWriters"]) {
      if (JSON.stringify(oldKind[field]) !== JSON.stringify(newKind[field])) {
        fail(`${oldKind.id} changed stable field ${field}`);
      }
    }
    if (JSON.stringify(newKind.allowedReaders) !== JSON.stringify(successor.actors)) {
      fail(`${oldKind.id} did not preserve unrestricted 1.2.0 read behavior`);
    }
  }
  for (const kindId of [
    "source-native.source-code", "source-native.native-test", "fixture",
    "repository.identity", "native.symbol-identity", "native.source-map"
  ]) {
    const kind = successor.artifactKinds.find((candidate) => candidate.id === kindId);
    if (JSON.stringify(kind.allowedWriters) !== JSON.stringify(["source-native"])) {
      fail(`${kindId} is not source-native-only writable`);
    }
  }
  const metrics = successor.artifactKinds.find((kind) => kind.id === "metrics.evaluation-evidence");
  if (JSON.stringify(metrics.allowedWriters) !== JSON.stringify(["hlv"])) {
    fail("metrics.evaluation-evidence is not HLV-only writable");
  }
  process.stdout.write("PASS registry-conformance: 49 unique full kinds; 26 stable IDs/semantics preserved; 23 additions complete\n");
}

function verifyCustodyFiles() {
  const manifest = JSON.parse(readFileSync(resolve(root, "contracts/authority-contracts.manifest.json"), "utf8"));
  if (!Array.isArray(manifest.acceptedContracts) || manifest.acceptedContracts.length !== 2) {
    fail("manifest must list exactly baseline and successor");
  }
  for (const [version, profile] of Object.entries(profiles)) {
    const entry = manifest.acceptedContracts.find((candidate) => candidate.version === version);
    if (!entry || entry.contractId !== "dev.lekalo.authority-matrix" || entry.digest !== `sha256:${profile.digest}`) {
      fail(`manifest exact triple missing for ${version}`);
    }
    const sidecar = readFileSync(resolve(root, entry.sidecar), "utf8");
    const expectedName = version === "1.2.0" ? "authority-matrix.v1.2.0.json" : "authority-matrix.v1.3.1.json";
    if (sidecar !== `${profile.digest}  ${expectedName}\n`) fail(`${version} sidecar bytes are not exact`);
  }
  if (!Array.isArray(manifest.rejectedContracts) || manifest.rejectedContracts.length !== 1) {
    fail("manifest must preserve exactly one rejected candidate");
  }
  const rejected = manifest.rejectedContracts[0];
  if (
    rejected.version !== yankedProfile.version ||
    rejected.digest !== `sha256:${yankedProfile.digest}` ||
    rejected.status !== "rejected-yanked-candidate" ||
    rejected.replacement?.version !== "1.3.1" ||
    sha256(yankedBytes) !== yankedProfile.digest
  ) {
    fail("manifest does not preserve exact yanked 1.3.0 lifecycle and replacement");
  }
  const rejectedSidecar = readFileSync(resolve(root, rejected.sidecar), "utf8");
  if (rejectedSidecar !== `${yankedProfile.digest}  authority-matrix.v1.3.0.json\n`) {
    fail("yanked 1.3.0 sidecar bytes changed");
  }
  if (!baselineBytes.equals(readFileSync(resolve(root, "contracts/authority-matrix.v1.json")))) {
    fail("historical authority-matrix.v1.json no longer equals the immutable 1.2.0 bytes");
  }
  if (
    manifest.currentAuthorityRef.contractId !== "dev.lekalo.authority-matrix" ||
    manifest.currentAuthorityRef.version !== "1.3.1" ||
    manifest.currentAuthorityRef.digest !== `sha256:${profiles["1.3.1"].digest}`
  ) {
    fail("manifest currentAuthorityRef is not the exact reviewed successor");
  }
  process.stdout.write("PASS exact-custody-files: accepted baseline/current, yanked candidate, sidecars, alias, and lifecycle agree\n");
}

try {
  for (const version of Object.keys(profiles)) {
    if (sha256(readFileSync(profiles[version].path)) !== profiles[version].digest) {
      fail(`${version} exact byte digest changed`);
    }
    const suite = run(["--contract-version", version]);
    if (suite.status !== 0) fail(`${version} fixture suite failed: ${suite.stderr.trim()}`);
    process.stdout.write(`PASS exact-${version}: digest and full fixture suite accepted\n`);
    assertDecision(`boundary-${version}`, runOperation(version, probeOperation), 3, "target.boundary-owner-mismatch");
  }

  const exactBaselineCopy = runCustomContract("exact-baseline-copy", baselineBytes, ref("1.2.0"));
  assertDecision("exact-baseline-copy", exactBaselineCopy, 3, "target.boundary-owner-mismatch");
  const exactSuccessorCopy = runCustomContract("exact-successor-copy", successorBytes, ref("1.3.1"));
  assertDecision("exact-successor-copy", exactSuccessorCopy, 3, "target.boundary-owner-mismatch");
  assertContractFailure(
    "yanked-1.3.0-exact-triple-rejected",
    runCustomContract("yanked-1.3.0", yankedBytes, ref("1.3.0", yankedProfile.digest))
  );
  assertContractFailure(
    "successor-bytes-with-stale-baseline-ref",
    runCustomContract("successor-with-baseline-ref", successorBytes, ref("1.2.0"))
  );
  assertContractFailure(
    "baseline-bytes-with-successor-ref",
    runCustomContract("baseline-with-successor-ref", baselineBytes, ref("1.3.1"))
  );

  verifyRegistry();
  verifyCustodyFiles();

  const semanticCopyBytes = Buffer.from(`${JSON.stringify(Object.fromEntries(Object.entries(clone(successor)).reverse()), null, 4)}\n`, "utf8");
  assertContractFailure(
    "byte-mutation-recomputed-digest",
    runCustomContract("byte-mutation-recomputed", semanticCopyBytes, ref("1.3.1", sha256(semanticCopyBytes)))
  );

  const ownerMutation = clone(successor);
  ownerMutation.artifactKinds.find((kind) => kind.id === "openspec.requirement").canonicalOwner = "source-native";
  const ownerMutationBytes = Buffer.from(`${JSON.stringify(ownerMutation, null, 2)}\n`, "utf8");
  assertContractFailure(
    "semantic-mutation-recomputed-digest",
    runCustomContract("semantic-mutation-recomputed", ownerMutationBytes, ref("1.3.1", sha256(ownerMutationBytes)))
  );
  assertContractFailure(
    "semantic-mutation-stale-exact-ref",
    runCustomContract("semantic-mutation-stale-ref", ownerMutationBytes, ref("1.3.1"))
  );

  const closedProfileMutations = [
    ["path-boundaries-empty", (value) => { value.pathBoundaries = []; }],
    ["path-boundaries-missing", (value) => { delete value.pathBoundaries; }],
    ["conditional-boundaries-empty", (value) => { value.conditionalPathBoundaries = []; }],
    ["conditional-boundaries-missing", (value) => { delete value.conditionalPathBoundaries; }],
    ["protected-owner-changed", (value) => { value.pathBoundaries[1].owner = "source-native"; }],
    ["conditional-owner-changed", (value) => { value.conditionalPathBoundaries[0].owner = "source-native"; }],
    ["conditional-kind-changed", (value) => { value.conditionalPathBoundaries[0].artifactKind = "source-native.source-code"; }],
    ["conditional-pattern-changed", (value) => { value.conditionalPathBoundaries[0].pattern = "hlv-project.yaml"; }],
    ["conditional-context-key-changed", (value) => { value.conditionalPathBoundaries[0].requiredContext = "detected"; }],
    ["conditional-required-value-changed", (value) => { value.conditionalPathBoundaries[0].requiredValue = false; }],
    ["conditional-entry-duplicated", (value) => { value.conditionalPathBoundaries.push(clone(value.conditionalPathBoundaries[0])); }],
    ["protected-entry-duplicated", (value) => { value.pathBoundaries.push(clone(value.pathBoundaries[0])); }],
    ["protected-entry-conflicts", (value) => { value.pathBoundaries.push({ pattern: "openspec/changes/**", owner: "source-native" }); }],
    ["protected-case-alias", (value) => { value.pathBoundaries[1].pattern = "OpenSpec/changes/**"; }],
    ["protected-shadow-wildcard", (value) => { value.pathBoundaries.push({ pattern: "openspec/**", owner: "openspec" }); }],
    ["non-conflicting-extension", (value) => { value.pathBoundaries.push({ pattern: "docs/**", owner: "lekalo" }); }],
    ["boundary-kind-binding-missing", (value) => { delete value.pathBoundaries.find((item) => item.pattern === "tests/fixtures/**").artifactKinds; }],
    ["boundary-kind-binding-empty", (value) => { value.pathBoundaries.find((item) => item.pattern === "tests/fixtures/**").artifactKinds = []; }],
    ["boundary-kind-binding-weakened", (value) => { value.pathBoundaries.find((item) => item.pattern === "tests/fixtures/**").artifactKinds.push("generated.code"); }],
    ["boundary-writer-binding-missing", (value) => { delete value.pathBoundaries.find((item) => item.pattern === "tests/fixtures/**").writers; }],
    ["boundary-writer-binding-weakened", (value) => { value.pathBoundaries.find((item) => item.pattern === "tests/fixtures/**").writers.push("lekalo"); }],
    ["boundary-reader-binding-missing", (value) => { delete value.pathBoundaries.find((item) => item.pattern === "tests/fixtures/**").readers; }],
    ["equal-specificity-conflicting-policy", (value) => {
      const conflict = clone(value.pathBoundaries.find((item) => item.pattern === "tests/fixtures/**"));
      conflict.pattern = "Tests/fixtures/**";
      conflict.artifactKinds = ["generated.code"];
      value.pathBoundaries.push(conflict);
    }],
    ["equal-specificity-duplicate-policy", (value) => {
      value.pathBoundaries.push(clone(value.pathBoundaries.find((item) => item.pattern === "tests/fixtures/**")));
    }],
    ["specificity-order-reversed", (value) => { value.boundaryPolicy.specificityOrder.reverse(); }],
    ["specificity-resolution-weakened", (value) => { value.boundaryPolicy.resolution = "first-match"; }],
    ["metrics-specific-boundary-removed", (value) => {
      value.pathBoundaries = value.pathBoundaries.filter((item) => item.pattern !== ".hlv/evidence/metrics/**");
    }],
    ["source-map-boundary-removed", (value) => {
      value.pathBoundaries = value.pathBoundaries.filter((item) => item.pattern !== "**/*.map");
    }],
    ["compatibility-extension-enabled", (value) => { value.compatibility.extensionsAllowed = true; }],
    ["sync-policy-missing", (value) => { delete value.syncPolicy; }],
    ["sync-authoritative-target-enabled", (value) => { value.syncPolicy.allowedTargetClassifications.push("canonical"); }],
    ["sync-bidirectional-enabled", (value) => { value.syncPolicy.automaticBidirectional = true; }],
    ["claim-policy-missing", (value) => { delete value.claimPolicy; }],
    ["claim-substitution-enabled", (value) => { value.claimPolicy.substitutionAllowed = true; }],
    ["promotion-policy-missing", (value) => { delete value.generatedPromotionPolicy; }],
    ["promotion-silent-enabled", (value) => { value.generatedPromotionPolicy.silentPromotion = true; }],
    ["adoption-policy-missing", (value) => { delete value.brownfieldAdoptionPolicy; }],
    ["adoption-generic-sync-enabled", (value) => { value.brownfieldAdoptionPolicy.genericSync = true; }],
    ["hlv-requirement-substitution-enabled", (value) => { value.artifactKinds.find((kind) => kind.id === "hlv.validation-result").substitutesRequirements = true; }],
    ["canonical-owner-changed", (value) => { value.artifactKinds.find((kind) => kind.id === "openspec.requirement").canonicalOwner = "source-native"; }],
    ["required-artifact-kind-removed", (value) => { value.artifactKinds = value.artifactKinds.filter((kind) => kind.id !== "openspec.requirement"); }],
    ["reader-list-missing", (value) => { delete value.artifactKinds[0].allowedReaders; }],
    ["reader-list-empty", (value) => { value.artifactKinds[0].allowedReaders = []; }],
    ["reader-list-unknown", (value) => { value.artifactKinds[0].allowedReaders.push("local-consumer"); }],
    ["added-kind-path-changed", (value) => { value.artifactKinds.find((kind) => kind.id === "ai.prompt").allowedPaths = ["**"]; }],
    ["added-kind-writer-changed", (value) => { value.artifactKinds.find((kind) => kind.id === "privacy.policy").allowedWriters.push("source-native"); }],
    ["added-kind-classification-changed", (value) => { value.artifactKinds.find((kind) => kind.id === "export.decision").classification = "derived"; }],
    ["privacy-field-added", (value) => { value.artifactKinds[0].dataSensitivity = ["internal"]; }],
    ["migration-added-list-changed", (value) => { value.migration.privacyHandoffAddedKindIdsFromAcceptedBaseline.pop(); }],
    ["predecessor-digest-changed", (value) => { value.predecessor.digest = "sha256:" + "0".repeat(64); }],
    ["path-safety-weakened", (value) => { value.pathSafety.trailingDotOrSpacePerSegment = "allow"; }],
    ["operation-denial-exit-changed", (value) => { value.operationProtocol.policyDeniedExitCode = 0; }],
    ["last-writer-wins-enabled", (value) => { value.conflictPolicy.lastWriterWins = true; }],
    ["absent-layer-transfers-authority", (value) => { value.projectRules.absentLayerTransfersAuthority = true; }],
    ["unknown-root-field", (value) => { value.shadowExtension = true; }]
  ];
  for (const [id, mutate] of closedProfileMutations) {
    const value = clone(successor);
    mutate(value);
    const bytes = Buffer.from(`${JSON.stringify(value, null, 2)}\n`, "utf8");
    assertContractFailure(
      `closed-profile-${id}`,
      runCustomContract(`closed-profile-${id}`, bytes, ref("1.3.1", sha256(bytes)))
    );
  }

  const extensionMutation = clone(successor);
  extensionMutation.artifactKinds.push({
    id: "local.shadow-kind",
    canonicalOwner: "lekalo",
    classification: "derived",
    allowedPaths: [".lekalo/shadow/**"],
    allowedReaders: [...successor.actors],
    allowedWriters: ["lekalo"]
  });
  const extensionBytes = Buffer.from(`${JSON.stringify(extensionMutation, null, 2)}\n`, "utf8");
  assertContractFailure(
    "runtime-extension-recomputed-digest",
    runCustomContract("runtime-extension", extensionBytes, ref("1.3.1", sha256(extensionBytes)))
  );

  const aliasMutation = clone(successor);
  aliasMutation.registryPolicy.localAliasesAllowed = true;
  aliasMutation.kindAliases = { "local.prompt": "ai.prompt" };
  const aliasBytes = Buffer.from(`${JSON.stringify(aliasMutation, null, 2)}\n`, "utf8");
  assertContractFailure(
    "local-alias-recomputed-digest",
    runCustomContract("local-alias", aliasBytes, ref("1.3.1", sha256(aliasBytes)))
  );

  const undeclaredSuccessor = clone(successor);
  undeclaredSuccessor.version = "1.4.0";
  const undeclaredBytes = Buffer.from(`${JSON.stringify(undeclaredSuccessor, null, 2)}\n`, "utf8");
  assertContractFailure(
    "undeclared-successor",
    runCustomContract("undeclared-successor", undeclaredBytes, ref("1.4.0", sha256(undeclaredBytes)))
  );

  const incompatibleSuccessor = clone(successor);
  incompatibleSuccessor.version = "2.0.0";
  incompatibleSuccessor.artifactKinds = incompatibleSuccessor.artifactKinds.filter((kind) => kind.id !== "openspec.requirement");
  const incompatibleBytes = Buffer.from(`${JSON.stringify(incompatibleSuccessor, null, 2)}\n`, "utf8");
  assertContractFailure(
    "incompatible-successor",
    runCustomContract("incompatible-successor", incompatibleBytes, ref("2.0.0", sha256(incompatibleBytes)))
  );

  const pathAlone = run(["--contract", successorPath]);
  assertContractFailure("contract-path-without-exact-ref", pathAlone);

  const successorBypassCases = [
    ["new-derived-cannot-sync-to-canonical", {
      action: "sync",
      actor: "lekalo",
      direction: "one-way",
      source: { artifactKind: "export.artifact", path: ".lekalo/privacy/exports/public.json" },
      target: { artifactKind: "lekalo.semantic-model", path: "lekalo/model/application.json" }
    }, 3, "sync.noncanonical-to-authoritative-forbidden"],
    ["new-kind-cannot-bypass-protected-contract-path", {
      action: "write",
      actor: "source-native",
      target: { artifactKind: "source-native.source-code", path: "contracts/privacy-policy.v1.json" }
    }, 3, "target.boundary-owner-mismatch"],
    ["unknown-kind-fails-malformed", {
      action: "read",
      actor: "lekalo",
      source: { artifactKind: "local.prompt", path: ".ai-factory/runs/r1/prompts/p1.json" }
    }, 1, undefined]
  ];
  for (const [id, operation, expectedExit, expectedCode] of successorBypassCases) {
    const result = runOperation("1.3.1", operation);
    if (expectedExit === 1) {
      if (result.status !== 1 || !result.stderr.includes("artifactKind-enum")) {
        fail(`${id}: expected malformed unknown-kind exit 1; stderr=${result.stderr.trim()}`);
      }
      process.stdout.write(`PASS ${id}: exit 1, closed registry\n`);
    } else {
      assertDecision(id, result, expectedExit, expectedCode);
    }
  }

  process.stdout.write("authority contract conformance: PASS (exact baseline/successor, registry, mutations, compatibility, boundary inheritance)\n");
} finally {
  rmSync(tempRoot, { recursive: true, force: true });
}
