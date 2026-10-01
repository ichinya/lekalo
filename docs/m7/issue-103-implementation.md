# Issue #103 — headless CI reports and the official workflow: implementation map

Delivered on `ichinya/m7-issue-103` (milestone M7). The headless-CI
surface is real: validate, `generate --check`, verify, and readiness emit
one closed, versioned, deterministic machine-readable report per run with
a stable, policy-driven exit; the two exit-policy defects the research
found are fixed (readiness no longer exits 0 when a required panel is
blocked under the gate mode; verify no longer discards its failure
receipt); and the repository's own CI dogfoods the reports and uploads
them as artifacts.

Commits, in order:

1. `feat(contracts): the ci.* diagnostic family, the registry successor, and the closed ci-report schema (issue #103)`
2. `feat(core): the ci_report model, builders, and pure format projections (issue #103)`
3. `feat(cli): the --report-file side channel and the readiness --check gate (issue #103)`
4. `test(contracts): the ci-report Ajv gate over pinned golden and adversarial vectors (issue #103)`
5. `ci: dogfood the closed CI report surface and upload the report artifacts (issue #103)`
6. `docs: the ci-reports surface and the lekalo-action composite design (issue #103)`
7. this document

## Delivered surface

- **Contract** `contracts/ci-report.schema.v0.6.3.json` (closed,
  `additionalProperties: false` everywhere, bounded arrays/strings): one
  report document binding `producer`, `invocation`, `provenance`
  (git/model/ir/lock/profiles/adapters as value states — `known` carries
  the complete pin, `unknown` only a closed reason, never a fabricated
  value), `commandResult`, `evaluation`, `checks`, `suites`,
  `diagnosticIndexes`, `diagnostics` (the closed
  `lekalo/diagnostic/v0.2.16` items), and `publication` (`ci-derived`).
- **Diagnostic registry successor** 0.4.0 → 0.6.3 (`#75`'s context family
  is merged here because the successor rule forces one version per
  commit): two new `ci.*` rules, `ci.report-write-failed` (LEK-CI-001)
  and `ci.required-check-missing` (LEK-CI-002), both `unavailable`. The
  frozen 0.4.0 bytes stay byte-identical; the validation-profile family
  follows with its registry pin (`#75` successor precedent). Every
  consumer and touched gate updated together
  (`test-diagnostic/validation/classification/expressions-contracts.mjs`).
- **Core** (`crates/lekalo-core/src/ci_report/`): `version` (identities),
  `model` (the typed document with cross-field invariant validation:
  sorted/unique ids, index binding, verdict/row coherence), `build` (the
  `CommandOutcome` seam, the closed `CiPolicy` vocabulary, and the
  deterministic evaluator), `provenance` (value-state blocks over the
  accepted owners), and the pure projections `junit`/`sarif`/`markdown`.
  Sixteen unit tests pin the bytes, the policy table, hostile-input
  escaping, determinism, and the dangling-index refusal.
- **CLI** (`report_output.rs`, `report_git.rs`, wiring in `main.rs`):
  `--report-file PATH [--report-format json|junit|sarif|md]` on validate,
  `generate --check`, verify, and readiness. The flag combination is
  validated before any work; the destination is confined (existing
  directory, no symlink, regular file); the projection renders purely
  from the typed report; the write failure is the typed
  `ci.report-write-failed` unavailable envelope composed so it never
  masks a failing run and never lets a passing run exit 0 without its
  artifact. The Git snapshot reuses the doctor's bounded read-only
  adapter and binds the working-set digest over the sorted tracked
  input inventory.
- **Exit-policy fixes:**
  - `readiness --check`: the informational doctor contract stays exit-0;
    the gate maps a blocked required panel onto the classified
    `unavailable` envelope (`ci.required-check-missing`, exit 4).
  - `verify`: the core `verify_with_components` seam captures the
    assembled component receipt before envelope aggregation; the CI
    report projects every component row with its failure class and the
    scenario suite even on the blocked verdict.
- **JUnit**: scenario/adapter suites plus a synthetic gate suite;
  `<failure>` = evaluated assertions, `<error>` = infrastructure,
  boot, evidence-invalid, security, and required-unavailable;
  `<skipped>` = policy-allowed absence; counts derive from emitted
  cases; attributes XML-escaped; no `system-out`/`system-err`.
