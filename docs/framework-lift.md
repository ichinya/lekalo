# Framework Lift recorded evaluation protocols

Status: **Implemented** offline protocol validation and recorded evidence comparison
for issue #100. Actual external agent/provider runs and independently authenticated
execution receipts remain an integration seam. Synthetic fixtures are recorded
simulations; they are never evidence of measured Framework Lift.

`lekalo evaluation` never launches an agent, shell, native gate, provider, judge or
network request. It reads explicit bounded documents and emits local-only canonical
JSON. The five closed `framework-lift-{baseline,task,campaign,arm,result}` families
are product `0.6.4`; each has synthetic goldens and a mandatory live CI gate.

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
provider, infrastructure, unsupported, interruption. Missing slots appear as
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
| `inputTokens`, `outputTokens`, `reasoningTokens`, `cachedInputTokens`, `totalTokens` | Provider-reported attempt totals under the pinned usage normalizer. Cached input is a subset of input and reasoning a subset of output for this recipe. A provider with different categories needs a faithful normalizer or unsupported fields; overlap is never added twice. Hidden reasoning stays unknown. |
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

Run `node scripts/test-framework-lift-contracts.mjs` with exact Ajv 8.17.1 on
`NODE_PATH` and a locked built CLI. CI runs this live gate after cargo build for
each family. It checks schema/Rust closure, independent arithmetic expectations,
baseline mutation, source provenance, stale receipts, arm contamination, provider
costs, lost slots and a hard failure with judge score 100. No external agent is
invoked. The implementation/remaining AC evidence map is in
[the issue report](m7/issue-100-implementation.md).
