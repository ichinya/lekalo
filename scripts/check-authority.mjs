#!/usr/bin/env node

import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { basename, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const manifestPath = resolve(root, "contracts/authority-contracts.manifest.json");
const defaultAllowedPath = resolve(root, "tests/fixtures/authority/allowed.json");
const defaultForbiddenPath = resolve(root, "tests/fixtures/authority/forbidden.json");
const defaultMalformedPath = resolve(root, "tests/fixtures/authority/malformed.json");
const trustedManifestSha256 = "b4b3c79a869e70b8a54b10fa23e5f345ff1b4db269dd3e9477bbc01bb86ee4ca";
const baselinePathBoundaries = [
  { pattern: "openspec/specs/**", owner: "openspec" },
  { pattern: "openspec/changes/**", owner: "openspec" },
  { pattern: ".ai-factory/plans/**", owner: "ai-factory" },
  { pattern: ".ai-factory/state/**", owner: "ai-factory" },
  { pattern: ".ai-factory/qa/**", owner: "ai-factory" },
  { pattern: ".ai-factory/rules/generated/**", owner: "ai-factory" },
  { pattern: "lekalo/**", owner: "lekalo" },
  { pattern: ".lekalo/**", owner: "lekalo" },
  { pattern: ".hlv/**", owner: "hlv" }
];
const baselineKindIds = [
  "openspec.requirement",
  "openspec.change-intent",
  "openspec.delta-spec",
  "openspec.expected-behavior",
  "ai-factory.execution-plan",
  "ai-factory.task-state",
  "ai-factory.runtime-state",
  "ai-factory.agent-lifecycle",
  "ai-factory.provider-evidence-envelope",
  "lekalo.semantic-model",
  "lekalo.stable-symbol",
  "lekalo.effect",
  "lekalo.scenario",
  "lekalo.target-binding",
  "lekalo.observed-model-draft",
  "lekalo.cache",
  "lekalo.generated-intermediate",
  "hlv.validation-result",
  "hlv.traceability-result",
  "hlv.gate-diagnostic",
  "hlv.project-contract",
  "source-native.source-code",
  "source-native.native-test",
  "generated.summary",
  "generated.rule",
  "generated.code"
];
const successorAddedKindIds = [
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
const acceptedProfiles = new Map([
  ["1.2.0", {
    contractId: "dev.lekalo.authority-matrix",
    version: "1.2.0",
    digest: "sha256:3446ce25ce33397f8c49426144a4c4dbf8c8ea23e4c759ceca70557606ffeda2",
    semanticSha256: "bb9797ddfa140a86de2e8e070bc887827c90b35c0a538e6f65a3aa1fbbd805fc",
    path: "contracts/authority-matrix.v1.2.0.json",
    sidecar: "contracts/authority-matrix.v1.2.0.sha256",
    pathBoundaries: baselinePathBoundaries,
    kindIds: baselineKindIds,
    readersRequired: false,
    boundarySchema: "legacy-profile"
  }],
  ["1.3.1", {
    contractId: "dev.lekalo.authority-matrix",
    version: "1.3.1",
    digest: "sha256:5c96ed68fe27956512b6de37e0fa23d223e4430d391b6a5869b12d2a8cb522d3",
    semanticSha256: "5e15ac4cf77b6fcfa6660a965714ffa4038e816f045032c97565a6c215bc3c38",
    path: "contracts/authority-matrix.v1.3.1.json",
    sidecar: "contracts/authority-matrix.v1.3.1.sha256",
    kindIds: [...baselineKindIds, ...successorAddedKindIds],
    readersRequired: true,
    boundarySchema: "kind-bound"
  }]
]);
const rejectedProfiles = new Map([
  ["1.3.0", {
    contractId: "dev.lekalo.authority-matrix",
    version: "1.3.0",
    digest: "sha256:50a4b9c8533644bd61841e695fc042e4ac52307952dcccb37bf6f2d3ab3a3211",
    path: "contracts/authority-matrix.v1.3.0.json",
    sidecar: "contracts/authority-matrix.v1.3.0.sha256",
    replacementVersion: "1.3.1"
  }]
]);
const requiredLegacyConditionalPathBoundary = {
  pattern: "project.yaml",
  owner: "hlv",
  artifactKind: "hlv.project-contract",
  requiredContext: "hlvLayoutConfirmed",
  requiredValue: true
};

class OperationShapeError extends Error {
  constructor(code) {
    super(code);
    this.name = "OperationShapeError";
    this.code = code;
  }
}

function fail(message) {
  throw new Error(message);
}

function shapeFail(code) {
  throw new OperationShapeError(code);
}

async function readJson(path) {
  try {
    return JSON.parse(await readFile(path, "utf8"));
  } catch (error) {
    fail(`Cannot read JSON ${path}: ${error.message}`);
  }
}

async function readExactJson(path) {
  let bytes;
  try {
    bytes = await readFile(path);
  } catch (error) {
    fail(`Cannot read JSON ${path}: ${error.message}`);
  }
  try {
    return { bytes, value: JSON.parse(bytes.toString("utf8")) };
  } catch (error) {
    fail(`Cannot read JSON ${path}: ${error.message}`);
  }
}

function sha256Bytes(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function unique(values, label) {
  if (new Set(values).size !== values.length) {
    fail(`${label} must contain unique values`);
  }
}

function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map((item) => canonicalJson(item)).join(",")}]`;
  if (value !== null && typeof value === "object") {
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
}

function semanticSha256(value) {
  return createHash("sha256").update(canonicalJson(value), "utf8").digest("hex");
}

function exactAuthorityRef(profile) {
  return `${profile.contractId}@${profile.version}@${profile.digest}`;
}

function parseAuthorityRef(value) {
  if (typeof value !== "string") fail("authorityRef must be a string");
  const match = /^([^@]+)@([^@]+)@(sha256:[0-9a-f]{64})$/u.exec(value);
  if (!match) fail("authorityRef must be contractId@version@sha256:<64 lowercase hex>");
  return { contractId: match[1], version: match[2], digest: match[3] };
}

async function loadTrustedManifest() {
  const { bytes, value: manifest } = await readExactJson(manifestPath);
  const actualManifestSha256 = sha256Bytes(bytes);
  if (actualManifestSha256 !== trustedManifestSha256) {
    fail(`Authority manifest exact bytes are not trusted: ${actualManifestSha256}`);
  }
  exactContractObject(
    manifest,
    ["manifestId", "formatVersion", "digestAlgorithm", "digestInput", "selfReferential", "acceptedContracts", "rejectedContracts", "currentAuthorityRef"],
    "authority manifest"
  );
  if (
    manifest.manifestId !== "dev.lekalo.authority-contract-manifest" ||
    manifest.formatVersion !== "1.0.0" ||
    manifest.digestAlgorithm !== "sha256" ||
    manifest.digestInput !== "exact-file-bytes" ||
    manifest.selfReferential !== false
  ) {
    fail("Authority manifest digest custody metadata is unsupported");
  }
  if (!Array.isArray(manifest.acceptedContracts) || manifest.acceptedContracts.length !== acceptedProfiles.size) {
    fail("Authority manifest must enumerate exactly the trusted accepted contracts");
  }

  const entries = new Map();
  for (const entry of manifest.acceptedContracts) {
    if (!isPlainObject(entry) || typeof entry.version !== "string") fail("Authority manifest entry is malformed");
    if (entries.has(entry.version)) fail(`Authority manifest duplicates version ${entry.version}`);
    const profile = acceptedProfiles.get(entry.version);
    if (!profile) fail(`Authority manifest declares unsupported version ${entry.version}`);
    if (
      entry.contractId !== profile.contractId ||
      entry.digest !== profile.digest ||
      entry.path !== profile.path ||
      entry.sidecar !== profile.sidecar
    ) {
      fail(`Authority manifest entry ${entry.version} does not match the trusted exact triple and paths`);
    }
    const sidecarBytes = await readFile(resolve(root, entry.sidecar));
    const expectedSidecar = `${profile.digest.slice("sha256:".length)}  ${basename(profile.path)}\n`;
    if (sidecarBytes.toString("utf8") !== expectedSidecar) {
      fail(`Authority sidecar ${entry.sidecar} does not exactly match ${exactAuthorityRef(profile)}`);
    }
    entries.set(entry.version, entry);
  }

  if (!Array.isArray(manifest.rejectedContracts) || manifest.rejectedContracts.length !== rejectedProfiles.size) {
    fail("Authority manifest must enumerate exactly the trusted rejected contracts");
  }
  for (const entry of manifest.rejectedContracts) {
    const profile = rejectedProfiles.get(entry.version);
    if (
      !profile ||
      entry.contractId !== profile.contractId ||
      entry.digest !== profile.digest ||
      entry.path !== profile.path ||
      entry.sidecar !== profile.sidecar ||
      entry.status !== "rejected-yanked-candidate" ||
      entry.replacement?.version !== profile.replacementVersion
    ) {
      fail(`Authority manifest rejected entry ${entry.version} is not the trusted yanked candidate lifecycle`);
    }
    const sidecarBytes = await readFile(resolve(root, entry.sidecar));
    const expectedSidecar = `${profile.digest.slice("sha256:".length)}  ${basename(profile.path)}\n`;
    if (sidecarBytes.toString("utf8") !== expectedSidecar) {
      fail(`Rejected authority sidecar ${entry.sidecar} does not preserve exact candidate bytes`);
    }
  }

  const current = manifest.currentAuthorityRef;
  if (!isPlainObject(current)) fail("Authority manifest currentAuthorityRef is malformed");
  const currentProfile = acceptedProfiles.get(current.version);
  if (
    !currentProfile ||
    current.contractId !== currentProfile.contractId ||
    current.digest !== currentProfile.digest ||
    current.version !== "1.3.1"
  ) {
    fail("Authority manifest currentAuthorityRef must be the trusted reviewed successor");
  }
  return { manifest, entries, currentProfile };
}

function selectProfile(options, manifestContext) {
  let requested;
  if (options.authorityRef !== undefined) {
    requested = parseAuthorityRef(options.authorityRef);
  } else if (options.contractVersion !== undefined) {
    if (rejectedProfiles.has(options.contractVersion)) {
      fail(`Authority contract ${options.contractVersion} is rejected/yanked and must never be selected`);
    }
    const profile = acceptedProfiles.get(options.contractVersion);
    if (!profile) fail(`Unsupported authority contract version ${options.contractVersion}`);
    requested = { contractId: profile.contractId, version: profile.version, digest: profile.digest };
  } else {
    requested = manifestContext.manifest.currentAuthorityRef;
  }

  const profile = acceptedProfiles.get(requested.version);
  if (rejectedProfiles.has(requested.version)) {
    fail(`Authority contract ${requested.version} is rejected/yanked and must never be selected`);
  }
  if (
    !profile ||
    requested.contractId !== profile.contractId ||
    requested.digest !== profile.digest
  ) {
    fail(`Unsupported exact authority reference ${requested.contractId}@${requested.version}@${requested.digest}`);
  }
  return profile;
}

function exactContractObject(value, keys, label) {
  if (!isPlainObject(value)) fail(`${label} must be an object`);
  const actualKeys = Object.keys(value).sort();
  const expectedKeys = [...keys].sort();
  if (JSON.stringify(actualKeys) !== JSON.stringify(expectedKeys)) {
    fail(`${label} must contain exactly ${expectedKeys.join(",")}`);
  }
}

function boundarySpecificity(pattern) {
  const segments = pattern.split("/");
  let literalPrefixSegments = 0;
  for (const segment of segments) {
    if (/[*?]/u.test(segment)) break;
    literalPrefixSegments += 1;
  }
  const literalSegmentCount = segments.filter((segment) => !/[*?]/u.test(segment)).length;
  const literalCharacterCount = [...pattern].filter((character) => !["*", "?", "/"].includes(character)).length;
  const wildcardTokenCount = pattern.match(/\*\*|\*|\?/gu)?.length ?? 0;
  return [
    literalPrefixSegments,
    literalSegmentCount,
    literalCharacterCount,
    segments.length,
    -wildcardTokenCount
  ];
}

function compareSpecificity(left, right) {
  for (let index = 0; index < left.length; index += 1) {
    if (left[index] !== right[index]) return left[index] - right[index];
  }
  return 0;
}

function tokenizeGlob(pattern) {
  const tokens = [];
  const lower = pattern.toLocaleLowerCase("en-US");
  for (let index = 0; index < lower.length; index += 1) {
    if (lower[index] === "*" && lower[index + 1] === "*" && lower[index + 2] === "/") {
      tokens.push({ type: "globstar-slash" });
      index += 2;
    } else if (lower[index] === "*" && lower[index + 1] === "*") {
      tokens.push({ type: "globstar" });
      index += 1;
    } else if (lower[index] === "*") {
      tokens.push({ type: "star" });
    } else if (lower[index] === "?") {
      tokens.push({ type: "any-non-slash" });
    } else {
      tokens.push({ type: "literal", value: lower[index] });
    }
  }
  return tokens;
}

function epsilonClosure(tokens, inputStates) {
  const states = new Set(inputStates);
  const pending = [...states];
  while (pending.length > 0) {
    const state = pending.pop();
    const token = tokens[state];
    if (token && ["star", "globstar", "globstar-slash"].includes(token.type) && !states.has(state + 1)) {
      states.add(state + 1);
      pending.push(state + 1);
    }
  }
  return states;
}

function moveGlob(tokens, states, character) {
  const next = new Set();
  for (const state of epsilonClosure(tokens, states)) {
    const token = tokens[state];
    if (!token) continue;
    if (token.type === "literal" && token.value === character) next.add(state + 1);
    if (token.type === "any-non-slash" && character !== "/") next.add(state + 1);
    if (token.type === "star" && character !== "/") next.add(state);
    if (token.type === "globstar") next.add(state);
    if (token.type === "globstar-slash") {
      next.add(state);
      if (character === "/") next.add(state + 1);
    }
  }
  return epsilonClosure(tokens, next);
}

const patternOverlapCache = new Map();

function patternsOverlap(leftPattern, rightPattern) {
  const cacheKey = [leftPattern, rightPattern].sort().join("\u0000");
  if (patternOverlapCache.has(cacheKey)) return patternOverlapCache.get(cacheKey);
  const left = tokenizeGlob(leftPattern);
  const right = tokenizeGlob(rightPattern);
  const alphabet = new Set(["/", "\u0000"]);
  for (const token of [...left, ...right]) {
    if (token.type === "literal") alphabet.add(token.value);
  }
  const startLeft = epsilonClosure(left, [0]);
  const startRight = epsilonClosure(right, [0]);
  const queue = [[startLeft, startRight]];
  const seen = new Set();
  while (queue.length > 0) {
    const [leftStates, rightStates] = queue.shift();
    const key = `${[...leftStates].sort((a, b) => a - b).join(",")}|${[...rightStates].sort((a, b) => a - b).join(",")}`;
    if (seen.has(key)) continue;
    seen.add(key);
    if (leftStates.has(left.length) && rightStates.has(right.length)) {
      patternOverlapCache.set(cacheKey, true);
      return true;
    }
    for (const character of alphabet) {
      const nextLeft = moveGlob(left, leftStates, character);
      const nextRight = moveGlob(right, rightStates, character);
      if (nextLeft.size > 0 && nextRight.size > 0) queue.push([nextLeft, nextRight]);
    }
  }
  patternOverlapCache.set(cacheKey, false);
  return false;
}

function boundaryPolicyIdentity(boundary) {
  return canonicalJson({
    owner: boundary.owner,
    artifactKinds: boundary.artifactKinds,
    readers: boundary.readers,
    writers: boundary.writers
  });
}

function validateBoundBoundary(boundary, label, kinds, owners, actors, conditional) {
  exactContractObject(
    boundary,
    conditional
      ? ["pattern", "owner", "artifactKinds", "readers", "writers", "requiredContext", "requiredValue"]
      : ["pattern", "owner", "artifactKinds", "readers", "writers"],
    label
  );
  if (typeof boundary.pattern !== "string" || typeof boundary.owner !== "string") {
    fail(`${label} pattern and owner must be strings`);
  }
  if (!owners.includes(boundary.owner)) fail(`${label} has unknown owner ${boundary.owner}`);
  for (const field of ["artifactKinds", "readers", "writers"]) {
    if (!Array.isArray(boundary[field]) || boundary[field].length === 0) fail(`${label}.${field} must be non-empty`);
    unique(boundary[field], `${label}.${field}`);
  }
  for (const actor of [...boundary.readers, ...boundary.writers]) {
    if (!actors.includes(actor)) fail(`${label} has unknown actor ${actor}`);
  }
  for (const kindId of boundary.artifactKinds) {
    const kind = kinds.get(kindId);
    if (!kind) fail(`${label} references unknown kind ${kindId}`);
    if (kind.canonicalOwner !== boundary.owner) fail(`${label} owner disagrees with ${kindId}`);
    const kindReaders = kind.allowedReaders ?? actors;
    if (boundary.readers.some((actor) => !kindReaders.includes(actor))) {
      fail(`${label} readers are not a subset of ${kindId}.allowedReaders`);
    }
    if (boundary.writers.some((actor) => !kind.allowedWriters.includes(actor))) {
      fail(`${label} writers are not a subset of ${kindId}.allowedWriters`);
    }
    if (!kind.allowedPaths.some((allowedPath) => patternsOverlap(boundary.pattern, allowedPath))) {
      fail(`${label} does not overlap an allowed path for ${kindId}`);
    }
  }
  if (conditional) {
    if (boundary.requiredContext !== "hlvLayoutConfirmed" || boundary.requiredValue !== true) {
      fail(`${label} must require hlvLayoutConfirmed boolean true`);
    }
  }
  return { ...boundary, specificity: boundarySpecificity(boundary.pattern), conditional };
}

function validateNoAmbiguousTies(boundaries) {
  for (let left = 0; left < boundaries.length; left += 1) {
    for (let right = left + 1; right < boundaries.length; right += 1) {
      const a = boundaries[left];
      const b = boundaries[right];
      if (
        compareSpecificity(a.specificity, b.specificity) === 0 &&
        patternsOverlap(a.pattern, b.pattern) &&
        boundaryPolicyIdentity(a) !== boundaryPolicyIdentity(b)
      ) {
        fail(`Equal-specificity overlapping boundaries are ambiguous: ${a.pattern} and ${b.pattern}`);
      }
    }
  }
}

function normalizeLegacyBoundaries(boundaries, conditionalBoundaries, profile, kinds, owners, actors) {
  if (boundaries.length !== profile.pathBoundaries.length) {
    fail("Legacy pathBoundaries must equal the closed 1.2.0 baseline");
  }
  const normalized = boundaries.map((boundary, index) => {
    exactContractObject(boundary, ["pattern", "owner"], `pathBoundaries[${index}]`);
    const required = profile.pathBoundaries[index];
    if (boundary.pattern !== required.pattern || boundary.owner !== required.owner) {
      fail(`Legacy boundary ${index} does not match the accepted 1.2.0 profile`);
    }
    const permittedKinds = [...kinds.values()].filter(
      (kind) => kind.canonicalOwner === boundary.owner && kind.allowedPaths.some((path) => patternsOverlap(path, boundary.pattern))
    );
    const writers = actors.filter((actor) => permittedKinds.every((kind) => kind.allowedWriters.includes(actor)));
    return {
      pattern: boundary.pattern,
      owner: boundary.owner,
      artifactKinds: permittedKinds.map((kind) => kind.id),
      readers: [...actors],
      writers,
      specificity: boundarySpecificity(boundary.pattern),
      conditional: false
    };
  });
  if (conditionalBoundaries.length !== 1) fail("Legacy conditional HLV boundary is required");
  const conditional = conditionalBoundaries[0];
  exactContractObject(
    conditional,
    ["pattern", "owner", "artifactKind", "requiredContext", "requiredValue"],
    "conditionalPathBoundaries[0]"
  );
  for (const [key, expected] of Object.entries(requiredLegacyConditionalPathBoundary)) {
    if (conditional[key] !== expected) fail(`Legacy conditional boundary ${key} changed`);
  }
  const normalizedConditional = {
    pattern: conditional.pattern,
    owner: conditional.owner,
    artifactKinds: [conditional.artifactKind],
    readers: [...actors],
    writers: ["hlv"],
    requiredContext: conditional.requiredContext,
    requiredValue: conditional.requiredValue,
    specificity: boundarySpecificity(conditional.pattern),
    conditional: true
  };
  validateNoAmbiguousTies([...normalized, normalizedConditional]);
  return { boundaries: normalized, conditionalBoundaries: [normalizedConditional] };
}

function validateBoundaryContracts(contract, profile, kinds, owners, actors) {
  const boundaries = contract.pathBoundaries;
  const conditionalBoundaries = contract.conditionalPathBoundaries;
  if (!Array.isArray(boundaries)) fail("pathBoundaries must be an array");
  if (!Array.isArray(conditionalBoundaries)) fail("conditionalPathBoundaries must be an array");
  if (profile.boundarySchema === "legacy-profile") {
    return normalizeLegacyBoundaries(boundaries, conditionalBoundaries, profile, kinds, owners, actors);
  }
  if (conditionalBoundaries.length !== 1) fail("Exactly one conditional HLV boundary is required");
  const normalized = boundaries.map((boundary, index) =>
    validateBoundBoundary(boundary, `pathBoundaries[${index}]`, kinds, owners, actors, false)
  );
  const normalizedConditional = conditionalBoundaries.map((boundary, index) =>
    validateBoundBoundary(boundary, `conditionalPathBoundaries[${index}]`, kinds, owners, actors, true)
  );
  const lowerPatterns = [...normalized, ...normalizedConditional].map((boundary) => boundary.pattern.toLocaleLowerCase("en-US"));
  unique(lowerPatterns, "case-insensitive boundary patterns");
  validateNoAmbiguousTies([...normalized, ...normalizedConditional]);
  return { boundaries: normalized, conditionalBoundaries: normalizedConditional };
}

function isPlainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function hasOwn(object, key) {
  return Object.prototype.hasOwnProperty.call(object, key);
}

function exactObject(value, required, optional, label) {
  if (!isPlainObject(value)) shapeFail(`${label}.object-required`);
  for (const key of required) {
    if (!hasOwn(value, key)) shapeFail(`${label}.${key}-required`);
  }
  const allowed = new Set([...required, ...optional]);
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) shapeFail(`${label}.${key}-unexpected`);
  }
}

function booleanField(object, key, label) {
  if (typeof object[key] !== "boolean") shapeFail(`${label}.${key}-boolean`);
}

function validateRefShape(ref, label, context) {
  exactObject(ref, ["artifactKind", "path"], [], label);
  if (typeof ref.artifactKind !== "string" || !context.kinds.has(ref.artifactKind)) {
    shapeFail(`${label}.artifactKind-enum`);
  }
  if (typeof ref.path !== "string") shapeFail(`${label}.path-string`);
}

function validateContextShape(operation) {
  if (!hasOwn(operation, "context")) return;
  exactObject(operation.context, ["hlvLayoutConfirmed"], [], "context");
  booleanField(operation.context, "hlvLayoutConfirmed", "context");
}

function validateEvidenceShape(evidence) {
  exactObject(evidence, ["explicitAdoption", "reviewed", "provenance"], [], "evidence");
  for (const key of ["explicitAdoption", "reviewed"]) {
    booleanField(evidence, key, "evidence");
  }
  exactObject(
    evidence.provenance,
    ["sourceRevision", "sourceDigest", "recordedBy"],
    [],
    "provenance"
  );
  if (typeof evidence.provenance.sourceRevision !== "string" || evidence.provenance.sourceRevision.length === 0) {
    shapeFail("provenance.sourceRevision-nonempty-string");
  }
  if (
    typeof evidence.provenance.sourceDigest !== "string" ||
    !/^sha256:[0-9a-f]{64}$/iu.test(evidence.provenance.sourceDigest)
  ) {
    shapeFail("provenance.sourceDigest-sha256");
  }
  if (typeof evidence.provenance.recordedBy !== "string" || evidence.provenance.recordedBy.length === 0) {
    shapeFail("provenance.recordedBy-nonempty-string");
  }
}

function validateReportsShape(reports) {
  exactObject(reports, ["loss", "conflict", "provenance"], [], "reports");
  for (const key of ["loss", "conflict", "provenance"]) {
    booleanField(reports, key, "reports");
  }
}

function validateOperationShape(operation, context) {
  if (!isPlainObject(operation)) shapeFail("operation.root-object-required");
  if (!hasOwn(operation, "action")) shapeFail("operation.action-required");
  const actions = new Set(["read", "write", "sync", "claim", "promote", "adopt"]);
  if (typeof operation.action !== "string" || !actions.has(operation.action)) {
    shapeFail("operation.action-enum");
  }

  if (operation.action === "read") {
    exactObject(operation, ["action", "actor", "source"], ["context"], "operation");
    validateRefShape(operation.source, "source", context);
  } else if (operation.action === "write") {
    exactObject(operation, ["action", "actor", "target"], ["context"], "operation");
    validateRefShape(operation.target, "target", context);
  } else if (operation.action === "sync") {
    if (!hasOwn(operation, "direction")) shapeFail("operation.direction-required");
    if (typeof operation.direction !== "string" || !["one-way", "bidirectional"].includes(operation.direction)) {
      shapeFail("operation.direction-enum");
    }
    if (operation.direction === "one-way") {
      exactObject(operation, ["action", "actor", "direction", "source", "target"], ["context"], "operation");
    } else {
      exactObject(
        operation,
        ["action", "actor", "direction", "automatic", "writesCanonical", "source", "target", "reports"],
        ["context"],
        "operation"
      );
      booleanField(operation, "automatic", "operation");
      booleanField(operation, "writesCanonical", "operation");
      validateReportsShape(operation.reports);
    }
    validateRefShape(operation.source, "source", context);
    validateRefShape(operation.target, "target", context);
  } else if (operation.action === "claim") {
    exactObject(operation, ["action", "actor", "substitute", "source", "target"], ["context"], "operation");
    booleanField(operation, "substitute", "operation");
    validateRefShape(operation.source, "source", context);
    validateRefShape(operation.target, "target", context);
  } else {
    exactObject(operation, ["action", "actor", "source", "target", "evidence"], [], "operation");
    validateRefShape(operation.source, "source", context);
    validateRefShape(operation.target, "target", context);
    validateEvidenceShape(operation.evidence);
  }

  if (typeof operation.actor !== "string" || !context.actors.has(operation.actor)) {
    shapeFail("operation.actor-enum");
  }
  validateContextShape(operation);
}

function isValidLogicalPath(value) {
  if (typeof value !== "string" || value.length === 0) return false;
  if (value.includes("\\") || value.startsWith("/") || /^[A-Za-z]:/.test(value)) return false;
  if (/[\u0000-\u001F\u007F]/.test(value) || /%(?:2e|2f|5c)/i.test(value)) return false;

  const dosDevice = /^(?:con|prn|aux|nul|clock\$|conin\$|conout\$|com[1-9\u00B9\u00B2\u00B3]|lpt[1-9\u00B9\u00B2\u00B3])$/iu;
  return value.split("/").every((segment) => {
    if (segment === "" || segment === "." || segment === "..") return false;
    if (/[. ]$/u.test(segment) || segment.includes(":") || /~[0-9]/iu.test(segment)) return false;
    const deviceCandidate = segment.split(".", 1)[0].replace(/[ .]+$/u, "");
    return !dosDevice.test(deviceCandidate);
  });
}

function globToRegExp(glob) {
  let output = "^";
  for (let index = 0; index < glob.length; index += 1) {
    const character = glob[index];
    if (character === "*") {
      if (glob[index + 1] === "*") {
        index += 1;
        if (glob[index + 1] === "/") {
          index += 1;
          output += "(?:.*/)?";
        } else {
          output += ".*";
        }
      } else {
        output += "[^/]*";
      }
    } else if (character === "?") {
      output += "[^/]";
    } else {
      output += character.replace(/[\\^$+?.()|{}\[\]]/g, "\\$&");
    }
  }
  return new RegExp(`${output}$`, "iu");
}

function matches(path, pattern) {
  return globToRegExp(pattern).test(path);
}

function decision(allowed, code, details = undefined) {
  return details === undefined ? { allowed, code } : { allowed, code, details };
}

function validateContract(contract, profile) {
  if (contract.contractId !== profile.contractId) fail("Unexpected contractId");
  if (contract.version !== profile.version) fail(`Contract version must be ${profile.version}`);
  if (contract.status !== "accepted") fail("Contract status must be accepted");
  const semanticallyIdenticalCopiesAllowed = profile.version === "1.2.0";
  if (
    contract.compatibility?.checkerProfile !== "closed-exact" ||
    contract.compatibility?.extensionsAllowed !== false ||
    contract.compatibility?.semanticallyIdenticalCopiesAllowed !== semanticallyIdenticalCopiesAllowed
  ) {
    fail("Contract compatibility profile must be closed-exact");
  }
  if (contract.pathSyntax !== "project-relative-posix") fail("Unsupported path syntax");
  if (contract.pathComparison !== "case-insensitive-fail-closed") fail("Unsupported path comparison");
  const pathSafety = contract.pathSafety ?? {};
  if (
    pathSafety.trailingDotOrSpacePerSegment !== "reject" ||
    pathSafety.alternateDataStreamColon !== "reject" ||
    pathSafety.dosDeviceName !== "reject" ||
    pathSafety.shortNameLikeTildeNumber !== "reject" ||
    pathSafety.physicalContainment !== "adapter-must-resolve-and-verify"
  ) {
    fail("Windows-alias and physical-containment path safety must be fail-closed");
  }

  if (!Array.isArray(contract.owners)) fail("owners must be an array");
  if (!Array.isArray(contract.actors)) fail("actors must be an array");
  if (!Array.isArray(contract.classifications)) fail("classifications must be an array");
  if (!Array.isArray(contract.artifactKinds)) fail("artifactKinds must be an array");
  const owners = contract.owners.map((owner) => owner.id);
  const actors = contract.actors;
  const classifications = new Set(contract.classifications ?? []);
  const kinds = contract.artifactKinds;
  const rawBoundaries = contract.pathBoundaries;
  const rawConditionalBoundaries = contract.conditionalPathBoundaries;

  if (owners.length !== 5) fail("The contract must define the five authority owners");
  unique(owners, "owners");
  unique(actors, "actors");
  unique(kinds.map((kind) => kind.id), "artifactKinds");
  if (JSON.stringify(kinds.map((kind) => kind.id)) !== JSON.stringify(profile.kindIds)) {
    fail(`Artifact registry for ${profile.version} must be closed-exact with stable ordered IDs`);
  }

  for (const owner of owners) {
    if (!actors.includes(owner)) fail(`Owner ${owner} must also be a valid actor`);
  }

  for (const kind of kinds) {
    if (typeof kind.canonicalOwner !== "string" || !owners.includes(kind.canonicalOwner)) {
      fail(`Artifact kind ${kind.id} must have exactly one known scalar canonicalOwner`);
    }
    if (!classifications.has(kind.classification)) fail(`Artifact kind ${kind.id} has an unknown classification`);
    if (!Array.isArray(kind.allowedPaths) || kind.allowedPaths.length === 0) {
      fail(`Artifact kind ${kind.id} must declare allowedPaths`);
    }
    if (!Array.isArray(kind.allowedWriters) || kind.allowedWriters.length === 0) {
      fail(`Artifact kind ${kind.id} must declare allowedWriters`);
    }
    if (profile.readersRequired && (!Array.isArray(kind.allowedReaders) || kind.allowedReaders.length === 0)) {
      fail(`Artifact kind ${kind.id} must declare allowedReaders`);
    }
    if (hasOwn(kind, "dataSensitivity") || hasOwn(kind, "exportDisposition")) {
      fail(`Artifact kind ${kind.id} must not contain privacy classification fields`);
    }
    if (Array.isArray(kind.allowedReaders)) {
      unique(kind.allowedReaders, `${kind.id}.allowedReaders`);
      for (const reader of kind.allowedReaders) {
        if (!actors.includes(reader)) fail(`Artifact kind ${kind.id} has unknown reader ${reader}`);
      }
    }
    unique(kind.allowedPaths, `${kind.id}.allowedPaths`);
    unique(kind.allowedWriters, `${kind.id}.allowedWriters`);
    for (const writer of kind.allowedWriters) {
      if (!actors.includes(writer)) fail(`Artifact kind ${kind.id} has unknown writer ${writer}`);
    }
  }
  const kindMap = new Map(kinds.map((kind) => [kind.id, kind]));
  const boundaryContext = validateBoundaryContracts(
    { pathBoundaries: rawBoundaries, conditionalPathBoundaries: rawConditionalBoundaries },
    profile,
    kindMap,
    owners,
    actors
  );

  const requiredOwnerPrefixes = new Map([
    ["openspec.", "openspec"],
    ["ai-factory.", "ai-factory"],
    ["lekalo.", "lekalo"],
    ["hlv.", "hlv"],
    ["source-native.", "source-native"],
    ["authority.", "lekalo"],
    ["privacy.", "lekalo"],
    ["diagnostics.", "ai-factory"],
    ["context.", "ai-factory"],
    ["trace.", "ai-factory"],
    ["ai.", "ai-factory"],
    ["metrics.", "hlv"],
    ["repository.", "source-native"],
    ["native.", "source-native"],
    ["consumer.", "lekalo"],
    ["export.", "lekalo"],
    ["redaction.", "lekalo"],
    ["aggregate.", "lekalo"]
  ]);
  for (const kind of kinds) {
    for (const [prefix, owner] of requiredOwnerPrefixes) {
      if (kind.id.startsWith(prefix) && kind.canonicalOwner !== owner) fail(`${kind.id} must be owned by ${owner}`);
    }
  }
  const fixtureKind = kinds.find((kind) => kind.id === "fixture");
  if (fixtureKind && fixtureKind.canonicalOwner !== "source-native") fail("fixture must be owned by source-native");

  const canonicalLekaloKinds = kinds.filter(
    (kind) => kind.id.startsWith("lekalo.") && kind.canonicalOwner === "lekalo" && kind.classification === "canonical"
  );
  if (canonicalLekaloKinds.length === 0) fail("Canonical Lekalo artifact kinds are missing");
  for (const kind of canonicalLekaloKinds) {
    if (kind.allowedPaths.some((pattern) => !pattern.startsWith("lekalo/"))) {
      fail(`Canonical Lekalo artifact ${kind.id} must stay under lekalo/**`);
    }
  }

  const hlvKinds = kinds.filter((kind) => kind.canonicalOwner === "hlv");
  if (hlvKinds.length === 0 || hlvKinds.some((kind) => kind.substitutesRequirements !== false)) {
    fail("Every HLV artifact kind must explicitly forbid requirement substitution");
  }

  const generatedKinds = kinds.filter((kind) => kind.id.startsWith("generated."));
  if (generatedKinds.length === 0 || generatedKinds.some((kind) => kind.silentCanonicalPromotion !== false)) {
    fail("Every generated artifact kind must explicitly forbid silent canonical promotion");
  }

  const sync = contract.syncPolicy ?? {};
  if (sync.automaticBidirectional !== false || sync.crossOwnerCanonicalWrites !== false) {
    fail("Automatic bidirectional or cross-owner canonical writes must be forbidden");
  }
  if (sync.reconciliationWritesCanonical !== false) fail("Reconciliation must be proposal-only");
  if (JSON.stringify(sync.allowedSourceClassifications) !== JSON.stringify(["canonical", "direct-evidence"])) {
    fail("One-way sync sources must be canonical or direct evidence");
  }
  if (JSON.stringify(sync.allowedTargetClassifications) !== JSON.stringify(["derived", "cached", "runtime-only"])) {
    fail("One-way sync targets must be non-authoritative");
  }
  for (const report of ["loss", "conflict", "provenance"]) {
    if (!sync.requiredReconciliationReports?.includes(report)) fail(`Reconciliation report ${report} is required`);
  }

  const promotion = contract.generatedPromotionPolicy ?? {};
  if (promotion.mode !== "explicit-adoption" || promotion.silentPromotion !== false) {
    fail("Generated promotion must require explicit adoption");
  }
  for (const evidence of ["explicitAdoption", "reviewed", "provenance"]) {
    if (!promotion.requiredEvidence?.includes(evidence)) fail(`Promotion evidence ${evidence} is required`);
  }

  const adoption = contract.brownfieldAdoptionPolicy ?? {};
  if (
    adoption.action !== "adopt" ||
    adoption.sourceKind !== "lekalo.observed-model-draft" ||
    adoption.targetKind !== "lekalo.semantic-model" ||
    adoption.actor !== "lekalo" ||
    adoption.genericSync !== false
  ) {
    fail("Brownfield adoption policy is incomplete");
  }
  for (const evidence of ["explicitAdoption", "reviewed", "provenance"]) {
    if (!adoption.requiredEvidence?.includes(evidence)) fail(`Adoption evidence ${evidence} is required`);
  }

  const operationProtocol = contract.operationProtocol ?? {};
  if (
    operationProtocol.discriminator !== "action" ||
    JSON.stringify(operationProtocol.actions) !==
      JSON.stringify(["read", "write", "sync", "claim", "promote", "adopt"]) ||
    JSON.stringify(operationProtocol.evidenceProvenanceFields) !==
      JSON.stringify(["sourceRevision", "sourceDigest", "recordedBy"]) ||
    operationProtocol.strictObjects !== true ||
    operationProtocol.unknownFields !== "reject" ||
    operationProtocol.malformedExitCode !== 1 ||
    operationProtocol.policyDeniedExitCode !== 3
  ) {
    fail("Operation protocol must be strict and distinguish malformed input from policy denial");
  }

  const claim = contract.claimPolicy ?? {};
  if (
    claim.mode !== "reference-only" ||
    claim.requiredSubstitute !== false ||
    claim.substitutionAllowed !== false ||
    claim.promotionAction !== "promote" ||
    claim.adoptionAction !== "adopt"
  ) {
    fail("Claim policy must be universally reference-only");
  }

  if (profile.version === "1.3.1") {
    const predecessor = contract.predecessor ?? {};
    const baselineProfile = acceptedProfiles.get("1.2.0");
    if (
      predecessor.contractId !== "dev.lekalo.authority-matrix" ||
      predecessor.version !== "1.3.0" ||
      predecessor.digest !== rejectedProfiles.get("1.3.0").digest ||
      predecessor.lifecycle !== "rejected-yanked-candidate"
    ) {
      fail("Corrective successor must name the exact rejected/yanked 1.3.0 candidate");
    }
    if (
      contract.acceptedBaseline?.contractId !== baselineProfile.contractId ||
      contract.acceptedBaseline?.version !== baselineProfile.version ||
      contract.acceptedBaseline?.digest !== baselineProfile.digest
    ) {
      fail("Corrective successor must retain the exact immutable 1.2.0 accepted baseline");
    }
    const registryPolicy = contract.registryPolicy ?? {};
    if (
      registryPolicy.closure !== "closed-exact" ||
      registryPolicy.runtimeExtensionsAllowed !== false ||
      registryPolicy.localAliasesAllowed !== false ||
      registryPolicy.unknownKinds !== "reject" ||
      registryPolicy.successorAdmission !== "reviewed-exact-triple-only" ||
      JSON.stringify(registryPolicy.kindRequiredFields) !== JSON.stringify([
        "id", "canonicalOwner", "classification", "allowedPaths", "allowedReaders", "allowedWriters"
      ]) ||
      JSON.stringify(registryPolicy.forbiddenKindFields) !== JSON.stringify(["dataSensitivity", "exportDisposition"])
    ) {
      fail("Successor registry policy must remain closed-exact without aliases or runtime extension");
    }
    const successorProcedure = contract.successorProcedure ?? {};
    if (
      successorProcedure.newVersionRequired !== true ||
      successorProcedure.exactDigestRequired !== true ||
      successorProcedure.predecessorExactRefRequired !== true ||
      successorProcedure.compatibilityMetadataRequired !== true ||
      successorProcedure.migrationMetadataRequired !== true ||
      successorProcedure.silentMutationAllowed !== false ||
      successorProcedure.acceptanceManifest !== "contracts/authority-contracts.manifest.json" ||
      successorProcedure.migrationDocument !== "docs/authority-contract-migration-1.3.0-to-1.3.1.md"
    ) {
      fail("Reviewed-successor procedure is incomplete");
    }
    const migration = contract.migration ?? {};
    if (
      migration.compatibility !== "corrective-successor-over-yanked-candidate" ||
      migration.operationSemantics !== "protected-path-custody-strengthened" ||
      migration.stableKindIdsPreserved !== true ||
      JSON.stringify(migration.removedKindIds) !== "[]" ||
      JSON.stringify(migration.addedKindIds) !== "[]" ||
      JSON.stringify(migration.changedKindIds) !== JSON.stringify([
        "metrics.evaluation-evidence", "native.symbol-identity", "native.source-map"
      ]) ||
      migration.boundarySchemaChange !== "owner-only-to-kind-reader-writer-bound" ||
      JSON.stringify(migration.privacyHandoffAddedKindIdsFromAcceptedBaseline) !== JSON.stringify(successorAddedKindIds) ||
      migration.rejectedCandidateBehavior !== "never-select-or-accept" ||
      migration.staleReferenceBehavior !== "reject" ||
      migration.consumerAction !== "replace-any-1.3.0-candidate-ref-with-exact-1.3.1-ref"
    ) {
      fail("Successor compatibility and migration metadata is incomplete");
    }
    const expectedWriterChanges = [
      { id: "metrics.evaluation-evidence", fields: ["allowedWriters"], from: ["hlv", "ai-factory", "aifhub-adapter"], to: ["hlv"] },
      { id: "native.symbol-identity", fields: ["allowedWriters"], from: ["source-native", "lekalo"], to: ["source-native"] },
      { id: "native.source-map", fields: ["allowedWriters"], from: ["source-native", "lekalo"], to: ["source-native"] }
    ];
    if (canonicalJson(migration.changedFieldsByKind) !== canonicalJson(expectedWriterChanges)) {
      fail("Corrective successor writer-change metadata is incomplete");
    }
    const expectedBoundaryPolicy = {
      schemaVersion: "1.0.0",
      resolution: "all-matches-most-specific",
      specificityOrder: [
        "literal-prefix-segments-desc",
        "literal-segment-count-desc",
        "literal-character-count-desc",
        "segment-depth-desc",
        "wildcard-token-count-asc"
      ],
      equalSpecificity: "identical-policy-or-reject",
      kindBinding: "exact-boundary-artifactKinds",
      readerWriterBinding: "boundary-and-kind-intersection",
      actionAccess: {
        read: { source: "read" },
        write: { target: "write" },
        syncOneWay: { source: "read", target: "write" },
        syncBidirectionalProposal: { source: "read", target: "read" },
        claimReference: { source: "read", target: "read" },
        promote: { source: "read", target: "write" },
        adopt: { source: "read", target: "write" }
      }
    };
    if (canonicalJson(contract.boundaryPolicy) !== canonicalJson(expectedBoundaryPolicy)) {
      fail("Boundary specificity and action-access policy changed or is incomplete");
    }
    for (const kindId of [
      "source-native.source-code",
      "source-native.native-test",
      "fixture",
      "repository.identity",
      "native.symbol-identity",
      "native.source-map"
    ]) {
      if (JSON.stringify(kindMap.get(kindId).allowedWriters) !== JSON.stringify(["source-native"])) {
        fail(`${kindId} direct source-native evidence must be source-native-only writable`);
      }
    }
    if (JSON.stringify(kindMap.get("metrics.evaluation-evidence").allowedWriters) !== JSON.stringify(["hlv"])) {
      fail("HLV metrics evidence must be HLV-only writable");
    }
    for (const kind of kinds.slice(0, baselineKindIds.length)) {
      if (JSON.stringify(kind.allowedReaders) !== JSON.stringify(actors)) {
        fail(`${kind.id} must preserve 1.2.0 unrestricted-read behavior through explicit readers`);
      }
    }
  }

  if (contract.conflictPolicy?.lastWriterWins !== false) fail("Last-writer-wins must be forbidden");
  if (contract.projectRules?.absentLayerTransfersAuthority !== false) fail("An absent layer must not transfer authority");
  const actualSemanticSha256 = semanticSha256(contract);
  if (actualSemanticSha256 !== profile.semanticSha256) {
    fail(
      `Contract semantics do not match the closed ${profile.version} accepted profile: ${actualSemanticSha256}`
    );
  }

  return {
    owners: new Set(owners),
    actors: new Set(actors),
    kinds: kindMap,
    boundaries: boundaryContext.boundaries,
    conditionalBoundaries: boundaryContext.conditionalBoundaries,
    profile
  };
}

function actorDeniedCode(operation, label) {
  if (operation.action === "read") return "read.actor-not-allowed";
  if (operation.action === "write") return "write.actor-not-allowed";
  if (operation.action === "sync") return "sync.actor-not-allowed";
  if (operation.action === "claim") return "claim.actor-not-allowed";
  if (operation.action === "promote" && label === "target") return "promote.target-owner-required";
  if (operation.action === "adopt" && label === "target") return "adopt.kind-pair-or-actor-unsupported";
  return `${operation.action}.${label}-reader-not-allowed`;
}

function refAccessMode(operation, label) {
  if (operation.action === "read") return "read";
  if (operation.action === "write") return "write";
  if (operation.action === "claim") return "read";
  if (operation.action === "sync") {
    if (operation.direction === "bidirectional") return "read";
    return label === "source" ? "read" : "write";
  }
  return label === "source" ? "read" : "write";
}

function resolveMostSpecificBoundary(ref, operation, context) {
  const matching = context.boundaries.filter((boundary) => matches(ref.path, boundary.pattern));
  for (const boundary of context.conditionalBoundaries) {
    if (!matches(ref.path, boundary.pattern)) continue;
    const confirmed = operation.context?.[boundary.requiredContext] === boundary.requiredValue;
    if (confirmed) {
      matching.push(boundary);
    } else if (boundary.artifactKinds.includes(ref.artifactKind)) {
      return { denied: decision(false, "conditional-boundary-unconfirmed") };
    }
  }
  if (matching.length === 0) return { boundary: undefined };
  let bestSpecificity = matching[0].specificity;
  for (const candidate of matching.slice(1)) {
    if (compareSpecificity(candidate.specificity, bestSpecificity) > 0) bestSpecificity = candidate.specificity;
  }
  const best = matching.filter((candidate) => compareSpecificity(candidate.specificity, bestSpecificity) === 0);
  const policies = new Set(best.map((candidate) => boundaryPolicyIdentity(candidate)));
  if (policies.size !== 1) return { denied: decision(false, "boundary.ambiguous-policy") };
  return { boundary: best[0] };
}

function validateRef(ref, label, operation, context) {
  const kind = context.kinds.get(ref.artifactKind);
  if (!isValidLogicalPath(ref.path)) return decision(false, `${label}.path-invalid`);
  if (!kind.allowedPaths.some((pattern) => matches(ref.path, pattern))) {
    return decision(false, `${label}.path-not-allowed`);
  }
  const resolved = resolveMostSpecificBoundary(ref, operation, context);
  if (resolved.denied) {
    return decision(false, `${label}.${resolved.denied.code}`);
  }
  const boundary = resolved.boundary;
  if (boundary && boundary.owner !== kind.canonicalOwner) {
    return decision(false, `${label}.boundary-owner-mismatch`);
  }
  if (boundary && !boundary.artifactKinds.includes(kind.id)) {
    return decision(false, `${label}.boundary-kind-not-allowed`);
  }
  const access = refAccessMode(operation, label);
  const kindActors = access === "read" ? (kind.allowedReaders ?? [...context.actors]) : kind.allowedWriters;
  const boundaryActors = boundary ? (access === "read" ? boundary.readers : boundary.writers) : [...context.actors];
  if (!kindActors.includes(operation.actor) || !boundaryActors.includes(operation.actor)) {
    return decision(false, actorDeniedCode(operation, label));
  }
  return { allowed: true, code: `${label}.valid`, kind };
}

function validateWrite(operation, targetResult) {
  if (!targetResult.kind.allowedWriters.includes(operation.actor)) {
    return decision(false, "write.actor-not-allowed");
  }
  return decision(true, "write.allowed");
}

function validateOneWaySync(operation, sourceResult, targetResult, contract) {
  if (!targetResult.kind.allowedWriters.includes(operation.actor)) {
    return decision(false, "sync.actor-not-allowed");
  }

  const authoritativeTargets = new Set(["canonical", "direct-evidence"]);
  if (authoritativeTargets.has(targetResult.kind.classification)) {
    if (sourceResult.kind.id === "generated.code" && targetResult.kind.id === "source-native.source-code") {
      return decision(false, "generated.explicit-adoption-required");
    }
    if (
      sourceResult.kind.id === "lekalo.observed-model-draft" &&
      targetResult.kind.id === "lekalo.semantic-model"
    ) {
      return decision(false, "adoption.explicit-action-required");
    }
    if (["derived", "cached", "runtime-only"].includes(sourceResult.kind.classification)) {
      return decision(false, "sync.noncanonical-to-authoritative-forbidden");
    }
    return decision(false, "sync.authoritative-target-forbidden");
  }

  if (!contract.syncPolicy.allowedSourceClassifications.includes(sourceResult.kind.classification)) {
    return decision(false, "sync.source-classification-forbidden");
  }
  if (!contract.syncPolicy.allowedTargetClassifications.includes(targetResult.kind.classification)) {
    return decision(false, "sync.target-classification-forbidden");
  }
  return decision(true, "sync.allowed");
}

function validateEvidenceTrue(operation, policy, code) {
  for (const evidence of policy.requiredEvidence) {
    if (evidence === "provenance") {
      if (!hasOwn(operation.evidence, evidence)) return decision(false, code, evidence);
    } else if (operation.evidence[evidence] !== true) {
      return decision(false, code, evidence);
    }
  }
  return undefined;
}

function evaluate(operation, contract, context) {
  validateOperationShape(operation, context);

  if (operation.action === "read") {
    const source = validateRef(operation.source, "source", operation, context);
    if (!source.allowed) return source;
    const allowedReaders = source.kind.allowedReaders ?? [...context.actors];
    return allowedReaders.includes(operation.actor)
      ? decision(true, "read.allowed")
      : decision(false, "read.actor-not-allowed");
  }

  if (operation.action === "write") {
    const target = validateRef(operation.target, "target", operation, context);
    return target.allowed ? validateWrite(operation, target) : target;
  }

  const source = validateRef(operation.source, "source", operation, context);
  if (!source.allowed) return source;
  const target = validateRef(operation.target, "target", operation, context);
  if (!target.allowed) return target;

  if (operation.action === "claim") {
    if (operation.substitute !== contract.claimPolicy.requiredSubstitute) {
      return decision(false, "claim.substitution-forbidden");
    }
    return decision(true, "claim.reference-allowed");
  }

  if (operation.action === "promote") {
    if (source.kind.id !== "generated.code" || target.kind.id !== "source-native.source-code") {
      return decision(false, "promote.kind-pair-unsupported");
    }
    if (operation.actor !== target.kind.canonicalOwner) return decision(false, "promote.target-owner-required");
    const missingEvidence = validateEvidenceTrue(
      operation,
      contract.generatedPromotionPolicy,
      "promote.evidence-required"
    );
    return missingEvidence ?? decision(true, "promote.allowed");
  }

  if (operation.action === "adopt") {
    const policy = contract.brownfieldAdoptionPolicy;
    if (
      source.kind.id !== policy.sourceKind ||
      target.kind.id !== policy.targetKind ||
      operation.actor !== policy.actor
    ) {
      return decision(false, "adopt.kind-pair-or-actor-unsupported");
    }
    const missingEvidence = validateEvidenceTrue(operation, policy, "adopt.evidence-required");
    return missingEvidence ?? decision(true, "adopt.allowed");
  }

  if (operation.direction === "bidirectional") {
    if (operation.automatic) return decision(false, "sync.bidirectional-automatic-forbidden");
    for (const report of contract.syncPolicy.requiredReconciliationReports) {
      if (!operation.reports[report]) return decision(false, "sync.reports-required", report);
    }
    if (operation.writesCanonical) {
      return decision(false, "sync.reconciliation-canonical-write-forbidden");
    }
    return decision(true, "sync.reconciliation-proposal-allowed");
  }

  return validateOneWaySync(operation, source, target, contract);
}

function validateFixtureShape(fixture, path) {
  if (!isPlainObject(fixture)) fail(`Fixture in ${path} must be an object`);
  const keys = Object.keys(fixture).sort().join(",");
  if (keys !== "expected,id,operation") fail(`Fixture in ${path} must contain exactly id, operation, expected`);
  if (typeof fixture.id !== "string" || fixture.id.length === 0) fail(`Fixture in ${path} has invalid id`);
  if (!isPlainObject(fixture.expected)) fail(`Fixture ${fixture.id} has invalid expected object`);
  if (Object.keys(fixture.expected).sort().join(",") !== "allowed,code") {
    fail(`Fixture ${fixture.id} expected must contain exactly allowed and code`);
  }
  if (typeof fixture.expected.allowed !== "boolean" || typeof fixture.expected.code !== "string") {
    fail(`Fixture ${fixture.id} has invalid expected values`);
  }
}

async function runFixtureFile(path, expectedAllowed, contract, context) {
  const fixtures = await readJson(path);
  if (!Array.isArray(fixtures) || fixtures.length === 0) fail(`Fixture file ${path} must be a non-empty array`);
  for (const fixture of fixtures) validateFixtureShape(fixture, path);
  unique(fixtures.map((fixture) => fixture.id), `fixture IDs in ${path}`);

  let passed = 0;
  for (const fixture of fixtures) {
    if (fixture.expected.allowed !== expectedAllowed) fail(`Fixture ${fixture.id} is in the wrong fixture file`);
    const actual = evaluate(fixture.operation, contract, context);
    if (actual.allowed !== fixture.expected.allowed || actual.code !== fixture.expected.code) {
      fail(`Fixture ${fixture.id} failed: expected ${JSON.stringify(fixture.expected)}, got ${JSON.stringify(actual)}`);
    }
    passed += 1;
  }
  return passed;
}

async function runMalformedFixtureFile(path, contract, context) {
  const fixtures = await readJson(path);
  if (!Array.isArray(fixtures) || fixtures.length === 0) fail(`Fixture file ${path} must be a non-empty array`);
  unique(fixtures.map((fixture) => fixture.id), `malformed fixture IDs in ${path}`);

  let passed = 0;
  for (const fixture of fixtures) {
    if (!isPlainObject(fixture) || typeof fixture.id !== "string" || typeof fixture.expectedCode !== "string") {
      fail(`Malformed fixture entry in ${path} has invalid wrapper`);
    }
    try {
      evaluate(fixture.input, contract, context);
      fail(`Malformed fixture ${fixture.id} was accepted`);
    } catch (error) {
      if (!(error instanceof OperationShapeError) || error.code !== fixture.expectedCode) {
        fail(`Malformed fixture ${fixture.id}: expected ${fixture.expectedCode}, got ${error.code ?? error.message}`);
      }
    }
    passed += 1;
  }
  return passed;
}

function parseArgs(args) {
  const options = {
    contractPath: undefined,
    contractVersion: undefined,
    authorityRef: undefined,
    operationPath: undefined
  };
  for (let index = 0; index < args.length; index += 1) {
    if (args[index] === "--contract" && args[index + 1]) {
      if (options.contractPath !== undefined) fail("--contract may be provided only once");
      options.contractPath = resolve(process.cwd(), args[index + 1]);
      index += 1;
    } else if (args[index] === "--contract-version" && args[index + 1]) {
      if (options.contractVersion !== undefined) fail("--contract-version may be provided only once");
      options.contractVersion = args[index + 1];
      index += 1;
    } else if (args[index] === "--authority-ref" && args[index + 1]) {
      if (options.authorityRef !== undefined) fail("--authority-ref may be provided only once");
      options.authorityRef = args[index + 1];
      index += 1;
    } else if (args[index] === "--operation" && args[index + 1]) {
      if (options.operationPath !== undefined) fail("--operation may be provided only once");
      options.operationPath = resolve(process.cwd(), args[index + 1]);
      index += 1;
    } else {
      fail(`Unknown or incomplete argument: ${args[index]}`);
    }
  }
  if (options.contractPath !== undefined && options.authorityRef === undefined) {
    fail("--contract requires an exact --authority-ref; a path alone is not authority");
  }
  if (options.contractVersion !== undefined && options.authorityRef !== undefined) {
    fail("Use either --contract-version or --authority-ref, not both");
  }
  if (options.contractPath !== undefined && options.contractVersion !== undefined) {
    fail("--contract cannot be combined with --contract-version");
  }
  return options;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const { contract, context, profile } = await loadAuthoritySelection(options);

  if (options.operationPath) {
    const operation = await readJson(options.operationPath);
    const result = evaluate(operation, contract, context);
    process.stdout.write(`${JSON.stringify({
      authorityRef: {
        contractId: profile.contractId,
        version: profile.version,
        digest: profile.digest
      },
      ...result
    })}\n`);
    process.exitCode = result.allowed ? 0 : contract.operationProtocol.policyDeniedExitCode;
    return;
  }

  const allowed = await runFixtureFile(defaultAllowedPath, true, contract, context);
  const forbidden = await runFixtureFile(defaultForbiddenPath, false, contract, context);
  const malformed = await runMalformedFixtureFile(defaultMalformedPath, contract, context);
  process.stdout.write(
    `authority contract ${exactAuthorityRef(profile)}: PASS (${allowed} allowed, ${forbidden} forbidden, ${malformed} malformed fixtures)\n`
  );
}

async function loadAuthoritySelection(options) {
  const manifestContext = await loadTrustedManifest();
  const profile = selectProfile(options, manifestContext);
  const selectedContractPath = options.contractPath ?? resolve(root, profile.path);
  const { bytes: contractBytes, value: contract } = await readExactJson(selectedContractPath);
  const actualDigest = `sha256:${sha256Bytes(contractBytes)}`;
  if (actualDigest !== profile.digest) {
    fail(`Contract exact bytes do not match ${exactAuthorityRef(profile)}; got ${actualDigest}`);
  }
  const context = validateContract(contract, profile);
  return { contract, context, profile };
}

export async function loadAcceptedAuthority(version = "1.3.1") {
  return loadAuthoritySelection({
    contractPath: undefined,
    contractVersion: version,
    authorityRef: undefined,
    operationPath: undefined
  });
}

export { boundarySpecificity, compareSpecificity, evaluate, matches, patternsOverlap };

if (process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) {
  main().catch((error) => {
    process.stderr.write(`authority contract: FAIL: ${error.message}\n`);
    process.exitCode = 1;
  });
}
