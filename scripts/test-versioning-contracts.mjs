#!/usr/bin/env node
// Issue #9 versioning contract gate.
//
// Validates the embedded version-registry artifact against its invariants
// with an independent implementation (the Rust side validates the same
// rules at build/run time), cross-checks the artifact against the compiled
// Rust constants it must stay in sync with, and proves the golden CLI
// projections match a projection regenerated here.
//
// The gate is hermetic: it needs only Node itself and the repository
// files. No dependency or lockfile belongs to this contract repo.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const failures = [];

const read = (relative) => readFileSync(resolve(root, relative), "utf8");
const fail = (caseName, detail) => failures.push({ case: caseName, detail });

// The closed canonical SemVer spelling: MAJOR.MINOR.PATCH with an optional
// prerelease and nothing else.
const CANONICAL = /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$/;
const ALIAS = /^v(?:0|[1-9][0-9]*)$/;

const precedence = (text) => {
  const [core, pre = ""] = text.split("-");
  const [major, minor, patch] = core.split(".").map(Number);
  if (pre) {
    const left = pre.split(".");
    return [major, minor, patch, 1, left];
  }
  return [major, minor, patch, 0];
};
const compare = (left, right) => {
  const a = precedence(left);
  const b = precedence(right);
  for (let index = 0; index < 4; index += 1) {
    if (a[index] !== b[index]) return a[index] < b[index] ? -1 : 1;
  }
  return 0;
};

// ---------------------------------------------------------------------------
// 1. The artifact parses and carries the closed top-level shape.
// ---------------------------------------------------------------------------
const registry = JSON.parse(read("crates/lekalo-core/src/versioning/contracts/version-registry.v1.2.0.json"));

if (registry.registry !== "dev.lekalo.version-registry") {
  fail("identity", `unexpected registry identity ${registry.registry}`);
}
if (registry.registryVersion !== "1.2.0" || !CANONICAL.test(registry.registryVersion)) {
  fail("registryVersion", `unexpected registry version ${registry.registryVersion}`);
}

const FAMILY_NAMES = ["model", "ir", "protocol"];
const familyNames = Object.keys(registry.families);
if (JSON.stringify(familyNames) !== JSON.stringify(FAMILY_NAMES)) {
  fail("families", `unexpected family set ${familyNames.join(",")}`);
}

const CLASSIFICATIONS = new Set(["breaking", "additive", "behavioral"]);
const IMPACTS = new Set(["none", "changed-modules", "all"]);
const STATES = new Set(["supported", "deprecated", "retired"]);

for (const name of FAMILY_NAMES) {
  const family = registry.families[name];
  const versions = family.versions.map((record) => record.version);
  const sorted = [...versions].sort(compare);
  if (JSON.stringify(versions) !== JSON.stringify(sorted)) {
    fail(`${name}:versions`, "versions are not strictly ascending");
  }
  if (new Set(versions).size !== versions.length) {
    fail(`${name}:versions`, "duplicate versions");
  }
  for (const record of family.versions) {
    if (!CANONICAL.test(record.version)) {
      fail(`${name}:canonical`, `${record.version} is not canonical`);
    }
    if (!STATES.has(record.state)) {
      fail(`${name}:state`, `unknown state ${record.state}`);
    }
    if (!CLASSIFICATIONS.has(record.classification)) {
      fail(`${name}:classification`, `unknown classification ${record.classification}`);
    }
    if (typeof record.reason !== "string" || record.reason.length === 0 || record.reason.length > 512) {
      fail(`${name}:reason`, `version ${record.version} needs a bounded written reason`);
    }
    if (record.state === "deprecated") {
      if (compare(record.deprecatedSince, record.version) <= 0) {
        fail(`${name}:deprecation`, `${record.version} deprecated since must be later`);
      }
      if (compare(record.retirementNotBefore, record.version) <= 0) {
        fail(`${name}:retirement`, `${record.version} retirement threshold must exceed the version`);
      }
    }
  }
  if (family.versions.length === 0 && family.current !== null) {
    fail(`${name}:current`, "unpublished family must carry no current version");
  }
  if (family.versions.length > 0) {
    if (family.current === null) {
      fail(`${name}:current`, "published family must declare a current version");
    } else if (!versions.includes(family.current)) {
      fail(`${name}:current`, `current ${family.current} is not registered`);
    }
  }
  for (const alias of family.aliases) {
    if (!ALIAS.test(alias.alias)) {
      fail(`${name}:alias`, `alias ${alias.alias} is not a v-major token`);
    }
    if (!versions.includes(alias.version)) {
      fail(`${name}:alias`, `alias ${alias.alias} targets unregistered ${alias.version}`);
    }
  }
  const ids = family.migrations.map((edge) => edge.id);
  if (new Set(ids).size !== ids.length) {
    fail(`${name}:migrations`, "duplicate edge ids");
  }
  for (const edge of family.migrations) {
    if (!versions.includes(edge.from) || !versions.includes(edge.to)) {
      fail(`${name}:migrations`, `edge ${edge.id} has unregistered endpoints`);
    }
    if (compare(edge.from, edge.to) >= 0) {
      fail(`${name}:migrations`, `edge ${edge.id} must strictly ascend`);
    }
    if (!CLASSIFICATIONS.has(edge.classification)) {
      fail(`${name}:migrations`, `edge ${edge.id} has an unknown classification`);
    }
    if (!IMPACTS.has(edge.regenerationImpact)) {
      fail(`${name}:migrations`, `edge ${edge.id} has an unknown regeneration impact`);
    }
  }
  // Unique simple paths between every ordered pair; ambiguity is a
  // registry fault, never a runtime choice.
  const adjacency = new Map(family.migrations.map((edge) => [edge.from, []]));
  for (const edge of family.migrations) {
    adjacency.get(edge.from).push([edge.to, edge.id]);
  }
  for (const source of versions) {
    const paths = new Map();
    const frontier = [[source, []]];
    while (frontier.length > 0) {
      const [version, chain] = frontier.pop();
      for (const [target, id] of adjacency.get(version) ?? []) {
        const next = [...chain, id];
        if (!paths.has(target)) paths.set(target, []);
        paths.get(target).push(next);
        frontier.push([target, next]);
      }
    }
    for (const [target, chains] of paths) {
      if (chains.length > 1) {
        fail(`${name}:paths`, `${chains.length} paths from ${source} to ${target}`);
      }
    }
  }
}

