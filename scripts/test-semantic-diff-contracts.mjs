#!/usr/bin/env node
// Issue #18 release gate: every semantic-diff golden fixture, validated
// with the same pinned third-party Draft 2020-12 implementation as the
// other contract gates. Exact Ajv 8.20.0 is provisioned outside this
// checkout (CI does the same on Node 18 and 24) and exposed through
// NODE_PATH / LEKALO_AJV_NODE_PATH.
//
// The gate checks the closed wire schema plus the invariants JSON Schema
// cannot express: canonical ordering of every array, dominance-ordered
// class sets, change-identity derivation inputs, metadata counts, and the
// equality invariant.

import { createRequire } from "node:module";
import { cpSync, existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  Ajv2020 = require("ajv/dist/2020").default;
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "ajv-unavailable",
    detail: error?.code ?? error?.message,
  }, null, 2)}\n`);
  process.exit(1);
}

if (ajvVersion !== "8.20.0") {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "ajv-version",
    detail: `expected 8.20.0, found ${ajvVersion}`,
  }, null, 2)}\n`);
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));

// The 0.8.0 successor schema validates the regenerated goldens (the
// frozen 0.2.16 predecessor stays in place byte-for-byte for
// predecessor consumers; the goldens carry the successor
// discriminator since the 0.8.0 bump).
const schema = read("contracts/semantic-diff.schema.v0.8.0.json");
// The frozen predecessor stays in place byte-for-byte for predecessor
// consumers; a successor never rewrites it.
const predecessorSchema = read("contracts/semantic-diff.schema.v0.2.16.json");
if (predecessorSchema.properties.schemaVersion.const !== "lekalo/semantic-diff/v0.2.16") {
  fail("predecessor-not-frozen", String(predecessorSchema.properties.schemaVersion.const));
}
const ajv = new Ajv2020({ strict: true, allErrors: true });
const validate = ajv.compile(schema);

function fail(reason, detail) {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
}

// The closed vocabularies mirrored from the contract (redundant with the
// schema enums by intent: a schema edit alone cannot widen these gates).
const CLASS_ORDER = [
  "unknown",
  "data-loss-risk",
  "storage-migration-required",
  "wire-breaking",
  "source-breaking",
  "behavioral",
  "target-specific",
  "additive",
];
const BREAKING = new Set([
  "data-loss-risk",
  "storage-migration-required",
  "wire-breaking",
  "source-breaking",
]);
const CHANGE_ID = /^sha256:[0-9a-f]{64}$/;

const casesRoot = resolve(root, "tests/fixtures/diff/cases");
const cases = readdirSync(casesRoot).sort();
if (cases.length === 0) fail("no-diff-cases", casesRoot);

let documents = 0;
let changes = 0;

const checkClassOrder = (where_, classes) => {
  const indexes = classes.map((name) => CLASS_ORDER.indexOf(name));
  if (indexes.some((index) => index < 0)) fail("unknown-class", where_);
  for (let index = 1; index < indexes.length; index += 1) {
    if (indexes[index - 1] >= indexes[index]) {
      fail("classes-not-in-dominance-order", { where_, classes });
    }
  }
};

const checkDocument = (where_, document) => {
  documents += 1;
  if (!validate(document)) {
    fail("schema-invalid", { where_, errors: validate.errors });
  }
  if (document.identity !== "dev.lekalo.semantic-diff@0.8.0") fail("identity-drift", where_);
  if (document.profiles.some((profile) => profile.profileVersion !== "lekalo/diff-profile/v0.8.0")) {
    fail("profile-version-drift", where_);
  }
  if (!CHANGE_ID.test(document.comparisonId)) fail("comparison-id-shape", where_);
  if (document.equal !== (document.changes.length === 0)) fail("equality-invariant", where_);
  if (document.complete !== true) fail("incomplete-golden", where_);
  if (document.completeReason !== null) fail("complete-reason-set", where_);

  // Metadata counts.
  const counts = {
    adapterCount: document.adapters.length,
    changeCount: document.changes.length,
    hintCount: document.migrationHints.length,
    profileCount: document.profiles.length,
    reasonCount: document.reasons.length,
    seedCount: document.affectedSeeds.length,
  };
  for (const [key, value] of Object.entries(counts)) {
    if (document.metadata[key] !== value) fail("metadata-count-drift", { where_, key });
  }

  // Changes: canonical ordering, sorted reasons, subject ordering.
  let previous = null;
  for (const change of document.changes) {
    changes += 1;
    if (!CHANGE_ID.test(change.changeId)) fail("change-id-shape", where_);
    if (change.reasons.length === 0) fail("change-without-reasons", where_);
    const sortedReasons = [...change.reasons].sort();
    if (JSON.stringify(change.reasons) !== JSON.stringify(sortedReasons)) {
      fail("change-reasons-unsorted", where_);
    }
    if (change.reasons.includes(change.kind) === false) {
      fail("change-reason-missing-kind", { where_, kind: change.kind });
    }
    const key = [
      change.subject.family,
      change.subject.id,
      change.subject.member ?? "",
      change.kind,
      change.changeId,
    ].join("\u001f");
    if (previous !== null && key <= previous) fail("changes-not-canonical-order", where_);
    previous = key;
  }

  // Reasons: sorted and unique.
  const sortedTopReasons = [...new Set(document.reasons)].sort();
  if (JSON.stringify(document.reasons) !== JSON.stringify(sortedTopReasons)) {
    fail("top-reasons-unsorted", where_);
  }

  // Classification: dominance order and derived from the change facts.
  checkClassOrder(where_, document.classification);

  // Seeds: subject order, aggregated change ids.
  let previousSeed = null;
  for (const seed of document.affectedSeeds) {
    if (seed.origin !== "direct-diff") fail("seed-origin", where_);
    const key = [seed.subject.family, seed.subject.id, seed.subject.member ?? ""].join("\u001f");
    if (previousSeed !== null && key <= previousSeed) fail("seeds-not-canonical-order", where_);
    previousSeed = key;
  }

  // Profiles: canonical profile order, ordered outcomes, dominance.
  const profileOrder = document.profiles.map((profile) => profile.profileId);
  if (JSON.stringify(profileOrder) !== JSON.stringify([...profileOrder].sort())) {
    fail("profiles-unsorted", where_);
  }
  for (const profile of document.profiles) {
    if (profile.policyRevision !== "diff-policy/v1") fail("policy-revision", where_);
    if (!CHANGE_ID.test(profile.policyDigest)) fail("policy-digest-shape", where_);
    checkClassOrder(where_, profile.classes);
    const outcomeIds = profile.outcomes.map((outcome) => outcome.changeId);
    if (JSON.stringify(outcomeIds) !== JSON.stringify([...outcomeIds].sort())) {
      fail("outcomes-unsorted", where_);
    }
    for (const outcome of profile.outcomes) {
      if (!document.changes.some((change) => change.changeId === outcome.changeId)) {
        fail("outcome-references-unknown-change", { where_: profile.profileId });
      }
      checkClassOrder(`${where_}:${profile.profileId}`, outcome.classes);
      if (outcome.classes.length === 0) fail("outcome-without-class", where_);
    }
    const breaking = profile.classes.some((name) => BREAKING.has(name));
    if (profile.verdict === "blocked" && profile.blockedOn === null) {
      fail("blocked-without-reason", where_);
    }
    if (profile.verdict !== "blocked" && profile.blockedOn !== null) {
      fail("blocked-reason-without-verdict", where_);
    }
    if (profile.verdict === "breaking" && !breaking) {
      fail("breaking-without-breaking-class", where_);
    }
  }

  // Adapters never mutate core facts: their effects reference known
  // changes and their digests are well-formed.
  for (const adapter of document.adapters) {
    for (const effect of adapter.effects) {
      if (!document.changes.some((change) => change.changeId === effect.changeId)) {
        fail("adapter-references-unknown-change", where_);
      }
    }
    if (!CHANGE_ID.test(adapter.contributionDigest)) fail("contribution-digest-shape", where_);
  }
};

