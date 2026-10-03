# Issue #76 implementation: optional AI readability lint

Implemented on `ichinya/m7-issue-76` from the approved [research](issue-76-research.md), commit `7bfdb79f`, based on `ichinya/M7` at `a56ee578`. Product and successor contracts use `0.6.4`. The research document remains unchanged. Implementation was committed incrementally; no branch was pushed.

## Delivered surfaces

| Surface | Searchable implementation anchors | Result |
| --- | --- | --- |
| Core service | `crates/lekalo-core/src/ai_lint/mod.rs`: `Request`, `analyze`, `observed_ambiguities`, `state_writes`, `defaults`, `render` | Checks accepted Model/IR, semantic graph, observed bindings and admitted evidence; target language parsing stays in adapters. |
| Evidence admission | `ai_lint/input.rs`: `parse`, `admit_evidence`, `read_source`; `ai_lint/wire.rs`: `Evidence`, `Record`, `Finding`, `Coverage`, `State` | Closed documents, current source fingerprints, UTF-8 byte ranges, semantic references, producer/input pins, activation guards and four-state uncertainty. |
| Depth and trend | `ai_lint/depth.rs`: `measure`; `ai_lint/compare.rs`: `validate_report`, `compare`, `validate_comparison` | Iterative SCC condensation; semantic dependency and native call depths remain separate. Strict immutable baselines distinguish raw change, waiver change, depth change and incomparable coverage. |
| CLI | `crates/lekalo-cli/src/ai_lint.rs`: `AiLintArgs`, `execute`; `main.rs`: `AiLint` | Exactly one selector: `--symbol`, `--module` or `--all`. Optional evidence, installed collectors, config, attachments, waivers, baseline, spans and explicit gate. |
| Installed collector | `ai_lint/collect.rs`: `collect`, `request`; `orchestration/catalog.rs`: `discover`; `target_protocol/mod.rs`: `TargetClient` | Selected package inventory, integrity/trust/revocation and manifest consistency are checked before the read-only protocol operation. Confirmed/explicit current observed bindings supply semantic ownership; nominated source files limit actual read grants. |
| Node collector | `adapters/node-typescript/src/ai-lint.mjs`: `collectAiLint`, `validateLintRequest` | Vendored TypeScript 5.9.3 AST/checker recognizes bound EventEmitter registration, emission, callback field writes and unresolved computed calls. Application imports and package scripts are not executed. |
| PHP collector | `adapters/php-laravel/src/ai-lint.php`: `lint_collect`, `lint_validate_request` | Built-in token parsing recognizes imported Eloquent model inheritance, explicit observer registration, bound save trigger, callback field writes and computed method calls. Application files are not included or executed. |
| Diagnostic seam | `contracts/diagnostic-registry.v0.6.4.json`; `diagnostics/{registry,version}.rs`; `validator/profile.rs` | 476 registry entries: all 459 predecessor entries plus 17 active issue rules. Warning findings and service failures use registered identities and closed data fields. |
| Contracts and CI | `scripts/gen-ai-lint-{contracts,protocol}.mjs`; `scripts/test-ai-lint-*-contracts.mjs`; `.github/workflows/ci.yml`: `Run AI lint contract families (issue #76)` | Five schema/golden/provenance/live-gate families, all wired into build-test after cargo build, using exact Ajv 8.17.1 through `LEKALO_AJV_NODE_PATH`. |

The five new families are `ai-lint-report`, `ai-lint-evidence`, `ai-lint-config`, `ai-lint-waivers` and `ai-lint-comparison`. Each has `contracts/<family>.schema.v0.6.4.json`, `tests/fixtures/<family>/golden/`, an explicit synthetic provenance entry, and `scripts/test-<family>-contracts.mjs`. Shared live producers and independent semantic assertions reside in `scripts/lib/ai-lint-contract-gate.mjs`; absent binary or adapter execution fails the gate. The umbrella source fixtures and adapter snapshots also have declared synthetic provenance.

Successors also cover target protocol, adapter manifest, validation profiles/report, provider capabilities, diagnostic registry and the internal version registry. Published predecessor schemas are retained byte-for-byte. Ordinary lock/bootstrap protocol remains `0.3.2`; the two-phase Describe exchange explicitly negotiates `0.6.4` for lint. An old-version Describe omits lint operation/capability members and retains empty read scopes for a bare describe-only Node launch; an old-version lint request refuses. Native planning and resolved profile members retain their meaning on both supported versions.

