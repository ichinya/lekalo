# Issue #103 — independent review, round 3 (Cline)

**Verdict: ISSUES.** Findings: **1 major, 2 minor**. All three converged
round-2 blockers are **confirmed fixed by adversarial reproduction**, and
every fix-2 gate leg is load-bearing under mutation. The single remaining
finding is a residual instance of the R2-4 class the fix round claimed to
have closed, not a regression of it.

Reviewed `ichinya/m7-issue-103` head `b964faf8` (fix round 2). Merge-base
against `origin/ichinya/M7` is `9510dd07`; the diff is the 19 commits of
the branch. Inputs read in full: `docs/m7/issue-103-review2-cline.md`
(R2 findings, 3 blocker / 3 major / 2 minor),
`docs/m7/issue-103-review2-devin.md` (R2 findings), and
`docs/m7/issue-103-fix2.md` (the fix report and its disposition table).
No implementation file was modified; this document is the only tracked
file added.

---

## 1. Adversarial reproduction of the fixed classes

Every round-2 blocker was re-attacked directly, not read. Probes ran on
disposable copies of `tests/fixtures/loader/valid-direct-visibility` under
the ignored `target/` tree, with the working directory **inside** the
fixture (an early attempt from the repository root produced a spurious
`structure.selection-escape` refusal that contaminated the signal; the
numbers below are all from the clean in-fixture runs).

### R2-1 / F1 — the dead non-empty-file check → handle-verified writes. **CONFIRMED FIXED.**

| Probe | Exit | Detail token | Result |
| --- | --- | --- | --- |
| 35-byte non-protected `out/existing.json` | 4 | `file-exists` | `INTACT=True`, len 35→35, content byte-identical |
| 5500-byte non-protected `out/long.json` | 4 | `file-exists` | `INTACT=True`, len 5500→5500 — **no splice, no residual tail** |
| exactly-empty `out/empty.json` | 0 | — | filled: 1501 bytes, `ready/0` |

The longer-file case is the decisive one: the round-2 defect head-overwrote
from offset 0 and left a ~4000-byte tail. Nothing was written at all, and
the SHA-256 of the file is unchanged before and after.

The mechanism is now genuinely sound rather than merely re-stat'ed:
`validate_destination` (`:261-274`) refuses on no-follow metadata
(`metadata.len() > 0` → `file-exists`), and `write_report` (`:322-353`)
either takes an atomic `create_new` open or opens `read+write` **without
truncation** and re-reads the whole file **on the held handle**, refusing
unless it is still empty at write time. The check and the write share one
handle, which also closes F8's two-open race. Both new negative controls
are present (`a_non_protected_existing_file_is_refused_and_never_touched`,
`an_exactly_empty_pre_created_file_is_filled_with_the_exact_bytes`), and
the old test that passed only via the protected-prefix path now has
non-protected company.

### R2-2 / F6 — `..` traversal, case variants, junction redirection. **CONFIRMED FIXED.**

All bypass spellings refused, each with the precise `protected-path`
token, each writing **nothing** into `lekalo/`:

| Spelling | Exit | Detail | Written? |
| --- | --- | --- | --- |
| `out\..\lekalo\evil-report.json` | 4 | `protected-path` | no |
| `out\..\lekalo\modules\beta\entities.yaml` | 4 | `protected-path` | model bytes intact |
| `LEKALO\case-evil.json` (case-insensitive volume) | 4 | `protected-path` | no |
| `out\..\LEKALO\evil2.json` (traversal **and** case) | 4 | `protected-path` | no |
| `out\..\lekalo\..\lekalo\evil3.json` (traversal round-trip) | 4 | `protected-path` | no |
| `.\lekalo\.\evil4.json` | 4 | `protected-path` | no |
| `out\..\.lekalo\evil5.json` | 4 | `protected-path` | no |
| `out\junc\junk.json` (junction `out\junc` → `lekalo`) | 4 | refused | no |
| `outjunc\k.json` (junction as a direct parent) | 4 | `directory-missing` | no |

