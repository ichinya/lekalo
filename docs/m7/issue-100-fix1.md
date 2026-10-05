# Issue #100: fixes after Codex review round 1

Status: **All three findings corrected and locally verified**, 2026-10-05.
Authority: the user's fix-round instruction and
[Codex review round 1](issue-100-review1-codex.md) at `c4ea9a5c` on
`ichinya/m7-issue-100`. Starting worktree/index were clean. This round changes
only Framework Lift admission/decoding, its existing live gate, and related
documentation. There is no push or external agent/provider execution.

F1 is committed as `22605b4a` (`fix(evaluation): reconcile token totals and enforce
component bounds`); F2 as `ef188c16` (`fix(evaluation): classify infrastructure
assertion outcomes`). F3 and this verification report accompany the integer
decoding fix commit.

## Finding-to-fix evidence

### F1 / P1: token totals and budget lower bounds

`crates/lekalo-core/src/framework_lift/mod.rs` now shares
`validate_token_totals` between arm admission and result-row validation. Under
the existing `framework-lift-metrics-1` normalizer, known input and output are
disjoint; when both and the total are known, total must equal their sum. A known
total must also cover available partial bounds. Inconsistency refuses with
`evaluation.metric-inconsistent`, detail `token-total`, before a success can be
derived. No invalid aggregate can bypass this check by importing a result row.

`token_lower_bound` computes
`max(known input, known cached input) + max(known output, known reasoning)`.
An unavailable leaf contributes no claimed measurement; a known subset still
bounds its unavailable parent. Cache/reasoning are never added again when the
parent is known. These bounds can prove the task's token cap was exceeded even
when total usage is unknown, unsupported or withheld. Acceptance gives that
violation task-failure precedence. It still requires a known, sourced total for
success, and does not rewrite unavailable metric states or invent a total.

Live gate counterexamples cover total zero, total 39 below input 40, total 51
inconsistent with 40+10, and input/output 10001 under the 10000 cap with stale
total 50. Each schema-shaped arm refuses through `validate`, `record-arm` and
`compare`. A partial breakdown with known total below known input also refuses.
An imported result with total zero refuses.

For each unavailable-total state, input above cap, output above cap and the
6000+6000 joint bound produce `task`, `verifiedSuccess: false`; a 40+10 bound
within cap remains `unsupported`. The original state is preserved in each row.
Exactly 10000 known total remains accepted; adding known reasoning 5 to the
existing input 40/output 10/cached input 20 still uses total 50 and succeeds.
Rust boundary tests also cover unavailable parents with known cache/reasoning
and the sum of two maximum admitted components without overflow.

### F2 / P2: infrastructure assertion classification

The existing `acceptance` precedence now recognizes an admitted assertion
`outcome: infrastructure` directly in the infrastructure class. Collectors do
not need to repeat it in the `failures` sidecar. The frozen order remains
custody-security, task, provider, infrastructure, unsupported, interruption.

The live no-sidecar vector admits through `record-arm` and compares as
`infrastructure`, with both success flags false and the pair kept incomplete.
A hard failure plus another infrastructure assertion remains `task` despite
judge 100. Combined provider/custody declarations retain their higher priority;
unsupported/interrupted declarations cannot hide witnessed infrastructure.
A Rust regression independently exercises these precedence transitions. The
existing negative/neutral golden is unchanged.

### F3 / P2: exact integral decimal/exponent decoding

`crates/lekalo-core/src/framework_lift/schema.rs` now normalizes numeric literals
from their decimal text before the existing unique-key serde decoder and closed
schema checks. The bounded scanner copies quoted strings verbatim. Its numeric
grammar preserves JSON syntax restrictions and determines integrality using
digits, exponent and trailing zeroes; it does not use floating-point rounding.
Integral spellings canonicalize to integer values before approval/reference
digests and semantic checks. Field-specific minima/maxima still apply.

