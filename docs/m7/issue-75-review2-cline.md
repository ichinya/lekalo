# Issue #75 independent review, round 2 (agent: Cline)

Reviewer: Cline (independent; not the implementer, and not the author of
round 1). Reviewed the pushed branch `ichinya/m7-issue-75` at `eebe9bd0`
against base `origin/ichinya/M7`. The branch tip predates the #37 merge, so
per the task scope I reviewed the merge-base diff and did not treat the #37
files' absence as a finding.

Authority: [issue-75-research.md](issue-75-research.md) (`feba13dc`,
acceptance authority), [issue-75-implementation.md](issue-75-implementation.md).
Round-1 inputs re-read in full:
[issue-75-review-devin.md](issue-75-review-devin.md) (2 blockers + 8 majors
+ 14 minors), [issue-75-review-codex.md](issue-75-review-codex.md) (1 blocker
+ 17 majors + 2 minors), [issue-75-review-cline.md](issue-75-review-cline.md)
(1 blocker + 6 majors + 4 minors). Dispositions: [issue-75-fix1.md](issue-75-fix1.md).

Method: I re-derived every finding independently from the source and from
live runs rather than adopting the fix report's evidence. I touched only
this document; no implementation, test, fixture, or gate file was modified.

## Verdict: ISSUES

**1 blocker, 1 major, 3 minor.**

The fix round is substantively good and I could not refute a single one of
the round-1 behavioral findings: every blocker and every major that had a
concrete behavioral claim is now genuinely fixed, and I confirmed each by
re-running the gate, the tests, or a live probe rather than by reading the
fix report. No gate was weakened — no assertion removed, no schema relaxed,
no threshold loosened, no test skipped or `#[ignore]`d. `git status` is clean
and the test-writer residue is genuinely gone.

The blocker and the major are both **new**, and both are delivery
plumbing rather than metric arithmetic: the new CI wiring makes the
`contracts` job fail on every push, and the new `provenance` pins are
computed but never reach the output. Neither is a wrong number; both are
"the fix was declared green on evidence that could not have shown the
defect."

## Findings

### Blocker

**R2-B1. major → blocker: the newly wired CI gate makes the `contracts` job
fail on every push — the job that runs it never builds the binary the gate
requires.**

Evidence: `.github/workflows/ci.yml:51` (the new step, inside the `contracts`
job), `scripts/test-context-budget-contracts.mjs:127-131`.

Fix round 1 added exactly one line to CI:

```
+          NODE_PATH="$LEKALO_AJV_NODE_PATH" node scripts/test-context-budget-contracts.mjs
```

The gate hard-fails when the binary is absent — this is the intended
"requiring the binary" strengthening, and it is the right design:

```js
if (!existsSync(binary) && !existsSync(binaryPosix)) {
  fail("binary-missing", "build target/debug/lekalo before this gate; the live checks are mandatory");
}
```

But it was added to the `contracts` job, and that job never builds a Rust
binary. Its steps are: checkout → setup-node → rust-toolchain (no build) →
provision Ajv → the Ajv gate list → `cargo test -p lekalo-core
target_protocol_conformance --locked` (`:82`) → contract checkers. The only
`cargo build --workspace` in the whole workflow is `ci.yml:181`, in the
separate `build-test` job, which has no `needs:` relationship to `contracts`
and runs in parallel on its own runner. `cargo test -p lekalo-core` does not
produce `target/debug/lekalo` (that is the `lekalo-cli` binary), and it runs
*after* line 51 anyway.

I reproduced the failure exactly rather than reasoning about it. Renaming the
built binary away and running the gate verbatim:

```
$ mv target/debug/lekalo.exe target/debug/lekalo.exe.bak
$ NODE_PATH=<ajv 8.17.1> node scripts/test-context-budget-contracts.mjs
EXIT=1
{
  "ok": false,
  "reason": "binary-missing",
  "detail": "build target/debug/lekalo before this gate; the live checks are mandatory"
}
$ mv target/debug/lekalo.exe.bak target/debug/lekalo.exe   # restored
```

With the binary present the same gate is green:

