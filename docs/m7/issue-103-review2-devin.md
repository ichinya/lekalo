# Issue #103 — independent review, round 2 (Devin)

**Verdict: ISSUES.**

Reviewed `ichinya/m7-issue-103` at head `7379b716` against
`origin/ichinya/M7`, covering fix commits `cc1e3334` (round 1) and
`7e32d4c7` (Cline round). Method: re-read all three round-1 reports
(`issue-103-review-devin.md`, `issue-103-review-codex.md`,
`issue-103-review-cline.md`) plus `issue-103-fix1.md`; re-inspected every
changed surface; re-ran all cheap gates (contract gate green on **Node 24
and Node 18** — 4 goldens, 8 refusal vectors, OASIS SARIF schema
validation; `cargo test -p lekalo-core ci_report` 17/17; `cargo test -p
lekalo-cli --test ci_report` 15/15; `cargo fmt --check` clean;
`cargo clippy -p lekalo-cli -p lekalo-core --all-targets --locked --
-D warnings` clean; `check-contract-versions.mjs --base
origin/ichinya/M7` ok); and ran live adversarial probes for destination
confinement, the secret canary, all three policy levels, preflight
reporting, and exit/report coherence on real fixture copies under
`target/` (ignored). The working tree is clean; no implementation code
was changed by this review.

Most of the round-1 fix landed correctly — the workflow smoke step,
Node-18 gate, lock-free provenance, lock payload digest, canonical JSON,
SARIF/schema fixes, JUnit counts, per-assertion scenario rows, verify
scope custody, `--ci-policy`, the working-set deadline, and the secret
sink scan are all verified working (disposition table below). But two
blocker-class confinement defects are still live (the original "report
can overwrite analyzed inputs" bug is still reproducible in a subtler
form), and the early-failure report path still drops the artifact *and*
the refusal signal.

## Findings

### R2-1 [blocker] The non-empty-file refusal is dead code — `--report-file` still head-overwrites existing files

`validate_destination` (`crates/lekalo-cli/src/report_output.rs:212-215`)
intends to refuse an existing non-empty file, but reads it with:

```rust
std::fs::File::open(path)
    .and_then(|mut file| file.read_to_end(&mut Vec::new()).map(|_| Vec::new()))
    .map(|bytes: Vec<u8>| bytes.is_empty())
    .unwrap_or(false);
```

`read_to_end` returns `usize`; the `.map(|_| Vec::new())` discards the
count and yields a fresh **empty** `Vec`, so `empty` is `true` for any
readable file — the `file-exists` refusal never fires. The writer then
hits `create_new` failing (`report_output.rs:266-275`), falls into the
`Err(_) if path.is_file()` branch, opens `truncate(false)` and
`write_all` — a **head-overwrite** that also leaves stale tail bytes when
the previous file was longer, producing a corrupted artifact.

Reproduced on the pushed head (`7379b716`):

- `lekalo validate --report-file ./lekalo.lock` → **exit 0**; the
  737-byte lock file was replaced by 1559 bytes of report JSON — the
  exact class of input destruction round-1 finding 2 flagged.
- Re-running `readiness --report-file out/readiness.json` over an
  existing report produced a file that is the new 3495-byte document
  followed by 133 stale bytes from the prior (larger) file — invalid
  JSON, and "deterministic bytes" is violated in place.
- The committed test `a_report_destination_over_an_existing_file_is_`
  `refused` passes because it points at `lekalo/modules/...` — refused
  earlier by the `protected-path` check — and never reaches the buggy
  non-empty branch. The fix report's evidence does not cover the
  regression it claims to fix.

Also confinement-by-accident: a bare filename (`--report-file
report.json`) is refused `directory-missing` because `Path::parent` is
`""` and `symlink_metadata("")` fails (`report_output.rs:162-168`) —
undocumented behavior.

### R2-2 [blocker] `..` traversal and case-variant spellings bypass `PROTECTED_PREFIXES`

`validate_destination` (`report_output.rs:181-197`) computes
`cwd.join(path)` and runs `strip_prefix(root)` on the **unnormalized**
spelling, then does a case-sensitive `starts_with` on the result. The
`..` components survive `strip_prefix`, so a path that escapes and
re-enters a protected home never matches the prefix list — and no
canonicalization happens anywhere before the write.

Reproduced live on Windows:

