# Issue #100: Codex independent review round 3

**Verdict: ACCEPT — the round-2 residual P2 is resolved, with no new findings
in the reviewed token-consistency delta.** The required gate, workspace Rust
filter and separate adversarial probes all pass. Acceptance is for the
corrective deterministic scope, not for actual external A/B results.

Reviewed on 2026-10-05 in `ichinya/m7-issue-100`, worktree
`C:/Users/User/orca/workspaces/lekalo/m7-issue-100`. Fix:
`7ac4acc5ce907cb1b489c8a62ff9358c320d63a7`; initial HEAD / Devin round-3 report:
`b455df09752c00460b11d183b5859b958e2f84b3`. Starting worktree and index were clean.

Read [the round-2 finding](issue-100-review2-codex.md),
[the corrective report](issue-100-fix2.md), Devin's round-3 ACCEPT, the source
delta, published schemas, fixtures, live gate and CI wiring. Rebuilt the real
CLI and challenged the reported behavior with a separate driver and independent
arithmetic expectations. Devin's verdict is not the basis of this assessment.
Fetched [live issue #100](https://github.com/ichinya/lekalo/issues/100) using
`gh issue view 100 --repo ichinya/lekalo`; it remains OPEN with nine unchecked
ACs. This review covers the corrective token-admission delta and adjacent
semantics, not empirical acceptance of external A/B campaigns.

## Numbered verification dispositions

### 1. Round-2 P2 — imported subset inversions: resolved

`crates/lekalo-core/src/framework_lift/mod.rs:154` defines the shared
`validate_token_consistency`. Arm admission calls it at line 227; every imported
result row calls it at line 408. It retains total reconciliation first, then
rejects a known subset exceeding its known parent with
`evaluation.metric-inconsistent`, detail `subset-count`. There is no separate
arm-only token subset implementation left.

Independently mutated the successful B / `pair-two` row in the unchanged
`framework-lift-result/golden/negative.json`, retaining both true success flags:

| Vector | Input | Output | Cached input | Reasoning | Total |
| --- | --- | --- | --- | --- | --- |
| Original cached inversion | known 40 | unknown | known 41 | unknown | known 50 |
| Original reasoning inversion | unknown | known 10 | known 20 | known 11 | known 50 |

Strict Ajv 8.17.1 admits both shapes. **Both refuse, exit 1**, with
`evaluation.metric-inconsistent` / `subset-count` through result validation and
the equivalent arm's validation, `record-arm`, and `compare`. Source coverage
was adjusted for each arm's changed known/unavailable leaves, so a missing
source was not the reason for refusal.

The commands take the appropriate family: `validate --family result` consumes
the result; arm validation, `record-arm` and `compare` consume the corresponding
arm. A result document is not passed to an arm-only command as a substitute
for this check.

### 2. Unavailable totals and subset/parent states: preserved

For both inversions, replaced total with each of `unknown`, `unsupported`, and
`withheld`. All six variants refuse on the same four paths, with `subset-count`.
For standalone imports these probes modify the already failing B / `pair-one`
row, whose unavailable total is otherwise permitted. This isolates the subset
check from the separate rule prohibiting success with an unavailable total;
costs, summaries and flags remain unchanged. Zero known parents with subset 1
also refuse on all four paths.

For both token pairs, each unavailable subset state with a known parent remains
valid; each known subset with an unavailable parent remains valid when its
partial bound fits the known total. Equal subset/parent counts also remain
valid. Admitted arm/result metrics are compared exactly, including their
unavailable states. Valid comparator output is imported again and must remain
identical. No state is coerced to known zero or to a fictional reconstructed
total.

Malformed unavailable leaves retaining a `value` are refused by both Ajv and
the production validator: 12 arm/result closure counterexamples across the
three unavailable states and two token subset fields. Thus preserving absence
does not admit contradictory leaf encodings.

### 3. Partial totals, token caps and overlap: unchanged

The independent oracle uses exact `BigInt` arithmetic. It rejects impossible
known subset relations, checks exact input/output sums when both are available,
and otherwise uses each available parent or its available subset as a disjoint
lower bound. It does not import the production gate or executor's helpers.

| Probe under cap 10000 | Independent CLI observation |
| --- | --- |
| Input 6000, output 4000, cached 6000, reasoning 4000, total 10000 | Recorded success and first-pass success; subsets are not charged twice. |
| Same counts with either or both parents unavailable, total known 10000 | Valid recorded success, all original unavailable states retained. |
| Same counts with total unknown/unsupported/withheld | Valid admission; comparison reports `unsupported`, false success flags. Lower bound is 10000, not 20000. |
| Available lower bound 10001 with unavailable total | Valid admission; comparison reports `task`, false success flags. Tested known parents and unavailable parents bounded by known subsets. |
| Input unknown, output unsupported, cached 6000, reasoning 6000, total withheld | `task` from the disjoint 12000 lower bound; states remain unchanged. |
| Input/output at 9007199254740991 each, total unknown | `task`; exact bound 18014398509481982, no overflow or hidden excess. |
| All five token leaves unavailable | `unsupported`; absent measurements cannot grant success. |
| All token counts known zero | Valid zero-usage vector. Distinct from total zero with known positive components, which refuses. |

Original F1 input 10001 / total 50, total zero with positive components, a total
below an available parent/subset bound, and a total above the exact known sum
all refuse on arm and result paths with `token-total`. Maximum known subsets
under unavailable parents also cannot fit an undersized known total. On every
valid comparison the four scheduled rows remain present, B's denominator stays
two, and success counts agree with the observed outcome.

### 4. Independent corpus and cumulative guards

The separate driver exercised **57 directed and 80 seeded token-state cases**:
51 inconsistent cases refused on all four admission paths; 86 consistent cases
admitted with the expected comparator outcome and exact result re-import.
Including golden controls and closure counterexamples, it made **652 local CLI
invocations**. All cases were evaluated; no successful subset was selected
afterward.

The bounded corpus uses xorshift32 seed `0x10000003` and shifts 13, 17, 5;
token field order is input, output, cached input, reasoning, total. Choices are
known 0/1/5/10 and unknown/unsupported/withheld, selected modulo seven. All five
choices are drawn first. Every fourth case then draws input modulo 5000, sets
output to 10000 minus input and subsets to the floored half of each parent.
Total is known 10000 when the zero-based case index is divisible by eight,
otherwise unknown. These are synthetic arithmetic probes, not observations of
an agent or provider.

The required gate retains all 189 pre-fix checks and adds 64; no older check was
removed or loosened. The live gate and Rust filter continue to exercise the
earlier token-total, infrastructure-precedence and integer-decoding regressions.
All four original arm goldens were replayed together and matched the committed
negative/neutral result exactly as JSON. All five family controls validate;
their canonical hashes and the frozen fixture bytes remain unchanged.

## Fresh local checks

Windows, Node 24.13.0, Cargo 1.98.0, product 0.6.4, exact Ajv 8.17.1 supplied
outside the checkout. These are fresh review results, not counts copied from
the fix or Devin reports:

| Command / check | Result |
| --- | --- |
| `cargo build --locked -p lekalo-cli` | Passed before the binary-dependent gate and independent probes. |
| `$env:NODE_PATH='C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'; node scripts/test-framework-lift-contracts.mjs` | Passed: **253 checks**, all five live families, all six existing refusal codes, `origin: recorded-simulation`, `externalAgentRuns: 0`. |
| `$env:CARGO_PROFILE_TEST_DEBUG='0'; cargo test framework_lift --locked -j 1` | Passed across the workspace: **9 passed, 0 failed, 0 ignored**, 1053 filtered in the core library; other compiled test targets had zero matches. Exit 0. |
| `cargo clippy --locked -p lekalo-cli --all-targets -j 1 -- -D warnings` | Passed. |
| `cargo fmt --all -- --check`; `node --check scripts/test-framework-lift-contracts.mjs` | Passed. |
| `node scripts/check-contract-versions.mjs --base 443447c7` | Passed: product 0.6.4, 121 artifacts. |
| `node scripts/test-fixture-provenance.mjs` | Passed: 82 families, all synthetic. |
| `node scripts/test-docs-ownership.mjs --static` | Passed: 348 surfaces, 13 P0 owners. |
| `git diff --check` and staged path review | Passed; only this review document is staged for the authorized local commit. |
| Delta / CI inspection | Published schemas, registry, fixture goldens/provenance, dependencies/lock, executor, ownership inventories and CI unchanged across the fix. CI provisions exact Ajv, builds the CLI, then runs the five-family live gate in the same job. |

The unfiltered workspace suite, hosted CI, other operating systems and external
agent campaigns were not run here. Imported records still require a receiver
to resolve pinned arm evidence, authenticate external sources and replay
comparison before asserting real execution. Self-consistent local records are
not independent proof of a provider run.

## Live issue AC boundary

| AC | Evidence / remaining boundary |
| --- | --- |
| 1. Three greenfield planner tasks have A/B results | Actual external campaigns remain pending; simulation replay does not complete this AC. |
| 2. One observed brownfield context/impact A/B result | Pending authorized private execution and local collection. |
| 3. Success, tokens, files, iterations, cost per success | Existing recorded schema/gate surface retained. The reviewed token subset inconsistency is now refused in both arm and result admission. External measurement authenticity remains a collector responsibility. |
| 4. Negative/neutral results preserved honestly | Exact semantic replay of all four arm goldens against the negative/neutral result; provider retry, hard failure, costs, scheduled denominator and attrition remain visible. |
| 5. Exact model/harness/profile provenance | Existing pinned joins and source admission remain live gate obligations. No new assumption about #84 contract names or executor qualification. |
| 6. Hard regression cannot be hidden by subjective score | Existing gate retains hard-fail/judge-100 separation and infrastructure/provider/custody precedence; Rust regression remains required. |
| 7. Future AIFHub result import | Shared token arithmetic is enforced during result admission. Receiver integration, source authentication and pinned-arm replay remain future work. |
| 8. No universal superiority claim | Task/profile-scoped simulations, scheduled denominators and uncertainty remain; no empirical superiority inference. |
| 9. Anonymized public consumer role aliases | Probe uses an opaque role alias; existing result scope is local-private. Public aggregation/export remains a receiver boundary. |

All probe artifacts use recorded simulations, with **externalAgentRuns: 0**.
No private code/prompts were uploaded and no implementation or other worktree
was changed. New probe scratch was outside the checkout and removed after
verification; historical scratch was untouched. Only this review document is
committed; no push.