```
$ NODE_PATH=<ajv 8.17.1> node scripts/test-context-budget-contracts.mjs
{"ok":true,"ajv":"8.17.1","registryEntries":457,"contextRules":8,
 "liveChecked":true,"golden":"tests/fixtures/context-budget/golden/planner.over-budget.json"}
EXIT=0
```

So the gate's *logic* is correct and the failure is purely one of job
placement. This is the same class of error as round 1's blocker: the
verification command that was actually run locally (with a pre-built binary
in the worktree) is not the command CI will run (on a clean runner). Every
`contracts`-matrix leg — Node 18 and Node 24 — will fail.

Contrast with how the repo already handles this: the other binary-requiring
gates (`scripts/test-run-history-cli.mjs`, `scripts/test-classification-cli.mjs`)
are all invoked from `build-test`, after `cargo build --workspace --locked`
at `ci.yml:181`. The new gate belongs in that list, not in the Ajv matrix.

Fix: move the `test-context-budget-contracts.mjs` invocation out of the
`contracts` job's Ajv list and into `build-test` after the `cargo build`
step. Alternatively add a `cargo build -p lekalo-cli --locked` step to
`contracts` before line 51 — but that duplicates a long build across two
jobs and is the worse of the two options. Note also that the file's own
header comment (lines 10-12) still says the live probe "is skipped with an
explicit flag when the binary is absent", which contradicts the hard-fail
the code now implements; the comment should be corrected either way.

This is a local reproduction of the CI job's step sequence, not a claim
about a hosted CI run.
### Major

**R2-M1. major: `provenance.policy` and `provenance.baseline` are computed
but never reach the output — the report is serialized before the pins are
applied.**

Evidence: `crates/lekalo-cli/src/main.rs:5341-5344` vs `:5440`.

`with_pins` exists and is correct
(`crates/lekalo-core/src/context_budget/mod.rs:175-183`), and the CLI
computes both pins faithfully (`:5427-5439`). But the envelope's payload is
serialized *first*:

```rust
// :5341
let canonical = match report.to_canonical_json() { ... };   // snapshot, no pins
let build_envelope = |comparison_json: &Option<String>| { ... canonical ... };
...
// :5440  --  400 lines later, on a *new* binding
let report = report.with_pins(policy_digest_pin.clone(), baseline_pin);
```

`build_envelope` closes over the `canonical` **string** captured at 5341, so
`with_pins` mutates a `report` binding that is never re-serialized. The pins
are dead code on the output path.

Live proof — a run with a correctly-pinned mandatory policy *and* a
consumed baseline (both pins should be `known`):

```
$ lekalo --json context-budget --symbol planner.focus_task --budget 200 \
    --policy <pin-matching mandatory policy> --baseline <baseline>
exit=0, status valid, comparison block present
provenance {"baseline":"unknown",
            "policy":{"state":"unknown"},
            "modelVersion":"0.2.16", ...}
```

Both pins read `unknown` on a run where the policy was applied and the
baseline was consumed. The comparison block that *is* emitted proves the
baseline really was read, so this is not a "the input was absent" case.

Why this matters beyond cosmetics: the entire stated purpose of the M7 /
codex-16 fix is that "reports bind to what they measured" and "a
mandatory-policy pass is distinguishable from an advisory run" (fix1.md M7
row, and `contracts/context-budget-report.schema.v0.6.3.json`'s own
description: "the policy pin carries the mandatory-policy digest when one
was selected"). A consumer reading `policy: unknown` cannot distinguish a
mandatory pass from an advisory one — the exact discrimination the field was
added for. The schema requires the block, and the block is present and
well-formed; it is just always unpopulated.

Fix: re-serialize after `with_pins` — e.g. move the `to_canonical_json()`
call and the `build_envelope` closure construction to *after* line 5440, or
have `build_envelope` take `&BudgetReport` and serialize on each call. Then
add a test asserting `provenance.policy.state == "known"` on a mandatory
### Minor

**R2-m1. minor: three scratch codemod scripts were committed under
`scripts/`.**

`scripts/.fix-absence.mjs`, `scripts/.fix-envelope.mjs`,
`scripts/.fix-envelope2.mjs` (added in `88198b4a`). They are one-shot
`readFileSync` → string-`replace` → `writeFileSync` patchers for the
comparison schema and the CLI envelope. Nothing references them: I grepped
`scripts/*.mjs` and `.github/workflows/*.yml` for `fix-absence`,
`fix-envelope`, and `cb-gate-probe` and got zero hits.

