# Issue #103 — independent review, round 2 (Cline)

**Verdict: ISSUES.** Findings: **3 blocker, 3 major, 2 minor**.

Reviewed `ichinya/m7-issue-103` head `7379b716` (fix round 1 = `cc1e3334` +
`7e32d4c7`, reports `8fd59323` / `7379b716`), diff base `origin/ichinya/M7`.
Inputs read in full: `docs/m7/issue-103-review-devin.md` (1 blocker, 6 major,
9 minor), `issue-103-review-codex.md` (4 blocker, 10 major, 2 minor),
`issue-103-review-cline.md` (4 blocker, 8 major, 5 minor), and the
`issue-103-fix1.md` disposition table. No implementation file was modified;
this document is the only tracked file added.

## Disposition of round-1 blockers and majors

Most of round 1 landed correctly and is confirmed fixed by reproduction, not
by assertion:

| Finding | Round-1 claim | Round-2 verdict |
| --- | --- | --- |
| D1/Cline1/C4 — `REPORTS` never exported; upload glob | fixed | **CONFIRMED FIXED.** `ci.yml:191-192` sets step-level `env: REPORTS:`; `:227-234` lists six explicit finalized filenames with `if-no-files-found: error`. No `/*` glob remains. |
| Cline2/C3 — Node-18 `Set.isSubsetOf` crash | fixed | **CONFIRMED FIXED.** `test-ci-report-contracts.mjs:123-124` uses a portable subset check. Re-ran the gate on **Node 24.13.0 —†’ ok** and **Node 18.20.8 —†’ ok**. |
| Cline3/D13 — SARIF `information_uri`; empty `rules`/`results` | fixed | **CONFIRMED FIXED.** `sarif.rs:58` renames to `informationUri`; the committed golden carries one `LEK-SEM-019` rule and one located result under `%SRCROOT%`; `:266-280` refuses an empty rules/results golden and compiles the pinned OASIS schema. |
| Cline11/D15 — trailing slash on `automationDetails.id` | fixed | **CONFIRMED FIXED.** `sarif.rs:311-315`; golden reads `lekalo/validate/blocked`. |
| CL5/C2 — secret scan asserted, not implemented | fixed (report sinks) | **CONFIRMED FIXED for the report sinks.** `build.rs:480-488` `scan_rendered` runs the #119 scanner on the rendered bytes; `report_output.rs:241-243` runs it for **every** format before the write. Reproduced: a planted `alpha.ghp_…` canary in a type reference is refused — `out/v.json` absent, exit preserved. |
| C9/D4 — readiness hand-patch; two exit authorities | fixed | **CONFIRMED FIXED.** The hand-patch is gone; `readiness --phase release --check` on a blocked project —†’ exit 4 with report `blocked/4`, `complete:false`, `coverage:incomplete` (coherent). |
| C8/D7/C15 — JUnit `skipped` counts passing cases | fixed | **CONFIRMED FIXED in the renderer.** A live `verify --locked` JUnit reports `skipped="5"` over 7 testcases with exactly 5 emitted `<skipped/>` elements (was `tests - failures - errors`). |
| C10 — lock binds the request digest | fixed | **CONFIRMED FIXED.** `provenance.rs:96` uses `lock.digest()`; live run binds `sha256:bbe9c51d…`, identical to the verify receipt's `lockDigest`. |
| C12 — schema admits nested payloads / wrong leaf types | fixed | **CONFIRMED FIXED.** Adversarial probe: `git.commit.value=false`, `lock.digest.value="0.6.3"`, nested secret objects, and open `causes`/`fixes`/`relatedLocations` are all refused by `contracts/ci-report.schema.v0.6.3.json`. |
| C14/D11 — thread-local evidence seam | fixed | **CONFIRMED FIXED.** `verify.rs:138-166` clears and *takes* the handoff on both the `Ok` and `Err` arms. |
| C13/D8 — unbounded `git ls-files` | fixed | **CONFIRMED FIXED.** `report_git.rs:57-116` spawns, polls `try_wait`, kills+reaps at the deadline, drains incrementally under a 1 MiB cap; `:117-149` bounds content reads with an 8 MiB aggregate budget. |
| Cline10 — `workingSetDigest` = SHA-256("") as `known` | fixed | **PARTIALLY FIXED — see F3 (blocker).** The *code* is correct (`report_git.rs:154-156` returns `None` on an empty inventory; `provenance.rs:75` maps a commit-without-digest to `unknown/not-applicable`). Live probe over a `git init` fixture: `workingSetDigest {"state":"unknown","reason":"not-applicable"}`. **But all four committed goldens still carry the fabricated `sha256:e3b0c442…b855`.** |
| CL7/D12 — `strict`/`lenient` unreachable; cases ignore policy | fixed | **CONFIRMED FIXED.** `--ci-policy` exists on all four reported commands; the shared `CiPolicyTable` (`build_policy.rs`) drives check rows, case rows, the evaluation, and JUnit. |

