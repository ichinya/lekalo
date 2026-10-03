# Issue #103 — fix round 2 (both round-2 reviews)

Round-2 reviewers (devin `issue-103-review2-devin.md`, cline
`issue-103-review2-cline.md`) returned **ISSUES** with converged
blockers. This round fixes every blocker, every major, and every minor.
No gate, test, or schema was weakened; the deterministic exit policy
(blocked ⇒ nonzero) is intact and strengthened; report bytes stay
deterministic (LF-only, one trailing LF, no timestamps/host paths);
`git status` is clean at the fix commit.

## Dispositions

| ID | Severity | Finding | Disposition | Evidence |
| --- | --- | --- | --- | --- |
| R2-1 | blocker | The non-empty-file refusal is dead code; `--report-file` head-overwrites existing files (stale tail) | **fixed** | `validate_destination` no longer reads into a throwaway buffer: the non-empty refusal fires on the no-follow metadata (`report_output.rs`), and `write_report` writes through one of two safe arms — an atomic `create_new` open, or a no-truncate open that **re-reads the whole file on the held handle** and refuses unless it is still empty at write time. No arm truncates or head-overwrites; the check and the write share one handle, which also closes F8's two-open race. New negative controls: `a_non_protected_existing_file_is_refused_and_never_touched` (a 4000-byte non-protected file stays byte-identical, `LEN 4000`, exit 4 `file-exists`) and `an_exactly_empty_pre_created_file_is_filled_with_the_exact_bytes`. The old test passed only via the `protected-path` prefix; it now has non-protected company. |
| F1 | blocker | Same finding (cline numbering) | **fixed** | Same as R2-1. |
| R2-2 | blocker | `..` traversal and case-variant spellings bypass `PROTECTED_PREFIXES` | **fixed** | New `resolved_destination` canonicalizes the deepest existing ancestor and appends the not-yet-existing tail; the protected check runs against the canonicalized project root, so `out/../lekalo/**`, `LEKALO/**` (case-insensitive volume), and junction redirection all resolve to the on-disk truth before the prefix test. Live probes: `out/../lekalo/evil-report.json` → exit 4, nothing written into `lekalo/`; `out/../lekalo/modules/beta/entities.yaml` → exit 4, model bytes intact; `LEKALO/case-evil.json` → exit 4 (Windows). Tests: `traversal_into_a_protected_home_is_refused`, `a_case_variant_protected_spelling_is_refused` (cfg(windows)). |
| R2-3 | blocker (major in devin) | Zero-row early-failure reports are silently dropped (`build::evaluate` vs `CiReport::validate` disagree) | **fixed** | The invariants are aligned on one rule: a failed command is terminal evidence. `model.rs::validate` now requires `blocking rows OR commandResult.exitCode != 0` to equal `verdict == blocked`, so a preflight refusal with zero rows is a valid blocked report instead of an unrepresentable one. Additionally `verify_reported` synthesizes the `verify.preflight` row (classified by the refusal's status) whenever the pipeline reaches the reporter with zero components and a failed result, so the verdict always has a visible row to bind to. Reproductions from both reviews now write the artifact: `verify --locked` (lock-free fixture) → exit 1 **plus** `out/v.json` with `invalid/blocked/1`, mode `full`, `verify.preflight:fail`; the core pin is `a_failed_command_with_zero_rows_is_a_valid_blocked_report` (19/19 core tests). |
| F2 | blocker | `verify` preflight refusals omit the requested report; no `ci.report-write-failed` anywhere | **fixed** | Same alignment as R2-3, plus: the root-resolution and `AdapterSupply` preflight refusals now route through `early_verify_report` (they previously returned without any report), and with the registry widening (R2-4) the join surfaces the refusal on the failing envelope. Live: `verify --locked --report-file missing-dir/v.json` → exit 1, envelope `status: invalid`, `reasonCodes: ["ci.report-write-failed","lock.missing"]`. Tests: `a_verify_preflight_refusal_writes_the_blocked_report`, `a_verify_changed_preflight_refusal_records_the_changed_mode`. |
| F3 | blocker | All four committed goldens still pin the fabricated `workingSetDigest` `sha256:e3b0c442…b855` (`SHA-256("")`) as `known`; the gate cannot catch the class | **fixed** | All goldens were regenerated from the fixed binary over disposable fixture copies under `target/golden/` (procedure below); every provenance leaf is now real — commit `4a52eccd` (the pre-fix HEAD at regeneration), `dirty: true`, and `workingSetDigest {"state":"unknown","reason":"not-applicable"}` (the honest empty-inventory state the fixed code emits). The gate now refuses the class outright: the `EMPTY_SHA256` invariant rejects any `known` working-set pin equal to `SHA-256("")` (mutation-tested: tampering a golden back to the fabricated digest fails the gate with `workingSetDigest pins the fabricated empty-inventory digest`), and the adversarial vector `invalid/fabricated-working-set.json` pins the refusal permanently. |
| R2-4 | major | The `ci.report-write-failed` join no-ops for invalid/denied command classes | **fixed** | The registry entry `ci.report-write-failed` (`LEK-CI-001`, `contracts/diagnostic-registry.v0.6.3.json`) now allows the failure-class statuses the join composes under (`invalid`, `denied`, `unavailable`, `unsupported-version`), so `DiagnosticSet::try_from_unsorted` succeeds and `compose` appends the refusal for every failing class. Live: the R2-3 probe with a missing report directory emits `reasonCodes: ["ci.report-write-failed","lock.missing"]` under `status: invalid`, exit 1 — the command's class stays authoritative and the refusal is observable. Registry version stays `v0.6.3` = product `0.6.3` (`check-contract-versions` ok). |
| R2-5 | major | A required-degraded readiness panel exits 0 while its report records `blocked`/4; `docs/ci-reports.md:62` contradicts `:75` | **fixed** | The gate and the report now read one computation: `build::evaluation_of` (the shared evaluator) is exposed from core, and `run_readiness` derives the `--check` gate from it over the policy-applied doctor rows instead of re-parsing the doctor's own `verdict`. A required-degraded row (`tools.gates`, not-a-repository) now yields exit **4** with the artifact recording `blocked/4` — process exit == recorded evaluation. Test `a_required_degraded_readiness_check_exits_with_the_recorded_evaluation` pins exit == `evaluation.exitCode` over a locked fixture outside any Git work tree; core pin `a_required_degraded_row_blocks_with_the_unavailable_class`. The informational carve-out is now stated in one place: `docs/ci-reports.md` scopes the "exits with the evaluation" claim to gated runs and names the informational readiness exception explicitly (the artifact records the evaluation the gated run would exit with; the informational process exit stays 0 by contract). |
| F5 | major | Informational readiness exits 0 beside a report claiming exit 4 | **fixed** | Same as R2-5 (docs + single exit authority). The report evaluation is unchanged (it truthfully records the gated outcome); the doc no longer contradicts itself and the artifact/exit pair is now documented rather than accidental. |
| F4 | major | The SARIF golden's `lekaloReportDigest` binds a report not in the repository; the gate checks only the `sha256:` prefix | **fixed** | The SARIF golden is regenerated from the same real run as a new committed JSON golden, `valid.validate-blocked.golden.json` (a blocked validate with the `LEK-SEM-019` inline diagnostic, `invalid/blocked/1`), and the gate now hashes every committed `valid.*.json` golden and requires `lekaloReportDigest` to equal `sha256:` + the bytes of one of them, with `lekaloStatus`/`lekaloExitCode`/`lekaloVerdict` agreeing with the bound document (mutation-tested: an arbitrary 64-hex digest fails with `lekaloReportDigest … matches no committed JSON golden`). Verified binding at generation: both spellings `sha256:fc1adc51…88b2cce`. |
| F6 | major | Confinement reduces to deepest-existing-ancestor-is-not-a-symlink; Windows junctions report `is_symlink()==false`; no linked-parent control | **fixed** | Two layers: `parent_chain_has_links` now treats Windows reparse points as links (`FILE_ATTRIBUTE_REPARSE_POINT` via `file_attributes`, alongside `is_symlink()`), and the protected-home check runs on the canonicalized resolution, so even a link that slips the ancestor walk cannot land the write in a protected home. Live Windows probe: a junction `link → out` refused with exit 4, nothing written through it. New control `a_linked_parent_directory_is_refused` (symlink on Unix, `symlink_dir` on Windows, skipped only where the OS declines link creation). |
| R2-6 | minor | The JS gate re-derives suite blocking from `sourceOutcome + required` instead of `effectiveOutcome` | **fixed** | The gate's `hasBlocking` reads `effectiveOutcome` for checks **and** suite cases (the single evaluated decision `model.rs` serializes), plus the failed-command evidence term. No false refusal is possible under `--ci-policy strict`. |
| R2-7 | minor | `docs/ci-reports.md` overclaims (repository-relative SARIF paths; "not silently swallowed"; "exits with the evaluation"; privacy never-enters claim) | **fixed** | The confinement paragraph now states the real rule (new or exactly-empty regular file, traversal/case/link refusal after resolution, protected homes); the join behavior replaces "not silently swallowed" (now true on every class); the exit claim is scoped to gated runs with the informational carve-out named; the SARIF bullet describes project-relative paths (monorepo caveat) and defines `lekaloReportDigest` as the exact emitted-JSON digest; the privacy paragraph documents the sink-side `scan_rendered` pass as defense in depth. The `report_output.rs` module doc was rewritten to match the implemented confinement. |
| R2-8 | minor | `verify` early-failure reports hardcode `mode: "full"` | **fixed** | `early_verify_report` takes the requested scope (`changed`) and records `changed` for `--changed` refusals; the root/supply refusals route through it with `full`. Tests: `a_verify_changed_preflight_refusal_records_the_changed_mode` asserts `invocation.mode == "changed"`. |
| F7 | minor | The JUnit gate count check is presence-only (tampered `skipped="99"` passes) | **fixed** | The gate now extracts each `<testsuite>` body, counts the emitted `<testcase>`, `<failure`, `<error`, and `<skipped` children, and refuses any attribute/child disagreement — plus an aggregate check that `<testsuites>` equals the suite sum (mutation-tested: `skipped="99" disagrees with 4 emitted children`, `skipped="5" disagrees with the suite sum 100`). |
| F8 | minor | The pre-created empty file is filled via a non-atomic two-open sequence | **fixed** | Closed by the R2-1 rewrite: the fallback arm opens `read+write` without truncation, re-reads the whole file **on the held handle**, and writes only if it is still exactly empty; a file that gained content between validation and open is refused (`file-exists`/`write-denied`), never overwritten. The misleading "atomic" comment is gone; the module doc describes both arms honestly. |

## Golden regeneration procedure (reproducible)

All goldens are real binary runs over disposable copies of
`tests/fixtures/loader/valid-direct-visibility` under `target/golden/`
(untracked), invoked from the repository root so the Git pins resolve to
the repository:

1. `plain` — untouched copy: `validate` (ready JSON golden + Markdown
   summary golden), `readiness --phase release` (blocked informational
   JSON golden).
2. `locked` — copy + `lekalo lock` (digest `bbe9c51d…`, identical to the
   round-1 goldens' pin): `verify --locked` (degraded JSON golden + the
   JUnit suite golden), `generate --check` (ready JSON golden).
3. `blocked` — copy with one planted unknown type reference
   (`type: alpha.ghost` in `lekalo/modules/beta/entities.yaml`):
   `validate` → the blocked JSON golden `valid.validate-blocked.golden.json`
   and, from the same run, the SARIF golden (digest binding verified
   equal at generation).

The `workingSetDigest` of every golden is the honest
`unknown/not-applicable` (the copies carry an empty *tracked* inventory),
never a fabricated digest. Byte stability re-verified (two fresh runs,
1501 bytes, byte-identical, LF-only, exactly one trailing LF).

## Gates (real outputs, this tree)

```
node scripts/test-ci-report-contracts.mjs          (Node 24.13.0)
  {"ok":true,"checked":"ci-report-contracts-v1","ajv":"8.17.1",
   "goldens":5,"invalid":9,"verdicts":["blocked","degraded","ready"]}

LEKALO_AJV_NODE_PATH=… npx --yes --package node@18.20.8 node \
  scripts/test-ci-report-contracts.mjs             (Node 18.20.8)
  {"ok":true,… "goldens":5,"invalid":9,"verdicts":["blocked","degraded","ready"]}

cargo test -p lekalo-core ci_report
  test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 957 filtered out

cargo test -p lekalo-cli --test ci_report
  test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured

cargo fmt --all -- --check
  clean

cargo clippy -p lekalo-cli -p lekalo-core --all-targets --locked -- -D warnings
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 24.93s

node scripts/check-contract-versions.mjs --base origin/ichinya/M7
  {"ok":true,"product":"0.6.3","contractArtifacts":96,"base":"origin/ichinya/M7"}
```

Mutation checks (all refused, then restored byte-identical): fabricated
working-set digest in a golden; unbound SARIF digest; tampered JUnit
`skipped="99"`.

## Exit policy and scope

- The deterministic exit policy is intact: blocked evaluations exit
  nonzero under every gate (`readiness --check` included — now also for
  required-degraded panels, which is strictly more coherent).
- The schema (`ci-report.schema.v0.6.3.json`) is unchanged; the only
  contract edit is the `ci.report-write-failed` registry entry's
  `allowed_statuses` widening, which makes the refusal *possible* on
  failing command classes (the reviewers' requested observability), not
  any refusal weaker.
- Round-1 dispositions stand; nothing they fixed regressed (the full
  round-1 negative controls still pass: 8 prior vectors + 1 new).
