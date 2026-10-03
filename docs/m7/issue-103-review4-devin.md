# Issue #103 — independent review, round 4 (Devin)

Reviewer: independent dispatch (Devin), verification of the fix-round-3
delta only. Branch `ichinya/m7-issue-103`, base `origin/ichinya/M7`,
HEAD `02419e73` (fix-3 delta `800d0330..02419e73`: 7 files, +497/−20 —
the six recovered worktree files plus `docs/m7/issue-103-fix3.md`).
Verified against the round-3 reports (`issue-103-review3-devin.md` @
`800d0330`, `issue-103-review3-cline.md` @ `21452e0b`). Live
reproductions used the real binary rebuilt at this HEAD
(`cargo build -p lekalo-cli --locked`, fresh) and pinned Ajv 8.17.1 /
ajv-draft-04 at the repo-convention paths (Node 24.13.0, Windows).
Probe fixture copies live outside the checkout (`C:\Users\User\r4probe\`).

## Verdict: ACCEPT

All four round-3 findings — the converged major plus three minors —
are fixed and verified live. The `unsupported` command class now
carries the report-write refusal exactly like the other failing
classes (registry + composition, both proven end-to-end with the real
binary), `verify --changed` records its requested scope through the
supply-preflight refusals, and the committed goldens are now bound to
binary reproduction by an integration test. No schema, gate, refusal,
destination-confinement, or exit rule was relaxed — the only contract
edit admits `unsupported` (a failing status, exit 4) to one
diagnostic's `allowed_statuses`; `valid` remains excluded.

## Finding disposition

### R3-1 / Cline-F1 [major] — `ci.report-write-failed` silently dropped on the `unsupported` class — FIXED, verified live

- Registry: `contracts/diagnostic-registry.v0.6.3.json` adds
  `unsupported` to `ci.report-write-failed`'s `allowed_statuses`
  (`[invalid, denied, unavailable, unsupported, unsupported-version]`
  — all five failing statuses; `valid` still excluded).
- Composition: `report_output::compose` now matches the concrete
  `DomainResult` variants — `Unsupported` preserves `capability` and
  carries the joined set, `UnsupportedOperation` carries the set,
  `DeniedWithEvidence` keeps its evidence payloads, `Valid` passes
  through. The misleading shared-arm comment is replaced with the
  actual failing-class rule.
- Independent live reproduction (disposable copy of
  `tests/fixtures/loader/valid-direct-visibility`, real binary):
  `lock` → `verify --locked --target beta --report-file
  missing-dir/v.json` exits **4** with `status:"unsupported"` and
  `reasonCodes` containing both `ci.report-write-failed` and
  `core.capability-unavailable` (×3 underlying refusals); `missing-dir`
  is not created. Control: a fresh destination writes the report with
  `unsupported/4/blocked` and **no** `ci.report-write-failed`
  diagnostic — the refusal appears exactly when the write refuses.
- The same class is covered in-tree by the unit control
  `a_write_refusal_joins_the_unsupported_class` (both diagnostic IDs,
  exit 4, capability preserved) and the real-binary
  `the_write_refusal_is_visible_on_the_unsupported_class` (missing-dir
  and 5500-byte existing destinations, byte-identical preservation).

### R3-2 [minor] — `verify --changed` recorded `mode:"full"` on supply-preflight refusals — FIXED

Both `early_verify_report` call sites in `run_verify`
(`crates/lekalo-cli/src/main.rs:9199,9217`) now pass `changed` instead
of a hardcoded `false`. The new test
`a_verify_changed_supply_refusal_records_the_changed_mode` builds a
real hermetic git fixture (committed base + a comment-only edit so the
changed scope resolves), requests `verify --changed` with a missing
adapter, and asserts the written report records
`invocation.mode:"changed"`, a blocked verdict, the
`lock.component-unavailable` supply diagnostic, and a non-pass
`verify.preflight` row — i.e. the run reaches the supply arm, not
scope resolution. Passes.

### Cline-F2 [minor] — `Valid`/`Unsupported` shared no-op arm — FIXED

Verified in the same `compose` diff above: every variant has an
explicit arm; `DeniedWithEvidence`'s `json`/`human` evidence is
retained rather than flattened; the stale comment claiming the arm is
unreachable for those classes is gone.

### Cline-F3 [minor] — gate pinned the golden set by name but never re-derived goldens from the binary — FIXED

`the_binary_reproduces_every_committed_json_golden` reproduces all
five JSON goldens on disposable fixture copies, comparing every byte
after normalizing only the checkout-dependent git `commit`/`dirty`
*values* (shapes and types still checked; the working-set pin and all
other fields stay in the comparison). Its mutation control — a
schema-valid stale model digest — passes the contract gate yet fails
this test, which is exactly the class it exists to catch. The
previously stale `valid.verify.golden.md` pin/digest now binds to the
JSON golden: verified the Markdown contains
`sha256:1edc923f97cb4e3f…cdf959`, the byte-SHA-256 of
`valid.verify.golden.json`, plus the real provenance commit
`4a52eccd…` (the fix-2 commit).

## No-weakening check

Delta scope is exactly the six implementation/fixture files plus the
fix report. Contract delta is the single `allowed_statuses` entry
(failing class only). Tests grow 23 → 26 with no `#[ignore]` and no
relaxed assertion; the determinism test was *strengthened* (it now
removes the first report before the second run — previously the
second run refused the existing destination and compared a file with
itself). `git diff --check 800d0330..HEAD` clean. (The two EOF blank
lines `git diff --check` flags under the wider `origin/ichinya/M7`
range are in earlier committed review docs, not in this delta.)

## Reviewer-run gates (this worktree, Windows, Node 24.13.0)

```text
cargo build -p lekalo-cli --locked                                PASS (fresh)
cargo test -p lekalo-cli --test ci_report --locked                26/26 PASS
  (incl. all three new tests + the strengthened determinism test)
node scripts/test-ci-report-contracts.mjs                       PASS
  {"goldens":5,"invalid":9,"verdicts":["blocked","degraded","ready"]}
node scripts/test-diagnostic-contracts.mjs                      PASS (459 entries)
node scripts/test-fixture-provenance.mjs                        PASS (64 families)
node scripts/test-contract-versions.mjs                         PASS (6 cases)
node scripts/check-contract-versions.mjs --base origin/ichinya/M7 PASS (96 artifacts)
cargo fmt --all -- --check                                        PASS
cargo clippy -p lekalo-cli -p lekalo-core --all-targets
  --locked -- -D warnings                                         PASS
git diff --check 800d0330..HEAD                                   PASS
```

`git status`: clean — the fix commit also removed the previous round's
stray `.fix-*.mjs` probes and the `cb-gate-probe/` leftover is a #75
artifact, not present here; only ignored `target/` artifacts remain
outside the commit.

## Bottom line

Landable. Fix round 3 closes the last `unsupported`-class refusal gap
the R2-4 residual left, records the requested scope honestly on both
supply-refusal paths, and upgrades golden provenance from name-pinning
to binary re-derivation — verified end-to-end with the real binary,
the real gate, and the recovered digest binding.
