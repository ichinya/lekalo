# Issue #41: managed ownership and deterministic implementation generation research

Research only, 2026-10-06. Acceptance authority: [live issue #41](https://github.com/ichinya/lekalo/issues/41), retrieved with `gh issue view 41 --repo ichinya/lekalo --json title,body,labels`. Parent #1 and dependencies #21/#23/#27/#30/#40 were read only for context. The issue defines three mode statements, ten capabilities, five constraints and seven acceptance criteria; each is mapped below. Inspected checkout: `ichinya/m8-issue-41`, starting HEAD `15bd13917654dc20ad19b089a5bb8e5e29c2099f`, initially clean. The supplied integration target is `ichinya/M8`, product `0.8.0`; the inspected source still says `0.6.5` in `Cargo.toml:9`. This research does not bump the product or any contract.

House references: `docs/m7/issue-37-research.md`, `docs/m7/issue-75-research.md` and `docs/m7/issue-84-research.md`; version authority: `docs/versioning.md`. Follow their source inventory, explicit fact/proposal separation, closed artifact design, diagnostic inventory, acceptance mapping, fixture/gate plan and risk sections. Historical proposals in those documents are not proof that a surface is absent on this HEAD.

**Recommendation (proposal):** implement managed as an explicit per-module ownership assignment plus a capability-qualified generation transaction over the existing artifact/protocol/orchestration owners. Model and pinned semantic attachments determine application behavior; adapters determine target rendering; explicit custom/foreign bindings determine maintained extension points. Never infer managed permission from the mere availability of the `generate` operation. Promotion, regeneration, cleanup, upgrade and rollback must consume reviewed plans and retain exact ownership and behavior evidence.

**Evidence level:** live issue retrieval and source/contract/test inspection, followed by documentation checks. No adapter execution, generated implementation, runtime acceptance, behavioral experiment, hosted CI or release is claimed. Only this research file and its local commit are delivered. Every section headed "Proposed", and every new path, field, flag, code, test or example introduced there, is a future proposal rather than an existing public interface.

## Existing-surface inventory

| Surface | Verified owner / contract | Existing behavior and exact remaining gap |
| --- | --- | --- |
| Version policy | `Cargo.toml:9`; `docs/versioning.md`; `scripts/check-contract-versions.mjs` | Product is currently `0.6.5`. Changed/new contracts take the product version of the implementing commit; unchanged families retain their exact bytes/version. The dispatched M8 implementation target is `0.8.0`. |
| Generate CLI | `crates/lekalo-cli/src/main.rs:450`, `:9549` | Existing flags are `--check`, `--locked`, `--clean`, `--dry-run`, `--confirm`, repeated `--target`, `--module`, `--timeout-ms`, `--allow-permission-expansion`, report selectors and `-- PROGRAM [ARGS...]`. `--confirm` currently confirms clean only; `--check` rejects adapters, target/module filters and dry-run. No managed promotion, saved generation-plan apply or generated-manifest rollback interface exists here. |
| Generation orchestration | `crates/lekalo-core/src/orchestration/generate.rs:46`, `:119`, `:460`; `docs/orchestration.md` | Existing pipeline compiles inputs, discovers locked adapter, plans through the process protocol, applies and verifies bytes, then updates ownership. `run_target` checks target declaration and IR read scope; these checks do not enumerate all semantics required for a managed module. |
| Dry-run writes | `crates/lekalo-core/src/orchestration/generate.rs:146`, `:816` | `run` materializes canonical IR and applicable evidence before adapter planning, including dry-run. Thus existing dry-run is not a proof of zero writes to the project. The new preview should use an external bounded staging area and leave project/cache/manifest/lock bytes and directory inventory unchanged. |
| Module selection | `crates/lekalo-core/src/orchestration/generate.rs:775`, `:797`; `docs/orchestration.md` | `--module` is attribution scope. Existing wire calls do not carry a module selection. Prefix filtering in `scope_inputs` is not a sufficient exact module boundary for managed generation; do not treat attribution as adapter execution isolation. |
| Artifact manifest | `crates/lekalo-core/src/artifacts/types.rs:22`; `contracts/artifact-manifest.schema.v0.2.16.json` | One manifest at `.lekalo/generated/manifests/ownership.json`, with exact lock/input/adapter pins, content digests, ownership and source maps. Lifecycles distinguish generated/scaffolded/checked/external/custom. Reuse this owner rather than introduce a parallel manifest. |
| Drift check | `crates/lekalo-core/src/artifacts/check.rs`; `crates/lekalo-core/src/artifacts/diagnostic.rs:127`, `:158`; `contracts/generate-check-receipt.schema.v0.6.3.json` | Hermetic `GenerateService::check` reads exact bytes, never repairs them. Manual drift maps to `lock.source-changed` (`LEK-LOCK-018`). Generation's `Prepared::prepare` loads inputs/manifest but is not itself this drift gate. Managed apply must explicitly validate the recorded generated baseline before replacing any bytes. |
| Protected files and publication | `crates/lekalo-core/src/orchestration/generate.rs:850`, `:921`; `crates/lekalo-core/src/target_protocol/plan.rs` | Existing ownership validation refuses non-generated lifecycles and manifest bookkeeping. Protocol validates declared writes and stages publication. Manifest update occurs afterward; durable whole-generation rollback and recovery spanning artifacts, source maps, lock/config and manifest require additional coordination. |
| Partial manifest preservation | `crates/lekalo-core/src/orchestration/generate.rs:959`, `:974`, `:1009` | `update_manifest` retains untouched non-generated entries, but does not add untouched generated entries to `keep`; source maps are rebuilt from the current writes. Managed scoped runs must preserve untouched generated entries/maps, reconcile current input pins and never drop another module/target's ownership. This is source inspection, not a reproduced runtime finding. |
| Source maps and ownership attribution | `crates/lekalo-core/src/orchestration/generate.rs:943`, `:1009`, `:1114`; `crates/lekalo-core/src/artifacts/source_map.rs` | Current generated anchor is `<project>.generated`, input refs are scope-wide, and selected `.map.json` sidecars are ingested. Existing half-open byte ranges and input revision validation are reusable; full managed coverage needs explicit symbol/module attribution for every implementation/test artifact and retained maps across scoped runs. |
| Adapter protocol | `crates/lekalo-core/src/target_protocol/version.rs:12`, `:15`, `:25`; `contracts/target-protocol.schema.v0.3.2.json`, `contracts/target-protocol.schema.v0.6.4.json` | `lekalo.target/v1`, ordinary protocol `0.3.2`, additive lint seam `0.6.4`; separate processes and bounded declared scopes/plans. Existing operation vocabulary is not a managed semantics certificate. A negotiated successor must carry exact module scope, semantic coverage, extension selections and typed artifact/source-map metadata. |
| Escape hatches | `crates/lekalo-core/src/implementation/{wire,canonical,mod}.rs`; `contracts/implementation.schema.v0.2.16.json`; `docs/implementation.md` | Explicit generated/custom/foreign/external/unsupported kinds, selection and declared-effect restrictions already exist. Scenarios apply to operations across implementations. The attachment alone does not prove target symbol existence/signature/effect enforcement; adapter evidence must qualify the selected binding. |
| Contracted mode | `crates/lekalo-core/src/contracted/{check,types,store}.rs`; `contracts/contracted-declaration.schema.v0.4.0.json`; `docs/contracted-mode.md` | Maintained handlers remain user-owned; registry fingerprints and support claims exist. Promotion must compare an exact contracted baseline, identify retained extension points and refuse unmodelled maintained logic. Existing support generation does not constitute regenerated handler semantics. |
| Target profiles / expressions | `crates/lekalo-core/src/target_profile/{document,resolution}.rs`; `crates/lekalo-cli/src/main.rs:1854` | Profile composition/required capability checks and expression built-in coverage checks exist. Their static catalogs are useful requirements inputs, but cannot by themselves certify the executable adapter's complete generation capability for a module. |
| Scenario oracle | `crates/lekalo-core/src/reference_evaluation/mod.rs`; `contracts/{scenario-ir,reference-evaluation}.schema.v0.2.16.json`; `contracts/scenario-run.schema.v0.4.0.json` | Reference evaluator handles a declared deterministic subset and explicitly lacks authorization, expression, race schedule and other capabilities. Unsupported assertion/reference coverage cannot become a behavioral-equivalence pass. Native execution/evidence must remain adapter-owned. |
| Planner E2E | `scripts/test-golden-planner-e2e.mjs`; `tests/fixtures/orchestration/project/`; `tests/fixtures/reference-evaluation/model/` | Existing chain exercises Model/IR and generation/scenario seams; the script documents residual corpus/trace joins. It does not prove deleting all planner target implementation and rebuilding it solely from Model. A new linked managed planner chain is needed. |
| Authority and structure | `contracts/authority-contracts.manifest.json`; `contracts/authority-matrix.v0.3.2.json`; `scripts/check-structure.mjs:23`; `docs/canonical-structure.md` | Current authority is `0.3.2`; generated-intermediate home includes `.lekalo/generated/**`. Canonical root entries and runtime homes are closed. Do not invent `lekalo/ownership.json` or a top-level `.lekalo/managed/` directory without a governed successor. |
| Diagnostics and CI | `crates/lekalo-core/src/diagnostics/version.rs:14`, `:51`, `:54`; `.github/workflows/ci.yml:223`, `:400` | Ordinary registry `0.6.4` has 514 entries; opt-in architecture successor `0.6.5` has 522. CI builds the workspace before later live golden gates. New managed integration must preserve both producer pin semantics and the union of existing rule allocations. |

## Proposed goal decomposition and implementable increments

All stages are required for the issue; publishing a configuration schema or a support-artifact generator alone does not complete managed mode.

| Increment | Implementation owners and deliverable | Independently observable boundary |
| --- | --- | --- |
| I1 — ownership assignment and semantic admission | New `crates/lekalo-core/src/managed/{mod,wire,coverage,diagnostic}.rs`, CLI dispatch, closed ownership/coverage contracts, lock pin integration. Exact project/module/target assignments for observed/contracted/managed; derive required semantics from compiled IR plus applicable pinned attachments and scenarios. | Mixed-mode synthetic project remains unchanged after inspection; missing/partial/unsupported/unknown required capability refuses managed selection before any apply. No handwritten fallback. |
| I2 — deterministic planning and complete output inventory | New `managed/{plan,canonical}.rs`; negotiated protocol successor; adapter recipe for implementation **and tests**, explicit artifact roles/owners and symbol maps. Saved immutable plan includes exact before-state and expected output digests. | Same IR + lock gives the same paths/bytes/plan identity in fresh roots; project-wide preview leaves no files/directories/cache changes. Output inventory covers the complete selected managed subtree, not only reported writes. |
| I3 — safe promotion/regeneration and custom boundaries | New `managed/apply.rs` integrates existing confinement/plan checks, pre-apply drift validation and staged manifest merge. Explicit extension resolution uses #30 bindings and adapter checks. | Maintained files only change when exact paths are explicitly surrendered in a confirmed promotion plan; generated edits block regeneration; custom/scaffolded/checked/external paths remain untouched. Other modules/targets retain manifest entries/maps. |
| I4 — cleanup, adapter upgrade and rollback | New `managed/{history,rollback}.rs`; whole-generation journal and content-addressed before/after snapshots; existing orphan-clean flow extended with module ownership. Upgrade plan diffs exact old/new adapter+lock pins and behavior evidence. | Orphan cleanup requires exact preview confirmation; unknown/unclaimed files are never guessed disposable. Failure/cancellation restores old generation, and explicit rollback restores the previous manifest and exact generated bytes without rerunning an old adapter. |
| I5 — behavioral equivalence and delete/reproduce planner | New `managed/equivalence.rs`, adapter implementation recipe, reference/native normalized scenario comparison and deterministic planner chain. | Regenerated planner executes the same supported scenarios with independently checked results/state/errors/effects. Unsupported coverage is a visible blocker. Delete only generated target outputs in a disposable fixture, regenerate from Model, then compare bytes, manifest/maps and scenario evidence. |
| I6 — hostile fixtures, registry union and mandatory CI | Managed schema/live CLI gates, fixture provenance/catalog/diagnostic producers, docs and version-parity wiring. | Real binary and adapter gates on Linux/macOS/Windows; no missing-binary/adapter skip, canned output substitution or schema-only acceptance. All seven AC rows and ten capabilities have passing evidence. |

## Proposed command surface and ownership

### Existing commands retained

These spellings already exist; future managed enforcement extends their behavior without claiming the current implementation has it:

```text
lekalo --json generate --check --locked --project .
lekalo --json generate --clean --dry-run --project .
lekalo --json generate --clean --confirm sha256:<plan-digest> --project .
lekalo --json generate --target node-typescript --module planner --dry-run --locked --project . -- node adapters/managed.mjs
lekalo --json verify --target node-typescript --module planner --locked --project . -- node adapters/managed.mjs
```

`adapters/managed.mjs` is a proposed fixture executable, not an existing adapter. In current generate, `--confirm` confirms cleanup only; a generation apply does not consume a user-saved plan. Current dry-run can update evidence cache and current `--module` is attribution only. Those gaps must be corrected/negotiated for the managed lane rather than hidden in examples.

### Proposed explicit management and generation flags

Use `config/lekalo-ownership.json` as an example of caller-owned project configuration **outside** the reserved canonical Model/runtime trees. It is an ownership decision, never recovered from target source. Bind its safe project-relative coordinate and canonical digest into a reviewed lock successor through proposed `lock --ownership FILE`; subsequent commands require the lock-pinned assignment, and an optional `--ownership FILE` must match that pin. Omitting the flag cannot bypass a managed assignment already recorded in the lock. Projects without an ownership pin retain their existing behavior.

```text
# Proposed: pin the explicit ownership configuration with existing adapter discovery.
lekalo --json lock --ownership config/lekalo-ownership.json --adapter node --project . -- adapters/managed.mjs

# Proposed: inspect full semantic coverage and ownership without writes.
lekalo --json managed check --ownership config/lekalo-ownership.json --module planner --target node-typescript --project . -- node adapters/managed.mjs

# Proposed: preview contracted -> managed promotion; result is returned on stdout.
lekalo --json managed plan --to managed --ownership config/lekalo-ownership.json --module planner --target node-typescript --locked --project . -- node adapters/managed.mjs

# Proposed: consume a caller-saved closed plan and its exact digest.
lekalo --json managed apply --plan config/planner-managed-plan.json --confirm sha256:<plan-digest> --project . -- node adapters/managed.mjs

# Proposed: regeneration preview and an explicit saved-plan apply.
lekalo --json generate --ownership config/lekalo-ownership.json --module planner --target node-typescript --dry-run --locked --project . -- node adapters/managed.mjs
lekalo --json generate --plan config/planner-generation-plan.json --confirm sha256:<plan-digest> --locked --project . -- node adapters/managed.mjs

# Proposed: before/after exact adapter-lock diff; no lock update or output writes.
lekalo --json managed upgrade-plan --module planner --target node-typescript --candidate-lock config/lekalo-candidate.lock --project . -- node adapters/managed-next.mjs

# Proposed: preview and confirm restoration of a previous generated generation.
lekalo --json managed rollback --to sha256:<generation-digest> --dry-run --project .
lekalo --json managed rollback --to sha256:<generation-digest> --confirm sha256:<rollback-plan-digest> --project .

# Proposed: explicit resolution only by restoring exact recorded generated bytes.
lekalo --json managed resolve --path apps/api/src/planner/focus-task.ts --restore --dry-run --project .
lekalo --json managed resolve --path apps/api/src/planner/focus-task.ts --restore --confirm sha256:<resolve-plan-digest> --project .
```

All proposed mutation modes reject conflicting selectors, malformed identities and missing confirmation. Saved plans are closed bounded JSON, not scripts. Apply rechecks exact Model/IR/lock/config/attachment/adapter/binding/before-state pins; changing any invalidates the plan. `generate --check` must remain write-free, including with managed assignments, on clean, missing, drifted and denied inputs. CI report files are writes: preserve ordinary explicit reporting, but do not include such flags in a zero-write invocation or claim it was write-free.

Promotion uses the generation planner and transaction, not an independent emitter. It only changes the selected ownership assignment to managed after capability, output ownership and behavior gates pass. The plan lists every maintained path being surrendered, every preserved extension, source/manifest changes and resulting lock/config pins; an undeclared maintained handler cannot be overwritten. The full observed -> contracted -> managed migration workflow belongs to related roadmap #111; this issue supplies only the checked managed admission/promotion boundary needed here.

Resolution never accepts manually edited generated bytes as a new baseline. Restoration consumes retained exact bytes through the same path/before-state transaction checks. Preserving custom logic instead requires an explicit reviewed #30 binding plus a new promotion/generation plan; there is no `--force`, implicit adoption or textual magic-region repair.

### Proposed runtime custody

Keep the one ownership manifest at its accepted path. Proposed generation history, snapshots and transaction metadata live under `.lekalo/generated/manifests/managed/`, inside the already accepted generated-intermediate home; adapters are denied that whole bookkeeping subtree. The artifact checker/cleaner must explicitly classify these owner files and must never enumerate snapshots as generated application orphans. A schema/path admission review is still required; an accepted broad authority home does not by itself define a safe cleanup policy.

Preview/check uses a bounded external temporary staging tree, which is removed after evaluation and never becomes project semantic input. Apply creates a bounded staged generation and journal, compares all before-state pins, publishes generated files, maps, ownership/config/lock updates as one recoverable transaction and records the committed generation. Since multi-file filesystem replacement is not inherently atomic, define journal states `prepared`, `publishing`, `committed`, `restoring`, and recovery behavior explicitly; unresolved transactions block further mutation. Reuse the existing transaction/confinement primitives rather than promise atomicity from one manifest rename.

## Proposed contract artifacts and versioning

Use `0.8.0` for every new or actually changed contract in the implementing M8 commit. The current research commit adds no contracts. All new schemas use closed nested objects, bounded arrays/tokens/files, duplicate-key refusal at runtime, exact family/discriminator/version and canonical digest domains.

| Proposed artifact | Identity / minimum content | Owner and compatibility consequence |
| --- | --- | --- |
| `contracts/managed-ownership.schema.v0.8.0.json` | `lekalo/managed-ownership/v0.8.0`, `dev.lekalo.managed-ownership@0.8.0`; project and unique exact module assignments, observed/contracted/managed mode, target/profile refs, implementation attachment refs, extension selections and generated roots. | Caller-owned explicit config; no Model keys or automatic inferred promotion. Exact compiled module IDs, no wildcard/prefix matches or overlapping target ownership. |
| `contracts/managed-coverage.schema.v0.8.0.json` | `lekalo/managed-coverage/v0.8.0`, `dev.lekalo.managed-coverage@0.8.0`; required semantics per symbol/attachment/assertion, adapter recipe/version/digest, full/partial/unsupported/unknown support with qualified witnesses and selected extension checks. | Core requirements, adapter capability evidence. Only full qualified support discharges a generation obligation; custom/foreign exemption must name a validated explicit binding and its preserved contract. |
| `contracts/managed-generation-plan.schema.v0.8.0.json` | `lekalo/managed-generation-plan/v0.8.0`, `dev.lekalo.managed-generation-plan@0.8.0`; action promotion/regeneration/upgrade, input closure pins, selected modules/targets, before manifest/generation, coverage, extensions, complete files/maps, creates/replaces/deletes, before/after digests and non-self-referential plan digest. | One immutable reviewable plan; derive identity over canonical payload excluding self-digest, no timestamps/root paths. Protocol's opaque `plan-*` ID is a separate bound field, never confused with CLI `sha256:*` confirmation. |
| `contracts/managed-generation-record.schema.v0.8.0.json` | `lekalo/managed-generation-record/v0.8.0`, `dev.lekalo.managed-generation-record@0.8.0`; previous/current manifest+generation refs, plan/lock/ownership pins, exact file snapshot refs, source maps and behavioral evidence. | Derived local custody, not canonical semantics or a public export. Retained bytes permit adapter-independent rollback; content-addressed private snapshots are verified before use. |
| `contracts/managed-transaction.schema.v0.8.0.json` | `lekalo/managed-transaction/v0.8.0`, `dev.lekalo.managed-transaction@0.8.0`; state, action, exact before/after refs, bounded stage coordinates, conflict/recovery status and affected file set. | Runtime recovery state is separate from deterministic application output/manifest. No machine time/random data enters the stable artifact digest. |
| `contracts/managed-rollback-plan.schema.v0.8.0.json` | `lekalo/managed-rollback-plan/v0.8.0`, `dev.lekalo.managed-rollback-plan@0.8.0`; current generation/before-state, retained destination generation, exact restore/delete set and confirmation digest. | Restore previous files/maps/manifest and matching lock/ownership pins; never restore Model or custom source. Current incompatible Model must first be restored by the caller or rollback refuses. |
| `contracts/managed-equivalence.schema.v0.8.0.json` | `lekalo/managed-equivalence/v0.8.0`, `dev.lekalo.managed-equivalence@0.8.0`; scenario/IR/semantic-attachment pins, implementation generation refs, native/reference recipe pins, normalized result/state/error/effect assertions, supported/failed/unsupported coverage and verdict. | Neutral behavior evidence; excludes timing metrics and raw source. Fixture oracle/native comparisons are distinct from a claim of universal production equivalence. |
| `contracts/target-protocol.schema.v0.8.0.json` | Successor discriminator/identity at `0.8.0`; keep the major transport token `lekalo.target/v1` only if its negotiation remains valid. Add typed managed capability report, module/semantic closure request, selected bindings and per-file ownership/maps. | Advertised/negotiated only; frozen `0.3.2`/`0.6.4` exchange shapes remain unchanged. Old adapters remain usable for prior lanes, but cannot impersonate managed support. If the envelope change is incompatible with v1 negotiation, explicitly change the transport major as part of this successor. |
| `contracts/artifact-manifest.schema.v0.8.0.json` | Extend accepted manifest with generation reference and explicit module/target attribution needed for mixed managed ownership. Preserve exact lock/input/adapter/content/map domains. | Change requires actual successor and parser/constants/producer/gate updates. Frozen `v0.2.16` must not receive new members. Existing maps remain byte ranges; no second ownership manifest. |
| `contracts/lock.schema.v0.8.0.json` | Ownership config and complete generation-input closure refs, exact protocol/adapter/profile/recipe pins and per-module generation requirements. | Required so same IR + lock identifies **all** semantic/generator inputs. Keep frozen `v0.3.2` unchanged; explicitly support/admit the successor rather than renaming every lock. |
| `contracts/orchestration-report.schema.v0.8.0.json` and `contracts/generate-check-receipt.schema.v0.8.0.json` | Add managed scope/coverage/plan/generation/equivalence refs and mandatory managed verification components; check retains drift and no-write verdict. | These additions change closed receipts and therefore require successors, dispatch/report consumers and golden updates. Reuse status-owned `DomainResult`; do not invent a parallel exit scheme. |
| `contracts/diagnostic-registry{,.schema}.v0.8.0.json`; `contracts/validation-profile{.schema,.default,.strict}.v0.8.0.json` | Union of current allocations plus proposed managed rules, exact registry pins and protected mandatory errors. | Preserve 514 ordinary and 522 successor allocations, including architecture additions; revise embedded versions/registry selection and all actual consumers that change pins. Do not erase opt-in producer distinctions without an explicit integration decision. |

**Frozen unless their actual bytes change:** Model and typed IR `0.2.16`, Scenario IR/reference-evaluation `0.2.16`, implementation attachment `0.2.16`, target-profile `0.2.16`, contracted-declaration `0.4.0`, ordinary/lint protocol predecessors, diagnostic item schema `0.2.16`, current authority matrix/privacy families. New ownership configuration stays outside closed canonical homes, and derived custody stays inside the admitted runtime home. If implementation needs a new canonical home/authority kind instead, publish the governed authority/structure successor and exact admission tests at `0.8.0`; do not refresh accepted custody digests just to quiet a checker.

## Proposed managed semantics and determinism strategy

The issue's mode statements are the semantic contract:

```text
Lekalo Model owns supported application semantics.
Adapter generates implementation and tests deterministically.
Manual logic lives only in declared custom/foreign extension points.
```

Translate these into a closed requirement ledger. Enumerate commands/queries and their input/output/errors, entities/state, expressions, effects/events/jobs/external ports, invariants/transitions, policies/actors, transaction/concurrency obligations and every scenario assertion/control applicable to the selected exact module. Required attachments must be present, current and pinned; absence of coverage is unknown rather than zero obligations. Target-profile support, adapter executable digest, protocol support, generation recipe and native scenario runner coverage must all match. A general `generate` operation or a static profile saying "full" is insufficient.

A supported basic planner slice may be the first recipe, but it must explicitly reject every unsupported required semantic construct. Generated implementation tests must call generated handlers, not a maintained stub or the reference evaluator hidden behind a native-looking test. Foreign/custom bindings replace only implementation and are selected explicitly; all canonical signature/error/effect and scenario obligations still apply. No magic comments/regions, arbitrary setup scripts, free-form handwritten snippets in plans, or silent scaffold-as-managed mode.

The generation function is `G(canonical IR, exact lock) -> exact generated file set + bytes + canonical manifest/maps`. Lock closure additionally pins applicable semantic/scenario/implementation attachments, ownership configuration, target/profile resolution, adapter package+entry bytes, recipe/toolchain and templates. This makes "same IR + lock" meaningful; any unpinned attachment, environment-driven generator option or current implementation read would violate AC1. Custom source may be fingerprinted and checked for binding/conformance and before-state protection, but it cannot supply generated application semantics. The generator receives only admitted IR/attachment/binding descriptors; it cannot parse arbitrary old implementation to reconstruct behavior.

Sort all output paths, artifact keys, owners, input refs, diagnostics and map entries by a fixed byte order; reject duplicate/case-colliding paths and overlapping generated roots. Exact file bytes mean fixed UTF-8/LF and renderer conventions, without normalizing observed drift away. Eliminate wall-clock/randomness, host absolute paths, locale/timezone, discovery order and uncontrolled formatter/dependency versions from generated bytes. Prefer deterministic headers containing contract/recipe pins rather than dates. Model source locations may affect diagnostic presentation only; source-map generated ranges must bind exact generated bytes and stable semantic IDs.

Define the stable generation digest over the pinned input closure and complete output inventory only. The manifest's generation reference uses that digest; previous-generation links, transaction attempt identity, before-state and plan action differences belong to separate history/journal records and never enter stable manifest bytes. Repeated no-op generation and a clean rebuild therefore have the same generated files, maps and manifest, even if their review plans legitimately differ as create versus unchanged/replace actions. A history record may have a separate custody digest without changing the deterministic generation identity.

Before apply, validate both current input admission and generated baseline drift. A legitimate Model/lock upgrade may make the old manifest's input pins stale; do **not** simply call a current-input drift gate and disable it on stale status. Verify old manifest integrity and its recorded generated bytes against its own previous generation first, then compare old/new pinned inputs through the upgrade/regeneration plan. Any manually changed recorded generated artifact blocks writes until an explicit resolve transaction. Unknown/unclaimed existing output conflicts block creation.

Complete inventory is checked independently against all declared generated roots and transaction writes. Preserve all unaffected manifest entries and source maps; shared outputs require declared multi-owner semantics and coordinated scope, never last-writer attribution. A global manifest's input revision cannot be rebased over unrelated outputs rendered from old IR. Regenerate the affected full dependency closure before advancing global input pins, or retain explicit per-generation input refs through the manifest successor. Choose and test one policy; proposal default is closure regeneration plus preserved unchanged files whose semantic input fingerprints remain equal.

Delete actions are only for exactly recorded generated ownership, within the confirmed scope, with matching before digests. Custom/scaffolded/checked/external entries cannot enter cleanup. Unclaimed orphans are visible but require a reviewed ownership-aware decision before deletion; broad recursive cleanup is not safe. Path traversal, symlink/junction escape, changed parent directories, Windows aliases/devices and special files use the existing structure/confinement refusals. Revalidate immediately before publication; hold a project mutation lock to prevent concurrent generation/clean/rollback.

Rollback restores the retained previous generation's exact files/maps/manifest plus matching ownership/lock state; it never executes arbitrary previous implementation or untrusted old adapter packages. Require the current generation and before-state to match; keep custom source unchanged. An upgrade rollback may repoint lock pins to the retained predecessor without launching that adapter. Recheck current Model/semantic inputs against the destination generation: refuse rather than install an implementation for a different current Model. Cancellation/recovery cannot publish success until journal reconciliation proves one complete generation.

Behavioral equivalence compares canonical observable result/errors, state deltas, permitted/forbidden effects, events/jobs, authorization outcomes and deterministic clock/ID/idempotency assertions for the exact supported scenario corpus. Compare new generated output against the supported reference and, for upgrades/promotion, the accepted prior implementation baseline. Missing, stale, partial or unsupported required scenario coverage blocks managed admission/equivalence rather than lowering the denominator. The current reference evaluator's unsupported capabilities require separately qualified native oracles or rejection of that recipe; expanding a general reference evaluator or delivering all-target production equivalence is outside this issue.

## Proposed diagnostics and registry implications

Retain existing source diagnostics, including `LEK-LOCK-018` (`lock.source-changed`), `LEK-TGT-004` (`target.capability-unsupported`), `LEK-TGT-006` (`target.protected-path`) and `LEK-TGT-008` (`target.plan-mismatch`). Add a managed family only for distinct admission/transaction/coverage decisions. Candidate codes below are proposals; `LEK-MGD-*` was absent from both inspected registries and must be checked again on the implementing base.

| Proposed ID / code | Trigger and severity | Allowed status / next action |
| --- | --- | --- |
| `managed.input-invalid` / `LEK-MGD-001` | Closed configuration/plan/history violation, duplicate/cross-project assignment or invalid module; error. | invalid/1; fix exact shape/reference rather than infer a default mode. |
| `managed.version-unsupported` / `LEK-MGD-002` | Unsupported managed schema/recipe or required successor not negotiated; error. | unsupported-version/5; use an explicitly supported contract/adapter. |
| `managed.capability-gap` / `LEK-MGD-003` | Required generation or assertion semantics lack qualified full support; error. | unsupported/4; select a covering adapter/recipe or retain contracted mode; no fallback implementation. |
| `managed.generated-drift` / `LEK-MGD-004` | Exact recorded generated bytes are manually changed/missing at apply preflight; error. | invalid/1; inspect witnessed paths and use explicit restore or reviewed extension conversion. Original drift diagnostics remain linked. |
| `managed.extension-unresolved` / `LEK-MGD-005` | Required custom/foreign selection missing, ambiguous, stale or not adapter-validated; error. | invalid/1; provide one qualified explicit binding, preserving declared contracts/effects. |
| `managed.plan-stale` / `LEK-MGD-006` | Any input/lock/config/adapter/binding/current generation/before-state differs from reviewed plan; error. | invalid/1; recompute/review a new plan, never silently refresh its identity. |
| `managed.artifact-coverage-incomplete` / `LEK-MGD-007` | Unmanifested output, missing owner/map, undeclared generated path, dropped unaffected entry or conflicting ownership; error. | invalid/1; repair complete producer inventory before publication. |
| `managed.equivalence-failed` / `LEK-MGD-008` | Qualified supported behavior differs on an independently checked assertion; error. | invalid/1; retain evidence and prior generation, fix semantic rendering. |
| `managed.equivalence-incomplete` / `LEK-MGD-009` | Required oracle/scenario/native evidence missing, unsupported or stale; error. | unsupported/4; obtain qualified evidence or refuse this managed recipe. |
| `managed.rollback-refused` / `LEK-MGD-010` | Missing/forged snapshot, incompatible current Model, destination pin mismatch or nonmatching current generation; error. | invalid/1; restore required input custody or use a verified retained generation. |
| `managed.recovery-required` / `LEK-MGD-011` | Unresolved journal/publish/restore conflict; error. | invalid/1; reconcile the bounded transaction before any further mutation. |
| `managed.ownership-denied` / `LEK-MGD-012` | Attempt to overwrite/delete protected or unclaimed maintained files, cross-module scope, hidden hybrid or bookkeeping; error. | denied/3; author a permitted explicit ownership/extension plan; retain original structure/target denial. |

Registry successor must retain the complete integrated allocation union, register bounded data fields (module/target/semantic requirement/path/pin/reason), allowed statuses, message IDs and explicit remediation, and update Rust embedding/selection, validation-profile pins, schema generators, diagnostic fixtures and coverage catalog. Status determines exit; warnings cannot be promoted through an unsupported severity override. Do not alter frozen diagnostic item schema merely to add codes. Add one positive trigger and one clean/negative control for each new code through the live producer, not handcrafted diagnostics only.

## Proposed test plan mapping each acceptance criterion

The first column below copies the issue checklist **verbatim**, including Russian wording and backticks. All acceptance evidence is future implementation work, not satisfaction by this research document.

| Live issue acceptance criterion, verbatim | Increment / concrete completion evidence |
| --- | --- |
| - [ ] Одинаковый IR + lock даёт byte-stable output. | I1/I2/I6: same exact closure pins in two fresh project roots and repeated generation produce identical complete file-set bytes, canonical manifest and semantic source maps. Permute discovery order; vary OS/locale/timezone/environment and irrelevant maintained source. Independently compare SHA-256 and exact bytes; changed pinned recipe/input yields a reviewed change, never ambient variation. |
| - [ ] Generated files полностью перечислены в manifest. | I2/I3/I6: independently walk declared managed roots and compare a bijection against generated entries, roles/module/target owners, exact digests and all required source maps. Include implementation, tests, helpers/config/schema/map sidecars; injected undeclared output and dropped untouched-module entry must fail. Owner bookkeeping is separately classified, never omitted as an application artifact. |
| - [ ] Manual drift обнаруживается. | I3/I6: edit exact generated handler/test/map bytes without changing Model; `generate --check`, regeneration, promotion/upgrade and rollback preflight detect qualified drift. No writes or digest refresh; malformed manifest/self-digest and concurrent edits also refuse. Resolve restores only reviewed exact bytes, and denied restore leaves edited bytes/evidence intact. |
| - [ ] Custom implementations сохраняются через explicit bindings. | I1/I3/I4/I5: selected #30 custom/foreign binding survives promotion, repeated generation, cleanup, adapter upgrade and rollback with exact custom-source bytes intact. Binding existence/signature/effects and identical operation scenarios are checked. Missing/ambiguous selection and hidden maintained logic fail; no textual region parsing is accepted. |
| - [ ] Adapter capability gap блокирует managed migration. | I1/I3/I5: inject one absent/partial/unknown required transition, expression, policy, transaction, effect or assertion capability into an otherwise valid adapter. Managed plan/admission/apply blocks before output/lock/config changes and names semantic witness. Unsupported native/reference coverage and old-protocol adapter are never pass/hybrid. |
| - [ ] `generate --check` подтверждает clean state без writes. | I3/I6: read-only snapshot of full project including directories/cache/ownership/lock/history before and after clean, drifted, missing, malformed and denied checks; assert identical tree bytes/inventory and correct verdict/exit. Missing baseline/input/manifest cannot be called clean. No adapter apply, implicit output/report file, cache repair or directory creation. |
| - [ ] Reference planner fixture можно удалить и воспроизвести из Model. | I2/I5/I6: disposable synthetic planner project with a real supported generated handler/test recipe. Record exact IR+lock/attachment pins and independent behavior expectations; delete its generated application files/tests/maps/derived manifest in the sandbox, retain canonical Model, locked adapter and explicit custom bindings. Recreate output from those inputs, compare complete deterministic bytes/manifest/maps and execute the same planner scenarios. No copy from golden/source backup or hidden prior implementation read. |

### Proposed capability and constraint traceability

| Issue capability, verbatim | Required design / fixture family |
| --- | --- |
| generation plan/dry-run; | I2 saved plan and project-write-free preview; before/after inventory and stale plan probes. |
| deterministic files; | I2 pin closure/canonical renderer and cross-host exact-byte family. |
| full artifact manifest и source maps; | I2/I3 manifest bijection, ranges/owner/revision validation and retained multi-module maps. |
| safe regeneration; | I3 generated-drift preflight, ownership/confined staging and whole-transaction recovery. |
| orphan cleanup; | I4 exact scoped generated-only cleanup plan; preserved custom and unknown-file controls. |
| protected custom boundaries без текстовых magic regions; | I1/I3 explicit #30 selections and ownership; hostile magic-comment/source-region controls. |
| adapter upgrade diff; | I4 candidate lock + exact adapter/recipe pins, output/ownership/maps/coverage/behavior diff and unchanged dry-run state. |
| behavioral equivalence gates; | I5 independent normalized native/reference/previous behavior; unsupported never pass. |
| rollback к previous generated manifest; | I4 retained exact bytes/maps/manifest/config/lock and interrupted transaction controls. |
| mixed modes внутри одного project. | I1/I3 exact module assignments; observed/contracted siblings and their maintained handlers retain all bytes/ownership. |

| Issue constraint, verbatim | Required refusal / evidence |
| --- | --- |
| нельзя переводить module в managed, если adapter не покрывает required semantics; | Requirement ledger + executable/recipe-qualified full coverage before plan/apply; missing and unknown are blockers. |
| no hidden fallback to handwritten/generated hybrid; | Unsupported generation cannot emit stubs/scaffolds then declare managed success; selected explicit foreign/custom extension remains visible with preserved contract gates. |
| generation не должна читать arbitrary previous implementation как источник истины; | Renderer read scopes admit IR/pinned attachments only; hash-only drift/before-state and extension conformance reads are separate. Canary/reordered/deleted old handlers do not change generated bytes. |
| unsupported custom logic требует explicit foreign binding; | Binding selection/signature/effects/scenario checks, undeclared custom source refusal; no arbitrary code payload in plans. |
| manual edit generated artifact блокирует check/regeneration до resolve. | Blocking drift at check/apply and exact pinned explicit restore plan; no silent baseline rehash or force overwrite. |

## Proposed fixture and gate families

Create synthetic `tests/fixtures/managed/` with explicit provenance, linked producer recipes and immutable expected outputs. Proposed subfamilies:

- `wire/{valid,invalid}`: each new schema/discriminator/pin, duplicate keys/IDs, unknown nested members, oversized inputs, path/enum/digest errors, wrong self-digest and unsupported successors.
- `coverage/`: one full basic recipe, every required semantic gap, unknown/partial support, unsupported assertions, stale describe evidence, wrong adapter entry/recipe/profile pins, explicit extension replacement and missing applicable attachment.
- `planner/`: canonical project + transitions/scenarios/locked target recipe, independent expected generated implementation and tests, exact maps, complete manifest and normalized behavior. Seed from current orchestration/reference fixture owners only after rejoining all pins; no private consumer data.
- `mixed/`: observed, contracted and managed exact modules including prefix-colliding names, cross-module refs, shared outputs and two targets. Assert untouched bytes and complete retained entries/maps after scoped runs.
- `drift/` and `custom/`: generated byte edits/missing files, custom/scaffolded/checked/external preservation, hidden logic, forged bindings and restoration plans.
- `transaction/` and `rollback/`: stale before-state, post-plan edit, concurrent processes, failure after each publish step, interruption/restart, missing/corrupt snapshot, wrong Model/lock, partial manifest and restore failures. Preserve destination/source/custom bytes on refusal.
- `cleanup/` and `upgrade/`: old/new adapter pins, removed generated outputs, preview identity changes, custom/unclaimed neighbors, behavioral delta and exact predecessor restoration without old adapter execution.
- `hostile/` and `determinism/`: traversal, links/junctions, Windows case/device/alias collisions, oversized plans/files, undeclared writes/reads, path swaps, nondeterministic renderer, absolute-path/secret canaries, root/environment/iteration variations and missing required executable.

Proposed new gates: `scripts/test-managed-contracts.mjs` (closed schemas and Rust/runtime parity), `scripts/test-managed-cli.mjs` (real binary/adapter/filesystem behavior), `scripts/test-managed-planner-e2e.mjs` (actual Model -> regenerated implementation -> native/reference evidence), and `scripts/gen-managed-contracts.mjs --check` (producer parity without writes). The schema gate uses exact Ajv `8.17.1` Draft 2020-12 through existing `LEKALO_AJV_NODE_PATH`/`NODE_PATH`; generated expectations cannot replace independently authored artifact/behavior oracles. Missing schema, binary, adapter, native runner or required fixture fails its lane rather than skipping.

Register provenance in `tests/fixtures/fixture-provenance.json`, plus existing fixture/golden catalog, case DAG, checksums and diagnostic coverage owners. Add deliberate temporary missing-artifact probes to ensure guards actually fail. Gates run on disposable external copies and never regenerate committed fixtures in acceptance mode. Register new diagnostic positive/control producers with the union registry; retain predecessor family coverage and gate semantics.

Proposed focused implementation commands:

```text
cargo build --workspace --locked
cargo test -p lekalo-core --test managed --locked
cargo test -p lekalo-cli --test managed --locked
node scripts/gen-managed-contracts.mjs --check
node scripts/test-managed-contracts.mjs
node scripts/test-managed-cli.mjs
node scripts/test-managed-planner-e2e.mjs
```

Retain current artifact-manifest, implementation, contracted, target-protocol/security/boundary, orchestration, lock/structure/authority, scenario/reference-evaluation and target native scenario gates. Re-run `scripts/test-golden-planner-e2e.mjs` for the existing chain rather than replacing it with a managed-only chain. Validate product/contract versions against the actual implementation base, fixture provenance, golden catalog/normalization/determinism/diagnostic coverage, structure/authority/privacy and repository fmt/clippy/MSRV/workspace CI. Schemas can run in the Node contracts matrix; live gates belong after `cargo build --workspace --locked` in the same build-test job on Windows/Linux/macOS, with declared native toolchains. Schema-only success is not filesystem or behavioral acceptance, and local green gates are not hosted CI/release evidence.

## Risks and non-goals

- **Coverage completeness:** static adapter/profile claims can hide unsupported policies, effects, expressions or assertions. Bind support to executable/recipe/source evidence and all applicable semantic attachments; refuse incomplete required coverage.
- **Same-IR determinism:** unpinned attachments, templates, custom snippets, formatter/env state and timestamps break the issue's IR+lock invariant. Freeze the complete input closure and test across fresh roots and hosts.
- **Scope/manifest correctness:** current attribution-only `--module` and untouched-generated/map handling cannot be reused as proof of safe mixed modes. Add exact wire module scope and whole-manifest merge/currentness rules, including shared artifacts.
- **Drift versus stale inputs:** changed Model legitimately makes an old manifest stale, but must not permit manual generated edits to disappear under a new digest. Verify old-generation bytes separately before planning current inputs.
- **Recoverability:** several file renames and a manifest update are not one atomic filesystem operation. Journal every phase, revalidate before-state, serialize mutation, and exercise crashes/restores before claiming safe regeneration or rollback.
- **Cleanup ambiguity:** generated directory membership alone does not confer deletion authority. Protect custom/scaffolded/checked/external and bookkeeping; unknown files require explicit reviewed custody.
- **Oracle limitations:** finite planner scenarios and the reference evaluator's documented subset do not prove production equivalence. Unsupported required semantics remain blockers, with explicit recipe scope and independent native outcomes.
- **Shared M8 integration:** ownership lock/protocol/manifest/receipts and registry union are shared seams. Recheck actual implementing base and code collisions, preserve frozen producer contracts, and rerun complete integrated gates after merging.

Non-goals: a universal programming language; automatic conversion of arbitrary handwritten handlers; implicit hybrid fallback or source-to-Model inference; text magic regions; core target-language parsing; all-target generators; general reference-evaluator expansion; a complete #111 migration workflow; broad architecture/style policy; executing arbitrary setup scripts; paid/provider experiments; private consumer scans; cloud export; publication/push/tag/release. Planner acceptance must deliver one honest, capability-qualified implementation recipe with generated tests, while naming unsupported constructs explicitly.

## Research validation and delivery boundary

The authorized deliverable is this Markdown file in a local commit with subject `docs(m8): research issue-41`. Documentation validation checks issue-verbatim AC/capability/constraint coverage, source inventory paths/anchors, required house sections, proposed-code collisions, UTF-8/LF and whitespace, and the single-file staged boundary. Existing version/structure/authority/privacy checkers validate that this documentation-only change preserves repository contracts and custody. Proposed managed gates and runtime tests are not executed in this research dispatch; the issue's implementation acceptance remains future work.

Executed repository checks: `node scripts/check-contract-versions.mjs --base 15bd13917654dc20ad19b089a5bb8e5e29c2099f` passed (product `0.6.5`, 138 contract artifacts); `node scripts/check-structure.mjs`, `node scripts/check-authority.mjs`, `node scripts/check-privacy.mjs` and `node scripts/test-fixture-provenance.mjs` passed (86 synthetic fixture families). The document audit verifies seven verbatim AC rows, ten capability rows, five constraint rows, ten main sections, existing concrete inventory paths/anchors and twelve collision-free proposed codes against both inspected registries. Git staged whitespace/path checks and HEAD artifact verification complete the permitted local delivery; none of these results claims managed runtime acceptance.
