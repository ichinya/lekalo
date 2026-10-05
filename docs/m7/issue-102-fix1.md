# Issue #102: fix1, implementation round 2

Status: **Both round-one findings corrected and locally verified**, 2026-10-05,
branch `ichinya/m7-issue-102`, product 0.6.4. Fix authority:
[Codex review round 1](issue-102-review1-codex.md), findings F1/F2, and the
explicit implementation-round-2 instruction. Starting HEAD: `dabb7276`;
worktree/index were clean. F1 commit: `78adfc6c`,
`fix(metrics): authorize same-origin repository storage`. F2 and this report
are committed together as `fix(metrics): enforce frozen run-history schemas`.

No frozen contract, policy, evaluator, golden, registry, CI workflow, command
owner or other worktree changes are part of these fixes. Registry remains
508 entries, preserving its original 500-entry predecessor. No push.
The original research, implementation and reviews remain historical evidence.

## F1: coherent repository storage and all six destinations

`metrics_export/privacy.rs:17` now derives the consumer identity once.
For `DestinationSpec::RepositoryStore`, the destination identity equals
that consumer identity; other destinations retain their previous endpoint
construction. The original #119 resolver still supplies the operation,
boundary, audience and relations. This corrects the same-origin metadata
without changing `privacy/evaluate.rs` or its contradiction refusal.

The live gate at `scripts/test-metrics-export-contracts.mjs:128` issues fresh,
purpose-bound synthetic evidence against each destination's exact template,
then checks ready state and operation/boundary/audience. Workspace local-use
omits export-transfer consent as the frozen policy requires. Transfer
endpoints remain distinct; repository storage endpoints are equal. A separate
real `privacy evaluate` invocation deliberately changes one repository-store
endpoint, refreshes binding, and still receives exit 3 with
`repository.same-origin-contradiction`. Unknown specs and authorization bound
to another destination continue to refuse.

| Destination | Operation / boundary | Authorized preview |
| --- | --- | --- |
| workspace | local-use / same-local-workspace | ready |
| repository-store | repository-store / same-repository | ready |
| transfer-tenant | transfer / same-tenant | ready |
| transfer-external | transfer / cross-repository | ready |
| transfer-cross-tenant | transfer / cross-tenant | ready |
| publish | publish / public | ready |

The new destination assertion failed against the old binary with the original
same-origin contradiction, then passed after the rebuild. An independent
temporary harness also confirms an actual repository-store package is
written and current authorized status is `valid-at-export`, beyond preview.
The six-destination regression runs in every selected family lane, including
the existing post-build CI loop, without changing CI wiring.

## F2: complete frozen source admission

New `crates/lekalo-core/src/metrics_export/source_schema.rs` compiles the exact
embedded `run-record.schema.v0.4.0.json` and
`run-assertions.schema.v0.4.0.json` as Draft 2020-12 validators. Both are cached
per process. Compilation or validation failure maps to the existing generic
source refusal; validator diagnostics/source values do not enter the receipt.

The dependency is exact `jsonschema = 0.29.1`, with default features disabled.
Its HTTP/file/async resolution features are not enabled, and a custom
retriever refuses every external resource. Only the embedded schemas and
their internal references resolve. Schema validation is additional to
`Store::get` byte rehashing, frozen references, scoped selection, identity,
assertion linkage/count and existing typed/semantic checks; none are removed.
`metrics_export/mod.rs:180` and `:210` enforce full schema admission before
aggregation and privacy subject construction.

Status also validates live records and assertions (`mod.rs:505`). A package
produced by the old permissive admission path cannot remain currently
eligible merely because its original body digests still match. The retained
round-one reproduction package at
`C:/Users/User/AppData/Local/Temp/lekalo-102-findings-repro-YoT8nG` reported
`valid-at-export` with the F1-only binary and `invalidated` after this rebuild,
using the same source bytes, custody and supplied authorization.

