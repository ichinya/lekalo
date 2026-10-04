# Issue #105 — Independent review round 2 (devin)

Reviewer: devin (re-verification of fix round 1)
Branch: `ichinya/m7-issue-105` @ `10a13440` (worktree `m7-issue-105`)
Prior verdicts: devin ACCEPT r1 (`6d8c5bea`); codex ISSUES r1
(`6af53b6c`, findings R1-1 + R1-2)
Fix under review: `ae7f3b04` (docs corrections), `cfd15799` (gate
hardening + replay), `10a13440` (fix evidence record)

## Verdict

**ACCEPT** — both codex findings resolved with enforced controls;
zero crates/contracts delta.

## Finding dispositions

| # | Codex finding | Disposition | Independent evidence |
|---|---------------|-------------|----------------------|
| R1-1 | Eight of thirteen required P0 pages absent from owner
metadata; gate did not check P0 mapping | **FIXED** |
`docs/documentation-owners.json` gains a `p0Pages` array — verified
programmatically: all 13 required pages mapped exactly once, 0 missing,
0 duplicates. `scripts/lib/docs-maintenance.mjs::validatePageOwners`
requires every P0 page exactly once (`missing P0 page owner`,
`duplicate P0 page owner`, `unknown P0 page`) and cross-checks each
page's prose `Owner:` line against the machine map (`P0 prose owner
drift`). `test-docs-ownership.mjs` exercises 33 refusal controls
(remove-each-page, duplicate-each-page, unknown-member probes).
Both lanes pass: `{"ok":true,"p0Owners":13,"pageOwnerControls":33}`
live-help and static. |
| R1-2 | Greenfield tutorial step C selected the wrong fixture
(`contracted/planner-slice`, whose `contract check` exits 1 invalid)
while the linked adoption sequence uses `docs/contracted-module` |
**FIXED** | `docs/tutorial-greenfield-planner.md` restructured: step C
explicitly works from `tests/fixtures/docs/contracted-module` (full
update→tests→check(exit 1, coverage-missing)→attach→check(exit 0)
sequence); step D documents the switch to a separate
`contracted/planner-slice` copy and honestly lists its limitations
(unimplemented `planner.count_focused`, absent support artifact, no
`native.test.mjs`). `tests/fixtures/docs/examples.json` mirrors the
linked sequence: `contract-planner` gained the exit-1
`contract check --module planner` step; `inspect-planner` replays
after it in its own fixture. Portable gate re-run:
`{"ok":true,"examples":9,"commands":25,"controls":21,
"sourcePreserved":true}` — the new sequence actually replays. |

## Re-verification record

- `git diff 6af53b6c..HEAD -- crates contracts` → empty (docs/test only,
  as required).
- `git diff --check` 0 issues; worktree clean.
- `test-docs-ownership.mjs` and `--static` → `ok:true`, 318 surfaces,
  13 required docs, 13 p0Owners, 33 pageOwnerControls.
- `test-docs-examples.mjs` (portable lane) → ok, includes corrected
  `contract-planner` and `inspect-planner` cases.

## Notes

- The fix chose the right direction: the machine map is the authority
  and prose `Owner:` lines are validated against it — drift in either
  direction now fails the gate.
- The tutorial's honest treatment of the original corpus's deliberate
  limitations is preserved and made explicit per step.

## Recommendation

Merge-ready pending codex re-review of the fix (round 2), per M7
convention.