Writing the report **onto** an analyzed model file is refused and the model
bytes are provably intact (`modelINTACT=True`). `resolved_destination`
(`:187-211`) canonicalizes the deepest existing ancestor and re-appends
the not-yet-existing tail, and the prefix test runs against the
canonicalized project root — so the on-disk truth, not the spelling,
decides. `is_link_metadata` (`:166-178`) additionally treats Windows
reparse points as links, and both junction legs refuse.

I also probed the case the fix report does not mention: with an
**unresolvable** project root (`project_root: None`, which skips the
protected block at `:246`), a `lekalo/evil-noroot.json` destination is
still refused with `protected-path` — the ancestor walk catches it, so
there is no root-None bypass.

### R2-3 / F2 — zero-row early-failure reports. **CONFIRMED FIXED.**

`verify --locked` on a lock-free fixture now exits **1 and writes the
requested report**:

```
EXIT=1  REPORT EXISTS=True
cmdStatus=invalid/1  evalStatus=invalid/1  verdict=blocked
mode=full  coverage=incomplete  complete=False
  check id=verify.preflight src=fail eff=fail class=evidence-invalid
```

The report carries the real `lock.missing` diagnostic, the synthesized
`verify.preflight` row, and honest provenance (`commit` known,
`workingSetDigest {"reason":"not-applicable","state":"unknown"}`). Stronger
than a JSON round-trip check: **this live-synthesized report passes the
real contract gate unmodified** — copied into the golden directory it was
accepted (`"goldens": 6`, `ok: true`), i.e. the produced artifact is
schema-valid and invariant-clean, not merely well-formed.

`--locked --report-file out\missing-dir\v.json` → exit 1,
`status: invalid`, `reasonCodes: ["ci.report-write-failed","lock.missing"]`
(R2-4 join live). `--changed` refusals record `mode=changed` (R2-8).

### F3 — regenerated goldens with real provenance + a gate that catches the fabricated pin. **CONFIRMED FIXED.**

All five JSON goldens now carry `workingSetDigest {"reason":"not-applicable","state":"unknown"}`;
the fabricated `sha256:e3b0c442…` (`SHA-256("")`) is gone from the tree.
Provenance is honest and self-consistent: commit `4a52eccd` (the pre-fix
HEAD at regeneration) with `dirty: true`.

Reproducibility re-derived independently: a fresh binary run over a
disposable fixture copy is **byte-identical to the committed golden modulo
the commit pin and dirty flag** (1501 bytes, 0 CR bytes, exactly one
trailing LF) — the goldens are real binary output, not hand-authored.

### F4 — SARIF digest bound to a committed golden. **CONFIRMED FIXED.**

`lekaloReportDigest = sha256:fc1adc51…88b2cce` is exactly the SHA-256 of
the newly committed `valid.validate-blocked.golden.json` (a blocked
validate carrying the `LEK-SEM-019` inline diagnostic, `invalid/blocked/1`),
and `lekaloStatus`/`lekaloExitCode`/`lekaloVerdict` agree with that bound
document.

---

## 2. The new gate legs are load-bearing

Each was mutation-checked and restored byte-identically:

| Mutation | Gate result |
| --- | --- |
| Golden `workingSetDigest` → `known` + `SHA-256("")` | **FAIL** — `workingSetDigest pins the fabricated empty-inventory digest` |
| SARIF `lekaloReportDigest` → 64 zeros | **FAIL** — `matches no committed JSON golden` |
| JUnit `skipped="99"` on `<testsuites>` | **FAIL** — `skipped="99" disagrees with the suite sum 5` |

F7's leg is now a real derivation check (per-suite emitted-child counts
plus an aggregate sum), not attribute presence.

---

## 3. Gates re-run in this tree