The manifest lane and combined live gate now test 14 rehashed record mutations
and six rehashed assertion mutations, including the reported root prompt,
invalid timestamp/recordedAt, nested operation/measurement/test/diagnostic/
privacy/repeat fields, missing assertion properties, bad semantic/evidence
identities and overlong assertion ids. Each mutation first fails the unchanged
frozen schema under exact Ajv 8.17.1. Assertion vectors rebind the assertion
digest, parent record assertion reference and parent body digest; the rebound
parent remains schema-valid. This reproduces the original custody bypass
without relying on a wrong hash or invalid parent as the refusal cause.

For each vector, both dry-run and confirmation with a previously authorized
preview refuse with exit 3 / `metrics-export.source-invalidated`. Refusals
contain no private canary, and dependent counts remain unchanged. Restoring
valid source bodies makes the authorized preview ready again. The prior
wrong-policy vector now mutates a clone, preserving the valid baseline used
to construct these independent assertion-parent controls.

Three new Rust tests cover embedded-schema compilation failure, prohibited
external references, complete record-field constraints, assertion root/nested
closure, required members, token/semantic grammar, length/array bounds and
uniqueness. Valid committed record/assertion fixtures remain accepted.

## Verification record

All binary checks below used the rebuilt local Windows CLI, synthetic
temporary projects and exact Ajv 8.17.1. No golden-authoring mode was used.

```powershell
cargo build --locked -p lekalo-cli
$env:NODE_PATH='C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
node scripts/test-metrics-export-contracts.mjs
cargo test --locked -p lekalo-core --lib metrics_export -- --test-threads=1
cargo clippy --locked -p lekalo-core -p lekalo-cli --all-targets -- -D warnings
cargo fmt --all --check
```

| Check | Observed result |
| --- | --- |
| Locked CLI build | PASS after both fixes. |
| Combined metrics contract/live gate | PASS, all five families, unchanged goldens and 500/508 registry check; includes all six ready destinations and digest-valid invalid-source refusal corpus. |
| Touched metrics Rust tests | PASS, 7 tests: original 4 security/lifecycle tests plus 3 frozen-schema validation tests. |
| Clippy for core/CLI all targets; fmt | PASS. |
| Independent fix harness | PASS: six ready destinations, malformed digest-valid timestamp/assertions refused, repository-store confirmation written and status valid. |
| Historical round-one invalid assertion package | Corrected binary returns invalidated with current supplied authorization; old binary returned valid-at-export. |
| `test-privacy-evaluator-parity.mjs`; `test-privacy-runtime-cli.mjs` | PASS, 120 vectors and 13 runtime cases. |
| `test-run-history-contracts.mjs`; `test-run-history-cli.mjs` | PASS, 4 schemas / 11 valid / 16 invalid fixtures, plus existing offline real-binary recorder flow. |
| `check-contract-versions.mjs --base dabb7276` | PASS, product 0.6.4, 122 contract artifacts. |
| Frozen-surface diff and lock audit | No contract/golden/registry/frozen-privacy change. All 75 predecessor package versions/checksums preserved; 76 new locked packages for schema validation. |
| Git diff/index scope | Only the fix files and this report committed; no push. |

The independent harness is retained outside the checkout at
`C:/Users/User/AppData/Local/Temp/lekalo-102-fix1-independent-20261005.mjs`;
its synthetic project is `C:/Users/User/AppData/Local/Temp/lekalo-102-findings-repro-VabLR7`.
It reuses the round-one standalone fixture setup and original #119 binding
helper, without calling the metrics gate. It can be rerun with Node 24, the
same NODE_PATH and this worktree path as its first argument.

Dependency resolution used Cargo's Rust-1.80-compatible fallback; the package
declared Rust baseline stays 1.80.0. Builds/tests here used Rust 1.98.0; this is
not a claim of a separate native Rust 1.80 or Linux/macOS qualification run.
Full workspace tests and hosted CI were not rerun. #100 producer/evaluation
execution, authentic issuer/revocation services, remote publication and AIFHub
acceptance remain the external seams documented in the original delivery.
