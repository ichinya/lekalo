# Issue #113: versioned module public API and dependency boundaries research

Research only, 2026-10-06. Acceptance authority: [issue #113](https://github.com/ichinya/lekalo/issues/113), read live with `gh issue view 113 --repo ichinya/lekalo --json title,body,labels`. Parent #1 places this issue in M8; dependencies #5, #6, #13, #18, #62, #64, #65, #77 and #84 were read only for context. This document follows the evidence/proposal/inventory/diagnostics/acceptance/risk shape of `docs/m7/issue-34-research.md` and `docs/m7/issue-77-research.md`; #84's research headings also informed the profile integration map. Context7 resolution for `Lekalo` and `ichinya/lekalo` returned unrelated projects; none was used as CLI documentation. Current CLI facts come from this checkout's primary source.

Checkout: branch `ichinya/m8-issue-113`, supplied `ichinya/M8` revision `15bd13917654dc20ad19b089a5bb8e5e29c2099f`, initially clean. The inspected Cargo product version is still `0.6.5`; the assigned M8 implementation product version is **`0.8.0`**. This research does not bump Cargo. Following `docs/versioning.md`, every new or changed contract in the proposed implementation takes `0.8.0`; unchanged contracts retain their exact accepted versions and bytes. Module API SemVer, definition revision integers, and Lekalo product/contract versions are separate quantities.

Decision: propose an independent, explicitly supplied **module API attachment** over accepted Model/IR, a standalone portable public bundle, and pure boundary/diff/context services. An allowlist defines exports; project visibility alone cannot create an export under the new boundary policy. Imports, public dependency pins, consumer uses, effects and shared-aggregate exceptions are explicit. Preserve the existing semantic graph, error/query/storage owners, and use their typed facts rather than create another compiler or infer imports from paths. The same public bundle is usable by an in-process consumer and an extracted-service consumer.

All new paths, APIs, CLI flags, schema members, diagnostic allocations and tests below are **proposals**, not implemented behavior or reserved codes. Only `docs/m8/issue-113-research.md` may change in this task. No implementation, contract/fixture edits, adapter execution, service deployment, external publication or push is part of this research.

## Existing-surface inventory

| Surface | Inspected owner and searchable anchors | Consequence for #113 |
| --- | --- | --- |
| Model and typed modules | `contracts/model.schema.v0.2.16.json`; `crates/lekalo-core/src/ir/mod.rs`: `Module { id, common, imports }`, `Common`, `Visibility::{Module,Project}`, `Portability::{Portable,TargetSpecific}`. `Common.version` is an unsigned definition revision. | There is no explicit export allowlist or independent module API SemVer in this module struct. Add a bound attachment without inserting unsupported fields into frozen Model/IR. |
| Fields and type shapes | `ir/mod.rs`: `Field`, `TypeRef::{Ref,List,Optional}`, entity/value-object/enum definitions. Model fields have no independent visibility property. | Exporting a named type exports its declared semantic shape. A narrow public DTO must be a distinct authored value-object/view, not an implicit removal of fields from the same type identity. |
| Existing semantic validation | `crates/lekalo-core/src/validator/rules.rs`: `cross_module_visibility`, `public_output`, `portability`; `validator/profile.rs`. Private cross-module references are checked directly; public-output checking here is query-return-specific; portability checks direct references. | Reuse direct findings; add explicit-export enforcement and complete transitive public signature closure across all exported kinds and applicable attachments. Do not claim the existing direct rules already enforce #113. |
| Existing rule identities | `contracts/diagnostic-registry.v0.6.4.json`: `semantic.visibility-boundary-violation` / `LEK-SEM-020`, `semantic.public-output-private-type` / `LEK-SEM-013`, `semantic.portable-target-reference` / `LEK-SEM-012`, `query.visibility-boundary` / `LEK-QRY-008`. Strict validation profile enables the semantic rules. | Preserve identities and distinguish already diagnosed direct violations from new missing-export/transitive findings. Severity alone does not determine process exit. |
| Graph and cycles | `crates/lekalo-core/src/graph/{build,model,traverse,cycle,slice}.rs`: `build`, `DependencyGraph`, adjacency, relation/reference roles, `find_all`, `ModuleBoundary`. `requires`/`derived_from` cycles are fatal; other relations can cycle. `build` returns no graph on a forbidden cycle. | Import-cycle witnesses must be obtained before successful graph construction is required. A module projection of semantic dependencies needs its own labeled SCC/witness view. Registered `writes` is not proof that the canonical builder emits write facts. |
| Effects and ownership | `crates/lekalo-core/src/effects/{mod,kind,index,conflict}.rs`: `EffectGraph`, writer/reader indexes, resource/field scope; `transaction_concurrency/` owns atomicity/concurrency. | Validate cross-owner direct writes using actual typed effect facts, including create/update/delete and entity-wide/field overlap. A transaction declaration or shared type does not grant write permission. |
| Error contracts | `crates/lekalo-core/src/error_contract/{mod,types,diff}.rs`: `OperationErrorContract`, `ErrorUnion`, `ErrorContract`, public/private payload exposure, typed result binding and separate infrastructure channel. Contracts remain `0.2.16`. | Export declared public errors and result signatures through exact owner pins; exclude private messages/payloads. A diagnostic ID is not a domain error ID. |
| Query and storage attachments | `query_model/{mod,resolve,query,diff}.rs`: `QueryModelAttachment`, typed selections/cardinality/pagination; `storage_projection/` owns domain-to-storage mapping and storage diff. | Use public result/query semantics without including SQL/ORM/table/technical columns. Internal read implementation dependencies are not consumer signature dependencies. |
| Event/port/capability facts | `extended_effects/mod.rs`: `ExtendedEffectsAttachment`, `CallContract`, `EventContract`, `CapabilityRequirement`; `target_profile/`, `transport_http/` and `target_protocol/` own target mapping/negotiation. | Reference current event/call contracts and capability IDs. A public service-port descriptor must reference semantic input/output/error/effect shapes; class names and wire routing stay in adapter mappings. |
| Semantic diff and identity | `crates/lekalo-core/src/diff/{mod,projection,identity,compatibility}.rs`: `compare`, `ProjectRef`, `Subject`, semantic projection; `ir/mod.rs::IdRegistry` has declared rename history/tombstones. | Reuse member changes/classes and accepted rename evidence. Current diff does not publish a module export contract or exact external consumer registry; add a separate report instead of widening its closed schema implicitly. |
| Coupling | `crates/lekalo-core/src/coupling/`, CLI `Commands::Coupling`, `contracts/coupling-report.schema.v0.6.4.json`; `coupling.cross-module-cycle` / `LEK-COUPLING-005`. | #77 is implemented in this checkout, unlike its historical research base. Reuse paths/coverage conventions, but a broad reverse radius is not an exact set of consumers of one public member. Boundary legality is distinct from advisory coupling metrics. |
| Capsules and budgets | `crates/lekalo-core/src/context/{mod,select,model,version}.rs`: `plan` takes a full `Compilation` and builds graphs; `CapsuleScope`, `Capsule`. `context_budget/` owns forward understanding metrics. `contracts/context-capsule.schema.v0.2.16.json` is closed. | A provider-contract-only consumer needs a new typed view. Do not call the existing full-compilation planner with fabricated provider internals or add keys to its frozen output. |
| Architecture profiles | `crates/lekalo-core/src/architecture_profile/{mod,wire}.rs`; catalog and profiles `v0.6.5`. Explicit scoped evidence/policy, separate from validation profiles. | Integrate boundary evidence through successor catalog/profile/report artifacts only where changed. Opt-in scopes cannot silently weaken exported semantics. |
| Registry and result contract | `diagnostics/registry.rs` embeds `diagnostic-registry.v0.6.4.json` and its `v0.6.5` successor; `result.rs::DomainResult`, `Status`. `0` valid/stdout, `1` invalid/stderr, `3` denied/stdout, `4` unsupported/unavailable/stdout, `5` unsupported-version/stderr. | Add one union-preserving `0.8.0` registry successor and update active consumers deliberately. Both inspected registries contain no `LEK-MODULE-*` allocation. Reuse the diagnostic wire when its shape stays unchanged. |
| CLI | `crates/lekalo-cli/src/main.rs`: `Commands::{Validate,Diff,Context,Coupling,ArchitectureProfile}`, `run_validate`, `run_diff`, `run_context`. Global `--json`/`--no-cache`; project selectors are command arguments. | Add a small `module_api.rs` CLI owner plus explicit dispatch/flags. New read-only commands must not inherit hidden cache persistence, generation or adapter execution. |
| Contract versions | `scripts/check-contract-versions.mjs`; `versioning/{family,registry}.rs`, exact version registries; `docs/versioning.md`. | New families use `0.8.0`, with exact identity/discriminator/hash/runtime pins. Module API range policy cannot turn exact wire-contract negotiation into guessed ranges. |
| Fixtures and gates | `tests/fixtures/{graph,impact,context,context-budget}/planner/`, `tests/fixtures/pilot/taskhub/`; `scripts/test-{model,graph,effect-graph,semantic-diff,context,coupling,architecture-profile}-contracts.mjs`. | Reuse source shapes and owner tests; create freshly bound synthetic Task/provider/planner fixtures in the new family, not combinations of unrelated fixed digests. |
| Suite and CI | `tests/fixtures/fixture-provenance.json`, suite catalog/run-manifest/diagnostic coverage; `.github/workflows/ci.yml` provisions Ajv 8.17.1 and builds the workspace before coupling/architecture binary gates. | Add schema fixtures to schema lanes and real-binary gates after the build. Update diagnostic coverage/provenance with the union registry, retaining existing lanes. |

## Proposed command/report schema and ownership

### Proposed CLI grammar

The commands below are exact proposed argv, not current invocations. `DIR`, `PATH` and IDs are illustrative operands; files are explicitly supplied and project-relative reads are bounded.

```sh
lekalo --json --no-cache module api check --project DIR --api PATH --strict
lekalo --json --no-cache module api check --project DIR --api PATH --module planner --strict --lock PATH
lekalo --json --no-cache module api export tasks --project DIR --api PATH
lekalo --json --no-cache module api consumers tasks --project DIR --api PATH
lekalo --json --no-cache module api lock --project DIR --api PATH
lekalo --json --no-cache module api lock --project DIR --api PATH --lock PATH --check
lekalo --json module api diff BASE_PUBLIC CANDIDATE_PUBLIC --consumers-before BASE_CONSUMERS --consumers-after CANDIDATE_CONSUMERS --strict
lekalo --json module api context --public TASK_PUBLIC --consumer planner --uses tasks.task_view,tasks.get_task --budget 4000
lekalo --json --no-cache validate --project DIR --strict --module-api PATH --module-api-lock PATH
```

Proposed commands and flags:

| Surface | Exact meaning |
| --- | --- |
| `module api check --project DIR --api PATH [--module ID] [--strict] [--lock PATH]` | Validate the whole bound project, all imports and export closures, applicable attachments, effects and consumers. `--module` filters owned diagnostics but retains mandatory cross-module errors. Default is advisory for well-formed boundary findings; strict rejects exact semantic violations and blocks required evidence gaps. Malformed inputs always fail. |
| `module api export MODULE --project DIR --api PATH` | Emit the complete public bundle for one exact module ID. Incomplete/illegal public closure cannot produce a certified bundle, even without `--strict`. No default file output and no storage/provider implementation expansion. |
| `module api consumers MODULE --project DIR --api PATH` | Emit an immutable snapshot of declared consumers and actually resolved uses, with project/graph/API pins, witness refs and coverage. Scope includes declared zero-use consumers so subscriptions/ranges are not lost. |
| `module api lock --project DIR --api PATH [--lock PATH --check]` | Without `--check`, print a candidate consumer lock payload; never create a file. `--check` requires `--lock` and compares exact accepted pins/ranges against current input. `--lock` without `--check` is usage-invalid. No reuse of `lekalo lock`'s adapter lock meaning. |
| `module api diff BASE_PUBLIC CANDIDATE_PUBLIC [--consumers-before PATH --consumers-after PATH] [--strict]` | Compare two exact public bundles for the same semantic module. Consumer snapshot arguments are a pair; absence means public changes are known but consumer coverage is unknown. Strict denies incompatible declared consumer/version/deprecation policy and required incomplete coverage. Both snapshots bind their respective side, not identical revision digests. |
| `module api context --public PATH --consumer MODULE --uses IDS --budget N` | Consume only the standalone public bundle. Exact comma-separated export IDs, nonempty/unique after normalization; no short-name guesses. Budget is explicit and bounded. No `--project`, `--api`, provider checkout, adapter, cache or network is required. Human output is Markdown; JSON emits the independent consumer capsule. |
| `validate --module-api PATH [--module-api-lock PATH]` | Explicit integration seam; the lock flag requires the attachment. Existing validation without these flags preserves existing accepted behavior. `--strict` selects blocking boundary policy only when the boundary seam is selected. |

`check` and validation retain existing Model/IR diagnostics. Exact malformed selectors, contradictory pins, unknown fields, duplicate JSON keys and unresolved identities are invalid/exit 1; unsupported family versions are exit 5. Exact strict boundary violations are invalid/exit 1. Policy failures against otherwise valid public APIs/consumer locks are denied/exit 3 with full evidence; required unavailable inputs/capabilities are exit 4. Advisory reports remain exit 0 with explicit `assessment: indeterminate` for optional gaps. Strict missing consumer evidence must never return pass. Non-fitting context returns a valid capsule with `fits:false`; a downstream consumer task must check that field before proceeding.

No command writes canonical Model, lock, reports or baselines, invokes a target, installs anything, resolves a network dependency or changes Git state. All reads use the existing classified filesystem boundary; core receives immutable typed values. Users can persist stdout deliberately through their own authorized workflow. A public **semantic API** is not permission to export private repository data; any new privacy/authority artifact admission needs explicit successor review.

### Proposed artifacts and owners

| Proposed path/artifact | Contents and implementation responsibility |
| --- | --- |
| `contracts/module-api.schema.v0.8.0.json` | Source attachment: bound project/Model/IR/attachment pins, exact module IDs, exports, imports/consumer policies, API version policy, deprecations and shared-aggregate grants. Immutable typed decoding; no arbitrary JSON policy bags. |
| `contracts/module-public-contract.schema.v0.8.0.json` | Standalone module public bundle: exported signatures and referenced portable type closure, public errors/events/ports/capabilities/effect summaries, exact public dependencies and deprecation metadata. Must work without the source project. |
| `contracts/module-api-report.schema.v0.8.0.json` | Boundary assessment/coverage, typed findings and graph witnesses, input pins and resolved public/import sets. Successful and denied projections retain evidence; no embedding into frozen graph output. |
| `contracts/module-api-consumers.schema.v0.8.0.json` | Closed declared/actual-use consumer inventory for one producer revision: consumer/module IDs, policy/range, exact symbol/member uses, direct/transitive distinction, original edge keys, evidence origin and completeness domain. |
| `contracts/module-api-diff.schema.v0.8.0.json` | Before/after public pins, per-change compatibility classes/reasons, exact affected consumer sets, possible/unknown sets, version/deprecation policy decisions and migration hints. |
| `contracts/module-api-lock.schema.v0.8.0.json` | Consumer resolutions: semantic provider module, optional API SemVer, public digest, exact dependency bundle pins, declared range policy and selected public IDs. Independent of the frozen adapter `lekalo.lock` contract. |
| `contracts/module-consumer-context.schema.v0.8.0.json` | Required/supplemental public facts, consumer/use selectors, budget/estimator pin, manifest, `fits`, minimum required cost, omissions and evidence gaps. No private implementation card or storage mapping. |
| `crates/lekalo-core/src/module_api/{mod,wire,version,resolve,public,validate,graph,effects,consumers,diff,lock,context,canonical,diagnostic}.rs` | Proposed pure APIs `resolve`, `check`, `export_public`, `consumer_snapshot`, `compare_public`, `resolve_lock`, `plan_consumer_context`; inputs are accepted typed compilation/attachment values or decoded public bundles. Core owns semantic closure/policy, not filesystem or subprocesses. |
| `crates/lekalo-cli/src/module_api.rs`, `main.rs`, `crates/lekalo-core/src/lib.rs` | Proposed argument structs, bounded file loading, fresh input checks, `Commands::Module` dispatch, renderers and explicit validator handoff. |
| `docs/module-api.md`, `docs/cli.md`, `docs/versioning.md` | Future normative grammar, examples, SemVer/consumer policy, disclosure distinction and in-process/service mapping. These are implementation touch points, not files changed by this research. |
| `crates/lekalo-core/tests/module_api.rs`, `crates/lekalo-cli/tests/module_api.rs`, `tests/fixtures/module-api/`, `scripts/test-module-api-contracts.mjs` | Proposed own fixture/gate family covering public CLI and independent semantic assertions. |

Each new family `X` uses identity `dev.lekalo.X@0.8.0`, discriminator `lekalo/X/v0.8.0`, and a versioned schema `$id` following the existing contract naming convention. Publish closed nested objects, bounded arrays/strings and exact external schema pins. Strict decoding rejects duplicate keys before typed deserialization. Schema validation is necessary but separate from reference, closure, policy, range and arithmetic invariants.

Required changed artifacts: `contracts/diagnostic-registry.v0.8.0.json`, `contracts/validation-profile.{default,strict}.v0.8.0.json` and `contracts/validation-report.schema.v0.8.0.json` for new profile/registry pins and explicit boundary integration; update their runtime consumers together. The registry schema and diagnostic wire can remain at their accepted versions if their shape is unchanged. Exact support registration must be added in `versioning/{family,registry}.rs` and a current-product successor registry where applicable.

When adding architecture rule/evidence integration, publish changed `architecture-rule-catalog`, `architecture-profile`, `architecture-profile-report` and associated schema successors at `0.8.0`; leave untouched architecture artifacts frozen. Provider discovery of the new operation family similarly requires `provider-capabilities.schema.v0.8.0.json` and producer/consumer pins when advertised. Do not advertise flags or evidence before implementing them. No Model, IR, graph, semantic-diff, effect, error, query, storage, legacy capsule, adapter-lock or target-protocol schema change is required by the attachment design. If implementation chooses to modify any of these closed wires, that family must also move to `0.8.0` with all references updated; editing old schema bytes in place is prohibited.

## Proposed public surface and boundary rules

### Attachment and standalone bundle

Proposed attachment entries are keyed by the exact existing semantic module ID. `exports` is an explicit set of typed references to definitions owned by that module or to a separately pinned error/port contract; no wildcard, implicit re-export, case-folding, directory convention or adapter-invented canonical ID. Visibility `module` contradicts export and is invalid. Omitted/project visibility is merely eligible: an unlisted symbol remains internal under this boundary. Local references stay legal.

Proposed `apiVersion` is either canonical stable SemVer or null. Null selects **digest-only** policy, supported for unversioned modules without claiming compatibility from the product version. A versioned module can remain at `1.2.0` while Lekalo uses product/contract `0.8.0`; integer definition `version` remains separate. A changed public digest never silently inherits an old consumer resolution. Consumer policy accepts one of `exact-digest`, `exact-version` or `version-range`; the latter uses a deliberately bounded interval with explicit lower/upper values and inclusive flags, not arbitrary package-manager syntax. Prerelease/range syntax beyond the selected policy is unsupported, not approximated. Existing `Module.imports` must agree with attachment imports; the attachment adds policy and symbol-use detail, not hidden new imports.

The standalone bundle materializes public semantic facts, with exact originating schema/digest pins, instead of requiring the consumer to dereference the provider's private source. Its closure traverses type/parameter/result/error/event/port references and public capability/effect obligations. A foreign public type requires an explicit public dependency bundle pin; a private local helper, target binding, implementation body, SQL/ORM mapping or internal operation read dependency cannot become a public contract edge. A module may depend privately on another module without re-exporting its API. Unknown coverage remains explicit.

| Issue public-surface item | Proposed representation and closed rule |
| --- | --- |
| exported types/value objects/enums | Explicit exported semantic IDs and complete public shapes, including required/nullability/enum semantics. Never export an internal entity with selected fields silently stripped; author a distinct public value object. |
| commands/queries/events | Explicit operation IDs, input/result/error/event payloads and observable effects. Queries include public cardinality/pagination/consistency policy from the query attachment; private query plans stay internal. |
| error contracts | Export public stable error codes/payload shape/retry metadata through #62. Private messages, debug payloads and stack data are excluded; typed unknown infrastructure failure stays separate. |
| endpoint/service ports | Public endpoint semantic contract or attachment-defined port descriptor with stable namespaced ID and typed operation references. Port IDs live in the attachment, not a fabricated new Model definition kind. Transport mapping remains separate. |
| capability requirements | Exact capability IDs and required/optional obligation with owner/version pin. A declaration is not proof that the selected adapter supports it. |
| compatibility/version policy | Optional independent `apiVersion`, digest-only mode, compatibility axes and explicit pre-1.0 bump policy. Semantic and wire version identities are different. |
| allowed cross-module effects | Consumer-visible allowed effect signatures plus narrow shared-aggregate grants. An export does not grant direct storage write authority. |
| consumer declarations | Producer ID, consumed public IDs/member paths, source/wire usage mode, declared interval/exact policy and current resolution; discover actual typed uses as a separate set. |
| deprecations/aliases | Existing identity history where applicable plus alias target, introduction/removal API versions, migration hint and replacement public ID. No heuristic rename, alias cycle or deleted-ID reuse. |
| internal/private symbols | Everything outside the allowlist is internal for the new boundary. Explicit legacy private visibility stays restrictive; no implicit exports of storage projections, helpers or target types. |

### Boundary algorithms and explainable paths

| Issue boundary rule | Proposed algorithm and required evidence |
| --- | --- |
| cross-module reference only to exported symbol | Resolve each typed cross-owner site against provider exports and consumer declared imports/uses. Existing private-reference diagnostic takes precedence for the same direct violation; project-visible but unexported is a new boundary violation. Include consumer -> import -> export/target path. |
| internal storage projection not public by default | Project only authored domain/public DTO signatures. Refuse storage attachment IDs, private technical fields or adapter classes in the portable public closure; report the exact signature/member path. |
| no cross-module direct write unless explicit shared-aggregate contract | For each actual declared write/create/delete effect, resolve aggregate owner/resource/member and consumer operation. A grant identifies provider/consumer, aggregate, exact operations/actions/field scope, invariant and concurrency obligations, version/pin and rationale; neither an import nor a common transaction is a grant. Refuse broader actual effects and stale/missing grants. |
| imports/dependencies declared | Compare resolved cross-module use edges with Model imports and attachment imports. Every externally owned public signature type also needs its pinned public dependency. Unused imports are visible but not automatically an error. |
| dependency cycles detected | Before `graph::build`, check declared import edges with iterative SCC and canonical cycle path so fatal cycles still have evidence. Then analyze projected semantic/public dependencies with labeled original-edge witnesses; do not conflate a module projection cycle with a symbol cycle or all local type recursion. |
| public type cannot expose private/target-specific implementation type | Exhaust the required public signature closure, including wrappers, nested fields, operation inputs/results/errors, event payloads, ports and query results. Paths retain relation/member role and owner. A direct existing finding is linked, not duplicated; transitive exposure is a new finding. |
| event/port preferred where direct coupling would cross ownership boundary | Emit an advisory alternative tied to the violating path and preserved signature/error/effect/ownership obligations. No automatic refactor or mandatory service architecture. |
| semantic diff computes affected consumers | Compare canonical public projections and intersect precise changed symbol/member paths with before/after declared and actual uses, retaining witness chains and compatibility reasons. Use a separately labeled conservative set for unsupported member precision. |
| target adapter maps boundary without changing meaning | Compare adapter mapping evidence to the same portable public digest/IDs/signatures/errors/effects, independently of deployment location. Unsupported mappings fail explicitly; never normalize them into a new canonical type or grant. |

Cycle and forbidden-write paths use original typed edge/effect references, relation/occurrence, owner IDs, declaration pointer and optional normalized relative span. Validation of fatal import cycles must not require an already valid graph. If source locations are unavailable, the semantic path remains useful; no fabricated physical position. Shared-state advisory findings from coupling may reference the same effects but do not override boundary legality.

## Proposed consumer-aware diff, lock and context

### Exact consumer set and compatibility

Compare public canonical semantics first, excluding layout, descriptions, physical locations, runtime bindings and product version from the public semantic digest. Retain source revision/digests separately for freshness. Independent API version and compatibility/deprecation declarations are public policy facts; implementation-only changes may change private provenance without changing the public digest or consumer capsule facts.

Classify changes using the existing axes where applicable: additive, behavioral, source-breaking, wire-breaking, storage-migration-required, data-loss-risk, target-specific and unknown. Public field removal, required-input addition, nullable-to-required change, signature/error removal, event payload break, export removal and incompatible capability/effect changes carry exact reasons. Optional output fields and enum additions depend on declared source/wire consumer policy and cannot be universally assumed safe. Storage-only/private implementation changes appear in their owner diff and do not contaminate public API compatibility.

Use the **union** of before/after consumer inventories: removed exports still affect old consumers, newly added consumers are explicit, and declared subscriptions with zero current edges remain range-policy consumers. Resolve ID changes only through admitted same-identity history/alias evidence; do not translate private helper renames into public breaks. Whole-record uses are labeled whole-shape sensitivity, not invented precise field reads. For known member uses, include only impacted paths; separately show compatible consumers and genuinely unaffected consumers as controls. Report exact known sets, possible sets and coverage reasons without merging them into one purported exact total.

“Exact” is scoped to the named, revision-bound semantic consumer inventory. Unknown external callers, incomplete observed imports or stale scans leave external completeness unknown. A digest-valid but schema-invalid snapshot is invalid; a structurally valid snapshot bound to the wrong side/project/API is invalid or stale, never trusted because of its hash. Without both inventories, public break classification is still possible but exact affected-consumer acceptance is incomplete.

For versioned modules with stable major >= 1, default proposed bump policy requires a major bump for an incompatible public change, at least minor for additive public surface and patch for compatible behavioral changes. Pre-1.0 policy explicitly chooses minor-breaking behavior and must be pinned; no hidden universal assumption. Digest-only modules require explicit new digest resolution without inventing a SemVer. A legal version bump does not make old consumer ranges compatible. `--strict` evaluates each consumer's declared policy separately and denies incompatible resolutions; a source break remains visible even when a consumer deliberately accepts the new major.

Deprecation windows use API release versions, with introduction/removal versions and explicit replacement hints; no wall-clock lookup. Early removal is a policy failure. Aliases are versioned, resolve to a public same-meaning target, cannot cross into private symbols, and cannot create hidden imports or ID reuse. Migration hints are advisory data; no model/lock rewrite occurs.

The new consumer lock pins the selected public digest, optional independent SemVer and transitive public dependency digests. `--check` detects public dependency changes even when the top-level version string is unchanged. Private layout/body/storage changes preserve resolved public identity. Checks cannot update a stale lock into a passing one. The existing adapter lock remains unchanged; module consumer ranges do not widen supported contract-family versions.

### Public-only bounded consumer context

`plan_consumer_context` decodes a standalone bundle and follows only selected exports and their required public dependencies. It supplies the exact public input/result/error/effect/capability/version/deprecation obligations for the consumer task, required scenario/contract references where public, and explicit gaps. It cannot load the provider's full `Compilation`; a fixture with the entire provider checkout absent must succeed. Referenced public dependencies must be materialized in the bundle's bounded dependency set, with exact pins and closure validation, so no implicit fetch is needed.

Reuse the offline estimator implementation and pin its identity/version/digest. Proposed protected required facts cannot be dropped merely to make `fits:true`; if they exceed the budget, preserve an explicit non-fitting manifest and minimum required cost, with no complete-context claim. Optional explanatory facts may be omitted with reasons. No raw source, absolute paths, private error text, credentials, provider implementation card or storage mapping is emitted. Privacy admission remains a separate check.

Proposed hard ceilings, all requiring boundary tests: input attachment/public bundle <= 8 MiB, <= 2,000 modules, <= 50,000 export/use records, dependency nesting <= 256, graph work within existing 100,000-node/1,000,000-edge bounds, context <= 128 selected roots and <= 1,000,000 requested tokens, report <= 32 MiB. Use checked arithmetic, visited sets and indexed adjacency; do not enumerate every simple path. Exhausted validation/public closure returns unavailable/incomplete, never a certified export or strict pass. A truncated witness is explicitly incomplete and cannot be presented as an exact explainable path.

## Proposed diagnostics and registry implications

Propose `LEK-MODULE-NNN` / `module-api.*`; `MODULE` fits the existing diagnostic-code grammar. Allocations were checked against both inspected active/successor registries and are proposals until implementation/merge rechecks the live union. No new diagnostic category or generic wire shape is necessary. Reuse existing direct semantic/query/graph findings; new rules explain additional boundary obligations.

| Proposed code / rule ID | Category and trigger; status policy |
| --- | --- |
| `LEK-MODULE-001` / `module-api.input-invalid` | infrastructure/error: malformed/duplicate/unknown fields, bad selector, unresolved attachment reference, contradictory pins, invalid interval or arithmetic; invalid/1. |
| `LEK-MODULE-002` / `module-api.version-unsupported` | compatibility/error: unknown contract/public recipe version; unsupported-version/5. |
| `LEK-MODULE-003` / `module-api.symbol-not-exported` | security/error: foreign reference resolves to an unlisted export, beyond the existing direct private rule; advisory valid/0 or strict invalid/1. |
| `LEK-MODULE-004` / `module-api.import-undeclared` | semantic/error: resolved foreign dependency lacks the declared Model/attachment import or selected public use; advisory valid/0 or strict invalid/1. |
| `LEK-MODULE-005` / `module-api.public-private-exposure` | security/error: exported signature closure reaches private/nonexported type, member or error contract; advisory valid/0 or strict invalid/1. Certified export rejects regardless of advisory mode. |
| `LEK-MODULE-006` / `module-api.public-target-exposure` | compatibility/error: portable public closure reaches target class/type/implementation mapping; advisory valid/0 or strict invalid/1. Direct existing portability findings retain their original code. |
| `LEK-MODULE-007` / `module-api.cross-module-write-forbidden` | security/error: actual direct cross-owner effect exceeds a valid explicit shared-aggregate grant; advisory valid/0 or strict invalid/1, preserving effect/resource/path evidence. |
| `LEK-MODULE-008` / `module-api.dependency-cycle` | semantic/error: boundary dependency SCC with canonical path. Existing fatal `requires` cycles retain `LEK-GRAPH-001`; new projected/public cycles use this rule, advisory valid/0 or strict invalid/1. |
| `LEK-MODULE-009` / `module-api.consumer-incompatible` | compatibility/warning: public change/range/digest resolution is incompatible for named consumers; valid/0 fact, strict policy denial via rule 014. |
| `LEK-MODULE-010` / `module-api.version-policy-violation` | compatibility/warning: changed API lacks the required independent version bump or explicit digest resolution; valid/0 fact, strict policy denial via rule 014. |
| `LEK-MODULE-011` / `module-api.deprecation-window-violation` | compatibility/warning: removal before the declared window or invalid public replacement/alias obligation; valid/0 fact, strict policy denial via rule 014. Malformed alias cycles are input-invalid. |
| `LEK-MODULE-012` / `module-api.evidence-incomplete` | semantic/warning: required public closure, precise uses, adapter capability or provider evidence missing/stale/bounded; valid/0 indeterminate for optional evidence, unavailable/4 when required. |
| `LEK-MODULE-013` / `module-api.lock-stale` | compatibility/warning: exact public/dependency resolution differs from the supplied consumer lock; advisory valid/0, strict/check policy denial via rule 014. |
| `LEK-MODULE-014` / `module-api.policy-denied` | compatibility/error: otherwise valid declarations fail selected strict compatibility/deprecation/lock policy; denied/3, with original findings and complete available report evidence. |

Register allowed statuses, immutable meanings, message IDs/defaults, exact data allowlists and location policy per rule. Proposed bounded data uses existing token/count/flag/list/record forms: subject, producer, consumer, export/member, effect, grant, witness ref, before/after digest, expected/actual version, range-policy ref and detail. Store full consumer sets/paths in the report and use references in diagnostics; existing 256 diagnostic/32 related-location bounds must not silently turn truncated consumer coverage into complete coverage. `allowed_fix_ids` stays empty: migration/extraction hints are not executable fixes.

The `0.8.0` registry successor must preserve the union of #76/#77/#84/waiver/CI and other landed entries, including tombstones; do not regenerate it from an old M7 research copy. Update registry runtime selection/normalizers, validation profile/report pins, CI report consumers, suite diagnostic coverage/digests and waiver applicable-rule tests together. A family catalog change is separate from a wire-schema change. Module boundary waivers cannot create hidden exports, grant wider writes, change canonical type meaning or hide incomplete evidence; only explicit, reviewed policy exceptions with preserved findings are eligible under the existing waiver owner.

## Proposed implementation increments

Each increment is reviewable independently, but completion of #113 requires all seven acceptance rows below.

| Increment | Proposed touch points and behavior | Exit condition before the next increment |
| --- | --- | --- |
| I1: typed contract and public projection | Add `module_api/{wire,version,resolve,public,canonical}.rs`, attachment/public schemas and exact version registration. Decode explicit exports, derive public closure from typed Model and applicable error/query/event facts, separate module SemVer from product version. | Valid/invalid schemas and typed constructor tests; stable public digest; public DTO contains no storage/provider internals; unknown versions rejected. No production discovery claim yet. |
| I2: boundary validation and paths | Add `validate`, `graph`, `effects`, report schema, own diagnostic entries/registry/profile successors and `check`/validator flags. Include pre-build import SCCs, undeclared import/export and scoped write-grant checks. | Strict private/nonexported/transitive-leak/write/cycle probes fail with exact paths; local/internal controls pass; default/strict modes preserve status/stream contracts. |
| I3: consumers, diff and lock | Add `consumers`, `diff`, `lock` and their schemas/CLI commands. Join before/after usages, policies, aliases/deprecations, ranges and exact dependency pins. | Real-binary diff identifies exact known consumers and break reasons, leaves unknown external coverage visible; incompatible strict policy and stale lock deny without writes. |
| I4: standalone consumer context | Add `context`, consumer-context schema/CLI. Build capsule from public bundle only with bounded required facts and exact estimator pins. | Planner task succeeds with provider checkout absent, provider-internal changes leave public facts stable; too-small budgets retain `fits:false`/required gaps. |
| I5: in-process/service mapping | Reuse existing adapter/transport/profile evidence seams for public-digest mapping; add own synthetic mapping vectors and unsupported/leak probes. Do not expand target protocol unless a real missing wire seam is established. | Identical portable IDs/signatures/errors/effects in local-call and service-port mappings, including capability refusal and read/write semantics. No deployment or service-extraction orchestrator claim. |
| I6: integration, gate and normative docs | Add schema/live gate, provenance/diagnostic coverage, after-build CI step, explicit architecture evidence successor and provider discovery if advertised; finish `docs/module-api.md` and grammar examples. | All AC fixtures and owner regression suites pass on the merged candidate; changed contracts use `0.8.0`, frozen ones retain bytes; docs label local conformance versus hosted/external evidence. |

Release/product metadata must move to `0.8.0` in an authorized implementation commit before changed contracts are admitted by the product-version gate. This task produces only research. No migration of old consumer projects is automatic; any future Model change/data migration needs its own explicit plan and registered transition.

## Proposed test plan mapping each acceptance criterion

The first column below reproduces the issue's acceptance criterion text **verbatim**. All fixture paths, tests and gates in the other columns are proposals; no implementation test was run for this research.

| Issue acceptance criterion (verbatim) | Proposed increment/fixture family | Observable proof and gate |
| --- | --- | --- |
| Internal symbol cannot be referenced cross-module in strict profile. | I2; `tests/fixtures/module-api/visibility/`: explicit private reference, project-visible-but-unexported symbol, local-private valid control. | `check --strict` and `validate --strict --module-api` reject exact cross-owner sites with `LEK-SEM-020` or `LEK-MODULE-003` as appropriate, exit 1; complete source/import/export path. Bare legacy strict rule still rejects existing module-private references. Own gate asserts both failures and controls. |
| Public API diff identifies exact consumers and breaking changes. | I3; `tests/fixtures/module-api/diff/`: public field removal, required input, error/event/capability change, optional field, removed export, declared alias, new/deleted consumer, formatting/private-only controls. | Built binary compares paired publics and both consumer snapshots; independently authored expected consumer IDs/member paths and compatibility reasons. Unaffected/disjoint consumers excluded; old consumers of deleted exports retained. Stale/malformed snapshots and incomplete coverage never claim exact completion; strict incompatible policy denies/3. |
| Planner can depend on Task public contract without owning provider internals. | I1/I4; `tests/fixtures/module-api/reference/{tasks,provider,planner}/` and `standalone/`. | Planner imports only authored Task view/query/error contracts. Provider credentials/client types/storage remain private and absent from bundle/capsule. Delete provider checkout in an isolated temporary fixture before the public-only context command; selected public task remains usable. Golden assertions check semantic facts, not just substrings. |
| Target-specific class/type cannot leak into portable public contract silently. | I1/I2/I5; `tests/fixtures/module-api/portability/`: direct and nested input/result/error/event/port wrapper leaks plus legal private target helper. | Direct findings retained and transitive paths use `LEK-MODULE-006`; strict/export fails. Private helper does not alter public digest. Unsupported adapter capability remains explicit. |
| Module cycle and forbidden write have explainable graph path. | I2; `tests/fixtures/module-api/{cycles,writes}/`: fatal import SCC, semantic projection cycle, self-loop, no-cycle controls, direct cross-owner field/entity write, missing/stale/broader grant, valid narrow shared aggregate. | Before-build fatal cycle preserves `LEK-GRAPH-001` and ordered declaration path; new cycle and write findings resolve every original relation/effect/resource/grant step. Entity-wide/delete cannot bypass a field-only grant. Strict failure/valid controls exercised by real CLI gate. |
| Public contract alone supports bounded context for consumer task. | I4; `tests/fixtures/module-api/context/`: standalone bundle, nested public dependencies, adequate/tiny budget, missing public dependency, oversize/depth/frontier limits. | No provider compilation/files/network; required public inputs/results/errors/effects/version policy retained; deterministic estimator/manifest; known adequate budget fits, tiny budget explicitly does not fit. Missing closure blocks certification. Filesystem/spawn log proves no hidden reads/writes. |
| Boundary works for in-process and extracted-service implementations. | I5; `tests/fixtures/module-api/mapping/{in-process,service}/`, same public digest and shared portable scenario expectations. | Local-call and service-port mapping vectors preserve exact canonical IDs/signatures/error codes/effect obligations and capabilities; transport routing/status stays separate. Contract/CLI conformance gate proves boundary mapping, while real target execution evidence is reported separately if provisioned. No remote deployment inferred from synthetic mapping. |

### Proposed requirement traceability

| Issue requirement (verbatim) | Proposed implementation/evidence |
| --- | --- |
| independent module version optional but supported; | I1/I3: null/digest-only, exact-version and bounded-range fixtures, independent module API `1.2.0` under Lekalo product/contract `0.8.0`. |
| project version and module API version separated; | No derivation from `Common.version` or product version; digest/version-policy tests change each independently. |
| deprecation window/migration hints; | I3: declared API-version window, before/at-removal boundaries, public replacement and alias closure, advisory typed migration hints. |
| consumer lock/range policy; | I3: exact public/transitive pins, supported interval edges, incompatible/malformed ranges, digest-only consumers, stale lock denial and no rewrite. |
| no hidden convention-based imports; | I1/I2: declared Model/attachment agreement, explicit foreign public dependency pin; directory/short-name/adapter-ID controls do not create imports. |
| context capsule can load public contract without all internals; | I4: standalone public bundle and filesystem isolation/no-provider test; new capsule family rather than widening frozen legacy output. |
| module extraction to service reuses same boundary; | I5: same portable public digest before/after local-to-port mapping and equal semantic conformance expectations. |
| no forced microservice architecture. | Local-call implementation remains first-class; recommendations are advisory and no extraction/generation/deployment command is introduced. |

### Proposed fixture and gate inventory

Create `tests/fixtures/module-api/{valid,invalid,visibility,imports,public-closure,portability,cycles,writes,diff,versions,deprecations,lock,context,mapping,reference,limits,privacy}/` with synthetic provenance, independently stated expected outcomes and immutable committed public/diff/capsule goldens. Reference authors should use existing planner and Taskhub shapes/source anchors but normalize them into explicitly authored `tasks`, `provider` and `planner` modules with fresh digest bindings; do not imply current Taskhub scans are complete canonical consumers. Attach relevant typed query/error/effect/storage facts only through their owner decoders. All negative inputs include valid controls to prevent an always-failing checker from passing acceptance.

The own `scripts/test-module-api-contracts.mjs` must require exact Ajv 8.17.1/Draft 2020-12 and a freshly built `target/debug/lekalo` (Windows `.exe`); missing binary is failure, not skip. Gate responsibilities:

1. Closed schema checks for all seven new families, duplicate/unknown/oversize inputs, exact version pins and malformed reference/digest contradictions.
2. Real CLI grammar/help, stdout/stderr/exit/status/assessment consistency, default/strict/export/lock/context behavior and unsupported-version refusal.
3. Independent expected public closure, exact consumer sets and ordered cycle/write paths, including stale-but-digest-valid and schema-invalid-but-digest-valid hostile inputs.
4. Determinism under YAML/JSON/layout/order perturbations, checked digest domains, accepted rename identity and before/after inventory union; no false consumers from whole-module fanout.
5. Bounds, unavailable/unknown states, non-fitting required context, non-bypassable shared grants, privacy canaries and no hidden canonical/cache/lock writes or adapter/network activity.
6. Public-only consumer and in-process/service mapping conformance against the same portable contract; capability unsupported path retained.

Proposed implementation validation commands, after product/contract integration and binary construction:

```sh
cargo build --workspace --locked
cargo test -p lekalo-core --test module_api --locked
cargo test -p lekalo-cli --test module_api --locked
node scripts/test-module-api-contracts.mjs
node scripts/test-model-contracts.mjs
node scripts/test-graph-contracts.mjs
node scripts/test-effect-graph-contracts.mjs
node scripts/test-semantic-diff-contracts.mjs
node scripts/test-context-contracts.mjs
node scripts/test-coupling-contracts.mjs
node scripts/test-architecture-profile-contracts.mjs
node scripts/test-diagnostic-contracts.mjs
node scripts/test-validation-contracts.mjs
node scripts/test-waivers-contracts.mjs
node scripts/test-ci-report-contracts.mjs
node scripts/test-versioning-contracts.mjs
node scripts/test-fixture-provenance.mjs
node scripts/test-golden-diagnostic-coverage.mjs
node scripts/check-contract-versions.mjs --base HEAD^
cargo test --workspace --locked
cargo fmt --all -- --check
```

Provide Ajv through both `LEKALO_AJV_NODE_PATH` and `NODE_PATH` as existing gate helpers require, outside the checkout. Use the actual implementation comparison base for a multi-commit integration rather than blindly checking only its last commit. Add new schema validation to the CI contracts lanes and the mandatory live gate to `build-test` after `cargo build --workspace --locked`, across Ubuntu/Windows/macOS; preserve existing Clippy/MSRV/Node lanes. Schema-only green fixtures, local focused gates, full workspace gates, hosted matrix and genuine target/service execution are separate evidence levels.

## Determinism, risks and non-goals

Canonical sets sort by exact kind/module/symbol/member identity and occurrence-aware edge keys; ordered behavioral lists keep their documented order. Public digest excludes its own digest member and uses a documented SHA-256 domain over public semantic/policy content plus referenced public dependency identities. Private provider revision pins remain separate provenance. Input snapshots, lock payloads and comparison receipts have distinct digest domains; a hash proves byte identity, not publisher authenticity or external consumer completeness. No timestamps or absolute machine paths enter semantic identity.

- **Attachment authority and completeness:** implicit exports or a missing attachment must never become a strict pass. Existing legacy visibility stays available until the explicit boundary seam is selected. Missing applicable query/error/effect facts leave public obligations unknown; do not invent empty errors/effects.
- **Type/member precision:** Model fields lack per-field visibility, and whole-record uses may overapproximate member sensitivity. Use authored public DTOs, precise typed use facts where available, and labeled possible sets elsewhere; do not publish a silently cropped internal type under its original ID.
- **Graph bootstrap:** fatal import cycles prevent graph construction. A checker that waits for a successful graph cannot satisfy cycle acceptance; pre-build typed import witnesses and separate module-projection labels are required.
- **Write-grant escape:** broad grants can erase module ownership. Require exact resource/operation/action scope and invariant/concurrency obligations; preserve all actual effects and current grant pins. Shared aggregate grants do not imply that distributed transactions are supported after service extraction.
- **Version and registry integration:** product is currently `0.6.5`; new artifacts must enter an implementation commit at `0.8.0`. Preserve old frozen contract bytes and the live registry union; update every changed profile/evidence/runtime pin consistently. Older research versions are historical context, not current constants.
- **Exact-consumer overclaim:** undeclared external consumers and inferred scans remain unknown. Counterexamples must test removed exports and before-only consumers; broad coupling/module reach is insufficient evidence of exact public-member use.
- **Context/privacy:** portable public semantics can still contain confidential business names. No automatic public export permission, source upload, QA write authority or privacy downgrade follows from this API design.
- **Adapter/service proof:** synthetic vectors establish mapping semantics, not remote deployment, runtime delivery/transaction equivalence or performance. Genuine adapter capability and scenario evidence must be separately provisioned when claiming those outcomes.
- **Scalability:** all-consumer closure and SCCs can become expensive; use bounded indexed algorithms, explicit work limits and honest incomplete coverage. Never hide a break behind output truncation or a tiny token budget.

Non-goals: a general package/network registry or dependency installer; changing stable semantic ID grammar; expanding core Model into target classes/SQL/ORM; automatic SemVer bumps/lock rewrites; source-code import inference as canonical authority; dangerous migration/refactor/fix application; a mandatory microservice style; a service extraction/deployment orchestrator; distributed transaction implementation; AIFHub/HLV task writes; external publication or model-quality evaluation.

## Research validation and completion boundary

Research evidence is the live issue/context bodies and source/contract inventories at the pinned revision. The document supplies seven verbatim acceptance rows, eight verbatim requirement rows, all ten public-surface items, all nine boundary rules, proposed CLI/contracts/diagnostics, six delivery increments and fixture/gate families. Documentation validation checks acceptance text, referenced current-owner paths, proposed-code collision absence, Markdown whitespace and exact changed/staged scope. No Rust build, implementation tests, new fixture generation, native adapter execution, service call, hosted CI or production acceptance is claimed. The authorized result is this document in one local HEAD commit; implementation requires a separate task.

Completed documentation checks: live-body comparison passed for all 7 acceptance and 8 requirement rows; all 14 proposed diagnostic codes are absent from both inspected registries; 12 directly rooted inventory paths exist; fences are balanced and bytes are LF-only. `node scripts/check-structure.mjs`, `node scripts/check-privacy.mjs` and `node scripts/check-contract-versions.mjs --base 15bd13917654dc20ad19b089a5bb8e5e29c2099f` passed. The version gate reports the unchanged checkout product `0.6.5` and 138 existing contract artifacts, consistent with research-only scope; it does not validate future `0.8.0` implementation contracts.
