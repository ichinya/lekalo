#!/usr/bin/env node
// Issue #19 fixture generator: emits the committed portability fixture
// set from one declarative description and refuses drift in --check.
//
// The description holds every authored byte (targets configuration,
// evidence manifest, profile documents, invalid controls). The digests
// the Node side can verify by hashing raw artifact bytes are verified
// here; the digests that require the Rust resolver (profile resolution
// pins, Model/IR pins) are template constants — the live contract gate
// (`scripts/test-portability-contracts.mjs`) re-derives them through the
// production binary, so a stale pin fails the gate, never silently
// passes. Acceptance only ever runs --check; writing is an explicit,
// reviewed authoring step.
//
// Usage:
//   node scripts/gen-portability-fixtures.mjs --check   (acceptance)
//   node scripts/gen-portability-fixtures.mjs           (authoring)

import { createHash } from "node:crypto";
import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fixtureDir = join(root, "tests/fixtures/portability");
const check = process.argv.includes("--check");
const write = !check;

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const read = (relative) => readFileSync(join(root, relative));
const readText = (relative) => read(relative).toString("utf8");

const fail = (reason, detail) => {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`,
  );
  process.exit(1);
};

// ---------------------------------------------------------------------------
// The declarative description. The profile documents are the checked-in
// target-profile fixtures (provenance: tests/fixtures/target-profile/);
// the pins are exact constants derived once from the Rust resolver.
// ---------------------------------------------------------------------------

const profilePins = {
  "node.json": {
    file: "node.json",
    id: "node-postgres-http",
    version: "0.2.16",
    sourceDigest: "sha256:c1801eabea9b7fb409df06baeb09571801d9d5005b2ec2910756b4bba92d9f55",
    resolvedDigest: "sha256:aaa568174b6c74fcfc80f93916fd58c8cb722dedee503b2a589097e018fb8866",
  },
  "laravel.json": {
    file: "laravel.json",
    id: "laravel-postgres-http",
    version: "0.2.16",
    sourceDigest: "sha256:a8b2cffad215df41742a766b6d88d25fab977b5d047cb51d765b4a478a30fbc4",
    resolvedDigest: "sha256:501c4672bdca8dbc6e812f21fc4eb38ab7380fef34c1ab5c6e2d74826601511f",
  },
  "go.json": {
    file: "go.json",
    id: "go-postgres-http",
    version: "0.2.16",
    sourceDigest: "sha256:1b25c8389cfa74d0fabe325490a31b365b76798f0727a9f88b4aabc73c607392",
    resolvedDigest: "sha256:545aff6a01a77cdccf3d6644c2bc1f77f049097ad5cd30a7bf7cbda884aedece",
  },
};

const binding = (targetId, profile, extras = {}) => ({
  targetId,
  profileDocumentRef: `config/profiles/${profile.file}`,
  profileRef: {
    id: profile.id,
    version: profile.version,
    sourceDigest: profile.sourceDigest,
    resolvedDigest: profile.resolvedDigest,
  },
  adapterEvidenceRef: null,
  storageProfileRefs: [],
  ...extras,
});

const targets = {
  schemaVersion: "lekalo/portability-targets/v0.8.0",
  identity: "dev.lekalo.portability-targets@0.8.0",
  defaultSource: "node-typescript",
  defaultDestinations: ["go-service", "php-laravel"],
  targets: [
    binding("node-typescript", profilePins["node.json"]),
    binding("php-laravel", profilePins["laravel.json"]),
    binding("go-service", profilePins["go.json"]),
  ],
};

const implementationArtifact = "reports/implementation.json";
const implementationDigest = `sha256:${sha256(
  read(`tests/fixtures/portability/planner/${implementationArtifact}`),
)}`;

// The recorded scenario receipt: a claimed result carries digest-bound
// receipt custody through the closed receipt contract — kind, subject,
// destination, compilation provenance, destination profile pin, and one
// result. The manifest pins its bytes with the computed digest.
const recipeDigest = `sha256:${sha256(read("contracts/portability-recipe.v0.8.0.json"))}`;

// The Model/IR pins require the production compiler; they are template
// constants re-derived live by the contract gate.
const modelPin = "sha256:ab765c6824e9af88aa643e29ca339574ba8c020ad3cb0f1d9d0a9a5ad8302e64";
const irPin = "sha256:f9a4b5d6649787482014dffcdac03de55e231950af28cd853e6deaa6f1a29578";

const focusReceiptArtifact = "reports/receipts/focus-flow.json";
const focusReceipt = {
  schemaVersion: "lekalo/portability-receipt/v0.8.0",
  identity: "dev.lekalo.portability-receipt@0.8.0",
  kind: "scenario",
  subject: "planner.focus_flow",
  targetId: "php-laravel",
  result: "pass",
  projectRef: {
    projectId: "planner",
    modelVersion: "0.2.16",
    irDigest: irPin,
  },
  profileRef: {
    id: profilePins["laravel.json"].id,
    version: profilePins["laravel.json"].version,
    resolvedDigest: profilePins["laravel.json"].resolvedDigest,
  },
};
const focusReceiptDigest = `sha256:${sha256(
  Buffer.from(`${JSON.stringify(focusReceipt, null, 2)}\n`),
)}`;

const evidence = {
  schemaVersion: "lekalo/portability-evidence/v0.8.0",
  identity: "dev.lekalo.portability-evidence@0.8.0",
  projectRef: {
    projectId: "planner",
    modelVersion: "0.2.16",
    irDigest: irPin,
  },
  recipeRef: {
    id: "dev.lekalo.portability-recipe",
    version: "0.8.0",
    digest: recipeDigest,
  },
  recordedContext: {
    state: "recorded",
    revision: null,
    note: "synthetic authored receipts; no native runtime executed",
  },
  families: [
    {
      family: "implementation",
      complete: true,
      artifactRef: implementationArtifact,
      digest: implementationDigest,
    },
    { family: "storage", complete: false, artifactRef: null, digest: null },
    { family: "scenario", complete: false, artifactRef: null, digest: null },
  ],
  adapters: ["node-typescript", "php-laravel", "go-service"].map((targetId) => ({
    targetId,
    state: "absent",
    identity: null,
    version: null,
    protocol: null,
    capabilityDigest: null,
  })),
  mappings: [
    {
      id: "map.schedule-php",
      requirement: "behavior.hook",
      capability: "runtime.async",
      targetId: "php-laravel",
      subject: "planner.schedule_calculate",
      coverage: "full",
      conditions: ["replacement-binding-declared", "php-84", "laravel-11"],
      definitionPins: [{ id: "runtime.async", version: "0.2.16" }],
    },
    {
      id: "map.reading-go",
      requirement: "behavior.hook",
      capability: "storage.returning",
      targetId: "go-service",
      subject: "planner.count_focused",
      coverage: "full",
      conditions: ["replacement-binding-declared", "go-122"],
      definitionPins: [{ id: "storage.returning", version: "0.2.16" }],
    },
  ],
  storageRequired: [
    {
      subject: "scalar:planner.weight",
      policy: { kind: "decimal", value: "exact", precision: 18, scale: 6 },
    },
    {
      subject: "entity:planner.task",
      policy: { kind: "isolation", value: "repeatable-read" },
    },
  ],
  storageOffered: [
    {
      targetId: "php-laravel",
      policy: { kind: "decimal", value: "exact", precision: 10, scale: 2 },
    },
    {
      targetId: "go-service",
      policy: { kind: "decimal", value: "exact", precision: 18, scale: 6 },
    },
  ],
  operationalRequirements: [],
  scenarios: [
    {
      scenario: "planner.focus_flow",
      targetId: "php-laravel",
      state: "recorded-pass",
      receiptRef: focusReceiptArtifact,
      receiptDigest: focusReceiptDigest,
    },
    {
      scenario: "planner.schedule_flow",
      targetId: "php-laravel",
      state: "untested",
      receiptRef: null,
      receiptDigest: null,
    },
  ],
  gates: [
    {
      gate: "planner.native-scenario-gate",
      scenarios: ["planner.focus_flow", "planner.schedule_flow"],
      state: "declared",
      targetId: null,
      receiptRef: null,
      receiptDigest: null,
    },
  ],
  hooks: [
    {
      subject: "command:planner.focus_task",
      hook: "implicit-observer",
      state: "observed-unsupported",
      targetId: "go-service",
    },
  ],
};

const documents = new Map([
  [
    "planner/config/portability-targets.json",
    `${JSON.stringify(targets, null, 2)}\n`,
  ],
  [
    "planner/reports/portability-evidence.json",
    `${JSON.stringify(evidence, null, 2)}\n`,
  ],
  [
    "planner/reports/receipts/focus-flow.json",
    `${JSON.stringify(focusReceipt, null, 2)}\n`,
  ],
]);

// The invalid controls: authored refusals with their expected reason.
// `cross-target-mapping` is not a decode refusal: its re-scoped php
// mapping names a capability the go profile never declares at full
// support, so the go schedule row must stay blocked (the report
// consumer below pins the expected row states).
const invalidControls = [
  ["invalid/duplicate-keys.evidence.json", "duplicate-key"],
  ["invalid/stale-project-pin.evidence.json", "project-pin"],
  ["invalid/recipe-pin.evidence.json", "recipe-pin"],
  ["invalid/unknown-source.targets.json", "default-source"],
  ["invalid/duplicate-binding.targets.json", "target-binding"],
  ["invalid/stale-profile-pin.targets.json", "profile-pin"],
  ["invalid/unknown-member.evidence.json", "closed-shape"],
  ["invalid/cross-target-mapping.evidence.json", "cross-target-mapping"],
  ["invalid/mapping-unqualified.evidence.json", "mapping-unqualified"],
  ["invalid/mapping-conflict.evidence.json", "mapping-conflict"],
  ["invalid/forged-storage.evidence.json", "forged-storage"],
  ["invalid/bad-family-digest.evidence.json", "family-digest"],
  ["invalid/forged-receipt.evidence.json", "receipt-unqualified"],
  ["invalid/receipt-conflict.evidence.json", "duplicate-receipt"],
  ["invalid/adapter-missing-version.evidence.json", "adapter-snapshot"],
  ["invalid/omitted-nullable.evidence.json", "closed-shape"],
  ["invalid/path-traversal.evidence.json", "artifact-ref"],
  ["invalid/duplicate-defaults.targets.json", "default-destination"],
  ["invalid/empty-targets.targets.json", "target-binding"],
  ["invalid/defaults-null.targets.json", "closed-shape"],
  ["invalid/revision-grammar.evidence.json", "recorded-context"],
  ["invalid/policy-precision-zero.evidence.json", "policy-shape"],
  ["invalid/policy-kind-evil.evidence.json", "policy-shape"],
  ["invalid/duplicate-required.evidence.json", "duplicate-requirement"],
  ["invalid/duplicate-offer.evidence.json", "duplicate-offer"],
  ["invalid/adapter-bad-version.evidence.json", "adapter-snapshot"],
  ["invalid/mapping-identity.evidence.json", "mapping-identity"],
  ["invalid/duplicate-storage-ref.targets.json", "duplicate-storage-ref"],
  ["invalid/duplicate-conditions.evidence.json", "mapping-conditions"],
  ["invalid/requirement-grammar.evidence.json", "operational-requirement"],
  ["invalid/gate-fabricated-executed.evidence.json", "gate-custody"],
  ["invalid/receipt-result-contradiction.evidence.json", "receipt-result"],
  ["invalid/gate-authored-failed.evidence.json", "gate-state"],
  ["invalid/gate-omitted-nullables.evidence.json", "closed-shape"],
  ["invalid/gate-bad-custody-grammar.evidence.json", "gate-identity"],
  ["invalid/policy-null-precision.evidence.json", "policy-shape"],
  ["invalid/duplicate-hooks.evidence.json", "duplicate-hook"],
  ["invalid/absent-adapter-bad-version.evidence.json", "adapter-snapshot"],
  ["invalid/receipt-ref-trailing-dash.evidence.json", "artifact-ref"],
  ["invalid/receipt-ref-device-stem.evidence.json", "artifact-ref"],
  ["invalid/storage-ref-over-length.targets.json", "artifact-ref"],
];

// The invalid controls are authored here, exactly like the manifests:
// each names its base document and its one distortion; write and --check
// modes emit/verify the committed bytes. `duplicate-keys` is authored
// raw (a duplicate key is invisible to a JSON round-trip) and stays
// existence-checked only.
const zero = `sha256:${"0".repeat(64)}`;
const ones = `sha256:${"1".repeat(64)}`;
const twos = `sha256:${"2".repeat(64)}`;
const controlAuthors = [
  ["invalid/stale-project-pin.evidence.json", "evidence", (m) => { m.projectRef.projectId = "foreign-project"; }],
  ["invalid/recipe-pin.evidence.json", "evidence", (m) => { m.recipeRef.digest = ones; }],
  ["invalid/unknown-source.targets.json", "targets", (m) => { m.defaultSource = "ruby-rails"; }],
  ["invalid/duplicate-binding.targets.json", "targets", (m) => { m.targets.push(structuredClone(m.targets[0])); }],
  ["invalid/stale-profile-pin.targets.json", "targets", (m) => { m.targets[2].profileRef.resolvedDigest = twos; }],
  ["invalid/unknown-member.evidence.json", "evidence", (m) => { m.unexpectedMember = true; }],
  ["invalid/cross-target-mapping.evidence.json", "evidence", (m) => { m.mappings[0].targetId = "go-service"; }],
  ["invalid/mapping-unqualified.evidence.json", "evidence", (m) => {
    m.mappings[0] = {
      id: "map.schedule-php",
      requirement: "unrelated.requirement",
      capability: "analysis.lint",
      targetId: "php-laravel",
      subject: "planner.schedule_calculate",
      coverage: "full",
      conditions: ["replacement-binding-declared", "php-84", "laravel-11"],
      definitionPins: [{ id: "nonexistent.capability", version: "99.0.0" }],
    };
  }],
  ["invalid/mapping-conflict.evidence.json", "evidence", (m) => {
    m.mappings.push({
      id: "map.reading-go-partial",
      requirement: "behavior.hook",
      capability: "storage.returning",
      targetId: "go-service",
      subject: "planner.count_focused",
      coverage: "partial",
      conditions: ["replacement-binding-declared", "go-122"],
      definitionPins: [{ id: "storage.returning", version: "0.2.16" }],
    });
  }],
  ["invalid/forged-storage.evidence.json", "evidence", (m) => {
    m.storageRequired = [];
    m.storageOffered = [];
    m.families[1] = { family: "storage", complete: true, artifactRef: "reports/nonexistent-storage.json", digest: zero };
  }],
  ["invalid/bad-family-digest.evidence.json", "evidence", (m) => {
    m.families[1] = { family: "storage", complete: true, artifactRef: "reports/implementation.json", digest: zero };
  }],
  ["invalid/forged-receipt.evidence.json", "evidence", (m) => {
    m.scenarios.push({
      scenario: "planner.schedule_flow",
      targetId: "go-service",
      state: "recorded-pass",
      receiptRef: "reports/receipts/missing.json",
      receiptDigest: zero,
    });
  }],
  ["invalid/receipt-conflict.evidence.json", "evidence", (m) => {
    m.scenarios.push({
      scenario: "planner.focus_flow",
      targetId: "php-laravel",
      state: "recorded-fail",
      receiptRef: m.scenarios[0].receiptRef,
      receiptDigest: m.scenarios[0].receiptDigest,
    });
  }],
  ["invalid/adapter-missing-version.evidence.json", "evidence", (m) => {
    m.adapters[1] = {
      targetId: "php-laravel",
      state: "recorded",
      identity: "composer/lekalo-planner-adapter",
      version: null,
      protocol: null,
      capabilityDigest: zero,
    };
  }],
  ["invalid/omitted-nullable.evidence.json", "evidence", (m) => { delete m.recordedContext.note; }],
  ["invalid/path-traversal.evidence.json", "evidence", (m) => {
    m.families[1] = { family: "storage", complete: false, artifactRef: "reports/a/../b.json", digest: zero };
  }],
  ["invalid/duplicate-defaults.targets.json", "targets", (m) => { m.defaultDestinations = ["go-service", "go-service"]; }],
  ["invalid/empty-targets.targets.json", "targets", (m) => { m.targets = []; }],
  ["invalid/defaults-null.targets.json", "targets", (m) => { m.defaultDestinations = null; }],
  ["invalid/revision-grammar.evidence.json", "evidence", (m) => { m.recordedContext.revision = "zzz-not-hex"; }],
  ["invalid/policy-precision-zero.evidence.json", "evidence", (m) => { m.storageRequired[0].policy.precision = 0; }],
  ["invalid/policy-kind-evil.evidence.json", "evidence", (m) => {
    m.storageRequired[0].policy = { kind: "evil-kind", value: "exact", precision: 18, scale: 6 };
  }],
  ["invalid/duplicate-required.evidence.json", "evidence", (m) => { m.storageRequired.push(structuredClone(m.storageRequired[0])); }],
  ["invalid/duplicate-offer.evidence.json", "evidence", (m) => {
    m.storageOffered.unshift({ targetId: "php-laravel", policy: { kind: "decimal", value: "exact", precision: 18, scale: 6 } });
  }],
  ["invalid/adapter-bad-version.evidence.json", "evidence", (m) => {
    m.adapters[1] = {
      targetId: "php-laravel",
      state: "recorded",
      identity: "evil-adapter",
      version: "banana",
      protocol: "made/up",
      capabilityDigest: zero,
    };
  }],
  ["invalid/mapping-identity.evidence.json", "evidence", (m) => { m.mappings[0].id = "INVALID ID"; }],
  ["invalid/duplicate-storage-ref.targets.json", "targets", (m) => {
    m.targets[1].storageProfileRefs = ["config/profiles/laravel.json", "config/profiles/laravel.json"];
  }],
  ["invalid/duplicate-conditions.evidence.json", "evidence", (m) => {
    m.mappings[0].conditions = ["php-84", "php-84", "laravel-11", "replacement-binding-declared"];
  }],
  ["invalid/requirement-grammar.evidence.json", "evidence", (m) => {
    m.operationalRequirements = [{
      subject: "command:planner.focus_task",
      requirement: "bad requirement!",
      conditions: ["recorded-scenario-receipt"],
      definitionPins: [],
    }];
  }],
  ["invalid/gate-fabricated-executed.evidence.json", "evidence", (m) => { m.gates[0].state = "executed"; }],
  ["invalid/receipt-result-contradiction.evidence.json", "evidence", (m) => { m.scenarios[0].state = "recorded-fail"; }],
  // An authored `failed` gate is schema-invalid: a gate may only become
  // failed through a qualified failing receipt, never an authored token.
  ["invalid/gate-authored-failed.evidence.json", "evidence", (m) => { m.gates[0].state = "failed"; }],
  // The gate custody members are required-but-nullable: omitting them
  // entirely is a closed-shape refusal, not a silent `None`.
  ["invalid/gate-omitted-nullables.evidence.json", "evidence", (m) => {
    delete m.gates[0].targetId;
    delete m.gates[0].receiptRef;
    delete m.gates[0].receiptDigest;
  }],
  // Present custody members obey their closed grammar in any authored
  // state: a declared gate may carry no malformed target id, traversal
  // receipt reference, or fabricated digest spelling.
  ["invalid/gate-bad-custody-grammar.evidence.json", "evidence", (m) => {
    m.gates[0].targetId = "INVALID TARGET";
    m.gates[0].receiptRef = "../out";
    m.gates[0].receiptDigest = "bad";
  }],
  // Policy precision/scale are integers-or-absent, never explicit nulls.
  ["invalid/policy-null-precision.evidence.json", "evidence", (m) => {
    m.storageRequired[0].policy.precision = null;
  }],
  // The hook observations are a set: a fully duplicated observation is
  // a decode refusal (the schema's uniqueItems).
  ["invalid/duplicate-hooks.evidence.json", "evidence", (m) => {
    m.hooks.push(structuredClone(m.hooks[0]));
  }],
  // Present adapter members obey their closed grammar in any state —
  // an `absent` record may not carry a malformed version either.
  ["invalid/absent-adapter-bad-version.evidence.json", "evidence", (m) => {
    m.adapters[0].version = "bad";
  }],
  // The `artifactRef` grammar mirrors the published pattern in both
  // directions: a trailing dash segment is schema-invalid, and so is a
  // DOS-device stem segment the pattern excludes.
  ["invalid/receipt-ref-trailing-dash.evidence.json", "evidence", (m) => {
    m.scenarios[0].receiptRef = "reports/x-/r.json";
  }],
  ["invalid/receipt-ref-device-stem.evidence.json", "evidence", (m) => {
    m.scenarios[0].receiptRef = "reports/nul/r.json";
  }],
  // A storage-profile reference above 252 characters is schema-invalid:
  // the report composes `<ref>#sha256:<digest>` into a member published
  // at 324, so the input bound holds the emitted bound by construction.
  ["invalid/storage-ref-over-length.targets.json", "targets", (m) => {
    m.targets[1].storageProfileRefs = [
      ["a".repeat(63), "a".repeat(63), "a".repeat(63), `b${"b".repeat(55)}.json`].join("/"),
    ];
  }],
];