// Model family policy: 0.1.0 deprecated since 1.0.0, retirement not before
// a published 2.0.0, alias v1 -> 1.0.0, exactly one real migration edge.
const model = registry.families.model;
const deprecated = model.versions.find((record) => record.version === "0.1.0");
if (!deprecated || deprecated.state !== "deprecated" || deprecated.deprecatedSince !== "1.0.0" || deprecated.retirementNotBefore !== "2.0.0") {
  fail("model:policy", "0.1.0 must be deprecated since 1.0.0 with retirement not before 2.0.0");
}
const supported = model.versions.find((record) => record.version === "1.0.0");
if (!supported || supported.state !== "supported" || model.current !== "1.0.0") {
  fail("model:policy", "1.0.0 must be the supported current version");
}
if (model.aliases.length !== 1 || model.aliases[0].alias !== "v1" || model.aliases[0].version !== "1.0.0") {
  fail("model:policy", "the sole declared alias is v1 -> 1.0.0");
}
if (model.migrations.length !== 1 || model.migrations[0].from !== "0.1.0" || model.migrations[0].to !== "1.0.0") {
  fail("model:policy", "exactly the 0.1.0 -> 1.0.0 migration edge is declared");
}

// IR family policy: the artifact must agree with the compiled constant.
const irFamily = registry.families.ir;
const irSource = read("crates/lekalo-core/src/ir/version.rs");
const irVersion = irSource.match(/VERSION: &str = "([^"]+)"/)?.[1];
if (!irVersion) fail("ir:constant", "cannot read the IR VERSION constant");
if (irFamily.current !== irVersion) {
  fail("ir:sync", `registry current ${irFamily.current} disagrees with the compiled IR ${irVersion}`);
}
if (irFamily.versions.length !== 1 || irFamily.versions[0].version !== irVersion) {
  fail("ir:sync", "the IR family must carry exactly the accepted IR version");
}

const protocolPolicy = (family, publishedBase) => {
  if (publishedBase === null) {
    return family.current === null && family.versions.length === 0 &&
      family.aliases.length === 0 && family.migrations.length === 0;
  }
  // Published policy (issues #27/#28/#29): the base additive 1.0.0, the
  // additive describe-response extension 1.1.0, and the additive
  // resolved-profile request extension 1.2.0 as current; alias
  // v1 -> 1.0.0; no migrations.
  const versions = family.versions.map((record) => record.version);
  return family.current === "1.2.0" &&
    JSON.stringify(versions) === JSON.stringify([publishedBase, "1.1.0", "1.2.0"]) &&
    family.versions.every(
      (record) => record.state === "supported" && record.classification === "additive",
    ) &&
    family.aliases.length === 1 && family.aliases[0].alias === "v1" &&
    family.aliases[0].version === publishedBase && family.migrations.length === 0;
};

