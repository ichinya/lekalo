# Issue #77 implementation: coupling and change-radius architecture checks

Implemented on `ichinya/m7-issue-77` from approved
[issue-77-research.md](issue-77-research.md), research commit `6a31a2e3`,
base `a56ee5786f1724f9ee61ef973088c7679684c14d` (`ichinya/M7`). The live
[issue #77](https://github.com/ichinya/lekalo/issues/77) was reread with
`gh issue view 77 --repo ichinya/lekalo`; its body remains dated
2026-08-30T10:17:52Z. Product and new/changed contract generation: **0.6.4**.
The implementation uses the approved semantic recipe and remains advisory by
default. All paths and APIs below are implemented surfaces.

Implementation commits: `5b25f58b` (analysis, contracts, registry successors,
fixtures and CI gate) and `49af05c6` (baseline decoding, affected-operation
planning, immutable fixture checks and live receipt bindings).

## Delivered surfaces

| Surface | Searchable anchors and responsibility |
| --- | --- |
| Core module | `crates/lekalo-core/src/coupling/mod.rs`: `analyze`, `Request`, `Selection`, `WitnessStore`. Pure typed analysis; no host/process/provider access. |
| Semantic projection | `coupling/projection.rs`: `Projection::new`, `walk`, `cycles`, `snapshot`, `from_snapshot`, `declared_writers`. Indexed admitted canonical edges; internal bridges; unique neighbors; separate SCC series. |
| Union summaries | `coupling/aggregation.rs::append`. Module/project boundary fan metrics and deduplicated affected sets, excluding redundant conservative field rows when the exact owner is selected. Imports are separate evidence. |
| Profiles | `coupling/profile.rs`: `Profile::parse`, `validate`, `limits`, `DomainGroup`, `Centrality`, `RegressionLimit`. Project/module thresholds, explicit domain groups, reviewed centrality and opt-in strict policy. |
| Baselines | `coupling/compare.rs`: `parse_baseline`, `validate`, `compare`, `denial`. Closed decoding; semantic projection replay; exact signed/rational deltas; reviewed baseline digest; required unknowns deny. |
| Evidence owners | `coupling/evidence.rs::Evidence::join`: existing artifact, trace, query and transaction owners validate their bytes and current pins before a join. Replica obligations compare typed fields/presence/identity. |
| Changed inputs | `coupling/input.rs::ChangeInput::parse` constructs existing `impact::ChangedInputSet`; no second Git parser. |
| Wire and diagnostics | `coupling/wire.rs`: `Report`, `State<T>`, `Snapshot`, `TransactionScope`, duplicate-key decoder and bounds. `coupling/diagnostic.rs` uses `diagnostics::normalize::build`. |
| CLI | `crates/lekalo-cli/src/coupling.rs`: `CouplingArgs`, bounded reads, `execute`, status/stream mapping, readable metric states. One localized dispatch addition in `main.rs`. |
| Usage | [docs/coupling.md](../coupling.md), plus the coupling entry in `docs/cli.md`. |
| Contract gate | `scripts/test-coupling-contracts.mjs`: mandatory built CLI, independent graph oracle in a temporary copy, strict Ajv 8.17.1, negative controls, immutable corpus fingerprint. |

## Acceptance criteria → evidence

| Live acceptance criterion | Implementation and executable evidence |
| --- | --- |
| Report identifies semantic hotspots with evidence. | `Report.subjects`, unique sets, digest-bound `projection`, ordered `witnesses`, explicit coverage and registered findings. Gate probes `unique-semantic-neighbors`, `reverse-closure-oracle`, `actual-path-witnesses`, configured fan/public/artifact/shared-abstraction thresholds. Planner Task has fan-in **6**, fan-out **5**, public radius **8**, and one declared shared mutable resource. Golden: `tests/fixtures/coupling/golden/planner.report.json`. |
| Formatting/file split alone does not change coupling metrics. | Gate `formatting-invariant` compares complete reports after comments/CRLF changes. Core unit `source_map_relayout_does_not_change_semantic_measurements` redistributes source-map entries over different paths/spans and compares complete reports. Canonical Model enforces fixed file/kind placement: the gate retains `ir.kind-placement` rejection for a prohibited physical split; that loader rule is not weakened. |
| Public vs internal impact separated. | `Projection::partition` runs after reverse traversal. Core test `internal_bridge_reaches_public_and_supporting_is_separate`; gate `internal-bridge`. Taskhub retains internal `type:sync.local_request` on the path to public `operation:sync.sync_event`. Effects/policies/scenarios are supporting symbols. Baseline replay verifies classification and union counts. |
| Cycles and shared state risks visible. | `cycles` records separate symbol/module SCCs with real edge keys; `cycle_series_are_separate` asserts both series and `coupling.cross-module-cycle`. This is a pure typed-IR probe; cyclic imports remain a loader failure. `centrality_is_visible_and_advisory_by_default` and the live planner report assert `coupling.shared-mutable-state` with exact declared CRUD effect references. |
| Baseline regression can block strict profile. | `compare` and `denial`; core `strict_baseline_regression_and_threshold_edits_preserve_trend`; gate `strict-baseline-regression`, `equality-passes`, `unknown-required-denies`, `denial-retains-report`. Adding a unique Task consumer changes fan-in **6 → 7**. A reviewed strict baseline with allowance 0 exits **3**, retaining report/comparison; allowance 1 passes. A missing baseline or unavailable required metric cannot pass. |
| Suggested extraction links exact graph paths. | `Report.suggestions` references real `Witness.id` values and preserves six explicit semantic invariants; `applied` is always false. Gate verifies every edge key/occurrence/role against an independent graph export and verifies suggestion links. No refactoring or scheduling action is executed. |
| Reference Task/provider/planner modules used as real fixture. | Planner canonical files have byte parity with the landed context-budget workload, checked for both public and core-local copies. `taskhub` and `provider` are authored normalized semantic reference fixtures, run by the production CLI and compared to goldens. Taskhub references real synthetic Task/TaskPage/sync/client declarations; provider derives all **10** operation IDs from production `OPERATIONS`, with manifest fan-in **10**. `tests/fixtures/coupling/README.md` records exact source anchors/digests and normalization limits; fixture provenance marks the family synthetic. |

## Ten checks and policy decisions

| Issue check | Delivered measurement / coverage |
| --- | --- |
| Symbol/module fan-in/out | Unique semantic neighbors; foreign-module counts; repeated field occurrences preserve paths without increasing counts. `unique_neighbors_and_union_arithmetic`. |
| Public contracts affected | Reverse union after internal traversal; public kind breakdown; internal/supporting/unclassified sections remain separate. |
| Cross-module cycles | Symbol and module SCC series; actual edge keys; inherited fatal import-cycle rejection remains authoritative. |
| Shared mutable state | Declared writer operations per exact resource; aliases/references alone are not writes. Planner declares shared writes; Taskhub's immutable transition declares none. |
| Hotspot fields/types | Typed field dependency rows, type consumer radius and explicit owner-conservative `possible` sets. Unsupported fine-grained reverse evidence remains unknown, never an exact owner-wide count or invented lower bound. |
| Field → artifacts/tests/targets amplification | Accepted manifests/traces join current Model/IR/graph/revision pins. Evidence golden gives one artifact/test/check for the planner symbol. Field propagation remains conservative; target counts are explicitly unsupported because producer identity does not prove a generated target set. |
| Unrelated transaction/effect modules | `TransactionScope` retains each atomic group's modules/domain groups/classification. `transaction_spread_uses_explicit_domain_groups` verifies unknown without classification and a cross-domain group count of one with explicit groups. Independent groups are not pooled. |
| Excessive shared abstractions | Raw public consumer radius plus configured limits; no universal threshold. Reviewed centrality annotates only fan/public/shared-abstraction rules and cannot waive regression/incomplete evidence. |
| Public target-specific exposure | Transitive forward witnesses to typed `TargetSpecific` declarations. `transitive_target_exposure_is_a_fact_with_paths`. |
| Duplication divergence vs harmless duplication | Reviewed replica obligations; actual typed shapes. Gate verifies identical replicas produce zero and a divergent field type produces `coupling.duplication-divergence`. Unrelated local repetition without an obligation is unsupported. |

`planning` carries required public review, internal context, declared mutation
resources, transaction groups, native checks and evidence gaps. Optional context
planning invokes the existing context-budget producer and retains its required
facts and capsule simulation (`context_plan_keeps_required_facts_and_capsule_simulation`).
For typed changed inputs, complete affected operations feed the existing effects
conflict owner (`changed_entity_maps_affected_operations_to_the_effects_owner`).
Runtime conflict evidence reports **declared-only** coverage. Public review overlap
is a separate set and does not authorize scheduling or prove runtime safety.

Default profiles have no numeric thresholds and no denial. Named profiles use
closed project limits, inherited module overrides, absolute/relative regression
allowances and explicit reviewed baseline bytes. Threshold edits preserve
measurement comparability. Equal allowances pass; a zero baseline has no relative
ratio; added/removed or unavailable measurements do not become zero-valued
comparable rows. Missing/forged counts, metric keys, witness paths, duplicate keys,
unknown members, unavailable states with values, stale attachments and unsupported
versions fail through registered diagnostics.

## Contracts, registry and additive merge

Five new closed Draft 2020-12 schemas are shipped at 0.6.4:
`coupling-{report,profile,comparison,evidence,change-input}`. Each has positive
goldens, negative decoding/semantic controls, synthetic provenance and the same
mandatory own gate. Gates never invoke the deliberate schema/fixture writers.
The gate uses only Node built-ins and **Ajv 8.17.1**; no dependency was added.

The active registry successor adds exactly **15** `coupling.*` entries,
`LEK-COUPLING-001..015`, preserving predecessor entries, statuses, messages and
normalization rules. Registry size is **474**. Validation profiles follow the
registry pin, with all **23** ordinary semantic rules unchanged. The closed
validation success receipt and provider discovery also take successors because
they pin those versions. Discovery retains its ten operations; coupling is not
silently advertised as a provider operation. Frozen Model/IR/graph/effect,
context-budget, CI-report, lock and orchestration wire schemas retain their own
versions. Current product/profile/digest bindings in live goldens were refreshed
without relaxing comparisons or schemas.

`.github/workflows/ci.yml` adds one `Run the coupling contract gate (issue #77)`
step in `build-test`, after `cargo build --workspace --locked`, with
`LEKALO_AJV_NODE_PATH` and `NODE_PATH`. Existing context-budget, provider, tests,
fmt/clippy/MSRV and platform lanes are retained.

Issue #76 owns its independent module, diagnostic family and gate. This work
does not inspect its worktree or depend on unmerged symbols. Merge seams are
localized: registry successor entries/pins, product/profile/provider consumers,
the core export/CLI dispatch, CI gate insertion and suite coverage. Merge by
unioning independent family entries and retaining both gate steps; rebind the
shared successor pins and metadata goldens if the merged product generation
changes. Do not rename family codes or fold readability metrics into coupling.

## Local validation

Verification was performed on Windows with the built production CLI. Commands
below are reproducible; gate runs use `NODE_PATH=$LEKALO_AJV_NODE_PATH` pointing
to the existing external Ajv 8.17.1 installation.

| Check | Evidence/result |
| --- | --- |
| `cargo fmt --all`; `cargo fmt --all -- --check` | Passed. |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed; no lint suppression added. |
| `cargo build --workspace --locked` | Passed. |
| Core coupling integration + source-map unit | 12 integration tests and one source-map relayout test passed. |
| `cargo test --workspace --locked --no-fail-fast` | 1,929 tests passed across 94 suites; zero failures. Two pre-existing cache benchmarks remain ignored. |
| `node scripts/test-coupling-contracts.mjs` | Five schemas; 36 live checks. Repeated runs pass and preserve corpus bytes. Missing production binary is a hard failure. |
| Touched contract gates | Diagnostic, validation, context-budget, classification, expressions, provider, CI-report, lockfile (native + Ajv) and orchestration gates passed. |
| Fixture/suite checks | Provenance (69 synthetic families), catalog (21 cases), diagnostic coverage (474 rules), normalization and determinism (41 rows across two cold lanes and warm-cache lane) passed. |
| Golden verification/update policy | `run-golden.mjs --verify` passed for 21 cases and four byte-compared envelopes; `test-golden-update-policy.mjs` passed the digest-bound plan/review/apply controls. |
| Version governance | `check-contract-versions.mjs --base a56ee578`, `test-contract-versions.mjs` and `test-versioning-contracts.mjs` passed. |

The minimal suite golden was refreshed with the existing reviewed plan/apply
flow. Its only semantic deltas were `profileVersion` and `registryVersion`
0.6.3 → 0.6.4; checksums and the two-lane run manifest were rebound. Other live
receipt updates preserve outputs and change only product/profile/lock/manifest
bindings and normative provider formatting. No write/skip path was added to a
gate. The graph oracle runs in a temporary copy because its existing command
owns a cache; the coupling producer and the gate leave the canonical corpus
unchanged. Runtime caches are excluded from the new corpus and its authoring
copy operation.

Observed architecture and generated-target enumeration remain explicitly
unsupported; field reverse propagation without accepted precise evidence remains
unknown. Confirmed trace fixture rows are synthetic declarations, not evidence
that native tests/provider execution occurred. CI wiring was verified locally;
remote Linux/macOS/Windows CI and deployment acceptance are not claimed. Nothing
was pushed and no other worktree was accessed.
