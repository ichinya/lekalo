# Issue #103 — independent review, round 4 (Codex)

Reviewer: `codex`. Reviewed on 2026-10-03 in
`C:\Users\User\orca\workspaces\lekalo\m7-issue-103`, branch
`ichinya/m7-issue-103`. Scope: fix-round-3 delta
`800d033043d96f8bccce6157bd85cd8efd89bc46..02419e73613ad40f790d173447481b95a224db21`
(7 files, +497/−20). Starting HEAD was
`9069181a3adb2d39152fd264690fcc9d218c854e`; its only change after the
fix commit is `docs/m7/issue-103-review4-devin.md`, so the implementation
tested here is the requested candidate. The local contract-check base
`origin/ichinya/M7` resolves to `87c27d86d3fd2936586f32d95f9d0f9e1bcc7817`.

Read `issue-103-fix3.md`, `issue-103-review3-devin.md`, and
`issue-103-review3-cline.md` first; used `issue-103-review4-devin.md`
for the established report format. Independently inspected the complete
fix delta, ran the requested build and gates, and exercised the rebuilt
`target/debug/lekalo.exe` on disposable fixture copies outside the
checkout. No implementation, fixture, contract, or gate file was edited.

## Verdict: ACCEPT

Findings: **0 blocker, 0 major, 0 minor** in the reviewed delta.
The converged unsupported write-refusal finding and all three minor
items are fixed. Live command exits, JSON envelopes, report contents,
and filesystem effects agree. All requested local gates pass, including
the 26-test CLI report suite and clippy with all workspace targets.
This verdict covers the fix-round-3 delta and local verification.

## Finding disposition

### R3-1 / Cline F1 [major] — unsupported write refusals — FIXED, verified live

`contracts/diagnostic-registry.v0.6.3.json` admits `unsupported` for
`ci.report-write-failed` / `LEK-CI-001`. Its allowed statuses are exactly
`invalid`, `denied`, `unavailable`, `unsupported`, and
`unsupported-version`; `valid` remains excluded.

`report_output::compose` (`crates/lekalo-cli/src/report_output.rs:392`)
joins the diagnostics into the command's own status, then handles both
`Unsupported { capability, .. }` and `UnsupportedOperation { .. }`
explicitly. The former retains its capability; both retain the joined
diagnostic set and exit class.

Independent live reproduction copied
`tests/fixtures/loader/valid-direct-visibility` into a new directory
under the host's temporary directory, set the child's cwd to that
fixture, and ran the actual rebuilt binary:

```text
lekalo.exe lock
  exit 0

lekalo.exe --json verify --locked --target beta
  --report-file missing-dir/v.json --project .
  exit 4; status unsupported; stderr empty
  reasonCodes:
    ci.report-write-failed
    core.capability-unavailable (three underlying refusals)
  missing-dir was not created; no report written

same verify command, --report-file out/existing.json
  exit 4; status unsupported; same reasonCodes
  existing 5500-byte file preserved byte-for-byte
  SHA-256 before and after:
    fad0d9f4568e3c9ab68473a787929126a5aa1423c50af27537294c4db0979e8f

same verify command, fresh --report-file out/v.json
  exit 4; status unsupported; only the three capability reasonCodes
  report written: 3988 bytes, 7 check rows
  commandResult: unsupported/4
  evaluation: unsupported/4/blocked, incomplete
  ci.report-write-failed absent from envelope and report
  model source and lekalo.lock hashes unchanged
```

The in-tree real-binary test
`the_write_refusal_is_visible_on_the_unsupported_class` also passes.
The separately run unit control
`a_write_refusal_joins_the_unsupported_class` verifies the
capability-bearing variant, both diagnostic IDs, and exit 4.

### R3-2 [minor] — changed mode on supply refusals — FIXED

Both supply-block calls to `early_verify_report` now pass `changed`
(`crates/lekalo-cli/src/main.rs:9202,9217`). The root-resolution refusal
arm was inspected; the missing-adapter refusal was exercised live and
by `a_verify_changed_supply_refusal_records_the_changed_mode`.

For the independent live run, a separate outside-tree fixture was
initialized as a local Git repository, its base committed, and a valid
comment-only source edit added. This makes changed-scope resolution
succeed before the adapter-supply refusal:

```text
lekalo.exe --json verify --changed --project .
  --report-file out/v.json -- totally-missing-adapter-xyz
  exit 4; status unavailable
  reasonCodes: [lock.component-unavailable]
  report invocation: command verify, mode changed, locked false
  report evaluation: unavailable/4/blocked, incomplete
  verify.preflight: required true, sourceOutcome not-run,
    effectiveOutcome error, failureClass missing-component
  report diagnostics include lock.component-unavailable
```