// Protocol family policy: issue #27 publishes the base additive 1.0.0;
// issue #28 publishes the additive describe-response extension 1.1.0;
// issue #29 publishes the additive resolved-profile request extension
// 1.2.0 as the current version. The alias v1 -> 1.0.0 stays, and no
// migrations exist. Historical snapshots retain the exact
// pre-publication family policy; publication never makes their null
// current version an implicit alias for the shipped protocol.
const protocol = registry.families.protocol;
const protocolVersions = protocol.versions.map((record) => record.version);
if (JSON.stringify(protocolVersions) !== JSON.stringify(["1.0.0", "1.1.0", "1.2.0"])) {
  fail("protocol:policy", `unexpected protocol version set ${protocolVersions.join(",")}`);
}
for (const record of protocol.versions) {
  if (record.state !== "supported" || record.classification !== "additive") {
    fail("protocol:policy", `protocol ${record.version} must be supported additive`);
  }
}
if (protocol.current !== "1.2.0") {
  fail("protocol:policy", "protocol current must be 1.2.0");
}
if (protocol.migrations.length !== 0) {
  fail("protocol:policy", "the protocol family declares no migrations");
}
if (protocol.aliases.length !== 1 || protocol.aliases[0].alias !== "v1" || protocol.aliases[0].version !== "1.0.0") {
  fail("protocol:policy", "the sole declared alias is v1 -> 1.0.0");
}
const protocolSource = read("crates/lekalo-core/src/target_protocol/version.rs");
const protocolVersion = protocolSource.match(/pub const VERSION: &str = "([^"]+)"/)?.[1];
const protocolBase = protocolSource.match(/pub const BASE_VERSION: &str = "([^"]+)"/)?.[1];
const protocolToken = protocolSource.match(/pub const PROTOCOL_TOKEN: &str = "([^"]+)"/)?.[1];
const schema10 = JSON.parse(read("contracts/target-protocol.schema.v1.0.0.json"));
const schema11 = JSON.parse(read("contracts/target-protocol.schema.v1.1.0.json"));
const schema12 = JSON.parse(read("contracts/target-protocol.schema.v1.2.0.json"));
if (protocolVersion !== "1.2.0" || protocolBase !== "1.0.0" || protocol.current !== protocolVersion) {
  fail("protocol:sync", "the registry current must equal the compiled current protocol version");
}
if (schema10.$defs.protocolVersion.const !== "1.0.0") {
  fail("protocol:sync", "the frozen 1.0.0 document must keep const 1.0.0");
}
if (JSON.stringify(schema11.$defs.protocolVersion.enum) !== JSON.stringify(["1.0.0", "1.1.0"])) {
  fail("protocol:sync", "the 1.1.0 document must negotiate exactly 1.0.0 and 1.1.0");
}
if (JSON.stringify(schema12.$defs.protocolVersion.enum) !== JSON.stringify(["1.0.0", "1.1.0", "1.2.0"])) {
  fail("protocol:sync", "the 1.2.0 document must negotiate exactly the three published versions");
}
if (!schema12.$defs.requestEnvelope.properties.profile_digest ||
    !schema12.$defs.requestEnvelope.properties.profile_capabilities) {
  fail("protocol:sync", "the 1.2.0 document must declare the resolved-profile members");
}
if (schema11.$defs.requestEnvelope.properties.profile_digest ||
    schema11.$defs.requestEnvelope.properties.profile_capabilities ||
    schema10.$defs.requestEnvelope.properties.profile_digest ||
    schema10.$defs.requestEnvelope.properties.profile_capabilities) {
  fail("protocol:sync", "the frozen documents must not declare the resolved-profile members");
}
if (protocolToken !== "lekalo.target/v1" || schema10.$defs.protocolToken.const !== protocolToken ||
    schema11.$defs.protocolToken.const !== protocolToken ||
    schema12.$defs.protocolToken.const !== protocolToken) {
  fail("protocol:token", "the compiled protocol token and all wire schemas must agree on lekalo.target/v1");
}