All five families now admit raw `1.0`/`1e0` or `0.0`/`0e0`, uppercase signed
exponents and scaled integral mantissas. Exact strict Ajv 8.17.1 agrees on those
vectors. Production output is byte-identical to the unchanged integer-spelled
CLI receipt, including its existing stream line endings; canonical document
digests remain identical. A negative integral result ratio is also accepted.

The safe-integer maximum 9007199254740991 round-trips through decimal/exponent
spellings. Fractional, negative count, non-finite, oversized and malformed
literals still refuse in every family. Fractions that floating-point parsing
could round to an integer, such as `1.000000000000000000001`, also refuse.
Nonzero extreme exponents refuse without overflow; mathematically zero literals
remain zero. No fractional input is repaired or silently rounded.

Four Rust regressions exercise 76,803 signed integral-literal round trips, exact
safe-integer/zero boundaries, malformed/fractional/oversized refusals, and quoted
strings plus duplicate escaped keys/trailing-document/structure refusals.
The live gate passes raw bytes to the binary, rather than serializing away the
lexemes under test with `JSON.stringify`.

## Verification

Each finding's new live assertion was first run against the preceding binary
and failed at the reported defect: F1 accepted total zero, F2 produced
`unsupported` for infrastructure, F3 refused baseline approval revision `1.0`.
After each correction the CLI was rebuilt before the production gate. F1's gate
passed 94 checks and two focused Rust tests; F2's gate passed 102 checks and three
focused Rust tests. The final combined evidence follows.

Local environment: Windows, Node 24.13.0, Cargo/Rust 1.98.0, product 0.6.4,
exact Ajv 8.17.1 supplied outside the checkout.

| Command / check | Final result |
| --- | --- |
| `cargo build --locked -p lekalo-cli` | Passed on the corrected sources before the final binary-dependent gate. |
| `$env:NODE_PATH='C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'; node scripts/test-framework-lift-contracts.mjs` | Passed: **189 checks**, all five live families, all six existing refusal codes, `origin: recorded-simulation`, `externalAgentRuns: 0`. |
| `cargo test --locked -p lekalo-core --lib -j 1 --quiet` | **1058 passed, 0 failed, 2 ignored**; includes all seven added Framework Lift Rust regression tests. Used `CARGO_PROFILE_TEST_DEBUG=0` and process-scoped Git safe.directory for this exact worktree. |
| `cargo clippy --locked -p lekalo-cli --all-targets -j 1 -- -D warnings` | Passed. |
| `cargo fmt --all -- --check` | Passed. |
| `node --check scripts/test-framework-lift-contracts.mjs` | Passed. |
| `node scripts/check-contract-versions.mjs --base c4ea9a5c` | Passed: product 0.6.4, 121 artifacts. |
| `node scripts/test-fixture-provenance.mjs` | Passed: 82 families, all synthetic. |
| `node scripts/test-docs-ownership.mjs` and `--static` | Passed: 348 surfaces, 13 P0 owners. |
| `git diff --check` and staged path review | Passed; only the scoped implementation, live gate and documentation paths are committed. |

All original 49 gate checks remain; the extension adds 140 checks. There are no
edits to published schemas, registry, Cargo dependencies/lock, CI workflow,
fixture goldens/provenance or executor plumbing relative to the reviewed SHA.
Closed objects, duplicate-key refusal, numeric bounds, metric-source admission,
private-policy refusal, missing-slot denominators, retry/attrition accounting and
hard-fail/judge separation remain live gate obligations. Existing CI already
runs this gate for each family after building the binary with exact Ajv; no
command/family ownership or generated CLI index changes are needed.

Test mutations use test-owned temporary copies of recorded simulations; there
are no private prompts, consumer repositories, provider requests or actual A/B
runs. Actual external evaluation ACs, authenticated metric/oracle provenance,
qualified executor containment and future receiver/export integration remain
pending. This report records local corrective verification, not independent
re-review, hosted CI or empirical Framework Lift acceptance. Other worktrees
were not accessed or changed. The historical review-only scratch directory was
left untouched; the gate cleans up its own new temporary copies.
