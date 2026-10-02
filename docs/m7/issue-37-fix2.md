# Issue #37 — fix round 2 report

Narrow fix for the round-2 reviews
([`issue-37-review2-cline.md`](issue-37-review2-cline.md) — 1 major,
1 minor; [`issue-37-review2-devin.md`](issue-37-review2-devin.md) —
concur on the major, plus 2 new minors and 3 nits). Every round-2
finding is dispositioned below. No gate, test, contract, or schema was
weakened; the gate gained three new legs.

## Findings and dispositions

| ID | Finding | Disposition | Evidence |
| --- | --- | --- | --- |
| M7 (major, cline; confirmed by devin) | emitted `unavailable` reason collapses every probe failure into `upstream-binary-missing` while the outbox records the precise reason; no gate leg exercises the nonzero path; fix1's M7 evidence described the wrong surface | **fixed** (`4ea3756a`) | one closed `probeReason` local now feeds BOTH the outbox record and the emitted public result: `upstream-probe-nonzero` (nonzero exit) vs `upstream-binary-missing` (spawn failure). Live reproduction of the reviewers' scenario — an exit-7 upstream — now yields `{"state":"unavailable","reason":"upstream-probe-nonzero"}` on stdout with the identical reason in `outbox.json`, while the missing-binary case still emits `upstream-binary-missing`. Gate legs: the nonzero branch asserts the precise reason on stdout AND the matching outbox state, the missing-binary leg keeps its assertion, and a new assertion requires the two shapes to stay distinguishable on the wire |
| minor (devin) | `source.clean` misses staged (index) modifications to approved paths | **fixed** (`4ea3756a`) | `worktreeIsClean` now also runs `git diff --cached --quiet`; a staged edit to a tracked file refuses `worktree-dirty`. Gate leg: stages a mutation of `docs/target-protocol.md`, asserts the refusal, restores the bytes, unstages, and asserts the clean state recovers |
| minor (devin) | usage `detail` reflects arbitrary non-flag tokens (stray positional echoed verbatim) | **fixed** (`4ea3756a`) | only known-shape flag names (starting `--`) are echoed; anything else becomes the bounded marker `unexpected-positional`. Gate leg: `alice-topsecret-positional` → exit 2, `detail: "unexpected-positional"`, no `alice` in output |
| minor (cline) | "never a shell" comments sit next to the `shell: true` `.cmd`/`.bat` branch | **fixed** (`4ea3756a`) | comments scoped: the header, the git-inputs section, and `sendUpstreamEvent` now state that only a Windows `.cmd`/`.bat` upstream path takes the shell branch in `runProcess` (per its extension), and that git inputs are always shell-free |
| nit (devin) | dead `save` closure in `resolveDelivery` | **fixed** (`4ea3756a`) | removed |
| nit (devin) | contracts-gate leak walker skips primitive array entries | **fixed** (`4ea3756a`) | the walker now probes primitive array entries too; gate green |

`docs/m7/issue-37-fix1.md` was corrected: its M7 row now states plainly
that round 1 fixed the behavior (no send proceeds) but recorded the
precise reason only in the private outbox, and that fix round 2 put the
precise closed code on the public surface with gate legs for both
shapes.

## Verification outputs (real runs, this machine)

```
$ git status --porcelain   → (empty); git diff --quiet → 0; git diff --cached --quiet → 0

$ LEKALO_AJV_NODE_PATH=<ajv-8.17.1> node scripts/test-ai-workspace-hook.mjs
{"ok":true,"gate":"ai-workspace-hook","binaryPhases":"skipped",
 "reason":"upstream-binary-not-configured (set AI_WORKSPACE_BIN to run the full proof)"}

$ AI_WORKSPACE_BIN=<pinned 8fdf818 build> LEKALO_AJV_NODE_PATH=<ajv-8.17.1> \
  node scripts/test-ai-workspace-hook.mjs
{"ok":true,"gate":"ai-workspace-hook","binaryPhases":"executed","upstream":"configured",
 "hookStates":["delivered","no-change","planned","refused","unavailable","unknown-delivery"]}

$ node scripts/ai-workspace-hook.mjs … --upstream <exit-7 fake> …   (cline's scenario)
  stdout: {"state":"unavailable","reason":"upstream-probe-nonzero", …}
  outbox: {"state":"unavailable","reason":"upstream-probe-nonzero"}
$ node scripts/ai-workspace-hook.mjs … --upstream <missing path> …
  stdout: {"reason":"upstream-binary-missing"}
$ node scripts/ai-workspace-hook.mjs … --frobnicate
  {"reason":"usage","detail":"--frobnicate"}
$ node scripts/ai-workspace-hook.mjs … alice-topsecret-positional
  {"reason":"usage","detail":"unexpected-positional"}   (no "alice" anywhere)

$ NODE_PATH=<ajv-8.17.1> node scripts/test-ai-workspace-contracts.mjs
{"ok":true,"gate":"ai-workspace-contracts","schema":"contracts/ai-workspace-event.schema.v0.6.3.json", …}
$ node scripts/check-contract-versions.mjs --base origin/ichinya/M7
{"ok":true,"product":"0.6.3","contractArtifacts":96,"base":"origin/ichinya/M7"}
$ check-authority / check-privacy / check-structure / test-fixture-provenance → all exit 0
$ AI_WORKSPACE_BIN=<pinned> node scripts/benchmark-ai-workspace-context.mjs --reps 5
  ok:true; staleness surfaced + resolves, deletion handled, revocation denied
```

No Rust files changed in this round (zero `crates/` diff), so the Rust
gates are unchanged from fix round 1 (fmt clean, clippy clean, 1765
tests passed).
