# Independent Codex review of issue #103

Verdict: **ISSUES**. Findings: **4 blocker, 10 major, 2 minor**.

Reviewed implementation: `3790d337`, against `origin/ichinya/M7` at
`9510dd0767a56c0ab34b8d3c8ceb2a14db8de825`, on `ichinya/m7-issue-103`.
The checkout and remote branch also contained `f53c4c97`, which adds only
another review document; its contents were not used in this independent review.
No implementation changes were made. This review document is the sole tracked
file added by this review.

Read `docs/m7/issue-103-research.md` and the live issue
[ichinya/lekalo#103](https://github.com/ichinya/lekalo/issues/103), then inspected
the implementation diff, report builders/renderers, CLI paths, contracts,
registry/profile successors, workflow, and tests. The implementation delta is
90 files, 6,930 additions, and 120 deletions, including the research document.
Findings below refer to implementation source line numbers at `3790d337`.
Verification ran on Windows with Node 24.13.0, Node 18.20.8, and Ajv 8.17.1.
Hosted CI and actual GitHub annotation/code-scanning ingestion were not run.

## Findings

1. **blocker — Report destinations can destroy analyzed source or locks during a read-only check.**
   Evidence: `crates/lekalo-cli/src/report_output.rs:90`, `:94`, `:101`,
   and `:134`; `crates/lekalo-cli/src/main.rs:9037`.
   Destination validation accepts every existing regular file and follows
   parent directories with `metadata`; `File::create` then truncates it.
   There is no exclusion for source, `lekalo.lock`, baselines, generated
   manifests, or history, and no no-follow protection for parent links or an
   atomic publication step. In a disposable valid fixture, after explicitly
   creating its lock as test setup, `generate --check --report-file
   lekalo/project.yaml --project .` exited **0** and replaced the model file
   with a `lekalo/ci-report/v0.6.3` document. This defeats the no-write
   acceptance criterion even when the analysis succeeds. Refuse analyzed or
   protected destinations before analysis and publish through a confined,
   no-follow, atomic report writer. Add source/lock overlap and linked-parent
   negative controls; the existing test checks only an ordinary `out/` path.

2. **blocker — Secret redaction and publication admission are absent, with a reproduced leak.**
   Evidence: `crates/lekalo-core/src/ci_report/build.rs:421`, `:430`;
   `crates/lekalo-cli/src/report_output.rs:133`;
   `docs/ci-reports.md:118`. Diagnostics are copied into the report and
   publication is unconditionally `ci-derived/allowed`; neither the accepted
   privacy evaluator nor redaction/leak scanning runs before output. Registry
   field allow-lists and bounded strings do not remove secret values. Changing
   a fixture's type reference to
   `alpha.ghp_abcdefghijklmnopqrstuvwxyz0123456789abcd` produced exit 1,
   retained that synthetic token in `diagnostics[0].data.reference` in both
   JSON report and CLI JSON stderr, and marked the report publishable. A
   synthetic token in an unexpected filename also survived in the JSON
   report's source path. Apply explicit privacy/export admission and
   secret scanning before all report/log sinks, with safe diagnostics on
   refusal. Add canary tests for every format and the status-owned streams;
   the claim that secrets cannot enter diagnostics is demonstrably false.

3. **blocker — The new required contract gate crashes on the supported Node 18 CI leg.**
   Evidence: `scripts/test-ci-report-contracts.mjs:104`;
   `.github/workflows/ci.yml:19`, `:49`. `Set.prototype.isSubsetOf` is used
   unconditionally although CI explicitly runs Node 18.x. With exact Ajv
   8.17.1 provisioned, `npx --yes --package node@18.20.8 node
   scripts/test-ci-report-contracts.mjs` fails with
   `TypeError: referenced.isSubsetOf is not a function` while examining the
   goldens. The same gate passes on Node 24.13.0. Use an implementation
   supported by both configured runtimes and execute the gate on both; do not
   remove the existing compatibility leg to hide the regression.

4. **blocker — CI smoke never exports REPORTS, and its failure leaves an unsafe upload pattern.**
   Evidence: `.github/workflows/ci.yml:190`, `:202`, `:214`, `:216`, `:220`.
   Bash sets a shell variable `REPORTS`, while the three Node checks read
   `process.env.REPORTS`. Running the exact first Node check from Bash after
   that assignment fails with `Cannot find module 'undefined/validate.json'`
   and exit 1. Under `set -e`, the step consequently never writes
   `REPORTS_DIR` to `GITHUB_ENV`; earlier build failures have the same effect.
   The following `always()` upload still uses `${{ env.REPORTS_DIR }}/*`.
   With the variable absent, this becomes `/*`, rather than the intended
   report directory. The broadening is established from the workflow; an
   actual root upload was deliberately not attempted. Export the Node input,
   establish the confined upload destination independently of smoke success,
   and upload explicit finalized filenames with a guard against an absent
   destination. This is a CI regression and invalidates the claimed fork
   artifact allow-list.

5. **major — SARIF fails on legitimate diagnostics and does not satisfy SARIF 2.1.0.**
   Evidence: `crates/lekalo-core/src/ci_report/sarif.rs:58`, `:227`, `:228`,
   `:272`; `scripts/test-ci-report-contracts.mjs:219`.
   The diagnostic contract permits a source path without a range, but the
   renderer indexes the source object with `source["range"]`. A fixture with
   an unexpected canonical filename produces that normal path-only
   diagnostic; requesting SARIF panicked at line 228, exited **101**, and
   produced neither report nor domain envelope. Independently, the driver
   serializes `information_uri` instead of SARIF's `informationUri`. Both the
   committed SARIF golden and a live located report were refused by the
   [OASIS SARIF schema](https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/schemas/sarif-schema-2.1.0.json)
   using a Draft-04 validator: the driver has the forbidden additional
   property `information_uri`. Use optional typed location access and the
   correct wire spelling, then validate every golden/live shape against the
   real schema. The present gate checks a handful of fields rather than
   compiling that schema.

6. **major — SARIF project-relative paths are mislabeled repository-relative.**
   Evidence: `crates/lekalo-core/src/ci_report/sarif.rs:232`;
   `crates/lekalo-cli/src/main.rs:2584`, `:9193`.
   The renderer copies the diagnostic's project-relative path directly under
   `%SRCROOT%`, described as the repository root. Neither acquisition nor
   rendering records or applies the selected project's repository prefix.
   A located diagnostic in the nested disposable fixture
   `target/review-103-codex-probes/sarif-monorepo` emitted URI
   `lekalo/modules/beta/entities.yaml`; its actual repository-relative path
   starts with that fixture prefix. A consumer project in a monorepo has the
   same failure: inline results point to the wrong file or no file. Bind a
   verified repository/project relationship and preserve safe source spelling
   when translating locations, with nested-project and containment tests.
   Existing SARIF tests construct paths that already look repository-relative.

7. **major — Policy is lost for suite rows and JUnit, and infrastructure can be waived as absence.**
   Evidence: `crates/lekalo-core/src/ci_report/build.rs:198`, `:234`;
   `crates/lekalo-core/src/ci_report/model.rs:490`;
   `crates/lekalo-core/src/ci_report/junit.rs:81`, `:89`.
   `apply_case_policy` ignores its policy argument; case outcomes are later
   recomputed without policy. A linked probe of the reviewed library gave
   exit **0** for the same optional unavailable suite case under default,
   strict, and lenient, although strict must promote it. Conversely, a
   strict optional unavailable *check* evaluated to exit **4/error**, but
   JUnit reconstructed a `CaseRow` and rendered it as `<skipped>`. An
   optional `Unavailable/Infrastructure` check under lenient also evaluated
   to **0/skip**, masking a provider failure rather than permitting a missing
   component. Persist one evaluated outcome for all row kinds and project it
   consistently; restrict absence exceptions by source/failure class. The
   CLI additionally hardcodes `CiPolicy::Default` at all four call sites;
   the deferred policy-file surface does not cure these core evaluator bugs.

8. **major — Early failures can look ready/complete or omit the requested report, and process exit is not the report authority.**
   Evidence: `crates/lekalo-core/src/ci_report/build.rs:254`, `:274`, `:305`;
   `crates/lekalo-core/src/ci_report/model.rs:696`;
   `crates/lekalo-cli/src/main.rs:9024`, `:9134`, `:9288`, `:9291`, `:5104`.
   A fresh fixture's `verify --locked --report-file out/verify.json` exited
   **1** for a missing lock, yet its report had no rows and
   `evaluation={status:invalid,exitCode:1,verdict:ready,coverage:complete,
   complete:true}`. The library validator accepts this contradiction.
   `generate --check --locked` returns from its lock preflight without
   writing the requested report at all; several verify preflights also bypass
   reporting. Reported commands return `compose(result, reported)`, not the
   evaluation, despite `docs/ci-reports.md:62` saying the evaluation owns exit.
   A blocked plain readiness report carried exit 4 while the process exited
   0; its informational mode is documented, but the invocation does not
   distinguish gate mode from informational mode. Model preflight failures
   as terminal blocking/incomplete evidence, always finalize requested
   reports where possible, and make gate-mode process/envelope/report results
   coherent while retaining explicitly identified informational behavior.

9. **major — The reported lock digest is the resolver request digest, not the lock revision.**
   Evidence: `crates/lekalo-core/src/ci_report/provenance.rs:82`;
   `crates/lekalo-core/src/lockfile/parse.rs:47`;
   `crates/lekalo-core/src/orchestration/verify.rs:422`.
   `lock_block` calls `lock.request_digest()`, whereas the accepted lock and
   verify owners use `lock.digest()` to bind the canonical lock payload.
   In the locked fixture, CI reported
   `sha256:8f194d076379133e399f4a99a3157dc58e5453b2c827e0a59e0ba04d86f8a7f9`,
   exactly `resolver.request_digest`; the actual canonical lock digest and
   the ordinary verify receipt's `lockDigest` were
   `sha256:bbe9c51d3a4a579cd10f3b6794090af14acb9e2dea2bceede83a7f5440334baa`.
   Resolver choices and resolved pins are not identified by the request
   digest. Use the lock owner's payload digest and test equality with the
   existing receipt and sensitivity to accepted resolved-pin changes.

10. **major — Valid input pins and actual verify scope are lost.**
    Evidence: `crates/lekalo-core/src/ci_report/provenance.rs:137`;
    `crates/lekalo-core/src/artifacts/check.rs:55`;
    `crates/lekalo-cli/src/report_git.rs:151`, `:162`, `:171`;
    `crates/lekalo-cli/src/main.rs:9272`.
    The provenance builder uses `GenerateService::inputs`, which also
    requires a lock/manifest. An otherwise valid, successful validation
    without a lock therefore reported model and IR as `unknown/invalid` and
    omitted its effective validation profile. Readiness always labels model
    and IR `not-applicable` and emits no profiles, even for a valid project.
    Verify additionally hardcodes `mode=full`, `targets=[]`, `modules=[]`:
    a live `verify --target node-typescript --module alpha` report lost both
    selectors. Adapter-selected profile pins are also absent from the
    built-in-only profile projection. Capture each available pin from its
    actual owner independently of unrelated preparation failures, and carry
    resolved invocation scope/profile identities through the typed seam.
    Tests currently assert only that a lock digest starts with `sha256:`
    and that a Git commit has the expected length.

11. **major — Scenario JUnit collapses assertion identity/counts and loses mixed infrastructure failures.**
    Evidence: `crates/lekalo-cli/src/main.rs:9202`, `:9220`, `:9234`;
    `crates/lekalo-core/src/orchestration/verify.rs:818`;
    `crates/lekalo-cli/tests/ci_report.rs:490`.
    Every scenario record/step is replaced by one
    `scenarios.execution/rollup` case using only the component's winning
    reason. With an accepted current-IR run record containing two failed
    assertions and one infrastructure outcome, live verify exited 1 but
    emitted just **one** scenario testcase, **one** assertion failure, and
    **zero** infrastructure errors. Scenario/step identity and the second
    failure were absent. The test named
    `verify_blocked_scenario_failure_is_a_junit_error_with_nonzero_evaluation`
    creates no failing record, requests Markdown, and asserts only a heading
    and that a process exit exists. Retain assertion-level typed rows before
    aggregation, including stable scenario/test/step identity and every
    failure class, and add a real CLI-to-JUnit mixed-outcome test. The current
    implementation does surface a generic failing suite, which is partial
    AC2 evidence, not the documented assertion-count projection.

12. **major — The advertised closed schema admits arbitrary nested payloads and wrong provenance types.**
    Evidence: `contracts/ci-report.schema.v0.6.3.json:136`, `:145`, `:484`,
    `:488`, `:492`, `:496`, `:500`;
    `scripts/test-ci-report-contracts.mjs:68`.
    The embedded diagnostic is not a faithful copy of the published
    diagnostic schema: `data` and `metadata` are open objects;
    related locations, causes, and fixes have no item schema. Ajv accepted
    a diagnostic containing an arbitrary nested secret object, arbitrary
    location/cause objects, and `{command:"rm"}` in `fixes`. The shared
    `valueState` union also admitted `git.commit={state:known,value:false}`
    and `lock.digest={state:known,value:"0.6.3"}`. The semantic gate adds no
    leaf-type or nested diagnostic validation to repair this. Reuse the
    complete diagnostic definitions, apply specific state/value schemas to
    each provenance leaf, and validate row/status/coverage coherence and
    bounds with shared semantic rules. Add these counterexamples to the
    required refusal vectors.

13. **major — The working-set Git call and content hashing are not bounded or cancellation-aware.**
    Evidence: `crates/lekalo-cli/src/report_git.rs:44`, `:54`, `:56`, `:59`,
    `:78`. `.output()` waits for Git termination and collects all output
    before checking the 10-second deadline or 1-MiB cap. It cannot kill a
    stuck process at the deadline. Then `fs::read` follows tracked paths and
    reads complete files with no per-file/aggregate cap or deadline.
    A disposable fake Git that returned valid commit/status facts and slept
    12 seconds for `ls-files` made validation wait **13,244 ms** and exit 0;
    only after waiting did it label the digest unavailable. A forever-stuck
    process was not run. Bound process lifetime/output while executing,
    terminate/reap on deadline, and hash through safe bounded reads with
    cancellation/deadline checks. A cancellation enum unit test and a CI
    step timeout do not establish this implementation's liveness behavior.

14. **major — The evidence-retention seam can attach a previous invocation's receipt to a failed verify.**
    Evidence: `crates/lekalo-core/src/orchestration/verify.rs:129`, `:149`,
    `:159`, `:273`, `:431`. `LAST_COMPONENTS` is updated only after full
    receipt assembly, and is never cleared or consumed at invocation entry.
    A linked-library probe called `verify_with_components` on a valid locked
    project, then a missing-lock project on the same thread. The first call
    returned `exit=0,components=7,verdict=Some(Ready)`; the second returned
    `exit=1,components=7,verdict=Some(Ready)`, reusing the old receipt.
    Ordinary separate CLI processes hide this, but the newly public core
    API violates per-run evidence custody. Return components directly with
    the pipeline result, or clear and consume thread-local state for every
    call. Add successful-then-preflight-failure and failed-then-failure
    sequence tests.

15. **minor — JUnit counts successful cases as skipped.**
    Evidence: `crates/lekalo-core/src/ci_report/junit.rs:169`.
    `skipped = tests - failures - errors` also counts passing cases. Live
    successful validation produced `tests=1,failures=0,errors=0,skipped=1`
    with no `<skipped>` child. A real XML parser accepted the XML and
    confirmed zero skipped elements. Count skipped rows when emitting them;
    check all aggregate counters against the emitted children.

16. **minor — Report JSON is deterministic but violates its declared byte-sorted canonical form.**
    Evidence: `crates/lekalo-core/src/ci_report/model.rs:650`;
    `contracts/ci-report.schema.v0.6.3.json:5`;
    `scripts/test-ci-report-contracts.mjs:150`, `:153`.
    Struct serialization preserves declaration order, starting with
    `schema_version,identity,producer`, rather than byte-sorted keys starting
    with `checks,commandResult,diagnosticIndexes`. The same problem occurs
    in nested structs. Committed goldens pin declaration order and the gate
    compares `JSON.stringify(parsed)` with `text.trim()`, preserving input
    key order and ignoring CRLF/multiple trailing newlines. Same-input
    reruns do produce stable LF bytes, but that is distinct from meeting the
    advertised canonical contract. Emit recursively sorted JSON or
    explicitly reconcile the contract and consumers; enforce exact newline
    bytes in the golden gate.

## Acceptance-criteria checklist

`PASS` means demonstrated in this review; `PARTIAL`, `FAIL`, and `NOT VERIFIED`
must not be treated as issue completion.

| Criterion | Result | Code/test evidence and limit |
| --- | --- | --- |
| AC1: Diagnostics inline through SARIF/annotations with safe repository paths | **FAIL** | Registry-derived LEK rule IDs and Unicode coordinates are present; real-schema rejection, path-only panic, and missing project-to-repository translation are F5/F6. There is no annotation or SARIF-upload implementation in this delta, and hosted inline placement was not verified. |
| AC2: Scenario failures appear in JUnit | **PARTIAL** | A live failing record yields a failure and nonzero verify exit, but one rollup loses case identities, multiple failures, and concurrent infrastructure errors (F11). Synthetic core projection tests pass; the named CLI scenario/JUnit test does not exercise its name. |
| AC3: Required failure nonzero; optional unavailable policy-driven | **FAIL** | Existing required-error, denied, cancelled, and optional-check table tests pass; live required adapter absence exits 4, and `readiness --check` exits 4 when blocked. Strict suite policy is ignored, strict JSON/JUnit disagree, infrastructure can be waived, and preflight verdicts are false-ready (F7/F8). Plain readiness retains the documented informational compatibility mode. |
| AC4: `generate --check` detects drift without writes | **FAIL** | The unchanged drift/check service and all 10 existing generate tests pass, including snapshot and drift controls. The new reported check test exercises a vacuous clean check and exit 0 rather than drift/exit 1. Source-overlap reporting overwrites a model on exit 0 (F1), and a locked preflight omits its report (F8). |
| AC5: Fork/untrusted PR workflow safe by default | **FAIL** | `pull_request` and global `contents: read` remain; this delta introduces no privileged secret or `pull_request_target`. However the report-upload path broadens on failure (F4), publication admission/redaction fails (F2), and actual fork execution was not verified. The stronger public-action posture remains design-only. |
| AC6: Exact git/model/lock/profile revisions bound | **FAIL** | Git commit value states and report-content hashing exist and focused determinism tests pass. Lock revision is misidentified (F9); valid validation/readiness pins, adapter profiles, and actual target/module scope are lost (F10). Schema permits false known leaf types (F12); the thread-local seam can retain previous-run evidence (F14). |
| AC7: Executable examples for core, Node consumer, Laravel fixture | **PARTIAL / NOT DELIVERED** | The core `ci.yml` smoke is present but broken (F3/F4). ADR-0048 section 5 explicitly defers `examples/ci/*.yml`; no such workflows or in-repository composite `action.yml` were found. Public action release and actual consumer runs are outside this dispatched implementation scope and remain necessary for full issue acceptance. |
| Real secret redaction in all sinks | **FAIL** | Synthetic canary survives in JSON artifact and status-owned output, with publication allowed (F2). No CI-report canary tests were found; schema field bounds are not redaction. |
| Deterministic bytes, LF, no timestamps/UUIDs/host absolute paths | **PARTIAL** | Focused repeated-output tests pass; ordinary tested report bytes use LF and omit ambient timestamps/host paths. Byte-sorted canonical JSON is not emitted or checked (F16); hostile data is not protected by a complete closed schema/redaction path (F2/F12). |
| Contract successor versioning and LEK registry conventions | **PASS, with schema quality failure separately recorded** | Version checker against `9510dd07` passes at product 0.6.3. CI-report identity/discriminator and registry/profile successors match their Rust consumers. Diagnostic, validation-profile, classification, and expression gates pass; `ci.report-write-failed/LEK-CI-001` and `ci.required-check-missing/LEK-CI-002` are registered. This does not validate the report's nested schema (F12). |
| No automatic lock/baseline updates | **PARTIAL** | Normal command paths introduce no automatic lock/baseline repair; locks created in probes were explicit test setup. The report writer can nevertheless overwrite protected existing paths (F1), so the invariant is not enforced for reported checks. |
| Timeouts/cancellation and infrastructure distinction | **FAIL / NOT VERIFIED** | Cancellation/denial enum tests block, and the smoke step has a timeout. Working-set Git/content reads are unbounded (F13), mixed scenario infrastructure detail is lost (F11), and no action child-tree cancellation or cross-OS cancellation qualification was delivered. |
| Existing `--json` envelope/status conventions preserved coherently | **PARTIAL** | Ordinary validate/check output stays on its status-owned stream; typed write-failure and readiness gate tests pass. Preflight failure artifacts and evaluated status/exit/verdict do not have one consistent authority (F8). SARIF rendering can panic before any envelope (F5). |

## Verification performed

Successful focused gates:

- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- `cargo test -p lekalo-core --lib ci_report --locked`: 16 passed.
- `cargo test -p lekalo-cli --test ci_report --locked`: 13 passed.
- `cargo test -p lekalo-cli --test generate --locked`: 10 passed.
- `node scripts/check-contract-versions.mjs --base 9510dd07`: product 0.6.3,
  96 contract families accepted.
- On Node 24.13.0 with exact Ajv 8.17.1: CI-report gate (4 goldens,
  8 refusal vectors), diagnostic gate (459 registry entries), validation
  profile gate, classification gate, and expression gate.
- `git diff --check 9510dd07..3790d337`.

Adversarial checks reproduced the failures described above. CLI fixture copies
were isolated under the ignored `target/review-103-codex-probes/` tree; fixtures
and locks were prepared there explicitly, never in analyzed repository sources.
Policy/component probes linked the reviewed library without modifying it;
their executables and external schema-validator dependencies were outside the
checkout. The SARIF schema was fetched from the OASIS URL linked in F5 and
validated with Ajv 8.17.1 plus `ajv-draft-04` 1.0.0. JUnit was also parsed by
a real XML parser before checking child/count coherence. No real secret,
privileged GitHub upload, root artifact upload, or indefinitely hanging child
was used in these probes.

The CI-report script's JUnit section at
`scripts/test-ci-report-contracts.mjs:240` is a no-op over a null candidate;
the SARIF section checks selected fields only. Consequently the passing gate
does not prove XML consumer validity, SARIF schema validity, privacy, or the
assertion-level CLI lifecycle. Full workspace tests and hosted OS/fork/consumer
acceptance were not rerun; the focused passes do not override the reproduced
failures.

The diff preserves the existing major workflow gates and adds the report
step, rather than deleting those gates. It does not provide the public action,
executable Node/Laravel workflows, annotation emission, job-summary publication,
native/script observation ingestion, offline rendering/bundling, or the
AIFHub Extension adapter; these are recorded as deferred scope in the
implementation map. ADR-0048 nevertheless calls an in-repository composite a
working reference without an `action.yml` in this checkout. Those delivery
claims need reconciliation, and the deferred integrations need their own
acceptance evidence before the full GitHub issue can be considered complete.

Required next work: correct the reproduced defects, replace nominal format
and lifecycle tests with the failing controls above, then independently review
the corrected implementation SHA and qualify the required hosted workflows.