- `--report-file out/../lekalo/evil-report.json` → **exit 0**, file
  created inside `lekalo/`. The next `validate` then trips
  `structure.canonical-unexpected-entry` — a report write poisons the
  project's own invariant.
- `--report-file out/../lekalo/modules/beta/entities.yaml` → **exit 0**,
  the model source itself overwritten with report bytes (combined with
  R2-1). The research line-228 requirement — reject destinations
  overlapping source/model/locks/history — is still not actually
  enforced.
- `--report-file LEKALO/case-evil.json` → **exit 0**, file lands in
  `lekalo/` on the case-insensitive volume: the prefix check is
  case-sensitive while the filesystem is not.

Round-1 findings D2/C1/CL4 are therefore **not fixed**: the fix covers
only the literal, non-traversed spelling of a protected path.

### R2-3 [major] Zero-row early-failure reports are still silently dropped — the evaluation/validation invariants disagree

`build::evaluate` marks any failed command as blocking
(`crates/lekalo-core/src/ci_report/build.rs:209-265`:
`command_failed = exit_code != 0` → `verdict = Blocked`), while
`CiReport::validate` (`model.rs:720-732`) requires a `Fail|Error`
effective-outcome **row** whenever the verdict is `Blocked`. A preflight
refusal that produces zero rows — `verify --locked` on a missing lock,
`verify --changed` with no changed inputs — builds `verdict=blocked` +
zero rows → `report.validate()` fails → `compose(result,
Err(unavailable))` → **no file is ever written**.

Reproduced on the pushed head:

- `verify --locked --report-file out/v.json` on a lock-free fixture →
  exit 1, `out/` empty, envelope contains only `lock.missing`.
- `verify --locked --changed` → exit 1, no report, only
  `impact.changed-input-invalid`.