- **SARIF 2.1.0**: Lekalo driver with registry-derived rules (sorted by
  the immutable `LEK-*` code), `unicodeCodePoints` columns, `%SRCROOT%`
  base ids with repository-relative safe paths, one-based positions from
  the validated diagnostic spans, and the closed Lekalo property
  projection (`lekaloReportDigest`, `lekaloStatus`, `lekaloExitCode`,
  `lekaloVerdict`). Deterministic across reruns and OSes.
- **Markdown**: verdict/exit heading, counts, the exact pin table, and a
  bounded (20-row) diagnostic table with explicit truncation counts;
  Markdown/HTML/link syntax escaped.
- **Gates**: `scripts/test-ci-report-contracts.mjs` (pinned Ajv 8.17.1:
  four command goldens the real binary produced, canonical-compact form,
  cross-language invariants, eight adversarial refusal vectors, the
  Markdown and SARIF projection pins) registered in the `contracts` job;
  the `build-test` job runs the per-OS report smoke (all four formats +
  the blocked readiness gate + the digest round-trip) and uploads the
  finalized report allow-list with `if-no-files-found: error` and
  `if: always()`.

## Acceptance criteria evidence

| Acceptance criterion | Evidence |
| --- | --- |
| **AC1 — diagnostics inline through SARIF/annotations** | `ci_report::tests::sarif_surfaces_diagnostics_with_safe_locations_inline` (rule derivation, `LEK-*` ruleId, `ruleIndex`, level mapping, safe repository-relative path under `%SRCROOT%`, one-based region, property binding), `sarif_is_deterministic_and_locationless_rules_stay_present` (locationless rules stay, index stays), the committed golden `tests/fixtures/ci-report/valid.sarif.golden.sarif` validated by the Ajv gate, and the CLI `sarif_projection_validates_and_stays_deterministic`. Annotation emission is the action layer's bounded `::error`/`::warning` formatter (ADR-0048 §3) fed by the same normalized diagnostics with repository-relative `file` values. |
| **AC2 — scenario failures in JUnit** | `ci_report::tests::suite_scenario_failures_and_infrastructure_separate_in_junit` (`<failure type="assertion">`, `<error type="infrastructure">`, `<skipped message="unsupported">`, no system-out/err) and `junit_escapes_hostile_case_text`; `verify_reported` (CLI) projects the durable `scenarios.execution` evidence as the scenario suite on every verify outcome, and `verify_report_keeps_declared_absences_visible_and_blocks` pins the e2e surface. The class mapping follows the research table: evaluated assertions are `<failure>`; infrastructure/boot/evidence-invalid/required-unavailable/denied are `<error>`. |
| **AC3 — required failure nonzero; optional unavailable policy-driven** | The core policy table `required_failure_is_nonzero_optional_unavailable_is_policy_driven` (required unavailable → exit 4 under all policies; optional → warn 0 / error 4 / skip 0), `optional_assertion_failure_never_downgrades_to_a_pass` (a genuine optional failure still fails), `security_and_cancellation_rows_are_never_optional` (denied keeps 3; cancellation is never waived), `underlying_command_failure_is_preserved_verbatim`; e2e: `readiness_check_gates_a_blocked_release_nonzero`, `readiness_informational_default_stays_exit_zero`, `report_write_failure_is_typed_and_does_not_mask_the_check`. |
| **AC4 — `generate --check` detects drift without writes** | The check path is unchanged and strictly read-only; `generate_check_drift_report_is_read_only_and_exit_one` snapshots the whole tree (path+size+sha256) around the reported check and asserts project-tree equality; the existing `generate.rs` write-refusal suite still passes; the report side channel is confined by `validate_destination` (existing directory, no symlink, regular file) and only the granted path is created. |
| **AC5 — fork/untrusted PR workflow safe by default** | The workflow keeps `contents: read`, adds no secrets, no `pull_request_target`, no persisted credentials; the upload step writes only the runner-temp report allow-list (`if-no-files-found: error`, `if: always()`, unique per-OS/per-attempt names). Reports carry no secrets by construction (diagnostic allow-lists). The full ADR-0048 security posture (no shell interpolation, command allow-list, no repair commands) is the reviewed design for the public action. |
| **AC6 — exact git/model/lock/profile revision binding** | `provenance_block` pins git (commit/dirty/working-set digest through the bounded read-only adapter), model/IR (the accepted `GenerateService::inputs` pins), lock (version + payload digest through `LockService`), profiles (embedded profile bytes digest), and adapters (locked pins); unknown leaves carry only closed reasons (`unknown_provenance_carries_no_fabricated_values`; the readiness golden pins `unknown/absent` for the missing lock). `report_digest_is_content_bound` proves any pin change moves the digest; `validate_report_binds_provenance_and_stays_a_side_channel` proves the e2e binding; deterministic bytes are pinned across reruns on the host (`validate_deterministic_report_bytes_across_reruns`). |
| **AC7 — example workflows** | Partial by dispatch scope: the repository's own `ci.yml` is the working dogfood example (report smoke + upload per OS), and the three consumer examples (core / Node / Laravel) are specified with their negative controls in ADR-0048 §5 for the public action extraction. Recorded as partial; the acceptance work for real GitHub runs (annotation placement, code-scanning ingestion) belongs to the action release. |