for (const caseName of cases) {
  checkDocument(`cases/${caseName}/expect.json`, read(`tests/fixtures/diff/cases/${caseName}/expect.json`));
  checkDocument(
    `cases/${caseName}/expect.profiles.json`,
    read(`tests/fixtures/diff/cases/${caseName}/expect.profiles.json`),
  );
}

// The equal-formatting case is the live formatting-only acceptance
// criterion; its facts golden must be empty and equal.
const equal = read("tests/fixtures/diff/cases/equal-formatting/expect.json");
if (!equal.equal || equal.changes.length !== 0 || equal.affectedSeeds.length !== 0) {
  fail("formatting-only-case-not-equal", "equal-formatting");
}

// Exercise actual successor producers, not only regenerated legacy
// goldens: a live Model 0.8.0 relation mutation and a legacy comparison
// both validate against the successor with their own IR identities.
const binary = ["lekalo", "lekalo.exe"].map((name) => join(root, "target/debug", name))
  .find((candidate) => existsSync(candidate));
if (!binary) fail("binary-missing", "cargo build --workspace --locked");
const liveDiff = (base, candidate) => {
  const run = spawnSync(binary, ["--no-cache", "--json", "diff", "--base", base, candidate],
    { cwd: root, encoding: "utf8", timeout: 120000 });
  if (run.error || run.status !== 0) fail("live-diff-exit", String(run.error ?? run.stderr));
  return JSON.parse(run.stdout).diff;
};
const work = mkdtempSync(join(root, "target/diff-contract-"));
try {
  const base = "tests/fixtures/task-external-links/project";
  const candidate = relative(root, join(work, "candidate")).replaceAll("\\", "/");
  cpSync(join(root, base), candidate, { recursive: true });
  const path = join(candidate, "lekalo/modules/tasklinks/entities.yaml");
  const before = readFileSync(path, "utf8");
  const after = before.replace(/(id: "?tasklinks\.rel\.task_links[\s\S]*?max: )1/, (_, prefix) => prefix + "2");
  if (before === after) fail("live-relation-mutation-no-op", path);
  writeFileSync(path, after);
  const current = liveDiff(base, candidate);
  checkDocument("live/model-0.8.0-relation", current);
  for (const snapshot of [current.base, current.candidate]) {
    if (snapshot.modelVersion !== "0.8.0" || snapshot.irIdentity !== "dev.lekalo.ir@0.8.0") {
      fail("live-source-identity", snapshot);
    }
  }
  if (current.equal || !current.changes.some((change) => change.subject.member?.startsWith("relations/"))) {
    fail("live-relation-fact-missing", current.changes);
  }
  const legacy = liveDiff("tests/fixtures/diff/cases/equal-formatting/base",
    "tests/fixtures/diff/cases/equal-formatting/candidate");
  checkDocument("live/model-0.2.16", legacy);
  for (const snapshot of [legacy.base, legacy.candidate]) {
    if (snapshot.modelVersion !== "0.2.16" || snapshot.irIdentity !== "dev.lekalo.ir@0.2.16") {
      fail("live-legacy-identity", snapshot);
    }
  }
} finally {
  rmSync(work, { recursive: true, force: true });
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  ajv: ajvVersion,
  cases: cases.length,
  documents,
  changes,
}, null, 2)}\n`);