Existing artifact manifests pin the load envelope; new evidence pins the normalized Model. CLI computes both current pins through existing loader surfaces, and core validates the artifact pin against its producer's exact convention. Trace joins require a current Model pin, IR, source revision, manifest digest, content fingerprint, matching semantic owner/lifecycle and semantic-to-artifact binding. Missing metadata yields unknown coverage. Transition attachments use their existing normalized Model/IR pins. These bridges change no existing attachment schema.

## Checks and diagnostic ownership

| Research check | Delivered decision and diagnostic |
| --- | --- |
| C1 competing resolutions; C2 ambiguous short IDs | Core retains all distinct current binding alternatives: `ambiguity.multiple-resolutions` / `LEK-AMBIGUITY-001`. CLI reuses exact inspect selection refusal; a lint waiver cannot select a binding. |
| C3 implicit hooks and observer state changes | Adapter supplies binding/trigger/registration/callback/effect evidence; core compares command effects and exact resource identity. `hidden.undeclared-effect` / `LEK-HIDDEN-006`, `hidden.observer-write` / `LEK-HIDDEN-009`. Codes 007/008 remain unallocated. |
| C4 unbound locator/dynamic dispatch | Core joins admitted dispatch evidence with complete binding coverage: `hidden.dispatch-without-binding` / `LEK-HIDDEN-001`. Language-specific locator recognition belongs to a target collector. |
| C5 convention-only path | Core checks admitted convention/configuration evidence with complete coverage: `hidden.convention-only-path` / `LEK-HIDDEN-002`. |
| C6 excessive indirection | Semantic graph recipe selects references/accepts/returns/reads/emits/exposes; native edges are a separate dimension. Explicit thresholds produce `indirection.depth-exceeded` / `LEK-INDIRECTION-001`. Recursion is reported separately. |
| C7 unrelated exact-field writers | Core joins admitted command/resource/field writes with an explicitly supplied current transition attachment and configured related-writer groups. CRUD declarations alone do not imply a field assignment. `ambiguity.scattered-state-writes` / `LEK-AMBIGUITY-002`. |
| C8 execution path without owner/trace | Core compares current ownership and trace attachments: `hidden.path-without-trace-owner` / `LEK-HIDDEN-003`. An absent/incomplete/stale join never proves ownership absence. |
| C9 unresolved reflective call | Adapter recognition; core admitted evidence: `hidden.reflective-call` / `LEK-HIDDEN-004`. |
| C10 unresolved string reference | Adapter establishes a reference role; core admitted evidence: `hidden.string-reference` / `LEK-HIDDEN-005`. Ordinary strings are not treated as references. |
| C11 divergent implicit target defaults | Core compares the same subject/key across distinct targets, retaining actual values and explicit permitted differences: `ambiguity.implicit-target-defaults` / `LEK-AMBIGUITY-003`. |

Service diagnostics are `ai-lint.input-invalid`, `version-unsupported`, `coverage-incomplete`, `waiver-invalid`, `baseline-incomparable` and `policy-denied` (`LEK-AILINT-001` through `006`). Every active rule has a real live producing vector in the family gates and a current suite coverage anchor.

## Profiles, claims and suppression

AI profiles `off`, `advisory`, `ci` are independent of mandatory semantic validation. Normal validate/observe/inspect do not enable lint. Advisory emits findings with exit 0. Explicit `--check` applies the selected gate and returns denied/exit 3 while retaining the report. Default thresholds remain unknown; there is no universal depth or writer limit. Config can select thresholds, required coverage, minimum gate confidence, comparable baseline and regression policy. Finding severity can be reduced to info; mandatory diagnostic families cannot be waived by this service.

Each finding carries stable identity, condition digest, semantic symbol or explicit uncertainty, target/scope, evidence and location references, witness, guards, confidence, claim, active/waived disposition, waiver reference and a concrete alternative. Identity excludes source offsets/revision; a changed source/configuration condition revokes suppression. Default projection keeps source paths out of the report; `--spans` adds current logical source ranges.

