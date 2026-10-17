# Issue #93: semantic search/find research

Research only, 2026-10-06. Acceptance authority: [live issue #93](https://github.com/ichinya/lekalo/issues/93), retrieved with `gh issue view 93 --repo ichinya/lekalo --json title,body,labels`. Parent #1 and dependencies #13, #14, #22 and #42 were read for context; their examples do not override #93. Inspected checkout: `ichinya/m8-issue-93`, initial HEAD `15bd13917654dc20ad19b089a5bb8e5e29c2099f`, initially clean. The dispatch identifies the base as `ichinya/M8`; this report pins the actual local HEAD. Cargo and `docs/versioning.md` currently say product `0.6.5`; the requested M8 implementation targets product **0.8.0**. Research changes no product or contract version.

House shape: `docs/m7/issue-77-research.md`, `docs/m7/issue-84-research.md` and `docs/m7/issue-102-research.md`: current inventory, proposed command/report contracts, definitions/data sources, determinism, diagnostics, policy/profile, acceptance/fixtures/gates, risks and delivery boundary. Only this document and its local commit are authorized; implementation is a subsequent task.

**Decision (proposal):** add one typed, offline search service with two entry points: `find` applies exact filters; `search` combines those filters with deterministic lexical retrieval, optionally fuzzy. Build a local, disposable index from canonical semantic data and explicitly admitted target/trace/effect evidence. Return bounded selection cards with provenance, coverage and explanations before an agent opens a file. Query execution never launches an adapter, performs a repository text scan, promotes a binding or chooses an ambiguous candidate.

**Proposal convention:** all paths, flags, APIs, schema fields, diagnostic allocations, limits and gates in sections headed "Proposed" are future work, even when repeated without a label. New contracts use `0.8.0`, the product version of the implementing commit; unchanged upstream contracts keep their exact accepted identities and bytes. Examples are proposed grammar or synthetic illustrations, not current successful CLI runs.

## Existing-surface inventory

The following facts come from source/contract inspection at the pinned HEAD, not runtime acceptance.

| Surface | Current owner / searchable anchors | Implication for #93 |
| --- | --- | --- |
| CLI | `crates/lekalo-cli/src/main.rs`: `Cli`, `Commands`, `run_graph`, `run_effects`, `run_requirements`, `run_bindings`; global `--json` and `--no-cache`, per-command `--project`. No `Search` or `Find` command. | Add dedicated dispatch and rendering; preserve envelope/exit ownership. |
| Typed semantic definitions | `crates/lekalo-core/src/ir/mod.rs`: `CompiledProject`, `Common`, `Text`, `Definition`, `DefinitionKind`, `CommandDef`, `QueryDef`, `SourceMap`. Definitions carry descriptions, typed fields and kinds. | Index definitions and members from IR; `command` is an operation subkind, not a graph node kind. Descriptions must be indexed independently of semantic digests that omit documentation. |
| Dependency adjacency | `crates/lekalo-core/src/graph/mod.rs`: `DependencyGraph::{nodes,edges,resolve_id,incoming,outgoing,reverse_dependencies}`; `graph/build.rs::build`. | Reuse reverse indexes. Do not reconstruct relationships by searching YAML/source strings. |
| Relation meaning | `crates/lekalo-core/src/graph/model.rs`: `RelationKindId`, `ReferenceRole::{EndpointInvokes,ScenarioCovers,CommandInput,QueryReturns}`. Twelve core relations; `writes`, `implements`, `verifies` are registered but not emitted by the base canonical builder. | Registered vocabulary is not evidence of populated relations. Generic `references` and scenario coverage cannot be labeled function calls. |
| Effect reverse queries | `crates/lekalo-core/src/effects/mod.rs`: `EffectGraph::{readers,writers,declared,detected,attach_detected}`, `SubjectSelector`; `effects/{edge,kind,provenance,index}.rs`. | Reuse resource/field selectors and retained origin/trust/confidence. Exact field versus entity-wide overlap must remain distinguishable. Detected effects need an admitted envelope. |
| Neutral trace | `crates/lekalo-core/src/trace/mod.rs`: `TraceManifest::{parse,relations,query,manifest}`; `trace/relation.rs`; `contracts/trace-manifest.schema.v0.2.16.json`. | Requirement-symbol-binding-scenario-gate join is a separate evidence graph. Preserve original IDs, relation kinds, partial links and revision/digest pins. |
| Requirements | `crates/lekalo-core/src/requirements/mod.rs`: `RequirementsAttachment::{parse,resolve}`; `requirements/report.rs`: `Report`, `RequirementRow`, `ReferenceRow`, coverage gaps. | Resolve provider input only during explicit index construction/update. Query saved typed rows offline; missing/stale/conflicting references are not fresh coverage. |
| Native bindings | `crates/lekalo-core/src/observed/{index,types,view,scan_service}.rs`; `docs/bindings.md`; `main.rs::run_bindings`. `ObservedIndex` stores locations, stable keys, signatures, provenance, candidates and freshness. | Consume recorded evidence, with explicit/user-confirmed priority and all inferred candidates retained. Do not run scan/audit as an invisible search side effect. |
| Observed selection cards | `observed/view.rs`: `ObservedCard`, `record_completeness`, `Completeness::{Complete,Incomplete,Unknown}`. | Reuse completeness conventions; a fingerprint at a recorded revision does not prove today's native file bytes unchanged. |
| Ownership, profiles and classification | `crates/lekalo-core/src/artifacts/types.rs::ArtifactManifest`, `crates/lekalo-core/src/{observed,contracted,implementation,classification,target_profile,architecture_profile}`; `classification/{resolve,types}.rs`; `contracts/authority-matrix.v0.3.2.json`. | Keep artifact ownership/mode evidence and observed status distinct; profile/classification metadata needs its actual producer, never guesses from directory names. |
| Diagnostic/waiver inputs | `crates/lekalo-core/src/diagnostics/{registry,types,normalize}.rs`; `crates/lekalo-core/src/ai_lint/{input,wire}.rs`. | Search admitted diagnostic instances and qualified waiver entries. Registry rules alone are not project findings, and waivers are not evidence that a violation vanished. |
| Cache | `crates/lekalo-core/src/cache/{record,key,pipeline,sqlite,path}.rs`; `docs/cache.md`. Existing `PayloadValue` is a digest plus bounded counts; records have typed dependencies. | Current cache is not a materialized search database. Do not put cards into the closed payload or reuse a graph-fragment key with different meaning. |
| Cache custody | Authority kind `lekalo.cache` permits `.lekalo/cache/**`; `cache clear --yes` owns disposable cache files, except migrations. Cache documentation prohibits source text/raw documents/credentials/transcripts. | A separate search index can use this home, but persistence of approved description metadata needs an explicit documented policy decision; raw snippets stay excluded. Preserve migration custody. |
| Confined reads | `crates/lekalo-core/src/project_fs.rs`: path/selection validation and confined bounded filesystem reads; `crates/lekalo-core/src/privacy`. | Apply current path/classification policy before ingesting or returning target evidence, and again when serving a stored snapshot. Relative paths alone do not make data safe. |
| Result and registries | `crates/lekalo-core/src/result.rs::DomainResult`: valid 0/stdout, invalid 1/stderr, denied 3/stdout, unsupported/unavailable 4/stdout, unsupported-version 5/stderr. `diagnostics/registry.rs` embeds base `0.6.4` plus successor `0.6.5`. | Preserve status mapping. Registry integration must retain both accepted sets and consumer pins rather than copying an older research registry. |
| Existing fixtures | `tests/fixtures/{graph,effects,inspect}/planner/`, `tests/fixtures/trace/full.trace.json`, `tests/fixtures/{requirements,bindings}/`, `tests/fixtures/pilot/taskhub/`. | Useful synthetic/source-backed seeds; independent fixture digests do not authorize joining them unchanged. The issue's `user_task_planning.focused_at` example needs an explicit fixture definition. |
| Gate owners | `scripts/test-{graph,effect-graph,requirements,trace,observed,bindings,cache,diagnostic}-contracts.mjs`, `test-fixture-provenance.mjs`; `.github/workflows/ci.yml`. | Preserve schema and real-binary checks, synthetic provenance and suite coverage. Binary-dependent gates belong after the same job's binary build. |
| Version rule | `docs/versioning.md`, `scripts/check-contract-versions.mjs`. Changed/new contracts must match product version; unchanged contracts remain frozen. | M8 search contracts and changed registry/config contracts use `0.8.0`; the research commit remains documentation only at product `0.6.5`. |

## Proposed command/report schema

### Command grammar and ownership

Keep every command shown in the issue valid without requiring an attachment argument:

```sh
lekalo search "focus task"
lekalo find --kind command --module planner
lekalo find --writes planner.user_task_planning.focused_at
lekalo find --calls planner.focus_task
lekalo find --requirement PLANNER-REQ-001
lekalo find --binding-path apps/api/src/planner
```

Additional proposed exact grammar:

```sh
lekalo --json search "focus task" --project . --limit 20
lekalo --json search planner.focus_task --fuzzy --module planner
lekalo --json find --id planner.focus_task
lekalo --json find --reads planner.user_task_planning.focused_at --effect-scope overlapping
lekalo --json find --requirement PLANNER-REQ-001 --requirement-source openspec --target node-typescript
lekalo --json find --native-symbol apps/api/src/planner/focus-task.ts#focusTask
lekalo --json find --type planner.task_id --input-type planner.task_id --output-type planner.task
lekalo --json find --error planner.task_not_found --relation verifies --related-to planner.focus_task
lekalo --json find --ownership observed --portability portable --profile api --diagnostic semantic.type-ref-unresolved
lekalo --json find --waiver lint-focus-debt --include-stale
lekalo --json find --kind scenario --limit 20 --cursor CURSOR
lekalo --json search-index build --project . --inputs config/search-inputs.json
lekalo --json search-index update --project . --inputs config/search-inputs.json
lekalo --json search-index status --project .
lekalo --no-cache --json find --kind command --project . --inputs config/search-inputs.json
```

`search` requires exactly one nonempty positional query; `find` requires at least one filter. Both accept `--project DIR`, `--inputs FILE`, `--limit N` (default 20, 1..100), `--cursor TOKEN`, and the filters above. `search` alone accepts `--fuzzy`. Repeated values for one flag are OR; different flags are AND, evaluated before lexical matching/ranking. `--related-to` and `--relation` occur together and select direct incident relations with direction recorded. `--effect-scope exact|overlapping` defaults to exact and is valid only with reads/writes. No regex, shell/glob interpretation, SQL expressions or implicit prefix ID match. `--binding-path` is an exact relative path or a directory prefix at component boundaries: `planner` does not match `planner-old`. Definition/member IDs resolve through typed identities, not a last-dot split that bypasses entity/field validation.

`--requirement-source` disambiguates external IDs reused by multiple providers. Without it, return all visible qualified candidates and `selection:"ambiguous"`; never take the first. `--include-stale` adds visibly stale rows, never excluded/sensitive rows. Freshness unknown at query time remains unknown rather than being asserted current. `--inputs` is optional: use the project's standard saved registry plus an already registered input manifest, or report unsupported/missing attachment coverage. A missing optional producer does not make the six issue commands syntactically invalid or silently produce a complete empty answer.

CLI owns confined reads, input admission, index persistence and envelope rendering; proposed `lekalo_core::search` owns pure typed projection/query/ranking. Add `crates/lekalo-cli/src/search.rs` and `crates/lekalo-core/src/search/{mod,input,projection,index,query,rank,cursor,wire,diagnostic,version}.rs`. Inputs are immutable typed IR, graph/effects and producer snapshots; no network/tool execution occurs in core. Explicit `search-index build/update` may read canonical Model and declared provider documents and saved evidence; target scanning remains the existing explicit `scan` command. A cold query may construct the same transient index from these bounded inputs and cache it, but cannot walk target source. `--no-cache` bypasses every index/cache read/write/lock and constructs transient cards; with `--cursor` it is invalid because the cursor requires an immutable persisted generation.

### Contract artifacts

| Proposed artifact | Identity / discriminator and purpose |
| --- | --- |
| `contracts/search-query.schema.v0.8.0.json` | `dev.lekalo.search-query@0.8.0`, `lekalo/search-query/v0.8.0`; closed normalized request/filter/cursor grammar shared by CLI and in-process consumers. |
| `contracts/search-result.schema.v0.8.0.json` | `dev.lekalo.search-result@0.8.0`, `lekalo/search-result/v0.8.0`; cards, ranking explanations, selection/coverage and page state. |
| `contracts/search-index.schema.v0.8.0.json` | `dev.lekalo.search-index@0.8.0`, `lekalo/search-index/v0.8.0`; canonical snapshot manifest, typed card/posting blocks, exact input/recipe/security pins, digest and limits. |
| `contracts/search-inputs.schema.v0.8.0.json` | `dev.lekalo.search-inputs@0.8.0`, `lekalo/search-inputs/v0.8.0`; project-scoped closed list of producer kind, relative path and exact contract/digest expectations; no caller-selected cache home. |
| `contracts/diagnostic-registry.v0.8.0.json` | Reviewed union successor adding the codes below, retaining accepted IDs and lifecycle information. Update schema only if its closed version constraints require a successor. |
| Dependent registry-pinned contracts | Updated validation profiles/reports and other consumers only where their actual registry pin/wire changes require it; publish changed artifacts at `0.8.0` and update embedded references/gates. |

The input manifest accepts only explicitly supported typed producers: requirements attachment/resolved report, neutral trace, observed index, detected-effect envelopes, error-contract attachments, artifact ownership/implementation evidence, target/architecture-profile snapshots, diagnostic sets and qualified AI-lint waivers. Reuse their published decoders and validate project/revision/Model/IR/source pins before joining; a generic arbitrary JSON row is never searchable evidence. An attachment whose schema lacks a complete binding requires a validated adapter projection with that gap explicitly reported. Add exact family support registration where the existing version registry owns it, and explicit pins elsewhere. Do not change Model, IR, dependency graph, effects, trace, observed, target protocol, lock or diagnostic wire shape solely to publish search.

Descriptions are bounded typed Model metadata, not native code snippets. Store only privacy-admitted descriptive metadata. Review/document whether this is permitted under the existing cache policy; if it requires a policy/authority contract change, publish that reviewed successor at `0.8.0` with digest/pin updates before enabling persistence. Do not silently broaden cache admission or weaken producer privacy restrictions. Persist no raw source, file bodies, prompt/tool output or hidden lexical tokens.

### Result and selection cards

Return a typed search payload through `DomainResult`, preserving normal diagnostics and reason-code normalization. Close all nested objects and cap request/input/output bytes before allocation. Report fields: contract identity, project ID, revision and exact input contract/digest refs, recipe `semantic-search/1`, normalized query, index digest, `selection`, `coverage`, `items`, and `page`. `selection` is `none|unique|multiple|ambiguous`; `unique` means one returned qualified identity within complete relevant coverage, not a runtime binding confirmation. Truncated/partial coverage cannot prove uniqueness. A page containing one row is not a singleton answer.

Each card carries stable qualified `key`, semantic/external/native ID as applicable, kind/subkind/module, privacy-admitted name/description, compact input/output/error refs, ownership/portability/profile, source locations (bounded relative path/range), native bindings, requirement/scenario/gate refs, effect/relation witnesses, producer/provenance/confidence, freshness and completeness. Keep evidence origin/status per relationship, not just one overall confidence label. Bound description to 512 Unicode scalar values and each preview list to eight refs; retain explicit omitted counts and a stable ID for `inspect` rather than silently dropping facts. No default source snippets.

`match` carries matched fields, exactness tier, token/edit facts and witness refs. `page` carries limit, returned count, visible total (exact only for a fully computed visible set), next cursor and explicit truncation/work-limit state. `coverage` is per producer/domain: `complete|partial|unknown|unsupported|withheld`; unknown is never zero. No hidden paths/IDs/descriptions appear in diagnostics, counts, cursor data, ranking explanation or cache status. If policy forbids even revealing a withheld domain, expose only a generic incomplete-coverage statement.

## Proposed index definitions with data sources

| Issue index family (all required) | Typed projection and exact filter semantics |
| --- | --- |
| semantic IDs/names/descriptions | IR qualified ID map and approved metadata postings. `--id` is exact; search checks byte-exact IDs before normalized names/terms. Renames/tombstones are labeled history, never silently rewritten to a live ID. |
| definition kind/module | IR definition subkind and canonical module membership. `--kind command` matches commands, not all operation nodes; include scenario/binding/requirement/gate/diagnostic/waiver card kinds from their owners. |
| types/input/output/errors | Typed definition signatures and admitted error-contract facts. `--type` selects direct type use; input/output/error filters select their corresponding role, with bounded witness refs. Missing error attachment coverage remains unknown. |
| dependency/effect edges | Indexed graph incoming/outgoing plus effect field/resource keys. `--reads/--writes` return operations and exact effect witnesses; overlapping entity-wide effects are labeled possible field overlap. Separate declared and detected rows and trust. |
| requirements/scenarios/gates | Requirements resolution + trace relation indexes + typed scenarios and recorded native gates. Qualified requirement identities preserve provider IDs. Join within the search view; do not insert speculative edges into canonical graph. |
| target/native symbols/paths | Saved observed binding index and trace/artifact refs. `--binding-path`, `--native-symbol`, `--target` select recorded evidence; preserve every candidate, stable key, source, signature, profile, revision and freshness. |
| ownership/portability/profile | Typed ownership/classification/IR and admitted profile records. No ownership inference from generated filename conventions; no claim of profile compliance solely from its name. |
| diagnostics/waivers | Admitted current or explicitly stale findings and qualified waiver refs. `--diagnostic` uses exact registered rule ID; `--waiver` uses qualified waiver ID. Suppression is metadata alongside the original finding; no erased failure. |

`--calls planner.focus_task` returns direct declared invocation witnesses (`ReferenceRole::EndpointInvokes`) and additional invocation facts only when an admitted producer supplies actual call semantics. It excludes scenario coverage, type use, policy applicability and generic references. The baseline graph has no general operation/native call relation; report native/operation-call coverage unsupported or partial. Do not treat external-call effects as calls to a same-named semantic operation. Full native call graph acquisition belongs to adapter/context work, not hidden repository scanning in #93. This makes the issue's command useful with explicit limitations and no fabricated exhaustiveness.

## Proposed determinism strategy

1. Apply admission/privacy and exact filters first; build lexical postings only for visible admitted metadata. Byte-exact full IDs outrank every lexical/fuzzy result, unless an explicit filter excludes that ID. `find` never fuzzes a selector.
2. Freeze normalization in the recipe: Unicode NFC, ASCII case folding, splitting whitespace and ID separators (`.`, `_`, `-`, `/`, `#`). Preserve original ID case/bytes for exact identity. No locale-dependent collation; non-ASCII case folding is outside recipe 1. Query length 1..512 scalars, at most 32 terms.
3. Rank by fixed tuple: exact full ID; exact qualified native/external ID; exact normalized name; all exact lexical terms; prefix tokens; optional fuzzy tokens. Within a tier, matched-field priority (ID/name/description), greater exact term coverage, fewer total edit operations, then qualified key in UTF-8 byte order. Use integer facts rather than opaque floating similarity; return the tuple explanation.
4. Fuzzy mode is opt-in and labeled `lexical-fuzzy`, bounded edit distance at most two, at most 10,000 candidate keys before scoring, and a fixed work cap. A cap hit returns partial/truncated coverage and cannot silently drop the exact-ID tier. No embeddings, network model, vector store or RAG dependency.
5. Semantic ties before the final key tie-break set `ambiguous` and retain all visible tied candidate IDs, even across pages. Sorting only provides stable presentation. Search never turns this order into a confirmed binding or singular selected item.
6. Cursor binds normalized query/filter digest, recipe, security policy digest, immutable index content digest and last full ranking tuple. Reject query/snapshot/policy mismatch rather than paging into a different generation. Cursor is opaque bounded encoded data without paths/snippets; it grants no read authority. Concatenated pages equal the full ranked visible set with no repeats/skips.

Cap index at 100,000 cards, 1,000,000 posting/relation entries, 32 MiB canonical payload and 4 MiB response, with producer-specific bounds still enforced. These are proposed starting engineering limits, not measured consumer adequacy. Bound counts before allocation; work-limit failure preserves honest coverage. Deterministic bytes exclude timestamps, elapsed times, SQLite row IDs, installation paths and cache-hit telemetry. Benchmark telemetry belongs in a separate artifact.

## Proposed invalidation and cache lifecycle

Store an independent index under `.lekalo/cache/search/`, using immutable digest-named generations and atomic publication of a bounded current manifest. Canonical typed index bytes use sorted keys/set members, compact UTF-8 JSON plus LF and SHA-256; derived postings are validated against their canonical cards and input manifest. This is a separate search-index contract, not a new member smuggled into frozen cache records. `cache clear --yes` must cover it and retain `.lekalo/cache/migrations/**` exactly as before; cache health integration is explicit and may require a versioned cache contract successor if its record vocabulary changes.

Keys bind recipe/tokenizer, full approved metadata digest (including descriptions), semantic IR/graph/effect digests, all consumed evidence contract/digest/revision pins and classification/exclusion policy. Use content identity, never mtime alone. Incremental update recomputes changed producer/card partitions and downstream relation joins; unchanged partitions are reused. Definition deletion, binding confirmation/removal, description-only edits, exclusions, profile/waiver changes and diagnostics updates invalidate the appropriate postings. Publish one coherent generation; interrupted build cannot make mixed-revision rows visible.

Queries check canonical Model/input-manifest digest drift through bounded admitted reads, without enumerating target source. Stored native fingerprints prove only the recorded scan/audit revision; disclose `as-recorded` plus query-time native freshness `unknown` when not revalidated. Native refresh is explicit scan/audit followed by index update. A changed recorded input or policy cannot yield a fresh old snapshot; rebuild from admitted metadata or refuse with `search.index-stale`. Existing freshness `stale` stays stale.

Missing/corrupt/locked derived storage safely falls back to transient construction from metadata, with no target scan. If required metadata cannot be read, emit the correct producer diagnostic/coverage, never stale data disguised as a current answer. Read-only `search-index status` does not repair; `build/update` atomically replace only cache-owned data. A query requiring a pinned missing cursor generation fails rather than silently rebuilding its page. Recheck path/privacy policy during every query so policy tightening cannot leak old postings or old native candidate paths.

## Proposed diagnostics families and codes

Candidate allocations were searched in both embedded registry files (`0.6.4` and `0.6.5`) and had no collision at the inspected HEAD. They are proposals, not reservations; recheck the actual M8 union before implementation.

| Proposed ID / code | Trigger and status | Data / action |
| --- | --- | --- |
| `search.query-invalid` / `LEK-SRCH-001` | Closed request/member/selector invariant failure; error, invalid. | Bounded member/reason; supply exact legal filters. CLI syntax failures still use `cli.usage`. |
| `search.version-unsupported` / `LEK-SRCH-002` | Unsupported search query/index/input/recipe contract; error, unsupported-version. | Safe contract/version refs; no aliases or guessed decoder. |
| `search.input-invalid` / `LEK-SRCH-003` | Malformed, duplicate-key or mixed project/revision/digest input; error, invalid. | Producer kind/ref and safe reason; rebuild an exactly bound input set. Preserve specific upstream parse diagnostics where applicable. |
| `search.index-stale` / `LEK-SRCH-004` | Required index input/security pins no longer match; warning, valid/denied. | Safe mismatch kind, no hidden subject; valid only after correct transient rebuild, otherwise denied with explicit update action. |
| `search.cursor-invalid` / `LEK-SRCH-005` | Malformed/oversize cursor or query/generation/security mismatch; error, invalid. | Safe reason; restart paging with the current snapshot. |
| `search.result-limit` / `LEK-SRCH-006` | Work/index/output cap prevents a complete answer; warning, valid/denied. | Limit kind and visible coverage; narrow filters or rebuild supported index. Ordinary pagination is not a limit failure. |
| `search.coverage-incomplete` / `LEK-SRCH-007` | Needed producer/domain missing/stale/partial/unsupported; warning, valid. | Allowlisted domain/state only; obtain explicit evidence before interpreting absence as proof. |
| `search.ambiguous` / `LEK-SRCH-008` | Multiple equally supported qualified interpretations; info, valid. | Visible bounded candidate refs/count; refine kind/module/provider. Retain candidate set without a selected winner. |

Zero matches is valid, not an error or ambiguous query. Structural confinement and privacy denial retain their existing families; no new diagnostic echoes a secret query/path. Register category, severity/status allowlists, messages, location policy and closed data fields. Update embedded registry refs, validation profile pins and diagnostic suite/catalog coverage using the union of current families. Keep `diagnostic.schema.v0.2.16.json` frozen unless its actual wire changes. Provider operation vocabulary/discovery can advertise search only through a reviewed `0.8.0` provider successor with exact search schema refs; provider/MCP/AIFHub transport integration is not required for local #93 acceptance.

## Proposed profile format and policy

No separate search relevance profile in the first increment: the versioned recipe fixes ranking and bounds, CLI flags fix exact scope, and existing privacy/exclusion/ownership policy controls admission. Architecture/target profiles are searchable facts, not caller permission to widen scope. `search-inputs` identifies typed producer inputs and pins; it cannot override exclusions, change classification or confer source read authority.

Explicitly review approved Model descriptions versus current cache text restrictions before persistence. When description metadata is withheld, exact-ID and permitted typed search still work with withheld coverage. Never create an `--include-sensitive` or private override to meet benchmark recall. Local JSON results remain subject to current classified output rules; printing a search card does not authorize public export or source upload.

## Proposed test plan mapping each acceptance criterion

The first column below reproduces all seven #93 acceptance criteria verbatim. Tests and fixtures are proposals; none were executed as implementation acceptance in this research.

| Issue acceptance criterion (verbatim) | Proposed implementation and observable evidence |
| --- | --- |
| Agent can locate writers/readers of a field without grep. | AC1: `tests/fixtures/search/effects/` declares `planner.user_task_planning.focused_at`, two exact writers, an exact reader, unrelated-field writer and entity-wide mutation. Assert exact/overlapping modes independently, declared/detected trust and missing coverage; real `find --writes/--reads` matches hand-authored ID/witness sets with no `rg` subprocess or native scan. |
| Exact ID search never loses to fuzzy result. | AC2: `search/ranking/` contains an exact `planner.focus_task` plus attractive lexical/fuzzy distractors. Exact ID ranks first on every page size/permutation in default and fuzzy modes; filter-excluded exact IDs are explained by scope. Test description-only invalidation, Unicode normalization, case-sensitive IDs and stable tie explanations. |
| Requirements, scenarios and target bindings searchable together. | AC3: `search/trace/` freshly rebinds a complete and partial requirement -> symbol -> binding -> scenario -> gate chain to one project/revision. One result set returns all relevant card kinds and typed witnesses. Duplicate external IDs across providers remain qualified/ambiguous; reject mixed digests, dangling refs and stale-confirmed laundering. |
| Results include enough metadata to choose item without reading every file. | AC4: independently assert cards contain kind/module/signature/errors, approved description, binding/effect/requirement refs, path/range, ownership/portability/profile and source/confidence/freshness. Include withheld/unsupported/omitted states and an independent selection-only consumer that chooses by card metadata with file-open count zero. No snippets required. |
| Ambiguous query does not silently pick one result. | AC5: `search/ambiguity/` covers equal lexical candidates, same-name symbols, multi-provider requirements and inferred native bindings. Report ambiguous plus all visible candidates over pagination; no implicit confirm, first-row selection or data mutation. Deterministic final key sorting does not erase pre-sort ambiguity. |
| Search works offline and without target source full scan after index build. | AC6: build with explicit saved scan/requirements/trace/effect inputs, then block adapter/network execution and deny target-source reads while preserving canonical metadata. Warm search/find/page succeed without target traversal/read; changed metadata/policy invalidates coherently. Missing/corrupt cache rebuild uses metadata only; native freshness stays as-recorded/unknown. Verify no-cache bypass, atomic update/crash, stale cursor and cache-clear migration custody. |
| Benchmarked against `rg` on representative task-oriented consumer questions. | AC7: `search/benchmark/` holds independently labeled planner, Taskhub observed and cross-module synthetic tasks: field writers/readers, exact operation, requirement/scenario/gate, binding prefix, input/error type, ambiguous name and diagnostic/waiver. Run both real binary and real `rg` on identical frozen local fixtures, record answer recall/precision, ambiguity honesty, selection correctness, latency, bytes/tokens and files opened; separate cold build, warm query and incremental update costs. Commit reproducible benchmark method/results with environment/tool/fixture pins, including negative cases. |

All ten issue requirements are mapped explicitly: exact filters first -> AC1/2 and filter intersection tests; optional labeled lexical fuzzy -> AC2; no vector/RAG -> dependency/offline AC6; explainable ambiguity/ranking -> AC2/5; deterministic order -> AC2/5 and repeated-byte tests; bounded results/pagination -> AC4/5/6; source/provenance/confidence -> AC1/3/4; sensitive/excluded paths hidden -> privacy fixture gate; incremental index/cache -> AC6; agent selection JSON -> AC4. The eight index-family rows above are mandatory, including diagnostics/waivers and ownership/profile facts, rather than deferred silently after a name-search MVP.

### Fixture families, provenance and benchmark design

Proposed `tests/fixtures/search/{model,effects,trace,bindings,ranking,ambiguity,privacy,invalid,cache,pagination,benchmark,golden}/`. Reuse planner shapes from graph/effect/inspect fixtures and the actual Taskhub source/index shape, recording which observations are scanner-derived versus authored. Add the issue's exact field identity explicitly; current example names alone are not fixture evidence. Rebind every copied attachment to the new project/Model/IR/revision before joining. Add declarations for input/output/errors, portability/ownership/profile, and current/stale finding/waiver cases. Record all families as synthetic in `tests/fixtures/fixture-provenance.json`, plus positive/negative pairs and rule coverage in `tests/fixtures/suite/v1/`.

Privacy fixtures plant canaries in excluded path names, native symbol names, signatures, descriptions, diagnostics, candidate lists and lexical tokens. Check raw serialized output, errors, cursor, count/ranking timing-independent facts, index bytes and status; no leakage via excluded postings or stale policy snapshots. Include traversal, absolute/UNC paths, links/reparse points, encoded separators, invalid Unicode, duplicate keys, oversized manifests/postings/cursors and policy revocation between pages.

Benchmark questions and relevant-answer sets are authored before running either tool. `rg` may scan the fixture and then require file inspection; account for that work and ambiguous matches rather than equating line hits with semantic answers. Use plain text search, e.g. `rg -n --fixed-strings -- "focused_at" FIXTURE`, plus any predeclared fair query variants needed to answer the task; record exact commands and required file opens. Run offline, repeat trials and report distribution/environment, not one favorable stopwatch value. No hard universal speedup is implied by #93; acceptance requires the reproducible comparison and correctness gates, with regression/negative results retained. Private consumer evaluation or model-assisted trials require separate authorization and are not needed to prepare the synthetic benchmark.

### Contract and runtime gate wiring

Proposed gates: `scripts/test-search-contracts.mjs` for closed schema/runtime admission parity and committed goldens; `scripts/test-search-cli.mjs` for real offline queries, no-target-read probes, pagination/privacy/cache lifecycle; `scripts/bench-search.mjs` for the reproducible `rg` task comparison. Exact Ajv `8.17.1`, Draft 2020-12 and existing `LEKALO_AJV_NODE_PATH`/`NODE_PATH` provisioning follow repository gate practice; no new library API choice is needed in research. Missing real binary, required fixture, `rg` benchmark binary or configured validator fails the relevant gate instead of succeeding via a skip.

Place schema-only checks in the existing contracts lane. Put live CLI checks after `cargo build --workspace --locked` in the same Linux/Windows/macOS build-test job. Use independent expected result/witness sets and canonical digests, not producer-generated expectations. Include repeated bytes, partition/order permutations, schema-valid but semantically wrong manifests, forged posting payloads, query filter intersections, exact-ID precedence and page equality. Keep benchmark timing out of deterministic goldens; deterministic benchmark questions/answer sets/commands remain versioned.

Proposed focused implementation verification:

```sh
cargo test -p lekalo-core --test search --locked
cargo test -p lekalo-cli --test search --locked
cargo build --workspace --locked
node scripts/test-search-contracts.mjs
node scripts/test-search-cli.mjs
node scripts/bench-search.mjs --fixtures tests/fixtures/search/benchmark --output reports/search-benchmark.json
node scripts/check-contract-versions.mjs --base IMPLEMENTATION_BASE
node scripts/test-fixture-provenance.mjs
```

Retain graph/effect/requirements/trace/observed/bindings/cache/diagnostic gates, fixture-suite normalization/determinism/coverage, structure/authority/privacy checks and repository fmt/clippy/MSRV/workspace CI. Search correctness includes cross-family joins and custody; a schema pass or one exact-name test is insufficient. Focused local acceptance does not claim hosted CI or production consumer acceptance.

## Risks and proposed delivery increments

| Increment | Concrete deliverable / exit criterion |
| --- | --- |
| 1. Contract and safety seam | Freeze query/result/index/inputs at `0.8.0`, ranking/coverage/cursor recipe, candidate codes, bounded projection and description-cache policy decision. Add independent exact-ID/filter/unsafe-input vectors. No new CLI flag hides an unsupported producer. |
| 2. Exact semantic/effect retrieval | Implement typed definition/signature/member and graph/effect indexes, `find`, non-fuzzy `search` and card rendering. Prove AC1/2 and baseline metadata part of AC4 with exact witness sets. |
| 3. Evidence joins and honest candidates | Add requirements/scenarios/gates, native binding/path, ownership/profile and diagnostic/waiver projections. Prove AC3/4/5, no false calls and policy filtering. Retain producer gaps instead of manufacturing coverage. |
| 4. Durable incremental index and paging | Atomic generations, content/policy invalidation, bounded postings, cursor equality, no-cache and corrupt-store handling. Prove AC6 with instrumented native-read/network/adapter prohibitions and crash/race cases. |
| 5. Optional fuzzy and full acceptance | Add bounded labeled fuzzy only after exact semantics; run all families, hostile gates, union-registry coverage and reproducible `rg` comparison AC7. Deliver user examples/benchmark report and merged CI evidence. |

Risks: attaching independently pinned evidence can produce false joins; use actual project/revision/digests. Operation/native call coverage is limited; do not invent edges. Description metadata changes independently of semantic IR identity and has cache privacy implications. Native state can change without a saved scan; distinguish historical evidence from query-time freshness. Entity-wide effects can overstate exact field writers; keep overlap explicit. Unicode/ranking recipe drift can reorder candidates/cursors; version it. Evidence-card expansion and fuzzy scoring can become unbounded; enforce input/work/output caps. Exclusion policy changes must invalidate old postings and active cursors. Shared registry/validation/provider pins can collide with parallel M8 work; merge unions and recheck on the final implementation base. Benchmark line hits, model-assisted answer selection and search latency measure different things; keep accuracy, build costs and source-open costs separate.

Non-goals: vector/RAG infrastructure, unrestricted repository full-text search, a new native scanner or call graph engine, automatic binding confirmation, Model/IR rewriting, target generation, native gate execution, code edits/refactors, public export, HLV/AIFHub writes, provider/MCP transport delivery, private consumer ingestion or a claim of universal search speed/AI quality improvement. All seven AC rows remain required; delivering only increments 1/2 is not issue completion.

## Research verification and delivery boundary

Read live #93 plus its named parent/dependency bodies, the three M7 house examples, `docs/versioning.md` and the current owners/contracts/gates above. Recorded actual HEAD/product and both embedded diagnostic registry versions; candidate `LEK-SRCH-*`/`search.*` names had no existing collision. No implementation test, target scan, benchmark, generation, external publication or hosted CI was performed. Every proposed fixture/gate/contract above is an implementation plan, not accepted behavior.

Delivery checks: verify the seven acceptance strings against the live issue, document section shape, existing inventory paths, UTF-8/LF and whitespace; run documentation-appropriate structure/authority/privacy/provenance/version checks. Stage only `docs/m8/issue-93-research.md`, inspect staged names, and commit locally as `docs(m8): research issue-93`. Acceptance for this dispatched research is that this document exists in HEAD and follows the M7 research shape. No files outside `docs/m8/` are changed, and no push is part of delivery.

Executed research checks passed: seven verbatim live acceptance strings, ten house-shaped sections, 52 expanded existing inventory paths, eight candidate codes absent from both embedded registries, UTF-8 without BOM and LF-only bytes. `node scripts/check-contract-versions.mjs --base 15bd13917654dc20ad19b089a5bb8e5e29c2099f` passed (product `0.6.5`, 138 contract artifacts); structure, authority and privacy checks passed; fixture provenance passed (86 synthetic families). These are documentation/current-contract checks, not new search behavior or benchmark evidence. Staged whitespace, single-path custody and HEAD presence are checked for the authorized local commit.