// Boundary/control pairs prove that reconciling the published inventory
// does not permit fake versions, aliases, lifecycle states, or migrations.
const unpublished = JSON.parse(read("tests/fixtures/versioning/protocol-unpublished.family.json"));
let protocolPolicyCases = 0;
const expectProtocolPolicy = (name, family, publishedVersion, expected) => {
  protocolPolicyCases += 1;
  if (protocolPolicy(family, publishedVersion) !== expected) {
    fail(`protocol:control:${name}`, `expected policy acceptance ${expected}`);
  }
};
expectProtocolPolicy("published", protocol, "1.0.0", true);
expectProtocolPolicy("historical-unpublished", unpublished, null, true);
expectProtocolPolicy("unpublished-is-not-current", unpublished, "1.0.0", false);
expectProtocolPolicy("published-is-not-historical", protocol, null, false);
for (const [name, mutate] of [
  ["no-current", (family) => { family.current = null; }],
  ["wrong-current", (family) => { family.current = "1.0.0"; }],
  ["no-version", (family) => { family.versions = []; }],
  ["wrong-base-version", (family) => { family.versions[0].version = "1.2.0"; }],
  ["extra-version", (family) => { family.versions.push({ ...family.versions[1], version: "1.2.0" }); }],
  ["dropped-version", (family) => { family.versions = [family.versions[1]]; }],
  ["deprecated", (family) => { family.versions[0].state = "deprecated"; }],
  ["retired", (family) => { family.versions[0].state = "retired"; }],
  ["wrong-classification", (family) => { family.versions[0].classification = "breaking"; }],
  ["no-alias", (family) => { family.aliases = []; }],
  ["wrong-alias", (family) => { family.aliases[0].alias = "v2"; }],
  ["wrong-alias-target", (family) => { family.aliases[0].version = "1.1.0"; }],
  ["extra-alias", (family) => { family.aliases.push({ alias: "v2", version: "1.0.0" }); }],
  ["migration", (family) => { family.migrations.push({ id: "invented" }); }],
]) {
  const family = structuredClone(protocol);
  mutate(family);
  expectProtocolPolicy(name, family, "1.0.0", false);
}
for (const field of ["current", "versions", "aliases", "migrations"]) {
  const family = structuredClone(unpublished);
  family[field] = field === "current" ? "1.0.0" : [{ id: "invented" }];
  expectProtocolPolicy(`historical-${field}`, family, null, false);
}

// Edge binding: every registry edge id must appear in the compiled catalog.
const graphSource = read("crates/lekalo-core/src/versioning/graph.rs");
for (const edge of model.migrations) {
  if (!graphSource.includes(`"${edge.id}"`)) {
    fail("binding", `registry edge ${edge.id} has no compiled implementation`);
  }
}
const catalogIds = [...graphSource.matchAll(/const (\w+): &str = "(model-[^"]+)"/g)].map((match) => match[2]);
for (const id of catalogIds) {
  if (!model.migrations.some((edge) => edge.id === id)) {
    fail("binding", `compiled step ${id} has no registry edge`);
  }
}

// ---------------------------------------------------------------------------
// 2. The golden CLI projection agrees with an independent projection.
// ---------------------------------------------------------------------------
const golden = JSON.parse(read("tests/fixtures/versioning/compatibility.golden.json"));
const projected = {
  status: "valid",
  registryVersion: registry.registryVersion,
  families: FAMILY_NAMES.map((name) => {
    const family = registry.families[name];
    return {
      family: name,
      current: family.current,
      min: family.versions[0]?.version ?? null,
      max: family.versions[family.versions.length - 1]?.version ?? null,
      aliases: family.aliases.map((alias) => ({ alias: alias.alias, version: alias.version })),
      versions: family.versions.map((record) => ({
        version: record.version,
        state: record.state,
        classification: record.classification,
      })),
      migrations: family.migrations.map((edge) => ({
        id: edge.id,
        from: edge.from,
        to: edge.to,
        classification: edge.classification,
      })),
    };
  }),
};
if (JSON.stringify(golden) !== JSON.stringify(projected)) {
  fail("golden:compatibility", "golden projection disagrees with the independent projection");
}

// The migration dry-run golden must reference the declared chain only.
const dryRun = JSON.parse(read("tests/fixtures/versioning/migration/dry-run.golden.json"));
if (JSON.stringify(dryRun.chain) !== JSON.stringify(model.migrations.map((edge) => edge.id))) {
  fail("golden:dry-run", "golden chain disagrees with the registry edges");
}
if (dryRun.loss.length !== model.migrations.reduce((count, edge) => count + edge.loss.length, 0)) {
  fail("golden:dry-run", "golden loss list disagrees with the declared losses");
}

// ---------------------------------------------------------------------------
// 3. The AdapterCompatibilityManifest schema rules, restated for the gate:
//    the compiled struct rejects wrong schema versions and inverted ranges,
//    so the artifact-side contract is the manifestVersion constant.
// ---------------------------------------------------------------------------
const compatSource = read("crates/lekalo-core/src/versioning/compatibility.rs");
if (!compatSource.includes('pub const MANIFEST_SCHEMA_VERSION: &str = "1.0.0"')) {
  fail("adapter-manifest", "the manifest schema version constant drifted");
}

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, failures }, null, 2)}\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({
  ok: true,
  registryVersion: registry.registryVersion,
  families: FAMILY_NAMES,
  modelVersions: model.versions.map((record) => record.version),
  protocolVersions: protocol.versions.map((record) => record.version),
  protocolPolicyCases,
  migrationEdges: model.migrations.map((edge) => edge.id),
}, null, 2)}\n`);
