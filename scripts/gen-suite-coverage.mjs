#!/usr/bin/env node
// Suite-v1 coverage index + catalog writer (issue #90).
//
// Regenerates two reviewed artifacts from the embedded diagnostic
// registry and the on-disk suite tree:
//
//   tests/fixtures/suite/v1/coverage/diagnostic-rules.json
//   tests/fixtures/suite/v1/catalog.json
//
// The coverage index enumerates EVERY registry rule with an explicit
// evidence state (suite-pair / family-fixture / test-witness /
// interaction-only). The mapping below is hand-maintained evidence:
// the gate (test-golden-catalog.mjs) verifies every referenced path
// still exists and every rule is still in the registry, so stale or
// dishonest entries fail the suite. Deterministic; run through the
// reviewed update flow.
//
// Usage: node scripts/gen-suite-coverage.mjs

import { readdirSync, readFileSync, writeFileSync, existsSync, statSync } from "node:fs";
import { createHash } from "node:crypto";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const registryPath = join(root, "contracts/diagnostic-registry.v0.6.4.json");
const registry = JSON.parse(readFileSync(registryPath, "utf8"));
const registryDigest = `sha256:${createHash("sha256").update(readFileSync(registryPath)).digest("hex")}`;
const suiteV1 = join(root, "tests/fixtures/suite/v1");

// ---------------------------------------------------------------------------
// 1. suite-pair evidence: every generated diagnostics/ pair directory.
// ---------------------------------------------------------------------------
const pairDirs = existsSync(join(suiteV1, "diagnostics"))
  ? readdirSync(join(suiteV1, "diagnostics")).sort()
  : [];
const pairRules = new Map();
for (const slug of pairDirs) {
  const expectPath = join(suiteV1, "diagnostics", slug, "expect.json");
  if (!existsSync(expectPath)) continue;
  const { rule } = JSON.parse(readFileSync(expectPath, "utf8"));
  pairRules.set(rule, slug);
}