They are also not safe to re-run. `.fix-absence.mjs` exits 1 with
`console.error("r1")` if its target text is absent — which it now is, since
the patch has already been applied. They are development scaffolding that
should not ship. Not a correctness risk (nothing invokes them), but they are
unreviewed artifacts in the tracked tree, and the task asked me to confirm
none slipped in.

**R2-m2. minor: `cb-gate-probe/baseline.json` is a committed probe
artifact at the repository root.**

Added in `88198b4a`. It is a 5,291-byte serialized report used to exercise
`--baseline` by hand during development. It is not a golden (it differs from
`tests/fixtures/context-budget/golden/planner.over-budget.json`), not a
fixture (it is outside `tests/fixtures/`, so
`scripts/test-fixture-provenance.mjs` never sees it), and nothing reads it. A
probe scratch file belongs in a temp directory. The existing `.gitignore`
(`/target/`, `/.repowise/`, `/.m3/`, `/.m4/`, `/.pi/`, `node_modules/`) has
no pattern that would have caught it.

**R2-m3. minor: the shared-cost attribution lands on an arbitrary row, so
the "explainable" breakdown is arithmetically honest but semantically
opaque.**

`crates/lekalo-core/src/context_budget/mod.rs:360-380`. Reconciliation is
real — I verified the golden: `ОЈ(exclusive) + ОЈ(shared) = 355 = required`,
which is the identity round 1 asked for and which now holds. But shared cost
is assigned to "the canonically first dependency"
(`closure.transitive.iter().next()`), which for the planner fixture is
`effect:planner.create_task`, purely because `"effect:..."` sorts first in
the `BTreeSet`. The result:

```
effect:planner.create_task   excl 28  shared 134
entity:planner.task          excl 53  shared 0
```

134 tokens land on one row with no semantic relationship to that row's 28
tokens of exclusive content. This satisfies the additive identity the reviews
demanded, so I score it minor rather than major — but `sharedRequiredTokens`
on `effect:planner.create_task` does not explain anything to a human reader,
and the schema describes the field as shared consumers. Round 1's codex #3
sub-point about representing shared consumers and witness paths is addressed
in form (a shared bucket exists and reconciles) but not in substance. A
defensible alternative: bill shared cost to the subject itself as a distinct
pseudo-row, or name the bucket for what it is (subject-owned facts) rather
than attributing it to an unrelated dependency.

## Findings verification — every round-1 blocker and major

I checked each of these against the source and, where cheap, against a live
run. All **fixed**.