```
node scripts/test-ci-report-contracts.mjs        (Node 24.13.0)
  {"ok":true,"ajv":"8.17.1","goldens":5,"invalid":9,"verdicts":["blocked","degraded","ready"]}
LEKALO_AJV_NODE_PATH=… npx --package node@18.20.8 node scripts/test-ci-report-contracts.mjs
  {"ok":true,"ajv":"8.17.1","goldens":5,"invalid":9,"verdicts":["blocked","degraded","ready"]}

cargo test -p lekalo-core ci_report --locked      19 passed; 0 failed
cargo test -p lekalo-cli --test ci_report --locked  23 passed; 0 failed
cargo fmt --all -- --check                        clean
cargo clippy -p lekalo-cli -p lekalo-core --all-targets --locked -- -D warnings
                                                 clean
node scripts/check-contract-versions.mjs --base origin/ichinya/M7
  {"ok":true,"product":"0.6.3","contractArtifacts":96}
```

Ajv 8.17.1 plus `ajv-draft-04`/`ajv-formats` were provisioned **outside**
the checkout, exactly as CI does. The Node 18 leg is green — the
`Set.isSubsetOf` incompatibility has not returned.

`git status` is clean before and after; every probe artifact was confined
to the ignored `target/` tree and removed. **No gate was weakened**: the
schema is unchanged from round 1, the adversarial vector count rose 8→9,
the CLI control suite rose 15→23, the core suite 17→19, and no test was
deleted or `#[ignore]`d.

---

## Findings

### F1. [major] The report-write refusal is still silently dropped under the `unsupported` command class

`crates/lekalo-cli/src/report_output.rs:386-399` — the `compose` join
arms cover `Invalid`, `Denied`, `Unavailable`, and `UnsupportedVersion`,
and then fold **every other class into a no-op arm**:

```rust
lekalo_core::result::Status::Valid
| lekalo_core::result::Status::Unsupported => command,
```

`Status::Unsupported` has **exit code 4** (`result.rs:50`), so it is a
failing class, but a write refusal under it is discarded: the caller gets
back an envelope with no `ci.report-write-failed` diagnostic at all. The
second half of the fix is the registry: `ci.report-write-failed`
(`LEK-CI-001`, `contracts/diagnostic-registry.v0.6.3.json`) now allows
`["invalid","denied","unavailable","unsupported-version"]` — `"unsupported"`
is still absent, so even if the match arm were added,
`DiagnosticSet::try_from_unsorted` would fail with `StatusNotAllowed`.

**Reproduced live, end-to-end.** `verify --locked --target net` with no
adapter supply yields a *required* `Unsupported` component
(`verify.rs:392-401`), which routes the degraded verdict to
`DomainResult::UnsupportedOperation` (`verify.rs:522-523`) →
`Status::Unsupported`:

```
PS> target\debug\lekalo.exe --json verify --locked --target net --report-file out\v.json
EXIT=4   status=unsupported
reasonCodes = ["core.capability-unavailable"] x 3   # adapter.net, native.gates, scenarios.execution
# ci.report-write-failed ABSENT; out\v.json was a pre-existing non-empty file
```

The requested CI artifact is **missing** and the run gives no indication
why. Control: with the same command and a fresh destination the report is
written normally (`verdict=blocked`, `unsupported/4`) — so the only
difference is the dropped refusal. A temporary unit probe calling
`compose(DomainResult::unsupported(..), Err(write_failure))` printed
`codes = ["core.capability-unavailable"]` and failed its assertion; the
probe was reverted and the tree is clean.

