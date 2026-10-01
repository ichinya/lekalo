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
const registryPath = join(root, "contracts/diagnostic-registry.v0.4.0.json");
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
};

// ---------------------------------------------------------------------------
// 3. test-witness evidence: rule -> named Rust/Node gate assertions.
//    The gate file must exist; the note names the asserting test.
// ---------------------------------------------------------------------------
const testWitness = {
  "adapter.check-failed": [["cargo:test lekalo-core adapter_conformance", "the failing-check exchange asserts the rule id"]],
  "adapter.process-failure": [["cargo:test lekalo-core adapter_conformance", "the crashing-adapter exchange asserts the rule id"]],
  "adapter.checksum-mismatch": [["cargo:test lekalo-core adapter_package", "tampered package bytes assert the rule"]],
  "adapter.manifest-mismatch": [["cargo:test lekalo-core adapter_package", "manifest identity drift asserts the rule"]],
  "adapter.incompatible": [["cargo:test lekalo-core adapter_package", "incompatible adapter version asserts the rule"]],
  "adapter.install-conflict": [["cargo:test lekalo-cli main adapter install", "conflicting install asserts the rule"]],
  "adapter.install-plan-required": [["cargo:test lekalo-cli main adapter install", "unconfirmed install asserts the rule"]],
  "adapter.diagnostic-invalid": [["cargo:test lekalo-core diagnostics", "registry-backed constructor refuses hand-assembled wire"]],
  "adapter.source-changed": [["cargo:test lekalo-core adapter_package", "source drift asserts the rule"]],
  "adapter.source-unavailable": [["cargo:test lekalo-core adapter_package", "missing source asserts the rule"]],
  "adapter.signature-unverified": [["cargo:test lekalo-core adapter_package", "unsigned manifest asserts the rule"]],
  "adapter.quarantined": [["cargo:test lekalo-core adapter_package", "quarantine flow asserts the rule"]],
  "adapter.recovery-required": [["cargo:test lekalo-core adapter_package", "interrupted install asserts the rule"]],
  "adapter.security-failure": [["cargo:test lekalo-core target_protocol_security", "hostile adapter asserts the rule"]],
  "authorization.actor-invalid": [["cargo:test lekalo-core authorization", "malformed actor asserts the rule"]],
  "authorization.composition-invalid": [["cargo:test lekalo-core authorization", "invalid composition asserts the rule"]],
  "authorization.document-invalid": [["cargo:test lekalo-core authorization", "malformed document asserts the rule"]],
  "authorization.effect-unprotected": [["cargo:test lekalo-core authorization", "unprotected effect asserts the rule"]],
  "authorization.error-ref-unresolved": [["cargo:test lekalo-core authorization", "dangling error ref asserts the rule"]],
  "authorization.field-uncovered": [["cargo:test lekalo-core authorization", "uncovered field asserts the rule"]],
  "authorization.limit-exceeded": [["cargo:test lekalo-core authorization", "over-limit policy asserts the rule"]],
  "authorization.mapping-stale": [["cargo:test lekalo-core authorization", "stale mapping asserts the rule"]],
  "authorization.policy-invalid": [["cargo:test lekalo-core authorization", "invalid policy asserts the rule"]],
  "authorization.profile-invalid": [["cargo:test lekalo-core authorization", "invalid profile asserts the rule"]],
  "authorization.ref-unresolved": [["cargo:test lekalo-core authorization", "dangling reference asserts the rule"]],
  "authorization.scope-invalid": [["cargo:test lekalo-core authorization", "invalid scope asserts the rule"]],
  "bindings.ambiguous": [["cargo:test lekalo-core bindings", "ambiguous proposal asserts the rule"]],
  "bindings.plan-mismatch": [["cargo:test lekalo-core bindings", "plan mismatch asserts the rule"]],
  "bindings.proposal-unknown": [["cargo:test lekalo-core bindings", "unknown proposal asserts the rule"]],
  "cache.contract-invalid": [["node:scripts/test-cache-contracts.mjs", "cache record schema rejects drift"]],
  "classification.custody-ir": [["cargo:test lekalo-core classification", "IR custody digest drift asserts the rule"]],
  "classification.custody-model": [["cargo:test lekalo-core classification", "model custody digest drift asserts the rule"]],
  "classification.custody-project": [["cargo:test lekalo-core classification", "project custody digest drift asserts the rule"]],
  "classification.conflicting-kind": [["cargo:test lekalo-core classification", "conflicting kind asserts the rule"]],
  "classification.duplicate-subject": [["cargo:test lekalo-core classification", "duplicate subject asserts the rule"]],
  "classification.expired-declassification": [["cargo:test lekalo-core classification", "expired declassification asserts the rule"]],
  "classification.invalid-declassification": [["cargo:test lekalo-core classification", "invalid declassification asserts the rule"]],
  "classification.kind-rule-missing": [["cargo:test lekalo-core classification", "missing kind rule asserts the rule"]],
  "classification.malformed-review-ref": [["cargo:test lekalo-core classification", "malformed review ref asserts the rule"]],
  "classification.missing-approval": [["cargo:test lekalo-core classification", "missing approval asserts the rule"]],
  "classification.policy-missing": [["cargo:test lekalo-core classification", "missing policy asserts the rule"]],
  "classification.self-approved": [["cargo:test lekalo-core classification", "self-approved review asserts the rule"]],
  "classification.sink-ceiling-exceeded": [["cargo:test lekalo-core classification", "sink ceiling asserts the rule"]],
  "classification.unknown-kind": [["cargo:test lekalo-core classification", "unknown kind asserts the rule"]],
  "client.consumer-invalid": [["cargo:test lekalo-core client_sdk", "invalid consumer asserts the rule"]],
  "client.contract-invalid": [["cargo:test lekalo-core client_sdk", "contract drift asserts the rule"]],
  "client.drift": [["cargo:test lekalo-core client_sdk", "generated drift asserts the rule"]],
  "client.input-invalid": [["cargo:test lekalo-core client_sdk", "invalid input asserts the rule"]],
  "client.limit-exceeded": [["cargo:test lekalo-core client_sdk", "over-limit input asserts the rule"]],
  "client.mapping-unsupported": [["cargo:test lekalo-core client_sdk", "unsupported mapping asserts the rule"]],
  "client.retry-unsafe": [["cargo:test lekalo-core client_sdk", "unsafe retry asserts the rule"]],
  "client.symbol-unresolved": [["cargo:test lekalo-core client_sdk", "unknown symbol asserts the rule"]],
  "contracted.binding-drift": [["cargo:test lekalo-core contracted", "binding drift asserts the rule"]],
  "contracted.coverage-missing": [["cargo:test lekalo-core contracted", "missing coverage asserts the rule"]],
  "contracted.declaration-invalid": [["cargo:test lekalo-core contracted", "invalid declaration asserts the rule"]],
  "contracted.declaration-limit": [["cargo:test lekalo-core contracted", "over-limit declarations assert the rule"]],
  "contracted.registry-io": [["cargo:test lekalo-core contracted", "registry I/O failure asserts the rule"]],
  "contracted.stale-artifact": [["cargo:test lekalo-core contracted", "stale artifact asserts the rule"]],
  "contracted.unknown-module": [["cargo:test lekalo-core contracted", "unknown module asserts the rule"]],
  "contracted.unknown-symbol": [["cargo:test lekalo-core contracted", "unknown symbol asserts the rule"]],
  "core.capability-unavailable": [["cargo:test lekalo-core lib", "capability seam refusal asserts the rule"]],
  "dataflow.adapter-metadata-loss": [["cargo:test lekalo-core dataflow", "metadata loss asserts the rule"]],
  "dataflow.destination-forbidden": [["cargo:test lekalo-core dataflow", "forbidden destination asserts the rule"]],
  "dataflow.exposed-private-field": [["cargo:test lekalo-core dataflow", "exposed private field asserts the rule"]],
  "dataflow.low-confidence-sensitive": [["cargo:test lekalo-core dataflow", "low-confidence sensitive flow asserts the rule"]],
  "dataflow.missing-approval": [["cargo:test lekalo-core dataflow", "missing approval asserts the rule"]],
  "dataflow.missing-destination": [["cargo:test lekalo-core dataflow", "missing destination asserts the rule"]],
  "dataflow.observed-incomplete": [["cargo:test lekalo-core dataflow", "incomplete observed graph asserts the rule"]],
  "dataflow.unknown-flow": [["cargo:test lekalo-core dataflow", "unknown flow asserts the rule"]],
  "diff.adapter-invalid": [["cargo:test lekalo-core diff", "invalid adapter section asserts the rule"]],
  "diff.change-limit": [["cargo:test lekalo-core diff", "over-limit change set asserts the rule"]],
  "diff.export-limit": [["cargo:test lekalo-core diff", "over-limit export asserts the rule"]],
  "diff.history-invalid": [["cargo:test lekalo-core diff", "invalid rename history asserts the rule"]],
  "diff.input-invalid": [["cargo:test lekalo-core diff", "invalid input asserts the rule"]],
  "diff.profile-invalid": [["node:scripts/test-semantic-diff-contracts.mjs", "unknown profile name asserts the rule"]],
  "diff.seed-limit": [["cargo:test lekalo-core diff", "over-limit seeds assert the rule"]],
  "diff.subject-limit": [["cargo:test lekalo-core diff", "over-limit subjects assert the rule"]],
  "error.binding-invalid": [["cargo:test lekalo-core error_contract", "invalid binding asserts the rule"]],
  "error.coverage-invalid": [["cargo:test lekalo-core error_contract", "invalid coverage asserts the rule"]],
  "error.diff-invalid": [["cargo:test lekalo-core error_contract", "invalid diff asserts the rule"]],
  "error.limit-exceeded": [["cargo:test lekalo-core error_contract", "over-limit registry asserts the rule"]],
  "error.mapping-invalid": [["cargo:test lekalo-core error_contract", "invalid mapping asserts the rule"]],
  "error.mapping-missing": [["cargo:test lekalo-core error_contract", "missing mapping asserts the rule"]],
  "error.payload-invalid": [["cargo:test lekalo-core error_contract", "invalid payload asserts the rule"]],
  "error.retry-idempotency-conflict": [["cargo:test lekalo-core error_contract", "retry/idempotency conflict asserts the rule"]],
  "error.unreachable": [["cargo:test lekalo-core error_contract", "unreachable error asserts the rule"]],
  "event.contract-invalid": [["node:scripts/test-extended-effects-contracts.mjs", "event contract vectors"]],
  "expression.builtin-unsupported": [["cargo:test lekalo-core expressions", "unsupported builtin asserts the rule"]],
  "expression.eval-invalid": [["cargo:test lekalo-core expressions", "evaluation failure vectors assert the rule"]],
  "expression.export-limit": [["cargo:test lekalo-core expressions", "over-limit export asserts the rule"]],
  "extended.export-limit": [["cargo:test lekalo-core extended_effects", "over-limit export asserts the rule"]],
  "graph.cycle-forbidden": [["cargo:test lekalo-core graph", "requires-cycle asserts the rule"]],
  "graph.export-limit": [["cargo:test lekalo-core graph", "over-limit export asserts the rule"]],
  "graph.path-not-found": [["node:CLI graph path", "disconnected pair asserts the rule"]],
  "graph.provenance-incomplete": [["cargo:test lekalo-core graph", "incomplete provenance asserts the rule"]],
  "graph.unknown-node": [["node:CLI graph show/impact/context", "unknown symbol asserts the rule"]],
  "graph.unknown-relation": [["cargo:test lekalo-core graph", "unknown relation asserts the rule"]],
  "impact.changed-input-incomplete": [["cargo:test lekalo-cli impact", "incomplete changed input asserts the rule"]],
  "impact.changed-input-invalid": [["cargo:test lekalo-cli impact", "invalid changed input asserts the rule"]],
  "impact.effect-stale": [["cargo:test lekalo-cli impact", "stale effect evidence asserts the rule"]],
  "impact.evidence-unknown": [["cargo:test lekalo-cli impact", "unknown evidence asserts the rule"]],
  "impact.gate-blocked": [["cargo:test lekalo-cli impact", "blocked gate asserts the rule"]],
  "impact.output-limit": [["cargo:test lekalo-cli impact", "over-limit output asserts the rule"]],
  "impact.public-impact-incomplete": [["cargo:test lekalo-cli impact", "incomplete public impact asserts the rule"]],
  "impact.selector-invalid": [["node:CLI impact", "malformed selector asserts the rule"]],
  "impact.symbol-unknown": [["node:CLI impact", "unknown symbol asserts the rule"]],
  "impact.traversal-limit": [["cargo:test lekalo-cli impact", "over-depth traversal asserts the rule"]],
  "inspect.output-limit": [["cargo:test lekalo-cli inspect", "over-limit output asserts the rule"]],
  "inspect.short-name-ambiguous": [["cargo:test lekalo-cli inspect", "ambiguous short name asserts the rule"]],
  "inspect.short-name-unknown": [["cargo:test lekalo-cli inspect", "unknown short name asserts the rule"]],
  "inspect.symbol-unknown": [["node:CLI inspect", "unknown symbol asserts the rule"]],
  "ir.duplicate-member": [["node:scripts/test-model-contracts.mjs", "duplicate member vectors"]],
  "init.adopt-ambiguous-root": [["cargo:test lekalo-core init_adopt_boundary", "ambiguous adopt root asserts the rule"]],
  "init.adopt-conflict": [["cargo:test lekalo-core init_adopt_boundary", "adopt conflict asserts the rule"]],
  "init.adopt-id-required": [["cargo:test lekalo-core init_adopt_boundary", "missing adopt id asserts the rule"]],
  "init.adopt-recovery-required": [["cargo:test lekalo-core init_adopt_boundary", "interrupted adopt asserts the rule"]],
  "init.adopt-write-failed": [["cargo:test lekalo-core init_adopt_boundary", "refused adopt write asserts the rule"]],
  "init.bootstrap-conflict": [["cargo:test lekalo-cli bootstrap", "bootstrap conflict asserts the rule"]],
  "init.bootstrap-id-required": [["cargo:test lekalo-cli bootstrap", "missing bootstrap id asserts the rule"]],
  "init.bootstrap-recovery-required": [["cargo:test lekalo-cli bootstrap", "interrupted bootstrap asserts the rule"]],
  "init.bootstrap-write-failed": [["cargo:test lekalo-cli bootstrap", "refused bootstrap write asserts the rule"]],
  "invariant.export-limit": [["cargo:test lekalo-core invariant_transition", "over-limit export asserts the rule"]],
  "job.contract-invalid": [["node:scripts/test-extended-effects-contracts.mjs", "job contract vectors"]],
  "lock.catalog-unavailable": [["cargo:test lekalo-core lockfile", "unavailable catalog asserts the rule"]],
  "lock.commit-failed": [["cargo:test lekalo-core lockfile", "failed commit asserts the rule"]],
  "lock.component-unavailable": [["cargo:test lekalo-core lockfile", "unavailable component asserts the rule"]],
  "lock.component-version-unsupported": [["cargo:test lekalo-core lockfile", "unsupported component version asserts the rule"]],
  "lock.digest-mismatch": [["cargo:test lekalo-cli lock", "tampered lock asserts the rule"]],
  "lock.missing": [["cargo:test lekalo-cli lock --check", "missing lock asserts the rule"]],
  "lock.path-denied": [["cargo:test lekalo-core lockfile", "denied path asserts the rule"]],
  "lock.platform-unavailable": [["cargo:test lekalo-core lockfile", "unavailable platform asserts the rule"]],
  "lock.preview-required": [["cargo:test lekalo-cli lock", "missing preview asserts the rule"]],
  "lock.private-data-forbidden": [["cargo:test lekalo-core lockfile", "private data asserts the rule"]],
  "lock.profile-unversioned": [["cargo:test lekalo-core lockfile", "unversioned profile asserts the rule"]],
  "lock.recovery-required": [["cargo:test lekalo-core lockfile", "interrupted lock asserts the rule"]],
  "lock.reference-invalid": [["cargo:test lekalo-core lockfile", "invalid reference asserts the rule"]],
  "lock.resolution-ambiguous": [["cargo:test lekalo-core lockfile", "ambiguous resolution asserts the rule"]],
  "lock.resolution-provider-unavailable": [["cargo:test lekalo-core lockfile", "unavailable provider asserts the rule"]],
  "lock.source-changed": [["cargo:test lekalo-core lockfile", "changed source asserts the rule"]],
  "lock.stale": [["cargo:test lekalo-cli lock", "stale lock asserts the rule"]],
  "lock.update-in-progress": [["cargo:test lekalo-core lockfile", "concurrent update asserts the rule"]],
  "native-gate.approval-missing": [["cargo:test lekalo-core native_gate", "unapproved script asserts the rule"]],
  "native-gate.capability-missing": [["cargo:test lekalo-core native_gate", "missing capability asserts the rule"]],
  "native-gate.env-denied": [["cargo:test lekalo-core native_gate", "denied env asserts the rule"]],
  "native-gate.manager-unsupported": [["cargo:test lekalo-core native_gate", "unsupported manager asserts the rule"]],
  "native-gate.output-withheld": [["cargo:test lekalo-core native_gate", "withheld output asserts the rule"]],
  "native-gate.plan-invalid": [["node:scripts/test-native-gate-contracts.mjs", "invalid plan vectors"]],
  "native-gate.plan-stale": [["cargo:test lekalo-core native_gate", "stale plan asserts the rule"]],
  "native-gate.policy-invalid": [["node:scripts/test-native-gate-contracts.mjs", "invalid policy vectors"]],
  "native-gate.run-failed": [["cargo:test lekalo-core native_gate", "failing gate asserts the rule"]],
  "native-gate.run-infrastructure": [["cargo:test lekalo-core native_gate", "infrastructure failure asserts the rule"]],
  "native-gate.script-unconfirmed": [["cargo:test lekalo-core native_gate", "unconfirmed script asserts the rule"]],
  "native-gate.security-violation": [["cargo:test lekalo-core native_gate", "security violation asserts the rule"]],
  "native-gate.trust-insufficient": [["cargo:test lekalo-core native_gate", "insufficient trust asserts the rule"]],
  "native-gate.workspace-invalid": [["cargo:test lekalo-core native_gate", "invalid workspace asserts the rule"]],
  "nfr.capability-unsatisfied": [["cargo:test lekalo-core nfr", "unsatisfied capability asserts the rule"]],
  "nfr.constraint-invalid": [["cargo:test lekalo-core nfr", "invalid constraint asserts the rule"]],
  "nfr.environment-incompatible": [["cargo:test lekalo-core nfr", "incompatible environment asserts the rule"]],
  "nfr.evidence-stale": [["cargo:test lekalo-core nfr", "stale evidence asserts the rule"]],
  "nfr.evidence-unavailable": [["cargo:test lekalo-core nfr", "unavailable evidence asserts the rule"]],
  "nfr.export-limit": [["cargo:test lekalo-core nfr", "over-limit export asserts the rule"]],
  "nfr.gate-denied": [["cargo:test lekalo-core nfr", "denied gate asserts the rule"]],
  "nfr.model-ref-mismatch": [["cargo:test lekalo-core nfr", "model ref mismatch asserts the rule"]],
  "nfr.projection-empty": [["cargo:test lekalo-core nfr", "empty projection asserts the rule"]],
  "nfr.scope-unknown": [["cargo:test lekalo-core nfr", "unknown scope asserts the rule"]],
  "observed.confirm-refused": [["cargo:test lekalo-core observed", "refused confirm asserts the rule"]],
  "observed.incomplete-graph": [["cargo:test lekalo-core observed", "incomplete graph asserts the rule"]],
  "observed.index-io": [["cargo:test lekalo-core observed", "index I/O failure asserts the rule"]],
  "observed.promotion-plan-mismatch": [["cargo:test lekalo-core observed", "promotion plan mismatch asserts the rule"]],
  "observed.promotion-refused": [["cargo:test lekalo-core observed", "refused promotion asserts the rule"]],
  "observed.scan-invalid": [["cargo:test lekalo-core observed", "invalid scan asserts the rule"]],
  "observed.scan-limit": [["cargo:test lekalo-core observed", "over-limit scan asserts the rule"]],
  "observed.stale-binding": [["cargo:test lekalo-core observed", "stale binding asserts the rule"]],
  "observed.unknown-module": [["cargo:test lekalo-core observed", "unknown module asserts the rule"]],
  "observed.unknown-symbol": [["cargo:test lekalo-core observed", "unknown symbol asserts the rule"]],
  "openapi.binding-unresolved": [["cargo:test lekalo-core openapi_render", "unresolved binding asserts the rule"]],
  "openapi.drift": [["cargo:test lekalo-core openapi_render", "declared drift asserts the rule"]],
  "openapi.export-limit": [["cargo:test lekalo-core openapi_render", "over-limit export asserts the rule"]],
  "openapi.input-invalid": [["cargo:test lekalo-core openapi_render", "invalid input asserts the rule"]],
  "openapi.merge-conflict": [["cargo:test lekalo-core openapi_render", "merge conflict asserts the rule"]],
  "openapi.projection-partial": [["cargo:test lekalo-core openapi_render", "partial projection asserts the rule"]],
  "openapi.schema-invalid": [["node:scripts/test-openapi-contracts.mjs", "schema-invalid vectors"]],
  "php-operations.join-invalid": [["node:scripts/test-php-operations-contracts.mjs", "invalid join vectors"]],
  "php-routes.join-invalid": [["node:scripts/test-php-routes-contracts.mjs", "invalid join vectors"]],
  "query.contract-invalid": [["cargo:test lekalo-cli query_model", "contract drift asserts the rule"]],
  "query.export-limit": [["cargo:test lekalo-cli query_model", "over-limit export asserts the rule"]],
  "query.filter-invalid": [["cargo:test lekalo-cli query_model", "invalid filter asserts the rule"]],
  "query.pagination-invalid": [["cargo:test lekalo-cli query_model", "invalid pagination asserts the rule"]],
  "query.reference-invalid": [["cargo:test lekalo-cli query_model", "invalid reference asserts the rule"]],
  "query.sort-invalid": [["cargo:test lekalo-cli query_model", "invalid sort asserts the rule"]],
  "query.source-invalid": [["cargo:test lekalo-cli query_model", "invalid source asserts the rule"]],
  "query.visibility-boundary": [["cargo:test lekalo-cli query_model", "visibility boundary asserts the rule"]],
  "requirements.export-limit": [["cargo:test lekalo-core requirements", "over-limit export asserts the rule"]],
  "requirements.model-ref-mismatch": [["cargo:test lekalo-core requirements", "model ref mismatch asserts the rule"]],
  "requirements.projection-empty": [["cargo:test lekalo-core requirements", "empty projection asserts the rule"]],
  "requirements.provider-unavailable": [["cargo:test lekalo-core requirements", "unavailable provider asserts the rule"]],
  "requirements.ref-unknown": [["cargo:test lekalo-core requirements", "unknown reference asserts the rule"]],
  "requirements.requirement-conflict": [["cargo:test lekalo-core requirements", "conflicting requirements assert the rule"]],
  "requirements.requirement-missing": [["cargo:test lekalo-core requirements", "missing requirement asserts the rule"]],
  "requirements.requirement-stale": [["cargo:test lekalo-core requirements", "stale requirement asserts the rule"]],
  "scenario.assertion-failed": [["node:scripts/test-node-scenario-tests.mjs", "failing assertion rows record the outcome"]],
  "scenario.binding-ambiguous": [["cargo:test lekalo-core scenario", "ambiguous binding asserts the rule"]],
  "scenario.binding-mismatch": [["cargo:test lekalo-core scenario", "binding mismatch asserts the rule"]],
  "scenario.binding-missing": [["cargo:test lekalo-core scenario", "missing binding asserts the rule"]],
  "scenario.drift": [["cargo:test lekalo-core scenario", "drifted scenario asserts the rule"]],
  "scenario.infrastructure": [["node:scripts/test-node-scenario-tests.mjs", "infrastructure rows record the outcome"]],
  "scenario.ir-ref-mismatch": [["cargo:test lekalo-core scenario", "IR ref mismatch asserts the rule"]],
  "scenario.operation-unresolved": [["cargo:test lekalo-core scenario", "unresolved operation asserts the rule"]],
  "scenario.port-missing": [["cargo:test lekalo-core scenario", "missing port asserts the rule"]],
  "scenario.port-shape": [["cargo:test lekalo-core scenario", "malformed port asserts the rule"]],
  "scenario.run-record-invalid": [["cargo:test lekalo-core scenario", "invalid run record asserts the rule"]],
  "scenario.runner-unknown": [["cargo:test lekalo-core scenario", "unknown runner asserts the rule"]],
  "scenario.stale-evidence": [["cargo:test lekalo-core scenario", "stale evidence asserts the rule"]],
  "scenario.unsupported-capability": [["node:scripts/test-node-scenario-tests.mjs", "the concurrency case records unsupported rows only"]],
  "storage-engine.capability-missing": [["cargo:test lekalo-core storage_engine", "missing capability asserts the rule"]],
  "storage-engine.conformance-failed": [["cargo:test lekalo-core storage_engine", "failed conformance asserts the rule"]],
  "storage-engine.drift-invalid": [["cargo:test lekalo-core storage_engine", "invalid drift asserts the rule"]],
  "storage-engine.export-limit": [["cargo:test lekalo-core storage_engine", "over-limit export asserts the rule"]],
  "storage-engine.extension-unsupported": [["cargo:test lekalo-core storage_engine", "unsupported extension asserts the rule"]],
  "storage-engine.input-invalid": [["node:scripts/test-storage-engine-contracts.mjs", "invalid input vectors"]],
  "storage-engine.introspection-invalid": [["cargo:test lekalo-core storage_engine", "invalid introspection asserts the rule"]],
  "storage-engine.lifecycle-invalid": [["cargo:test lekalo-core storage_engine", "invalid lifecycle asserts the rule"]],
  "storage-engine.mapping-invalid": [["cargo:test lekalo-core storage_engine", "invalid mapping asserts the rule"]],
  "storage-engine.migration-gated": [["cargo:test lekalo-core storage_engine", "gated migration asserts the rule"]],
  "storage-engine.migration-invalid": [["cargo:test lekalo-core storage_engine", "invalid migration asserts the rule"]],
  "storage-engine.profile-invalid": [["node:scripts/test-storage-engine-profile-contracts.mjs", "invalid profile vectors"]],
  "storage-engine.render-unsupported": [["cargo:test lekalo-core storage_engine", "unsupported render asserts the rule"]],
  "storage-engine.version-unsupported": [["cargo:test lekalo-core storage_engine", "unsupported version asserts the rule"]],
  "storage.diff-invalid": [["cargo:test lekalo-core storage_projection", "invalid diff asserts the rule"]],
  "storage.export-limit": [["cargo:test lekalo-core storage_projection", "over-limit export asserts the rule"]],
  "storage.introspection-diff-invalid": [["cargo:test lekalo-core storage_introspection", "invalid introspection diff asserts the rule"]],
  "storage.profile-diff-invalid": [["cargo:test lekalo-core storage_engine_profile", "invalid profile diff asserts the rule"]],
  "storage.profile-invalid": [["cargo:test lekalo-core storage_engine_profile", "invalid profile asserts the rule"]],
  "storage.profile-limit": [["cargo:test lekalo-core storage_engine_profile", "over-limit profile asserts the rule"]],
  "structure.directory-required": [["cargo:test lekalo-core project_fs_directory", "required directory asserts the rule"]],
  "structure.directory-unreadable": [["cargo:test lekalo-core project_fs_directory", "unreadable directory asserts the rule"]],
  "structure.lock-not-file": [["cargo:test lekalo-core project_fs_directory", "lock-not-file asserts the rule"]],
  "structure.path-absolute": [["cargo:test lekalo-core project_fs_directory", "absolute path asserts the rule"]],
  "structure.path-case": [["cargo:test lekalo-core project_fs_directory", "case-collision path asserts the rule"]],
  "structure.path-device": [["cargo:test lekalo-core project_fs_directory", "device path asserts the rule"]],
  "structure.path-empty": [["cargo:test lekalo-core project_fs_directory", "empty path asserts the rule"]],
  "structure.path-escape": [["cargo:test lekalo-core project_fs_directory", "escape path asserts the rule"]],
  "structure.path-link": [["cargo:test lekalo-core project_fs_directory", "symlink path asserts the rule"]],
  "structure.path-not-nfc": [["cargo:test lekalo-core project_fs_directory", "non-NFC path asserts the rule"]],
  "structure.path-segment": [["cargo:test lekalo-core project_fs_directory", "invalid segment asserts the rule"]],
  "structure.path-short-name": [["cargo:test lekalo-core project_fs_directory", "8.3 alias path asserts the rule"]],
  "structure.path-special": [["cargo:test lekalo-core project_fs_directory", "special-name path asserts the rule"]],
  "structure.path-traversal": [["cargo:test lekalo-core project_fs_directory", "traversal path asserts the rule"]],
  "structure.project-not-directory": [["cargo:test lekalo-core project_fs_directory", "file-as-root asserts the rule"]],
  "structure.root-not-found": [["cargo:test lekalo-core project_fs_directory", "missing root asserts the rule"]],
  "structure.root-unreadable": [["cargo:test lekalo-core project_fs_directory", "unreadable root asserts the rule"]],
  "structure.scan-limit": [["cargo:test lekalo-core project_fs_directory", "over-limit scan asserts the rule"]],
  "structure.selection-absolute": [["cargo:test lekalo-core project_fs_directory", "absolute selection asserts the rule"]],
  "structure.selection-alias": [["cargo:test lekalo-core project_fs_directory", "alias selection asserts the rule"]],
  "structure.selection-device": [["cargo:test lekalo-core project_fs_directory", "device selection asserts the rule"]],
  "structure.selection-empty": [["cargo:test lekalo-core project_fs_directory", "empty selection asserts the rule"]],
  "structure.selection-escape": [["cargo:test lekalo-core project_fs_directory", "escape selection asserts the rule"]],
  "structure.selection-segment": [["cargo:test lekalo-core project_fs_directory", "invalid selection segment asserts the rule"]],
  "structure.selection-short-name": [["cargo:test lekalo-core project_fs_directory", "8.3 alias selection asserts the rule"]],
  "structure.selection-traversal": [["cargo:test lekalo-core project_fs_directory", "traversal selection asserts the rule"]],
  "structure.selection-unreadable": [["cargo:test lekalo-core project_fs_directory", "unreadable selection asserts the rule"]],
  "target-profile.capability-unsatisfied": [["node:scripts/test-target-profile-contracts.mjs", "unsatisfied capability vectors"]],
  "target-profile.combination-incompatible": [["node:scripts/test-target-profile-contracts.mjs", "incompatible combination vectors"]],
  "target-profile.component-unknown": [["node:scripts/test-target-profile-contracts.mjs", "unknown component vectors"]],
  "target-profile.document-invalid": [["node:scripts/test-target-profile-contracts.mjs", "invalid document vectors"]],
  "target-profile.inheritance-weakening": [["node:scripts/test-target-profile-contracts.mjs", "weakening inheritance vectors"]],
  "target-profile.reference-invalid": [["node:scripts/test-target-profile-contracts.mjs", "invalid reference vectors"]],
  "target.cancelled": [["cargo:test lekalo-core target_protocol", "cancelled exchange asserts the rule"]],
  "target.capability-unsupported": [["cargo:test lekalo-core target_protocol", "unsupported capability asserts the rule"]],
  "target.crash": [["cargo:test lekalo-core target_protocol", "crashing adapter asserts the rule"]],
  "target.dry-run-mutation": [["cargo:test lekalo-core target_protocol", "mutating dry-run asserts the rule"]],
  "target.handshake-required": [["cargo:test lekalo-core target_protocol", "skipped handshake asserts the rule"]],
  "target.ir-unsupported": [["cargo:test lekalo-core target_protocol", "unsupported IR asserts the rule"]],
  "target.operation-failed": [["cargo:test lekalo-core target_protocol", "failed operation asserts the rule"]],
  "target.output-limit": [["cargo:test lekalo-core target_protocol", "over-limit output asserts the rule"]],
  "target.timeout": [["cargo:test lekalo-core target_protocol", "timed-out exchange asserts the rule"]],
  "target.transport-failed": [["cargo:test lekalo-core target_protocol", "failed transport asserts the rule"]],
  "transaction.external-atomic": [["cargo:test lekalo-core transaction_concurrency", "external atomicity asserts the rule"]],
  "transaction.export-limit": [["cargo:test lekalo-core transaction_concurrency", "over-limit export asserts the rule"]],
  "transport.capability-unsatisfied": [["node:scripts/test-transport-contracts.mjs", "unsatisfied capability vectors"]],
  "transport.export-limit": [["cargo:test lekalo-core transport_http", "over-limit export asserts the rule"]],
  "transport.param-invalid": [["cargo:test lekalo-core transport_http", "invalid param asserts the rule"]],
  "transport.pagination-invalid": [["cargo:test lekalo-core transport_http", "invalid pagination asserts the rule"]],
  "transport.security-invalid": [["cargo:test lekalo-core transport_http", "invalid security asserts the rule"]],
  "validate.profile-invalid": [["node:scripts/test-validation-contracts.mjs", "profile identity checks"]],
  "validate.module-unresolved": [["node:CLI validate --module", "unknown module scope asserts the rule"]],
  "validate.span-unavailable": [["cargo:test lekalo-core validator", "unavailable span asserts the rule"]],
  "versioning.adapter-incompatible": [["cargo:test lekalo-core versioning", "incompatible adapter asserts the rule"]],
  "versioning.adapter-manifest-invalid": [["cargo:test lekalo-core versioning", "invalid manifest asserts the rule"]],
  "versioning.backup-failed": [["cargo:test lekalo-core versioning", "failed backup asserts the rule"]],
  "versioning.commit-failed": [["cargo:test lekalo-core versioning", "failed commit asserts the rule"]],
  "versioning.extension-incompatible": [["cargo:test lekalo-core versioning", "incompatible extension asserts the rule"]],
  "versioning.migration-in-progress": [["cargo:test lekalo-core versioning", "concurrent migration asserts the rule"]],
  "versioning.migration-precondition": [["cargo:test lekalo-core versioning", "unmet precondition asserts the rule"]],
  "versioning.no-migration-path": [["cargo:test lekalo-core versioning", "missing path asserts the rule"]],
  "versioning.not-file-migratable": [["cargo:test lekalo-core versioning", "unmigratable file asserts the rule"]],
  "versioning.recovery-required": [["cargo:test lekalo-core versioning", "interrupted migration asserts the rule"]],
  "versioning.registry-invalid": [["cargo:test lekalo-core versioning", "invalid registry asserts the rule"]],
  "versioning.rollback-conflict": [["cargo:test lekalo-core versioning", "rollback conflict asserts the rule"]],
  "versioning.rollback-failed": [["cargo:test lekalo-core versioning", "failed rollback asserts the rule"]],
  "versioning.source-changed": [["cargo:test lekalo-core versioning", "changed source asserts the rule"]],
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
    row.testWitness = testWitness[id].map(([gate, note]) => ({ gate, note }));
  } else if (interactionOnly[id]) {
    row.evidence = "interaction-only";
    row.note = interactionOnly[id];
  } else {
    // Unmapped rules fall back to their subsystem's contract gate where
    // one exists; otherwise they are an explicit acceptance gap.
    row.evidence = "test-witness";
    row.testWitness = [{
      gate: "UNMAPPED — acceptance gap, see docs/m7/issue-90-implementation.md",
      note: "no dedicated evidence recorded yet; adding evidence is tracked backlog",
    }];
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