| Round-1 finding(s) | Status | How I verified |
| --- | --- | --- |
| **B1** devin / **1** codex / **1** cline — CI clippy gate fails on `clippy::ok_expect` | **fixed** | Ran the exact CI command `cargo clippy --workspace --all-targets --locked -- -D warnings`: `EXIT=0`. Because the first run was a cached no-op, I forced a genuine re-check by touching `compare.rs` and re-running `cargo clippy -p lekalo-core --all-targets --locked -- -D warnings` — `Finished ... in 59.73s`, clean. `.ok().expect(...)` is gone (e.g. `closure.rs:257` now calls `.expect("fixture loads")` directly). |
| **B2** devin / **2** codex / **2** cline — closure drops direct-only deps | **fixed** | `closure.rs:97-104` seeds `transitive` with root-edge targets inside the same admission branch that fills `direct`, and `:229-233` computes `indirect_only` as the set difference. Golden now reports direct 3 / transitive 9 / indirect 6, and I checked the arithmetic: `3 + 6 == 9`. `dependency_counts_on_the_fixture_graph` calls the real walk (not a hand-built struct) and asserts direct ⊆ transitive, the set-difference identity, and that both former direct-only targets appear. |
| **M1** devin / **3** codex / **3** cline — breakdown over-bills whole modules, `shared` always 0 | **fixed** | Per-dependency attribution at `mod.rs:328-388`: each required fact bills its owning node once, effect-edge facts bill the declaring operation. Golden: `Σ(exclusive)+Σ(shared) = 355 = required` (was 2,198 across 7 rows). The two direct-only deps now appear as rows. |
| **M2** devin / **10** codex — `duplicateSupportingTokens` structurally 0 | **fixed** | `mod.rs:590-593` computes it from a real bounded `supporting_requests` walk, and `metrics::duplicates_are_requested_minus_unique` asserts a non-zero case. |
| **M3** devin / **7** codex — minimum-safe uses floors | **fixed** | `estimate.rs:50-53` computes `ceil(a*num/den)` via `(a + den - 1)/den` in `i128` with checked div and `u64::try_from`. Both named vectors pinned: `(101, 11/10) = 112` and `(355, 2/3, +100) = 337`. |
| **M3b** codex — the gate does not apply the profile cost | **fixed** | `mod.rs:306-315`: `effective_required = minimum_safe(&profile, required)` feeds both `assess` and `over_by`, so framing cannot slip past the gate. |
| **M4** devin / **13** codex — `--baseline` not schema-validated | **fixed** | `baseline.rs` is a strict typed decoder: `deny_unknown_fields` at every level, exact four-state shapes with known-requires-value, identity/metric-version/estimator pins, ledger and summary arithmetic. Live: `{ broken` → exit 1 `baseline-malformed`; a `{"state":"unknown","value":3}` metric → exit 1 `baseline-state-with-value`. |
| **M5** devin / **14** codex / **5** cline — comparison computed then discarded | **fixed** | `main.rs:5345-5356` attaches `contextBudgetComparison` to the envelope; `contracts/context-budget-comparison.schema.v0.6.3.json` is a new published closed contract (signed absolute + rational relative deltas, explicit incomparable rows). Live description-only probe on a temp fixture copy: base 355 → candidate 364, with `minimumRequiredSemanticTokens` delta `+9` emitted and `comparable: true`. (The fix report says +22; that number is just a different description length — the mechanism is what matters, and it is visible.) |
| **M6** devin / **6** cline — Node gate not in CI | **fixed in form, new blocker** | The gate is now wired at `ci.yml:51` and is wired *requiring the binary* as intended. But it is wired into a job that never builds the binary — see **R2-B1**. |
| **M7** devin / **16** codex / **6** cline — no provenance pins | **partial → new major** | The `Provenance` struct, the schema block, and the pin computation all landed. But the pins never reach the output — see **R2-M1**. |
| **M8** devin / **4** cline — no paired integration fixture (AC6) | **fixed** | `tests/fixtures/context-budget/integration/` ships 5 real modules (auth, delivery, events, integration, tasks). Live run of `integration.sync_external_objects`: required 418, requiredModules 5, maxCrossModuleHops 2, direct 5 / transitive 9 / indirect 4, edgeOccurrences 14 — strictly wider than the planner reference (355 / 1 / 0) under an identical profile digest. Real comparison tests on **both** sides: `tests.rs:333` (core) and `crates/lekalo-cli/tests/context_budget.rs:663` (CLI), each asserting cost, module count, hops, and identical pins. |
| **M9** devin / **11** codex — `generatedMaintainedRatio` fabricated | **honest-unknown, correctly retained** | `mod.rs:594` emits `StateValue::Unknown`. No ownership-evidence adapter exists, and inventing a number would be the fabricated-value shape the research forbids. Accepted. |
| **codex 4** — applicability expands peer subjects into F | **fixed** | `facts.rs:322-353, 374-405`: a `no_expand` set suppresses forward expansion of policy/scenario nodes discovered as incoming applicability, so their other covered subjects never enter F. |
| **codex 5/6** — profile limits never bound the walks; unbounded caller profile strings | **fixed** | `ClosureLimits::effective` (`closure.rs:71-79`) caps profile values against the contract maxima, and `mod.rs:287-290` threads the same owner-capped `limits` through `collect_facts`, `dependency_closure`, and `module_hop_attribution`. Profile id/version grammar and the pinned selection version are validated (`profile.rs:218-229`) and selection is bound into the digest. Caller file reads are bounded by `MAX_INPUT_BYTES` via `read_bounded` (`main.rs:5498-5517`), with a dedicated invalid class. |
| **codex 8** — unrepresentable-error gap is a false positive | **fixed** | `facts.rs:520-540` consults the embedded error registry and only fires the gap when a binding actually matches a counted operation, and it then reduces `complete`. Live planner report: `gaps: ["detected-effects-absent"]` only — no false positive. |
| **codex 9** — declared effects counted as operations | **fixed** | `facts.rs:416-445` counts distinct effect *facts* and only for operations with at least one admitted edge. Golden: `declaredEffects: 1` (was an operation count). |
| **codex 12** — simulation not a real deterministic selection | **fixed** | Deterministic greedy selection with derived missing ids; the CLI test asserts a non-empty `missingRequiredIds` with `legacyFits: false`. |
| **codex 15** — mandatory regression gate silently skipped without a baseline | **fixed** | `policy.rs` returns `Some("baseline-required")` when the arm is selected and no baseline was consumed. Live: exit 3, `denied`, detail `baseline-required`. |
| **codex 11** — largest semantic fact mislabeled as an artifact | **fixed** | `mod.rs:587-588`: `largestRequiredArtifact` is honestly `Unknown` (no evidence adapter), and `largestRequiredSemanticFact` is its own wire field. Golden: artifact `unknown`, fact `entity:planner.task / 53 tokens`. |
| **cline 7** — `detectedEffects` fabricated `0` | **fixed** | `mod.rs:572-581`: `Unknown` when no detection evidence was contributed, `Known(len)` when it was — never an optimistic zero. Golden: `{"state":"unknown"}`. |
| **cline 8/10** — test-writer residue in the fixture tree | **fixed** | `crates/lekalo-cli/tests/context_budget.rs:427-455` copies the fixture into a private temp dir via `fixture_copy`; every profile/policy/baseline writer targets the copy. `git status --porcelain` is empty after my full test run. |
| **devin 5/12, codex 19** — bounded reads, reason/class coupling | **fixed** | See the `read_bounded` row above. Coupling negatives are asserted in the Node gate (`:241-248`). |
| **codex 18 / cline 6** — a separate `test-context-budget-cli.mjs` | **rebutted (form), substance accepted** | The fix report's reasoning holds: the CLI acceptance vectors now live in `crates/lekalo-cli/tests/context_budget.rs`, which CI's cargo job runs. That is the stronger gate — real process exits and envelopes, not a Node re-implementation. I ran it: `13 passed; 0 failed; 0 ignored`. |

