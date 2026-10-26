# Framework Lift recorded evaluation protocols

Status: **Implemented** offline protocol validation and recorded evidence comparison
for issue #100, with a versioned post-run import for #167. Independently authenticated
provider execution remains an integration seam. Synthetic fixtures are recorded
simulations; they are never evidence of measured Framework Lift.

Post-MVP issue [#167](https://github.com/ichinya/lekalo/issues/167) now has real
public synthetic CLI runs, with an allowlisted
[read-once result](../scripts/research/results/2026-10-26-read-once.json) and
[receipt audit / admission notes](../scripts/research/README.md). These are
research disclosures, separate from local-private admitted records and product
privacy exports. The 1.1.0 successor now admits all eight archived attempts through
`record-arm/compare`, preserving unknown defaults/pricing, explicit uncapped
policy, timing recipe and post-run translation provenance. Results remain
`recorded-unverified`, with partial profile knowledge; admission does not
authenticate the underlying execution. Historical simulations and the MVP 1.0.0
acceptance limitation retain their original status.

`lekalo evaluation` never launches an agent, shell, native gate, provider, judge or
network request. It reads explicit bounded documents and emits local-only canonical
JSON. The five closed `framework-lift-{baseline,task,campaign,arm,result}` families
have frozen product `0.6.4` and explicit `1.1.0` contracts. CI checks both with
independent Ajv and the actual CLI. The examples below retain the 0.6.4 simulations.

```sh
lekalo --json evaluation validate --family task --input tests/fixtures/framework-lift-task/golden/priority.json
lekalo --json evaluation preflight --baseline tests/fixtures/framework-lift-baseline/golden/approved.json --task tests/fixtures/framework-lift-task/golden/priority.json --campaign tests/fixtures/framework-lift-campaign/golden/scheduled.json --workspace tests/fixtures/framework-lift-baseline/workspace
lekalo --json evaluation record-arm --baseline tests/fixtures/framework-lift-baseline/golden/approved.json --task tests/fixtures/framework-lift-task/golden/priority.json --campaign tests/fixtures/framework-lift-campaign/golden/scheduled.json --input tests/fixtures/framework-lift-arm/golden/b-hard-fail.json
lekalo --json evaluation compare --baseline tests/fixtures/framework-lift-baseline/golden/approved.json --task tests/fixtures/framework-lift-task/golden/priority.json --campaign tests/fixtures/framework-lift-campaign/golden/scheduled.json --arm tests/fixtures/framework-lift-arm/golden/b-hard-fail.json --consumer-alias consumer-greenfield-one
```

Success (exit 0) means the offline document operation succeeded; a recorded task
failure still admits and appears in the result. Invalid protocol/custody/policy
input exits 1 with registered bounded reason tokens. No failing rejected value is
echoed. `validate` checks the selected document only; `preflight` joins the approved
baseline/task/campaign and verifies listed baseline file bytes; `record-arm` admits a
candidate record against those pins; `compare` derives every scheduled slot and
retains all supplied attempts, including missing slots.

Integral decimal/exponent number spellings canonicalize to bounded integers
before approval and reference digests are checked. Fractional, non-finite and
oversized literals refuse without floating-point rounding; signed counts remain
subject to each field's existing bounds. Strings and duplicate-key checks are
preserved.

Lift and attrition bounds use exact signed numerator/denominator ratios in
percentage points. Wilson endpoints use conservative integer parts per million
(lower rounded down, upper rounded up), so receipt JSON has no floating-point
round-trip ambiguity. These are marginal intervals, not a paired lift interval.

Baseline and campaign approval is a digest over recursively key-sorted compact
UTF-8 JSON, one LF, omitting only `approval`. The declaration names role/revision;
it is not a signature or proof of approver authenticity. Drift refuses before
the executor callback. The runner does not rewrite a baseline or record. Keep
approved files at immutable content-addressed local storage or an exact Git
revision; protection of approval issuance and archived evidence remains with the
operator. Repeating the same inputs produces the same comparison bytes.
The operator freezes the full source/dependency/data snapshot; preflight checks
the listed file pins, not an exhaustive tree inventory or a Git revision lookup.

The campaign freezes at least two complete A/B pairs for one task and profile.
Each pair's task/fixture/dependencies/provider/model/executor/runtime/sampling/
price/budget/oracle tuple matches exactly. Arm A disables semantic exposure and
Lekalo calls; B enables them. Ordinary native tools and the independent native
oracle must be the same. The primary question is change workflow on identical
starting native bytes, which may already be generated. Profile references are
opaque versioned pins: there is no assumption about issue #84's contract names.
Private observed campaigns require local-only provider/executor policy and disabled
telemetry. Declared policy cannot attest an arbitrary executor's real containment.

The `scripts/lib/framework-lift-executor.mjs` callback seam preflights before
calling an explicitly supplied executor. Both arms receive the same profile and
request digest with a declared exposure policy. The embedding harness enforces
separate candidate copies; the supplied baseline workspace stays unchanged.
Changed protocol pins during a callback are refused. The embedding harness enforces
actual read/tool views and obtains usage/tool/intervention events, preserves exact
provider revision/configuration, freezes the native oracle and collects candidate
receipts. No default provider, cloud fallback, private upload or paid execution
is supplied. Returned documents can be consumed with `record-arm`; hosts must
independently verify source receipts before making execution claims.

Metrics use `known/unknown/withheld/unsupported`; every known leaf requires exactly
one versioned component/evidence source. Input/output/reasoning/cache overlap is
retained without summing categories into fictional usage. Money uses integer
millionths (`costMicros`) with one campaign currency, price snapshot and reported
or estimated basis. No current pricing is fetched. Known costs include all
attempts; missing cost makes totals/ratios unknown. Zero success has explicit
`no-success`, never a zero cost per success. Nonzero holdout regressions, missing
required assertions, stale candidate/oracle pins, resource-cap violations and
hard failures cannot be hidden by the separate optional judge score.

Result rows preserve failure precedence: custody-security, task regression,
provider, infrastructure, unsupported, interruption. Admitted infrastructure
assertion outcomes supply that class directly, without a duplicate failure
sidecar; hard regressions retain precedence. Missing slots appear as
`not-started`. Primary success counts use every scheduled slot; all attempts and
their costs remain visible. Results expose marginal Wilson 95% intervals, paired
discordant counts, attrition bounds and task/profile-scoped claims. Marginal
intervals are not a paired lift confidence interval. Real repeated runs and
broader task/model/repository sampling remain necessary for an empirical claim.

Results are `local-private` and `recorded-simulation` or `recorded-unverified`.
No imported record is automatically upgraded to independently verified execution.
Role aliases use opaque caller-supplied tokens and `consumer-repository`; no
repository names/URLs/path hashes are generated. There is no public export.
Privacy admission/public aggregation is #102's seam; existing #119 policy does
not authorize unknown evaluation artifact kinds. Current #121 history contracts
remain unchanged and ineligible for export; rich evaluation fields cannot be
inserted into their closed vocabulary. A future history/import adapter must
preserve states, refs, provider failures and source outcomes.

The frozen `framework-lift-metrics-1` recipe defines one attempt's measurement
window from executor handoff through the independent acceptance decision. Common
fixture provisioning is outside that window; B's context/impact/verification work
inside the window is charged to B. Each retry is a separate arm document and
retains its usage, cost and duration. The ordered slots are the preregistered
execution order; their seed/order is declared, not generated by the comparator.
Repetition numbers identify pairs and do not substitute for independent runs.

| Fields | Required collector meaning / evidence |
| --- | --- |
| `filesRead` | Distinct native source files whose contents reach the agent, including tool responses and automatic attachments. Merely naming a path is not a read. The external collector maintains the local deduplication ledger; source refs bind it. |
| `filesChanged`, `unrelatedFiles`, `unrelatedAddedLoc`, `unrelatedDeletedLoc` | Candidate diff against the pinned baseline. Unrelated classification uses the approved oracle's relevance policy, not a filename guess. Binary changes require explicit unsupported LOC states. |
| `toolCalls`, `replanCount`, `fixCycles`, `humanInterventions` | Counts of the matching ordered event kinds. A submission follows the candidate decision; a fix occurs after a failed submission. Human help prevents first-pass success. Agent-internal replans without exposed events are unknown. |
| `retryCount` | Additional provider requests within this attempt, from the provider request ledger; campaign-level retries are the separate `attempt` ordinals. Do not infer a known retry count from a missing request ledger. |
| `inputTokens`, `outputTokens`, `reasoningTokens`, `cachedInputTokens`, `totalTokens` | Provider-reported attempt totals under the pinned usage normalizer. Known total equals known input plus output; cached input is a subset of input and reasoning a subset of output, never added again. A known total cannot be below available component bounds. If a parent is unavailable, its known subset still supplies a lower bound. These disjoint bounds can prove a token-cap violation even without a total, but unavailable total states remain explicit and cannot grant success. A provider with different categories needs a faithful normalizer or unsupported fields. Hidden reasoning stays unknown. |
| `costMicros` | All attempt charges in integer millionths of the campaign currency, including failures and cache tiers, bound to the campaign tariff snapshot. Estimated and reported campaigns stay distinct. |
| `durationMs`, `humanDurationMs` | Wall-clock milliseconds through acceptance, with waiting/tool/verifier time included; separately recorded human assistance time. Comparison retains attempt durations and does not invent total time across overlapping runs. |
| `escapedRegressions` | Failed independent holdout assertions after submission; known zero requires complete frozen holdout execution against the candidate. |
| `capsuleBytes`, `capsuleEstimatedTokens`, `includedFacts`, `candidateFacts`, `requiredFacts`, `includedRequiredFacts` | Actual serialized capsule bytes, pinned estimator and independent fact/coverage ledger. Candidate facts are eligible supplied facts; required facts are frozen by the task oracle. Capsule coverage is reported as counts, not a claimed relevance score. Unsupported A capsule fields stay unsupported. |

The CLI deterministically checks joins, states, event counts, acceptance, scheduled
denominators, costs and uncertainty from supplied records. It does not observe
agent reads, billing, hidden reasoning, wall clocks, task relevance or capsule
facts. These measurements must be collected by the explicit executor and backed
by the versioned `measurementSources` references. Result rows retain arm digests;
importers resolve those arm documents to obtain the sources, assertions and events.

Run `node scripts/test-framework-lift-contracts.mjs` with exact Ajv 8.20.0 on
`NODE_PATH` and a locked built CLI. CI runs this live gate after cargo build for
each family. It checks schema/Rust closure, independent arithmetic expectations,
baseline mutation, source provenance, stale receipts, arm contamination, provider
costs, lost slots and a hard failure with judge score 100. No external agent is
invoked. The gate checks the supplied-record evaluator; it does not establish
real agent outcomes, provider billing or production qualification.

## Explicit 1.1.0 import and measurement recipe

All joined documents must use the same contract generation. There is no automatic
upgrade of 0.6.4 records. `scripts/gen-framework-lift-v11.mjs --check` verifies the
five committed successor schemas; `scripts/test-framework-lift-v11.mjs` checks
live admission, arithmetic, unavailable state closure and privacy boundaries.

Sampling numbers and pricing may retain `unknown/withheld/unsupported` states.
Each resource limit is either a bounded integer, `{ "state": "unbounded" }` or
`{ "state": "unknown" }`. An unknown limit prevents a successful acceptance;
an explicit unbounded limit imposes no cap. Known breaches still fail, including
token subset lower bounds. Unknown pricing cannot admit a known monetary charge
or become a zero total, even when no attempt started.

The distinct `public-synthetic-observed` pilot uses public synthetic data and
hosted agents. Private `brownfield-observed` still requires local-only execution
and disabled telemetry. Public telemetry may be unknown; that uncertainty remains
visible. Requested CLI/model aliases never claim a known server model revision.

`framework-lift-metrics-1` retains its original through-acceptance window.
`framework-lift-agent-window-1` measures executor start to close, excluding later
independent grading and maintenance. The result carries the exact profile,
timing recipe, pricing state, profile completeness and campaign provenance.
It never combines these windows into one timing claim.

Post-run provenance binds the original frozen campaign digest, archive manifest
digest and translator component. This is a later interpretation of immutable
receipts, not a newly preregistered experiment. Baseline revision may be unknown
when native files have content pins but no common Git revision. Dot-prefixed
ordinary files such as `.gitignore` can be pinned; traversal and unsafe aliases
remain refused. Exact `filesRead` stays unknown when only a subset was audited;
`sourceFilesReadLowerBound` and `sourceMarkerBytes` preserve that separate evidence.

The offline `scripts/research/framework-lift-import.mjs` verifies the original
manifest and raw usage before writing to a fresh directory outside the archive.
It checks record-arm/compare parity with the independently audited disclosure,
retains the incomplete Devin B1 and unknown costs, and makes no provider request.
Local-private result records remain ineligible for the existing product public
export until the #102 admission boundary explicitly supports this artifact.