// ---------------------------------------------------------------------------
// 2. family-fixture evidence: rule id -> committed fixture paths.
//    Every path is verified to exist by the gate; drift fails it.
// ---------------------------------------------------------------------------
const FIX = "tests/fixtures";
const familyFixture = {
  "metrics-export.input-invalid": ["tests/fixtures/metrics-export"],
  "metrics-export.evaluation-required": ["tests/fixtures/metrics-export"],
  "metrics-export.source-invalidated": ["tests/fixtures/metrics-export"],
  "metrics-export.authorization-refused": ["tests/fixtures/metrics-export"],
  "metrics-export.leak-refused": ["tests/fixtures/metrics-export"],
  "metrics-export.preview-stale": ["tests/fixtures/metrics-export"],
  "metrics-export.storage-refused": ["tests/fixtures/metrics-export"],
  "metrics-export.overlap-refused": ["tests/fixtures/metrics-export"],
  "trace.bridge-mapping-missing": [`${FIX}/trace-assessment/golden/missing-mapping.json`],
  "trace.bridge-reference-unresolved": [`${FIX}/trace-assessment/golden/dangling-mapping.json`],
  "trace.bridge-conflict": [`${FIX}/trace-assessment/golden/duplicate-mapping.json`],
  "trace.bridge-evidence-stale": [`${FIX}/trace-assessment/golden/stale-model.json`],
  "trace.bridge-provider-unsupported": [`${FIX}/trace-assessment/golden/hlv-unavailable.json`],
  "trace.bridge-execution-unverified": [`${FIX}/trace-assessment/golden/execution-digest.json`],
  "trace.bridge-chain-uncovered": [`${FIX}/trace-assessment/golden/uncovered-scenario.json`],
  "trace.bridge-input-invalid": [`${FIX}/trace-assessment/golden/unknown-member.json`],
  "trace.bridge-policy-denied": [`${FIX}/trace-assessment/golden/hlv-fail.json`],
  // loader.* — the loader conformance corpus (CLI load.rs gate).
  "loader.ambiguous-short-reference": [`${FIX}/loader/invalid-ambiguous-short-reference`],
  "loader.conflicting-declaration": [`${FIX}/loader/invalid-conflicting-declaration`],
  "loader.document-shape": [`${FIX}/loader/invalid-structure-module-yaml-missing/expect.json`, `${FIX}/loader/invalid-structure-unexpected-entry`],
  "loader.duplicate-definition": [`${FIX}/loader/invalid-duplicate-definition`],
  "loader.duplicate-module-id": [`${FIX}/loader/invalid-duplicate-module-id`],
  "loader.encoding": [`${FIX}/loader/invalid-encoding-bom`],
  "loader.import-cycle": [`${FIX}/loader/invalid-import-cycle`],
  "loader.import-invalid": [`${FIX}/loader/invalid-import-grammar`],
  "loader.import-missing": [`${FIX}/loader/invalid-import-missing`],
  "loader.json-parse": [`${FIX}/loader/invalid-json-trailing-comma`, `${FIX}/loader/invalid-json-comment`],
  "loader.limit-exceeded": [`${FIX}/loader/invalid-type-depth`],
  "loader.path-escape": [`${FIX}/loader/invalid-import-path-escape`],
  "loader.reference-without-import": [`${FIX}/loader/invalid-reference-without-import`],
  "loader.short-reference-unresolved": [`${FIX}/loader/invalid-short-reference-unresolved`],
  "loader.type-depth": [`${FIX}/loader/invalid-type-depth`],
  "loader.type-syntax": [`${FIX}/loader/invalid-type-syntax`],
  "loader.yaml-parse": [`${FIX}/loader/invalid-yaml-parse`],
  "loader.yaml-unsupported": [`${FIX}/loader/invalid-yaml-flow`, `${FIX}/loader/invalid-yaml-alias`],
  // versioning / model layer.
  "versioning.unsupported-version": [`${FIX}/loader/invalid-version-unsupported`, `${FIX}/loader/invalid-version-mixed`],
  "versioning.mixed-versions": [`${FIX}/loader/invalid-version-mixed`],
  "versioning.invalid-version": [`${FIX}/loader/invalid-version-nonstring`],
  "versioning.protocol-unpublished": [`${FIX}/versioning/protocol-unpublished.family.json`],
  // structure.* — project path policy (structure-contracts gate).
  "structure.canonical-unexpected-entry": [`${FIX}/structure/invalid-denied-extra-kind-file/expect.json`, `${FIX}/structure/invalid-denied-runtime-in-canonical/expect.json`],
  "structure.document-missing": [`${FIX}/structure/invalid-malformed-module-manifest-missing/expect.json`, `${FIX}/loader/invalid-structure-module-yaml-missing/expect.json`],
  "structure.module-subdirectory": [`${FIX}/structure/invalid-denied-module-subdirectory/expect.json`],
  "structure.nested-root": [`${FIX}/structure/invalid-denied-nested-root/expect.json`],
  "structure.runtime-unexpected-entry": [`${FIX}/structure/invalid-denied-canonical-in-runtime/expect.json`, `${FIX}/structure/invalid-denied-runtime-entry/expect.json`],
  // ir.* — the IR conformance corpus (CLI ir.rs gate).
  "ir.duplicate-member": [`${FIX}/ir/invalid-duplicate-set-member`],
  "ir.kind-placement": [`${FIX}/ir/invalid-kind-placement`],
  "ir.kind-unknown": [`${FIX}/ir/invalid-operation-unknown`, `${FIX}/ir/invalid-decision-unknown`],
  "ir.missing-field": [`${FIX}/ir/invalid-missing-field`, `${FIX}/ir/invalid-empty-identity`],
  "ir.unknown-field": [`${FIX}/ir/invalid-unknown-field`],
  "ir.value-invalid": [`${FIX}/ir/invalid-id-syntax`],
  // graph.*
  "graph.input-invalid": [`${FIX}/scenario/invalid/bad-digest.expect.json`],
  "graph.traversal-limit": [`${FIX}/scenario/invalid/duplicate-step-id.expect.json`],
  // diagnostics envelope corpus.
  "diagnostics.registry-invalid": [`${FIX}/diagnostics/loader-failure-envelope.json`],
  // target-protocol wire vectors.
  "target.plan-mismatch": [`${FIX}/target-protocol/invalid/dry-run-request-with-plan-id.expect.json`],
  "target.protected-path": [`${FIX}/target-protocol/invalid/traversal-ir-path.expect.json`],
  "target.protocol-mismatch": [`${FIX}/target-protocol/invalid/request-id-malformed.expect.json`],
  "target.request-invalid": [`${FIX}/target-protocol/invalid/absolute-ir-path.expect.json`],
  "target.response-invalid": [`${FIX}/target-protocol/invalid/response-missing-evidence.expect.json`],
  "target.scope-violation": [`${FIX}/target-protocol/invalid/duplicate-write-path.expect.json`],
  // storage projection/introspection matrices.
  "storage.input-invalid": [`${FIX}/storage-projection/invalid/bad-attachment-revision.expect.json`],
  "storage.domain-invalid": [`${FIX}/storage-projection/invalid/aggregate-both.expect.json`],
  "storage.mapping-invalid": [`${FIX}/storage-projection/invalid/collation-charset-mismatch.expect.json`],
  "storage.projection-invalid": [`${FIX}/storage-projection/invalid/blob-key-without-prefix.expect.json`],
  "storage.relation-invalid": [`${FIX}/storage-projection/invalid/aggregate-child-detach.expect.json`],
  "storage.introspection-invalid": [`${FIX}/storage-introspection/invalid/credential-member.expect.json`],
  // extended-effects matrix.
  "cache.contract-invalid": [`${FIX}/extended-effects/invalid/cache-operation-kind-mismatch.expect.json`],
  "call.contract-invalid": [`${FIX}/extended-effects/invalid/call-missing-compensation.expect.json`],
  "case.case-invalid": [`${FIX}/extended-effects/invalid/cyclic-schedule.expect.json`],
  "contract.capability-missing": [`${FIX}/extended-effects/invalid/call-missing-capability.expect.json`],
  "contract.gate-required": [`${FIX}/extended-effects/invalid/sensitive-without-gate.expect.json`],
  "contract.retry-conflict": [`${FIX}/extended-effects/invalid/job-retry-idempotency-conflict.expect.json`],
  "event.contract-invalid": [`${FIX}/extended-effects/invalid/event-dedup-key-missing.expect.json`],
  "extended.input-invalid": [`${FIX}/extended-effects/invalid/unknown-top-field.expect.json`],
  "publication.consent-missing": [`${FIX}/extended-effects/invalid/approval-without-approver.expect.json`],
  // invariant-transition matrix.
  "invariant.contract-invalid": [`${FIX}/invariant-transition/invalid`],
  "invariant.graph-invalid": [`${FIX}/invariant-transition/invalid`],
  "invariant.input-invalid": [`${FIX}/invariant-transition/invalid`],
  "invariant.mapping-invalid": [`${FIX}/invariant-transition/invalid`],
  "invariant.state-invalid": [`${FIX}/invariant-transition/invalid`],
  "invariant.transition-invalid": [`${FIX}/invariant-transition/invalid`],
  // transaction-concurrency matrix.
  "concurrency.capability-missing": [`${FIX}/transaction-concurrency/invalid`],
  "concurrency.case-invalid": [`${FIX}/transaction-concurrency/invalid`],
  "concurrency.invariant-invalid": [`${FIX}/transaction-concurrency/invalid`],
  "concurrency.precondition-invalid": [`${FIX}/transaction-concurrency/invalid`],
  "concurrency.retry-conflict": [`${FIX}/transaction-concurrency/invalid`],
  // error-registry corpus.
  "error.code-reused": [`${FIX}/error-contract/invalid/tombstoned-code-reused.registry.json`],
  "error.contract-invalid": [`${FIX}/error-contract/invalid/unknown-category.registry.json`],
  "error.registry-invalid": [`${FIX}/error-contract/invalid/unknown-top-field.registry.json`],
  // implementation semantic matrix.
  "implementation.document-invalid": [`${FIX}/implementation/semantic/ir-pin.expect.json`],
  "implementation.effect-mismatch": [`${FIX}/implementation/semantic/effect-mismatch.expect.json`],
  "implementation.reference-unresolved": [`${FIX}/implementation/semantic/reference-unresolved.expect.json`],
  "implementation.selection-ambiguous": [`${FIX}/implementation/semantic/selection-ambiguous.expect.json`],
  "implementation.selection-unknown": [`${FIX}/implementation/semantic/selection-unknown.expect.json`],
  "implementation.symbol-invalid": [`${FIX}/implementation/semantic/symbol-scheme.expect.json`],
  "implementation.target-missing": [`${FIX}/implementation/semantic/target-missing.expect.json`],
  // expressions invalid matrix.
  "expression.complexity-limit": [`${FIX}/expressions/invalid/depth-exceeded.json`],
  "expression.contract-invalid": [`${FIX}/expressions/invalid/bad-expression-id.json`],
  "expression.input-invalid": [`${FIX}/expressions/invalid/unknown-top-field.json`],
  "expression.type-invalid": [`${FIX}/expressions/invalid/arithmetic-type.json`],
  // requirements invalid matrix.
  "requirements.document-invalid": [`${FIX}/requirements/invalid/bad-schema-version.json`],
  "requirements.provider-invalid": [`${FIX}/requirements/invalid/bad-provider-kind.json`],
  // nfr invalid matrix.
  "nfr.document-invalid": [`${FIX}/nfr/invalid/bad-schema-version.json`],
  "nfr.evidence-invalid": [`${FIX}/nfr/invalid/e-bad-schema-version.json`],
  "nfr.diff-invalid": [`${FIX}/nfr/diff/unknown-constraint-evidence.json`],
  // run-history matrix.
  "history.input-invalid": [`${FIX}/run-history/invalid/missing-required.json`],
  "history.unsafe-field": [`${FIX}/run-history/invalid/absolute-path-token.json`],
  "history.version-unsupported": [`${FIX}/run-history/invalid/wrong-identity.json`],
  // orchestration receipts.
  "observed.index-missing": [`${FIX}/orchestration/invalid/verify-missing-required.json`],
  // lockfile corpus.
  "lock.schema-invalid": [`${FIX}/lockfile/invalid/missing-required.lock.json`],
  "lock.unsupported-schema-version": [`${FIX}/lockfile/invalid/unsupported-schema-version.lock.json`],
  "lock.noncanonical": [`${FIX}/lockfile/invalid/bad-version-v-prefix.lock.json`],
  // classification matrix.
  "classification.unclassified-sensitive-sink": [`${FIX}/classification/invalid/secret-in-sink/classification.json`],
  "classification.unknown-subject": [`${FIX}/classification/invalid/unknown-subject/classification.json`],
  // query-model.
  "query.input-invalid": [`${FIX}/query-model/invalid/filter-type-mismatch.json`],
  // privacy corpus.
  "dataflow.tenant-crossing": [`${FIX}/classification/invalid/tenant-crossing/classification.json`],
  // adapter manifests.
  "adapter.manifest-invalid": [`${FIX}/adapter-manifest/invalid/wrong-schema-version.json`],
  "adapter.permission-escalated": [`${FIX}/adapter-manifest/invalid/required-policy-without-signature.json`],
  "adapter.revoked": [`${FIX}/adapter-manifest/invalid/revoked-without-revocation.json`],
  "adapter.hooks-declared": [`${FIX}/adapter-manifest/invalid/non-empty-hooks.json`],
  "adapter.trust-insufficient": [`${FIX}/adapter-manifest/invalid/traversal-adapter-id.json`],
  // transport/openapi attachments.
  "transport.input-invalid": [`${FIX}/transport-http/invalid/union-member-unmapped.json`],
  "transport.contract-invalid": [`${FIX}/transport-http/invalid/capability-unsatisfied.json`],
  "transport.mapping-missing": [`${FIX}/transport-http/invalid/endpoint-kind.json`],
  "transport.endpoint-unresolved": [`${FIX}/transport-http/invalid/public-with-scheme.json`],
  "openapi.version-unsupported": [`${FIX}/openapi/invalid/wrong-openapi-version.json`],
  // doctor envelope corpus.
  "cli.usage": [`${FIX}/diagnostics/usage-envelope.json`],
  // Fix round 1: rules previously unmapped, now pinned to committed evidence.
  "loader.duplicate-key": [`${FIX}/loader/invalid-json-duplicate-key/expect.json`, `${FIX}/loader/invalid-yaml-duplicate-key/expect.json`],
  "transaction.group-missing": [`${FIX}/transaction-concurrency/invalid`],
  "transaction.group-overlap": [`${FIX}/transaction-concurrency/invalid`],
  "transaction.input-invalid": [`${FIX}/transaction-concurrency/invalid`],
  "transaction.partial-unacknowledged": [`${FIX}/transaction-concurrency/invalid`],
};

