# Issue #103 — independent review (Devin)

**Verdict: ISSUES.**

Reviewed `ichinya/m7-issue-103` (head `3790d337`, diff base
`origin/ichinya/M7` = `9510dd07`), eight commits, ~6.9k added lines.
Method: full diff read, acceptance criteria mapped to code and tests,
`cargo test -p lekalo-core ci_report` (16/16 pass), `cargo test -p
lekalo-cli --test ci_report` (13/13 pass), `cargo fmt --check` and
`cargo clippy` clean, `check-contract-versions.mjs --base
origin/ichinya/M7` ok, `test-ci-report-contracts.mjs` run locally with
pinned ajv 8.17.1 (ok: 4 goldens, 8 adversarial vectors refused, all
three verdicts covered). No code was changed by this review.

The core design is sound and the letter of most acceptance criteria is
met, but the workflow dogfood step is broken (CI will be red on every
matrix leg) and the report surface has several evidence-fidelity and
confinement defects that should be fixed before merge.

## Findings

### 1. [blocker] ci.yml report smoke always fails — `REPORTS` is never exported

`.github/workflows/ci.yml:190` sets `REPORTS="$RUNNER_TEMP/lekalo-ci-reports"`
as a plain shell variable. The three `node -e` assertions at lines 202,
210 and 212 read `process.env.REPORTS`, which is `undefined` in the
child process — `require("undefined/validate.json")` throws
`MODULE_NOT_FOUND` and `set -euo pipefail` kills the step. The
`build-test` job therefore fails on all three OS legs on every run, and
because `REPORTS_DIR` only reaches `$GITHUB_ENV` at line 214 (after the
failing assertions), the `if: always()` upload step (line 220) runs with
`path: ${{ env.REPORTS_DIR }}/*` unresolved to `/*` — at best an
`if-no-files-found: error`, at worst a glob over the runner root.

### 2. [major] `--report-file` can truncate arbitrary existing files, including model and lock inputs

`validate_destination` (`crates/lekalo-cli/src/report_output.rs:90-107`)
only checks that the parent is an existing directory and the target is
absent or a regular non-symlink file; `File::create` at line 134 then
truncates whatever regular file was named. `lekalo validate
--report-file lekalo.lock` or `--report-file <model-source.yaml>`
happily destroys the input the run just validated, after which the
report pins the pre-truncation digests. The research
(`docs/m7/issue-103-research.md:228`) requires rejecting destinations
overlapping source/model/locks/baselines/history and pre-existing
arbitrary files; the implementation's "confined" claim in
`docs/ci-reports.md` and the impl map is stronger than what is
implemented (no output-directory grant, no input-overlap refusal, no
existing-file refusal).

### 3. [major] provenance collapses to `unknown/invalid` on any project without a lock

`provenance_block` (`crates/lekalo-core/src/ci_report/provenance.rs:137`)
routes through `GenerateService::inputs`, whose `Prepared::prepare`
(`crates/lekalo-core/src/artifacts/check.rs:57-60`) hard-fails on
`LockState::Absent` — *after* `current_inputs` already computed the
model/IR pins. `run_validate_reported` and friends then fall back to
`empty_provenance_for` (`crates/lekalo-cli/src/report_git.rs:127-146`),
which labels model and IR `unknown/invalid` and drops the profile and
adapter pins entirely. `lekalo validate` is explicitly a lock-free
command, so the common case emits a report with no model/IR/profile
binding at all and a reason (`invalid`) that is false — the model just
passed validation. The committed golden
`tests/fixtures/ci-report/valid.validate.golden.json` pins exactly this
(model `unknown/invalid`, `profiles: []` on a `ready`/exit-0 run), i.e.
the suite enshrines the gap instead of catching it. AC6 (exact
git/model/lock/profile revision binding) is only satisfied when a lock
exists.

### 4. [major] `readiness --check` report can claim blocked/exit-4 while the process exits 0

The CLI gate (`crates/lekalo-cli/src/main.rs`, the `gate` closure in
`run_readiness`, ~4988-5010) fails the run only when the doctor document
verdict is the string `blocked`. The report evaluator, however, maps a
*required* `degraded` check to `error`
(`crates/lekalo-core/src/ci_report/build.rs:178-184`), which makes
`evaluation.verdict` `blocked` and `evaluation.exitCode` 4. A required
degraded check is reachable — `lock.freshness` is required for release
and degrades on a stale/absent lock
(`crates/lekalo-core/src/doctor/checks.rs:439-440`), and the committed
readiness golden itself shows a required row with
`sourceOutcome: "degraded"`. On a run where degraded is the worst
required state the doctor verdict stays `degraded`: the process exits 0
while the emitted report self-describes `blocked`/exit 4. The contract
says `evaluation` is "the authoritative gated outcome" the CLI exits
with; here it disagrees with the actual process exit inside a single
artifact.

### 5. [major] the verify report misrecords the invocation