This is the same defect shape as R2-4 ("the `ci.report-write-failed` join
no-ops for invalid/denied command classes"), which fix 2 correctly fixed
for four classes. `unsupported` is the fifth. The fix report's claim that
the join "appends the refusal for **every** failing class", and
`docs/ci-reports.md:62-63` ("the refusal is observable on **every command
class**"), are therefore overclaims.

Severity is major, not blocker: the run still exits nonzero with a correct
class and correct underlying diagnostics, so no false-success is
introduced and the exit policy is intact — but on a CI consumer that
requested `--report-file`, the artifact is silently absent with zero
diagnostic trace, which is the observability property the round was
specifically about.

**Fix:** add the `Unsupported` arm to the `compose` match and add
`"unsupported"` to `ci.report-write-failed`'s `allowed_statuses` (registry
version stays `v0.6.3` = product `0.6.3`). Add a control that runs
`verify --locked --target <id> --report-file <blocked>` and asserts
`ci.report-write-failed` is present. Either correct the two docs claims or
make them true.

### F2. [minor] `compose` conjoins `Status::Valid` and `Status::Unsupported` into one arm

The offending arm is written as `Valid | Unsupported => command` with the
comment *"The Valid and Unsupported classes carry no report-refusal
composition (a successful or negotiated-unsupported command never reaches
this branch with a failing write)."* The premise is wrong: `Unsupported`
is not the "negotiated-unsupported" (`UnsupportedVersion`, exit 5) class —
it is exit 4 and it is produced by an ordinary degraded verify. Grouping it
with `Valid` is what made the gap easy to miss on review and is why R2-4's
sweep missed it. Even after the F1 fix, keeping an explicit `Unsupported`
arm is worth having so the two genuinely-success-shaped classes are never
conjoined again.

### F3. [minor] The gate pins the golden *set* by name but never re-derives the goldens from the binary

`scripts/test-ci-report-contracts.mjs` verifies each golden's schema,
invariants, canonical bytes, and cross-references, and
`check-contract-versions` confirms artifact versions — but nothing asserts
that a committed golden is still what the fixed binary emits. That is
exactly how F3 happened: the *code* fix landed at `7e32d4c7` and the
goldens kept pinning the fabricated digest through two review rounds,
because only a human diffing prose against JSON would notice. Round 3
re-derived the byte-identity by hand (see §1) and it holds, so this is not
a live defect — but the class remains one regeneration away from
recurring. A cheap golden-to-run comparison (run the binary over a
disposable fixture copy, ignoring the commit pin) in
`crates/lekalo-cli/tests/ci_report.rs` would make it structural; the
existing `validate_deterministic_report_bytes_across_reruns` proves
*run-to-run* stability but never *golden-to-run* agreement.

---

## Round-2 disposition summary

| Finding | Round-2 severity | Round-3 verdict |
| --- | --- | --- |
| R2-1 / F1 — dead non-empty check, head-overwrite + stale tail | blocker | **FIXED** (reproduced: refuses, byte-identical, no splice at 5500 B) |
| R2-2 — `..` / case / junction bypass | blocker | **FIXED** (9 spellings refused, incl. traversal+case combined) |
| R2-3 / F2 — zero-row early-failure reports dropped | blocker | **FIXED** (writes a blocked report that passes the real gate) |
| F3 — fabricated `e3b0c442…` pinned in goldens | blocker | **FIXED** (honest pins; gate leg mutation-tested) |
| R2-4 — write-failed join no-ops for failing classes | major | **PARTIALLY FIXED** — 4 of 5 classes; `unsupported` still drops the refusal (**F1**) |
| R2-5 / F5 — readiness exit authority | major | **FIXED** (required-degraded → exit 4, matching recorded `unavailable/4`; the informational carve-out is documented in one place at `docs/ci-reports.md:81-90`) |
| F4 — SARIF digest unbound | major | **FIXED** (binds `valid.validate-blocked.golden.json`; leg mutation-tested) |
| F6 — junction confinement | major | **FIXED** (reparse points counted as links; both junction legs refuse) |
| R2-6 — gate re-derived blocking from `sourceOutcome` | minor | **FIXED** (`effectiveOutcome` for checks *and* cases, plus the failed-command term) |
| R2-7 — docs overclaims | minor | **PARTIALLY FIXED** — most corrected, but the "every command class" claim survives (**F1**) |
| R2-8 — `mode: "full"` hardcoded | minor | **FIXED** (`--changed` records `changed`) |
| F7 — JUnit count check presence-only | minor | **FIXED** (emitted-children + aggregate; leg mutation-tested) |
| F8 — non-atomic two-open sequence | minor | **FIXED** (shared handle; "atomic" comment corrected) |

**Required before merge:** F1 (add the `Unsupported` arm + the
`"unsupported"` registry status, with a control, and correct the two
overclaiming docs sentences). F2 and F3 are quality items that can follow.

Note on scope: per the round-3 brief, the absence of the #37 files is not a
finding, and the review covered the merge-base diff only.