// ---------------------------------------------------------------------------
// 3. test-witness evidence: rule -> named Rust/Node gate assertions.
//    The gate file must exist; the note names the asserting test.
// ---------------------------------------------------------------------------
const testWitness = {
  "coupling.input-invalid": [["scripts/test-coupling-contracts.mjs","closed-schemas and duplicate-keys-fail production probes"]],
  "coupling.profile-unsupported": [["scripts/test-coupling-contracts.mjs","unsupported-version production probe"]],
  "coupling.fan-exceeded": [["scripts/test-coupling-contracts.mjs","configured neighbor threshold production probe"]],
  "coupling.public-contract-amplification": [["scripts/test-coupling-contracts.mjs","configured public radius production probe"]],
  "coupling.cross-module-cycle": [["crates/lekalo-core/tests/coupling.rs","cycle_series_are_separate acceptance probe"]],
  "coupling.shared-mutable-state": [["crates/lekalo-core/tests/coupling.rs","centrality_is_visible_and_advisory_by_default asserts rule"]],
  "coupling.change-amplification": [["scripts/test-coupling-contracts.mjs","configured artifact amplification production probe"]],
  "coupling.transaction-spread": [["crates/lekalo-core/tests/coupling.rs","transaction_spread_uses_explicit_domain_groups acceptance probe"]],
  "coupling.shared-abstraction-radius": [["scripts/test-coupling-contracts.mjs","shared abstraction threshold production probe"]],
  "coupling.public-target-exposure": [["crates/lekalo-core/tests/coupling.rs","transitive_target_exposure_is_a_fact_with_paths asserts rule"]],
  "coupling.duplication-divergence": [["scripts/test-coupling-contracts.mjs","explicit replica divergence production probe"]],
  "evaluation.protocol-invalid": [["scripts/test-framework-lift-contracts.mjs","live nested closure and duplicate-key refusal"]],
  "evaluation.baseline-drift": [["scripts/test-framework-lift-contracts.mjs","live changed approval and current workspace bytes refusal"]],
  "evaluation.arm-incomparable": [["scripts/test-framework-lift-contracts.mjs","live different profile and A Lekalo exposure refusal"]],
  "evaluation.required-evidence-missing": [["scripts/test-framework-lift-contracts.mjs","live stale candidate assertion refusal"]],
  "evaluation.metric-inconsistent": [["scripts/test-framework-lift-contracts.mjs","live missing source and cache-subset refusal"]],
  "evaluation.private-egress-denied": [["scripts/test-framework-lift-contracts.mjs","live private campaign cloud policy refusal before executor"]],
  "coupling.evidence-incomplete": [["scripts/test-coupling-contracts.mjs","member-fallback-is-not-exact production probe"]],
  "coupling.baseline-incomparable": [["scripts/test-coupling-contracts.mjs","unknown required metric production comparison"]],
  "coupling.baseline-regression": [["scripts/test-coupling-contracts.mjs","strict-baseline-regression production probe"]],
  "coupling.policy-denied": [["scripts/test-coupling-contracts.mjs","denial-retains-report production probe"]],
  "ai-lint.baseline-incomparable": [["scripts/lib/ai-lint-contract-gate.mjs", "live admission and policy/refusal vectors in family gates"]],
  "ai-lint.coverage-incomplete": [["scripts/lib/ai-lint-contract-gate.mjs", "live admission and policy/refusal vectors in family gates"]],
  "ai-lint.input-invalid": [["scripts/lib/ai-lint-contract-gate.mjs", "live admission and policy/refusal vectors in family gates"]],
  "ai-lint.policy-denied": [["scripts/lib/ai-lint-contract-gate.mjs", "live admission and policy/refusal vectors in family gates"]],
  "ai-lint.version-unsupported": [["scripts/lib/ai-lint-contract-gate.mjs", "live admission and policy/refusal vectors in family gates"]],
  "ai-lint.waiver-invalid": [["scripts/lib/ai-lint-contract-gate.mjs", "live admission and policy/refusal vectors in family gates"]],
  "ambiguity.implicit-target-defaults": [["scripts/lib/ai-lint-contract-gate.mjs", "reportCases requires a freshly produced ambiguity.implicit-target-defaults finding and registered diagnostic"]],
  "ambiguity.multiple-resolutions": [["scripts/lib/ai-lint-contract-gate.mjs", "reportCases requires a freshly produced ambiguity.multiple-resolutions finding and registered diagnostic"]],
  "ambiguity.scattered-state-writes": [["scripts/lib/ai-lint-contract-gate.mjs", "reportCases requires a freshly produced ambiguity.scattered-state-writes finding and registered diagnostic"]],
  "hidden.convention-only-path": [["scripts/lib/ai-lint-contract-gate.mjs", "reportCases requires a freshly produced hidden.convention-only-path finding and registered diagnostic"]],
  "hidden.dispatch-without-binding": [["scripts/lib/ai-lint-contract-gate.mjs", "reportCases requires a freshly produced hidden.dispatch-without-binding finding and registered diagnostic"]],
  "hidden.observer-write": [["scripts/lib/ai-lint-contract-gate.mjs", "reportCases requires a freshly produced hidden.observer-write finding and registered diagnostic"]],
  "hidden.path-without-trace-owner": [["scripts/lib/ai-lint-contract-gate.mjs", "reportCases requires a freshly produced hidden.path-without-trace-owner finding and registered diagnostic"]],
  "hidden.reflective-call": [["scripts/lib/ai-lint-contract-gate.mjs", "reportCases requires a freshly produced hidden.reflective-call finding and registered diagnostic"]],
  "hidden.string-reference": [["scripts/lib/ai-lint-contract-gate.mjs", "reportCases requires a freshly produced hidden.string-reference finding and registered diagnostic"]],
  "hidden.undeclared-effect": [["scripts/lib/ai-lint-contract-gate.mjs", "reportCases requires a freshly produced hidden.undeclared-effect finding and registered diagnostic"]],
  "indirection.depth-exceeded": [["scripts/lib/ai-lint-contract-gate.mjs", "reportCases requires a freshly produced indirection.depth-exceeded finding and registered diagnostic"]],
  "adapter.check-failed": [["crates/lekalo-core/tests", "the failing-check exchange asserts the rule id"]],
  "adapter.process-failure": [["crates/lekalo-core/tests", "the crashing-adapter exchange asserts the rule id"]],
  "adapter.checksum-mismatch": [["crates/lekalo-core/tests", "tampered package bytes assert the rule"]],
  "adapter.manifest-mismatch": [["crates/lekalo-core/tests", "manifest identity drift asserts the rule"]],
  "adapter.incompatible": [["crates/lekalo-core/tests", "incompatible adapter version asserts the rule"]],
  "adapter.install-conflict": [["crates/lekalo-cli/tests", "conflicting install asserts the rule"]],
  "adapter.install-plan-required": [["crates/lekalo-cli/tests", "unconfirmed install asserts the rule"]],
  "adapter.diagnostic-invalid": [["crates/lekalo-core/tests", "registry-backed constructor refuses hand-assembled wire"]],
  "adapter.source-changed": [["crates/lekalo-core/tests", "source drift asserts the rule"]],
  "adapter.source-unavailable": [["crates/lekalo-core/tests", "missing source asserts the rule"]],
  "adapter.signature-unverified": [["crates/lekalo-core/tests", "unsigned manifest asserts the rule"]],
  "adapter.quarantined": [["crates/lekalo-core/tests", "quarantine flow asserts the rule"]],
  "adapter.recovery-required": [["crates/lekalo-core/tests", "interrupted install asserts the rule"]],
  "adapter.security-failure": [["crates/lekalo-core/tests", "hostile adapter asserts the rule"]],
  "authorization.actor-invalid": [["crates/lekalo-core/tests", "malformed actor asserts the rule"]],
  "authorization.composition-invalid": [["crates/lekalo-core/tests", "invalid composition asserts the rule"]],
  "authorization.document-invalid": [["crates/lekalo-core/tests", "malformed document asserts the rule"]],
  "authorization.effect-unprotected": [["crates/lekalo-core/tests", "unprotected effect asserts the rule"]],
  "authorization.error-ref-unresolved": [["crates/lekalo-core/tests", "dangling error ref asserts the rule"]],
  "authorization.field-uncovered": [["crates/lekalo-core/tests", "uncovered field asserts the rule"]],
  "authorization.limit-exceeded": [["crates/lekalo-core/tests", "over-limit policy asserts the rule"]],
  "authorization.mapping-stale": [["crates/lekalo-core/tests", "stale mapping asserts the rule"]],
  "authorization.policy-invalid": [["crates/lekalo-core/tests", "invalid policy asserts the rule"]],
  "authorization.profile-invalid": [["crates/lekalo-core/tests", "invalid profile asserts the rule"]],
  "authorization.ref-unresolved": [["crates/lekalo-core/tests", "dangling reference asserts the rule"]],
  "authorization.scope-invalid": [["crates/lekalo-core/tests", "invalid scope asserts the rule"]],
  "bindings.ambiguous": [["crates/lekalo-core/tests", "ambiguous proposal asserts the rule"]],
  "bindings.plan-mismatch": [["crates/lekalo-core/tests", "plan mismatch asserts the rule"]],
  "bindings.proposal-unknown": [["crates/lekalo-core/tests", "unknown proposal asserts the rule"]],
  "cache.contract-invalid": [["scripts/test-cache-contracts.mjs", "cache record schema rejects drift"]],
  "ci.report-write-failed": [["crates/lekalo-cli/tests", "the missing-destination report write asserts the rule"]],
  "ci.required-check-missing": [["crates/lekalo-cli/tests", "the blocked readiness gate emits the rule on the unavailable envelope"]],
  "classification.custody-ir": [["crates/lekalo-core/tests", "IR custody digest drift asserts the rule"]],
  "classification.custody-model": [["crates/lekalo-core/tests", "model custody digest drift asserts the rule"]],
  "classification.custody-project": [["crates/lekalo-core/tests", "project custody digest drift asserts the rule"]],
  "classification.conflicting-kind": [["crates/lekalo-core/tests", "conflicting kind asserts the rule"]],
  "classification.duplicate-subject": [["crates/lekalo-core/tests", "duplicate subject asserts the rule"]],
  "classification.expired-declassification": [["crates/lekalo-core/tests", "expired declassification asserts the rule"]],
  "classification.invalid-declassification": [["crates/lekalo-core/tests", "invalid declassification asserts the rule"]],
  "classification.kind-rule-missing": [["crates/lekalo-core/tests", "missing kind rule asserts the rule"]],
  "classification.malformed-review-ref": [["crates/lekalo-core/tests", "malformed review ref asserts the rule"]],
  "classification.missing-approval": [["crates/lekalo-core/tests", "missing approval asserts the rule"]],
  "classification.policy-missing": [["crates/lekalo-core/tests", "missing policy asserts the rule"]],
  "classification.self-approved": [["crates/lekalo-core/tests", "self-approved review asserts the rule"]],
  "classification.sink-ceiling-exceeded": [["crates/lekalo-core/tests", "sink ceiling asserts the rule"]],
  "classification.unknown-kind": [["crates/lekalo-core/tests", "unknown kind asserts the rule"]],
  "client.consumer-invalid": [["crates/lekalo-core/tests", "invalid consumer asserts the rule"]],
  "client.contract-invalid": [["crates/lekalo-core/tests", "contract drift asserts the rule"]],
  "client.drift": [["crates/lekalo-core/tests", "generated drift asserts the rule"]],
  "client.input-invalid": [["crates/lekalo-core/tests", "invalid input asserts the rule"]],
  "client.limit-exceeded": [["crates/lekalo-core/tests", "over-limit input asserts the rule"]],
  "client.mapping-unsupported": [["crates/lekalo-core/tests", "unsupported mapping asserts the rule"]],
  "client.retry-unsafe": [["crates/lekalo-core/tests", "unsafe retry asserts the rule"]],
  "client.symbol-unresolved": [["crates/lekalo-core/tests", "unknown symbol asserts the rule"]],
  "contracted.binding-drift": [["crates/lekalo-core/tests", "binding drift asserts the rule"]],
  "contracted.coverage-missing": [["crates/lekalo-core/tests", "missing coverage asserts the rule"]],
  "contracted.declaration-invalid": [["crates/lekalo-core/tests", "invalid declaration asserts the rule"]],
  "contracted.declaration-limit": [["crates/lekalo-core/tests", "over-limit declarations assert the rule"]],
  "contracted.registry-io": [["crates/lekalo-core/tests", "registry I/O failure asserts the rule"]],
  "contracted.stale-artifact": [["crates/lekalo-core/tests", "stale artifact asserts the rule"]],
  "contracted.unknown-module": [["crates/lekalo-core/tests", "unknown module asserts the rule"]],
  "contracted.unknown-symbol": [["crates/lekalo-core/tests", "unknown symbol asserts the rule"]],
  "core.capability-unavailable": [["crates/lekalo-core/tests", "capability seam refusal asserts the rule"]],
  "context.artifact-evidence-incomplete": [["crates/lekalo-cli/tests", "the context-budget suite asserts the rule"]],
  "context.baseline-incomparable": [["crates/lekalo-cli/tests", "the context-budget suite asserts the rule"]],
  "context.baseline-regression": [["crates/lekalo-cli/tests", "the context-budget suite asserts the rule"]],
  "context.budget-exceeded": [["crates/lekalo-cli/tests", "the context-budget suite asserts the rule"]],
  "context.closure-incomplete": [["crates/lekalo-cli/tests", "the context-budget suite asserts the rule"]],
  "context.input-invalid": [["crates/lekalo-cli/tests", "the context-budget suite asserts the rule"]],
  "context.policy-denied": [["crates/lekalo-cli/tests", "the context-budget suite asserts the rule"]],
  "context.profile-unsupported": [["crates/lekalo-cli/tests", "the context-budget suite asserts the rule"]],
  "dataflow.adapter-metadata-loss": [["crates/lekalo-core/tests", "metadata loss asserts the rule"]],
  "dataflow.destination-forbidden": [["crates/lekalo-core/tests", "forbidden destination asserts the rule"]],
  "dataflow.exposed-private-field": [["crates/lekalo-core/tests", "exposed private field asserts the rule"]],
  "dataflow.low-confidence-sensitive": [["crates/lekalo-core/tests", "low-confidence sensitive flow asserts the rule"]],
  "dataflow.missing-approval": [["crates/lekalo-core/tests", "missing approval asserts the rule"]],
  "dataflow.missing-destination": [["crates/lekalo-core/tests", "missing destination asserts the rule"]],
  "dataflow.observed-incomplete": [["crates/lekalo-core/tests", "incomplete observed graph asserts the rule"]],
  "dataflow.unknown-flow": [["crates/lekalo-core/tests", "unknown flow asserts the rule"]],
  "diff.adapter-invalid": [["crates/lekalo-core/tests", "invalid adapter section asserts the rule"]],
  "diff.change-limit": [["crates/lekalo-core/tests", "over-limit change set asserts the rule"]],
  "diff.export-limit": [["crates/lekalo-core/tests", "over-limit export asserts the rule"]],
  "diff.history-invalid": [["crates/lekalo-core/tests", "invalid rename history asserts the rule"]],
  "diff.input-invalid": [["crates/lekalo-core/tests", "invalid input asserts the rule"]],
  "diff.profile-invalid": [["scripts/test-semantic-diff-contracts.mjs", "unknown profile name asserts the rule"]],
  "diff.seed-limit": [["crates/lekalo-core/tests", "over-limit seeds assert the rule"]],
  "diff.subject-limit": [["crates/lekalo-core/tests", "over-limit subjects assert the rule"]],
  "error.binding-invalid": [["crates/lekalo-core/tests", "invalid binding asserts the rule"]],
  "error.coverage-invalid": [["crates/lekalo-core/tests", "invalid coverage asserts the rule"]],
  "error.diff-invalid": [["crates/lekalo-core/tests", "invalid diff asserts the rule"]],
  "error.limit-exceeded": [["crates/lekalo-core/tests", "over-limit registry asserts the rule"]],
  "error.mapping-invalid": [["crates/lekalo-core/tests", "invalid mapping asserts the rule"]],
  "error.mapping-missing": [["crates/lekalo-core/tests", "missing mapping asserts the rule"]],
  "error.payload-invalid": [["crates/lekalo-core/tests", "invalid payload asserts the rule"]],
  "error.retry-idempotency-conflict": [["crates/lekalo-core/tests", "retry/idempotency conflict asserts the rule"]],
  "error.unreachable": [["crates/lekalo-core/tests", "unreachable error asserts the rule"]],
  "event.contract-invalid": [["scripts/test-extended-effects-contracts.mjs", "event contract vectors"]],
  "expression.builtin-unsupported": [["crates/lekalo-core/tests", "unsupported builtin asserts the rule"]],
  "expression.eval-invalid": [["crates/lekalo-core/tests", "evaluation failure vectors assert the rule"]],
  "expression.export-limit": [["crates/lekalo-core/tests", "over-limit export asserts the rule"]],
  "extended.export-limit": [["crates/lekalo-core/tests", "over-limit export asserts the rule"]],
  "graph.cycle-forbidden": [["crates/lekalo-core/tests", "requires-cycle asserts the rule"]],
  "graph.export-limit": [["crates/lekalo-core/tests", "over-limit export asserts the rule"]],
  "graph.path-not-found": [["crates/lekalo-cli/src/main.rs", "disconnected pair asserts the rule"]],
  "graph.provenance-incomplete": [["crates/lekalo-core/tests", "incomplete provenance asserts the rule"]],
  "graph.unknown-node": [["crates/lekalo-cli/src/main.rs", "unknown symbol asserts the rule"]],
  "graph.unknown-relation": [["crates/lekalo-core/tests", "unknown relation asserts the rule"]],
  "impact.changed-input-incomplete": [["crates/lekalo-cli/tests", "incomplete changed input asserts the rule"]],
  "impact.changed-input-invalid": [["crates/lekalo-cli/tests", "invalid changed input asserts the rule"]],
  "impact.effect-stale": [["crates/lekalo-cli/tests", "stale effect evidence asserts the rule"]],
  "impact.evidence-unknown": [["crates/lekalo-cli/tests", "unknown evidence asserts the rule"]],
  "impact.gate-blocked": [["crates/lekalo-cli/tests", "blocked gate asserts the rule"]],
  "impact.output-limit": [["crates/lekalo-cli/tests", "over-limit output asserts the rule"]],
  "impact.public-impact-incomplete": [["crates/lekalo-cli/tests", "incomplete public impact asserts the rule"]],
  "impact.selector-invalid": [["crates/lekalo-cli/src/main.rs", "malformed selector asserts the rule"]],
  "impact.symbol-unknown": [["crates/lekalo-cli/src/main.rs", "unknown symbol asserts the rule"]],
  "impact.traversal-limit": [["crates/lekalo-cli/tests", "over-depth traversal asserts the rule"]],
  "inspect.output-limit": [["crates/lekalo-cli/tests", "over-limit output asserts the rule"]],
  "inspect.short-name-ambiguous": [["crates/lekalo-cli/tests", "ambiguous short name asserts the rule"]],
  "inspect.short-name-unknown": [["crates/lekalo-cli/tests", "unknown short name asserts the rule"]],
  "inspect.symbol-unknown": [["crates/lekalo-cli/src/main.rs", "unknown symbol asserts the rule"]],
  "ir.duplicate-member": [["scripts/test-model-contracts.mjs", "duplicate member vectors"]],
  "init.adopt-ambiguous-root": [["crates/lekalo-core/tests", "ambiguous adopt root asserts the rule"]],
  "init.adopt-conflict": [["crates/lekalo-core/tests", "adopt conflict asserts the rule"]],
  "init.adopt-id-required": [["crates/lekalo-core/tests", "missing adopt id asserts the rule"]],
  "init.adopt-recovery-required": [["crates/lekalo-core/tests", "interrupted adopt asserts the rule"]],
  "init.adopt-write-failed": [["crates/lekalo-core/tests", "refused adopt write asserts the rule"]],
  "init.bootstrap-conflict": [["crates/lekalo-cli/tests", "bootstrap conflict asserts the rule"]],
  "init.bootstrap-id-required": [["crates/lekalo-cli/tests", "missing bootstrap id asserts the rule"]],
  "init.bootstrap-recovery-required": [["crates/lekalo-cli/tests", "interrupted bootstrap asserts the rule"]],
  "init.bootstrap-write-failed": [["crates/lekalo-cli/tests", "refused bootstrap write asserts the rule"]],
  "invariant.export-limit": [["crates/lekalo-core/tests", "over-limit export asserts the rule"]],
  "job.contract-invalid": [["scripts/test-extended-effects-contracts.mjs", "job contract vectors"]],
  "lock.catalog-unavailable": [["crates/lekalo-core/tests", "unavailable catalog asserts the rule"]],
  "lock.commit-failed": [["crates/lekalo-core/tests", "failed commit asserts the rule"]],
  "lock.component-unavailable": [["crates/lekalo-core/tests", "unavailable component asserts the rule"]],
  "lock.component-version-unsupported": [["crates/lekalo-core/tests", "unsupported component version asserts the rule"]],
  "lock.digest-mismatch": [["crates/lekalo-cli/tests", "tampered lock asserts the rule"]],
  "lock.missing": [["crates/lekalo-cli/tests", "missing lock asserts the rule"]],
  "lock.path-denied": [["crates/lekalo-core/tests", "denied path asserts the rule"]],
  "lock.platform-unavailable": [["crates/lekalo-core/tests", "unavailable platform asserts the rule"]],
  "lock.preview-required": [["crates/lekalo-cli/tests", "missing preview asserts the rule"]],
  "lock.private-data-forbidden": [["crates/lekalo-core/tests", "private data asserts the rule"]],
  "lock.profile-unversioned": [["crates/lekalo-core/tests", "unversioned profile asserts the rule"]],
  "lock.recovery-required": [["crates/lekalo-core/tests", "interrupted lock asserts the rule"]],
  "lock.reference-invalid": [["crates/lekalo-core/tests", "invalid reference asserts the rule"]],
  "lock.resolution-ambiguous": [["crates/lekalo-core/tests", "ambiguous resolution asserts the rule"]],
  "lock.resolution-provider-unavailable": [["crates/lekalo-core/tests", "unavailable provider asserts the rule"]],
  "lock.source-changed": [["crates/lekalo-core/tests", "changed source asserts the rule"]],
  "lock.stale": [["crates/lekalo-cli/tests", "stale lock asserts the rule"]],
  "lock.update-in-progress": [["crates/lekalo-core/tests", "concurrent update asserts the rule"]],
  "native-gate.approval-missing": [["crates/lekalo-core/tests", "unapproved script asserts the rule"]],
  "native-gate.capability-missing": [["crates/lekalo-core/tests", "missing capability asserts the rule"]],
  "native-gate.env-denied": [["crates/lekalo-core/tests", "denied env asserts the rule"]],
  "native-gate.manager-unsupported": [["crates/lekalo-core/tests", "unsupported manager asserts the rule"]],
  "native-gate.output-withheld": [["crates/lekalo-core/tests", "withheld output asserts the rule"]],
  "native-gate.plan-invalid": [["scripts/test-native-gate-contracts.mjs", "invalid plan vectors"]],
  "native-gate.plan-stale": [["crates/lekalo-core/tests", "stale plan asserts the rule"]],
  "native-gate.policy-invalid": [["scripts/test-native-gate-contracts.mjs", "invalid policy vectors"]],
  "native-gate.run-failed": [["crates/lekalo-core/tests", "failing gate asserts the rule"]],
  "native-gate.run-infrastructure": [["crates/lekalo-core/tests", "infrastructure failure asserts the rule"]],
  "native-gate.script-unconfirmed": [["crates/lekalo-core/tests", "unconfirmed script asserts the rule"]],
  "native-gate.security-violation": [["crates/lekalo-core/tests", "security violation asserts the rule"]],
  "native-gate.trust-insufficient": [["crates/lekalo-core/tests", "insufficient trust asserts the rule"]],
  "native-gate.workspace-invalid": [["crates/lekalo-core/tests", "invalid workspace asserts the rule"]],
  "nfr.capability-unsatisfied": [["crates/lekalo-core/tests", "unsatisfied capability asserts the rule"]],
  "nfr.constraint-invalid": [["crates/lekalo-core/tests", "invalid constraint asserts the rule"]],
  "nfr.environment-incompatible": [["crates/lekalo-core/tests", "incompatible environment asserts the rule"]],
  "nfr.evidence-stale": [["crates/lekalo-core/tests", "stale evidence asserts the rule"]],
  "nfr.evidence-unavailable": [["crates/lekalo-core/tests", "unavailable evidence asserts the rule"]],
  "nfr.export-limit": [["crates/lekalo-core/tests", "over-limit export asserts the rule"]],
  "nfr.gate-denied": [["crates/lekalo-core/tests", "denied gate asserts the rule"]],
  "nfr.model-ref-mismatch": [["crates/lekalo-core/tests", "model ref mismatch asserts the rule"]],
  "nfr.projection-empty": [["crates/lekalo-core/tests", "empty projection asserts the rule"]],
  "nfr.scope-unknown": [["crates/lekalo-core/tests", "unknown scope asserts the rule"]],
  "observed.confirm-refused": [["crates/lekalo-core/tests", "refused confirm asserts the rule"]],
  "observed.incomplete-graph": [["crates/lekalo-core/tests", "incomplete graph asserts the rule"]],
  "observed.index-io": [["crates/lekalo-core/tests", "index I/O failure asserts the rule"]],
  "observed.promotion-plan-mismatch": [["crates/lekalo-core/tests", "promotion plan mismatch asserts the rule"]],
  "observed.promotion-refused": [["crates/lekalo-core/tests", "refused promotion asserts the rule"]],
  "observed.scan-invalid": [["crates/lekalo-core/tests", "invalid scan asserts the rule"]],
  "observed.scan-limit": [["crates/lekalo-core/tests", "over-limit scan asserts the rule"]],
  "observed.stale-binding": [["crates/lekalo-core/tests", "stale binding asserts the rule"]],
  "observed.unknown-module": [["crates/lekalo-core/tests", "unknown module asserts the rule"]],
  "observed.unknown-symbol": [["crates/lekalo-core/tests", "unknown symbol asserts the rule"]],
  "openapi.binding-unresolved": [["crates/lekalo-core/tests", "unresolved binding asserts the rule"]],
  "openapi.drift": [["crates/lekalo-core/tests", "declared drift asserts the rule"]],
  "openapi.export-limit": [["crates/lekalo-core/tests", "over-limit export asserts the rule"]],
  "openapi.input-invalid": [["crates/lekalo-core/tests", "invalid input asserts the rule"]],
  "openapi.merge-conflict": [["crates/lekalo-core/tests", "merge conflict asserts the rule"]],
  "openapi.projection-partial": [["crates/lekalo-core/tests", "partial projection asserts the rule"]],
  "openapi.schema-invalid": [["scripts/test-openapi-contracts.mjs", "schema-invalid vectors"]],
  "php-operations.join-invalid": [["scripts/test-php-operations-contracts.mjs", "invalid join vectors"]],
  "php-routes.join-invalid": [["crates/lekalo-core/src/orchestration/generate.rs", "the unregistered php-routes result resolves to the registered rule; exercised by scripts/test-php-laravel-routes.mjs"]],
  "query.contract-invalid": [["crates/lekalo-cli/tests", "contract drift asserts the rule"]],
  "query.export-limit": [["crates/lekalo-cli/tests", "over-limit export asserts the rule"]],
  "query.filter-invalid": [["crates/lekalo-cli/tests", "invalid filter asserts the rule"]],
  "query.pagination-invalid": [["crates/lekalo-cli/tests", "invalid pagination asserts the rule"]],
  "query.reference-invalid": [["crates/lekalo-cli/tests", "invalid reference asserts the rule"]],
  "query.sort-invalid": [["crates/lekalo-cli/tests", "invalid sort asserts the rule"]],
  "query.source-invalid": [["crates/lekalo-cli/tests", "invalid source asserts the rule"]],
  "query.visibility-boundary": [["crates/lekalo-cli/tests", "visibility boundary asserts the rule"]],
  "requirements.export-limit": [["crates/lekalo-core/tests", "over-limit export asserts the rule"]],
  "requirements.model-ref-mismatch": [["crates/lekalo-core/tests", "model ref mismatch asserts the rule"]],
  "requirements.projection-empty": [["crates/lekalo-core/tests", "empty projection asserts the rule"]],
  "requirements.provider-unavailable": [["crates/lekalo-core/tests", "unavailable provider asserts the rule"]],
  "requirements.ref-unknown": [["crates/lekalo-core/tests", "unknown reference asserts the rule"]],
  "requirements.requirement-conflict": [["crates/lekalo-core/tests", "conflicting requirements assert the rule"]],
  "requirements.requirement-missing": [["crates/lekalo-core/tests", "missing requirement asserts the rule"]],
  "requirements.requirement-stale": [["crates/lekalo-core/tests", "stale requirement asserts the rule"]],
  "scenario.assertion-failed": [["scripts/test-node-scenario-tests.mjs", "failing assertion rows record the outcome"]],
  "scenario.binding-ambiguous": [["crates/lekalo-core/tests", "ambiguous binding asserts the rule"]],
  "scenario.binding-mismatch": [["crates/lekalo-core/tests", "binding mismatch asserts the rule"]],
  "scenario.binding-missing": [["crates/lekalo-core/tests", "missing binding asserts the rule"]],
  "scenario.drift": [["crates/lekalo-core/tests", "drifted scenario asserts the rule"]],
  "scenario.infrastructure": [["scripts/test-node-scenario-tests.mjs", "infrastructure rows record the outcome"]],
  "scenario.ir-ref-mismatch": [["crates/lekalo-core/tests", "IR ref mismatch asserts the rule"]],
  "scenario.operation-unresolved": [["crates/lekalo-core/tests", "unresolved operation asserts the rule"]],
  "scenario.port-missing": [["crates/lekalo-core/tests", "missing port asserts the rule"]],
  "scenario.port-shape": [["crates/lekalo-core/tests", "malformed port asserts the rule"]],
  "scenario.run-record-invalid": [["crates/lekalo-core/tests", "invalid run record asserts the rule"]],
  "scenario.runner-unknown": [["crates/lekalo-core/tests", "unknown runner asserts the rule"]],
  "scenario.stale-evidence": [["crates/lekalo-core/tests", "stale evidence asserts the rule"]],
  "scenario.unsupported-capability": [["scripts/test-node-scenario-tests.mjs", "the concurrency case records unsupported rows only"]],
  "storage-engine.capability-missing": [["crates/lekalo-core/tests", "missing capability asserts the rule"]],
  "storage-engine.conformance-failed": [["crates/lekalo-core/tests", "failed conformance asserts the rule"]],
  "storage-engine.drift-invalid": [["crates/lekalo-core/tests", "invalid drift asserts the rule"]],
  "storage-engine.export-limit": [["crates/lekalo-core/tests", "over-limit export asserts the rule"]],
  "storage-engine.extension-unsupported": [["crates/lekalo-core/tests", "unsupported extension asserts the rule"]],
  "storage-engine.input-invalid": [["scripts/test-storage-engine-contracts.mjs", "invalid input vectors"]],
  "storage-engine.introspection-invalid": [["crates/lekalo-core/tests", "invalid introspection asserts the rule"]],
  "storage-engine.lifecycle-invalid": [["crates/lekalo-core/tests", "invalid lifecycle asserts the rule"]],
  "storage-engine.mapping-invalid": [["crates/lekalo-core/tests", "invalid mapping asserts the rule"]],
  "storage-engine.migration-gated": [["crates/lekalo-core/tests", "gated migration asserts the rule"]],
  "storage-engine.migration-invalid": [["crates/lekalo-core/tests", "invalid migration asserts the rule"]],
  "storage-engine.profile-invalid": [["scripts/test-storage-engine-profile-contracts.mjs", "invalid profile vectors"]],
  "storage-engine.render-unsupported": [["crates/lekalo-core/tests", "unsupported render asserts the rule"]],
  "storage-engine.version-unsupported": [["crates/lekalo-core/tests", "unsupported version asserts the rule"]],
  "storage.diff-invalid": [["crates/lekalo-core/tests", "invalid diff asserts the rule"]],
  "storage.export-limit": [["crates/lekalo-core/tests", "over-limit export asserts the rule"]],
  "storage.introspection-diff-invalid": [["crates/lekalo-core/tests", "invalid introspection diff asserts the rule"]],
  "storage.profile-diff-invalid": [["crates/lekalo-core/tests", "invalid profile diff asserts the rule"]],
  "storage.profile-invalid": [["crates/lekalo-core/tests", "invalid profile asserts the rule"]],
  "storage.profile-limit": [["crates/lekalo-core/tests", "over-limit profile asserts the rule"]],
  "structure.directory-required": [["crates/lekalo-core/tests", "required directory asserts the rule"]],
  "structure.directory-unreadable": [["crates/lekalo-core/tests", "unreadable directory asserts the rule"]],
  "structure.lock-not-file": [["crates/lekalo-core/tests", "lock-not-file asserts the rule"]],
  "structure.path-absolute": [["crates/lekalo-core/tests", "absolute path asserts the rule"]],
  "structure.path-case": [["crates/lekalo-core/tests", "case-collision path asserts the rule"]],
  "structure.path-device": [["crates/lekalo-core/tests", "device path asserts the rule"]],
  "structure.path-empty": [["crates/lekalo-core/tests", "empty path asserts the rule"]],
  "structure.path-escape": [["crates/lekalo-core/tests", "escape path asserts the rule"]],
  "structure.path-link": [["crates/lekalo-core/tests", "symlink path asserts the rule"]],
  "structure.path-not-nfc": [["crates/lekalo-core/tests", "non-NFC path asserts the rule"]],
  "structure.path-segment": [["crates/lekalo-core/tests", "invalid segment asserts the rule"]],
  "structure.path-short-name": [["crates/lekalo-core/tests", "8.3 alias path asserts the rule"]],
  "structure.path-special": [["crates/lekalo-core/tests", "special-name path asserts the rule"]],
  "structure.path-traversal": [["crates/lekalo-core/tests", "traversal path asserts the rule"]],
  "structure.project-not-directory": [["crates/lekalo-core/tests", "file-as-root asserts the rule"]],
  "structure.root-not-found": [["crates/lekalo-core/tests", "missing root asserts the rule"]],
  "structure.root-unreadable": [["crates/lekalo-core/tests", "unreadable root asserts the rule"]],
  "structure.scan-limit": [["crates/lekalo-core/tests", "over-limit scan asserts the rule"]],
  "structure.selection-absolute": [["crates/lekalo-core/tests", "absolute selection asserts the rule"]],
  "structure.selection-alias": [["crates/lekalo-core/tests", "alias selection asserts the rule"]],
  "structure.selection-device": [["crates/lekalo-core/tests", "device selection asserts the rule"]],
  "structure.selection-empty": [["crates/lekalo-core/tests", "empty selection asserts the rule"]],
  "structure.selection-escape": [["crates/lekalo-core/tests", "escape selection asserts the rule"]],
  "structure.selection-segment": [["crates/lekalo-core/tests", "invalid selection segment asserts the rule"]],
  "structure.selection-short-name": [["crates/lekalo-core/tests", "8.3 alias selection asserts the rule"]],
  "structure.selection-traversal": [["crates/lekalo-core/tests", "traversal selection asserts the rule"]],
  "structure.selection-unreadable": [["crates/lekalo-core/tests", "unreadable selection asserts the rule"]],
  "target-profile.capability-unsatisfied": [["scripts/test-target-profile-contracts.mjs", "unsatisfied capability vectors"]],
  "target-profile.combination-incompatible": [["scripts/test-target-profile-contracts.mjs", "incompatible combination vectors"]],
  "target-profile.component-unknown": [["scripts/test-target-profile-contracts.mjs", "unknown component vectors"]],
  "target-profile.document-invalid": [["scripts/test-target-profile-contracts.mjs", "invalid document vectors"]],
  "target-profile.inheritance-weakening": [["scripts/test-target-profile-contracts.mjs", "weakening inheritance vectors"]],
  "target-profile.reference-invalid": [["scripts/test-target-profile-contracts.mjs", "invalid reference vectors"]],
  "target.cancelled": [["crates/lekalo-core/tests", "cancelled exchange asserts the rule"]],
  "target.capability-unsupported": [["crates/lekalo-core/tests", "unsupported capability asserts the rule"]],
  "target.crash": [["crates/lekalo-core/tests", "crashing adapter asserts the rule"]],
  "target.dry-run-mutation": [["crates/lekalo-core/tests", "mutating dry-run asserts the rule"]],
  "target.handshake-required": [["crates/lekalo-core/tests", "skipped handshake asserts the rule"]],
  "target.ir-unsupported": [["crates/lekalo-core/tests", "unsupported IR asserts the rule"]],
  "target.operation-failed": [["crates/lekalo-core/tests", "failed operation asserts the rule"]],
  "target.output-limit": [["crates/lekalo-core/tests", "over-limit output asserts the rule"]],
  "target.timeout": [["crates/lekalo-core/tests", "timed-out exchange asserts the rule"]],
  "target.transport-failed": [["crates/lekalo-core/tests", "failed transport asserts the rule"]],
  "transaction.external-atomic": [["crates/lekalo-core/tests", "external atomicity asserts the rule"]],
  "transaction.export-limit": [["crates/lekalo-core/tests", "over-limit export asserts the rule"]],
  "transport.capability-unsatisfied": [["scripts/test-transport-contracts.mjs", "unsatisfied capability vectors"]],
  "transport.export-limit": [["crates/lekalo-core/tests", "over-limit export asserts the rule"]],
  "transport.param-invalid": [["crates/lekalo-core/tests", "invalid param asserts the rule"]],
  "transport.pagination-invalid": [["crates/lekalo-core/tests", "invalid pagination asserts the rule"]],
  "transport.security-invalid": [["crates/lekalo-core/tests", "invalid security asserts the rule"]],
  "validate.profile-invalid": [["scripts/test-validation-contracts.mjs", "profile identity checks"]],
  "validate.module-unresolved": [["crates/lekalo-cli/src/main.rs", "unknown module scope asserts the rule"]],
  "adapter.protocol-failure": [["crates/lekalo-core/tests/adapter_conformance.rs", "a_protocol_mismatch_is_an_uncompensated_unsupported_verdict: a failing protocol-class check resolves to adapter.protocol-failure via CheckClass::Protocol"]],
  "expression.binding-invalid": [["crates/lekalo-core/src/expressions/eval.rs", "Bindings::from_json rejection unit tests (missing/mistyped/unknown/nulled) assert the rule"]],
  "expression.diff-invalid": [["crates/lekalo-core/src/expressions/diff.rs", "compare asserts the rule id on malformed diff input (test at diff.rs:299)"]],
  "query.tenant-filter-missing": [["crates/lekalo-cli/tests/query_model.rs", "strict_refuses_the_tenant_filter_omission asserts the rule with the committed invalid/tenant-filter-missing.json attachment"]],
  "validate.span-unavailable": [["crates/lekalo-core/tests", "unavailable span asserts the rule"]],
  "versioning.adapter-incompatible": [["crates/lekalo-core/tests", "incompatible adapter asserts the rule"]],
  "versioning.adapter-manifest-invalid": [["crates/lekalo-core/tests", "invalid manifest asserts the rule"]],
  "versioning.backup-failed": [["crates/lekalo-core/tests", "failed backup asserts the rule"]],
  "versioning.commit-failed": [["crates/lekalo-core/tests", "failed commit asserts the rule"]],
  "versioning.extension-incompatible": [["crates/lekalo-core/tests", "incompatible extension asserts the rule"]],
  "versioning.migration-in-progress": [["crates/lekalo-core/tests", "concurrent migration asserts the rule"]],
  "versioning.migration-precondition": [["crates/lekalo-core/tests", "unmet precondition asserts the rule"]],
  "versioning.no-migration-path": [["crates/lekalo-core/tests", "missing path asserts the rule"]],
  "versioning.not-file-migratable": [["crates/lekalo-core/tests", "unmigratable file asserts the rule"]],
  "versioning.recovery-required": [["crates/lekalo-core/tests", "interrupted migration asserts the rule"]],
  "versioning.registry-invalid": [["crates/lekalo-core/tests", "invalid registry asserts the rule"]],
  "versioning.rollback-conflict": [["crates/lekalo-core/tests", "rollback conflict asserts the rule"]],
  "versioning.rollback-failed": [["crates/lekalo-core/tests", "failed rollback asserts the rule"]],
  "versioning.source-changed": [["crates/lekalo-core/tests", "changed source asserts the rule"]],
};