- `generate --check --locked` on the same fixture *does* write a
  coherent blocked report — but only because that path synthesizes a
  failure check row. Validate also survives because it synthesizes
  `model.validation` (`main.rs:2625-2646`). Verify's and readiness's
  early-failure routes (`early_verify_report` → `verify_reported`,
  `main.rs:9234-9261`; readiness's `compose` at `main.rs:5120-5124`)
  emit zero rows and hit the invariant wall.

Fix-report claim "always finalize requested reports … preflight
refusals route through the report finalizers" is implemented in routing
but vetoed by the invariant — exactly the C8 class remains
artifact-silent. And because of R2-4 the failure is invisible.

### R2-4 [major] The write-failure join no-ops for every non-unavailable command class

`compose` (`report_output.rs:313-334`) joins
`ci.report-write-failed` diagnostics into the command's set via
`DiagnosticSet::try_from_unsorted(joined, command.status())`. But the
registry entry for `ci.report-write-failed` allows only `unavailable`
status — for `invalid`/`denied` commands the rebuild fails
`StatusNotAllowed`, the `if let Ok(set)` falls through, and the command
is returned **unchanged**: the report refusal disappears without trace.

Reproduced: both R2-3 probes emit envelopes listing only the command's
own diagnostics — no `ci.report-write-failed` anywhere; the secret-canary
run below shows the same signature. The join works only when the command
was already `unavailable` — a minority class. D10 is still effectively
unfixed for the dominant classes, and `docs/ci-reports.md`'s "the report
failure is not silently swallowed" remains false.

### R2-5 [major] D4 residual — a required-degraded panel still diverges: `--check` exits 0 while the report says blocked/4

The gate now builds the report from `gated_result` and validates it
(`main.rs:5100-5127`) — the hand-patch is gone, and the blocked-panel
path is coherent in both modes. But the two authorities still disagree
on **degraded** rows: the gate fires only on doctor `verdict ==
"blocked"` (`main.rs:5033`), and the doctor derives `blocked` only from
required `Blocked|Unknown` rows — a required `Degraded` row leaves the
verdict `degraded`. Meanwhile the report evaluator maps required
`Degraded` → `Error` → `evaluation: {verdict: blocked, exitCode: 4}`.

Reproduced live: a valid locked fixture copied outside a Git work tree
degrades `tools.gates` (required, `not-a-repository`) with nothing
blocked → `readiness --check` exited **0** while the emitted artifact
records `evaluation.verdict = blocked`, `exitCode = 4`. A stale-lock
panel reaches the same split through required-degraded
`lock.freshness`/`artifacts.drift` rows. The schema describes
`evaluation` as the outcome the CLI "must exit with" and the code
comment at `main.rs:5103-5105` claims report verdict and process exit
"can never disagree inside one artifact" — both remain falsifiable.

### R2-6 [minor] JS gate's suite-side blocking-row invariant re-derives instead of reading `effectiveOutcome`

`scripts/test-ci-report-contracts.mjs` (~lines 97-107) checks the
`verdict==blocked ⇔ blocking row` invariant by re-computing blocking
from `sourceOutcome + required` for suite cases, while checks use
`effectiveOutcome` — the same rule `model.rs` uses for both. Under
`--ci-policy strict` an optional-absent suite case promotes to `error`
(effective) while its source stays `unsupported`/`not-run`: a valid
strict report with a suite-only blocker would be refused by the gate.
Latent today — the live strict verify still had a check-row blocker —
but it will produce false refusals as soon as a strict/degraded suite
report is pinned.

### R2-7 [minor] `docs/ci-reports.md` still overclaims

- Still says report paths are "repository-relative … under the
  `%SRCROOT%` base" with no project-relative/monorepo caveat — the C6
  scoped limitation the fix report claims was documented is absent.
- Still states "the report failure is not silently swallowed" (false for
  invalid/denied commands, R2-4) and "the producing command exits with
  the evaluation" (false, R2-5).
- The privacy paragraph still claims "secret material never enters a
  report because it never enters a diagnostic" — the canary probe proved
  secrets *do* reach the rendered bytes and are stopped only by the new
  sink-side `scan_rendered`; the doc doesn't mention the scan.
- `report_output.rs` doc comments (lines 12-16, 152-159) describe
  confinement that R2-1/R2-2 disprove.

### R2-8 [minor] `verify` early-failure reports misrecord the invocation as `mode: "full"`

`early_verify_report` hardcodes `scope_mode = "full"`
(`main.rs:9251`) regardless of `--changed`/`--target`. Latent today
because those reports are dropped anyway (R2-3), but the D5 fix only
covers the successful path; a failed `verify --changed` would still be
recorded as a full run.

## Round-1 disposition table

| Finding (round 1) | Status | Evidence |
| --- | --- | --- |
| CI `REPORTS` never exported (D1/C1/CL1) | **FIXED** | `ci.yml:191-192` sets `env: REPORTS` at step scope; explicit filenames in upload (227-233) + `if-no-files-found: error`. |
| Report destination overwrites inputs (D2/C1/CL4) | **NOT FIXED** | R2-1 (dead non-empty check + head-overwrite), R2-2 (`..` + case bypass). Reproduced live. |
| Lock-free provenance `unknown/invalid` (D3/C10) | **FIXED** | `provenance_block` reads model/IR pins via loader+ir directly; verified in regenerated goldens. |
| Readiness exit/report divergence (D4) | **PARTIAL** | Blocked-panel path coherent + `report.validate()` added (`main.rs:5106-5127`); degraded-only panels still diverge (R2-5). |
| Verify scope hardcoded (D5) | **FIXED** | Mode/targets/modules captured before the move; `--module beta` verified live in report. Early path still `full` (R2-8). |
| Scenario evidence rollup-only (D6) | **FIXED** | Per-assertion `CaseDraft`s with scenario/step/ordinal identity (`main.rs:9290-9330`); golden exercises rows. |
| JUnit skipped counts (D7/C15/CL8) | **FIXED** | Counts emitted `<skipped>` elements only; aggregate on `testsuites`; golden shows `skipped="4"` = 4 elements. |
| Node-18 `isSubsetOf` gate failure (D8/C3/CL2) | **FIXED** | Manual subset loop; gate run green on real Node 18 and Node 24. |
| Readiness collapse to unavailable + `check:"readiness"` (D9) | **NOT FIXED (minor)** | `main.rs:5042-5047` unchanged: single `unavailable` class, literal `"readiness"` as the check id. |
| Write-failure silently dropped (D10) | **PARTIAL** | Join exists but no-ops on non-unavailable classes (R2-4). |
| Thread-local evidence leak (D11) | **FIXED** | `verify_with_components` clears at entry and `take()`s per run. |
| Strict/lenient unreachable; cases ignore policy (D12/C7/CL7) | **FIXED** | `--ci-policy` wired through all four commands; strict→`error`/`blocked` and lenient→`skip` verified live. |
| SARIF `information_uri` (D13) | **FIXED** | `informationUri` emitted; real rule + located result in golden; clean `automationDetails.id`. |
| JUnit gate no-op + no SARIF schema (D14/C14) | **FIXED** | Gate asserts JUnit counts/structure for real and validates SARIF against the pinned OASIS schema (`sarif-schema-2.1.0.json` vendored). |
| Docs inconsistencies (D15) | **PARTIAL** | Impl map/versioning wording corrected; `docs/ci-reports.md` still overclaims (R2-7). |
| Secret scanning/publication admission absent (C11) | **FIXED** | `scan_rendered` runs the #119 scanner over rendered bytes before every sink; canary → `secrets.detected` refusal, artifact absent, exit preserved. `privacy/redact.rs` hardened against UTF-8 boundary panic. Caveat: refusal signal lost on invalid commands (R2-4); status-envelope echo is the declared C2 limitation. |
| Lock digest = request digest (C9) | **FIXED** | Canonical payload digest; `bbe9c5…` in golden matches `lekalo lock` output. |
| Empty working set reported `sha256("")` (CL10) | **FIXED** | Typed `KnownValue`; empty inventory → `unknown`/`not-applicable`. |
| Unbounded `git ls-files`/reads (D8/C13) | **FIXED** | spawn + `try_wait` poll + kill + reap at the deadline; output + 8 MiB content caps. |
| Canonical JSON not recursively sorted (C16) | **FIXED** | `sort_json_keys` byte-sorts recursively; gate asserts canonical bytes. |
| Schema allowed generic diagnostics (C12) | **FIXED** | Closed typed provenance states, required `effectiveOutcome` on checks and cases, closed diagnostic data grammar; 8 invalid vectors refused in the gate. |
| Report-write status absent from registry | **FIXED** | `ci.report-write-failed` registered; but join constraint causes R2-4. |

No regressions found in the fix diff: no skipped tests, no relaxed
schema clauses (strictly closed further), no removed assertions — the
changed assertions are canonical-order adaptations. `redact.rs` gained
UTF-8-boundary hardening, appropriate since rendered report bytes now
flow through the scanner.

## Verification log

| Gate | Result |
| --- | --- |
| `node scripts/test-ci-report-contracts.mjs` (ajv 8.17.1 provisioned) | ok on Node 24 **and** Node 18: 4 goldens, 8 invalid vectors refused, OASIS SARIF validation |
| `cargo test -p lekalo-core ci_report` | 17/17 |
| `cargo test -p lekalo-cli --test ci_report --locked` | 15/15 |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy -p lekalo-cli -p lekalo-core --all-targets --locked -- -D warnings` | clean |
| `node scripts/check-contract-versions.mjs --base origin/ichinya/M7` | ok (product 0.6.3, 96 contract artifacts) |
| Determinism (LF, no timestamps/abs paths, byte-stable reruns) | ok on fresh writes; **violated in place** on overwrite (stale tail, R2-1) |
| Secret canary `ghp_…` in a diagnostic | artifact refused (`secrets.detected`); refusal code absent from envelope (R2-4); token still echoed in status-owned envelope (declared C2 limitation) |
| `--ci-policy strict`/`lenient` live | strict promotes optional absence → `error`/`blocked`/4; lenient → `skip` |
| Destination confinement live probes | **fail**: `./lekalo.lock` overwritten; `out/../lekalo/**` written; `LEKALO/**` bypass (R2-1, R2-2) |
| Early-failure report probes | **fail**: `verify --locked` (missing lock) and `verify --changed` produce no artifact, no refusal signal (R2-3, R2-4) |
| `generate --check --locked` blocked report | coherent blocked report written |
| Readiness degraded-only panel | **divergent**: `--check` exits 0, report says `blocked`/4 (R2-5) |
| `git status` | clean (only ignored `target/` probe artifacts) |

## Bottom line

The round-1 fix is real and broad — 16 of 22 tracked items verified
fixed — but it cannot be accepted yet: the destination-confinement story
is still broken in two live ways (R2-1, R2-2; both reproduce the
original input-destruction blocker), the "always finalize requested
reports + never silently drop the refusal" claims fail on the
zero-row/non-unavailable paths (R2-3, R2-4), and the readiness
evaluation can still disagree with the process exit inside one artifact
(R2-5). R2-1..R2-5 need a fix round; R2-6..R2-8 can follow up, though
R2-7's doc corrections are cheap and should ride along.