`verify_reported` (`crates/lekalo-cli/src/main.rs`, ~9280) hardcodes
`mode: "full"`, `targets: Vec::new()` and `modules: Vec::new()` in the
`CommandOutcome` regardless of `--changed`, `--target`, or `--module`.
A scoped `verify --changed` or `verify --target node-typescript` run is
recorded as a full, unscoped verify — the evidence artifact misstates
what was actually evaluated. (Validate records `--module` correctly;
`generate --check` legitimately cannot take targets/modules.)

### 6. [major] scenario evidence reaches JUnit only as a single rollup testcase

The scenario component becomes one `SuiteDraft` containing exactly one
case, `scenarios.execution/rollup` (`crates/lekalo-cli/src/main.rs`
~9199-9210). No per-scenario/test/step identity, no assertion-level
cases, no expected-vs-observed coverage testcase — a failing Planner
suite and a single failing assertion are indistinguishable in JUnit,
and the failing scenario's identity is nowhere in the report. The
research (research doc lines 162, 333) requires one testcase per
assertion with stable scenario/test/step/kind identity; the impl-map
claim "one aggregated row per failure class observed" also overstates
it — the code emits one row unconditionally. The e2e test
`verify_blocked_scenario_failure_is_a_junit_error_with_nonzero_evaluation`
(`crates/lekalo-cli/tests/ci_report.rs:489-519`) asserts only that the
process exits and Markdown exists; it never inspects JUnit or a failure
row despite its name.

### 7. [major] JUnit `skipped` attribute counts passing cases

`junit.rs:169` computes `skipped = tests - failures - errors`, so every
passing testcase (no child element) is reported as skipped — an
all-pass gate suite reports `skipped="N"` equal to its full test count.
The documented invariant is that counts derive from emitted cases; the
`skipped` count should count emitted `<skipped>` elements only.
Consumers show wrong skip totals on every JUnit document.

### 8. [minor] the "bounded" `git ls-files` wait is unbounded

`bounded_working_set` (`crates/lekalo-cli/src/report_git.rs:44-58`) uses
`Command::output()`, which blocks until the child exits — the 10 s
`start.elapsed()` check runs only *after* git has already finished, so a
hung `git ls-files` stalls every reported command indefinitely, unlike
the sibling `doctor_git::bounded_git` which actually kills at its
deadline. The per-file reads below also have no deadline; the doc
comment's "the deadline applies" is not implemented.

### 9. [minor] `readiness --check` collapses every blocked cause to `unavailable`/4

The gate returns `DomainResult::unavailable(ci.required-check-missing)`
for any blocked verdict, so an invalid model or a custody refusal under
`--check` reports exit 4 instead of the classified status the research
prescribes (invalid → 1, denied → 3). The diagnostic's `check` data
field is also filled with the literal string `"readiness"` — a command
name, not a check id — so the emitted reason names the wrong thing.

### 10. [minor] a report-write failure after a failed command is silently dropped

`compose` (`report_output.rs:150-161`) returns the command result
unchanged when both the command and the report write fail — the
`ci.report-write-failed` fact disappears entirely (no joined diagnostic,
no stderr note), while `docs/ci-reports.md` claims "the report failure
is not silently swallowed". In that path it is exactly that.

### 11. [minor] the verify evidence seam can attach stale components

`verify_with_components` reads a thread-local `LAST_COMPONENTS`
(`crates/lekalo-core/src/orchestration/verify.rs:157-177`) that is only
written at the end of `run` and never cleared at the start; a second
in-process `verify` that fails early would bind the previous run's rows
to the new report. The one-shot CLI makes this latent today, but the
seam is unsound for any embedding or repeated in-process use.

### 12. [minor] `CiPolicy::Strict`/`Lenient` are unreachable and suite cases ignore policy

No `--ci-policy` flag exists; every command passes `CiPolicy::Default`.
`apply_case_policy` and `evaluate` take `_policy` and never use it, so
case rows hard-code the default warn table — strict/lenient cannot
promote an optional suite absence. The policy-table tests pin check-row
behavior only; the "policy-driven" suite edge is dead code presented as
the delivered mechanism (the research's file-borne policy is separately
and acceptably deferred).

### 13. [minor] SARIF emits `information_uri` instead of `informationUri`

`sarif.rs:58` serializes the driver field as `information_uri`; the
SARIF 2.1.0 `toolComponent` property is `informationUri`, so the field
is silently ignored by consumers — which is also why the absent
official-schema validation (finding 14) did not catch it.

### 14. [minor] the contract gate's JUnit pin block is a no-op and SARIF is not schema-validated

`scripts/test-ci-report-contracts.mjs` section 5 pushes one placeholder
into `junitCandidates` and iterates it doing nothing
(`const _ = candidate`); no JUnit golden is committed or checked. SARIF
is verified by a handful of ad-hoc assertions, not the pinned official
SARIF schema the research specified (research doc line 332). Related:
`run_readiness` is the only report path that skips `report.validate()`
after mutating `evaluation` by hand (a likely-dead-code patch plus the
unused `command_status` binding), so the one place a report is edited
post-build is also the one place the cross-field invariants are not
re-checked before writing.

### 15. [minor] docs/metadata inconsistencies