Focused gates re-run locally, all green: `cargo fmt --all -- --check`;
`cargo clippy -p lekalo-cli -p lekalo-core --all-targets --locked -- -D warnings`;
`cargo test -p lekalo-core --lib ci_report` (17 passed);
`cargo test -p lekalo-cli --test ci_report` (15 passed);
`node scripts/test-ci-report-contracts.mjs` on Node 24.13.0 and Node 18.20.8
(`ok: true`, 4 goldens, 8 vectors);
`node scripts/check-contract-versions.mjs --base origin/ichinya/M7`
(`{"ok":true,"product":"0.6.3","contractArtifacts":96}`);
determinism (two consecutive `validate` reports byte-identical, 1502 bytes,
0 CR bytes, exactly one trailing LF). `git status` clean before and after
this review. **No gate was weakened or skipped** — the schema got stricter,
the refusal vectors still cover the same surface, and no test was deleted or
`#[ignore]`d.

The regressions below are what round 1 introduced or left behind.

---

## Findings

### F1. [blocker] The "refuse an existing non-empty file" control is dead code: any existing file is still overwritten in place

`crates/lekalo-cli/src/report_output.rs:212-215` — the belt-and-braces
non-empty check reads the file into a **throwaway** buffer and then tests a
different, always-empty `Vec`:

```rust
let empty = std::fs::File::open(path)
    .and_then(|mut file| file.read_to_end(&mut Vec::new()).map(|_| Vec::new()))
    .map(|bytes: Vec<u8>| bytes.is_empty())
    .unwrap_or(false);
```

`read_to_end(&mut Vec::new())` fills a temporary that is dropped, `.map(|_| Vec::new())`
then hands back a **freshly constructed empty** `Vec`, and `.is_empty()` is
therefore **always `true`**. The `Err(write_failure("file-exists"))` branch is
unreachable, so `validate_destination` returns `Ok(())` for *every* existing
regular file regardless of size.

`write_report` (`:266-286`) then takes its second arm —
`Err(_) if path.is_file()` — open `.write(true).truncate(false)` — which
**overwrites from offset 0 without truncating**. The pre-existing bytes are
destroyed up to the report length, and any surplus tail survives.

