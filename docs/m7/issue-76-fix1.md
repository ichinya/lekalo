# Issue #76 fix round 1

Fixes for the independently reproduced R1/R2 in [Codex review round 1](issue-76-review1-codex.md), on `ichinya/m7-issue-76` from review HEAD `be0c78a9`. This supplements the [implementation record](issue-76-implementation.md). The existing evidence/protocol schemas, their 32-location document bound and diagnostic registry entries remain unchanged.

| Finding | Disposition and searchable implementation anchors | Regression evidence |
| --- | --- | --- |
| R1: collectors exceed the published document bound; installed collection and supplied replay disagree | Fixed. Node `adapters/node-typescript/src/ai-lint.mjs::collectAiLint` and PHP `adapters/php-laravel/src/ai-lint.php::lint_collect` retain whole records in canonical order within the union of their record and activation locations. Remaining location capacity may retain unreferenced spans. An exhausted document budget returns at most 32 locations, partial supported-rule coverage and the explicit `document-location-limit` limitation. Core `ai_lint/input.rs::admit_evidence` now applies the same `validate_tree` wire bounds as supplied-file decoding. | `adapterCases` runs the shipped bundles and installed TargetClient on exactly 32/33 detector-location cases for both adapters. Strict Ajv validates each request, response and evidence document. Collection/replay agree on retained finding identities, confidence/severity, coverage and registered `ai-lint.coverage-incomplete`; oversized file replay refuses. The core test `protocol_evidence_and_file_replay_share_document_location_bounds` verifies that typed protocol evidence bypassing file decoding also refuses with the same `wire-bound` diagnostic. |
| R2: unavailable action falsely proves an undeclared effect and can deny CI | Fixed. `ai_lint/mod.rs::analyze` compares declarations only when operation, resource and action are known. `declared_effect` takes a known action rather than an optional one. Unavailable comparison inputs yield partial coverage for the affected observer/direct rule with `effect-comparison-input-unavailable`, rather than a negative-match finding. | `evidenceCases` runs observer and direct records for `planner.edit_task_cmd`: known update produces no missing-effect finding and keeps complete corresponding coverage; unknown, unsupported and withheld actions produce no missing-effect finding and yield partial coverage plus the registered uncertainty diagnostic. Advisory and explicit CI check stay valid. A known mismatching create action still produces the appropriate warning and denies CI. The existing actual-adapter declared-update negatives remain exercised. |

The existing `ai-lint.coverage-incomplete` diagnostic provides explicit uncertainty in the CLI envelope. No new diagnostic is needed, so no registry entry is added or edited. There is no shortened observer activation chain, missing span reference, complete-coverage claim on an exhausted slice, or conversion of unavailable action into a demonstrated missing declaration. The collectors remain static and read-only.

## Location boundary controls

| Collector input | Detector locations before projection | Published locations | Retained reflection records | Boundary disposition |
| --- | ---: | ---: | ---: | --- |
| Node: 32 functions with one computed call each | 32 | 32 | 32 | Existing partial static coverage; no budget-exhaustion limitation. |
| Node: 33 functions with one computed call each | 33 | 32 | 32 | Partial coverage with `document-location-limit`; whole records retained. |
| PHP: 16 methods with one computed call each | 32 (method + call spans) | 32 | 16 | Existing partial static coverage; no budget-exhaustion limitation. |
| PHP: those 16 methods plus an empty seventeenth method | 33 | 32 | 16 | Partial coverage with `document-location-limit`; whole records retained. |

Every row is tested through both installed `--scan-target` and `--evidence` replay, in disposable copies outside the tracked fixture tree. Source bytes are restored after each target's boundary controls. Shipped manifests are regenerated against deterministic bundle bytes; only the corresponding artifact pins in the adapter evidence snapshots and the existing Node manifest exemplar assertion are refreshed.

## Validation

Local environment: Windows, Rust 1.98.0 / MSRV 1.80.0, Node 24.13.0, PHP 8.5.0 and exact Ajv 8.17.1 provisioned outside the checkout. No extra dependencies were added.

| Command / gate | Result |
| --- | --- |
| `cargo fmt --all -- --check`, `cargo build --workspace --locked` | PASS. |
| Stable and Rust 1.80.0 `cargo clippy --workspace --all-targets --locked -- -D warnings` | Both PASS; MSRV uses its own target directory inside this worktree. |
| All five `test-ai-lint-*-contracts.mjs`, without `--update` | All PASS with fresh production behavior, including R1/R2 controls. |
| `cargo test --workspace --locked` | PASS, exit 0: 1,919 passed, zero failed, two existing benchmark tests ignored, across 93 test-result groups including doc tests. The new typed-admission/file-replay bound test passes. |
| `test-node-typescript-kernel.mjs`, `test-node-typescript-scanner.mjs` | PASS across all four kernel and three scanner test files, including deterministic bundle reconstruction. |
| `test-php-laravel-adapter.mjs` | PASS: 150 protocol, 40 process and 61 analyzer checks, deterministic packaging and strict core-owned production conformance. |
| `test-adapter-manifest-{contracts,golden}.mjs` | Both PASS for current Node/PHP package pins. |
| `test-diagnostic-contracts.mjs`, `test-fixture-provenance.mjs`, `test-golden-{hygiene,diagnostic-coverage}.mjs` | PASS: 476 registry entries, 74 synthetic families, 260 fixture files; all registry rules remain covered. |
| `gen-ai-lint-{contracts,protocol}.mjs --check`, `gen-suite-{coverage,diagnostic-pairs}.mjs --check` | PASS; no generated contract or coverage changes required. |
| `check-contract-versions.mjs --base be0c78a9`, `git diff --check` | PASS; 109 contract artifacts, no contract or registry delta from review HEAD. |

Adapter evidence snapshots were regenerated through the live gate; a structural comparison against review HEAD confirms that only `producer.artifactDigest` changed in each. No baseline metrics, schema acceptance or assertion was relaxed.

No push, hosted CI execution or mutation of another worktree is part of this fix round. Existing review documents and approved research remain unchanged.
