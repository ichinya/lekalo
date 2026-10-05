# Issue #100: imported token subset consistency fix

Status: **Residual round-2 P2 corrected and locally verified**, 2026-10-05.
Authority: the user's fix-round-3 instruction and finding 1 in
[Codex review round 2](issue-100-review2-codex.md). Starting HEAD was
`443447c79e10a0540cd02ddcd8093357c546dd4b` on `ichinya/m7-issue-100`; the worktree
and index were clean. This report accompanies the corrective implementation
and regression tests in one conventional fix commit. There is no push.

## Correction

`crates/lekalo-core/src/framework_lift/mod.rs:154` now defines
`validate_token_consistency`, the single authority for token totals and known
token subset relations. Both arm admission (line 227) and imported result rows
(line 408) invoke it. The token subset checks were moved out of the arm-only
loop; its non-token subset checks remain in place.

Under the existing `framework-lift-metrics-1` normalizer, a known cached-input
count cannot exceed known input, and known reasoning cannot exceed known output.
Violations refuse with the existing `evaluation.metric-inconsistent`, detail
`subset-count`. The shared validator retains the total reconciliation and
partial lower-bound rules, including the existing `token-total` refusal and
its precedence when a total is also inconsistent. Cache and reasoning are
not added to their known parents a second time.

When either leaf of a subset/parent pair is unavailable, the validator does not
invent a measurement or a relation. An unavailable subset with a known parent
is valid; a known subset with an unavailable parent still supplies the existing
lower bound. Unknown, unsupported and withheld states remain explicit. Known
subset inversions are refused even when the total is unavailable. Success still
requires the existing known, sourced acceptance-budget metrics; unavailable
total usage cannot grant success.

## Reproduction and regression evidence

Before changing production code, the CLI was rebuilt from the starting sources.
The new live gate failed on result validation admitting the cached subset
inversion. The review document's self-contained reproduction was also rerun:
both imported vectors exited 0 with success and first-pass flags true, while
arm validation, `record-arm` and `compare` each refused with `subset-count`.

After the correction and rebuild, the same vectors are permanent live gate
obligations in `scripts/test-framework-lift-contracts.mjs:102`. Each mutates only
the successful B / `pair-two` row's metrics in a temporary copy of the unchanged
`framework-lift-result/golden/negative.json`, using corresponding B-neutral
arms with source coverage adjusted for changed metric states.

| Vector | Input | Output | Cached input | Reasoning | Total | Corrected disposition |
| --- | --- | --- | --- | --- | --- | --- |
| Cached inversion | known 40 | unknown | known 41 | unknown | known 50 | Result validation, arm validation, `record-arm` and `compare`: exit 1, `evaluation.metric-inconsistent` / `subset-count`. |
| Reasoning inversion | unknown | known 10 | known 20 | known 11 | known 50 | Same refusal on all four paths. |

Strict Ajv 8.17.1 accepts both vectors' schema shape. Runtime refusal enforces
the existing normalizer rather than changing or relaxing the published schemas.
The gate deliberately supplies true success/first-pass flags in the imported
rows, so those declarations cannot bypass the subset check.

The gate adds **64 checks** while retaining all 189 prior checks: eight refusal
checks for the two inversions; eight positive checks for equal subset/parent
counts; 24 positive checks for unavailable subsets of known parents; and 24
for known subsets of unavailable parents. Positive cases cover both token pairs
and all three unavailable states across the same four CLI paths. They compare
the admitted metrics exactly, including unavailable states, and retain valid
recorded success/first-pass outcomes under the existing known total.

Two new focused Rust regressions cover subset inversions with known, unknown,
unsupported and withheld totals, plus equal counts and unavailable parents or
subsets. The existing total-bound, infrastructure-precedence and exact-integer
regressions continue to pass. Valid golden comparison output is unchanged.

## Local verification

Windows, Node 24.13.0, Cargo 1.98.0, product 0.6.4; exact Ajv 8.17.1 is supplied
outside the checkout. Final evidence was obtained after the corrected build.

| Command / check | Result |
| --- | --- |
| `cargo build --locked -p lekalo-cli` | Passed on the final corrected sources before the live gate. |
| `$env:NODE_PATH='C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'; node scripts/test-framework-lift-contracts.mjs` | Passed: **253 checks**, all five live families, all six existing refusal codes; `origin: recorded-simulation`, `externalAgentRuns: 0`. |
| `$env:CARGO_PROFILE_TEST_DEBUG='0'; cargo test framework_lift --locked -p lekalo-core --lib -j 1` | **9 passed, 0 failed, 0 ignored**, 1053 filtered. |
| `cargo clippy --locked -p lekalo-cli --all-targets -j 1 -- -D warnings` | Passed. |
| `cargo fmt --all -- --check` | Passed. |
| `node --check scripts/test-framework-lift-contracts.mjs` | Passed. |
| `node scripts/check-contract-versions.mjs --base 443447c7` | Passed: product 0.6.4, 121 artifacts. |
| `node scripts/test-fixture-provenance.mjs` | Passed: 82 families, all synthetic. |
| `node scripts/test-docs-ownership.mjs --static` | Passed: 348 surfaces, 13 P0 owners. |
| `git diff --check`, staged path review | Passed; only the core validator/tests, live gate and this report are committed. |

No published schemas, registry entries, fixture goldens/provenance, Cargo
dependencies/lock, executor, CI workflow, commands or contract families changed.
No ownership regeneration is required. The existing CI gate remains after the
binary build with exact Ajv; it now includes these new checks. The registry's
506 entries and the unchanged 500-entry predecessor remain gate obligations.

The gate and reproduction used owned temporary simulations and cleaned them up.
No other worktrees or historical scratch were changed. The unfiltered workspace
suite, hosted CI and external agent campaigns were not run in this round. This
is corrective local verification, not independent re-review or empirical A/B
acceptance. Actual external-result ACs and receiver authentication/replay remain
pending; no private prompts/code were uploaded and no live campaign was fabricated.