### Regression and gate-weakening sweep — clean

- **No skipped tests.** Round 1's codex report noted 5 skipped CLI
  profile/policy/baseline tests. There are now zero: `cargo test -p
  lekalo-cli --test context_budget --locked` → `13 passed; 0 failed; 0
  ignored`, and the file contains no `#[ignore]` and no early `return`.
- **No assertion removed.** The Node gate's live block still checks the
  assessment, the over-by arithmetic, the ledger reconciliation, the
  `context.budget-exceeded` warning, completeness, the union-vs-per-subject
  reconciliation, three schema negatives, byte-level determinism, golden byte
  equality, and a host-path/timestamp privacy sweep — plus the new comparison
  and consumer-import blocks. It got stricter, not looser.
- **No schema relaxed.** The new comparison schema is closed
  (`additionalProperties: false` throughout) with `oneOf` branches that keep
  the reason/deltas coupling the round-1 negatives were about. The profile and
  policy schemas are new closed contracts with their own negatives.
- **No threshold loosened.** The new profile/policy inputs *add* refusals
  (`policy-mode-unsupported`, `policy-fail-on-empty`,
  `profile-margin-denominator`, input byte limits) and the mandatory policy
  *adds* a denial (`baseline-required`).
- **Adjacent gates still green.** Re-ran with pinned Ajv 8.17.1:
  `check-contract-versions.mjs --base origin/ichinya/M7` → `{"ok":true,
  "product":"0.6.3","contractArtifacts":99}`; `test-fixture-provenance.mjs`
  → `{"ok":true,"families":64,"synthetic":64,"evidenceBacked":0}`;
  `test-context-budget-contracts.mjs` → `ok:true`; `test-context-contracts`,
  `test-diagnostic-contracts`, `test-run-history-contracts`,
  `test-semantic-diff-contracts`, `test-classification-contracts`,
  `test-validation-contracts`, `check-structure`, `check-privacy`,
  `check-authority` → all exit 0.