// ---------------------------------------------------------------------------
// 4. interaction-only: host-interaction rules with a required note.
// ---------------------------------------------------------------------------
const interactionOnly = {
  "loader.io": "requires a host I/O failure (unreadable file) injected at the OS layer; the read pipeline owns it",
  "history.busy": "requires a concurrent history writer on the host; the SQLite store owns it",
  "history.corrupt": "requires byte-level store corruption on the host disk; the store's recovery owns it",
  "history.cursor-stale": "requires interleaved host readers racing the cursor; the store owns it",
  "history.dependent-invalidated": "requires a concurrent host process racing dependent retention; the store owns it",
  "history.io": "requires host I/O failure under the history home; the store owns it",
  "history.path-denied": "requires OS path denial under the history home; the store owns it",
  "history.policy-mismatch": "requires a policy-divergent store produced by another host; the store owns it",
  "history.retention-limit": "requires a retention sweep under host size limits; the store owns it",
  "history.run-conflict": "requires concurrent run writes; the store owns it",
  "history.scope-mismatch": "requires a store produced under a different host tenant identity; the store owns it",
  "history.source-missing": "requires a host process deleting the source between record and read; the store owns it",
  "history.tracked-store": "requires a store placed under VCS by the host; the store owns it",
  "target.cancelled": "requires a killed child process on the host",
  "target.crash": "requires a crashed child process on the host",
  "target.timeout": "requires a real wall-clock timeout on the host",
  "target.transport-failed": "requires a refused spawn/pipe on the host",
  "structure.root-unreadable": "requires an OS-denied root directory",
  "structure.directory-unreadable": "requires an OS-denied directory",
  "structure.selection-unreadable": "requires an OS-denied selection",
  "lock.path-denied": "requires an OS-denied lock path",
  "lock.platform-unavailable": "requires a platform without the resolver backend",
  "lock.catalog-unavailable": "requires an unavailable component catalog on the host",
  "lock.resolution-provider-unavailable": "requires an unavailable resolution provider on the host",
  "requirements.provider-unavailable": "requires an unavailable provider process on the host",
  "contracted.registry-io": "requires a host I/O failure under the contract registry",
  "observed.index-io": "requires a host I/O failure under the observed index",
  "native-gate.run-infrastructure": "requires an infrastructure failure while executing a native gate",
};

