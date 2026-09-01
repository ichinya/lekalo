#!/usr/bin/env node

// Adversarial conformance suite for the issue #4 structure contract.
// CLI behavior is exercised through real subprocesses; temporary filesystem
// trees cover junction/symlink containment and authority-home compatibility.

import { spawnSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, readdir, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { checkPathGrammar, checkSelectionGrammar, RUNTIME_AUTHORITY_HOMES } from "./check-structure.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const checker = join(root, "scripts", "check-structure.mjs");
const fixturesDir = join(root, "tests", "fixtures", "structure");
const authorityPath = join(root, "contracts", "authority-matrix.v1.3.1.json");

const failures = [];
let fixtureCases = 0;
let subprocessCases = 0;
let pathCases = 0;
let importCases = 0;
let filesystemCases = 0;
let authorityCases = 0;
let cliActionCases = 0;

function run(args, env = {}, cwd = root) {
  const childEnv = { ...process.env };
  delete childEnv.LEKALO_PROJECT;
  const result = spawnSync(process.execPath, [checker, ...args], {
    cwd,
    encoding: "utf8",
    env: { ...childEnv, ...env }
  });
  subprocessCases += 1;
  return { code: result.status, stdout: result.stdout, stderr: result.stderr };
}

function parseJson(text) {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

function check(name, condition, detail) {
  if (!condition) failures.push({ case: name, detail });
}

function expectOutcome(name, result, outcome, reason = null) {
  const expectedCode = outcome === "valid" ? 0 : outcome === "denied" ? 3 : 1;
  const text = outcome === "invalid" ? result.stderr : result.stdout;
  const doc = parseJson(text);
  check(`${name}:exit`, result.code === expectedCode, { expectedCode, result });
  check(`${name}:status`, doc?.status === outcome, { expected: outcome, actual: doc?.status, text });
  if (reason !== null) check(`${name}:reason`, doc?.reasonCodes?.[0] === reason, { expected: reason, actual: doc?.reasonCodes?.[0], text });
  check(`${name}:single-stream`, outcome === "invalid" ? result.stdout === "" : result.stderr === "", result);
}

async function makeProject(parent, name = "project") {
  const project = join(parent, name);
  await mkdir(join(project, "lekalo", "modules"), { recursive: true });
  await writeFile(join(project, "lekalo", "project.yaml"), "opaque marker\n", "utf8");
  return project;
}

// --- committed fixture conformance ---------------------------------------------

const conformance = run([]);
fixtureCases += 1;
expectOutcome("conformance", conformance, "valid");
const conformanceDoc = parseJson(conformance.stdout);
check("conformance-counts", JSON.stringify(conformanceDoc?.fixtures) === JSON.stringify({ valid: 14, malformed: 1, denied: 6 }), conformanceDoc);

const fixtureDirs = (await readdir(fixturesDir, { withFileTypes: true }))
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .filter((name) => name.startsWith("valid-") || name.startsWith("invalid-"))
  .sort();

for (const name of fixtureDirs) {
  fixtureCases += 1;
  const result = run(["--project", `tests/fixtures/structure/${name}`]);
  if (name.startsWith("valid-")) {
    expectOutcome(`fixture:${name}`, result, "valid");
    continue;
  }
  const expectation = parseJson(await readFile(join(fixturesDir, name, "expect.json"), "utf8"));
  check(`fixture:${name}:expect`, expectation !== null, name);
  if (expectation !== null) expectOutcome(`fixture:${name}`, result, expectation.outcome, expectation.reasonCodes[0]);
}

// --- root discovery, relative selectors and envelope custody -------------------

const discoveryCases = [
  ["tests/fixtures/structure/discovery-nearest/apps/web/src", "tests/fixtures/structure/discovery-nearest"],
  ["tests/fixtures/structure/discovery-nearest", "tests/fixtures/structure/discovery-nearest"],
  ["tests/fixtures/structure/discovery-sibling-roots/apps/web/src", "tests/fixtures/structure/discovery-sibling-roots/apps/web"],
  ["tests/fixtures/structure/discovery-sibling-roots/apps/api", "tests/fixtures/structure/discovery-sibling-roots/apps/api"]
];
for (const [from, expected] of discoveryCases) {
  const result = run(["--find-root", "--from", from]);
  expectOutcome(`discovery:${from}`, result, "valid");
  check(`discovery:${from}:root`, parseJson(result.stdout)?.root === resolve(root, expected), { expected: resolve(root, expected), actual: parseJson(result.stdout)?.root });
}

expectOutcome("discovery:none", run(["--find-root", "--from", "tests/fixtures/structure/discovery-none/a/b"]), "invalid", "structure.root-not-found");
expectOutcome("discovery:file", run(["--find-root", "--from", "README.md"]), "invalid", "structure.from-not-directory");
expectOutcome("env-project", run([], { LEKALO_PROJECT: "tests/fixtures/structure/valid-minimal" }), "valid");
expectOutcome("project-missing", run(["--project", "tests/fixtures/structure/does-not-exist"]), "invalid", "structure.project-not-directory");

const absoluteFixture = resolve(fixturesDir, "valid-minimal");
const absoluteProject = run(["--project", absoluteFixture]);
expectOutcome("project:absolute", absoluteProject, "invalid", "structure.selection-absolute");
check("project:absolute:no-echo", !absoluteProject.stderr.includes(absoluteFixture), absoluteProject.stderr);
const absoluteEnv = run([], { LEKALO_PROJECT: absoluteFixture });
expectOutcome("env:absolute", absoluteEnv, "invalid", "structure.selection-absolute");
check("env:absolute:no-echo", !absoluteEnv.stderr.includes(absoluteFixture), absoluteEnv.stderr);
expectOutcome("project:traversal", run(["--project", "../lekalo"]), "invalid", "structure.selection-traversal");
expectOutcome("usage", run(["--wat"]), "invalid", "structure.usage");

// --- strict CLI action discriminator -------------------------------------------

const validProjectSelector = "tests/fixtures/structure/valid-minimal";
const actionUsageCases = [
  ["action:from-without-discovery", ["--from", "README.md"]],
  ["action:path-and-project", ["--validate-path", "lekalo", "--project", validProjectSelector]],
  ["action:discovery-and-path", ["--find-root", "--validate-path", "lekalo"]],
  ["action:project-and-discovery", ["--project", validProjectSelector, "--find-root"]],
  ["action:duplicate-from", ["--find-root", "--from", ".", "--from", validProjectSelector]],
  ["action:duplicate-project", ["--project", validProjectSelector, "--project", validProjectSelector]],
  ["action:duplicate-discovery", ["--find-root", "--find-root"]],
  ["action:duplicate-path", ["--validate-path", "lekalo", "--validate-path", "lekalo/modules"]],
  ["action:missing-project-value", ["--project"]],
  ["action:missing-from-value", ["--find-root", "--from"]],
  ["action:missing-path-value", ["--validate-path"]]
];
for (const [name, args] of actionUsageCases) {
  cliActionCases += 1;
  expectOutcome(name, run(args), "invalid", "structure.usage");
}

const unrelatedEnv = { LEKALO_PROJECT: "C:/must-not-be-inherited" };
cliActionCases += 1;
expectOutcome("action:path-ignores-env-project", run(["--validate-path", "lekalo"], unrelatedEnv), "valid");
cliActionCases += 1;
const discoveryIgnoresEnv = run(["--find-root", "--from", "tests/fixtures/structure/discovery-nearest"], unrelatedEnv);
expectOutcome("action:discovery-ignores-env-project", discoveryIgnoresEnv, "valid");
check("action:discovery-ignores-env-project:root", parseJson(discoveryIgnoresEnv.stdout)?.root === resolve(root, "tests/fixtures/structure/discovery-nearest"), discoveryIgnoresEnv);
cliActionCases += 1;
expectOutcome("action:explicit-project-overrides-env", run(["--project", validProjectSelector], unrelatedEnv), "valid");
cliActionCases += 1;
expectOutcome("action:from-with-env-still-usage", run(["--from", "."], { LEKALO_PROJECT: validProjectSelector }), "invalid", "structure.usage");

// --- path and selector grammar oracles -----------------------------------------

const pathMatrix = [
  ["lekalo", null], ["lekalo/modules/a", null], ["modules/123", null], ["a.b/c-d_e/1.2.3", null],
  ["node-typescript.yaml", null], ["ir/manifests/report-1", null], ["consoles", null], ["com9x", null],
  ["", "structure.path-empty"], ["/abs", "structure.path-absolute"], ["//host/share", "structure.path-absolute"],
  ["\\\\server\\share", "structure.path-absolute"], ["c:/x", "structure.path-absolute"], ["c:x", "structure.path-absolute"],
  ["~/x", "structure.path-absolute"], ["file:///etc/x", "structure.path-absolute"], ["a\\b", "structure.path-escape"],
  ["%2e%2e/x", "structure.path-escape"], ["a/%41", "structure.path-escape"], ["a:b", "structure.path-absolute"],
  ["a/b:c", "structure.path-escape"], ["a//b", "structure.path-segment"], ["a/", "structure.path-segment"],
  ["A/b", "structure.path-case"], [".", "structure.path-traversal"], ["..", "structure.path-traversal"],
  ["a/../b", "structure.path-traversal"], ["a.", "structure.path-traversal"], ["a ", "structure.path-traversal"],
  ["CON", "structure.path-device"], ["con.txt", "structure.path-device"], ["conin$", "structure.path-device"],
  ["clock$", "structure.path-device"], ["progra~1", "structure.path-short-name"], ["foo~9.txt", "structure.path-short-name"],
  ["e\u0301", "structure.path-not-nfc"], ["a/" + "x".repeat(65), "structure.path-segment"]
];

for (const digit of ["¹", "²", "³", "⁴", "⁵", "⁶", "⁷", "⁸", "⁹", "₁", "₂", "₃", "₄", "₅", "₆", "₇", "₈", "₉", "①", "②", "③", "④", "⑤", "⑥", "⑦", "⑧", "⑨"]) {
  pathMatrix.push([`com${digit}.txt`, "structure.path-device"], [`lpt${digit}`, "structure.path-device"]);
}
pathMatrix.push(["ｃｏｍ１.txt", "structure.path-device"], ["ｌｐｔ９", "structure.path-device"]);

for (const [candidate, expected] of pathMatrix) {
  pathCases += 1;
  const result = run(["--validate-path", candidate]);
  if (expected === null) expectOutcome(`path:${JSON.stringify(candidate)}`, result, "valid");
  else expectOutcome(`path:${JSON.stringify(candidate)}`, result, "invalid", expected);
  importCases += 1;
  check(`path-import:${JSON.stringify(candidate)}`, checkPathGrammar(candidate) === expected, { expected, actual: checkPathGrammar(candidate) });
}

const selectionMatrix = [
  [".", null], ["apps/API project", null], ["./apps/api", null], ["", "structure.selection-empty"],
  ["C:/repo", "structure.selection-absolute"], ["/repo", "structure.selection-absolute"], ["../repo", "structure.selection-traversal"],
  ["apps/../repo", "structure.selection-traversal"], ["apps\\repo", "structure.selection-escape"],
  ["progra~1/repo", "structure.selection-short-name"], ["con/repo", "structure.selection-device"]
];
for (const [candidate, expected] of selectionMatrix) {
  importCases += 1;
  check(`selection-import:${JSON.stringify(candidate)}`, checkSelectionGrammar(candidate) === expected, { expected, actual: checkSelectionGrammar(candidate) });
}

// --- accepted authority 1.3.1 runtime homes ------------------------------------

const authority = JSON.parse(await readFile(authorityPath, "utf8"));
const authorityHomes = authority.artifactKinds
  .filter((kind) => kind.canonicalOwner === "lekalo")
  .flatMap((kind) => kind.allowedPaths.filter((path) => path.startsWith(".lekalo/")).map((pattern) => ({ artifactKind: kind.id, pattern })))
  .sort((left, right) => `${left.artifactKind}:${left.pattern}` < `${right.artifactKind}:${right.pattern}` ? -1 : 1);
const declaredHomes = [...RUNTIME_AUTHORITY_HOMES].sort((left, right) => `${left.artifactKind}:${left.pattern}` < `${right.artifactKind}:${right.pattern}` ? -1 : 1);
authorityCases += 1;
check("authority:exact-runtime-homes", JSON.stringify(declaredHomes) === JSON.stringify(authorityHomes), { declaredHomes, authorityHomes });

const tempRoots = [];
try {
  const authorityTemp = await mkdtemp(join(tmpdir(), "lekalo-structure-authority-"));
  tempRoots.push(authorityTemp);
  const authorityProject = await makeProject(authorityTemp);
  for (const home of authorityHomes) {
    const representative = home.pattern.replace("/**", "/sample.json");
    const diskPath = join(authorityProject, ...representative.split("/"));
    await mkdir(dirname(diskPath), { recursive: true });
    await writeFile(diskPath, "{}\n", "utf8");
    authorityCases += 1;
  }
  expectOutcome("authority:all-homes-valid", run(["--project", "."], {}, authorityProject), "valid");

  const wrongPlacements = [
    ["cache.sqlite", "structure.runtime-unexpected-entry"],
    ["ir/sample.json", "structure.runtime-unexpected-entry"],
    ["consumer/exports/sample.json", "structure.runtime-unexpected-entry"],
    ["privacy/decisions/exports/sample.json", "structure.runtime-unexpected-entry"]
  ];
  for (const [logical, reason] of wrongPlacements) {
    const wrongTemp = await mkdtemp(join(tmpdir(), "lekalo-structure-wrong-home-"));
    tempRoots.push(wrongTemp);
    const project = await makeProject(wrongTemp);
    const diskPath = join(project, ".lekalo", ...logical.split("/"));
    await mkdir(dirname(diskPath), { recursive: true });
    await writeFile(diskPath, "{}\n", "utf8");
    authorityCases += 1;
    expectOutcome(`authority:wrong-home:${logical}`, run(["--project", "."], {}, project), "denied", reason);
  }

  // Contents are deliberately opaque at #4. These bytes are for #5/#7/#10.
  const opaqueTemp = await mkdtemp(join(tmpdir(), "lekalo-structure-opaque-"));
  tempRoots.push(opaqueTemp);
  const opaqueProject = await makeProject(opaqueTemp);
  await writeFile(join(opaqueProject, "lekalo", "project.yaml"), "not: [valid: yaml\n", "utf8");
  await mkdir(join(opaqueProject, "lekalo", "modules", "catalog"), { recursive: true });
  await writeFile(join(opaqueProject, "lekalo", "modules", "catalog", "module.yaml"), "schema_version: future/model\nimports: ???\n", "utf8");
  await writeFile(join(opaqueProject, "lekalo.lock"), "schema_version: lekalo/lock/v1\ncore: next\nadapters: rich downstream envelope\n", "utf8");
  filesystemCases += 1;
  expectOutcome("opaque-downstream-contents", run(["--project", "."], {}, opaqueProject), "valid");

  // A child project outside governed trees is legal; nearest-root-wins makes
  // the inner root independent. Roots inside lekalo/.lekalo remain fixtures.
  const nestedTemp = await mkdtemp(join(tmpdir(), "lekalo-structure-nested-"));
  tempRoots.push(nestedTemp);
  const outer = await makeProject(nestedTemp, "outer");
  const inner = await makeProject(join(outer, "packages"), "inner");
  filesystemCases += 2;
  expectOutcome("nested:outer-valid", run(["--project", "."], {}, outer), "valid");
  const nestedDiscovery = run(["--find-root", "--from", "."], {}, inner);
  expectOutcome("nested:inner-discovery", nestedDiscovery, "valid");
  check("nested:nearest-root", parseJson(nestedDiscovery.stdout)?.root === inner, { expected: inner, actual: parseJson(nestedDiscovery.stdout)?.root });

  // Real junction/symlink probes. Windows junctions need no developer-mode
  // privilege; POSIX uses directory symlinks through the same API.
  const linkType = process.platform === "win32" ? "junction" : "dir";

  const canonicalLinkTemp = await mkdtemp(join(tmpdir(), "lekalo-structure-canonical-link-"));
  tempRoots.push(canonicalLinkTemp);
  const canonicalProject = join(canonicalLinkTemp, "project");
  const externalCanonical = await makeProject(canonicalLinkTemp, "external-canonical");
  await mkdir(canonicalProject, { recursive: true });
  await symlink(join(externalCanonical, "lekalo"), join(canonicalProject, "lekalo"), linkType);
  filesystemCases += 1;
  expectOutcome("physical:canonical-link", run(["--project", "."], {}, canonicalProject), "denied", "structure.path-link");

  const runtimeLinkTemp = await mkdtemp(join(tmpdir(), "lekalo-structure-runtime-link-"));
  tempRoots.push(runtimeLinkTemp);
  const runtimeProject = await makeProject(runtimeLinkTemp);
  const externalCache = join(runtimeLinkTemp, "external-cache");
  await mkdir(externalCache, { recursive: true });
  await mkdir(join(runtimeProject, ".lekalo"), { recursive: true });
  await symlink(externalCache, join(runtimeProject, ".lekalo", "cache"), linkType);
  filesystemCases += 1;
  expectOutcome("physical:runtime-link", run(["--project", "."], {}, runtimeProject), "denied", "structure.path-link");

  const aliasTemp = await mkdtemp(join(tmpdir(), "lekalo-structure-selector-alias-"));
  tempRoots.push(aliasTemp);
  const aliasTarget = await makeProject(aliasTemp, "target");
  await symlink(aliasTarget, join(aliasTemp, "alias"), linkType);
  filesystemCases += 1;
  expectOutcome("physical:selector-alias", run(["--project", "alias"], {}, aliasTemp), "denied", "structure.selection-alias");
} catch (error) {
  failures.push({ case: "temporary-filesystem-probes", detail: { name: error?.name, code: error?.code, message: error?.message } });
} finally {
  for (const tempRoot of tempRoots.reverse()) await rm(tempRoot, { recursive: true, force: true });
}

// --- deterministic envelopes ----------------------------------------------------

const first = run(["--project", "tests/fixtures/structure/valid-standalone"]);
const second = run(["--project", "tests/fixtures/structure/valid-standalone"]);
check("determinism:project", first.code === second.code && first.stdout === second.stdout && first.stderr === second.stderr, { first, second });
const confFirst = run([]);
const confSecond = run([]);
check("determinism:conformance", confFirst.stdout === confSecond.stdout && confFirst.stderr === confSecond.stderr, { confFirst, confSecond });

if (failures.length > 0) {
  process.stderr.write(JSON.stringify({ ok: false, failures }, null, 2) + "\n");
  process.exitCode = 1;
} else {
  process.stdout.write(JSON.stringify({ ok: true, fixtureCases, subprocessCases, pathCases, importCases, filesystemCases, authorityCases, cliActionCases, fixtures: conformanceDoc.fixtures }, null, 2) + "\n");
}