## Issue requirements mapping

- *deterministic exit policy* — the closed evaluator (`build::evaluate`):
  the worst class wins, policy promotion is explicit, the underlying
  result is preserved; legacy streams untouched (the full CLI suite
  passes unchanged).
- *annotations use repository-relative safe paths* — diagnostics already
  enforce the logical-path grammar; SARIF emits exactly those paths under
  `%SRCROOT%` (no rewriting, no absolute spellings; asserted in the gate).
- *logs/artifacts redact secrets* — reports are built from validated
  diagnostics and registry text only; no environment, argv, host, or raw
  output surface exists in the model (the closed schema refuses extra
  members; `extra-field.json` is a refusal vector).
- *fork PRs get no privileged secrets* — see AC5.
- *cache keyed by exact versions/hashes* — no new cache is introduced;
  the working-set digest binds the sorted tracked input inventory; the
  report digest binds the exact bytes. The action cache-key requirements
  are recorded in ADR-0048 §4.
- *no baseline/lock update automatically* — the four report commands are
  read-only over the project (the only permitted write is the granted
  report path); the action allow-list excludes every mutating surface.
- *cancellation and timeouts* — a cancelled row keeps
  `failureClass=infrastructure` and blocks under every policy
  (`security_and_cancellation_rows_are_never_optional`); a hard kill
  produces no report, and a missing report is never success
  (`if-no-files-found: error` on the upload).
- *provider infrastructure failures distinguishable* — the closed
  failure-class vocabulary separates assertion, boot, infrastructure,
  missing-component, incompatible, security, policy, evidence-invalid,
  and output; JUnit maps them to `<failure>` vs `<error>`.
- *matrix support* — per-OS report artifacts with unique names; the
  report records the invocation targets; SARIF consumers must use
  distinct categories per leg (ADR-0048 §4).

## Deviations and explicitly partial scope

- **`--ci-policy <policy>` as a file contract is not in this delivery.**
  The research proposes a digest-bound policy file; here the policy is
  the closed three-level built-in (`default`/`strict`/`lenient` via the
  typed `CiPolicy`, applied through the same evaluator). A file-borne
  policy is a new versioned contract (`ci-policy.schema.vR.json`) and
  follows with the action's policy-reference input.
- **`lekalo report render`/`report bundle` offline commands are not in
  this delivery.** The projections are pure functions of the report and
  are reachable through `--report-format` at production time; the offline
  transformation commands (and the bundle manifest contract) are a
  follow-up that needs no new decision.
- **Gate-observation bridge (`--gate-observation`) is not in this
  delivery.** Script/native observations enter through the typed
  component seam today; the opt-in file bridge needs its contract
  (`ci-gate-observation.schema.vR.json`) and producer-side adoption.
- **`aif-gate-result` stays with the AIFHub Extension.** As the research
  records, the actual Extension schema was not available in this
  checkout; the canonical report is the handoff input and the mapping is
  Extension-owned follow-up work.
- **The public `ichinya/lekalo-action@v1` repository is not published**
  (per dispatch scope): ADR-0048 records the reviewed composite design,
  the input/output contract, and the extraction path; the repository's
  own CI exercises the full surface today.
- **The cache flake in `cache::tests::cold_and_warm…` is pre-existing**
  on this Windows host (reproduced on the stashed baseline at ~1-in-10:
  transient `.lekalo/cache` `structure.directory-unreadable` /
  `structure.path-case` preflight refusals between sibling temp-case
  teardowns). No cache or structure code is touched by this issue; the
  full suite passes deterministically on repeated runs and in CI.
- **Annotation and code-scanning ingestion proofs need real GitHub
  runs** in a dedicated acceptance repository (research A1); the report
  and SARIF surfaces they consume are delivered and pinned here.
