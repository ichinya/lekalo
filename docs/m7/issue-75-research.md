# Issue #75: context-budget and local-understandability research

Research only, 2026-10-01. Acceptance authority: [issue #75](https://github.com/ichinya/lekalo/issues/75), read live, including all ten metric bullets, seven capabilities and seven acceptance criteria. Checkout: `ichinya/m7-issue-75`, base `9510dd0767a56c0ab34b8d3c8ceb2a14db8de825` (`ichinya/M7`); product version at inspection is `0.6.3`. The issue depends on #13/#14/#17/#20. [Issue #100](https://github.com/ichinya/lekalo/issues/100) was also read to verify the Framework Lift consumer contract. No implementation, provider evaluation, private-project scan, or measured AI-quality claim is part of this document.

Decision: add a read-only `lekalo context-budget` command backed by a pure `lekalo_core::context_budget` service. Share typed fact collection with `context`; measure the complete selected dependency closure before any token-budget selection. Keep minimum required semantic facts, supporting semantic facts, optional source context, and empirical AI sufficiency distinct. Use a pinned offline estimator, explicit coverage/value states, an explainable ledger, and a separate comparison attachment for semantic diff/QA.

All new paths, APIs, schema members and diagnostic codes below are proposals. `<V>` means the product version of the implementation commit, not a new independent `1.0.0` line. Existing contracts are not amended by this research.

## Existing-surface inventory

Paths in this section exist at the inspected base; API names provide searchable implementation anchors.

| Surface | Current implementation and contract | Consequence for #75 |
| --- | --- | --- |
| CLI context | `crates/lekalo-cli/src/main.rs`, `Commands::Context`, `run_context` (around line 5069): `context SYMBOL --budget N [--spans] [--project DIR]` or `context --changed IDS --budget N`; loader normalization, IR compilation, then `context::plan`. | CLI only resolves inputs and renders `DomainResult`; metric decisions belong in core. The command does not currently load a tokenizer/budget profile or observed binding evidence. |
| Capsule API | `crates/lekalo-core/src/context/mod.rs`: `CapsuleScope`, `plan`, `Capsule::{fits,estimated_tokens,minimum_required_tokens,candidate_count,included_count,excluded_count}`. `model.rs` contains crate-private typed `Fact` records; `select.rs` gathers and orders them; `render.rs` projects JSON/Markdown; `canonical.rs` orders keys. `contracts/context-capsule.schema.v0.2.16.json`, `docs/context.md`, ADR-0018. | Reuse/extract a typed collection seam, not parsing Markdown or JSON back into metrics. Existing wire output stays compatible unless an explicit successor is introduced. Core owns an in-memory capsule; AI Factory owns persisted `.ai-factory/context/**`. |
| Existing budget semantics | `context/select.rs` totals **all candidate facts** into `minimum_required`, then greedily includes each fact if it fits. Sections: symbol, policies, effects, dependencies, scenarios, public-impact, bindings, types, closure. Even an early protected fact can be excluded under a tiny budget. `context/version.rs`: budget <= 1,000,000, roots <= 128, manifest <= 50,000, supporting closure depth 2 / 2,000 nodes / 10,000 edges. | `minimumRequired` is not a new full-closure minimum-safe metric. Preserve its meaning. A new simulation must explicitly report missing mandatory facts; current `fits` alone cannot establish completeness. |
| Estimator | `context/estimate.rs`: `dev.lekalo.estimator.chars-4@0.2.16`, exact `SPEC` SHA-256; empty string -> 0, otherwise `ceil(Unicode scalars / 4)`, applied per typed fact's semantic values. | Already offline and integer-only, with no tokenizer dependency. It estimates semantic content, not an actual provider prompt including JSON/Markdown wrappers, tools and system text. |
| Graph | `graph::{build,DependencyGraph,NodeId,EdgeFilter,TraversalSpec,Direction,PathSpec}`; `DependencyGraph::{nodes,edges,node,resolve,direct_dependencies,reverse_dependencies,transitive,shortest_path}` in `graph/mod.rs` and `traverse.rs`. `model.rs` defines kind-qualified ids, edge occurrence, provenance and confidence; `cycle.rs` handles cycles. | Indexed forward/reverse adjacency and canonical tie breaking are reusable. Module imports (`requires`) are module-to-module edges, not implicit edges from every symbol. A symbol closure must not pull every symbol in every imported module. |
| Graph bounds and uncertainty | `graph/version.rs`: graph 100,000 nodes / 1,000,000 edges; query depth 256 / 50,000 nodes / 250,000 edges; canonical export 32 MiB. `transitive` returns a diagnostic on bound exhaustion, not a partial traversal. `graph/build.rs::Builder::finish` refuses unresolved endpoints. | Do not turn failure into zero or treat the accepted graph as a source of unresolved edges. #75 needs an evidence view plus an explicit completeness result. Existing `shortest_path` limits visited nodes as well as path construction; do not issue one path query per dependency at project scale. |
| Effects, policies, scenarios | `effects/mod.rs`: `build`, `attach_detected`, `EffectGraph::{declared,detected,operation_edges,envelope_count}`. Typed IR definitions contain policies/applies-to and scenarios/covers. `scenario/`, `authorization/`, `error_contract/`, `invariant_transition/`, `transaction_concurrency/`, `extended_effects/`, `expressions/`, `query_model/` own later attachments. | Policy/scenario edges can point **toward** the subject: forward traversal alone misses them. Core context still declares `error-contracts-unrepresentable` and absent-detected-effect gaps. Inventory all loaded attachment families; no claim of complete semantics when applicable attachments have no fact adapter. |
| Inspect and impact | `inspect/mod.rs::run`, `InspectRequest`, `Include`; inspect currently has optional bindings/scenarios, not budget reports. `impact/` owns bounded change-radius analysis, evidence, request profiles and gate selection. `impact/input.rs` has typed changed inputs; CLI `ImpactArgs` has Git `--base/--head/--worktree`. | Reuse selector validation, provenance and pure changed-input boundaries. Impact is principally reverse change propagation; it is not the forward dependency cost of understanding a symbol. |
| Source and target mappings | `loader/mod.rs::SourceMapEntry` and `ir/mod.rs::SourceMap` map semantic ids/pointers to logical model paths/spans. `observed/{types,index,view}.rs` supplies symbol locations, references, candidate mappings, fingerprints, origin/confidence and fresh/stale/missing states. `artifacts/types.rs::{ArtifactManifest,ArtifactEntry,SourceMapBinding}` binds owners, paths, content digests and lifecycle; `trace/` binds model -> artifact -> test/gate. | A declaration span identifies a Model file, not an implementation file. A target binding id alone gives no filename or byte size. Join only current, digest-bound evidence; unresolved mappings stay unknown. Do not invoke generation, scan adapters or native gates to calculate the report. |
| Ownership | Artifact lifecycles are `generated`, `scaffolded`, `checked`, `external`, `custom`; `artifacts/check.rs` owns drift checks. | Generated ratio must come from ownership evidence. Scaffolded code is maintained after creation; checked/custom are maintained; external/unclassified must be reported separately. Do not infer from extensions, directory names or headers. |
| Diagnostics | `diagnostics/registry.rs::REGISTRY_BYTES` embeds `contracts/diagnostic-registry.v0.4.0.json`; `normalize::build`, `DiagnosticSet::try_from_unsorted`, `types.rs`, `render.rs` enforce codes, fields, statuses and canonical order. `diagnostics/id.rs` accepts the `LEK-CONTEXT-NNN` grammar. `context/diagnostic.rs` currently reuses graph rules and introduces no context codes. | Add registered `context.*` rules through this seam. The registry includes the message catalog (`message_id`, default message, typed data fields). No literal ad hoc warnings or auto-fix payloads. The requested `LEK-CONTEXT-006` is unallocated at this base. |
| Existing profiles/policies | `validator/profile.rs` loads closed `validation-profile.*.v0.4.0.json`; CLI `validate --strict` selects built-ins (there is no general validation `--profile FILE`). Impact `--profile` selects `default` or `strict`, a different enum. `target_profile/{document,resolution}.rs` and `target-profile.schema.v0.2.16.json` compose target components with source/resolved digests; init/adopt `--profile` selects adapter configuration. Diff uses `--profiles`; storage profile is another family. `native_gate/policy.rs` governs execution. Module `policies.yaml` describes domain policies. | Introduce an independently typed **budget profile**, referenced with `--budget-profile`; do not overload these unrelated profile meanings or add budget keys to domain policies/target documents/native execution policy. |
| Semantic diff | `diff/mod.rs::{DiffRequest,compare,DiffResult}`, `identity.rs`, `canonical.rs`, `projection.rs`, `seed.rs`; CLI `run_diff` (around line 2505). `contracts/semantic-diff.schema.v0.2.16.json` compares two accepted `CompiledProject` values with stable identity/history handling. Descriptions, source paths and physical file boundaries are excluded from semantic equality. | Context cost can regress while semantic diff remains equal, e.g. longer documentation or source. Add a versioned assessment attachment without changing semantic equality, compatibility classes or affected seeds. Do not mislabel token growth a breaking API change. |
| M6 history | `run_history/{value,types,validate,store,limits,version}.rs`, `contracts/run-{record,observation,assertions}.schema.v0.4.0.json`, `run-history-store.schema.v0.4.0.json`; `docs/run-history.md`, `docs/m6/issue-121-implementation.md`. Store is local, scoped SQLite under `.lekalo/history/`; `history record --input -` ingests a harness observation. | Use history conventions, not a new storage engine. `Vs<T>` is crate-private and distinguishes known/unknown/withheld/unsupported; only known carries value. Provenance, source attribution, coverage, assertions separation and digest binding are reusable contracts, not arbitrary JSON extension points. |
| M6 limitations to preserve | Current `metrics.context` has bytes, estimatedTokens, includedFacts, candidateFacts, coverageRatio, representation and estimatorVersion. It has **no** estimator-id/digest, full closure metrics or report reference. Operation kinds are closed (`context`/`evaluation` exist). #121 explicitly deferred producer `--record-run` instrumentation and accepted authority admission of history artifact kinds. | No undocumented metric bag or silent insertion into v0.4.0. A full #75 history link requires a versioned successor; capsule-specific fields retain capsule meaning. Current recorder custody pins `privacy-policy`/`authority-matrix` 0.3.2 and is local-private/export-ineligible. #102 owns public aggregate export; #100 owns evaluation. |

Two code-level caveats affect the implementation plan. First, the existing depth-two `context/select.rs::closure` skips deeper frontiers without necessarily setting `complete=false`; its gap flag is therefore insufficient proof of full transitive closure. Second, `scripts/test-diagnostic-contracts.mjs` still checks the historic 0.2.16 registry, while Rust embeds 0.4.0: extend coverage to the newly active registry instead of trusting that gate alone. Neither is fixed in this research.

Version authority is `scripts/check-contract-versions.mjs`: changed/new contracts take the product version of their commit; older docs about independent version numbering do not override this checker. Register new families in `crates/lekalo-core/src/versioning/` and its compatibility fixtures as appropriate; update consumers of changed registry/profile refs together. Frozen privacy/authority contracts require their own successor procedure, not a mechanical version bump.

## Proposed command/report schema

### Command and ownership

Proposed CLI grammar (the existing global `--json` is reused):

```sh
lekalo context-budget --symbol planner.focus_task --budget-profile local-12k --profiles config/context-budgets.json --json
lekalo context-budget --module planner --budget-profile local-12k --profiles config/context-budgets.json --json
lekalo context-budget --all --budget 12000 --json
lekalo context-budget --symbol planner.focus_task --budget 12000 --simulate-capsule --json
lekalo context-budget --all --budget-profile local-12k --profiles config/context-budgets.json --baseline reports/base.context-budget.json --json
lekalo context-budget --all --budget-profile local-12k --profiles config/context-budgets.json --policy ci/context-budget-policy.json --json
lekalo diff OLD NEW --context-budget-profile local-12k --context-budget-profiles config/context-budgets.json --json
```

Exactly one of `--symbol`, `--module`, `--all`; `--project DIR` follows existing selection rules. Require an explicit budget or named profile: no scientifically endorsed default threshold. `--budget N` selects the documented generic chars-4 profile with N available content tokens; reject combining it with `--budget-profile` in v1. `--profiles FILE`, `--policy FILE` and `--baseline FILE` are bounded project-relative, capability-checked reads. No Git checkout, directory mutation, provider request, generation, native execution, scan or automatic history append. A caller may redirect stdout into its own report file; core never owns a new persistence home.

Use a new top-level command because `inspect` is a single-symbol projection and `context` emits a capsule; a project report and cross-revision comparison have different inputs and failure/coverage semantics. An inspect link/summary can be a later explicit contract addition using the same service, not an alternative metric implementation. Project/module reports enumerate per-symbol rows plus union summaries; a bare module graph node would measure only imports and is insufficient.

Core layout proposed under `crates/lekalo-core/src/context_budget/`:

- `mod.rs`, `request.rs`, `input.rs`, `version.rs`: typed request, prevalidated `Compilation`/graph/effects/attachment/evidence pins, resource bounds and report API.
- `closure.rs`, `facts.rs`, `metrics.rs`, `estimate.rs`: closure and required-fact selection, arithmetic and profile estimator dispatch. Extract a shared candidate collector from `context/select.rs` with legacy selection preserved and golden-tested.
- `profile.rs`, `policy.rs`, `wire.rs`, `canonical.rs`, `diagnostic.rs`: closed loaders, report model and registry-backed output.
- `simulate.rs`, `suggest.rs`, `compare.rs`, `history.rs`: deterministic budget simulation, advisory cuts, baseline comparison and a typed history observation adapter.

CLI `Commands::ContextBudget` / `run_context_budget` loads bounded inputs once; a filesystem evidence adapter uses `project_fs.rs` to validate selected file metadata/fingerprints, then supplies typed counts/digests to core. Core metric computation never opens paths or shells out. Optional source measurement is opt-in `--source-context mapped-files`: only current mapped files, whole-file accounting in v1, bounded by explicit limits; no recursive repository crawl. File bytes are counted in memory, not placed in the report or history. An additional `--paths` sidecar may expose project-relative paths locally; default artifact identifiers are opaque.

Separate result status from assessment. A valid complete or partial report under advisory policy returns 0 with `assessment: within-budget|over-budget|indeterminate`. A selected mandatory policy returns denied/3 on budget or regression failure and on required indeterminate measurements; malformed inputs return invalid/1, I/O unavailable/4, unsupported versions/5. Preserve the structured report alongside denial. Infrastructure failure must never appear as a budget regression. Existing `lekalo context` exit and truncation semantics are unchanged.

### Draft JSON shape

New closed contract families: `context-budget-report.schema.v<V>.json`, `context-budget-profile.schema.v<V>.json`, `context-budget-policy.schema.v<V>.json`, and `context-budget-comparison.schema.v<V>.json`. One report shape serves all three scopes; no free-form metrics dictionary. This **illustrative shape is not a measured result or a complete golden**: `<V>`, `<digest>` and sample counts are placeholders, expanded before conformance testing. Arrays are shortened to show row types; real ledgers must reconcile totals.

```json
{
  "status": "valid",
  "contextBudget": {
    "schemaVersion": "lekalo/context-budget-report/v<V>",
    "identity": "dev.lekalo.context-budget-report@<V>",
    "metricVersion": "context-budget-semantics/1",
    "scope": { "kind": "symbol", "id": "planner.sync_external_objects" },
    "provenance": {
      "modelVersion": "0.2.16",
      "modelDigest": "sha256:<digest>",
      "irDigest": "sha256:<digest>",
      "graphVersion": "0.2.16",
      "graphDigest": "sha256:<digest>",
      "effectDigest": "sha256:<digest>",
      "attachments": [],
      "evidenceDigest": { "state": "unknown" },
      "targetProfileDigest": { "state": "unknown" },
      "revision": { "state": "unknown" },
      "workingSetDigest": { "state": "unknown" },
      "coreVersion": "<V>"
    },
    "profile": {
      "id": "local-12k",
      "version": "1",
      "digest": "sha256:<digest>",
      "estimator": {
        "id": "dev.lekalo.estimator.chars-4",
        "version": "0.2.16",
        "digest": "sha256:<digest>",
        "basis": "estimated-semantic-values",
        "model": { "state": "unknown" },
        "calibration": { "state": "unknown" }
      },
      "availableContentTokens": 12000
    },
    "coverage": {
      "state": "complete",
      "scope": "declared-semantic-closure",
      "sourceContext": "not-requested",
      "limitsHit": [],
      "gaps": []
    },
    "subjects": [
      {
        "id": "operation:planner.sync_external_objects",
        "metrics": {
          "directDependencies": { "state": "known", "value": 8 },
          "transitiveDependencies": { "state": "known", "value": 79 },
          "requiredModules": { "state": "known", "value": 23 },
          "contextClosureEstimatedTokens": { "state": "known", "value": 46800 },
          "minimumRequiredSemanticTokens": { "state": "known", "value": 41200 },
          "optionalSemanticTokens": { "state": "known", "value": 5600 },
          "optionalSourceTokens": { "state": "unknown" },
          "modelFiles": { "state": "known", "value": 28 },
          "sourceFiles": { "state": "unknown" },
          "targetFiles": { "state": "unknown" },
          "maxCrossModuleHops": { "state": "known", "value": 4 },
          "unresolvedEdges": { "state": "known", "value": 0 },
          "ambiguousEdges": { "state": "known", "value": 0 },
          "declaredEffects": { "state": "known", "value": 6 },
          "detectedEffects": { "state": "unknown" },
          "policies": { "state": "known", "value": 3 },
          "scenarios": { "state": "known", "value": 9 },
          "largestRequiredArtifact": { "state": "unknown" },
          "duplicateSupportingTokens": { "state": "known", "value": 800 },
          "generatedMaintainedRatio": { "state": "unknown" },
          "minimumSafeContextEstimate": { "state": "known", "value": 41200 },
          "empiricallySafeContextTokens": { "state": "unknown" }
        },
        "assessment": "over-budget",
        "overByTokens": { "state": "known", "value": 29200 },
        "dependencyBreakdown": [
          {
            "dependency": "entity:planner.task",
            "module": "planner",
            "witnessEdges": ["edge:<digest>"],
            "factIds": ["fact:<digest>"],
            "exclusiveRequiredTokens": 240,
            "sharedRequiredTokens": 80
          }
        ],
        "factLedger": [
          {
            "id": "fact:<digest>",
            "subject": "entity:planner.task",
            "class": "required-semantic",
            "reason": "referenced-contract",
            "estimatedTokens": 240,
            "requiredBy": ["operation:planner.sync_external_objects"],
            "origin": "declared"
          }
        ],
        "simulation": { "state": "unknown" },
        "suggestions": [],
        "measurementSources": [
          { "metric": "directDependencies", "source": "dependency-graph" }
        ]
      }
    ],
    "summary": {
      "subjects": 1,
      "overBudgetSubjects": 1,
      "indeterminateSubjects": 0,
      "unionRequiredTokens": { "state": "known", "value": 41200 }
    },
    "policy": { "mode": "advisory", "digest": { "state": "unknown" } },
    "comparison": { "state": "unknown" },
    "diagnostics": [
      {
        "code": "LEK-CONTEXT-006",
        "subject": "operation:planner.sync_external_objects",
        "requiredModules": 23,
        "estimatedRequiredTokens": 41200,
        "availableContentTokens": 12000
      }
    ]
  }
}
```

The illustrative diagnostic row is a projection of registered diagnostics, not permission to bypass #11: implementation uses the existing diagnostic schema for the actual envelope. Add per-metric completeness/reason rows to `coverage.gaps` with closed metric keys; state wrappers themselves stay the exact four-state M6 union. Lower bounds are separate `coverage.lowerBounds` rows, never silently stored as exact counts. Every known metric has a bounded source enum and input reference; derived metrics name the computation version. Field absence, `null`, NaN, negative counts, unknown keys and unknown enum values must refuse in persisted output. A present `unknown`/`withheld`/`unsupported` with even `value:null` must refuse in both Rust and JSON Schema.

`subjects` has deterministic stable-id order. Summary includes union counts and per-subject distributions (count/min/max plus sorted worst subjects), never sums duplicated closures as project cost. Proposed optional typed records: `largestRequiredArtifact` is `{artifactId, role, bytes, estimatedTokens}`; ratio is `{generatedFiles, maintainedFiles, externalFiles, unclassifiedFiles, numerator, denominator}`; simulation is `{candidateFacts,includedFacts,requiredFacts,includedRequiredFacts,estimatedTokens,requiredFits,allCandidatesFit,excluded,legacyCapsule}`; suggestions contain boundary ids, cut edges, preserved facts, estimated before/after, blockers and advisory status. Counts in all summaries must reconcile with the ledger. No raw source text, file paths, provider messages or prompts in the default report.

### Baseline, semantic diff and QA

`--baseline` reads an immutable prior report, validates its closed schema and digest, and calls `context_budget::compare`. Bind same project scope locally without exporting repository names/URLs. Comparability requires the same metric/fact-selection version, estimator identity/spec digest, effective budget profile (including source selection), attachment capabilities, and resource limits. Model/IR/source **revision digests intentionally differ** across revisions and are recorded on both sides; requiring equality would prevent any trend. Unknown provenance, mismatched capabilities or incomplete needed metrics produce `incomparable`, not zero deltas or a pass. Profile/estimator changes produce a configuration-change row; remeasure both revisions under one pinned profile to obtain a trend.

Use exact stable kind/id pairs, with #18's accepted rename/tombstone mapping only; added/removed subjects are explicit, not compared against zero. Store signed absolute deltas, rational relative deltas, threshold crossings and top contributing fact/edge additions/removals. A base zero makes the relative delta unavailable with reason `zero-baseline`; absolute delta still exists. Compare both complete per-symbol rows and unions. Multiple pinned baselines yield an ordered revision series, with missing/incomparable points preserved; no clock-dependent regression decision.

Extend `run_diff` to compute the same report for both loaded compilations when the opt-in flags are present. Add a **versioned CLI assessment envelope** binding the unchanged #18 diff payload/digest to the closed context-budget comparison; do not add arbitrary fields to `semantic-diff.v0.2.16`. Keep `diff.equal`, classes, seeds and normal output unchanged without the flags. `context_budget/compare.rs` remains separate from semantic change classification. A documentation-only token increase must be visible in the assessment even when `equal=true`. No current top-level `qa` command was found; QA integration is an exit-status/JSON consumer in CI (proposed `scripts/test-context-budget-cli.mjs` and documented recipes), not a fictitious existing QA API.

For #100, provide stable measurement ids, exact report/profile/estimator/model/attachment pins, scope state (`observed|contracted|hybrid` as evidence, not inferred promotion), and value-state metrics. A future harness attaches task/arm/model/provider/execution-profile pins and actual usage; these measurements are not provider input/output/reasoning/cached tokens, task success, or empirical AI quality. Greenfield and observed brownfield must use the same report schema with different evidence coverage.

M6 integration is two levels: (1) unchanged `run-observation.v0.4.0` can receive the actual simulated capsule's existing context metadata under operation `context`, with supported measurement-source keys, separate hard assertions, and unavailable values unknown; it cannot store the full #75 result; (2) a deliberate `run-observation`/`run-record` successor adds a closed `contextBudget` summary plus exact report/estimator pins and measurement-source keys, wired through `run_history/types.rs` and `validate.rs`. Do not put closure tokens in capsule `estimatedTokens` when they measure different sets. Full report persistence remains local caller/harness evidence until ownership is accepted. Any digest-bound history dependent must use the existing same-scope registration/resolution seam so deletion/retention invalidates it; cached summaries cannot resurrect deleted evidence. Neither this report nor the recorder acquires a public export API.

## Metric definitions with data sources

### Closure and required-fact semantics

Define root set R by exact kind-qualified symbol id; module means all its definitions plus its module card, project means all modules/definitions. Unknown or ambiguous selectors fail; never choose the first alias. Build canonical graph/effect indexes once. Forward symbol dependencies use registered dependency-bearing relations (`references`, `accepts`, `returns`, `reads`, `writes`, `emits`, `derived_from`); include endpoint `exposes` and policy `authorizes` when they originate at the selected root. Report module `requires` closure separately and include module cards without expanding every imported module's members. Register every admitted relation and direction in the fact-selection specification; unsupported attachment relation kinds produce gaps.

Construct required fact set F by a deterministic fixed point: root contracts; reachable semantic contracts needed to interpret them (including type definitions); applicable policy/effect/error/invariant/transaction facts; referenced identifiers' contracts; directly covering scenario contracts; binding identity relevant to those subjects. Collect incoming policy applicability and scenario coverage explicitly and follow their required semantic references. Include loaded supported attachments through typed adapters; leave their behavioral array order intact. Public-consumer summaries, narrative descriptions, unrelated example scenarios and other nonessential summaries are supporting facts O. Raw implementation/target bytes S are optional and separate. This deliberately revises the protected/supporting split for the **new** report; legacy capsules retain their existing selection.

Maintain two views: dependency closure D over declared/evidence edges, and required-context closure F including applicability/coverage facts. Report which view every count uses. A policy's applicability to another symbol does not drag the entire project into D; its referenced contract may enter F with an explicit reason. Required-fact completeness is scoped to supported declared semantics, never inferred behavioral sufficiency of arbitrary source code. Missing applicable attachment facts make the minimum-safe estimate unknown. A full fixed-point walk must prove frontier exhaustion; a depth/node/edge/work limit hit is incomplete even when the visible set fits the budget.

| Metric from #75 | Definition, units, data source and implementation locus | Required test (proposed in `crates/lekalo-core/tests/context_budget.rs` unless stated) |
| --- | --- | --- |
| **M1 direct/transitive dependency count** | Direct = distinct adjacent dependency node ids outside R; transitive = all distinct reachable dependency ids outside R, **including direct**. Also emit indirect-only = transitive minus direct and edge-occurrence count separately. D uses `graph` adjacency plus separately labeled current observed evidence; dedup ids, not reference occurrences. `closure.rs`, `metrics.rs`. | `dependency_counts_chain_diamond_cycle`: A->B, A->C, B->D, C->D gives direct 2/transitive 3/indirect 1; self/mutual cycles terminate; duplicate edges do not inflate node count; invalid `requires` cycles retain graph failure. |
| **M2 context-closure estimated tokens** | Sum estimator over distinct canonical F union O facts before budget pruning. Emit required F, optional O and optional source S subtotals. Overall optional combined estimate is only known when requested S is complete. Reuse semantic-value estimator/shared facts, not truncated rendered capsule size. `facts.rs`, `estimate.rs`, `metrics.rs`. | `closure_is_measured_before_budget`: lowering budget changes simulation, not full cost; Unicode/empty facts, shared types and unsupported attachment gaps. |
| **M3 number of source/target files required** | Distinct logical file identities needed by the selected source-context recipe for F; distinguish `modelFiles` (IR source map), maintained implementation `sourceFiles` (fresh observed bindings/checked artifacts) and generated/reference target `targetFiles` (artifact/trace evidence). File roles may overlap; union dedups actual logical identity. Counts can be known without reading content; no mapping is unknown, confirmed empty is zero. `input.rs` evidence adapter, `metrics.rs`. | `file_roles_and_missing_maps`: many symbols in one file, one symbol in many artifacts, overlapping roles, same basenames in different paths, stale/missing mapping; model path must not become implementation path. |
| **M4 cross-module hops** | For each reachable dependency, minimum number of module-boundary transitions on any permitted root path (0/1-weight traversal); maximum of those minima is `maxCrossModuleHops`. Also count distinct boundary edges and required module ids (including root modules) for explanation. Unknown module ownership makes the affected hop metric unknown. Module import hops are a separate series. `closure.rs`. | `cross_module_zero_one_paths`: same-module chains add zero, A->B->A counts two on that path, a lower-crossing alternative wins even if it has more edges; cycles and canonical witness ties are stable. |
| **M5 unresolved/ambiguous edges** | Count distinct reference occurrences with zero resolution candidates / more than one candidate in the selected evidence domain. Accepted canonical graph alone has known zero unresolved canonical endpoints, not proven zero implementation uncertainty. Join `observed::ReferenceEvidence` and `SymbolRecord.candidates` without choosing a candidate; binding ambiguity is separately classified when it cannot be attributed to an edge. Build/validation failure retains its own diagnostics, with dependent metrics unknown. `input.rs`, `closure.rs`. | `unresolved_and_ambiguous_never_zero_by_absence`: canonical zero, observed unresolved target, ambiguous binding candidate set, unknown observation coverage, stale scan and fatal canonical reference failure. |
| **M6 effect/policy/scenario count** | Unique effect fact identities, policy ids and scenario ids applying to R and F, with root-only vs closure totals. Declared and detected effects remain separate; absent detection capability = unknown, not zero. Legacy scenarios and Scenario IR attachments count once by canonical identity without counting each step as a scenario. `effects::operation_edges`, IR policies/covers, attachment adapters in `facts.rs`. | `incoming_policy_and_scenario_coverage`: reverse applicability found, shared policy counted once, conflicting/stale evidence cannot be promoted, declared and detected provenance preserved. |
| **M7 largest required artifact** | Among required mapped artifacts for the selected recipe, max estimated tokens and a separate max bytes, each with opaque artifact id and role; stable id breaks ties. Use current manifest/source fingerprints and bounded content measurement, not SHA digest length, LOC or whole-project size. Semantic-only mode additionally reports largest required semantic fact; this must not masquerade as a file. Missing candidate size leaves exact max unknown with a visible known lower bound. `input.rs`, `metrics.rs`. | `largest_artifact_ties_and_unknown_candidate`: byte/token winners may differ; huge optional file excluded; unread/missing file blocks exact max; privacy output contains no source. |
| **M8 duplicate supporting context** | Before dedup, collect bounded provenance requests `(root, referring-edge, fact-id)`, never enumerate all graph paths. `duplicateSupportingTokens = sum(request tokens) - sum(unique supporting-fact tokens)`; report duplicate fact requests and, separately, duplicate source content only when identical full-byte digests are known. Distinct semantic identities with identical prose are not merged. `facts.rs`, `metrics.rs`. | `supporting_diamond_dedup`: D requested through B and C counted once in closure but has one duplicate request; repeat report roots do not fabricate duplicates; same prose/different contract preserved; cycles cannot create infinite requests. |
| **M9 generated vs maintained ratio** | Primary file-count ratio G/(G+M), stored as integer numerator/denominator plus all lifecycle counts. G = generated; M = scaffolded + checked + custom. External and unclassified remain separate. A token-weighted ratio is optional only with complete token counts. G+M=0 yields unknown with reason `empty-denominator`, not 0 or NaN; partial ownership does not yield an apparently complete ratio. `artifacts/types.rs`, `input.rs`, `metrics.rs`. | `ownership_ratio_not_name_heuristic`: lifecycle mapping, external-only/no-file inputs, duplicate mappings, partial ownership; fixture names/extensions never alter classification. |
| **M10 minimum safe context size** | `minimumRequiredSemanticTokens = sum(F)` if required selection is complete. `minimumSafeContextEstimate` adds profile framing/margin to that deterministic estimate (effective budget has already reserved output/tools/system capacity). It is a **profile-scoped structural estimate**, not a measured threshold for AI success. Optional O/S never silently become required. `empiricallySafeContextTokens` remains unknown absent task/model/harness calibration from #100. If frontier/evidence/required attachment coverage is incomplete, minimum-safe estimate is unknown, with separate known lower bound. `facts.rs`, `estimate.rs`, `simulate.rs`. | `minimum_required_is_separate`: tiny budget cannot claim a sufficient capsule; optional source changes do not alter F; mandatory type/policy/error fact removed => incomplete; empirical safety stays unknown for uncalibrated profiles. |

No metric aggregates known and unknown values into a fake total. Ratios and deltas use integer numerators/denominators; human formatting is a separate projection. Structural counts are exact only within declared coverage. For example, a closed canonical Model may have exact counts while observed code remains unmeasured.

### Explainability, simulation and extraction boundaries

Every included required fact carries stable id, typed owner, inclusion reason, estimated token cost, provenance and the edge/attachment that required it. Give each dependency a canonical witness path and per-module attribution. Attribute each shared fact once to a deterministic primary owner for additive totals; list other consumers separately. `exclusiveRequiredTokens + shared attribution` must reconcile to F; repeat appearances in witness paths are not billed twice. Show the largest contributing modules/facts and excluded candidates in both JSON and human output. Bound displayed top-N rows, but keep truncation/count metadata; never label a truncated breakdown complete.

`--simulate-capsule` performs no provider call. It reports (a) the unchanged legacy `context::plan` result and its version/bounds, and (b) the new required/supporting selection at the same effective budget. Required facts are atomic as a sufficiency set: if F does not fit, `requiredFits=false`, missing required ids are explicit, and no output is called safe; an explanatory preview may still be returned. When F fits, supporting candidates are selected by stable priority/id and then optional source files in stable order. Report achieved mandatory coverage separately from overall candidate coverage. Reuse the same fact/estimator engine; do not equate legacy `minimumRequired` or `fits=true` with new full closure coverage.

Extraction suggestions use declared/evidence module boundaries and graph components: identify a cluster reached through a small explicit interface, list crossing edges, effects/policies/scenarios that must remain visible, and simulate the caller context with that **proposed** interface contract. Rank candidates by estimated savings, then boundary id. Reject or label blocked candidates that hide required authorization, transaction, effect or type facts; SCCs must not be split on an arbitrary edge. Before/after numbers are hypothetical estimates, not a real refactor or a promise of better AI quality. Emit no patches, generated interfaces or autofixes. Policy may gate the measured budget/regression, but cannot make an unvalidated suggested extraction a mandatory automatic change.

## Determinism strategy

1. Fingerprint normalized Model/IR, graph/effects, every consumed attachment/evidence inventory, artifact byte digests, selector set, metric algorithm/fact-selection version, full effective profile, estimator spec and policy. A target/model name without exact revision/spec digest is not a reproducible tokenizer identity. Changing any measured input produces a different provenance tuple.
2. Preserve the current chars-4 estimator unchanged. For required and supporting facts, count the same canonical semantic-value strings using Unicode scalar iteration, integer division and checked addition. Optional source uses a separate declared whole-file UTF-8 scalar recipe; count exact bytes with no locale or hidden CRLF/Unicode normalization. No tokenization network calls, downloaded vocabularies or new Cargo dependencies.
3. A successor optional estimator may apply a profile-declared rational scale and fixed overhead: `ceil(base * numerator / denominator) + overhead`, with checked arithmetic and positive bounded denominator. It must have a new identity/spec digest and cannot be labeled an exact named-model tokenizer. Add calibration/model revision pins and documented corpus/error bounds when evidence exists; otherwise calibration state is unknown. Exact BPE/model tokenization is deferred until an explicit pinned vocabulary/algorithm contract exists. Unsupported tokenizer ids refuse; never silently fall back.
4. Sort roots, graph nodes, edges, evidence and fact ids by documented UTF-8 byte keys. Dedup exact identities with provenance-conflict checks. Use visited sets/SCC condensation to handle permitted cycles; preserve ordered scenario/expression steps. Canonical compact UTF-8 JSON, sorted object keys, one LF for stored report bytes; render human output from the same typed value. Exclude wall clock, random run id, absolute path, Git branch label, performance duration and locale formatting from metric bytes/digest. History ingestion timestamps live outside this deterministic payload.
5. Enforce typed resource bounds before expansion: inherit graph query hard maxima, bounded facts/report bytes, explicit project-wide work-unit and subject limits, and bounded file count/per-file/total bytes for opt-in measurement. Build indexes once and memoize closures/components for shared subgraphs; do not materialize an unbounded all-pairs graph. Work exhaustion is deterministic incomplete coverage; wall-time cancellation is unavailable/infrastructure, not a partial success chosen by host speed. Never raise the legacy 128-root capsule bound implicitly for project reports: analyze per subject and union dedup separately.
6. Closed schema validation and typed semantic validation are both required. Use explicit nested structs/unions, reject duplicate keys and unknown fields, enforce safe integer limits for JS consumers, and check total/ratio/ledger arithmetic. Serde's current official docs confirm `deny_unknown_fields` and that it is incompatible with `flatten`; fetched through Context7 (`/websites/serde_rs`): [container attributes](https://serde.rs/container-attrs.html), [flatten restrictions](https://serde.rs/attr-flatten.html). Do not copy a permissive map or rely on absent `Option<T>` to distinguish explicit null from absence.

## Diagnostics (LEK-CONTEXT-*) proposal

Reserve/register this family in the active registry successor, update `diagnostics/version.rs`, registry embedding, matching validation profiles when relevant, contract fixtures and message rendering. `context_budget/diagnostic.rs` must call `normalize::build` with registered typed data. Bounded semantic ids, integer counts and fixed reason tokens only; no paths/raw input in messages. Suggestions live in the report; no fix id until a separately authorized refactoring contract exists.

| Proposed code / rule | Trigger and default behavior |
| --- | --- |
| `LEK-CONTEXT-001` / `context.input-invalid` | Invalid selector combination, unknown selector, malformed report/evidence/profile shape, invalid arithmetic or unsupported relation selection: invalid/1; no best-effort clean result. Existing loader/IR failures pass through with original codes. |
| `LEK-CONTEXT-002` / `context.profile-unsupported` | Unsupported profile/estimator/contract version or missing exact tokenizer spec: unsupported-version/5; no fallback. |
| `LEK-CONTEXT-003` / `context.closure-incomplete` | Frontier bound, unresolved/ambiguous required edge, stale required evidence or unrepresentable applicable attachment: warning in advisory report, assessment indeterminate (or definitely over-budget when a proven lower bound already exceeds budget). |
| `LEK-CONTEXT-004` / `context.artifact-evidence-incomplete` | Missing/stale mappings, sizes or ownership for requested source recipe: relevant metrics unknown; unrelated complete semantic metrics remain available. |
| `LEK-CONTEXT-005` / `context.baseline-incomparable` | Profile/metric/coverage mismatch, stale digest, missing pins or unresolved identity mapping: preserve both sides, refuse a regression verdict; malformed baseline remains invalid. |
| `LEK-CONTEXT-006` / `context.budget-exceeded` | Complete required estimate exceeds effective content budget (or explicitly labeled proven lower bound exceeds it); warning/advisory by default. Data includes subject, requiredModules, estimate basis, required tokens, configured/effective budgets and breakdown reference. |
| `LEK-CONTEXT-007` / `context.baseline-regression` | Comparable metric exceeds an explicitly configured absolute/relative regression allowance; emits signed delta and contributing changes, advisory by default. |
| `LEK-CONTEXT-008` / `context.policy-denied` | Selected mandatory project policy fails budget, configured regression or required completeness: denied/3; wraps reasons without suppressing report. |

Human rendering of #75's synthetic example: `LEK-CONTEXT-006 planner.sync_external_objects requires 23 modules; estimated minimum semantic context 41,200 tokens (chars-4, uncalibrated); available content budget 12,000; over by 29,200.` The numeric example is not a fixture measurement. A 23-module count by itself never triggers a universal limit.

Registry severity and command status are different: existing `ValidationProfile` only permits lowering non-error severities, so mandatory mode must not attempt to elevate a warning through that override mechanism. Use `context.policy-denied` and explicit gate state. Register allowed statuses consistently, and test the active registry through Rust plus an updated Node gate.

## Profile format

Budget profiles are closed JSON documents loaded from an explicit caller-selected committed file outside reserved `lekalo/**` and `.lekalo/**`, for example `config/context-budgets.json`; these paths are examples of user-toolchain configuration, not newly accepted Lekalo canonical homes. Use safe project-relative input reading. Do not add generic profile discovery or new Model keys. A future canonical budget-policy home requires a structure/ownership successor; this feature need not claim one to provide reproducible explicit CLI inputs.

Illustrative document (same placeholder-version convention as the report):

```json
{
  "schemaVersion": "lekalo/context-budget-profile/v<V>",
  "identity": "dev.lekalo.context-budget-profile@<V>",
  "profiles": [
    {
      "id": "local-12k",
      "version": "1",
      "estimator": {
        "id": "dev.lekalo.estimator.chars-4",
        "version": "0.2.16",
        "specDigest": "sha256:<digest>",
        "model": { "state": "unknown" },
        "calibration": { "state": "unknown" }
      },
      "budget": {
        "contextWindowTokens": 16384,
        "reservedOutputTokens": 2048,
        "reservedSystemToolTokens": 2336,
        "framingTokens": 0,
        "marginNumerator": 1,
        "marginDenominator": 1
      },
      "selection": {
        "version": "required-semantic-facts/1",
        "sourceContext": "none",
        "target": { "state": "unknown" }
      },
      "limits": {
        "maxDepth": 256,
        "maxNodes": 50000,
        "maxEdges": 250000,
        "maxFacts": 50000,
        "maxReportBytes": 33554432,
        "maxSubjects": 10000,
        "maxWorkUnits": 10000000,
        "maxSourceFiles": 2000,
        "maxSourceFileBytes": 16777216,
        "maxSourceTotalBytes": 67108864
      }
    }
  ]
}
```

`availableContentTokens = contextWindowTokens - reservedOutputTokens - reservedSystemToolTokens = 12000`; reject nonpositive result/overflow. `minimumSafeContextEstimate = ceil(minimumRequiredSemanticTokens * marginNumerator / marginDenominator) + framingTokens`; retain unadjusted metric separately. If modeling minimum total window, add both reservations and label that field `minimumWindowEstimate`. Neither reservations nor margin/framing are counted twice. Zero framing and a 1/1 margin are an explicitly optimistic structural measurement, not empirical safety calibration. Profile numbers are user choices, not vendor model limits. Named-model profiles carry exact model identity/revision and explicit approximation/calibration provenance; target profile selection only filters bound artifacts and is pinned separately from tokenizer identity.

Profiles are non-inheriting in v1, unique and canonically sorted; the effective canonical bytes are digest-bound, including limits/selection, and every report embeds that digest. Optional CLI source selection produces a separately fingerprinted effective profile; a selected mandatory policy can prohibit such overrides. Different declared document ordering gives the same normalized profile or a closed validation error, never different metric values.

Separate closed policy example: `{schemaVersion, identity, profileRef:{id,version,digest}, mode:"mandatory", failOn:["over-budget","required-incomplete","baseline-regression"], regressionLimits:[{metric:"minimumRequiredSemanticTokens",absoluteIncrease:1000,relativeIncrease:{numerator:1,denominator:10}}]}`. Specify threshold semantics: regression if **either** configured allowance is exceeded; equality passes; skip relative allowance at zero baseline with explicit reason and still evaluate absolute allowance. Policy can require a baseline and permitted source recipe; omission then denies rather than disables the gate. Default without a policy is advisory; CI must explicitly select the checked-in project policy and pinned profile. Policy selection and digest are visible, so an advisory local invocation cannot be represented as passing the mandatory QA policy. There is no default universal module/token threshold, automatic baseline replacement, or requirement to execute suggested refactors.

## Test plan mapping each acceptance criterion

All tests below are implementation work, not tests claimed to exist or run in this research. Proposed new fixture family `tests/fixtures/context-budget/` must be declared synthetic in `tests/fixtures/fixture-provenance.json`. Keep source/contract fixtures hermetic; no private brownfield code or live model call is needed for structural acceptance.

| Issue acceptance criterion | Implementation locus and evidence to deliver |
| --- | --- |
| **AC1 deterministic computation per model/profile version** | Core `closure/facts/metrics/estimate/canonical` and shared context collector; core tests `same_pins_same_bytes`, `permuted_inputs_same_bytes`, `changed_profile_not_comparable`, `bounded_cycles_and_work`, Unicode and checked-overflow vectors. Cross-platform CI golden bytes. Digest binds all consumed evidence, not only Model version. |
| **AC2 over-budget symbols have explainable dependency breakdown** | `metrics.rs`, `diagnostic.rs`, `LEK-CONTEXT-006`, CLI human/JSON rendering. Synthetic 23-module/high-cost fixture with independent expected ledger arithmetic, witness paths, shared-cost reconciliation and stable top contributors; at exact budget boundary passes, one token below reports over-budget. Model the 41,200 example only if a fixture actually computes it, never hardcode a diagnosis to match the issue prose. |
| **AC3 suggestions advisory unless project policy makes gate mandatory** | `suggest.rs`, `policy.rs`, CLI `DomainResult` mapping. `advisory_reports_exit_zero`, `mandatory_policy_denies`, `unknown_required_evidence_denies`, `policy_cannot_be_weakened_by_override`, `suggestions_never_write`; policy failure preserves report. No native command or refactoring execution. |
| **AC4 minimum required semantic facts separate from optional source** | `facts.rs`, `simulate.rs`, selected artifact adapter and report schema. Golden F/O/S ledgers; privacy canaries absent; optional code growth cannot change F; required type/error/policy/transaction attachment missing makes minimum-safe unknown. `legacy_capsule_unchanged` retains existing context goldens; tiny-budget simulation exposes excluded mandatory facts. |
| **AC5 baseline regression visible in semantic diff/QA** | `compare.rs`, diff opt-in assessment envelope, CLI tests and `scripts/test-context-budget-cli.mjs`. Add-edge/dependency regression, accepted rename vs ambiguous rename, removal, zero baseline, changed estimator, incomplete evidence, equal semantic diff with longer docs/source, policy threshold crossing and denied QA exit; immutable baseline bytes checked before/after. |
| **AC6 planner reference compared with broader integration module** | Fixture comparison below under identical profiles; core/CLI golden reports plus `tests/fixtures/context-budget/comparison.md` (future artifact) tabulating all M1-M10 and unknown states. Retain a narrower/equal result if obtained; no AI-understanding claim from structural metrics. |
| **AC7 output usable by AIFHub Framework Lift evaluation** | Closed report/comparison schemas, `context_budget/history.rs`, intentional run-history successor when full metrics are recorded, contract and consumer-import tests. Round-trip generic #100 harness fixtures for both contracted and observed scopes; exact metric/model/profile/estimator provenance, actual tokens kept separate, independent hard assertions, complete/partial/unknown classifications. Future #100/#102 evaluation/export work stays explicit. |

Capability coverage: project/module/symbol selection -> CLI scope and union tests; configurable budgets and target/model tokenizer profiles -> closed profile/digest/unknown-tokenizer tests; trend/baseline -> AC5; extraction suggestions -> AC3; capsule simulation -> AC4; no universal thresholds -> no default budget, uncalibrated labels and AC6/AC7 evidence limits. The metric table maps every issue metric to its source, implementation seam and named test.

### Planner versus integration fixture

Use `tests/fixtures/context/planner/` and `planner.focus_task` as the small declared reference, with `tests/fixtures/reference-evaluation/model/` and `tests/fixtures/scenario/valid/planner-switch-focus.json` as attachment/behavior references where compatible. The existing pinned `tests/fixtures/context/golden/planner.context.json` says budget 5,000, estimated/minimumRequired 262; this is an **existing depth-two legacy capsule golden**, not a new measured minimum-safe result.

Broader integration source evidence already exists in synthetic `tests/fixtures/pilot/taskhub/packages/sync/src/index.ts`: `syncEvent` and `toPayload` join tasks, events and integration payload/client contracts across packages. `packages/integrations/src/index.ts` is deliberately a narrow interface, so its name alone does not make it the large module. Use the sync flow as the broad workload with `taskhub-scans/taskhub.scan.v0_2_16.json` where its symbol bindings cover the selection; missing scan coverage remains explicit. Do not pretend TypeScript imports are canonical #13 edges.

Create paired **new synthetic** declared fixtures under `context-budget/planner` and `context-budget/integration`: planner's small task command and an `integration.sync_external_objects` model whose explicit supported references connect task, event, delivery, authorization and retry contracts across modules. Add fresh synthetic source/target mappings and ownership/size evidence so all ten metrics are exercisable. Preserve provenance explaining this normalization; do not promote the observed Taskhub model or modify its source as part of measuring it. Run both at identical 4k/12k/48k illustrative budgets and same estimator, both semantic-only and opt-in mapped-source recipes; publish counts, required/supporting/source costs, completeness, simulation coverage and largest/shared artifacts. Compare module and symbol rows, not just project totals. The test asserts the fixture's designed graph/cost differences and ledger arithmetic, not general superiority of semantic modeling.

### Verification commands for implementation

Existing regression gates to retain: `cargo test -p lekalo-core --test context --test graph --test diagnostics --test diff`, `cargo test -p lekalo-cli --test context --test diff --test history`, `node scripts/test-context-contracts.mjs`, `node scripts/test-semantic-diff-contracts.mjs`, `node scripts/test-run-history-contracts.mjs`, `node scripts/test-diagnostic-contracts.mjs` (expanded active-registry coverage). Add `cargo test -p lekalo-core --test context_budget`, `cargo test -p lekalo-cli --test context_budget`, `node scripts/test-context-budget-contracts.mjs` and `node scripts/test-context-budget-cli.mjs` with schema-positive/negative goldens, adversarial duplicate keys/null states, closed nested objects, bounded paths/links, no network/process/write assertions, source canaries, canonical output and exit-envelope parity. Schema checks alone cannot prove semantic arithmetic; independently validate ledger sums and ratio/coverage relations in Rust and the Node gate.

Integration gates: `node scripts/check-contract-versions.mjs --base <implementation-base>`, `node scripts/test-fixture-provenance.mjs`, `node scripts/check-structure.mjs`, `node scripts/check-authority.mjs`, `node scripts/check-privacy.mjs`, then repository fmt/clippy/workspace tests and MSRV/platform checks required by CI. Do not silently update accepted authority/privacy digests to satisfy a gate. No expensive provider evaluation belongs to #75 structural tests; #100 owns randomized A/B repeats and uncertainty.

Research validation: live issue texts and the above checkout surfaces were read. Both fenced JSON illustrations parsed successfully; a structural check found all eight requested sections, ten metric rows and seven acceptance rows. `git diff --check`, `node scripts/check-contract-versions.mjs --base HEAD` (product 0.6.3, 95 contract artifacts), `node scripts/check-structure.mjs` and `node scripts/check-privacy.mjs` passed. Only this Markdown file is to be committed; no Rust implementation tests or new metric measurements were run or claimed.

## Risks

- **Meaning of “safe”:** deterministic semantic coverage is a useful lower-level measurement, not proof an AI understands an operation. chars-4 can under/overestimate model tokens, particularly code and non-Latin text. Keep estimated vs measured and structural vs empirical fields explicit; calibration and observed task outcomes belong to #100.
- **Closure correctness:** using the existing depth-two capsule or module import fan-out can respectively undercount or explode context. Required semantic references and incoming applicability must have typed, versioned selection rules; test frontier exhaustion and annotation direction explicitly.
- **Coverage gaps across model versions:** accepted later attachments contain facts the old context grammar cannot express. A supported Model version is insufficient unless applicable attachment coverage is known. Missing/detected evidence must not become an optimistic zero or an invented safety threshold.
- **Artifact facts are not universal:** source maps, observed candidates, generated manifests and declared bindings have different identities/freshness. File totals and generated ratios need a defined selected recipe and current evidence; reading a file does not prove it is semantically required.
- **Scale:** naive per-symbol closure/path enumeration can be quadratic or worse. Share indexes/components and provenance requests, bound total deterministic work, and mark incomplete results. A host time limit is not a deterministic algorithm limit.
- **Profile/policy confusion:** existing validation, target, impact, diff, storage and native-gate profiles are distinct. An explicit user-toolchain budget policy is enforceable only in invocations/CI that select it; declaring a new automatic canonical home requires an accepted structure contract.
- **Version skew:** active runtime registry and historical script fixture versions differ; all newly active contracts/consumers need matching tests. M6 schemas and value wrappers are closed and partly crate-private. No new fields may slip into old schemas or use null as unknown.
- **Trend honesty:** renamed symbols, changed estimators, missing baseline pins and removed history inputs can create misleading deltas. Keep incomparable/deleted/missing states; never rewrite the baseline or treat a missing prior row as zero.
- **Ownership/privacy:** a local report is not HLV-owned `metrics.evaluation-evidence`, nor an accepted public artifact. Preserve current policy refs, scoped history/delete semantics and default no-source/no-path output; no export or upload is added. Opaque digests are identifiers, not proof of anonymization.
- **Advisory extraction:** a smaller context may hide critical effects/authorization or transfer cost elsewhere. Include preserved facts and hypothetical assumptions; never auto-extract code or claim a smaller token count improved agent quality.

The implementation can proceed in reviewable stages: close schemas/profile/diagnostic identities; share and test fact collection; implement closure and all M1-M10 states; add explainable reports/simulation/policy; attach baseline comparison to diff/QA; integrate versioned history summaries and the #100 import fixture. Each stage keeps missing evidence explicit; completing only graph counts is not completion of #75.