// ---------------------------------------------------------------------------
// Emission (--write) and verification (--check).
// ---------------------------------------------------------------------------

if (write) {
  for (const relative of documents.keys()) {
    const path = join(fixtureDir, relative);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, documents.get(relative));
  }
  for (const [relative, base, mutate] of controlAuthors) {
    const path = join(fixtureDir, relative);
    mkdirSync(dirname(path), { recursive: true });
    const document = base === "targets" ? structuredClone(targets) : structuredClone(evidence);
    mutate(document);
    writeFileSync(path, `${JSON.stringify(document, null, 2)}\n`);
  }
  process.stdout.write(
    `${JSON.stringify({
      ok: true,
      written: documents.size + controlAuthors.length,
    })}\n`,
  );
  process.exit(0);
}

// --check: the committed files must match the description byte-for-byte.
for (const [relative, expected] of documents) {
  const path = join(fixtureDir, relative);
  if (!existsSync(path)) fail("missing-fixture", relative);
  const actual = readText(`tests/fixtures/portability/${relative}`);
  if (actual !== expected) fail("fixture-drift", relative);
}
for (const [relative, base, mutate] of controlAuthors) {
  const path = join(fixtureDir, relative);
  if (!existsSync(path)) fail("missing-fixture", relative);
  const document = base === "targets" ? structuredClone(targets) : structuredClone(evidence);
  mutate(document);
  const expected = `${JSON.stringify(document, null, 2)}\n`;
  if (readText(`tests/fixtures/portability/${relative}`) !== expected) {
    fail("control-drift", relative);
  }
}

