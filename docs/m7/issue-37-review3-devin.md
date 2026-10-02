# Issue #37 — review round 3 (devin)

Independent focused verification of **fix round 2** on branch
`ichinya/m7-issue-37`, HEAD `c480f0ba` (delta over round-2 close-out
`541c11c7`: commits `1076aa02` + `c480f0ba`, touching
`scripts/ai-workspace-hook.mjs`, `scripts/test-ai-workspace-hook.mjs`,
`scripts/test-ai-workspace-contracts.mjs`, `docs/m7/issue-37-fix1.md`,
`docs/m7/issue-37-fix2.md`; zero Rust changes). Review-only: the only
change in this review is this document. Findings verified against
`issue-37-review2-cline.md` (1 major + 1 minor),
`issue-37-review2-devin.md` (concur on the major + 2 minors + 3 nits),
and the dispositions in `issue-37-fix2.md`.

## Verdict: ACCEPT

The converged round-2 major is genuinely fixed and verified live on
this machine: one closed `probeReason` local now feeds both the
private outbox record and the emitted public result, and a real gate
leg exercises the nonzero-probe branch. All claimed minors and nits
verified. No assertion was removed, no vocabulary opened, no
fail-closed path weakened — two paths were strengthened.

## Reproduced live (this machine, not taken as reported)

Pinned Ajv 8.17.1 provisioned at `C:\tmp\m7-review2\ajv\node_modules`.

| Probe (real run) | Observed |
| --- | --- |
| `LEKALO_AJV_NODE_PATH=<ajv> node scripts/test-ai-workspace-hook.mjs` | `{"ok":true,"gate":"ai-workspace-hook","binaryPhases":"skipped","reason":"upstream-binary-not-configured…"}` exit 0 — all dependency-free phases green, **including the three new legs** |
| `NODE_PATH=<ajv> node scripts/test-ai-workspace-contracts.mjs` | `{"ok":true,"gate":"ai-workspace-contracts",…}` exit 0 |
| `--upstream <temp>/exit7.cmd` (`@exit /b 7`), `--send --allow-unadmitted-send` | **stdout**: `{"state":"unavailable","reason":"upstream-probe-nonzero","eventKey":"sha256:8f981ac4…"}` exit 0; **outbox.json**: `[{"state":"planned"},{"state":"unavailable","reason":"upstream-probe-nonzero"}]` — identical reason on both surfaces |
| `--upstream <temp>/definitely-not-here.exe`, same send args | stdout `unavailable`/`upstream-binary-missing`; outbox records the same — the two failure shapes stay distinguishable on the wire |
| `C:\Users\alice\topsecret-positional` (stray positional) | `refused`/`usage`, `detail:"unexpected-positional"`, exit 2 — no `alice` anywhere in output |
| `--frobnicate` (unknown flag) | `detail:"--frobnicate"` — flag names remain echoable CLI surface |
| `--base` with no value | `detail:"--base"` |
| Staged mutation of `docs/target-protocol.md` (`git add`, no commit) | `refused`/`worktree-dirty`, exit 3 — plan mode included |
| Untracked file in repo root | `planned`, exit 0 — documented non-taint of untracked files preserved |
| `check-authority`, `check-privacy`, `check-structure`, `test-fixture-provenance`, `check-contract-versions` | all exit 0 |

Post-run hygiene: `git status --porcelain` empty, `git stash list`
empty, staged-probe bytes restored (the gate's own staged-taint leg
leaves the tree clean too — verified after the gate run).

## Fix-2 delta inspection

- `scripts/ai-workspace-hook.mjs:824-829` — `const probeReason =
  probe.kind === "nonzero" ? "upstream-probe-nonzero" :
  "upstream-binary-missing"` is pushed to `entry.states` and emitted
  verbatim — exactly the reviewers' suggested shape. `runProcess`
  returns only `ok|nonzero|error|throw`, so the emitted `unavailable`
  vocabulary is now the closed pair; the outbox reason also narrowed
  from open-ended `upstream-${code}` to the same pair (more closed,
  not less).
- `:176-190` `worktreeIsClean` now fails closed on `git diff --quiet`
  OR `git diff --cached --quiet` (any code outside 0/1 counts dirty).
  Staged state is now tainting — matching the "committed bytes
  describe the visible state" guarantee; checked at `:718` before
  plan or send.
- `:659-668` `bad()` echoes only `--`-shaped names; everything else
  becomes `unexpected-positional`. All four call sites (`:673`
  missing-value, `:687` unknown arg, `:702`, `:707`) pass flag-shaped
  literals or raw argv — semantics correct.
- Comments scoped as claimed: header `:23-27`, git-inputs `:159`,
  `sendUpstreamEvent` `:503-505`. Dead `save` closure removed
  (`resolveDelivery`).
- `test-ai-workspace-hook.mjs` gained three legs inside always-on
  phase B: nonzero-probe asserting stdout reason AND matching outbox
  reason AND cross-shape distinguishability (`:257-278`), staged-taint
  with control → mutate+stage → `worktree-dirty` → restore+unstage →
  recovered-clean (`:280-302`), stray-positional (`:304-312`). The
  missing-binary assertion at `:252` is unchanged.
- `test-ai-workspace-contracts.mjs` leak walker now pushes primitive
  array entries — strictly stronger; gate green.
- `git diff 541c11c7..HEAD -- scripts/test-*.mjs` removed lines:
  **one** (the walker's `forEach`, replaced by a stricter version).
  Zero removed `assert`/`deepEqual`/`skip`/`only`/`expected` — no
  gate-weakening.
- `issue-37-fix1.md` M7 row now honestly states round 1 fixed the
  behavior but recorded the precise reason only privately — corrected
  in `c480f0ba`.

## Observations (non-blocking nits)

- A probe **timeout** or spawn-throw (`runProcess` kinds `error`/
  `throw`, e.g. ETIMEDOUT) is labeled `upstream-binary-missing` on
  both surfaces — mildly imprecise naming for "present but
  unresponsive", but bounded, closed, and fail-closed; the round-2
  requirement was exactly this two-way distinction.
- A staged **new** file (not just staged modifications) now also
  taints — the old comment's staged-new carve-out was deliberately
  removed. Direction is more conservative, consistent with the
  pre-existing unstaged check which is likewise not scoped to
  approved paths.
- `issue-37-fix2.md` cites commit `4ea3756a`; the landed commit is
  `1076aa02` — same patch, different hash (rebase artifact; `git diff
  4ea3756a 1076aa02 -- scripts/` is empty). Cosmetic only.
- The gate's staged-taint leg writes a real tracked file; `finally`
  restores bytes + unstages, but a hard kill mid-leg could leave the
  index dirty. Bounded risk; the clean-tree control assertion runs
  first, so a pre-dirty index is never mutated.
- The `.cmd`/`.bat` `shell: true` branch itself remains (comments
  scoped — an option cline's round-2 minor explicitly allowed). The
  DEP0190 warning still appears on stderr for `.cmd` upstreams, as
  documented in round 2.

## Honest limits

- Binary phases (`AI_WORKSPACE_BIN`) not executed in this round — the
  pinned-binary build was already verified in round 2 and fix-2
  changed no code on the binary-path legs; the new legs live in the
  dependency-free tier, which ran green here.
- `cargo fmt/clippy/test` not re-run — zero Rust diff in the delta.
