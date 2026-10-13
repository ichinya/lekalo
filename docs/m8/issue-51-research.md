# Issue #51: Worklog and opt-in Changelog research

Research only, 2026-10-06. Acceptance authority: live [issue #51](https://github.com/ichinya/lekalo/issues/51), read with `gh issue view 51 --repo ichinya/lekalo --json title,body,labels`. Parent #1 and dependencies #23, #24, #25, #26 and #50 were read live for context; the five dependencies are CLOSED, which does not establish this pilot's acceptance. Checkout: branch `ichinya/m8-issue-51`, inspected HEAD `15bd13917654dc20ad19b089a5bb8e5e29c2099f`, clean at entry. The dispatch assigns M8 product **0.8.0**; the inspected `Cargo.toml` still declares **0.6.5**. A product-version update belongs to implementation, outside this document-only commit.

Decision: build a synthetic, target-neutral contracted pilot with separate local completion, worklog, provider outbox, draft, approval, publication and withdrawal operations. Reuse the accepted semantic attachments; add a small pilot composition contract and an evidence/portability report for guarantees that are not established by the existing declarations. Retain maintained PHP/Laravel implementation ports from the #50 pattern. Treat Go execution as unavailable until an actual backend supplies bound evidence.

This document follows the inventory, proposal, diagnostics, determinism, acceptance mapping and risks structure studied in `docs/m7/issue-77-research.md`, `docs/m7/issue-34-research.md` and `docs/m7/issue-90-research.md`. Every new path, semantic id, flag, contract, diagnostic and gate below is a **proposal**, not an implemented surface or reserved allocation. No runtime tests or pilot execution were performed during this research. Context7 resolution for Lekalo returned unrelated projects; CLI syntax below comes from the inspected `crates/lekalo-cli/src/main.rs`, not those results.

## Existing-surface inventory

| Surface and searchable owner | Current evidence | Consequence for #51 |
| --- | --- | --- |
| CLI: `crates/lekalo-cli/src/main.rs`, `Commands`, `ImpactArgs`, `EffectsCommands`, `GraphCommands` | `load`, `validate`, `inspect`, `effects`, `graph`, `impact`, `context` and `requirements` already exist; global `--json` and `--no-cache` are explicit. | Reuse inspection commands. No product CLI command is needed to publish real content in this pilot. |
| Model/IR: `crates/lekalo-core/src/ir/mod.rs`, loader and validator; accepted schemas in `contracts/` | Typed entities, fields, commands, queries, policies, events, scenarios and bindings can describe the domain without framework classes. | Add a synthetic module, not new Model definition kinds named `TaskWorklog` or `ChangelogEntry`. Source names use neutral `worklog.*` and `changelog.*` ids. |
| Effects: `crates/lekalo-core/src/effects/{mod,kind,edge}.rs`, `EffectGraph`, `EffectId` | Separate database writes, `enqueue-job`, `external-call` and `publish-output` have explicit identities and declared/detected provenance. | Each selected action needs its own operation and effect declaration. Sharing a correlation key does not imply execution or a single transaction. |
| Extended effects: `crates/lekalo-core/src/extended_effects/{mod,wire,contract,capability}.rs`, `ExtendedEffectsAttachment::validate_against_graph`, `map_capabilities` | `contracts/extended-effects.schema.v0.2.16.json`; publication consent has fixed `optIn:true`, approval `draft/preview/approved`, approver reference, and a revisioned snapshot field. Publication requires `publish.approval` and `publish.revisioned_snapshot`; sensitive effects require `security.review_gate`. | Reuse declaration semantics. Static approver references do not prove runtime approval of the current preview, immutable stored bytes or safe HTML rendering. The attachment is not automatically loaded by ordinary CLI validation. |
| Partial failures: `extended_effects/case.rs`, attachment `partialFailureCases`; `docs/extended-effects.md` | Fault schedules have per-contract outcomes and exact Scenario IR pins; declarations never execute. Existing valid fixture: `tests/fixtures/extended-effects/valid/planner-effects.json`. | Add pilot-specific fault schedules and runtime evidence, including a successful local completion followed by failed provider/publication actions. Existing #26 fixtures are regression coverage, not this pilot's execution. |
| Transactions: `crates/lekalo-core/src/transaction_concurrency/`, `TransactionConcurrencyAttachment`, capability mapper | Frozen `contracts/transaction-concurrency.schema.v0.2.16.json`; explicit atomic groups, replay/retry and capability requirements. | A local completion transaction must end before separately requested effects. An outbox insertion may be atomic with its own request record; an external delivery is outside that transaction. |
| Authorization: `crates/lekalo-core/src/authorization/`, `AuthorizationAttachment` | Frozen `contracts/authorization.schema.v0.2.16.json`; actors, policies and scoped protected effects. | Declare workspace/tenant ownership, service worker actor, public reader actor and publishing authority. Enforce and test them in maintained runtime ports. |
| Scenario IR: `crates/lekalo-core/src/scenario/{assertion,port,coverage}.rs` | Frozen `contracts/scenario-ir.schema.v0.2.16.json`; state, forbidden-effect, authorization, idempotency and contract assertions; unsupported features are explicit. | Reuse the same scenarios across backends. Do not add fake passing assertions when a backend cannot evaluate publication or security behavior. |
| Reference evaluator: `crates/lekalo-core/src/reference_evaluation/mod.rs`, `ReferenceEvaluation`, capability lists | In-memory staged transitions and rollback; authorization, job execution, contract-match and race schedules have explicit limitations. No reference-evaluation CLI command exists. | Use it only for the supported state-transition subset through tests. Its per-command rollback cannot prove independence of production completion from a later publication failure. |
| #50/#114 pilot: `docs/m5/issue-50-implementation.md`, `scripts/test-pilot-laravel-vue.mjs`, `tests/fixtures/php-laravel/{routes,planner}/` | Maintained Laravel port bodies, neutral Model, OpenAPI/client projections, scenario and parity harnesses exist. `scripts/test-planner-model-neutrality.mjs` checks the current planner roots. | Reuse the contracted implementation pattern and shared task identity. Add isolated worklog fixtures and extend neutrality roots explicitly; planner scenarios do not cover changelog. |
| Graph/impact/context: `crates/lekalo-core/src/{graph,impact,context}/` | Canonical dependency edges, reverse traversal and bounded context already exist. Attachments have per-surface contribution switches. | Public DTO, public query, endpoint, provider payload, scenario and binding dependencies must be registered explicitly. Ordinary Model impact does not prove all external consumers of an attachment were discovered. |
| Diagnostic registry: `crates/lekalo-core/src/diagnostics/{registry,version,id,normalize}.rs` | Base registry embeds `contracts/diagnostic-registry.v0.6.4.json`; a `v0.6.5` successor is also embedded. Diagnostic wire remains `0.2.16`; codes use `LEK-[A-Z][A-Z0-9]{1,11}-NNN`. | Merge proposed pilot entries into a product-version successor with all existing entries preserved; do not allocate against only the base registry. `WC` fits the existing grammar. |
| Privacy and fixture governance: `crates/lekalo-core/src/privacy/`, `scripts/{check-privacy,test-fixture-provenance,test-golden-hygiene}.mjs`, `tests/fixtures/suite/` | Export governance and synthetic fixture/provenance gates exist; privacy export is separate from application publication. | Public artifacts must be synthetic and identity-free. Changelog approval cannot authorize exporting private Lekalo evidence. No new governed export kind is needed for a local report. |
| CI and versioning: `.github/workflows/ci.yml`, `scripts/check-contract-versions.mjs`, `docs/versioning.md` | Existing schema gates, Rust suites, binary-driven gates, native target jobs and fixture checks have separate wiring. Changed/new contracts take the product version; unchanged contracts retain accepted bytes and versions. | At implementation product 0.8.0, publish only changed/new families at 0.8.0 and pin frozen inputs exactly. Add binary-driven gates after the binary build in the same job. |

## Proposed command/report schema

### Goal decomposition and implementation increments

Each increment is reviewable independently; completing an increment does not satisfy every acceptance row.

| Increment | Proposed deliverables | Observable exit condition |
| --- | --- | --- |
| 1. Neutral domain and independent effects | `tests/fixtures/pilot/worklog-changelog/model/lekalo/modules/{worklog,changelog}/`, task import compatible with #50, OpenSpec requirements and requirement bindings; composition contract and pure core validator. | Every entity/operation resolves; completion-only has no path that enables a provider comment, worklog or changelog; all effects and default absence are inspectable. |
| 2. Worklog and outbox | Maintained contracted ports for completion, worklog create/edit, explicit provider-action request and service-worker delivery; transaction, authorization, job/call and error attachments. | Local success survives independent failure; per-operation idempotency, payload conflict and concurrent duplicate requests are covered without a live provider. |
| 3. Draft, preview, approval and snapshot | Explicit draft/preview/approve/publish/withdraw operations; versioned public DTO, append-only snapshots and audit; renderer/public-query contract and security policy. | Publish rejects missing/stale approval; editing a source worklog cannot change published bytes; withdrawal hides public output and keeps restricted audit. |
| 4. Shared scenarios and PHP evidence | Scenario IR corpus, deterministic fixtures, Laravel native runtime tests, fake provider transport and fault injection; requirements-to-test/gate trace. | All ten issue scenarios have actual assertion results and exact provenance; unsupported checks are retained, never counted as passes. |
| 5. Impact and portability | Closed evidence report, reverse-consumer oracle, PHP/Go capability mapping and negative backend fixtures; CI/suite catalog integration. | Every declared changelog consumer appears with a witness; required missing publication guarantees block strict target acceptance; unknown external consumers remain visible. |
| 6. Publication hygiene and delivery | Model/fixture/evidence neutrality and leak gates, implementation notes, reproducible golden/checksum updates and version pins. | Full eight-row acceptance mapping is reviewable; public artifacts contain no private application identity or data. |

### Proposed semantic definitions and effect boundaries

`TaskWorklog` maps to `worklog.task_worklog`: tenant/workspace, task, author, private body, record revision, correlation key and timestamps. `ChangelogEntry` maps to `changelog.entry`: tenant/workspace, optional source-worklog reference, independent draft content, visibility, lifecycle state and current draft revision. `changelog.snapshot` contains a public allowlisted payload, immutable revision, content digest and schema version. `changelog.approval` binds entry, exact preview revision/digest, destination, publishing authority and policy revision. `changelog.audit` preserves lifecycle transitions under a restricted read policy; it is never returned by the public endpoint. Provider request/outbox state is a separate `worklog.provider_action` entity.

| Operation id | Explicit declared effects and behavior |
| --- | --- |
| `worklog.complete_task` | Update the task's local completion state only; local transaction and optimistic precondition. No implicit write to worklog, provider action or changelog. |
| `worklog.write_worklog`, `worklog.edit_worklog` | Create/update the private worklog only under the author/tenant policy. Editing invalidates any dependent draft preview if it is refreshed, never a stored published snapshot. |
| `worklog.request_provider_comment` | Explicit user selection creates a deduplicated request/outbox row and `enqueue-job` declaration. It does not execute the provider synchronously. |
| `worklog.deliver_provider_comment` | Service actor performs the declared `external-call`, records its independent result, retry state and typed error. Missing provider idempotency support forbids blind retries of an ambiguous write. |
| `changelog.create_draft`, `changelog.edit_draft` | Explicit opt-in creates/edits an independent draft. Copying source content is a reviewed public projection; private fields are not inherited wholesale. |
| `changelog.preview_entry`, `changelog.approve_entry` | Preview produces canonical sanitized public bytes. Approval records authority and the precise preview/destination/policy binding; any draft change invalidates approval. |
| `changelog.publish_entry` | Require fresh approval and policy; atomically append immutable snapshot and audit/update visibility within its own local transaction; declare `publish-output` separately. A remote destination, when selected, adds a separate delivery effect and separate outcome. |
| `changelog.withdraw_entry` | Explicit authorized withdrawal hides the public projection and appends audit; immutable history survives under restricted access. A delayed publish worker cannot resurrect an already withdrawn entry. |
| `changelog.public_entry` | Public actor reads only currently published snapshot fields through a public DTO. No worklog body, private title, internal identifiers, provider credentials or audit joins. |

Composition may request completion plus worklog in one user interaction, but declares each selected effect and receives a separate outcome. Omitted selections default to absent. The pilot contract contains no `complete => publish` shortcut. Correlation groups observations; idempotency deduplicates a particular operation. Dedup keys are scoped to `(tenant, actor-or-service-scope, operation, correlationKey)`, bind a canonical payload digest and retain replay results. The same key with a different payload conflicts. Reusing a correlation key for completion cannot accidentally suppress a separately requested worklog or publication.

Local completion, optional worklog, provider request, provider delivery and publication have separate commit boundaries. Default partial success returns a vector of independent outcomes. An explicitly declared local atomic group may roll back its members; there is no claim that it includes an external provider write. Outbox durability and enqueue visibility need their own capability evidence. A crash after remote acceptance leaves delivery unknown/recovery-required until reconciliation or supported idempotent replay resolves it.

### Exact CLI surface and proposed harness

Existing commands below are source-verified syntax; the fixture root and semantic ids are proposed. Run from the repository root after implementation has created the fixture. `--no-cache` bypasses runtime cache mutation; these commands inspect/validate and never publish application content.

```text
lekalo --json --no-cache load --project tests/fixtures/pilot/worklog-changelog/model
lekalo --json --no-cache load --ir --project tests/fixtures/pilot/worklog-changelog/model
lekalo --json --no-cache validate --strict --project tests/fixtures/pilot/worklog-changelog/model
lekalo --json --no-cache inspect worklog.complete_task --include bindings,scenarios --project tests/fixtures/pilot/worklog-changelog/model
lekalo --json --no-cache effects show worklog.complete_task --project tests/fixtures/pilot/worklog-changelog/model
lekalo --json --no-cache effects show changelog.publish_entry --project tests/fixtures/pilot/worklog-changelog/model
lekalo --json --no-cache graph callers changelog.public_entry_dto --transitive --project tests/fixtures/pilot/worklog-changelog/model
lekalo --json --no-cache impact changelog.public_entry_dto --depth 256 --profile strict --project tests/fixtures/pilot/worklog-changelog/model
lekalo --json --no-cache context changelog.publish_entry --budget 5000 --project tests/fixtures/pilot/worklog-changelog/model
lekalo --json --no-cache requirements validate tests/fixtures/pilot/worklog-changelog/attachments/requirements.json --project tests/fixtures/pilot/worklog-changelog/model
lekalo --json --no-cache requirements trace tests/fixtures/pilot/worklog-changelog/attachments/requirements.json --project tests/fixtures/pilot/worklog-changelog/model
```

`validate --strict` validates the Model, not arbitrary sibling attachments. The proposed core composition tests must parse, pin and cross-validate the #23–#26 attachments explicitly. `impact --depth 256` still needs completeness inspection; it is not a proof that all consumers are present. `context` truncation and unsupported gates remain visible.

Proposed new harness entrypoint, not a current command:

```text
node scripts/test-worklog-changelog-pilot.mjs --target php --profile strict --case all
node scripts/test-worklog-changelog-pilot.mjs --target go --profile permissive --case all
node scripts/test-worklog-changelog-pilot.mjs --target go --profile strict --case all
```

Closed flags: `--target php|go`, `--profile strict|permissive`, and `--case all|<exact-stable-scenario-id>`; no shell command, provider URL or consumer path flags. The gate emits a schema-valid report on stdout, bounded infrastructure diagnostics on stderr, and exits 0 only when the selected supported obligations pass; strict required unsupported/unknown semantics exit nonzero while retaining the report. A known absent Go backend produces explicit unsupported evidence: the negative gate asserts that denial, not Go execution. Only a real backend can produce passing native scenario results. No setup, package install, credential lookup or live network call occurs in verification.

### Contract artifacts and ownership

| Proposed artifact | Required shape and owner |
| --- | --- |
| `contracts/worklog-changelog-pilot.schema.v0.8.0.json` | Closed `dev.lekalo.worklog-changelog-pilot@0.8.0`, discriminator `lekalo/worklog-changelog-pilot/v0.8.0`; pure `crates/lekalo-core/src/worklog_changelog/` owner validates composition. Pin project/Model/IR/effect graph and consumed attachment identities/digests; enumerate operation/effect roles, explicit selections, transaction groups, replay scopes, approval/snapshot field refs, public field allowlist and required capabilities. No runtime values, secrets or physical paths. |
| `contracts/worklog-changelog-evidence.schema.v0.8.0.json` | Closed `dev.lekalo.worklog-changelog-evidence@0.8.0`, discriminator `lekalo/worklog-changelog-evidence/v0.8.0`; local harness-owned report. Exact input/schema/backend/toolchain pins; stable scenario ids; per-effect outcomes and assertions; requirement-to-test/gate references; completeness and consumer witnesses; PHP/Go capabilities and verdicts. No raw request/response/body, logs, host paths or credentials. |
| `tests/fixtures/pilot/worklog-changelog/attachments/*.json` | Instantiate accepted transaction-concurrency, authorization, extended-effects, error, invariant-transition and requirements families; add `pilot.json` using the new composition family. Canonicalize pins after compiling the neutral Model; no placeholder digest accepted in a golden. |
| `tests/fixtures/pilot/worklog-changelog/public/changelog-entry.schema.json` | Public application DTO schema at semantic version 0.8.0, public snapshot payload version, generated/checked transport/OpenAPI and client bindings. This is a domain payload contract, distinct from a Lekalo attachment discriminator. |
| `tests/fixtures/pilot/worklog-changelog/scenarios/*.json`, `evidence/`, `invalid/`, `php/`, `go/` | Stable Scenario IR documents, synthetic fault controls and native evidence; Go capability/refusal fixtures initially. Store canonical expected reports with checksums and provenance. Native implementation code is confined to the target fixture, not embedded in the semantic module. |
| `contracts/diagnostic-registry.v0.8.0.json` | Registry successor merges base and current successor entries plus pilot codes; sorted ids, unique codes, messages, severity/status, location and closed data declarations. Registry consumers/constants and pin-bearing reports are updated consistently in implementation. |

New schemas use Draft 2020-12 and reject unknown/duplicate keys. Proposed bounds: 128 operation-role bindings, 128 consumers, 64 scenarios, 256 assertions per scenario, 32 capability requirements per target, 4 MiB canonical report. Overflow rejects or yields an explicitly incomplete verdict; it never silently removes a failed obligation. Report digest excludes its own digest field and uses a documented domain prefix plus canonical UTF-8 bytes.

At implementation product **0.8.0**, new composition/evidence contracts and a changed registry get 0.8.0 names, `$id`, identity, discriminator, embedded constants and consumer references together. Model, IR, effect graph, Scenario IR, authorization, transaction-concurrency, extended-effects, trace and diagnostic wire stay frozen if their bytes remain unchanged. Existing fixture instances may change while their schema version stays fixed. Do not add runtime approval receipts to the closed extended-effects wire without its own 0.8.0 successor; the proposal instead models approval records in the domain and validates their binding through the pilot contract. Any unavoidable protocol/profile change requires a separately identified successor, not a global version replacement. Follow `docs/versioning.md`; this research neither changes Cargo nor publishes schemas.

## Proposed check definitions with data sources

| Check | Data source and required evidence |
| --- | --- |
| Independent effects | Typed operations/effect graph and composition-role bindings; negative completion-only, worklog-only and draft-only executions. Inspect absence of provider/publication effects, not merely a missing UI checkbox. |
| Preview/approval | Domain approval records + publish preconditions + publication consent attachment. Verify authority, current preview digest/revision, tenant, destination and policy binding at commit time. Static `approved` metadata alone cannot pass. |
| Partial success | Explicit transaction boundaries, Scenario IR fault schedule and runtime state before/after each action. Failed worklog/provider/publication leaves independently committed completion intact; a declared local atomic group has a contrasting rollback case. |
| Snapshot immutability | Append-only revision identity, exact canonical public bytes/digest and native persistence test. Worklog/draft edits cannot mutate an existing snapshot; republish creates a new revision, and no approved mutable-source pointer substitutes for stored content. |
| Privacy/security | Public DTO field allowlist, actor/tenant policy, safe rendering/URL policy and public HTTP result. Cross-tenant access denied; anonymous reader sees only published snapshots; full response headers/body checked for private joins and unsafe HTML/links. |
| Replay/retry safety | Transaction replay records, exact payload bindings, outbox uniqueness and fake provider attempt log. Duplicate requests replay; payload drift conflicts; different tenants and operations remain independent; ambiguous remote acceptance is explicit. |
| Consumer impact | Complete reverse dependency closure of `changelog.public_entry_dto`, field mutation diff, typed bindings/trace and a fixture oracle listing expected consumers. Count known consumers exactly; unavailable manifests/inferred external consumers leave coverage incomplete. |
| PHP/Go portability | #24/#26 capability requirements mapped to pinned backend snapshots, selected profile and bound execution evidence. Missing/partial approval, revisioned storage, outbox/delivery, authorization or rendering semantics remain individually visible. Language-level DTO generation is insufficient. |

Public security policy proposal: plain text with target-appropriate escaping is the default; rich text requires a declared sanitizer policy and test corpus. URLs permit only reviewed `https`/`http` forms, reject script/data schemes, encoded/obfuscated schemes and control characters; previews and final public rendering apply the same policy. The backend builds public fields from an allowlist, never serializes a private worklog/entity wholesale. Correlation/idempotency keys, provider identifiers and audit metadata are private unless a distinct public contract deliberately admits them. Security requirements remain target-neutral; no unsupported backend receives a portable-success label.

## Proposed determinism strategy

Use explicit Scenario IR clocks, stable task/tenant/actor ids, seeded id sources, sorted operations/effects/consumer witnesses and canonical JSON. Timezone, locale, process cwd, filesystem enumeration and native auto-increment ids cannot alter neutral expectations. Snapshot content digest uses canonical public fields, field/schema version and declared Unicode normalization; mutable worklog metadata is excluded. Approvals bind the exact canonical preview bytes, not rendered browser text or a hash of private input.

Fake provider scripts have no network and deterministic timeout/error/ambiguous-acceptance modes. Fault controls distinguish failure before local commit, after local commit, before outbox commit, after remote acceptance and before publication visibility. Use real concurrent duplicate/withdraw-publish tests where the PHP fixture provides isolation; otherwise preserve unsupported concurrency, not a serialized imitation. Fixture databases and artifacts are isolated per case under temporary roots. Inspection and gate runs never update committed goldens implicitly.

Golden regeneration is a deliberate reviewed update using existing suite tools and exact producer pins. Register the synthetic family in existing fixture provenance and suite catalog/checksums; add trigger/non-trigger coverage for new rules. Cross-platform normalization removes physical paths and wall-clock metrics from comparable evidence, never failed assertions or unsupported capability rows. Reports retain exact consumed input identities so stale evidence cannot satisfy current acceptance.

## Proposed diagnostics families and codes

Reuse existing registered `LEK-XE-006` (`publication.consent-missing`), `LEK-XE-007` (`contract.capability-missing`), `LEK-XE-008` (`contract.retry-conflict`), `LEK-XE-009` (`case.case-invalid`) and `LEK-XE-010` (`contract.gate-required`) for their current declaration failures. Reuse authorization, transaction and `semantic.public-output-private-type` rules for those owners' violations. Business errors such as stale approval, denied actor and conflicting replay belong to the error-contract/domain outcome vocabulary, not an undifferentiated diagnostic catch-all.

Proposed `LEK-WC-*` entries below describe composition validation or evidence admission. They do not duplicate a dependency's wire-parse diagnostic and are not allocated in the current registry.

| Code / rule id | Category; severity/status | Trigger |
| --- | --- | --- |
| `LEK-WC-001` / `worklog-changelog.input-invalid` | model; error/invalid | Malformed composition document, unsupported member, duplicate role or inconsistent input pins. Unknown version follows existing unsupported-version handling. |
| `LEK-WC-002` / `worklog-changelog.effects-coupled` | semantic; error/invalid | Completion/worklog/provider/publication roles merge effects or enable an unselected independent effect. |
| `LEK-WC-003` / `worklog-changelog.approval-unbound` | security; error/invalid | Publication preconditions cannot bind approval to the current tenant, preview revision/digest, destination and authority. |
| `LEK-WC-004` / `worklog-changelog.snapshot-mutable` | semantic; error/invalid | Published projection points to mutable source state or admitted evidence changes an existing snapshot. |
| `LEK-WC-005` / `worklog-changelog.public-data-unsafe` | security; error/invalid | Public projection violates private-field, visibility, renderer or safe-URL obligations. |
| `LEK-WC-006` / `worklog-changelog.replay-scope-invalid` | semantic; error/invalid | Operation/tenant replay scopes collide or retries have no payload-bound safety policy. |
| `LEK-WC-007` / `worklog-changelog.evidence-incomplete` | compatibility; error/unsupported | Required native assertion/capability/consumer evidence is absent, stale, unsupported or incomplete. The report keeps original per-target statuses. |

The pure core service normalizes through the registry-backed constructors and existing `DiagnosticSet`; the harness does not invent unregistered JSON diagnostic objects. Each rule has a stable message id, closed fixed-token `detail` field, bounded semantic location when available and no body/URL/provider-log echo. Keep parser and registry infrastructure failures distinct. Registry, fixture diagnostic coverage, diagnostic contract gates and every pin-bearing consumer must agree on the successor; no change to the diagnostic-code grammar/category enum/wire schema is proposed. Recheck allocations against the actual merged M8 registry before implementing.

## Proposed profile format and non-waivable policy

Profiles are explicit harness selections, not changes to ordinary `validate --strict`. Both profiles require schema validity, effect separation, tenant isolation, public-data protection, payload-bound replay and honest evidence. **Strict** denies any required capability/assertion that is unsupported, unknown, unapproved partial or not executed. **Permissive** emits a degraded report for missing target execution while retaining every gap; it cannot satisfy the issue's PHP/Go runtime claims. No waiver turns private data exposure, missing approval or mutable snapshots into success.

Portability rows include target id, exact backend/profile identity and digest, required capability, support `supported|partial|unsupported|unknown`, approval of any partial guarantee, assertion binding and required-blocking flag. Aggregate verdict derives from these rows. At this checkout no Go publication execution was established; the initial proposed Go report must say so. A PHP report needs actual runtime tests for approval, immutable persistence, public rendering and outbox behavior, even if the adapter advertises those capabilities.

## Proposed test plan mapping each acceptance criterion

The first column reproduces issue #51 acceptance text **verbatim**, including checkbox spelling. Scenario/gate references are proposals; none was executed for this research.

| Issue acceptance criterion (verbatim) | Deliverable and required proof |
| --- | --- |
| `- [ ] Independent effects представлены отдельными declarations.` | Increment 1 Model/effect-role inventory, composition validator and `worklog.scenario.completion_without_worklog`, `worklog.scenario.completion_with_worklog`, `worklog.scenario.no_provider_comment`, `changelog.scenario.no_opt_in`; forbidden effects and separate outcome assertions. |
| `- [ ] Publication contract требует preview/approval.` | Increment 3 publication/approval contracts; `changelog.scenario.approval_required` and `changelog.scenario.stale_preview`; missing preview/approval, changed draft, wrong actor/tenant/destination and concurrent invalidation all deny without a snapshot. |
| `- [ ] Partial failures не откатывают successful local completion без declared transaction.` | Increment 2/4 transaction attachment plus `worklog.scenario.provider_partial_failure`, `worklog.scenario.worklog_partial_failure`, `changelog.scenario.publication_partial_failure`; completion remains committed. Contrasting `worklog.scenario.declared_atomic_rollback` proves the declared local boundary. |
| `- [ ] Snapshot semantics переносимы между targets.` | Increment 3/5 canonical public DTO/revision rules and identical byte/digest expectations; `changelog.scenario.source_edit_after_publish`, `changelog.scenario.republish_revision`; per-target capability and execution evidence. Unsupported target rows block strict acceptance rather than masquerading as equality. |
| `- [ ] Scenarios проверяют privacy/security/idempotency.` | Increment 4 authorization, public endpoint, renderer/URL corpus and replay tests: cross-tenant denial, private-field exclusion, XSS/unsafe links, identical replay and same-key/different-payload conflict; fixture isolation and deterministic clock/id controls. |
| `- [ ] Impact показывает все consumers changelog schema.` | Increment 5 reverse-consumer oracle includes draft preview, snapshot, publisher/withdrawer, public query/DTO, HTTP/OpenAPI projection, client binding and scenarios. Additive and breaking field mutation probes compare exact consumer sets and paths; missing attachment/manifest evidence cannot report complete. |
| `- [ ] PHP/Go portability report не скрывает unsupported publication semantics.` | Increment 5 report contains PHP and Go separately for `publish.approval`, `publish.revisioned_snapshot`, required delivery/security/authorization guarantees and native assertion coverage. Unknown/partial/unsupported remain visible; strict negative tests preserve a report and fail the gate. |
| `- [ ] Public artifacts не раскрывают название consumer application.` | Increment 6 neutrality/provenance/leak gates scan the complete public artifact inventory, successful and failed report branches, Model/OpenSpec, schema comments, snapshots, diagnostic messages and generated bindings. Use neutral synthetic ids; no private checkout discovery or identity hashes. |

### Proposed fixture families and issue-scenario coverage

| Issue scenario (verbatim) | Proposed stable scenario / essential assertion |
| --- | --- |
| `completion without worklog;` | `worklog.scenario.completion_without_worklog`: completion persists; no worklog, provider request or changelog rows/effects. |
| `completion with worklog;` | `worklog.scenario.completion_with_worklog`: explicit selection creates one worklog; provider and changelog remain absent. |
| `repeated correlation key;` | `worklog.scenario.repeated_correlation_key`: same operation/body replays, changed body conflicts, different tenant/operation remains independent; concurrent duplicate creates one logical action. |
| `no provider comment by default;` | `worklog.scenario.no_provider_comment`: provider attempt log is empty for completion and worklog-only paths. |
| `no changelog without opt-in;` | `changelog.scenario.no_opt_in`: no draft/snapshot/publish effect for completion/worklog/provider paths. |
| `edit worklog after publish does not alter snapshot;` | `changelog.scenario.source_edit_after_publish`: exact existing snapshot bytes/revision/digest unchanged after worklog edit. |
| `provider/publication partial failure;` | Separate provider/publication fault cases above; independently successful completion remains committed, failed action is explicit, replay and recovery do not duplicate publication. |
| `withdraw keeps audit trail;` | `changelog.scenario.withdraw_audit`: anonymous endpoint no longer serves the entry; restricted audit and immutable history survive; repeated withdrawal replays safely. |
| `public endpoint excludes private data;` | `changelog.scenario.public_projection`: allowlist-only payload; drafts/withdrawn entries and cross-tenant private joins excluded, including provider/correlation/author metadata. |
| `XSS/unsafe links.` | `changelog.scenario.unsafe_content`: script tags, event attributes, unsafe/encoded schemes and malformed links rejected or safely rendered under the same preview/final policy; never report sanitizer support without backend evidence. |

The family additionally needs invalid declaration pairs, stale/missing input pins, unsupported assertion/capability vectors, approval TOCTOU, publish/withdraw ordering, ambiguous remote acceptance, altered report digests and altered snapshot schema versions. `evidence/consumer-oracle.json` lists the synthetic expected consumer set and witness paths; `evidence/php.json` and `evidence/go.json` distinguish observed tests from capability-only claims. Extend existing suite/provenance catalogs and neutrality roots; do not create a competing global golden catalog or silently rewrite #50 evidence.

### Proposed contract gates and CI wiring

Create `scripts/test-worklog-changelog-contracts.mjs` for both new closed schemas, canonical bytes, duplicate keys, pin coherence, registry entries and hostile invalid vectors. Create `scripts/test-worklog-changelog-pilot.mjs` for the explicit target/profile/case execution/report surface, `scripts/test-worklog-changelog-impact.mjs` for binary-driven consumer mutation probes, and `scripts/test-worklog-changelog-neutrality.mjs` for complete public artifact/leak and semantic-neutrality checks. Add `crates/lekalo-core/tests/worklog_changelog.rs` for pure composition, attachment joins, capability refusal and diagnostic normalization; native PHP assertions live in the fixture target and are called by the pilot gate.

Future implementation verification commands, not run during research:

```text
cargo fmt --all -- --check
cargo test --workspace --locked
cargo build -p lekalo-cli --locked
node scripts/test-worklog-changelog-contracts.mjs
node scripts/test-worklog-changelog-pilot.mjs --target php --profile strict --case all
node scripts/test-worklog-changelog-impact.mjs
node scripts/test-worklog-changelog-neutrality.mjs
node scripts/test-extended-effects-contracts.mjs
node scripts/test-transaction-concurrency-contracts.mjs
node scripts/test-authorization-contracts.mjs
node scripts/test-scenario-contracts.mjs
node scripts/test-diagnostic-contracts.mjs
node scripts/test-fixture-provenance.mjs
node scripts/test-golden-diagnostic-coverage.mjs
node scripts/check-contract-versions.mjs --base HEAD^
node scripts/check-structure.mjs
node scripts/check-privacy.mjs
```

Schema gates use the existing exact Ajv 8.17.1 provisioning outside the checkout with `NODE_PATH` and `LEKALO_AJV_NODE_PATH`; no package installation inside verification. Binary gates follow `cargo build` in their own CI job and resolve the fresh binary explicitly. Native PHP provisioning is a separate pinned Composer/Laravel/Laratesto job; missing dependencies fail required acceptance rather than silently skipping. Add the Go unsupported-report negative gate in ordinary CI; real Go execution, when supplied, gets a separate toolchain-pinned job using the same corpus. Retain the current planner/native/parity gates when changing shared fixture helpers. Hosted CI, native acceptance and real provider acceptance are separate evidence levels.

## Risks and non-goals

- **Private requirement availability.** Issue #51 supplies the public semantic inventory and acceptance; its private source requirement was not supplied. This plan cannot claim complete private-product parity. Clarify any additional unpublished behavior before expanding implementation scope.
- **Static approval mistaken for runtime permission.** Existing `approved` and `approverRef` are declaration data. Binding approval to a current preview and checking it atomically at publish time is required, including stale approval and publish/withdraw races.
- **Framework transaction default.** Wrapping every selected action in one request-wide transaction would violate the independent-success requirement. Declare boundaries first and test faults after an actual local commit. Do not inherit reference evaluator rollback as application policy.
- **Outbox ambiguity.** Durable request insertion does not prove exactly-once remote comments. Require supported provider idempotency/reconciliation or retain an unknown delivery outcome; compensation is a recovery contract, not proof that an external call is atomic.
- **Mutable snapshots and leakage.** Public serialization of a live worklog, private content hashes or restricted audit pointers defeats immutable/public-data semantics. Store the allowlisted snapshot itself and prove every public endpoint against it.
- **Incomplete impact.** Registered relation kinds, broad module bindings and depth-capped traversals are not proof of every consumer. Compare a full fixture oracle, attach explicit schema-consumer dependencies and retain unknown external-consumer coverage.
- **Portability overclaim.** Existing PHP code generation and Go expression rendering cannot prove approval, queue delivery, isolation or renderer security. Unsupported semantics remain per-target report data and strict blockers until native evidence exists.
- **Version/registry merge drift.** The M8 branch currently carries product 0.6.5 and two embedded registry generations. Implementation must coordinate the 0.8.0 product update, successor pins and code allocations across the actual merged branch without changing frozen families unnecessarily.
- **Artifact privacy.** A passing local report is not authorized for upload by application publication approval. Synthetic public fixtures and existing privacy/export governance remain distinct.

Non-goals: implementing a production provider integration, sending real comments or publishing/withdrawing real content, discovering private consumer repositories, reading private product requirements, migrating production databases, inventing a rich-text editor, adding a mandatory runtime engine or new Model kinds, claiming PHP/Go behavioral equivalence from schema validity, and changing #50's planning ownership. This dispatched phase changes only this file in `docs/m8/`, commits locally and performs no push or GitHub mutation.

## Research validation and completion boundary

Research evidence is live issue/dependency reads, repository source/schema/gate inspection and the M7 document format. Document checks passed for nine required sections, eight verbatim acceptance rows, all ten issue scenarios, 17 existing owner paths, seven proposed codes without collisions in either embedded registry, and UTF-8/LF bytes. `node scripts/check-contract-versions.mjs` passed against HEAD (product 0.6.5, 138 contract artifacts); `node scripts/check-structure.mjs` and `node scripts/check-privacy.mjs` passed. These checks validate the documentation boundary and existing governance, not the proposed pilot. Staging is limited to this document and checked with `git diff --cached --check`. The deliverable is this file present in the HEAD commit with message `docs(m8): research issue-51`; implementation, runtime conformance, hosted CI and external provider/publication acceptance remain future work.
