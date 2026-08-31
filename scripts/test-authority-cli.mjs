#!/usr/bin/env node

import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const checker = resolve(root, "scripts/check-authority.mjs");
const fixturesRoot = resolve(root, "tests/fixtures/authority");

function fail(message) {
  throw new Error(message);
}

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

const tempRoot = mkdtempSync(resolve(tmpdir(), "lekalo-authority-cli-"));

function runInput(id, input, version) {
  const path = resolve(tempRoot, `${id}.json`);
  writeFileSync(path, `${JSON.stringify(input)}\n`, "utf8");
  return spawnSync(process.execPath, [checker, "--contract-version", version, "--operation", path], {
    cwd: root,
    encoding: "utf8",
    windowsHide: true
  });
}

function assertDecision(id, input, version, expectedExit, expectedAllowed, expectedCode) {
  const result = runInput(id, input, version);
  if (result.status !== expectedExit) {
    fail(`${id}: expected exit ${expectedExit}, got ${result.status}; stderr=${result.stderr.trim()}`);
  }
  const output = JSON.parse(result.stdout);
  if (output.allowed !== expectedAllowed || output.code !== expectedCode) {
    fail(`${id}: unexpected decision ${result.stdout.trim()}`);
  }
  process.stdout.write(`PASS ${version}-${id}: exit ${expectedExit}, ${expectedCode}\n`);
}