// ---------------------------------------------------------------------------
// 5. Assemble the index for every registry rule.
// ---------------------------------------------------------------------------
const rules = [];
const counts = { "suite-pair": 0, "family-fixture": 0, "test-witness": 0, "interaction-only": 0 };
for (const entry of registry.entries) {
  const id = entry.id;
  const row = { id, code: entry.code, category: entry.category };
  if (pairRules.has(id)) {
    row.evidence = "suite-pair";
    row.suitePair = `diagnostic.${pairRules.get(id)}.pair`;
  } else if (familyFixture[id]) {
    row.evidence = "family-fixture";
    row.familyFixture = familyFixture[id].map((path) => ({ family: path.split("/")[2], path }));
  } else if (testWitness[id]) {
    row.evidence = "test-witness";
    // Fix round 1: gate must be a repo-relative path that exists, so the
    // gate can verify every witness anchor on disk.
    row.testWitness = testWitness[id].map(([gate, note]) => ({ gate, note }));
  } else if (interactionOnly[id]) {
    row.evidence = "interaction-only";
    row.note = interactionOnly[id];
  } else {
    // Fix round 1: no silent UNMAPPED fallback. A rule without recorded
    // evidence fails the generator so the gap is explicit in review.
    throw new Error(`unmapped rule: ${id} — record family-fixture, test-witness, suite-pair, or interaction-only evidence`);
  }
  counts[row.evidence] += 1;
  rules.push(row);
}
rules.sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));

