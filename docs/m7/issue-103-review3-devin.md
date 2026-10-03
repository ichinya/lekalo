# Issue #103 — independent review, round 3 (Devin)

**Verdict: ISSUES** — 1 major, 1 minor. Both are narrow residuals of
round-2 findings, not new defect classes; the converged blockers are
verified fixed by live adversarial reproduction.

Reviewed `ichinya/m7-issue-103` at head `b964faf8` (fix round 2) against
`origin/ichinya/M7`. Method: re-read both round-2 reports
(`issue-103-review2-devin.md`, `issue-103-review2-cline.md`) and
`issue-103-fix2.md`; inspected the full fix-2 delta; re-ran every cheap
gate (contract gate green on **Node 24.13.0 and Node 18.20.8** — 5
goldens, 9 refusal vectors, OASIS SARIF validation; `cargo test -p
lekalo-core ci_report` 19/19; `cargo test -p lekalo-cli --test
ci_report` 23/23; `cargo fmt --all -- --check` clean; `cargo clippy -p
lekalo-cli -p lekalo-core --all-targets --locked -- -D warnings` clean;
`check-contract-versions.mjs --base origin/ichinya/M7` ok, product
0.6.3, 96 artifacts); ran live adversarial probes on disposable fixture
copies; and mutation-tested two of the new gate legs (both caught the
tampering, both restored byte-identical). `git status` clean; no
implementation file touched by this review.

## Verified fixed (live reproduction, not reading)

| ID | Check | Evidence |
| --- | --- | --- |
| R2-1/F1 | `--report-file` on a non-protected 4000-byte existing file | exit 4, `ci.report-write-failed`/`file-exists`; file byte-identical after (`LEN 4000`, all `A`) — no head-overwrite, no stale tail. `--report-file ./lekalo.lock` on a locked fixture: exit 4, the 737-byte lock survives as a lock. New controls `a_non_protected_existing_file_is_refused_and_never_touched`, `an_exactly_empty_pre_created_file_is_filled_with_the_exact_bytes` pass. |
| R2-2 | `out/../lekalo/evil.json`, `out/../lekalo/modules/beta/entities.yaml`, `LEKALO/case.json` | all exit 4; nothing lands inside `lekalo/`; model bytes intact. `resolved_destination` canonicalizes the deepest existing ancestor before the prefix test. |
| R2-3/F2 | `verify --locked` on a lock-free fixture | exit 1 **and** `out/v.json` written: `evaluation invalid/blocked/1`, `invocation.mode "full"`, synthesized `verify.preflight:fail` row binding the verdict. `verify --changed` refusal records `mode "changed"`. Model invariant now `blocking rows OR commandResult.exitCode != 0`, aligned with `evaluate`. |
| R2-4 | `verify --locked --report-file missing-dir/v.json` | exit 1, invalid envelope carries `reasonCodes ["ci.report-write-failed","lock.missing"]` — join works on `invalid`. **Residual on `unsupported` — see R3-1.** |
| R2-5/F5 | required-degraded readiness (locked fixture outside any Git work tree → required `tools.gates` degraded/`error`) | `readiness --check` exits **4**, artifact records `blocked`/`unavailable`/`4` — process exit == recorded evaluation via shared `evaluation_of`. Informational run exits 0 beside the same blocked/4 artifact (`commandResult valid/0`) — the documented carve-out, stated once in `docs/ci-reports.md`. |
| F3 | goldens' `workingSetDigest` | all five JSON goldens carry the honest `{"state":"unknown","reason":"not-applicable"}`; commit pin is real (`4a52eccd`, `dirty:true`). Mutation check: restoring `sha256:e3b0c442…b855` as `known` fails the gate (`workingSetDigest pins the fabricated empty-inventory digest`); `invalid/fabricated-working-set.json` pins the refusal. |
| F4 | SARIF `lekaloReportDigest` | `fc1adc51…88b2cce` equals the byte-SHA-256 of committed `valid.validate-blocked.golden.json`; `lekaloStatus/lekaloExitCode/lekaloVerdict` agree with the bound document (`invalid/1/blocked`). Mutation check: an arbitrary 64-hex digest fails `sarif:binding`. |
| R2-6 | gate blocking-row rule | `hasBlocking` now reads `effectiveOutcome` for checks **and** suite cases plus `commandResult.exitCode !== 0` — same rule `model.rs` validates; strict-policy suite blockers can no longer be mis-refused. |
| F7 | JUnit count check | gate counts emitted `<testcase>/<failure>/<error>/<skipped>` children per suite and the `<testsuites>` aggregate against declared attributes — tampered `skipped="99"` is now refused (fix-report mutation evidence; code verified). |
| F6 | junction confinement | junction `link → out`: `link/r.json` → exit 4. Junction `jlek → lekalo`: `jlek/r.json` → exit 4, nothing in `lekalo/`. Traversed spellings `out/../jlek/evil.json` and `out/../link/sub/r.json` → exit 4 (`path-invalid`): `parent_chain_has_links` pushes `..` into the probe prefix so the OS resolves the true spelling, and reparse points count via `FILE_ATTRIBUTE_REPARSE_POINT`. |
| R2-7 | docs | `docs/ci-reports.md` now states the real confinement rule, scopes "exits with the evaluation" to gated runs with the informational carve-out named, describes project-relative SARIF paths, and documents the sink-side `scan_rendered` pass. The module doc in `report_output.rs` was rewritten to match. **One claim remains falsified — see R3-1.** |
| R2-8 | early-failure mode | changed-block arms pass the real scope; `verify --changed` refusal records `changed`. **Residual in the supply arm — see R3-2.** |
| F8 | two-open race | closed: the fallback arm opens `read+write`/`truncate(false)` and re-reads the whole file **on the held handle**, writing only if still empty. |