The claim in the fix report (D2/C1: "refuses existing non-empty files, and
the writer opens `create_new` … never a truncating create") is therefore not
what ships. The only real protection is the `PROTECTED_PREFIXES` list
(`lekalo/`, `.lekalo/`, `apps/`, `:128`), which is checked only when
`project_root` resolves *and* the destination strips to that prefix.

Reproduced on a disposable fixture copy of
`tests/fixtures/loader/valid-direct-visibility`:

```
# an ordinary pre-existing, non-protected file
PS> 'PREEXISTING-MUST-SURVIVE-1234567890' | Set-Content target\r2i\out\existing.json
PS> target\debug\lekalo.exe --json validate --project . --report-file out\existing.json
{"status":"valid", ... }                      exit 0
INTACT=False
CONTENT: {"checks":[{"detail":"","diagnosticIndexes":[],...

# a longer pre-existing file: head replaced, tail retained
PS> Set-Content target\r2g\tail.json ('A' * 4000)
PS> target\debug\lekalo.exe validate --project target\r2f\fx --report-file target\r2g\tail.json
LEN=4000   HEAD={"checks":[{"detail":"structure.selection-escape"...   countA=2432
```

The second case is the more damaging shape: the file is not even a valid
report — it is a report spliced onto 2,432 leftover bytes, so a consumer that
trusts the destination's existence reads corrupt content.

The CLI test cited as the negative control,
`a_report_destination_over_an_existing_file_is_refused`
(`crates/lekalo-cli/tests/ci_report.rs:559-584`), does pass — but only because
it names `lekalo/modules/beta/entities.yaml`, which the **prefix list**
catches. It never exercises the non-empty-file branch, which is why the dead
code is invisible to CI. The research requirement
(`docs/m7/issue-103-research.md:228`, reject destinations overlapping
source/model/locks/baselines/history) and AC4's "no writes" are still unmet
for any path outside the three prefixes.

Fix: read into a real buffer (`let mut buf = Vec::new(); file.read_to_end(&mut buf)`)
and test `buf.is_empty()`, or simply drop the second arm and let
`validate_destination` be the single authority. Add a negative control that
names a **non-protected** pre-existing file, plus a longer-file case asserting
no residual tail.

### F2. [blocker] `verify` preflight refusals still omit the requested report entirely

The fix report claims (C8): "generate `--check --locked` preflight and verify
preflights route through the report finalizers". `generate` is fixed —
`main.rs:9046-9051` routes the `locked_check` refusal into
`generate_check_reported` and the report lands. **`verify` is not.**

`run_verify` (`main.rs:9123-9227`) only calls `early_verify_report` for the
`--changed` / `AdapterSupply` preflights (`:9142`, `:9148`, `:9160`). A
`Prepared::prepare` / `compile` failure inside the core pipeline
(`orchestration/verify.rs:323-332`) is returned as `Err(result)` from
`run()`, surfaced by `verify_with_components` as `Verified { result, .. }`,
and then — because `request.is_requested()` is true — `verify_reported` *is*
reached. But `verify_reported` builds a report whose `checks`/`suites` are
**empty** (no components were assembled), and `CiReport::validate`
(`model.rs`, *"verdict disagrees with the blocking rows"*) rejects it: the
`Evaluation` says `blocked` while there is no `Fail`/`Error` row to justify
it. The invariant branch returns
`compose(result, Err(report_write_failed("invariant")))`, so **no file is
ever created**.

Reproduced:

```
PS> target\debug\lekalo.exe --json verify --locked --project . --report-file out\v.json
{ "status": "invalid", "reasonCodes": [ "lock.missing" ] }    exit 1
REPORT=False
```

Two distinct problems, both live:

1. The requested artifact is missing with **no** `ci.report-write-failed` on
   the status-owned stream — the exact "silently dropped" shape D10 claimed
   to fix. The invariant refusal is a *developer-fault* path being reached by
   ordinary user input, and it swallows the fact.
2. A zero-row blocked report is structurally impossible to express, so the
   `blocked`-with-no-rows case (the review's own C8 reproduction shape) can
   never be written.

Fix: give the preflight path a synthesized terminal check row (e.g. the
`lock.missing` / loader refusal as a required `Fail` row) so the verdict has
a row to bind to, and surface `ci.report-write-failed` on stderr when the
invariant branch fires. Add the round-1 reproduction as a test: fresh fixture
+ `verify --locked --report-file out/verify.json` must produce the file with
`evaluation.status=invalid`, `verdict=blocked`, and a blocking row.

### F3. [blocker] The committed goldens still pin the fabricated `workingSetDigest` the fix claims to have removed

The fix report (CL10) states: *"the fabricated `e3b0c442…` value is gone from
the goldens."* It is not gone. All four committed goldens still carry

```json
"workingSetDigest":{"state":"known","value":"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}
```

`e3b0c442…b855` is exactly `SHA-256("")` (recomputed on this host to
confirm), i.e. the digest of an **empty** inventory. Present in:

- `tests/fixtures/ci-report/valid.validate.golden.json`
- `tests/fixtures/ci-report/valid.generate-check.golden.json`
- `tests/fixtures/ci-report/valid.readiness.golden.json`
- `tests/fixtures/ci-report/valid.verify.golden.json`

`git show 7e32d4c7 --stat` confirms the commit that fixed the *code*
(`report_git.rs`, `provenance.rs`) **did not touch any golden**. So the
repository now pins, as a released artifact, exactly the value the fix round
declared fabricated — and the gate ratifies it, because
`test-ci-report-contracts.mjs` checks canonical byte form, schema validity,
and invariants, but never that a `known` digest corresponds to a non-empty
inventory.

The code path is genuinely fixed (see the table), so this is a stale-golden
regression rather than a live code defect — but it is a **false statement in
the fix report** and a pinned artifact that mislabels an empty inventory as a
known pin. AC6's "binds the sorted tracked input inventory" is still
contradicted by the shipped evidence.

Fix: regenerate all four goldens from the fixed binary, and add a gate
invariant rejecting `workingSetDigest.state == "known"` when the value equals
`sha256:` + SHA-256(""). Also note all four goldens pin commit `27f7b957…`, a
*review-document* commit rather than the implementation head, with
`dirty:true` — consider whether shipping them is meaningful evidence at all.

### F4. [major] The SARIF golden's `lekaloReportDigest` binds a report that is not in the repository, and the gate checks only the `sha256:` prefix

`tests/fixtures/ci-report/valid.sarif.golden.sarif` declares

```json
"lekaloReportDigest":"sha256:bdc202fb30afdd7fc99cf89bda2ff2d98aa3e0d2716e975ebc1112347ea1a425"
```

with `"lekaloStatus":"invalid","lekaloExitCode":1,"lekaloVerdict":"blocked"`.
The SHA-256 of every committed JSON golden:

```
valid.generate-check.golden.json = sha256:39f86277…
valid.readiness.golden.json      = sha256:2aa1943c…
valid.validate.golden.json       = sha256:858958d2…
valid.verify.golden.json         = sha256:0d6f8b4f…
```

None is `bdc202fb…`. The SARIF golden is a hand-assembled artifact whose
digest binding cannot be checked against anything in the tree, and the gate
(`test-ci-report-contracts.mjs:250-252`) only asserts
`lekaloReportDigest.startsWith("sha256:")` — a prefix test any 64-hex string
passes. Round 1's own acceptance for this field was "SARIF
`lekaloReportDigest` verified to match `sha256` of the committed JSON golden"
(devin, deterministic-bytes row); the regenerated golden broke that property
while the gate stayed silent.

The golden *does* now correctly exercise the inline-diagnostic path (CL3 is
fixed), so this is a binding-integrity gap, not a shape gap.

Fix: either regenerate the SARIF golden from a real binary run so the digest
equals a committed report, or pin the source report as a fifth fixture and
assert the equality in the gate.

### F5. [major] Informational `readiness` still exits 0 while its report self-describes `blocked`/exit 4

Round 1's D4/C9 removed the post-build hand-patch and unified the builder as
the single authority — good. But the surviving `compose` contract still lets
the process exit and the report disagree:

```
PS> target\debug\lekalo.exe --json readiness --phase release --project . --report-file out\rdy2.json
INFO_EXIT=0
verdict=blocked exit=4
```

`docs/ci-reports.md:62-63` says "The producing command exits with the
**evaluation**, never blindly with the legacy envelope", and `:32` calls
`evaluation` "the authoritative gated outcome". Lines 75-78 then carve out an
exception for informational readiness ("exits 0 whenever produced"). So the
behavior is documented — but the document now contradicts itself three lines
earlier, and a CI consumer that reads the uploaded report sees `exitCode: 4`
on a run the pipeline recorded as success. That is precisely the "single
artifact self-describing two outcomes" defect D4 was raised against; it moved
from the readiness path into the `compose` contract.

This is narrower than round 1's version (the report is now internally
coherent and the exception is written down), which is why it is major rather
than blocker.

Fix: make the informational case explicit in the report itself — e.g. an
`evaluation.gateApplied: false` or a distinct informational verdict — so an
exit-0 run cannot ship an artifact claiming exit 4, or narrow the
`docs/ci-reports.md:62` claim to name the exception.

### F6. [major] Destination confinement reduces to "the deepest existing ancestor is not a symlink"; no linked-parent negative control exists

`parent_chain_has_links` (`report_output.rs:133-150`) walks `path.components()`
and returns `true` on the first existing symlink component, but returns
**`false`** as soon as a component does not exist (`:139-141`, "missing parts
cannot link"). Combined with `validate_destination`'s `Err(_) => Ok(())` at
`:223` (an absent target is always admissible), the confinement reduces
entirely to: "the deepest existing ancestor of the destination must not be a
symlink".

On Windows, `std::fs::symlink_metadata` on a **junction/reparse point**
reports `is_symlink() == false`, so a junctioned output directory is admitted
and the write lands wherever the junction points. The repository's own
research (line 228) and the fix report both claim link-free confinement; the
implementation is narrower than the claim, and the claim is what reviewers
were asked to verify.

Not reproduced end-to-end (creating a junction is host-specific setup I did
not run), so this is reported as a **coverage gap with a concrete mechanism**
rather than a proven exploit. Add a `no-follow` negative control on a
junctioned/symlinked parent per OS leg — codex F1 asked for exactly this
("Add source/lock overlap and linked-parent negative controls") and the fix
report claims the requirement is met, but no such test exists in
`crates/lekalo-cli/tests/ci_report.rs`.

### F7. [minor] The JUnit gate's count check is not load-bearing

The gate now pins a real JUnit golden and checks prolog, balanced tags,
control characters, and that the four count attributes are non-negative
(`test-ci-report-contracts.mjs:292-320`). But the count check is a pure
presence test:

```js
if (tests < 0 || failures < 0 || errors < 0 || skipped < 0) fail("junit:counts", …)
```

Nothing compares the attributes against the emitted children. Tampering the
committed golden to claim `skipped="99"` over 7 testcases with 4 `<skipped/>`
elements still yields `{"ok": true}` from the gate — verified by mutating the
file, running the gate (exit 0), and restoring the file. The gate's own
comment claims "counts derive from the emitted children"; they do not.

Fix: count `<testcase>`, `<failure>`, `<error>`, and `<skipped/>` elements
per suite and assert equality with the declared attributes.

### F8. [minor] The report destination fills a pre-created empty file through a non-atomic two-open sequence

`write_report` (`:266-286`) tries `create_new` first and, on failure, falls
back to opening the existing file `.write(true).truncate(false)`. Validation
admits an exactly-empty pre-created file (`:207-208`), so the fallback is the
intended path for a `touch`-created placeholder — but the sequence is not
atomic: between `symlink_metadata` (validation) and the `OpenOptions` open, a
concurrent writer can place content that is then overwritten from offset 0
with no tail truncation. The comment at `:249-251` calls the create path "an
atomic fail-if-exists open", which is true of the first arm and misleading
about the second.

Low severity (single-user CLI, small window), but the fix report advertises
atomic publication and this is the residual non-atomic seam.

---

## Re-verified round-1 items that need no further work

- **REPORTS export + upload paths** — fixed, no glob remains.
- **Node 18 contract gate** — fixed and re-run green on Node 18.20.8.
- **SARIF `informationUri` / inline-diagnostic golden / OASIS schema validation** — fixed; the gate compiles the real schema.
- **Secret canary at the sink** — fixed and reproduced (report absent, exit preserved).
- **Readiness `--check` coherence on the gate path** — exit 4, report `blocked/4`, `complete:false`.
- **JUnit skipped counting in the renderer** — fixed (5 emitted, 5 counted).
- **Lock payload digest** — fixed and equal to the verify receipt.
- **Schema nested-payload / leaf-type closure (C12)** — fixed; adversarial probe refused.
- **Evidence-seam custody (C14/D11)** — fixed.
- **Bounded git working set (C13/D8)** — fixed.
- **`--ci-policy` reachability + shared policy table (CL7/D12/C7)** — fixed.
- **Scenario assertion-level rows (D6/C11)** — fixed at the core seam.
- **Deterministic bytes** — verified: byte-identical reruns, LF-only, one trailing LF, no timestamps/host paths.
- **Contract version consistency** — `{"ok":true,"product":"0.6.3","contractArtifacts":96}`.
- **`git status`** — clean at review time; all probe artifacts confined to the ignored `target/` tree and removed.

## Required next work before merge

1. **F1** — make the existing-non-empty-file refusal real (or remove the
   unreachable branch and re-derive the claim); add a non-protected-path
   negative control and a long-file no-residual-tail assertion.
2. **F2** — make `verify` preflight refusals finalize the requested report
   with a synthesized blocking row, and surface `ci.report-write-failed` when
   the invariant branch fires.
3. **F3** — regenerate all four goldens from the fixed binary so the
   fabricated `e3b0c442…` working-set digest is actually gone, and add a gate
   invariant that refuses it.
4. **F4** — bind the SARIF golden's `lekaloReportDigest` to a committed
   report (or pin the source report) and assert the equality in the gate.
5. **F5** — make the informational-readiness exit exception visible in the
   report itself, and reconcile `docs/ci-reports.md:62` with `:75`.
6. **F6/F7/F8** — add the linked-parent negative control per OS leg, make the
   JUnit count check compare against emitted children, and close the
   two-open non-atomic seam.

F1, F2, and F3 are merge-blocking: F1 restores the exact "report side channel
destroys analyzed project state" defect that three reviewers raised as a
blocker, F2 leaves a requested CI artifact silently missing on a routine
preflight, and F3 ships a pinned artifact that the fix report explicitly
claims no longer exists.