Static collectors issue possible-behavior claims. Confidence is the weakest activation/candidate/record confidence, capped at medium for inferred evidence and high for behavioral claims. Native depth also retains the weakest edge confidence; low confidence is info. Unknown, stale or missing native edges and bounded witnesses yield unknown depth and partial coverage. Static effect/dispatch/reflection/reference records cannot claim structural or verified behavior; this release has no execution-proof producer and verified-effects remains zero.

Waivers require exact rule, subject, target and condition digest, bounded owner/reason and an optional exact source pin. `expiresOn` uses the required four-state field: `{state:'unknown'}` means no expiry, and a known date requires explicit `--as-of`. Expiry is inclusive on that date. Empty reasons, bad dates, duplicate scope and foreign config pins refuse. Applied, expired, condition-changed, source-changed and orphan dispositions remain auditable; raw counts are preserved.

## Acceptance evidence

| Live issue acceptance criterion | Executable evidence and result |
| --- | --- |
| **AC1: ambiguous binding never resolves silently** | `reportCases` produces the two-candidate ambiguity warning and retains its witness; `comparisonCases` removes one candidate and independently checks raw ambiguity delta -1. `adapterCases` rejects duplicate native bindings naming different semantic commands before collection. Existing bindings/inspect/observed workspace tests exercise mandatory ambiguity refusals. Lint performs no resolution write. |
| **AC2: hidden observer/dynamic effect visible in PHP/Node fixtures** | `adapterCases` executes both shipped bundles on `ai-lint/model/{src/events.ts,app/events.php}`, admits fresh evidence and asserts observer-write plus reflective-call findings for `planner.focus_task`. It installs both adapters, records/confirms observed bindings via the real CLI, then repeats the positive assertions through installed collector, production TargetClient decode and core join for module and single-symbol selection. Entity bindings remain available to map writes under `--symbol planner.focus_task`. Declared `planner.edit_task_cmd` update and removed observer registration are negative controls. |
| **AC3: suppression scoped, justified and optionally expiring** | `waiverCases` verifies exact application, unchanged raw counts, inclusive expiry/next-day expiry, required as-of, permanent waiver, foreign target remaining active, invalid reason/date, duplicate scope and changed-condition audit. Waiver/report-waived goldens validate against their own schemas. |
| **AC4: affected symbols and source spans linked** | Fresh collector findings link `planner.focus_task`, mapped task/state, all five activation roles and source spans; installed run requests `--spans`. Admission checks exact fingerprints, UTF-8 ranges and Unicode line/column positions using source fixtures containing non-ASCII and emoji. `evidenceCases` rejects stale fingerprints/ranges and missing semantic fields, then shifts source text and asserts stable IDs with changed condition digests. Default path privacy and deterministic bytes are asserted. |
| **AC5: low-confidence inference not reported as proven fact** | `evidenceCases` lowers registration and native-edge confidence and requires info findings; forged verified-behavior and structural effect claims refuse. Stale, unknown and missing native edges yield unknown depth/partial coverage. A 33-edge chain exceeds the witness bound and denies a required-coverage check even with active-warning gating disabled. Stale/partial inputs cannot prove an absence. All fresh adapter reports assert verifiedEffects = 0; required missing coverage can still deny through `configCases`. |
| **AC6: trend measurable across revisions** | `comparisonCases` supplies distinct revisions with current trace provenance, checks comparable unchanged counts, then increases native depth without increasing the finding count and requires regression/depth delta +1. Removing a binding competitor yields raw delta -1. Waiver-only churn changes active counts with raw delta 0 and no structural regression. Confidence-bin migration also keeps raw totals/regression unchanged. Unknown coverage is incomparable; malformed arithmetic, u64-sized depths and unordered metrics refuse; baseline bytes remain immutable. |
| **AC7: optional for ordinary observed legacy code** | `reportCases` exercises off/advisory/ci/check: off yields zero findings/disabled coverage, advisory remains valid, explicit ci check denies with retained report. Missing target evidence stays unknown. Installed tests preserve observed index and source bytes. The full existing validate/observe/inspect suites provide regression evidence for their ordinary behavior. |

## Validation record

Validation used the current built CLI and committed adapter bundles, with Ajv 8.17.1 provisioned outside the checkout. No extra dependencies were added for the new family gates.

