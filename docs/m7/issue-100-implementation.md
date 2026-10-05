# Issue #100 implementation: offline Framework Lift evidence

Status: **Implemented** deterministic protocol/admission/replay scope; **Pending**
actual external agent A/B campaigns. This follows
[the committed research](issue-100-research.md) at `ea903b90` on
`ichinya/m7-issue-100`, product `0.6.4`. Acceptance authority is
[issue #100](https://github.com/ichinya/lekalo/issues/100), including its requirement
to report greenfield contracted and private brownfield observed separately.
No private consumer, external model/provider, paid agent run or public upload was
used to produce the delivered fixtures.

The new `framework_lift` core module owns closed document decoding, approval/pin
joins, arm treatment admission, failure classification, metric-source validation
and deterministic comparison. The CLI adapter supplies bounded read-only file
access through `evaluation validate`, `preflight`, `record-arm`, `compare`.
`scripts/lib/framework-lift-executor.mjs` is the neutral callback handoff: explicit
executor, same profile/request for A/B, intentional semantic/tool exposure delta,
baseline preflight before and after the callback. It supplies no cloud provider,
shell default, private egress bypass or claim of external containment.

Five closed Draft 2020-12 families at `v0.6.4` are added under `contracts/`:
`framework-lift-baseline`, `framework-lift-task`, `framework-lift-campaign`,
`framework-lift-arm`, `framework-lift-result`. Each has a dedicated synthetic
fixture family, provenance declaration, committed golden and live CLI/schema
validation in `scripts/test-framework-lift-contracts.mjs`. CI invokes the gate for
each family after `cargo build --workspace --locked`, with exact Ajv 8.17.1.
Generators are explicit authoring tools, never verification or CI repair steps.

The active registry seam retains product `0.6.4` and all 500 predecessor entries,
adding six `evaluation.*` rules (`LEK-EVAL-001..006`, total 506). Existing Model,
IR, target protocol, history, privacy and architecture-profile contracts are not
widened. Diagnostic suite coverage adds six live witness entries and refreshes
the registry digest. Command/schema ownership is registered in
`scripts/lib/docs-maintenance.mjs`; `docs/documentation-owners.json` and the CLI
index are regenerated through `scripts/update-docs-owners.mjs --write`.

## Acceptance evidence and remaining seams

| AC | Delivered evidence | Exact remaining seam / status |
| --- | --- | --- |
| AC1: three greenfield planner A/B tasks | Task protocol enumerates all seven change classes; campaign freezes paired slots, source/oracle/profile pins and budgets. Live gates reject mismatched arm/profile/exposure and preserve missing slots. | **Pending external evaluation.** Deliver three independent approved Laravel/Vue task baselines/oracles and repeated A/B arm records from one qualified executor. Priority recorded fixtures are simulated protocol vectors, not actual planner task outcomes. |
| AC2: one private observed brownfield context/impact task | Pilot enum distinguishes observed brownfield; preflight rejects private campaigns with non-local provider policy. Capsule/context counts, files/tool/events and metric-source refs are represented. | **Pending local external evaluation.** Bind an approved private Hono/Drizzle/MySQL task and native oracle, enforce A/B read/tool views, collect real inspect/impact/context and exploration evidence. Synthetic MySQL docs/pilot fixtures remain stand-ins; observed coupling is not assumed available. |
| AC3: success/tokens/files/iterations/cost per success | Arm schema has value-state token categories, cached input, files/churn, calls/retries/replans/fix cycles, interventions, latency/capsule/coverage and cost. Every known metric leaf requires a versioned component/evidence source. Result retains all attempts and computes cost per recorded hard-assertion-qualified success. Gate expects A cost 300 vs B cost 400 with equal success and tests unknown cost/no success. | **Implemented recording/derivation; pending telemetry authenticity.** External harness supplies complete actual usage/read/event/billing records. Known metrics are supplied evidence, not automatically measured by history/core. `costMicros` is integer millionths under a pinned currency/tariff/basis. |
| AC4: honest negative/neutral preservation | Recorded fixture has hard failure with judge 100, provider outage/retry and absent scheduled partner. Comparison emits five rows for four slots, neutral lift and higher B cost; all rows/costs survive. Paired attrition and marginal intervals are visible. | **Implemented deterministic guarantee.** Preregister real schedule before execution and provide every attempt; missing record stays `not-started`, never disappears from the scheduled denominator. The library cannot discover a deliberately withheld unsupplied attempt; external ledger custody remains necessary. |
| AC5: exact model/harness/profile provenance | Campaign binds provider/model/harness/executor/runtime id/revision/digest, sampling/seed/reasoning/output caps, network/telemetry, tariff, limits and recipe. Arms must match its digest; tests refuse altered pins. Architecture-profile ref is independently optional/unsupported. | **Implemented exact reference seam.** The external collector resolves truthful revision/configuration artifacts; digest/shape do not attest authenticity. #84 is parallel: no contract names, rules, severity or inheritance are assumed. New effective profile means new approved campaign. |
| AC6: subjective score cannot hide regression | Required new/regression/holdout IDs must pass for the candidate and oracle digest. Unsupported/missing/stale evidence cannot yield success. Failure priority preserves custody/task failures before provider/infra; missing resource-cap measurements cannot prove within-budget acceptance. Judge is independent. | **Implemented recorded-evidence decision.** Native/auth/concurrency oracle execution and authenticity remain external. `verify ready` or reference semantic pass alone is not ingested as task success. Source receipts must be independently verified before any execution claim. |
| AC7: future AIFHub importability | Closed versioned campaign/arm/result documents preserve dataset/task/baseline refs, component/model/provider/runtime/tariff provenance, units/states, trust disposition, schedules and assertion references. No arbitrary metrics bag or free-form prompt. | **Schema seam delivered; receiver integration pending.** AIFHub #28 has no assumed receiver contract here. Resolve campaign/arm refs with exact bytes and preserve `recorded-simulation/recorded-unverified`; add receiver conformance adapter when its contract is published. No remote import was attempted. |
| AC8: no universal superiority | Result fixes `claim: tested-task-profile-only`, pilot stays separate, scheduled counts and marginal Wilson 95% intervals/paired discordance/attrition bounds are explicit. Negative/neutral results use the same shape. | **Implemented scope/uncertainty representation.** Marginal Wilson intervals are not a paired lift CI or proof of generalization. Actual repetitions, task breadth and multiple model/repository strata remain required for empirical conclusions. |
| AC9: anonymized role aliases | Result contains opaque consumer alias and `consumer-repository`, constant `local-private` disposition. There is no raw prompt/source body, repository URL or generated path hash in result format. Private network policy is checked before callback; diagnostic failures do not echo rejected values. | **Local-safe seam implemented; public export pending #102/#119 admission.** Caller must choose opaque aliases and keep the real mapping local. Unknown evaluation artifact kinds do not gain export eligibility. Actual executor egress containment must be qualified externally, including telemetry/judge paths. |

## Custody and operational limits

Approval binds exact canonical content bytes (excluding the approval sidecar),
role and revision. A caller altering approved baseline or campaign bytes without
updating approval is refused. Current baseline files are digest-checked without
following symlink/reparse children; original fixture bytes remain unchanged in
the gate. Approval is declared custody, not a cryptographic signature. Immutable
archive/write protection, authorization of approval issuance, external event
capture and candidate/oracle execution isolation remain operator obligations.
Preflight checks the manifest's listed file pins, not an exhaustive workspace
inventory or the truth of the declared Git revision. The qualified executor must
resolve and freeze the complete source/dependency/data snapshot behind those
pins; extra unlisted files cannot be treated as approved application evidence.
There is no baseline rewrite, record mutation, retention bypass or automatic
public projection in these commands.

History #121 stays unchanged: its record/observation/assertion families do not
represent cached usage, rich provider profiles, task schedules or all new metrics.
The new families carry those fields independently. A future adapter can project
supported values to local history while binding returned record/assertion digests;
it must respect retention/dependent invalidation and cannot relabel history as
public evidence. No history ingestion is claimed by `record-arm`.

Input size is bounded at 4 MiB; manifests at 256 file pins, schedules at 256
slots, arm events at 4096 and supplied arms at 1024. A complete pair is required
for each preregistered repetition and at least two pairs for a campaign. Retry
attempts must be contiguous and follow provider failures; repair cycles belong
inside an arm's ordered event ledger. Unrelated LOC and task-context coverage
remain reported measurements requiring independent relevance/fact evidence, not
inferences from filenames. Missing measurement states never become zero.

The committed goldens are deliberate recorded simulations. Their assertion,
provider and usage refs refer to synthetic test evidence. Reports never mark
simulation or supplied external records as independently verified external runs.
This delivery establishes the consumer and refusal gates for future real runs;
it does not satisfy the actual A/B-result ACs with invented observations.

The measurement recipe and each metric's collector meaning are documented in
[`docs/framework-lift.md`](../framework-lift.md). Lift/attrition ratios retain exact
signed numerators/denominators; marginal Wilson bounds round outward to integer
parts per million. Imports must resolve campaign/task/baseline/arm references
with exact bytes and replay comparison; standalone result validation does not
authenticate the referenced external evidence. Optional judge scores remain
supplied annotations, not an independently executed rubric or acceptance oracle.

## Validation

Local Windows checks use Node 24.13.0, Rust/Cargo 1.98.0, locked product 0.6.4
and exact Ajv 8.17.1.
The approved baseline workspace was preserved; all disposable candidate changes
occurred in test-owned temporary copies. The fixture generator was an explicit
authoring step, then gates consumed committed-candidate bytes without repairs.

| Command / check | Local evidence |
| --- | --- |
| `cargo build --workspace --locked -j 1` | Passed; the gates require this real CLI and refuse a missing binary. |
| `cargo clippy --workspace --all-targets --locked -j 1 -- -D warnings` | Passed. |
| `cargo fmt --all -- --check` | Passed. |
| `node scripts/test-framework-lift-contracts.mjs --family <baseline/task/campaign/arm/result>` | All five invocations passed, 49 live receipt checks each. Exact Ajv, all five families/goldens, predecessor 500-entry canonical digest, six diagnostic refusals, forged aggregate/trust/pending-row denial, negative/neutral preservation, mixed-case native file pins, case-alias/traversal/reserved-name refusal, private zero-callback refusal, neutral profile/request pins and during-callback protocol drift are covered. External agent runs: **0**. |
| `node scripts/check-contract-versions.mjs --base ea903b90` | Passed at product 0.6.4; 121 contract artifacts. No existing family changed except the additive active registry seam. |
| `node scripts/test-diagnostic-contracts.mjs`, `node scripts/test-validation-contracts.mjs` | Passed with exact Ajv 8.17.1; registry 506, existing default/strict profiles unchanged. |
| `node scripts/update-docs-owners.mjs --write`, `node scripts/test-docs-ownership.mjs` (live and `--static`) | Passed; 159 commands, 129 contracts, 56 protocols, four globals, 348 owned surfaces and 13 P0 page owners. CLI index generated from the built help tree. |
| `node scripts/test-fixture-provenance.mjs`, `node scripts/test-docs-examples.mjs --static` | Passed; all 82 families synthetic, 12 documentation examples. |
| `node scripts/test-golden-catalog.mjs`, `node scripts/test-golden-hygiene.mjs`, `node scripts/test-golden-diagnostic-coverage.mjs` | Passed; 21 catalog cases, all 506 active diagnostics indexed, all 20 suite pairs executed. |
| `node scripts/run-golden.mjs --verify` | Passed; 21 existing cases, four raw-byte golden comparisons unchanged. |
| Structure/authority/privacy regression gates | `check-structure`, `test-structure-contracts`, `check-authority`, `test-authority-contracts`, `check-privacy`, `test-privacy-contracts` and `test-privacy-runtime-cli` passed. Live privacy CLI: 13 cases. |
| `cargo test --workspace --locked --no-fail-fast -j 1` with `CARGO_PROFILE_TEST_DEBUG=0`, local subprocess access and process-scoped `safe.directory` for this worktree | **Not fully green:** 1,941 passed, three failed, two ignored across 99 target summaries; exit 101. All three failing cases reproduce on the pre-implementation research SHA `ea903b90` with the same digest differences (below). Debug-symbol reduction changes compilation resource use, not tests/assertions. |

The default parallel workspace test attempt failed during Rust compilation with a
memory-allocation error. A first serial repeat was interrupted for the native
filename correction; neither attempt is claimed as a completed behavioral run.
Node subprocess gates initially encountered sandbox spawn refusals and were
rerun with authorized local subprocess access. The final full Cargo run also
used scoped Git trust so commit pins remain available. Sandbox-only adapter,
symlink and Git failures disappear in that qualified local run.

The three inherited failures are:

| Existing case | Confirmed on both this implementation and `ea903b90` |
| --- | --- |
| `ci_report::the_binary_reproduces_every_committed_json_golden` | `valid.verify.golden.json` expects lock digest `49d58e9b...`; live receipt binds `615d19e4...`. |
| `generate_orchestrate::the_full_sequence_matches_the_golden_receipts` | Dry-run golden expects lock digest `39e65245...`; live receipt binds `144b402d...`. |
| `provider::drift_reported_receipt_matches_the_published_golden` | Golden expects lock/manifest `49d58e9b...` / `4171f1b0...`; live receipt binds `615d19e4...` / `801fe29f...`. |

Baseline evidence used `git archive ea903b90` into an ignored directory under
this workspace's `target/`, with an independent `--target-dir`; no second Git
worktree was created or changed. Focused baseline commands were
`cargo test --manifest-path <archive>/Cargo.toml --target-dir <baseline-build>
--locked -j 1 -p lekalo-cli --test provider
drift_reported_receipt_matches_the_published_golden -- --exact` and
`cargo test --manifest-path <archive>/Cargo.toml --target-dir <baseline-build>
--locked -j 1 -p lekalo-cli --test ci_report --test generate_orchestrate
--no-fail-fast golden`. All three reproduced before #100; their goldens and
assertions remain unchanged. A follow-up integration repair must investigate
their generation pins rather than relaxing these comparisons.

Hosted CI, Linux/macOS execution, native planner/MySQL oracle campaigns, receiver
integration, public export and actual external A/B outcomes remain unclaimed.
No push is part of this delivery.
