# Issue #34: Lekalo provider integration research

Research only, 2026-10-01. This document proposes implementation; it does not publish a provider protocol or implement an adapter.

## Scope and evidence

Acceptance authority: live [Lekalo #34](https://github.com/ichinya/lekalo/issues/34), OPEN, and cross-project [AIFHub Extension #136](https://github.com/ichinya/aifhub-extension/issues/136), CLOSED, read with `gh issue view ... --json` during this investigation. Closing #136 did not complete its future Lekalo execution path.

| Checkout | Inspected revision | Scope |
| --- | --- | --- |
| Lekalo, branch `ichinya/m7-issue-34` | `9510dd0767a56c0ab34b8d3c8ceb2a14db8de825`, supplied M7 base | Source and contracts; only this report may change |
| `D:/Projects/aifhub/aifhub-extension`, branch `main` | `9b9db326865c342761467d2238895a53a341f7cf` | Read-only provider implementation and focused existing tests |
| `D:/Projects/aifhub/ai-factory` | `68490e3d0c4303578616cee514dd5fb9f8d4336f` | Read-only glance at lifecycle skill entrypoints |

All three checkouts were clean when inspected. These are local source pins, not assertions about the latest remote release. Context7 resolution was attempted for both AIFHub Extension and Lekalo; it returned unrelated projects, so none was used as documentation. Conclusions below come from the pinned source and live issues.

**Finding:** most Lekalo operations already exist. The missing boundary is an explicitly published AIFHub-consumable command/capability contract, plus extension execution, lifecycle scheduling, normalization and freshness work. A general-purpose new workflow or a rewrite of the target adapter protocol is unnecessary.

Important version distinction: Lekalo product is **0.6.3** in `Cargo.toml`. Its existing `lekalo.target/v1` protocol, contract version **0.3.2**, is published for **Lekalo -> target adapter** communication. It is not **AIFHub -> Lekalo** capability discovery. `lekalo compatibility --json` describes Model/IR/target-protocol compatibility, not the provider operations required by #34. The extension's v0.1.10 comments about inspect/impact/context being stubs are obsolete; its actual Lekalo adapter still deliberately refuses execution.

## Extension provider-model anatomy (files and APIs)

Paths in this section are relative to the extension checkout. Primary entrypoints: [dispatcher](https://github.com/ichinya/aifhub-extension/blob/9b9db326865c342761467d2238895a53a341f7cf/scripts/aifhub-providers.mjs), [policy](https://github.com/ichinya/aifhub-extension/blob/9b9db326865c342761467d2238895a53a341f7cf/scripts/provider-policy.mjs), [Lekalo stub](https://github.com/ichinya/aifhub-extension/blob/9b9db326865c342761467d2238895a53a341f7cf/scripts/lekalo-provider.mjs), [evidence schema](https://github.com/ichinya/aifhub-extension/blob/9b9db326865c342761467d2238895a53a341f7cf/schemas/provider-evidence.schema.json).

| File | Existing API / behavior | Lekalo integration consequence |
| --- | --- | --- |
| `scripts/tool-config.mjs`, `schemas/tool-config.schema.json` | Independent `aifhub.tools.{openspec,hlv,lekalo}` booleans, all false by default | Preserve composition and AI Factory-only operation; discovery never enables a tool |
| `scripts/provider-policy.mjs`, `schemas/provider-config.schema.json` | `parseProviderConfig`, `normalizeProviderPolicies`, `readProviderPolicies`, `providerGate`; kinds are `hlv: validation`, `lekalo: semantic_model` | A registry slot already exists. Current policies are only optional/required; disabled boolean supplies effective off |
| `scripts/aifhub-providers.mjs` | `runProviders(options)`, `runProviderCommand(argv, options)`, `providerDiagnostics`; `options` carries root, phase, changeId, write/readOnly, process/revision test seams and cancellation | Owns scheduling, policy, exact revision checks, evidence persistence and aggregate blocking |
| `commands/aifhub-providers.mjs`, `extension.json` | `register(program)` forwards the installed `ai-factory aifhub-providers` command to the script | Existing public entrypoint; extend its arguments for substeps, bounded scope and targets rather than invent another mandatory command family |
| `scripts/hlv-provider.mjs` | `HLV_COMMAND_CONTRACT`, `detectHlv`, `detectHlvVersion`, `runHlvOperation`, `normalizeHlvResult` | Working external-process example; exact HLV 1.0.0 command mapping, not a universal Lekalo parser |
| `scripts/lekalo-provider.mjs` | `LEKALO_COMMAND_CONTRACT` reserved as `aifhub.lekalo-cli@0.0.0`; `detectLekalo(rootDir, config, options)`, `runLekaloOperation(operation, rootDir, config, options)` | Both always return `unsupported/protocol_unpublished`; neither launches a process. This is the primary replacement seam |
| `scripts/provider-process.mjs` | `runProviderProcess(executable, args, options)` | Argument arrays, `shell:false`, closed stdin, hidden Windows process, combined output cap, timeout, abort and descendant termination; reuse it |
| `scripts/provider-files.mjs` | `safeId`, `safeProviderPath`, `readProviderFile`, `writeProviderFile`, `providerRevision`, `digest` | Contains paths; rejects links/hard links; atomically replaces evidence; preserves previous bytes/timestamp on identical results |
| `scripts/aifhub-providers.mjs`, `schemas/provider-capabilities.schema.json` | `negotiateProviderCapabilities(manifest, requested)` requires exact `aifhub.provider@1.0.0`, kind, toolVersion and operation list | This is an extension-owned neutral manifest, not a published Lekalo handshake; its closed operations omit `generate` and `verify` |
| `scripts/semantic-provider-contract.mjs`, `schemas/provider-semantic-evidence.schema.json` | `normalizeSemanticEvidence`, `validateNeutralTrace`; fake semantic result supports impact digests, context digest/token count, diagnostic codes and five-field trace links | Useful normalization precedent, but not wired to real CLI; no actual context capsule or full Lekalo occurrence graph |
| `schemas/provider-evidence.schema.json` | Closed phase evidence v1.0.0 with status, gate, operations, tool/adapter versions, change/revision and provenance | Still constrains non-null commandContract to HLV; diagnostic regex accepts HLV-style codes, not `LEK-*`; no generate/verify operation or semantic payload fields |
| `injections/core/aif-implement-plan-folder.md` | Existing OpenSpec/legacy execution override | No real Lekalo pre/post hooks; add them while preserving the upstream ultra-bundle handoff boundary |
| `injections/core/aif-verify-plan-folder.md` | Calls providers verify **after** native gates | Must change scheduling for #34: Lekalo validate/verify/trace first, native gates next, optional HLV last |
| `skills/aif-done/SKILL.md`, `scripts/openspec-done-readiness.mjs` | Done skill invokes provider done; readiness calls `runProviders({phase:'done', readOnly:true, ...})` before archive | Add Lekalo-specific current-evidence checks; the existing saved-evidence validator is HLV-only |
| `scripts/aifhub-mcp-server.mjs` | `providersStatus`, exposed as `aifhub.providers_status`, accepts status/doctor only | Aggregation already exists; preserve read-only behavior and its 1 MiB response cap |
| `scripts/aif-mode.mjs`, `scripts/aif-artifact-sync.mjs` | `doctorAifMode` calls `runProviders` with doctor phase and `write:false` | Mode diagnostics already aggregate providers; no separate Lekalo setup/status workflow is needed |
| `scripts/tool-initialization.mjs`, `skills/shared/TOOLS.md`, `docs/validation-providers.md` | Explicit tool selection and OpenSpec/HLV scaffolding flow | Do not extend shared automatic initialization behavior to Lekalo; #34 requires no hidden init |

The invocation path is host command -> installed wrapper -> `runProviderCommand` -> `runProviders` -> provider adapter -> `runProviderProcess`. No Rust crate is imported by the extension. HLV operations map to status, doctor, check, workflow and trace. `aifhub.hlv-cli@1.0.0` is the extension's reviewed command contract, not an upstream HLV protocol.

Current dispatcher schedule is shared by both providers:

| Phase | Current operations | Required Lekalo schedule |
| --- | --- | --- |
| status | status after detect | negotiate/detect, then read-only status |
| doctor | doctor, status | negotiate, doctor and status; no fixes/setup |
| implement | **empty** | Before edits: impact -> bounded context. After edits: validate -> explicitly requested generate/check |
| verify | validate, status, trace | validate -> verify -> trace; then host native gates; then configured HLV |
| done | validate, readiness, trace | Current validation/verification evidence + readiness release/done + drift + freshness; no generation |

Simply replacing the stub would produce an empty successful implement result and would still never call Lekalo verify/generate. The source comment that only the adapter needs changing is insufficient for #34's full flow. Use provider-specific operation plans and an implement before/after substep; do not make HLV run Lekalo operations. The wrapper currently accepts only phase, `--change`, `--write`, `--json`, so it cannot yet transport scope, budget, target or substep selections.

### Persistence and freshness already present

`runProviders` only persists implement/verify/done with `write:true` and a validated change ID (`[a-z0-9][a-z0-9-]{0,99}`). It currently writes `.ai-factory/qa/<id>/providers/<provider>-<phase>.json`, **not** the issue's `.ai-factory/qa/<change-id>/providers/lekalo.json`. Each provider is isolated; one failure does not erase another provider's file.

`providerRevision` binds HEAD and a SHA-256 digest of sorted file path/mode/content tuples, including missing tracked files. It includes tracked/untracked nonignored files and explicitly traverses ignored `openspec`, `.hlv`, `lekalo`, root `project.yaml`, and `.ai-factory/config.yaml`. It excludes `.ai-factory/qa`, `.ai-factory/state` and generated rules. It does **not** explicitly cover ignored `.lekalo` generated artifacts/lock-related inputs or ignored AI Factory plan/spec/context/trace homes. Required additions are explicit, bounded input inventories and input digests, not a broad hash of every cache/runtime output.

For HLV read-only done, the newest current verify/done record must agree on change, revision, tool/command contract, layout, policy digest, streams, counts and diagnostics; a later failure supersedes an old pass. Lekalo has no corresponding branch. Source mutation during a run and evidence-write failures currently block even under optional policy: these are host integrity failures. A mutating generate step therefore cannot sit inside the existing single before/after equality guard; it needs an explicit mutation transaction and subsequent validation at the new revision.

## Lekalo surface mapping (command -> operation -> gap)

Current CLI grammar is in [`crates/lekalo-cli/src/main.rs`](../../crates/lekalo-cli/src/main.rs), with global `--json`; it is more authoritative than older version examples in [`docs/cli.md`](../cli.md). Commands below show existing syntax, except the clearly marked proposal. Run with the consumer root as process cwd and a validated `--project` selection when applicable; place Lekalo options **before** the adapter `-- PROGRAM [ARGS...]` delimiter.

| Existing command with `--json` | Provider operation | Implementation / output | Gap or mapping rule |
| --- | --- | --- | --- |
| `lekalo --version --json`; `lekalo compatibility --json` | detect / negotiate | `DomainResult::version`, `run_compatibility`; product version; registered Model/IR/target-protocol versions | Neither advertises workflow operations or their output schemas. Add a read-only provider manifest; do not infer capabilities from version number |
| `lekalo status --project DIR --json` | status | `run_status`, `lekalo_core::doctor::report`; freshness/revisions report | No safe auto-enable/init. Map report verdict, not just exit. Git commit/dirty in this report is not AIFHub's complete source digest |
| `lekalo doctor --project DIR [--trace PATH]... --json` | doctor | `run_doctor`; structural/version/lock/adapters/bindings/drift/tools/integration checks | Read-only. Do not pass `--fix` (currently advice only, unnecessary at this boundary). Missing trace evidence remains explicit |
| `lekalo impact SYMBOL --project DIR --json`; `lekalo impact --changed --base REF [--head REF] --project DIR --json`; `lekalo impact --changed --worktree --project DIR --json` | impact | `run_impact`, CLI `git_input`, core `impact`; exact digests, impacted nodes, risks/gates/completeness | Extension must pin base/head/change scope; a Git branch name alone is not evidence. Strict profile denial is exit 3. Large or incomplete results must not become a complete context claim |
| `lekalo context SYMBOL --budget N --project DIR --json`; `lekalo context --changed ID1,ID2 --budget N --project DIR --json` | context | `run_context`, core `context`; typed capsule, estimator identity and truncation manifest | `--changed` takes semantic IDs, **not** a Git switch. Extract the bounded ID set from impact. Check `fits`/minimumRequired and byte limits before sending to the agent; no `--spans` by default |
| `lekalo inspect SYMBOL [--include bindings,scenarios] --project DIR --json` | optional context support | `run_inspect`, core `inspect` | Implemented supporting surface, not a replacement for bounded context or a required new provider op |
| `lekalo validate --project DIR [--module MODULE] [--strict] --json` | validate | Core semantic validator; diagnostics, counts, profile identity | Valid success/warnings on stdout; invalid JSON on stderr; strict authorization denial on stdout. Preserve original registry IDs/codes/categories |
| `lekalo generate --check [--locked] --project DIR --json` | drift / generate check | `GenerateService::check`; ownership manifest, lock/model/IR/artifact-byte verification | Read-only and no adapter spawn. Does not generate code; report it as drift/check, not successful apply |
| `lekalo generate --target TARGET [--module MODULE] [--dry-run] [--locked] --project DIR --json -- PROGRAM [ARGS...]` | generate | `run_generate`, core `orchestration::generate`, target process protocol and manifest writer | Real generation requires explicit installed adapter argv. `--target` alone is insufficient. Separate mutation boundary, before/after binding, write authority; never run in done/detection. Dry-run can prepare cache evidence and is not a global no-writes guarantee |
| `lekalo verify [--target TARGET]... [--module MODULE] [--changed] [--locked] [--trace PATH] --project DIR --json [-- PROGRAM [ARGS...]]` | verify | `run_verify`, core `orchestration::verify`; validation, drift, target verification, binding/scenario/trace receipts | Read-only host pipeline; pinned adapter supply/cache needed for target verification. Component states and required flags matter. Native gates remain a separate host step |
| `lekalo readiness --phase implement\|generate\|verify\|release\|done --project DIR [--trace PATH]... --json` | readiness | `run_readiness`, `DoctorPhase::Done -> Release`, core doctor | Already exists. A produced blocked report exits **0**. Done must additionally validate the extension's exact evidence binding and configured required operations |
| `lekalo trace export PATH --json` | trace export | `trace_export`; validates supplied manifest, canonical trace object + `manifestDigest` | Requires PATH; does not discover/compose an OpenSpec-to-HLV trace by itself. Partial trace is valid data, not full coverage |
| `lekalo requirements trace ... --json`; `lekalo trace collect --project DIR --json` | trace inputs | Requirements trace projection and `orchestration::collect_scenario_trace` over ingested scenario evidence | Requirements projection is read-only. Collect writes `.lekalo/import/trace/` and may remove a stale export when evidence is absent; it is a separate explicit mutation, never an implicit export/done step. Do not fabricate requirement/test/gate relations or claim native execution from an exported record |

Supporting sources: [`docs/context.md`](../context.md), [`docs/impact.md`](../impact.md), [`docs/doctor.md`](../doctor.md), [`docs/orchestration.md`](../orchestration.md), [`docs/trace-manifest.md`](../trace-manifest.md), [`crates/lekalo-core/src/result.rs`](../../crates/lekalo-core/src/result.rs), and [`crates/lekalo-core/src/orchestration/verify.rs`](../../crates/lekalo-core/src/orchestration/verify.rs).

### Existing result contracts and limits

The exit/stream contract already exists; #34 should consolidate and publish its operation-specific interpretation, not invent a contradictory generic stdout-only contract:

| Exit | Native status / stream | Provider interpretation |
| --- | --- | --- |
| 0 | Success/report, normally stdout | Parse the operation's schema and verdict. Doctor/readiness blocked, context not fitting, and trace gaps cannot be treated as pass just because exit is zero |
| 1 | `invalid`, stderr JSON | Classify diagnostics: implementation/model invalidity versus usage/input/configuration errors; a schema-valid validation failure is not a provider crash |
| 3 | `denied`, stdout JSON | Preserve typed policy/confinement/strict-gate denial; map to operation failure with reason/classification |
| 4 | `unsupported`, stdout JSON, sometimes an orchestration receipt | Capability absence or degraded verification; distinguish required missing components from optional findings using the receipt |
| 5 | `unsupported-version`, stderr JSON | Unsupported protocol/schema/input version; never ordinary test failure |
| Other exit / signal / no valid envelope | No trusted domain verdict | Infrastructure error; retain bounded stream metadata, no raw streams in durable evidence |

The exact native root shape varies: a command may emit a direct receipt, a `status` envelope with a nested payload, or a diagnostics failure. `DomainResult` owns the exit and stream; receipt schema plus verdict owns the semantic result. Reject contradictory stream/exit/status/schema combinations. HLV's parser reads stdout only and cannot be reused unchanged.

Current source constants: context/impact/trace/orchestration schemas are **0.2.16**; doctor is **0.3.2**; diagnostic wire is **0.2.16**, diagnostic registry **0.4.0**; target protocol **0.3.2**. They differ from product **0.6.3**. `docs/versioning.md` says a changed/new contract gets the product version at its implementation commit; unchanged contracts retain their own versions. Do not release a manifest claiming every schema is 0.6.3 or arbitrarily assign all new Lekalo contracts 1.0.0.

The core permits up to 1,000,000 context-budget tokens and 128 roots; impact/trace exports may reach 32 MiB. Extension defaults are 30 seconds/1 MiB, configurable only up to 300 seconds/4 MiB; MCP status is capped at 1 MiB. Publish operation-specific bounds and use a smaller workflow budget (for example 5,000 tokens initially), preserve `fits:false`, reject oversized transport, and never silently truncate JSON or drop diagnostic errors. Nested target operations have longer native defaults, so explicitly align their deadlines with the outer process budget. In current verify code, scenario execution consumes ingested run evidence; `native.gates` is still explicitly unsupported. Do not inherit stale prose claiming all scenario evidence is absent or claim verify executed native tests.

## Split of work between repositories

| Concern | Lekalo repository | AIFHub Extension repository |
| --- | --- | --- |
| Semantic behavior | Own existing context/impact/validation/drift/verify/readiness/trace results | Consume and preserve their meaning; no reimplementation or Rust dependency |
| Published process boundary | Publish provider manifest, command mapping, output schema identities, exit/stream/side-effect/limit contracts | Pin accepted manifest versions, validate negotiation and schemas, construct reviewed argv |
| Lifecycle | No `/aif-*` workflow or phase policy inside Lekalo | Before/after implement hooks; verify sequencing; done blocking; optional HLV composition |
| Change/revision identity | Supply exact Model/IR/lock/artifact/trace identities and deterministic payload digests | Resolve change ID and base/head, bind full consumer revision, policy/capabilities/tool identity, compare before/after |
| Evidence custody | Emit process results; enforce privacy and existing write ownership | Write `.ai-factory/qa/<change-id>/providers/lekalo.json` atomically; maintain phase/substep observations; enforce freshness |
| Canonical ownership | `lekalo/**` and explicitly authorized generated roots; no OpenSpec canonical writes | OpenSpec remains requirements/change owner, AI Factory workflow owner, HLV own diagnostics; never auto-sync a conflict |
| Distribution | Publish CLI binary plus immutable contract artifacts in a release | Require preinstalled compatible tool; no hidden download/install/update/init |
| Tests | Public CLI contract fixtures and child-process conformance tests | Adapter/process/policy/lifecycle tests and full OpenSpec + Lekalo + HLV E2E |

The existing accepted authority matrix `contracts/authority-matrix.v0.3.2.json` already defines `ai-factory.provider-evidence-envelope` under `.ai-factory/qa/**`, writable by AI Factory/AIFHub adapter, **not Lekalo**. `context.capsule` is runtime-only under `.ai-factory/context/**`; `trace.manifest` is AI Factory-owned direct evidence under `.ai-factory/traces/**`. `contracts/privacy-policy.v0.3.2.json` classifies capsule as local-private, provider envelope/trace as shareable-with-redaction, and raw tool output as forbidden-to-export. These embedded versions are confirmed in `crates/lekalo-core/src/privacy/context.rs`. Reuse these kinds; a new path/kind/export permission requires an accepted successor, not a runtime alias. A file being in QA does not make arbitrary semantic content public-safe.

The separate existing scenario export under `.lekalo/import/trace/` is not permission to relocate the authority kind `trace.manifest` there or make Lekalo a QA writer. Consume that local artifact only through its own accepted custody, and let the extension perform any authorized cross-layer projection.

AI Factory itself needs no change for the initial implementation: its `skills/aif-plan`, `skills/aif-implement`, `skills/aif-verify` are the base lifecycle; the extension supplies overrides and its done skill. Keep this overlay opt-in and preserve ultra-bundle ownership/handoffs. AIFHub registry compatibility publication can follow the two repository changes; it is not a third implementation dependency for this research deliverable.

## Proposed Lekalo-side changes (files, schemas and tests)

### Minimum deliverable

Publish a stable **command contract plus a read-only discovery shim**, keeping existing operational commands. A full `provider run` facade is optional future simplification, not required to duplicate all of the CLI today. No generic execute-by-manifest facility: the consumer recognizes known operations/versions and uses fixed argument construction, never executes arbitrary command text returned by a provider.

| Proposed file / existing seam | Work |
| --- | --- |
| New `docs/provider-contract.md` | Normative operation table, negotiation, exact argv/operands, schemas, side effects, limits, statuses/exits/streams, privacy, digest domains and conformance examples |
| `docs/cli.md`, `docs/versioning.md` | Document the new discovery command and distinguish workflow provider from target protocol; refresh stale examples needed by the contract |
| New `contracts/provider-capabilities.schema.v<release>.json` | Closed manifest with Lekalo provider-contract identity, exact product version, supported operation IDs, output schema IDs/digests, effect class and resource bounds; `<release>` is the implementation commit's product version |
| New `crates/lekalo-core/src/provider/{mod.rs,version.rs,manifest.rs}`, export in `src/lib.rs` | Typed deterministic manifest and capability selection using existing accepted constants; no filesystem, provider installation or lifecycle policy |
| `crates/lekalo-cli/src/main.rs` | Proposed **`lekalo provider describe --json`**: works outside a project, emits manifest without loading project/config/cache, launches nothing and writes nothing |
| New `crates/lekalo-cli/tests/provider.rs`, `tests/fixtures/provider/` | Child-process manifest and native-command contract fixtures, exact stream/exit checks, no-side-effect discovery fixture |
| New `scripts/test-provider-contracts.mjs`, relevant `.github/workflows/ci.yml` step | Validate manifest and command-result fixtures against pinned schemas; cross-check operation/schema/version inventory with CLI tests |

The discovery manifest should advertise operations separately: `status`, `doctor`, `impact`, `context`, `validate`, `generate`, `verify`, `readiness`, `trace.export`; detection is the manifest/installed-tool/layout check. Each entry declares a recognized command ID, read-only versus generated-artifact mutation, output schema family/version/digest, supported options/bounds and any project/target prerequisites. Discovery supports no `init`, `install`, `update`, cleanup or sync operation. Product availability, project readiness and target capability are separate facts; a supported CLI operation does not promise that a target is configured.

Use a new identity such as `dev.lekalo.workflow-provider@<release>` rather than overloading the `protocol` family currently governing `lekalo.target/v1`. The extension adapts the accepted manifest to its own neutral capability model. Include output schema pins even where an operation uses the shared diagnostics failure envelope. Manifest operation/version mismatch is unsupported, not a validation failure. A known selected contract is mandatory; unknown fields/operations can only be accepted under an explicitly documented compatibility rule, never an inferred semver range.

No missing doctor/readiness/context implementation is required for the minimum. Trace export already exists, but the contract must specify its input manifest and preserve partial/unknown/stale/conflicting states. Add a CLI shim only if public conformance tests demonstrate a missing stable projection or bound; do not silently expand existing contracts. Test and document context failure to fit rather than promising every token budget can contain mandatory facts.

### Lekalo acceptance tests to add

1. Discovery outside any repository with a hostile or absent PATH: deterministic JSON; no child process, init, cache or canonical-file writes; manifest product/schema versions match compiled constants.
2. Existing public command argv matrix: valid/invalid/denied/unsupported/version refusal on the correct stream, malformed usage, direct receipt variants, warning diagnostics, readiness exit 0 with blocked verdict, verify degraded exit 4.
3. Context from a bounded changed-symbol set: fits and insufficient-budget cases, estimator/truncation preserved, no raw source/absolute path/secret; byte overflow cannot pass as complete output.
4. Drift and freshness: changed generated bytes, stale/missing lock/IR evidence, target capability refusal, stale trace and missing scenario evidence remain distinguishable.
5. Generation: explicit argv and write scope only; protected OpenSpec/HLV paths refused; check/readiness/discovery never invoke generation or mutate scaffolding.
6. A Node child-process consumer fixture invokes the built/released CLI and validates published JSON Schema, with no linking to `lekalo-core`; unsupported handshake and provider crash are distinct. This is Lekalo boundary conformance, not the extension's full lifecycle E2E.

Future validation commands: `cargo test --workspace --locked`, `node scripts/test-provider-contracts.mjs`, and `node scripts/check-contract-versions.mjs` with the implementation's comparison base, plus existing affected doctor/context/impact/trace/orchestration contract suites. These are proposed gates, not claims that implementation tests ran during this research.

### Reference extension implementation outline and exact touch set

1. Replace `scripts/lekalo-provider.mjs` stub with `detectLekalo` discovery/layout validation and `runLekaloOperation` reviewed argv dispatch. Add `normalizeLekaloResult` and a closed registry mapping native schema/exit/verdict/diagnostic classes to provider results. Process transport remains `provider-process.mjs`.
2. Change `scripts/aifhub-providers.mjs` to provider-specific operation plans, implement before/after substeps, scope/budget/target arguments, negotiated commandContract provenance, and Lekalo read-only done validation. Split generation from nonmutating validation; bind its receipt to both revisions, then obtain fresh post-generation results. Include required trace in gating instead of universally excluding every `trace` operation.
3. Change `schemas/provider-capabilities.schema.json`, `schemas/provider-evidence.schema.json`, `schemas/provider-semantic-evidence.schema.json`, `scripts/semantic-provider-contract.mjs`: version the extension successor explicitly; add generate/verify, Lekalo contract identity, native diagnostic IDs/codes/categories, per-operation schemas, context metadata, trace coverage and input binding. Update neutral negotiation/trace validation in `aifhub-providers.mjs` too.
4. Change `scripts/provider-policy.mjs`, `schemas/provider-config.schema.json` for literal `policy:off` and validated Lekalo selections/budget/target options if persisted there. Keep `aifhub.tools` precedence; no implicit enablement. Do not allow arbitrary provider-returned argv or reinterpret HLV settings.
5. Change `scripts/provider-files.mjs` for the exact Lekalo aggregate evidence path and bounded input inventory; retain safe atomic writes and isolation. Version input digest domains and validate both schema and cross-field invariants when reading evidence.
6. Change `injections/core/aif-implement-plan-folder.md`, `injections/core/aif-verify-plan-folder.md`, `skills/aif-done/SKILL.md`, `scripts/openspec-done-readiness.mjs`, and `skills/shared/TOOLS.md` to wire the specified sequence, new policy/freshness semantics and explicit no-Lekalo-init rule. Plan continues creating normal OpenSpec + AI Factory artifacts; no required Lekalo step in `/aif-plan`.
7. Update `docs/validation-providers.md`, `docs/usage.md`, `README.md`, `CHANGELOG.md` and relevant wrapper help. Existing `commands/aifhub-providers.mjs` forwards unknown options and may need no executable change; `extension.json` already registers the command. Verify packaging rather than adding a duplicate command. `scripts/aifhub-mcp-server.mjs` should normally reuse aggregation unchanged; adjust only response validation if the successor requires it.
8. Extend `scripts/lekalo-provider.test.mjs`, `scripts/validation-providers.test.mjs`, `scripts/openspec-done-readiness.test.mjs`, `scripts/aifhub-mcp-contract.test.mjs`, `scripts/tool-config.test.mjs`, `scripts/tool-initialization.test.mjs`, and fixtures under `test/fixtures/validation-providers/`; add lifecycle-hook tests. Keep HLV adapter behavior unchanged and cover all tool combinations.

Reference control flow: resolve phase/change/policy -> skip disabled/unconfigured phase -> safe discovery -> negotiate selected operation/schema -> bind inputs -> spawn reviewed argv -> parse the status-owned JSON stream -> validate schema and semantic invariants -> normalize and retain original diagnostics -> recheck nonmutating input revision -> apply phase policy -> write safe evidence. On done, inspect the latest observations and current identities before accepting readiness; never search backward for a convenient pass. No module should import Lekalo Rust internals.

## Evidence-file schema draft

**Proposed extension-owned successor**, not valid under the current v1.0.0 HLV-shaped schema. Keep HLV files readable under v1; define an explicit new Lekalo aggregate schema (illustratively `aifhub.lekalo-evidence/v1`) rather than adding unrecognized fields to old records. Its sole writer is AIFHub/AI Factory at the exact required path:

```text
.ai-factory/qa/<change-id>/providers/lekalo.json
```

Use one atomically replaced, closed object with phase/substep observations. A single latest-status slot is insufficient: an implement context observation must not erase a verify failure. Identical reruns preserve bytes and timestamps. Every attempt, including unavailable/crash/schema failure, must supersede the corresponding previous successful observation; malformed or stale records are not trusted history. Concurrent writers require a lock or compare-and-swap to avoid lost observations.

| Field | Type / draft rule |
| --- | --- |
| `schemaVersion`, `provider`, `kind` | Exact new envelope discriminator; constants `lekalo`, `semantic_model` |
| `changeId` | Same validated safe ID as resolved OpenSpec change or AI Factory plan; no branch-slug guessing |
| `policy`, `configuredPhases`, `policyDigest` | Effective policy, sorted selected phases and hash of effective configuration; off emits no new file |
| `tool` | Exact available product version, executable content identity when available, upstream provider-contract identity/digest, selected manifest digest and extension adapter contract version; null fields require an explicit discovery-failure reason |
| `observations` | Bounded map keyed by `implement.before`, `implement.after`, `verify`, `done`; each independent result has its own binding and timestamp |
| `observations[*].binding` | `changeId`, exact HEAD, source worktree digest, operation-input digest, selected target/profile identities, canonical Model/IR and lock digests where available; no absolute path |
| `observations[*].operations` | Bounded ordered operation results, unique operation/substep IDs, selected schema identity, original status/verdict, normalized status/reason, failure class, diagnostic list, stream-presence/exit metadata and payload digest |
| `observations[*].gate` | `pass\|warn\|fail`, boolean blocking; consistent with policy, configured phase and required operations |
| `observations[*].timestamp` | UTC observation timestamp owned by extension; freshness primarily derives from exact identities, not wall clock alone |
| `context` operation summary | Budget/estimated tokens, estimator pin, fits, minimumRequired, truncation/included/excluded counts, capsule digest; actual capsule delivered through local-private runtime channel |
| `trace` operation summary | Original manifest/schema digest, source revision, completeness/coverage, gap counts/statuses and validated occurrence-preserving projection or reference; no invented confirmed links |
| `generate` operation summary | Planned/applied/check mode, plan ID, allowed write-set digest and before/after bindings; only post-apply validation may attest the new state |
| `freshness` | Current/stale/missing/unknown plus bounded reason; computed by the consumer, never accepted solely from an old producer verdict |

Diagnostic draft: preserve `id`, exact `code`, `severity`, `category`, `schema_version`, `registry_version`, and namespaced `original_code` when present; include bounded schema-validated structured data/logical locations only at their permitted disclosure level. `failureClass` distinguishes validation/model/implementation, configuration, unsupported, policy and infrastructure. A normalized gate never overwrites the native classification. Do not persist raw stdout/stderr, provider prose, source text, credentials, environment or private absolute paths. Retain original diagnostics in the local process result; public/exportable evidence carries the policy-approved projection and payload digest, not a claim that a redacted record is byte-identical to the original.

The new schema must declare `additionalProperties:false`, size/count limits and nullable identity rules. Cross-field checks must enforce matching change IDs, tool/contract/schema pins, exit/stream/verdict consistency, diagnostic counts, unique operations, context completeness, trace revision and binding. SHA-256 is identity/integrity metadata, not anonymization or a signature. Publish digest domains: source inventory; operation inputs (including ignored generated artifacts/manifest, lock, target/config and explicitly consumed trace); canonical JSON payload excluding its own digest; and exact executable bytes. Never make QA output part of its own input hash.

Reuse original #22 trace node/relation IDs, occurrences, confidence and gap states when projecting. The extension's five-hash neutral links cannot express all of these; retain the original validated manifest in its authorized trace home or a permitted bounded projection/reference. Hashed semantic references alone are insufficient input for the implement agent; it must receive the actual bounded capsule through the permitted runtime channel.

## Policy and degradation semantics

Current configuration expresses off with `aifhub.tools.lekalo:false`; literal `providers.lekalo.policy:off` is **rejected**, even in existing negative tests. To satisfy #34 literally, add `off` as a provider-policy value and normalize it to no invocation/no new evidence while retaining boolean precedence: a false/omitted tool cannot be enabled by policy. Enabled provider without policy keeps the extension's current required default; show optional explicitly in examples.

```yaml
aifhub:
  tools:
    openspec: true
    hlv: true
    lekalo: true
  providers:
    lekalo:
      policy: optional
      phases: ["implement", "verify", "done"]
    hlv:
      policy: optional
      phases: ["verify", "done"]
```

| Situation | Off / phase not selected | Optional in selected phase | Required in selected phase |
| --- | --- | --- | --- |
| Tool/project absent | No detection/process/new evidence | `unavailable`, degraded warn | Blocking unavailable for this phase only |
| Unknown provider/schema version or missing required operation | No invocation | `unsupported`, degraded warn | Blocking unsupported; no guessed command |
| Model/implementation invalid, drift, strict denial | No invocation | Preserve native failure, gate warn | Preserve native failure, gate fail |
| Crash, signal, timeout, cancellation, output overflow | No invocation | Infrastructure error, degraded warn | Infrastructure error, blocking; never label it a test failure |
| Valid report with optional concern/missing optional trace | No invocation | Warn with explicit absence | Warn if genuinely optional; missing **required** operation/evidence blocks |
| Missing/stale required verify evidence during done | No invocation | Warn; no verified/completed-provider claim | Block done until fresh evidence exists |

Configuration ambiguity, unsafe path/write failure or source mutation during a supposedly read-only operation remains a host integrity failure, not an optional-provider absence. Keep this distinction explicit in diagnostics. Required Lekalo configured only for verify must not block implement, plan, or done; tool switches and phase membership are checked before discovery/revision/evidence work. Status/doctor may report the problem but cannot silently expand the phases that gate workflow.

Map readiness ready/degraded/blocked and verify component states explicitly. A required provider is not equivalent to making every optional native component mandatory, and an optional provider failure never becomes pass. Conversely, a missing operation selected as required for that phase cannot hide behind a generic warn.

Done freshness requires the current change, source and operation inputs, policy, tool executable/manifest/adapter contract, selected schemas, target/profile, lock and generated-artifact identities to agree with the latest relevant observations. A successful readiness call alone cannot refresh stale verify evidence. A new failure at the same revision supersedes a prior pass. Refresh validation explicitly in the done lifecycle if authorized; the read-only archive-readiness checker only reads evidence and performs read-only checks. Any timestamp TTL, if introduced, must be explicit policy and additive to identity checks.

All-provider composition preserves the issue's sequence: `/aif-plan` creates OpenSpec/AI Factory artifacts; `/aif-implement` gets impact/context before edits and validates/generates afterward; `/aif-verify` records Lekalo validation/verification/trace, then native gate results, then optional HLV; `/aif-done` checks freshness/readiness/drift before ordinary finalization. Generation is an explicit implementation action within the existing command, not permission inferred from detection or validation opt-in. Missing prerequisites return diagnostics with explicit setup advice; never run install/update/init/sync as recovery.

## Requirement and acceptance coverage

| #34 requirement / acceptance | Deliverable and proof |
| --- | --- |
| Every provider operation | Command mapping above; manifest inventory and CLI contract tests; extension provider-specific schedule |
| `off\|optional\|required`; required blocks configured phase only | Policy normalization successor; matrix tests across all phases, including false/omitted tool and explicit off |
| External CLI/process JSON boundary | Public binary child-process tests; reuse bounded process runner; no internal crate coupling |
| Capability/version negotiation | Published Lekalo discovery/schema pins + extension accept/refuse fixtures for exact protocol/operation versions |
| Optional absence degrades | Missing executable, missing project and unsupported version fixtures; warning gate with native unavailable/unsupported retained |
| Exact QA path and no OpenSpec writes | Assert `providers/lekalo.json`; canonical trees hash-equal across read-only phases and forbidden generation-path test |
| Exact change/revision binding | Wrong change, committed/dirty/untracked/ignored input, policy/tool/target change, stale trace and in-flight mutation fixtures |
| No hidden install/update/init | Fake process argv log plus before/after filesystem inventory; missing-tool/project cases never run setup |
| AI Factory-only continues | No providers configured: no process/revision/evidence side effects and unchanged host lifecycle |
| OpenSpec + Lekalo + HLV simultaneously | Extension E2E with published CLI/protocol, existing HLV adapter, separate evidence and canonical ownership assertions |
| Implement receives bounded context capsule | Real impact-to-ID-to-context flow, fits and insufficient-budget fixtures, capsule delivered locally and digest retained in QA |
| Verify preserves original diagnostics + normalized gate | Semantic-invalid fixture with exact LEK codes/IDs and registry versions; native result and policy gate inspected separately |
| Stale evidence blocks done by policy | Required versus optional stale/missing/malformed/newer-failure cases; read-only done must not synthesize fresh verify |
| Provider crash distinct from implementation failure | Kill/timeout/no-JSON fixtures versus valid invalid-model/failed-target receipts; distinct failureClass/reason |
| E2E uses published protocol, not Rust internals | Extension test launches release CLI with published schema bundle; Node/TypeScript consumer first, explicit PHP/Laravel target coverage thereafter |

## Risks

1. **False readiness from exit 0.** Doctor/readiness deliberately emit successful reports even when blocked. Receipt-aware normalization and negative fixtures are release blockers.
2. **Schema drift hidden by stub comments.** Product, target protocol, diagnostic registry and operation schemas differ. Publish all relevant pins and refresh extension documentation instead of keying capabilities to old v0.1.10 assumptions.
3. **Incomplete extension seam.** Shared empty implement schedule, absent verify/generate enum values, HLV-only evidence schema and HLV-only done validator require changes outside `lekalo-provider.mjs`.
4. **Incomplete input binding.** Ignored `.lekalo` artifact bytes and ignored plan/trace/context inputs may escape the current digest. Bind consumed inputs explicitly and test each changed-byte case; avoid self-invalidating evidence hashes.
5. **Expected generation writes versus mutation detection.** Separate generation transaction and revalidate the new state; never disable mutation detection for all provider commands.
6. **Lossy diagnostics/trace/context.** HLV code regex rejects Lekalo codes; neutral hashed links lose occurrences/gaps; a context digest alone is not an agent capsule. Version schemas and enforce privacy without erasing semantic failure evidence.
7. **Resource mismatches.** Core 32 MiB exports and ten-minute target defaults exceed host caps. Return bounded explicit unsupported/overflow states, constrain scope/budgets, and align nested deadlines; no truncated JSON pass.
8. **Premature native/E2E claims.** Native gate execution remains outside production Lekalo verify; current extension tests use fake providers. Full acceptance needs a released compatible CLI and a real composed lifecycle fixture, not only passing normalization tests.
9. **Automatic scaffolding inheritance.** Existing shared HLV/OpenSpec setup rules must not cause hidden Lekalo init. This requires a specific extension contract/test, even though discovery itself is read-only.
10. **Aggregate evidence races and stale-pass fallback.** One exact file needs serialized/CAS updates and explicit latest-observation semantics; retain per-phase bindings and never replace a failed verify with an unrelated successful context call.

## Research validation and completion boundary

Ran existing extension tests, read-only with respect to its checkout:

```text
node --test scripts/lekalo-provider.test.mjs scripts/validation-providers.test.mjs
19 tests passed, 0 failed (about 30 seconds, Windows).
```

These establish the current fail-closed stub and existing fake-process policy/freshness/transport behavior, including optional degradation, diagnostic-code preservation, safe files and descendant cleanup. They do not demonstrate real Lekalo integration or satisfy #34's future E2E. No provider was installed, initialized or updated, no release build or production native gate was run, and no source implementation was changed. The requested deliverable is this committed research report only; implementation and cross-repository release/acceptance remain future work.