| Commands / gate | Local result |
| --- | --- |
| `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo build --workspace --locked` | PASS. |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS on Rust 1.98.0. |
| `cargo +1.80.0 clippy --workspace --all-targets --locked -- -D warnings` | PASS using a separate target directory inside this worktree. |
| `cargo test --workspace --locked` | PASS, exit 0: 1,918 passed, zero failed, two ignored across 93 test-result groups, including doc tests. |
| `test-ai-lint-{report,evidence,config,waivers,comparison}-contracts.mjs` | All five PASS: closed schemas, goldens and fresh production CLI behavior. Evidence gate includes installed Node/PHP module and single-symbol positives, registration/declaration negatives and bounded-depth required-coverage denial. |
| `test-{diagnostic,validation,classification,expressions,context-budget,provider,adapter-manifest,versioning,lockfile,target-protocol}-contracts.mjs` | All PASS. Current registry contains 476 entries; context-budget includes the live producer; target protocol checks 19 goldens, 30 invalid and 13 contextual vectors. |
| `test-adapter-manifest-golden.mjs` | PASS for exact artifact bytes, framed package digests and negotiated lint compatibility on both shipped adapters. |
| `test-node-typescript-kernel.mjs`, `test-node-typescript-scanner.mjs` | PASS across four kernel and three scanner test files, including deterministic rebuild and old-wire bare Describe behavior. |
| `test-php-laravel-adapter.mjs` and strict production conformance | PASS: protocol, process and analyzer suites; deterministic PHP bundle; core-owned protocol/IR conformance badge. |
| `gen-ai-lint-{contracts,protocol}.mjs --check`, `gen-suite-{coverage,diagnostic-pairs}.mjs --check` | PASS with no regeneration needed. |
| `test-fixture-provenance.mjs`, `test-golden-{catalog,diagnostic-coverage,hygiene}.mjs` | PASS: 74 declared synthetic families, 21 catalog cases, all 476 registry rules covered, 260 fixture files checked. |
| `run-golden.mjs --verify` | PASS for all 21 cases; four declared producer outputs are byte-identical to committed goldens. |
| `check-structure.mjs`, `check-authority.mjs`, `check-privacy.mjs` | PASS. |
| `check-contract-versions.mjs --base 7bfdb79f`, `git diff --check` | PASS: 109 contract artifacts, 14 added successors and no modified predecessor contract paths; approved research has no diff. |
| Existing CI-report JSON goldens | All five validate with strict Ajv 8.17.1; Rust CI-report tests exercise production writers and projections. The unchanged full Node CI-report gate additionally requires its pre-existing Draft-04/formats dependencies for official SARIF validation; that complete gate is not claimed in this Ajv-only local record. |

## Usage and practical limits

`lekalo --json ai-lint --module planner` runs advisory core analysis. `--config lint-config.json --lint-profile ci --check` activates explicit policy. `--scan-target node-typescript --source-file src/events.ts` uses a selected installed collector; current confirmed/explicit observed command/entity bindings provide its semantic mappings. `--evidence` accepts separately produced closed evidence. Attach current `--transitions`, `--trace` and `--artifacts` only for their corresponding joins. A baseline file contains the report object extracted from `envelope.report` (or denied `envelope.payload.report`), not the outer CLI envelope.

Collectors deliberately declare partial support for their recognized language slices and unsupported coverage elsewhere. PHP framework version is unknown without a qualified input; neither static collector proves callback execution. External framework hooks, arbitrary locators and convention resolution require adapter evidence. Full/clean coverage is never inferred from a parser completing successfully.

Bounds include 8 MiB input/source/report payloads, 10,000 evidence/selection/finding rows, 32-step witnesses, 50,000 nodes, 250,000 edges, 64 target inputs and a 10,000,000-work admission/graph budget. A witness exceeding its bound yields unknown maximum instead of a smaller apparently complete value. Sources are read under the shared project Fs capability and protocol confinement; collection grants no writes.

Research test organization was consolidated into five family gates plus core depth unit tests and existing regression suites, rather than adding parallel standalone test runners. This keeps each new contract family responsible for schema vectors, committed goldens and fresh production behavior. Local acceptance records Windows with Rust 1.98.0, Rust 1.80.0, Node 24.13.0 and PHP 8.5.0. New live gates are configured in the Node 24 build-test matrix on Ubuntu/Windows/macOS with PHP 8.3; existing contract jobs use Node 18/24. Remote CI remains unrun because this branch was not pushed.
