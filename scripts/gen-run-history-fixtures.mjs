// One-shot generator for the issue #121 canonical fixture family:
// the closed valid goldens and the adversarial vectors, all synthetic,
// written in the canonical form (byte-sorted keys, compact, one
// trailing LF) the contract gate re-derives. Run from the repository
// root: node scripts/gen-run-history-fixtures.mjs
import { mkdirSync, writeFileSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root =
  resolve(dirname(fileURLToPath(import.meta.url)), "..", "tests", "fixtures", "run-history") + "/";

// Canonical writer: byte-sorted keys, compact, one trailing LF.
function canon(value) {
  if (Array.isArray(value)) return `[${value.map(canon).join(",")}]`;
  if (value && typeof value === "object") {
    const keys = Object.keys(value).sort((a, b) => {
      const left = Array.from(a);
      const right = Array.from(b);
      for (let index = 0; index < Math.min(left.length, right.length); index++) {
        const delta = left[index].codePointAt(0) - right[index].codePointAt(0);
        if (delta !== 0) return delta;
      }
      return left.length - right.length;
    });
    return `{${keys.map((key) => `${JSON.stringify(key)}:${canon(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

function write(rel, value) {
  writeFileSync(`${root}${rel}`, `${canon(value)}\n`);
  console.log(rel);
}

const policyRef = {
  policyId: "dev.lekalo.privacy-export-policy",
  version: "0.3.2",
  digest: "sha256:5a80966fa628fd4c9452325d34e7191f40ebb7a9cb49185c91d413861fb18384",
};
const authorityRef = {
  contractId: "dev.lekalo.authority-matrix",
  version: "0.3.2",
  digest: "sha256:7ae6454ea20f7b61202d368411ef9bff4e70af96f1f2a408c209d84fe9722f80",
};
const classificationRef = {
  contractId: "dev.lekalo.privacy-classification-decision",
  version: "0.2.16",
};

function provenance() {
  return {
    adapters: [
      {
        bundleDigest: { state: "known", value: "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff" },
        id: "php-laravel",
        manifestDigest: { state: "known", value: "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee" },
        version: { state: "known", value: "0.4.0" },
      },
    ],
    core: {
      buildDigest: { state: "known", value: "sha256:5555555555555555555555555555555555555555555555555555555555555555" },
      buildRevision: { state: "known", value: "build-2026-09-30" },
      version: { state: "known", value: "0.4.0" },
    },
    git: {
      commit: { state: "known", value: "1111111111111111111111111111111111111111" },
      dirty: { state: "known", value: false },
      workingSetDigest: { state: "known", value: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" },
    },
    harness: {
      id: { state: "known", value: "pilot-harness" },
      modelId: { state: "known", value: "pilot-model" },
      modelRevision: { state: "known", value: "pilot-model-rev-1" },
      version: { state: "known", value: "0.1.0" },
    },
    lock: {
      digest: { state: "known", value: "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd" },
      version: { state: "known", value: "0.4.0" },
    },
    model: {
      digest: { state: "known", value: "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" },
      irDigest: { state: "known", value: "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc" },
      revision: { state: "known", value: "planner-model" },
    },
    profile: {
      digest: { state: "known", value: "sha256:2222222222222222222222222222222222222222222222222222222222222222" },
      id: { state: "known", value: "validation-profile.default" },
      version: { state: "known", value: "0.4.0" },
    },
  };
}

function metrics(overrides = {}) {
  const base = {
    context: {
      bytes: { state: "unknown" },
      candidateFacts: { state: "unknown" },
      coverageRatio: { state: "unknown" },
      estimatedTokens: { state: "unknown" },
      estimatorVersion: { state: "unknown" },
      includedFacts: { state: "unknown" },
      representation: { state: "unknown" },
    },
    cost: { amount: { state: "unknown" }, basis: { state: "unknown" }, currency: { state: "unknown" } },
    durationMs: { state: "known", value: 1500 },
    filesChanged: { state: "known", value: 0 },
    filesRead: { state: "known", value: 0 },
    replanCount: { state: "unknown" },
    retryCount: { state: "unknown" },
    toolCalls: { state: "known", value: 2 },
    tokens: {
      input: { state: "known", value: 100 },
      output: { state: "known", value: 20 },
      reasoning: { state: "unknown" },
      total: { state: "known", value: 120 },
    },
  };
  return Object.assign(base, overrides);
}

function runRecord(overrides = {}) {
  const record = {
    artifactKind: "history.run-record",
    assertionsRef: null,
    diagnostics: [],
    identity: "dev.lekalo.run-record@0.4.0",
    measurementSources: [],
    metrics: metrics(),
    operation: { affectedSemanticIds: ["planner.focus_task"], kind: "evaluation" },
    pilot: { mode: "greenfield", scopeState: "observed" },
    privacy: {
      authorityRef,
      classificationContractRef: classificationRef,
      dataSensitivity: "internal",
      exportDisposition: "local-private",
      exportEligibility: "ineligible",
      policyRef,
      provenance: { derived: false, origin: "local", repositoryRole: "local-workspace", sourceRefs: [] },
    },
    provenance: provenance(),
    recordedAt: "2026-09-30T12:00:00.000Z",
    repeat: null,
    runId: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1",
    schema_version: "lekalo/run-record/v0.4.0",
    scope: {
      repositoryId: "33333333333333333333333333333333",
      repositoryRole: "local-workspace",
      tenantScopeId: "44444444444444444444444444444444",
    },
    status: { coverageState: "complete", outcome: "pass" },
    testGateSummaries: [
      {
        coverageState: "complete",
        evidenceRef: null,
        failed: { state: "known", value: 0 },
        id: "planner-scenarios",
        infrastructure: { state: "known", value: 0 },
        kind: "test",
        outcome: "pass",
        passed: { state: "known", value: 3 },
        sourceOutcome: "passed",
        unsupported: { state: "known", value: 0 },
      },
    ],
    timestamp: "2026-09-30T12:00:00Z",
  };
  return Object.assign(record, overrides);
}

// --- valid family -------------------------------------------------------
write("valid/greenfield.json", runRecord({ pilot: { mode: "greenfield", scopeState: "contracted" } }));
write(
  "valid/brownfield.json",
  runRecord({
    pilot: { mode: "brownfield", scopeState: "observed" },
    privacy: {
      authorityRef,
      classificationContractRef: classificationRef,
      dataSensitivity: "tenant-scoped",
      exportDisposition: "local-private",
      exportEligibility: "ineligible",
      policyRef,
      provenance: {
        derived: false,
        origin: "local",
        repositoryRole: "consumer-repository",
        sourceRefs: ["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaab2"],
      },
    },
    runId: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaab1",
    scope: {
      repositoryId: "33333333333333333333333333333333",
      repositoryRole: "consumer-repository",
      tenantScopeId: "44444444444444444444444444444455",
    },
  }),
);
write(
  "valid/unknown-cost.json",
  runRecord({ runId: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaac1" }),
);
write(
  "valid/known-zero.json",
  runRecord({
    runId: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaad1",
    metrics: metrics({
      cost: {
        amount: { state: "known", value: "0" },
        basis: { state: "known", value: "reported" },
        currency: { state: "known", value: "USD" },
      },
    }),
  }),
);
write(
  "valid/withheld.json",
  runRecord({
    runId: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaae1",
    metrics: metrics({
      tokens: {
        input: { state: "withheld" },
        output: { state: "withheld" },
        reasoning: { state: "withheld" },
        total: { state: "withheld" },
      },
    }),
  }),
);
write(
  "valid/unsupported.json",
  runRecord({
    runId: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaf1",
    status: { coverageState: "incomplete", outcome: "unsupported" },
    testGateSummaries: [
      {
        coverageState: "incomplete",
        evidenceRef: null,
        failed: { state: "known", value: 0 },
        id: "planner-scenarios",
        infrastructure: { state: "known", value: 0 },
        kind: "gate",
        outcome: "unsupported",
        passed: { state: "known", value: 0 },
        sourceOutcome: "unsupported",
        unsupported: { state: "known", value: 5 },
      },
    ],
  }),
);
write(
  "valid/infrastructure.json",
  runRecord({
    runId: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa07",
    status: { coverageState: "unknown", outcome: "infrastructure" },
    testGateSummaries: [
      {
        coverageState: "unknown",
        evidenceRef: null,
        failed: { state: "unknown" },
        id: "planner-suite",
        infrastructure: { state: "unknown" },
        kind: "test",
        outcome: "infrastructure",
        passed: { state: "unknown" },
        sourceOutcome: "missing",
        unsupported: { state: "unknown" },
      },
    ],
  }),
);
write(
  "valid/repeat-exact.json",
  runRecord({
    runId: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaab2",
    repeat: {
      comparability: "exact",
      inputFingerprint: "sha256:6666666666666666666666666666666666666666666666666666666666666666",
      parentRunId: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1",
    },
  }),
);
write("valid/assertion-set.json", {
  artifactKind: "history.assertion-set",
  authorityRef,
  dataSensitivity: "internal",
  exportDisposition: "local-private",
  identity: "dev.lekalo.run-assertions@0.4.0",
  policyRef,
  rows: [
    {
      assertionId: "focus-task-returns-planned-order",
      evidenceRef: null,
      kind: "behavior",
      outcome: "pass",
      subjectSemanticId: "planner.focus_task",
    },
    {
      assertionId: "planner-schema-contract",
      evidenceRef: null,
      kind: "contract",
      outcome: "degraded",
      subjectSemanticId: null,
    },
  ],
  schema_version: "lekalo/run-assertions/v0.4.0",
  scope: {
    repositoryId: "33333333333333333333333333333333",
    tenantScopeId: "44444444444444444444444444444444",
  },
  setId: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaab2",
  runId: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1",
});
write("valid/store-document.json", {
  dependents: [
    {
      generation: 3,
      id: "claim.lift",
      kind: "claim",
      sourceRuns: [
        {
          assertionDigest: "sha256:7777777777777777777777777777777777777777777777777777777777777777",
          recordDigest: "sha256:8888888888888888888888888888888888888888888888888888888888888888",
          runId: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1",
        },
      ],
      state: "valid",
    },
    { generation: 4, id: "index.stale", kind: "index", sourceRuns: [], state: "invalidated" },
  ],
  generation: 4,
  identity: "dev.lekalo.run-history-store@0.4.0",
  repositoryId: "33333333333333333333333333333333",
  retention: { maxAgeDays: 30, maxBytes: 67108864, maxRecords: 10000 },
  schema_version: "lekalo/run-history-store/v0.4.0",
  tenantScopes: [{ createdAt: "2026-09-30T08:00:00Z", tenantScopeId: "44444444444444444444444444444444" }],
});
write("valid/observation.json", {
  assertions: null,
  dataSensitivity: "internal",
  diagnostics: [],
  identity: "dev.lekalo.run-observation@0.4.0",
  measurementSources: [],
  metrics: {},
  operation: { affectedSemanticIds: ["planner.focus_task"], kind: "evaluation" },
  pilot: { mode: "greenfield", scopeState: "observed" },
  provenance: { adapters: [], core: {}, git: {}, harness: {}, lock: {}, model: {}, profile: {} },
  repeatParentRunId: null,
  schema_version: "lekalo/run-observation/v0.4.0",
  status: { coverageState: "unknown", outcome: "warn" },
  testGateSummaries: [],
  timestamp: "2026-09-30T12:00:00Z",
});

// --- invalid family: every vector must fail its schema -------------------
write(
  "invalid/negative-count.json",
  (() => {
    const record = runRecord();
    record.metrics.durationMs = { state: "known", value: -5 };
    return record;
  })(),
);
write(
  "invalid/extra-field.json",
  (() => {
    const record = runRecord();
    record.sourceSnippet = "must not exist";
    return record;
  })(),
);
write(
  "invalid/unknown-state.json",
  (() => {
    const record = runRecord();
    record.metrics.durationMs = { state: "absent" };
    return record;
  })(),
);
write(
  "invalid/known-without-value.json",
  (() => {
    const record = runRecord();
    record.metrics.durationMs = { state: "known" };
    return record;
  })(),
);
write(
  "invalid/absolute-path-token.json",
  (() => {
    const record = runRecord();
    record.provenance.harness.id = { state: "known", value: "C:/Users/someone/prompt.txt" };
    return record;
  })(),
);
write(
  "invalid/url-token.json",
  (() => {
    const record = runRecord();
    record.provenance.harness.id = { state: "known", value: "https://provider.example/v1" };
    return record;
  })(),
);
write(
  "invalid/unknown-enum.json",
  (() => {
    const record = runRecord();
    record.operation.kind = "deploy";
    return record;
  })(),
);
write(
  "invalid/unprefixed-digest.json",
  (() => {
    const record = runRecord();
    record.provenance.model.digest = {
      state: "known",
      value: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    };
    return record;
  })(),
);
write(
  "invalid/bad-timestamp.json",
  (() => {
    const record = runRecord();
    record.timestamp = "2026-09-30T12:00:00+01:00";
    return record;
  })(),
);
write(
  "invalid/wrong-identity.json",
  (() => {
    const record = runRecord();
    record.identity = "dev.lekalo.run-record@0.3.0";
    return record;
  })(),
);
write(
  "invalid/missing-required.json",
  (() => {
    const record = runRecord();
    delete record.privacy;
    return record;
  })(),
);
write(
  "invalid/duplicate-run-summary.json",
  (() => {
    const record = runRecord();
    record.testGateSummaries.push(record.testGateSummaries[0]);
    return record;
  })(),
);
write(
  "invalid/empty-string-token.json",
  (() => {
    const record = runRecord();
    record.provenance.harness.id = { state: "known", value: "" };
    return record;
  })(),
);
write(
  "invalid/ratio-out-of-range.json",
  (() => {
    const record = runRecord();
    record.metrics.context.coverageRatio = { state: "known", value: "1.5" };
    return record;
  })(),
);
write(
  "invalid/assertion-expected-actual.json",
  (() => {
    const set = JSON.parse(readFileSync(`${root}valid/assertion-set.json`, "utf8"));
    set.rows[0].expected = "planned order";
    return set;
  })(),
);
writeFileSync(
  `${root}invalid/duplicate-key.json`,
  '{"schema_version":"lekalo/run-record/v0.4.0","schema_version":"lekalo/run-record/v0.4.0"}\n',
);
console.log("invalid/duplicate-key.json");
console.log("fixtures written");
