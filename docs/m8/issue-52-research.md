# Issue #52: multi-provider TaskExternalLink research

Research only, 2026-10-06. Acceptance authority: [issue #52](https://github.com/ichinya/lekalo/issues/52), read live with `gh issue view 52 --repo ichinya/lekalo --json title,body,labels`. Checkout: `ichinya/m8-issue-52`, base `15bd13917654dc20ad19b089a5bb8e5e29c2099f` from `ichinya/M8`; inspected Cargo product version is `0.6.5`, while the dispatched M8 implementation target is **0.8.0**. Parent #1 and direct dependencies #23/#24/#25/#26/#49/#50 were read only for context. This phase changes only this document under `docs/m8/`, makes one local documentation commit, and does not implement contracts or access a private consumer.

Decision: deliver a synthetic, target-neutral `tasklinks` module and explicit integration contracts, then exercise them through deterministic mock providers and qualified native bindings. A task owns zero or more links; every external identity belongs to at most one task in its installation scope. Every outbound action passes an explicit per-link capability and field/action policy decision, then becomes a durable outbox intent. Provider success, conflict, denial and partial delivery remain separate observations. No provider-to-provider bridge is introduced.

The structure follows the evidence inventories, proposal sections, diagnostic tables, acceptance mapping and risk sections in `docs/m7/issue-77-research.md`, `issue-90-research.md` and `issue-100-research.md`. All paths, fields, APIs, flags and codes in proposal sections are **proposals**, not present functionality or allocated registry entries. Existing-surface rows describe inspected source, not executed acceptance. The eight acceptance criteria are quoted verbatim below; implementation details resolve unspecified choices without claiming they were in the issue.

## Existing-surface inventory

| Surface | Inspected owner / current fact | Consequence for #52 |
| --- | --- | --- |
| Model / typed IR | `contracts/model.schema.v0.2.16.json`, `crates/lekalo-core/src/ir/mod.rs`. Entity declarations have typed `fields` and an `identity` field set. The closed entity shape has no typed relation or secondary unique-constraint members. Policy declarations have `applies_to` and `decision: allow|deny`. | Existing entity identity is useful but cannot express all AC1 constraints. Model/IR successors need typed relation and uniqueness declarations; policy prose cannot implement conditional per-link fan-out. |
| Loader / validation | `crates/lekalo-core/src/loader/`, `validator/`, CLI `Commands::Load` and `Commands::Validate` in `crates/lekalo-cli/src/main.rs`. `validate --strict` also calls authorization review. | Add explicit semantic checks through these owners; do not treat successful YAML loading as relation, policy or delivery acceptance. |
| Existing storage relations | `contracts/storage-projection.schema.v0.4.0.json`, `crates/lekalo-core/src/storage_projection/`, `tests/fixtures/storage-projection/valid/planner-storage.json`, `docs/storage-projection.md`. The fixture already has `planner.relation.task_external_links` (aggregate child, min 0), `planner.relation.link_provider` (external reference, detach), and a `uq_external_identity` over provider/external key. | Reuse projection mechanics, but its illustrative provider/key scope and task-child cascade are not the full #52 identity/history contract. The Model must own the new semantics and projections must agree with them. The fixture is not a provider synchronization implementation. |
| Dependency / impact graph | `crates/lekalo-core/src/graph/`, `impact/`, `target_profile/portability.rs`; `graph callers --transitive`, `impact SYMBOL`, typed target profile axis/capability deltas. | Represent contract consumers through typed references and provide witnesses. Core target-profile portability is an in-process API; it does not constitute a complete pilot/provider portability report. |
| Effects | `crates/lekalo-core/src/effects/`, `extended_effects/mod.rs::ExtendedEffectsAttachment`, `effect_bindings`, `validate_against_graph`; `contracts/extended-effects.schema.v0.2.16.json`. Events, jobs, calls, partial failure cases and capability requirements already have closed contracts and explicit contribution switches. | Reuse durable event/job, external-call and partial-failure declarations. Set the graph/impact/context/scenario contributions explicitly and validate exact Model/IR/effect pins. A declaration never proves network delivery. |
| Transactions / authorization | `transaction_concurrency/`, `authorization/`, `invariant_transition/`; `contracts/{transaction-concurrency,authorization,invariant-transition}.schema.v0.2.16.json`. | Reuse atomic local effect groups, optimistic version checks, idempotency, actor/scope/policy and typed transitions. External provider calls sit outside the local DB transaction. Capability support and authority remain independent decisions. |
| Scenario declarations / execution | `scenario/mod.rs::ScenarioIr`, `reference_evaluation/`, `scenario_evidence.rs`; `contracts/scenario-ir.schema.v0.2.16.json`, `scenario-run.schema.v0.4.0.json`. Scenario IR has result/error, entity-state, emitted/forbidden-effect, authorization and idempotency assertions. The reference evaluator explicitly reports authorization, opaque fixture materialization and concurrency race schedules as unsupported; it has no CLI command in its initial design. | Add supported deterministic mock/reference bindings for the new behavior or return unsupported. Native DB races, outbox restart behavior and provider behavior need their own evidence. Do not invent a current `lekalo scenario run` command. |
| Public consumer baselines | `tests/fixtures/pilot/taskhub/`, `taskhub-scans/README.md`, `tests/fixtures/php-laravel/routes/model/lekalo/`, `docs/m5/tutorial-laravel-vue-pilot.md`. #49 is an observed task/integration baseline; live #50 specifies maintained Laravel backend and separate Vue frontend in contracted mode. | Use synthetic names and explicit Task/event bindings. Keep provider updates away from local planning fields. Do not infer that #52 must be Node-only from #49 or older template prose. |
| Diagnostics | `diagnostics/registry.rs` embeds `diagnostic-registry.v0.6.4.json` and separately exposes the `0.6.5` successor. `diagnostics/types.rs::Category` has model/semantic/compatibility/adapter/infrastructure/security. | A new rule must enter the correct successor and its consumers, not merely edit one historical registry. Use a distinct proposed `LEK-TASKLINK-*` family; preserve existing families and categories. |
| Privacy / evidence | `docs/privacy.md`, `contracts/authority-contracts.manifest.json`, `scripts/test-privacy-leak-corpus.mjs`, fixture provenance and suite catalog/coverage owners. | Public artifacts contain synthetic identities and no consumer application name, repository URL, token or raw private payload. Classification and export admission are required, including failure diagnostics and logs. |
| Gates / CI | `scripts/test-{scenario,transaction-concurrency,authorization,extended-effects,storage-projection}-contracts.mjs`; `.github/workflows/ci.yml` provisions exact Ajv 8.17.1 outside the checkout and runs `cargo build --workspace --locked` before binary-dependent gates. | Extend each changed owner and add a distinct #52 gate. Required binaries/dependencies must fail when absent. Register synthetic fixtures and diagnostic pairs in the golden suite; never rely on a binary produced by another CI job. |
| Version rule | `docs/versioning.md`, `scripts/check-contract-versions.mjs`. Changed/new contract versions, filenames, identities, discriminators, embedded constants and consumer pins follow the implementation commit's product version; unchanged contracts keep their accepted bytes/version. | M8 implementation must establish product `0.8.0` before publishing changed contracts. This research does not bump Cargo or replace accepted contracts. |

### Dependency disposition

#1 supplies the roadmap boundary. #23 supplies portable scenarios and explicit unsupported outcomes; #24 supplies local atomicity, race/idempotency and partial-success boundaries; #25 supplies actor/scope/policy; #26 supplies external calls/jobs/events and retry semantics. #49 supplies confirmed observed Task/integration links, with existing code canonical; #50 supplies the Laravel/Vue contracted planner and provider/planning ownership boundary. Their issue bodies are context, not proof of completed implementations or permission to expand this task. No transitive dependency or private source was inspected.

## Goal decomposition into implementable increments

| Increment | Concrete proposed change | Completion witness / dependency boundary |
| --- | --- | --- |
| I1 Model relations and identity | Add typed Model/IR relation and unique-constraint support; author TaskExternalLink, installation, conflict, inbox receipt and outbox entities; bind the synthetic Task explicitly. | Valid/invalid cardinality, cross-workspace reference and uniqueness fixtures; canonical Model/IR roundtrip; storage projection agreement. Complete before claiming AC1. |
| I2 Decisions and contracts | Add typed per-link capabilities, action/field policies, policy decision records, allowed-field projection and explicit provider request/response/error contracts. | Allow/deny/unsupported/unknown matrix and provenance, scope isolation, forbidden planning-field write; every provider intent has a preceding admitted decision. Uses #25/#26 contracts rather than provider-name heuristics. |
| I3 Inbound and conflicts | Declare attach/import/ingest/detach/resolve commands, attributed inbound events, cursor compare-and-swap and conflict state transitions. | Duplicate/reordered inbound events, identity race, stale revision and audited resolution scenarios; detach leaves history and the external object intact. |
| I4 Outbox and partial delivery | Declare atomic local task+outbox intent commit, per-link dispatch/ack/retry transitions, claim leasing and crash recovery; keep network calls outside the transaction. | Two-provider partial delivery, crash before/after acknowledgement, bounded retry, lease expiry, revoked policy and detached-link cancellation; successful recipients are not redelivered as a new action. |
| I5 Evidence, impact and portability | Bind public provider contract fixtures and deterministic mocks, add source-bound consumer impact, core/provider adapter separation and public evidence admission. | All ten issue scenarios plus negative controls, independently asserted effect traces, provider-contract-change impact witness and portability rows with unsupported guarantees visible. |
| I6 Qualified native validation | Extend maintained Laravel fixture ports/tests and a real transactional DB runner with synthetic state; keep observed Node baseline separate. | Synchronized concurrent identity/lease/version tests, durable outbox restart replay, maintained implementation gate results. Passing mocks does not earn a production/provider E2E claim. |

I1-I5 form the portable pilot; I6 proves the selected target's actual guarantees. If an owner cannot support an increment, deliver an explicit capability gap and block required acceptance instead of substituting prose or a fake pass. These are implementation increments, not changes made by this documentation commit.

## Proposed semantic model and policy boundaries

### Relations, external identity and lifecycle

Proposed Model successor adds optional entity `relations` and `unique_constraints`, preserving existing entities without those members. A relation carries stable `id`, `target`, `local_fields`, `target_fields`, `min`, `max` (null means unbounded), `ownership` and `on_detach`/`on_delete` semantics. A unique constraint carries stable `id`, ordered `fields` and explicit identity comparison rules. Validate field existence/type/arity, referenced scope, min/max, duplicate declarations and projection agreement. Carry these facts into the IR, inspect view, semantic diff and graph reference-role consumers. Do not put them into arbitrary metadata. This is a proposed wire change requiring coordinated Model/IR owner work.

Proposed synthetic IDs and entities:

- `tasklinks.task_external_link` (public label `TaskExternalLink`): stable link id, workspace/task refs, installation id, provider id, object type, external id, sync mode, state, independent inbound/outbound cursors, optimistic version and detached revision. Links refer to one local Task; Task has cardinality `0..N` links. External objects are references, not cascade-owned records.
- `tasklinks.installation`: workspace-scoped installation/provider identity and a credential **handle** only. A handle never contains a token. Synthetic exports use role aliases instead of live handles.
- `tasklinks.conflict`: link/action/field refs, base and observed revisions, bounded classified value references, reason, state and resolution/audit refs. Sensitive conflicting values stay in local classified storage rather than entering public evidence.
- `tasklinks.inbox_receipt`: installation/link/event identity, provider revision, canonical event digest and processing result. Attribution includes originating installation/provider/object and correlation/causation identities.
- `tasklinks.outbox_item`: intent id, originating command, task/link identity, action, policy/capability pins, allowed payload schema/digest, link generation/version, idempotency key, attempt state, next-attempt time, lease/fencing token and receipt/error refs.

Proposed external identity is the exact tuple `(workspace_id, installation_id, provider_id, object_type, external_id)`. The same tuple cannot bind two tasks silently; repeated attach/import to the same task replays the original link/result, whereas another task receives `tasklinks.error.identity_already_bound`. Provider-specific normalization must be declared and version-pinned; comparison is otherwise exact, case-sensitive text. No truncation, case-insensitive collation or prefix-only unique index may stand in for exact uniqueness. Two installations with the same external id remain distinct.

Keep the identity reservation and history when detached. Reattach to the same task uses the retained link with a new generation; cross-task reassignment needs an explicit future transfer contract and is outside this pilot. Propose `sync_mode: disabled|inbound|outbound|bidirectional` and `state: active|paused|conflicted|detached`; delivery states belong to outbox items, so one provider timeout does not detach a link. Delete/detach semantics must preserve audit history and must never issue an external delete implicitly. The old storage fixture's cascade is an example to replace in the new pilot, not a rule to inherit.

### First-class capability and policy decisions

Proposed `tasklinks` integration attachment binds exact project, Model/IR/effect graph, authorization and extended-effect pins. It has closed `providerContracts`, `linkCapabilityRequirements`, `fieldPolicies`, `actionPolicies`, `operationBindings`, `deliveryRules` and `contributions` members. Every provider action names a capability, request/response/error schema, timeout, retry/idempotency guarantee, classification and effect ref. Discovery claims are installation/link-specific and revision-bound; provider brand alone never grants capability.

The decision record binds actor/service actor, workspace/task/link scope, action, field mask, policy revision, capability revision and causation. Capability result is `supported|partial|unsupported|unknown`; authorization result is `allowed|denied`; both must permit the exact action/field set. Partial support needs an explicit permitted subset and applicable waiver/admission; unknown support is not permission. Output a typed `allowed|denied|unsupported|indeterminate` decision and reason refs. Known denial and normal runtime conflict are domain outcomes, not malformed Model diagnostics.

Proposed policies use a closed, typed predicate/reference subset resolved through the authorization owner; no arbitrary expressions, callback strings or provider SDK objects. Reject undecidable required policies as unsupported. Inbound ownership permits selected canonical task fields and explicitly forbids `planner.user_task_planning` fields (`planned_for`, `position`, `focused_at`, `paused_at`). This applies both to live ingestion and conflict resolution. Outbound payload construction is an allowlist projection after the decision, including field-level redaction. Recheck policy, capability, link state/generation and current version before each dispatch attempt; a previously queued action does not retain authority after revocation or detach.

### Commands, effects, conflicts and durable outbox

Proposed domain commands are `tasklinks.attach_link`, `tasklinks.import_link`, `tasklinks.ingest_event`, `tasklinks.request_action`, `tasklinks.detach_link`, `tasklinks.resolve_conflict`, `tasklinks.dispatch_outbox` and `tasklinks.ack_delivery`; queries include `tasklinks.list_links`, `tasklinks.list_conflicts` and `tasklinks.delivery_status`. Commands are Model operations executed by bound runtimes, not additional Lekalo network CLI verbs. Errors include identity-already-bound, scope-denied, action-unsupported, stale-revision, conflict-unresolved, detached-link and delivery-failed, each with an explicit error-contract binding.

Inbound ingestion deduplicates the scoped provider event identity, records attribution, applies allowed fields through the canonical task command and advances its cursor in one local transaction. A provider revision incomparable with the cursor creates an explicit conflict/unknown outcome; it is never sorted lexically into a guessed ordering. Outbound cursor acknowledgement is separate from inbound progression.

Conflict resolution input names the conflict, expected conflict/link versions, resolution (`accept_local|accept_remote|merge_allowed_fields`) and explicit allowed values for a merge. Reuse current actor/field policy, then atomically transition the conflict and write an immutable audit event; any follow-up external update is a new outbox intent under the same authority checks. Stale resolution refuses without overwriting later state. Audit records preserve actor, decision, revisions and causation, while public reports carry synthetic refs and redacted digests.

`request_action` commits the canonical task update, policy decision, per-link outbox intents and local audit together. A denied/unsupported recipient has an explicit decision row and **no** external-call intent. `dispatch_outbox` claims one eligible intent using a fenced lease, calls one provider outside the DB transaction, then records receipt/error and ack independently. Its effect graph shows database writes, durable enqueue/job, external write and partial-failure cases; no atomic group encloses the network call.

Proposed outbox state is `pending|leased|delivered|retry_wait|conflicted|cancelled|dead_letter`. Stable keys include originating intent, link generation and action; retries keep the same key. Per-recipient results distinguish delivered, pending, failed, denied and unsupported; overall delivery is partial while any admitted recipient is unresolved. After provider acceptance but before local acknowledgement, retry queries/reconciles a receipt or uses the provider's deduplication guarantee. If neither exists, report unknown delivery/manual reconciliation rather than exactly-once success. Bounded attempts/backoff/dead-letter use #26's vocabulary; a deterministic injected clock owns tests.

No-loop rule: provider event -> attributed canonical command -> policy decision -> eligible link intents. Preserve originating link, event identity and causation through that chain; an echo cannot create a fresh user intent or resend to its source. Independent user actions may fan out through the canonical task. No adapter calls another provider directly. Unique local inbox/outbox records prove local deduplication only; they do not prove remote exactly-once semantics.

## Proposed CLI, artifact layout and versioning

### Existing CLI reused, with exact syntax

The following grammar exists in `crates/lekalo-cli/src/main.rs`; the proposed fixture root and symbols do not exist yet. Commands are verification examples for the implementation phase and were not executed here:

```sh
lekalo --json --no-cache load --project tests/fixtures/task-external-links/project --ir
lekalo --json --no-cache validate --project tests/fixtures/task-external-links/project --strict
lekalo --json --no-cache inspect tasklinks.task_external_link --project tests/fixtures/task-external-links/project --include bindings,scenarios
lekalo --json --no-cache effects show tasklinks.request_action --project tests/fixtures/task-external-links/project
lekalo --json --no-cache effects show tasklinks.resolve_conflict --project tests/fixtures/task-external-links/project
lekalo --json --no-cache graph callers tasklinks.provider_contract --transitive --project tests/fixtures/task-external-links/project
lekalo --json --no-cache impact tasklinks.provider_contract --depth 256 --profile strict --project tests/fixtures/task-external-links/project
```

`tasklinks.provider_contract` is a proposed typed contract/value symbol referenced by consumer operations; adapter-only contract changes additionally require attachment-aware impact joins. Depth 256 is a query bound, not proof of completeness; the gate must inspect completeness and consumer witnesses. `effects conflicts` concerns overlapping semantic change sets, not runtime provider conflict resolution. Ordinary `validate` does not execute a provider.

### New read-only pilot command surface

Propose `Commands::TaskLinks`, a small CLI adapter in `crates/lekalo-cli/src/task_links.rs`, with pure core services in `crates/lekalo-core/src/task_links/{mod,wire,validate,decision,view,portability,version}.rs`. Proposed exact commands:

```sh
lekalo --json --no-cache task-links validate tests/fixtures/task-external-links/contracts/integration.json --project tests/fixtures/task-external-links/project --profile strict
lekalo --json --no-cache task-links inspect tasklinks.task_external_link --project tests/fixtures/task-external-links/project --attachment tests/fixtures/task-external-links/contracts/integration.json --evidence tests/fixtures/task-external-links/evidence/mock-run.json
lekalo --json --no-cache task-links portability tests/fixtures/task-external-links/contracts/integration.json --project tests/fixtures/task-external-links/project --from-profile tests/fixtures/task-external-links/profiles/laravel.json --to-profile tests/fixtures/task-external-links/profiles/node.json --profile strict
```

`validate PATH` joins the attachment to the current compilation and reports static obligations. `inspect SYMBOL` displays typed relations, decisions, effects and admitted evidence; missing evidence remains unavailable. `portability PATH` joins resolved target/component profiles and provider contract requirements, separating `coreSemantics`, `providerAdapters`, `targetGuarantees` and `evidenceCoverage`. `--profile strict|permissive` controls capability admission; it does not silently alter authorization. Strict denies missing/unsupported required semantics; permissive reports gaps without claiming pass. All reads are bounded and classified; no command runs providers, retries, writes outbox state, imports tokens or stores reports automatically. Normal DomainResult channels/status/exit mapping applies.

| Proposed artifact | Content / owner decision |
| --- | --- |
| `tests/fixtures/task-external-links/project/lekalo/project.yaml`, `modules/tasklinks/{module,entities,commands,queries,policies,events,scenarios,bindings}.yaml` | Canonical synthetic module and explicit imported Task/planner bindings; new Model members from I1. |
| `tests/fixtures/task-external-links/contracts/{integration,authorization,transaction-concurrency,extended-effects,invariant-transition,storage-projection}.json` | Fresh same-revision digest-bound declarations; use accepted owner contracts where expressive. No copied placeholder digest from unrelated fixture roots. |
| `contracts/task-links-integration.schema.v0.8.0.json` | New closed attachment, proposed identity `dev.lekalo.task-links-integration@0.8.0`, discriminator `lekalo/task-links-integration/v0.8.0`. Contains requirements/decisions/bindings, never credentials or provider runtime objects. |
| `contracts/task-links-{report,evidence,portability}.schema.v0.8.0.json` | Closed static report, bounded normalized execution evidence, and core/provider/target portability projection. Family identities/discriminators use the corresponding `task-links-*` stem and `0.8.0`; pins, status/value coupling, unique sets and counts are independently validated. |
| `contracts/model.schema.v0.8.0.json`, proposed new `contracts/ir.schema.v0.8.0.json` | Model schema successor and new explicit compiled-IR schema for relation/secondary uniqueness facts. The inspected base has IR identity/constants in `crates/lekalo-core/src/ir/version.rs`, but no `contracts/ir.schema.v0.2.16.json`. Update actual Model/IR discriminators, parser/compiler constants, exact supported-version registry and dependent pin consumers together. No alias under `0.2.16`. |
| Diagnostic registry / consuming contracts at `0.8.0` | New registry data successor and validation/profile/report/provider consumers whose bytes or required enums/pins change. Registry schema needs a successor only if its shape changes. Preserve the union of existing rules. |
| `tests/fixtures/task-external-links/providers/{github-issues,mock-secondary}/` | Public-provider contract snapshot metadata plus fully synthetic event/request/response/error fixtures and versioned mock capability recipes. Proposed public baseline is GitHub Issues; do not publish a consumer repo or use its events as fixtures. Exact upstream API/version pin must be verified before implementing this adapter. |
| `tests/fixtures/task-external-links/{scenarios,evidence,impact,portability,invalid}/` | Ten issue scenario families, deterministic admitted traces, contract-change pairs, capability matrix and malformed/negative controls. |
| `docs/task-external-links.md`, proposed Rust core/CLI integration tests, new gate scripts below | Eventual user guide and independently asserted validation/delivery/impact/evidence behavior. These are future implementation files, not created here. |

Unchanged Scenario IR `0.2.16`, scenario-run `0.4.0`, extended-effects/authorization/transaction/invariant contracts `0.2.16`, storage-projection `0.4.0`, graph/effects/protocol/lock/authority/privacy contracts stay frozen if their bytes and semantics do not change. Fixture-instance pins may change without rewriting those schemas. Existing closed pin enums may need successors to admit Model/IR `0.8.0`; audit each consumer, and bump **only** the changed contract to `0.8.0`. Wire-shape changes to graph/inspect/diff/provider/CI report likewise require explicit successors; do not conceal new fields in an old closed schema. Refresh affected hashes and consumer references using each owning algorithm; do not globally reserialize frozen contracts.

Before an implementation commit, establish Cargo product `0.8.0` and run `node scripts/check-contract-versions.mjs`; after committing run `node scripts/check-contract-versions.mjs --base HEAD^`. A provider domain/API version pin is separate from Lekalo product/contract versions. New upstream snapshots require explicit reviewed pins, not live unbounded fetches in CI.

## Diagnostics (LEK-TASKLINK-*) proposal

`TASKLINK` fits the existing diagnostic family grammar. Proposed allocations below are not reserved; recheck both live registry routes and all other M8 additions before implementation. Domain errors for normal denied, duplicate or conflicting actions remain in the domain error contracts, while these diagnostics validate declarations/evidence.

| Proposed code / rule id | Trigger | Category / default severity |
| --- | --- | --- |
| `LEK-TASKLINK-001` / `tasklink.relation-identity-invalid` | Missing/invalid relation cardinality, identity field, installation scope or Model/storage uniqueness disagreement. | model / error |
| `LEK-TASKLINK-002` / `tasklink.decision-incomplete` | Required action/field decision lacks actor scope, explicit policy, capability or revision-bound decision input. | security / error |
| `LEK-TASKLINK-003` / `tasklink.fanout-not-admitted` | A declared/evidenced outbound effect targets a denied, detached, unsupported or unknown link/action/field. | security / error |
| `LEK-TASKLINK-004` / `tasklink.delivery-contract-invalid` | External write is declared atomic with local state, durable intent is missing, retry lacks dedup/reconciliation, or partial recipients are masked as complete. | semantic / error |
| `LEK-TASKLINK-005` / `tasklink.conflict-resolution-invalid` | Resolution lacks explicit command/precondition/policy, immutable audit effect, or a valid transition. | semantic / error |
| `LEK-TASKLINK-006` / `tasklink.history-or-ownership-violation` | Detach deletes external/history data, ingestion/resolution writes protected planning fields, or a direct provider-to-provider bridge is declared. | semantic / error |
| `LEK-TASKLINK-007` / `tasklink.required-guarantee-unsupported` | Selected adapter cannot supply required identity, transaction, policy, durable outbox or delivery guarantees. Unknown and partial retain explicit reasons. | adapter / error in strict |
| `LEK-TASKLINK-008` / `tasklink.evidence-invalid` | Evidence has stale/wrong pins, fabricated counts/coverage, missing source attribution, or an unadmitted stronger claim than its mock/native receipt supports. | compatibility / error |

Reuse existing schema/normalization diagnostics for malformed JSON, unsupported versions and resource limits; reuse privacy diagnostics for token/private identity rejection rather than allocating a second secret scanner. Each new rule needs message catalog identity, allowed terminal statuses, source/evidence locations and closed safe data fields. Data may contain synthetic semantic refs and expected/observed state tokens, never raw provider payload/token/consumer names. Add minimally differing trigger/non-trigger pairs that reach the rule's stage, update validation profiles, and refresh registry/fixture coverage pins and paired run receipts. If the diagnostic envelope changes its registry pin, publish its required `0.8.0` successor; leaving a new code outside the negotiated registry is invalid.

## Fixture and gate families

### All ten issue scenarios

| Verbatim scenario from #52 | Proposed deterministic fixture and independent assertions |
| --- | --- |
| one task linked to two providers; | `two_providers`: one Task, distinct installations/links, one allowed action produces two independently attributed intents/receipts. |
| duplicate link/import is idempotent; | `duplicate_import`: repeat the same scoped external identity and idempotency key; one link, unchanged history and replayed result. Include concurrent duplicate imports in native tests. |
| same external object cannot bind two tasks silently; | `identity_collision`: second Task in the same tuple scope receives the exact identity-already-bound error and creates no second link. Different installation is a positive control. |
| inbound events aggregate with attribution; | `attributed_inbound`: both providers produce permitted task updates; event/link/installation/source revision remain distinguishable, and duplicate events do not repeat effects. |
| fan-out only for supported/allowed action; | `decision_matrix`: capability and policy independently allow/deny/unknown/partial; only admitted fields/actions create intents and provider calls. |
| partial provider failure; | `partial_delivery`: provider A succeeds, B times out; status stays partial, A's receipt persists and only B is retry-eligible. |
| stale/conflicting update; | `stale_conflict`: outdated cursor/version creates a conflict; stale resolver cannot overwrite, fresh explicit resolution records audit and permitted effects. |
| detach preserves history/external object; | `detach_history`: link becomes detached, prior receipts/conflicts/history and remote object remain; pending dispatch is cancelled/refused and no external delete occurs. |
| retry does not create loop/duplicate; | `retry_echo_crash`: retry retains its key, duplicate echo does not spawn a new intent, and acceptance-before-ack uses reconciliation or explicit unknown delivery. |
| provider token never enters payload/evidence. | `secret_canary`: synthetic credential token at runtime boundary is absent from payload, evidence, error, stdout/stderr, logs and exports; an attempted export is rejected/redacted by the privacy owner. |

Additional fixtures cover cross-workspace actors, installation revocation, field-policy changes between enqueue/dispatch, lease fencing/restart, history-preserving reattach, non-comparable provider revisions, byte-distinct external identities, invalid cardinality, digest-valid but schema-invalid evidence and missing capability receipts. Token canaries are synthetic markers, never credentials. Assert exact state/effect sets and absence of forbidden effects; do not merely snapshot the implementation's own summaries.

### Gate ownership and concrete commands

Proposed independent scripts:

| New gate | Mandatory checks |
| --- | --- |
| `scripts/test-task-links-contracts.mjs` | Draft 2020-12 schemas under exact Ajv 8.17.1, closed unions/unknown fields, version/identity/pin parity, live CLI validate/inspect/portability, all diagnostic trigger/control pairs. Missing binary is a failure. |
| `scripts/test-task-links-scenarios.mjs` | All ten stable scenario ids, source-bound mock receipts and independent field/effect/idempotency assertions; unsupported required assertions fail acceptance. No hidden network. |
| `scripts/test-task-links-outbox.mjs` | Deterministic failpoints and echo/retry/lease state machine; accepted-before-ack uncertainty, recipient accounting, no loop/new key on retry. |
| `scripts/test-task-links-impact-portability.mjs` | Mutate one exact provider contract field/capability and follow consumer operation -> policy -> worker/binding -> scenario/test/check with witnessed completeness; compare unchanged core to changed provider/target requirements. |
| `scripts/test-task-links-native.mjs` | Provisioned maintained Laravel/transactional DB execution with synchronized competing workers and process restarts; mock-only or missing DB/toolchain never satisfies durable/race assertions. |
| `scripts/test-task-links-public-evidence.mjs` | Public-provider/synthetic provenance, classification/export admission, canary absence across every output/error/log path, no private application/repository identity, trace/pin revalidation. |

Proposed implementation verification sequence from repository root:

```sh
cargo build --workspace --locked
cargo test --locked -p lekalo-core --test task_links
cargo test --locked -p lekalo-cli --test task_links
node scripts/test-task-links-contracts.mjs
node scripts/test-task-links-scenarios.mjs
node scripts/test-task-links-outbox.mjs
node scripts/test-task-links-impact-portability.mjs
node scripts/test-task-links-public-evidence.mjs
node scripts/test-task-links-native.mjs
node scripts/test-fixture-provenance.mjs
node scripts/test-golden-diagnostic-coverage.mjs
node scripts/check-contract-versions.mjs
```

Provision exact Ajv through `LEKALO_AJV_NODE_PATH` and `NODE_PATH` before these commands, following current CI; new gates do not install dependencies. Re-run the existing Model/IR, scenario, authorization, transaction, extended-effects, storage and privacy gates whose owners change. Add schema-only checks in the existing Node contract lanes; place binary-dependent checks after the workspace build in every applicable OS lane. Put actual Laravel/DB tests in a qualified native job with pinned dependencies, fresh synthetic DB seeds and explicit required capabilities. Upload only admitted public normalized receipts; private payloads/logs stay outside artifacts. Workflow wiring is not itself native or hosted acceptance.

Register the new synthetic family in `tests/fixtures/fixture-provenance.json`, the suite catalog and diagnostic-rule coverage index; recompute affected registry digests and paired execution receipts. Public provider snapshots, synthetic mocks and real native runs have different evidence origins and must remain labeled. Real authenticated provider behavior is later explicitly provisioned evidence, not an ordinary CI requirement or a stronger claim inferred from public docs.

## Determinism and portability strategy

Inject clock, IDs, provider responses/revisions, failpoint schedule and lease timings. Canonical sets sort by semantic identity; ordered scenario steps remain ordered. Preserve owning contracts' canonicalization algorithms. Repeated runs from separate temporary roots must produce equal canonical Model/IR/decision/effect/evidence bytes, without updating expected files; paths and native timestamps are separate normalized views. Bound attachment/evidence size, recipient/field counts, traversal and retries explicitly; exhaustion is incomplete/error, never empty successful delivery.

Portability rows name the unchanged domain fact (identity, 0..N, detach/history, actor/field policy, version conflict, idempotent intent), provider adapter facts (object type, field mapping, revision ordering, request/error schemas, remote dedup/receipt capability), and target facts (unique indexes, compare-and-swap, transaction and durable outbox guarantees). Each row has required/provided capability, supported/partial/unsupported/unknown result, exact pins and evidence source. Node/Laravel comparison fixtures do not assert actual equivalence unless both native bindings execute the same admitted scenario with matching normalized assertions.

## Test plan mapping each acceptance criterion

The criterion column is verbatim from the live issue, including its mixed language. Every row is a proposed acceptance obligation, not a completed result.

| Verbatim acceptance criterion | Increment / required witness |
| --- | --- |
| Relation cardinality/identity constraints выражены в Model. | I1: canonical Model/IR contains typed Task -> links `0..N`, link -> Task cardinality/scope and exact external-identity constraint. Trigger/control fixtures and a synchronized native uniqueness race reject a second task binding. Storage projection alone is insufficient. |
| Capability/policy decisions first-class, not prose only. | I2: inspect/report contains typed actor/scope/action/field/capability decisions with revision pins; matrix scenarios prove denied/unsupported/unknown recipients produce no forbidden intent/call, including revocation between enqueue and dispatch. |
| External writes and partial failure represented in effect graph. | I4: declared external-call/job/local-write edges and partial-failure case refs bind the same inputs; two-provider execution produces one delivered and one unresolved recipient with preserved receipts and separate local/network boundaries. |
| Conflict resolution has explicit commands/audit effects. | I3: `tasklinks.resolve_conflict` and its expected-version/actor/field-policy input, allowed transitions and audit event are first-class; stale and successful resolution scenarios check exact state and audit effects. |
| Scenarios use public provider + synthetic mock evidence before broader claims. | I5: GitHub Issues public contract snapshot/version metadata plus synthetic primary/secondary provider fixtures cover all ten scenarios; evidence labels mock/reference/native origins and exact pins. Missing native/provider guarantees remain unsupported/unknown, never wider acceptance. |
| Impact follows provider contract changes to consumers. | I5: same-base change pair alters a specific provider request/field/error/capability contract; reverse impact witnesses reach consumers, action policy, outbox worker, native bindings and verification scenarios/checks. Validate attachment joins and completeness, not just a generic Task node. |
| Portability report separates core semantics from provider-specific adapters. | I5/I6: closed portability report separates core, provider and target rows with capability/evidence states; a provider-only change preserves core facts, while missing remote dedup or target isolation explicitly changes guarantee coverage. |
| Public artifacts не раскрывают название consumer application. | I5: synthetic aliases only in model/fixtures/reports/trace/logs; privacy canary/admission gate and public-artifact scan reject private names/URLs/tokens, including failed runs and conflict data. No private consumer access is required for public fixture acceptance. |

### Research validation boundary

This phase read the live issue and its direct context links, three M7 research files, versioning rules, relevant owner source/schema/CLI declarations and synthetic fixture references. Documentation checks require all eight exact criterion strings, all ten scenario mappings, existing owner path resolution, proposed/current separation, whitespace and the single-file staged/HEAD boundary. No binary build, Rust/native/provider test, package installation, fixture generation, DB access or hosted CI occurred here. The dispatch acceptance is this research document in HEAD with the M7 research shape; implementation and product acceptance remain the increments above.

## Risks and non-goals

- Model/IR relation additions have broad pin/consumer impact. Audit closed enums and negotiate successors before joining old attachments to new IR; a contract-valid digest alone does not establish compatibility. Preserve unrelated M8 registry additions when integrating.
- Identity comparison must survive actual storage collation/index behavior. Installation/provider/object scoping, full-key uniqueness and concurrency require independent DB tests; a shortened MySQL prefix key can reject distinct identities or weaken intended comparisons.
- Source private requirements are unavailable in this task. Identity transfer, task deletion/retention and provider-specific merge semantics need explicit contracts before expansion; this proposal conservatively retains detached reservations/history.
- Authorization is distinct from provider capabilities. Stored decisions expire with policy/link/capability revisions, and retries must not turn revoked actions into external writes.
- Outbox durability and provider delivery have different failure boundaries. Neither a local transaction nor two mock success responses prove exactly-once remote writes; unknown delivery needs visible reconciliation obligations.
- Existing reference evaluator support is insufficient for all new assertions. Required authorization/race/durable delivery checks need qualified execution, with unsupported preserved until then.
- Public provider snapshots need verified upstream version/license/provenance before implementation. Synthetic replay does not establish real webhook ordering, rate limits, permissions or idempotency.
- Privacy covers every artifact path, including conflict values, failures and logs. Consumer-name disclosure is separately forbidden even when the data contains no token or PII.
- Non-goals: a production synchronization service, credential/OAuth UI, hidden live provider access, a provider-to-provider bridge, silent external deletion, implicit identity transfer, full private consumer modeling, rewriting #49 observed code, changing #50 planning ownership, broader frontend work, or a claim of Node/Laravel/production equivalence from mocks.

No implementation or contract mutation is authorized by this document; the only delivered change is the research brief and its local commit.

## Fix-round addendum: AC1 disposition (implementation round 2, 2026-10-07)

The review of the first implementation round returned CHANGES-REQUESTED on two alternative readings of AC1. The reviewed round expressed the typed `Task -> links 0..N` relation and the exact external-identity uniqueness in the closed integration attachment, not in a negotiated Model/IR successor, and the research brief above (line 34, line 119, line 211) asked for `model.schema.v0.8.0.json` plus an explicit IR schema before claiming AC1.

This fix round resolves the deviation explicitly rather than silently: AC1's cardinality/identity semantics are enforced at the attachment level and made compilation-bound, and the Model/IR successor remains negotiated owner work. Concretely, the attachment now (a) pins its `irRef.digest`/`modelRef.irDigest` to the recomputed canonical IR of the live compilation and refuses a divergent compilation, (b) recomputes its own `attachmentRevision` from its canonical bytes and refuses fabrication, (c) pins every owner contract against the exact accepted file bytes in `contracts/`, (d) requires the exact ordered five-part identity tuple under the exact scope on the link entity (a wrong tuple, a `declared` scope without the tuple, or a missing `externalIdentity` binding all invalidate), (e) checks relation `localFields` against the declaring link entity and `targetFields` against the relation target, and (f) requires the relation `max` member at runtime parity with the schema. The negative controls for each of these refusals are registered trigger fixtures and the trigger matrix grew accordingly.

The remaining gap to the literal AC1 text — the same two constructs carried inside `model.schema.v0.8.0.json` and a compiled-IR schema, with inspect/semantic-diff/graph consumers reading them from the IR — is **explicitly renegotiated with the issue owner** through this addendum: it is a coordinated Model/IR owner change (loader version dispatch, embedded version registry, every downstream pin consumer), and doing it inside this fix round would have re-versioned the shared Model family unilaterally. Until that successor lands, the compilation-verified attachment declarations above are the enforced AC1 semantics, and any drift between the attachment and the compiled entities is a refusal, not a silent pass. If the issue owner ratifies the attachment-level disposition, this addendum is the trace; if the owner requires the Model successor, this section is the handoff scope.