Determinism re-verified: two consecutive `validate` reports
byte-identical (1502 bytes), LF-only, exactly one trailing LF.

## Findings

### R3-1 [major] `ci.report-write-failed` is still silently dropped on the `unsupported` command class (R2-4 residual)

The round-2 fix widened `LEK-CI-001`'s `allowed_statuses` to `invalid`,
`denied`, `unavailable`, `unsupported-version` — but not `unsupported`.
`Status::Unsupported` is reachable on the reportable path:
`orchestration/verify.rs:523` returns `DomainResult::UnsupportedOperation`
when the verify verdict is `Degraded` (degraded/unsupported components).

Reproduced live on a locked fixture:

```
$ lekalo --json verify --target beta --project . --report-file missing-dir/v.json
{ "status": "unsupported",
  "reasonCodes": ["core.capability-unavailable",
                  "core.capability-unavailable",
                  "core.capability-unavailable"] }   exit 4
```

No `ci.report-write-failed` anywhere, no artifact — the refusal fact is
invisible, exactly the D10/R2-4 shape. Mechanism at
`crates/lekalo-cli/src/report_output.rs:382-400`: `compose` joins the
write refusal via `DiagnosticSet::try_from_unsorted(joined,
Status::Unsupported)`, which fails `StatusNotAllowed` (the registry does
not admit `ci.report-write-failed` under `unsupported`); the `if let
Ok(set)` falls through, and even had it succeeded the
`Status::Unsupported => command` arm would discard the joined set. The
comment's claim that "a negotiated-unsupported command never reaches
this branch with a failing write" is falsified.

Consequence chain is double-broken: fix needs (a) `unsupported` added to
`LEK-CI-001`'s `allowed_statuses` in
`contracts/diagnostic-registry.v0.6.3.json`, and (b) a real
`Status::Unsupported` arm in `compose` (the joined set must be carried —
`DomainResult::UnsupportedOperation { diagnostics: set }`). The
`docs/ci-reports.md` claim "the refusal is observable on every command
class" is false until then.

Positive control verified: the same `--target beta` run against a valid
destination writes a coherent blocked report (`unsupported`/4, 7 check
rows) — the defect is specifically the silent join, not over-refusal.

### R3-2 [minor] `verify --changed` records `invocation.mode: "full"` when the adapter-supply preflight refuses (R2-8 residual)

`run_verify` resolves the `--changed` scope first, then the adapter
supply. Both supply-block arms pass a hardcoded `changed: false` to
`early_verify_report` (`crates/lekalo-cli/src/main.rs:9199-9202` for the
root-resolution arm, `:9211-9219` for the `AdapterSupply::new` refusal).

Reproduced live on a git fixture with a valid change:

```
$ lekalo --json verify --changed --project . --report-file out/v.json \
         -- totally-missing-adapter-xyz
{ "status": "unavailable", "reasonCodes": ["lock.component-unavailable"] }  exit 4
out/v.json → invocation.mode "full"   (verdict blocked, coherent)
```

The report is written and internally coherent — only `invocation.mode`
misrecords the requested scope, the same class R2-8 flagged on the other
arms. Fix: thread the `changed` flag through the supply block
(`early_verify_report(&selection, result, locked, changed, …)`).

## Gate results (this tree)

| Gate | Result |
| --- | --- |
| `node scripts/test-ci-report-contracts.mjs` (ajv 8.17.1) | ok on Node 24.13.0 **and** Node 18.20.8: 5 goldens, 9 invalid vectors refused, OASIS SARIF validation |
| `cargo test -p lekalo-core ci_report` | 19/19 |
| `cargo test -p lekalo-cli --test ci_report --locked` | 23/23 |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy -p lekalo-cli -p lekalo-core --all-targets --locked -- -D warnings` | clean |
| `node scripts/check-contract-versions.mjs --base origin/ichinya/M7` | ok (product 0.6.3, 96 contract artifacts) |
| Mutation: fabricated `workingSetDigest` in a golden | gate fails (`workingSetDigest pins the fabricated empty-inventory digest`), restored |
| Mutation: arbitrary SARIF `lekaloReportDigest` | gate fails (`sarif:binding … matches no committed JSON golden`), restored |
| Destination confinement live probes | pass: existing-file/`lekalo.lock` refused intact; `..` traversal, case-variant, and junction spellings (incl. `out/../` re-entry) all refused |
| Early-failure report probes | pass: `verify --locked` and `verify --changed` write coherent blocked reports with `verify.preflight`/changed mode |
| `readiness --check` required-degraded | exit 4 == recorded `evaluation.exitCode` |
| `git status` | clean (probes lived under ignored `target/` and `$TEMP`) |

## Bottom line

Fix round 2 lands the converged blockers correctly: the destination
confinement is real now (existing files, `..`, case variants, junctions,
and traversal-into-protected all refused with the artifact intact), the
zero-row early-failure report finalizes with a synthesized preflight
row, goldens carry honest provenance with a gate that catches
fabrication, the SARIF digest binds a committed report, and the
readiness gate shares one exit authority. Two residuals keep this from
an accept: the write-refusal join still silently drops on the
`unsupported` class (R3-1 — fix is one registry status plus one compose
arm), and `verify --changed` + adapter-supply refusal still records
`full` (R3-2 — thread the flag through the supply block). Neither needs
a redesign; a small follow-up commit closes them.