- **Test writer isolation holds.** `git status --porcelain` was empty after a
  full `cargo test -p lekalo-core -p lekalo-cli --locked` (91 test-result
  lines, all `ok`, no `FAILED`) plus the focused context-budget suites
  (core lib 53 passed; CLI 13 passed).
- **No regression in the untouched legacy path.** `lekalo context` is not in
  the fix diff beyond the documented `pub(crate)` widening.

## Cleanliness

`git status --porcelain` is empty (including
`--untracked-files=all tests/fixtures`). The two round-1 residue findings are
genuinely resolved at the root: the writers target temp copies, so the files
do not reappear after a test run — I confirmed this empirically by running the
full test suite and re-checking status.

The three unreviewed artifacts in **R2-m1** and **R2-m2** are the only
untracked-by-intent additions I found. They do not affect behavior.

## Verification actually performed

Passed locally against the reviewed source:

- `cargo clippy --workspace --all-targets --locked -- -D warnings` — **exit 0**,
  plus a forced re-check of `lekalo-core` all-targets (59.73s, clean).
- `cargo test -p lekalo-core -p lekalo-cli --locked` — 91 suites, all `ok`, no
  failures, no ignored.
- `cargo test -p lekalo-core --lib context_budget --locked` — 53 passed.
- `cargo test -p lekalo-cli --test context_budget --locked` — 13 passed, 0
  ignored.
- `node scripts/test-context-budget-contracts.mjs` (pinned Ajv 8.17.1) —
  `ok:true`, `liveChecked:true`; and the negative control: exit 1
  `binary-missing` with the binary renamed away.
- `node scripts/check-contract-versions.mjs --base origin/ichinya/M7` —
  `{"ok":true,"contractArtifacts":99}`.
- `node scripts/test-fixture-provenance.mjs`, `check-structure.mjs`,
  `check-privacy.mjs`, `check-authority.mjs`, and six sibling contract gates —
  all exit 0.
- Live CLI probes: golden byte equality and determinism; description-only
  growth → `+9` visible delta on `minimumRequiredSemanticTokens` /
  `contextClosureEstimatedTokens` / `minimumSafeContextEstimate`; malformed
  baseline → exit 1 `baseline-malformed`; unknown-with-value → exit 1
  `baseline-state-with-value`; mandatory policy without a baseline → exit 3
  `baseline-required`; integration fixture 5 modules / hops 2; a
  correctly-pinned mandatory run with a consumed baseline → **pins read
  `unknown`** (R2-M1).

Failed:

- `node scripts/test-context-budget-contracts.mjs` with `target/debug/lekalo`
  absent → **exit 1, `binary-missing`** (R2-B1). This is a deliberate negative
  control reproducing the CI `contracts` job's step sequence, not a claim about
  a hosted CI run.

Not verified: a hosted CI run (none claimed); a full cold
`cargo clippy --workspace --all-targets` from an empty target directory — my
workspace-wide clippy was served from an existing build cache, so I forced a
real recompile only for `lekalo-core`, which is where the original defect
lived. Practical limit: the comparison payload's exact delta magnitude depends
on the description chosen, so I verified the mechanism rather than the fix
report's specific `+22`.

## Recommendation

Not merge-ready, but close — this is now a plumbing round, not a correctness
round. The metric arithmetic that round 1 found wrong is right, and I could
not break it. Minimum before merge:

1. Move `test-context-budget-contracts.mjs` into the `build-test` job after
   `cargo build --workspace --locked` (or add a CLI build to `contracts`), and
   fix the stale header comment that still describes the binary probe as
   skippable (R2-B1).
2. Re-serialize the report after `with_pins` and add a test asserting both pins
   reach the output on a run that actually set them (R2-M1).
3. Delete `scripts/.fix-absence.mjs`, `scripts/.fix-envelope.mjs`,
   `scripts/.fix-envelope2.mjs`, and `cb-gate-probe/` (R2-m1, R2-m2).
4. Optional: give `sharedRequiredTokens` a defensible owner instead of the
   lexicographically-first dependency (R2-m3).