// Internal coherence the Node side can prove by hashing:
//  - the implementation artifact digest in the manifest equals the
//    digest of the committed artifact bytes;
//  - every referenced profile document exists and carries the pinned
//    identity in its declared content.
const manifest = evidence;
const implementationFamily = manifest.families.find(
  (family) => family.family === "implementation",
);
const artifactBytes = read(
  `tests/fixtures/portability/planner/${implementationFamily.artifactRef}`,
);
if (implementationFamily.digest !== `sha256:${sha256(artifactBytes)}`) {
  fail("artifact-digest", implementationFamily.artifactRef);
}
if (manifest.recipeRef.digest !== recipeDigest) fail("recipe-pin", "manifest");
for (const bindingEntry of targets.targets) {
  const document = JSON.parse(
    readText(`tests/fixtures/portability/planner/${bindingEntry.profileDocumentRef}`),
  );
  const declared = document.profiles.find(
    (profile) => profile.id === bindingEntry.profileRef.id,
  );
  if (!declared) fail("profile-id", bindingEntry.profileRef.id);
  if (declared.version !== bindingEntry.profileRef.version) {
    fail("profile-version", bindingEntry.profileRef.id);
  }
}
for (const [relative] of invalidControls) {
  if (!existsSync(join(fixtureDir, relative))) fail("missing-fixture", relative);
}
// The Model/IR template pins are re-derived live by the contract gate;
// here they only need to be exact spellings.
for (const pin of [modelPin, irPin]) {
  if (!/^sha256:[0-9a-f]{64}$/.test(pin)) fail("pin-spelling", pin);
}

process.stdout.write(
  `${JSON.stringify({
    ok: true,
    mode: "check",
    fixtures: documents.size + invalidControls.length,
    modelPin,
    irPin,
  })}\n`,
);