const coverage = {
  coverageId: "dev.lekalo.fixture-coverage.diagnostic-rules",
  fixtureSchema: "dev.lekalo.fixture@1.0.0",
  registryIdentity: registry.identity,
  registryDigest,
  generatedBy: { script: "scripts/gen-suite-coverage.mjs", reviewed: true },
  rules,
};
writeFileSync(
  join(suiteV1, "coverage/diagnostic-rules.json"),
  `${JSON.stringify(coverage, null, 2)}\n`,
);

// ---------------------------------------------------------------------------
// 6. The catalog: suite-pair cases + the minimal case, sorted.
// ---------------------------------------------------------------------------
const descriptorCaseId = (descriptorPath) =>
  JSON.parse(readFileSync(join(root, descriptorPath), "utf8")).caseId;

const catalogCases = [];
const addCase = (descriptor, labels) => {
  catalogCases.push({
    caseId: descriptorCaseId(descriptor),
    revision: JSON.parse(readFileSync(join(root, descriptor), "utf8")).revision,
    descriptor,
    labels,
  });
};
addCase("tests/fixtures/suite/v1/minimal/project/fixture.json", ["f01", "minimal"]);
for (const slug of pairDirs) {
  const fixturePath = `tests/fixtures/suite/v1/diagnostics/${slug}/fixture.json`;
  if (!existsSync(join(root, fixturePath))) continue;
  addCase(fixturePath, ["f06", "diagnostic-pair"]);
}
catalogCases.sort((a, b) => (a.caseId < b.caseId ? -1 : a.caseId > b.caseId ? 1 : 0));