The first reviewer harness incorrectly required `effectiveOutcome: fail`;
inspection showed the correct missing-component outcome is `error`.
The corrected complete harness was rerun on fresh copies and exited 0
with all assertions passing. No product change was needed.

### Cline F2 [minor] — shared Valid/Unsupported arm — FIXED

The composition match has an individual arm for all eight concrete
`DomainResult` variants. `Valid` has its own arm. The unsupported
variants carry their diagnostics, and `DeniedWithEvidence` preserves
both `json` and `human` evidence while replacing the diagnostic set.
The misleading shared-arm comment is gone.

### Cline F3 [minor] — goldens never reproduced by the binary — FIXED

Inspected and ran `the_binary_reproduces_every_committed_json_golden`
(`crates/lekalo-cli/tests/ci_report.rs:257`) within the passing 26-test
suite. It checks that its case list equals the on-disk
`valid.*.golden.json` set and reproduces all five cases: validate,
blocked validate, readiness, locked verify, and generate-check.
Fixture preparation plants the blocked reference, locks the required
cases, and warms the readiness cache through validation.

`assert_report_matches_golden` (`:197`) verifies canonical live bytes
with exactly one LF, known Git pin shapes, a hexadecimal live revision,
and a boolean dirty value. It normalizes only the checkout-dependent
Git commit and dirty values, then compares the entire document's bytes.
The working-set pin and all other fields remain in the comparison.
It also binds both Markdown goldens' commit and digest to their JSON
sources.

Independently hashed `valid.verify.golden.json`:
`sha256:1edc923f97cb4e3f3a61bb64dd297391ae87658782cc6ca656d9b7ccedcdf959`.
The corrected `valid.verify.golden.md` contains that exact digest and
the JSON's commit `4a52eccdb9cd478cc1cfd1e199c1e89a0ac97ec3`.

## No-weakening check

The complete delta changes the diagnostic registry, two mode arguments,
composition and its unit coverage, CLI integration coverage, CI report
documentation, the verify Markdown pin/digest, and the fix report.
No schema, gate script, workflow, contract version, dependency manifest,
or lockfile changed. The registry edit adds one failing status to one
diagnostic; it does not permit that diagnostic on success.

No test was removed or ignored. The CLI report suite grows from 23 to
26. The existing determinism test now asserts both runs succeed and
removes the first destination before the second run, so it compares
two actual emissions. Existing preservation, confinement, privacy,
preflight, and readiness controls remain green. `git diff --check`
for the exact fix delta is clean.

## Reviewer-run gates

Windows host, Node `v24.13.0`. Node schema gates used
`NODE_PATH=C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules`.
The SARIF leg used `LEKALO_AJV_NODE_PATH` pointing to the existing
`target/issue-103-r3-deps/node_modules` with `ajv-draft-04` and
`ajv-formats`. No dependency installation was needed.

| Command | Independently observed result |
| --- | --- |
| `cargo build -p lekalo-cli --locked` | PASS, exit 0 |
| `cargo test -p lekalo-cli --test ci_report --locked` | PASS, 26 passed, 0 failed, 0 ignored |
| `node scripts/test-ci-report-contracts.mjs` | PASS, Ajv 8.17.1; 5 goldens, 9 refusal vectors, OASIS SARIF validation; blocked/degraded/ready |
| `node scripts/test-diagnostic-contracts.mjs` | PASS, 459 registry entries, 5 envelopes, 5 diagnostic items |
| `node scripts/test-fixture-provenance.mjs` | PASS, 64 families, all synthetic |
| `node scripts/test-contract-versions.mjs` | PASS, 6 cases |
| `node scripts/check-contract-versions.mjs --base origin/ichinya/M7` | PASS, product 0.6.3, 96 contract artifacts |
| `cargo fmt --all -- --check` | PASS, exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS, exit 0, no diagnostics |
| `cargo test -p lekalo-cli --bin lekalo report_output --locked` | PASS, 4 passed, 0 failed |
| `cargo test -p lekalo-core ci_report --locked` | PASS, 19 passed, 0 failed |
| `git diff --check 800d0330 02419e73` | PASS |

Git was clean before review and after the probes; only this review
document is added for the reviewer commit. Probe evidence is retained
under ignored `target/issue-103-r4-codex/`. Automatic approval review
rejected cleanup of the two explicitly scoped outside-tree probe
directories with `blocked by policy`; those disposable copies remain
outside the checkout. Existing ignored artifacts were preserved.
Only this document is committed; no push is performed.