try {
  const allowed = readJson(resolve(fixturesRoot, "allowed.json"));
  const forbidden = readJson(resolve(fixturesRoot, "forbidden.json"));
  const malformed = readJson(resolve(fixturesRoot, "malformed.json"));

  const allowedIds = [
    "hlv-evidence-copied-one-way-to-ai-factory-envelope",
    "claim-remains-reference-only-with-substitute-false",
    "hlv-writes-own-greenfield-contract",
    "unrelated-root-project-yaml-remains-source-native",
    "lekalo-explicitly-adopts-reviewed-brownfield-draft"
  ];
  const deniedIds = [
    "generated-code-silently-becomes-source",
    "brownfield-draft-cannot-generic-sync-to-canonical-model",
    "claim-cannot-substitute-generated-code-for-source",
    "claim-cannot-substitute-brownfield-draft-for-model",
    "claim-cannot-substitute-generated-rule-for-plan",
    "generated-rule-cannot-generic-sync-to-execution-plan",
    "lekalo-cache-cannot-generic-sync-to-canonical-model",
    "runtime-state-cannot-generic-sync-to-execution-plan",
    "openspec-trailing-dot-alias-rejected-mixed-case",
    "ntfs-ads-colon-rejected",
    "dos-device-name-rejected",
    "short-name-like-segment-rejected",
    "unconfirmed-hlv-root-project-contract-rejected",
    "hlv-root-project-contract-without-discovery-context-rejected"
  ];

  for (const version of ["1.2.0", "1.3.1"]) {
    for (const id of allowedIds) {
      const fixture = allowed.find((candidate) => candidate.id === id);
      if (!fixture) fail(`Required allowed CLI probe ${id} is missing`);
      assertDecision(`allowed-${id}`, fixture.operation, version, 0, fixture.expected.allowed, fixture.expected.code);
    }
    for (const id of deniedIds) {
      const fixture = forbidden.find((candidate) => candidate.id === id);
      if (!fixture) fail(`Required denied CLI probe ${id} is missing`);
      assertDecision(`denied-${id}`, fixture.operation, version, 3, fixture.expected.allowed, fixture.expected.code);
    }

    for (const fixture of malformed) {
      const result = runInput(`malformed-${version}-${fixture.id}`, fixture.input, version);
      if (result.status !== 1) {
        fail(`${version}-${fixture.id}: expected malformed exit 1, got ${result.status}; stdout=${result.stdout.trim()}`);
      }
      if (!result.stderr.includes(fixture.expectedCode)) {
        fail(`${version}-${fixture.id}: stderr did not contain ${fixture.expectedCode}: ${result.stderr.trim()}`);
      }
      process.stdout.write(`PASS ${version}-malformed-${fixture.id}: exit 1, ${fixture.expectedCode}\n`);
    }
  }

  const successorAllowed = [
    ["fixture-source-native-write", {
      action: "write", actor: "source-native",
      target: { artifactKind: "fixture", path: "tests/fixtures/authority/clean.json" }
    }, "write.allowed"],
    ["source-map-source-native-write", {
      action: "write", actor: "source-native",
      target: { artifactKind: "native.source-map", path: "src/maps/application.map" }
    }, "write.allowed"],
    ["repository-identity-source-native-write", {
      action: "write", actor: "source-native",
      target: { artifactKind: "repository.identity", path: ".source-native/repository/id.json" }
    }, "write.allowed"],
    ["metrics-hlv-write", {
      action: "write", actor: "hlv",
      target: { artifactKind: "metrics.evaluation-evidence", path: ".hlv/evidence/metrics/run.json" }
    }, "write.allowed"],
    ["fixture-ai-factory-read", {
      action: "read", actor: "ai-factory",
      source: { artifactKind: "fixture", path: "tests/fixtures/authority/clean.json" }
    }, "read.allowed"],
    ["metrics-ai-factory-read", {
      action: "read", actor: "ai-factory",
      source: { artifactKind: "metrics.evaluation-evidence", path: ".hlv/evidence/metrics/run.json" }
    }, "read.allowed"]
  ];
  const successorDenied = [
    ["fixture-generated-code-relabel", {
      action: "write", actor: "lekalo",
      target: { artifactKind: "generated.code", path: "tests/fixtures/authority/poison.json" }
    }, "target.boundary-kind-not-allowed"],
    ["fixture-source-map-relabel", {
      action: "write", actor: "lekalo",
      target: { artifactKind: "native.source-map", path: "tests/fixtures/maps/poison.map" }
    }, "target.boundary-kind-not-allowed"],
    ["repository-generated-code-relabel", {
      action: "write", actor: "lekalo",
      target: { artifactKind: "generated.code", path: ".source-native/repository/id.json" }
    }, "target.boundary-kind-not-allowed"],
    ["repository-source-map-relabel", {
      action: "write", actor: "lekalo",
      target: { artifactKind: "native.source-map", path: ".source-native/repository/id.map" }
    }, "target.boundary-kind-not-allowed"],
    ["metrics-cross-domain-writer", {
      action: "write", actor: "ai-factory",
      target: { artifactKind: "metrics.evaluation-evidence", path: ".hlv/evidence/metrics/run.json" }
    }, "write.actor-not-allowed"],
    ["metrics-validation-result-relabel", {
      action: "write", actor: "ai-factory",
      target: { artifactKind: "hlv.validation-result", path: ".hlv/evidence/metrics/run.json" }
    }, "target.boundary-kind-not-allowed"],
    ["fixture-wrong-writer-exact-kind", {
      action: "write", actor: "lekalo",
      target: { artifactKind: "fixture", path: "tests/fixtures/authority/poison.json" }
    }, "write.actor-not-allowed"],
    ["repository-wrong-writer-exact-kind", {
      action: "write", actor: "lekalo",
      target: { artifactKind: "repository.identity", path: ".source-native/repository/id.json" }
    }, "write.actor-not-allowed"],
    ["source-map-lekalo-writer", {
      action: "write", actor: "lekalo",
      target: { artifactKind: "native.source-map", path: "src/maps/application.map" }
    }, "write.actor-not-allowed"],
    ["symbol-identity-lekalo-writer", {
      action: "write", actor: "lekalo",
      target: { artifactKind: "native.symbol-identity", path: ".source-native/symbols/application.json" }
    }, "write.actor-not-allowed"],
    ["fixture-generated-code-read-relabel", {
      action: "read", actor: "lekalo",
      source: { artifactKind: "generated.code", path: "tests/fixtures/authority/poison.json" }
    }, "source.boundary-kind-not-allowed"],
    ["repository-source-map-claim-source-relabel", {
      action: "claim", actor: "lekalo", substitute: false,
      source: { artifactKind: "native.source-map", path: ".source-native/repository/id.map" },
      target: { artifactKind: "openspec.requirement", path: "openspec/specs/application.md" }
    }, "source.boundary-kind-not-allowed"]
  ];
  for (const [id, operation, code] of successorAllowed) {
    assertDecision(`successor-${id}`, operation, "1.3.1", 0, true, code);
  }
  for (const [id, operation, code] of successorDenied) {
    assertDecision(`successor-${id}`, operation, "1.3.1", 3, false, code);
  }

  const yanked = spawnSync(process.execPath, [checker, "--contract-version", "1.3.0"], {
    cwd: root, encoding: "utf8", windowsHide: true
  });
  if (yanked.status !== 1 || !yanked.stderr.includes("rejected/yanked")) {
    fail(`1.3.0 yanked lifecycle was not enforced: status=${yanked.status}; stderr=${yanked.stderr.trim()}`);
  }
  process.stdout.write("PASS 1.3.0-yanked-selection: exit 1 before contract evaluation\n");

  process.stdout.write(
    `authority CLI: PASS (2 accepted versions x ${allowedIds.length} allowed, ${deniedIds.length} denied, ${malformed.length} malformed; successor ${successorAllowed.length} allowed/${successorDenied.length} denied; yanked exit 1)\n`
  );
} finally {
  rmSync(tempRoot, { recursive: true, force: true });
}