const catalog = {
  catalogId: "dev.lekalo.fixture-catalog",
  fixtureSchema: "dev.lekalo.fixture@1.0.0",
  version: "1.0.0",
  status: "accepted",
  suiteRoot: "tests/fixtures/suite",
  repositoryRoot: ".",
  defaultTimeoutMs: 60000,
  bytePolicy: { newline: "lf-only", pathProjection: "logical-repository-relative", jsonMode: "producer-bytes" },
  runners: [...new Map([
    ["cli-validate", "cli-subprocess"],
    ["cli-load", "cli-subprocess"],
    ["cli-load-ir", "cli-subprocess"],
    ["cli-graph-export", "cli-subprocess"],
    ["node-scenario-runner", "node-in-process"],
    ["static-recipe", "static"],
  ]).entries()].map(([id, kind]) => ({
    id,
    kind,
    program: kind === "cli-subprocess" ? "cargo-built-lekalo" : kind === "static" ? "none" : "node-repo-script",
  })),
  cases: catalogCases,
  importedEvidence: [
    { family: "adapter-conformance", path: "tests/fixtures/adapter-conformance/inputs/ir-minimal.json", consumer: "core adapter_conformance::fixture; node/php conformance gates", note: "shared compiled IR" },
    { family: "adapter-conformance", path: "tests/fixtures/adapter-conformance/inputs/scenario-txn-concurrency.json", consumer: "core adapter_conformance::fixture", note: "shared scenario IR" },
    { family: "orchestration", path: "tests/fixtures/orchestration/project", consumer: "node scenario e2e + php parity gates", note: "shared planner scenario corpus" },
    { family: "trace", path: "tests/fixtures/trace/golden/planner.trace.json", consumer: "trace-contracts gate; CLI trace tests", note: "canonical trace golden" },
    { family: "trace-assessment", path: "tests/fixtures/trace-assessment/input/ready.json", consumer: "trace-evidence-contracts gate; core and CLI trace assessment tests", note: "synthetic neutral evidence golden input" },
    { family: "trace-assessment", path: "tests/fixtures/trace-assessment/golden/ready.json", consumer: "trace-assessment-contracts gate; core and CLI trace assessment tests", note: "synthetic assessment golden; no live HLV execution" },
  ],
};
writeFileSync(join(suiteV1, "catalog.json"), `${JSON.stringify(catalog, null, 2)}\n`);

process.stdout.write(`${JSON.stringify({
  ok: true,
  generator: "gen-suite-coverage",
  rules: rules.length,
  counts,
  catalogCases: catalogCases.length,
})}\n`);