`docs/m7/issue-103-implementation.md` says "the frozen 0.4.0 bytes stay
byte-identical" but `contracts/diagnostic-registry.v0.4.0.json` was
deleted outright (its entries carried into the 0.6.3 successor);
`docs/native-gates.md` still cites `dev.lekalo.diagnostic-registry@0.4.0`
as the current registry authority. `crates/lekalo-cli/src/report_git.rs:96-99`
carries a duplicated doc comment and duplicated `#[cfg(test)]`. The
`context.*` family (LEK-CONTEXT-001..008) is registered in the successor
registry but referenced by no producer in this tree — merging #75's
family here is defensible under the one-version-per-commit rule but
registers eight rules with no emitter yet.

## Acceptance-criteria checklist

| Criterion | Verdict | Evidence / gap |
| --- | --- | --- |
| AC1 SARIF surfaces diagnostics inline, repo-relative safe paths | **PASS (caveat)** | `sarif.rs` emits `%SRCROOT%`-relative validated paths, `unicodeCodePoints`, sorted `LEK-*` rules; unit + golden tests pass. Caveats: no official-schema validation; `information_uri` misspelled (findings 13-14). |
| AC2 scenario failures appear in JUnit | **PARTIAL** | A rollup `<failure>`/`<error>` appears, but per-scenario/assertion identity is dropped and the e2e test does not assert JUnit content (finding 6). |
| AC3 required failure nonzero; optional-unavailable policy-driven | **PASS (caveats)** | Exit-policy table fully tested and correct (required unavailable → 4 under all policies; optional → warn/0, error/4, skip/0; denied → 3; cancellation never waived; report-write failure → 4 and never masks a failure). Caveats: strict/lenient unreachable, cases ignore policy, readiness report-vs-exit divergence (findings 4, 12). |
| AC4 `generate --check` detects drift without writes | **PASS** | Check path untouched/read-only; `generate_check_drift_report_is_read_only_and_exit_one` snapshots the tree; only the granted report path is created — subject to the destination-confinement hole (finding 2). |
| AC5 fork/untrusted PR safe by default | **PASS (caveat)** | `contents: read`, no secrets/`pull_request_target`/persisted credentials; upload is a runner-temp allow-list with `if-no-files-found: error`. Caveat: `if: always()` + unset `REPORTS_DIR` glob hazard (finding 1). |
| AC6 exact git/model/lock/profile revision binding | **PARTIAL** | Real pins (git commit/dirty/working-set digest, model/IR digests, lock digest, embedded-profile bytes digest) verified in goldens when a lock exists; collapses to `unknown/invalid` with a false reason when it does not (finding 3). |
| AC7 consumer examples (core/Node/Laravel) + public action | **NOT DELIVERED (declared)** | No `action.yml`, no `examples/ci/*.yml`; ADR-0048 records the composite design only. Impl map declares this partial explicitly. |
| Secret redaction is real | **PASS (by construction)** | Closed vocabularies only; no env/argv/host/raw-output fields exist in the model; diagnostics ride registry allow-lists. No leak-scan/canary test on emitted bytes — prevention is structural, not scanned. |
| Deterministic bytes | **PASS** | Byte-identical reruns pinned at unit and e2e level; compact canonical JSON, one LF, sorted keys, no timestamps/paths. SARIF `lekaloReportDigest` verified to match `sha256` of the committed JSON golden. |
| Contract version bump consistent | **PASS** | `ci-report@0.6.3` matches product 0.6.3; `check-contract-versions.mjs` ok; registry/profile successors bump all identity constants, embeds, consumers and fixtures together. |
| No automatic baseline/lock updates | **PASS** | All four report commands are read-only over the project; the sole permitted write is the granted report path. |
| Timeouts/cancellation sane | **PARTIAL** | Cancelled rows fail closed and are never policy-waived (tested); a hard kill produces no report and the upload requires files. Gaps: unbounded `git ls-files` wait (finding 8); smoke step got a 5-min timeout but the research's job-level deadlines/concurrency-cancellation were not added. |
| Readiness exit-0-when-blocked fix | **PASS (caveat)** | `readiness --check` maps a blocked required panel to unavailable/4, tested e2e both directions; informational default preserved. Caveat: single-class collapse + report/process divergence (findings 4, 9). |
| Verify failure-receipt retention fix | **PASS** | `verify_with_components` captures assembled rows before `aggregate_envelopes`; blocked reports carry all component rows with failure classes; terminal result unchanged. |

## Notes

- Verified locally: core ci_report 16/16, CLI ci_report 13/13, fmt/clippy
  clean, contract version check ok, ci-report Ajv gate ok (ajv 8.17.1
  provisioned ad hoc). The pre-existing flaky `cache::tests` note in the
  impl map was not reproduced and not investigated.
- The deleted 0.4.0 registry instance is consistent with
  `docs/versioning.md`'s convention that superseded contract instances are
  removed, but the impl-map wording and `docs/native-gates.md` should be
  corrected (finding 15).
- Bottom line: fix the workflow blocker (finding 1) and the confinement,
  provenance-collapse, exit-coherence, invocation-fidelity, JUnit-counts,
  and scenario-granularity defects (findings 2-7) before merge; the minor
  findings can mostly follow up.
