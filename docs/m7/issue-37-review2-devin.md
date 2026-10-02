# Issue #37 — review round 2 (devin)

Reviewer: devin (independent round-2 verification of the fix round).
Review-only: no implementation, test, fixture, or gate file was touched;
the only change in this review is this document. Scope: branch
`ichinya/m7-issue-37`, diff base `origin/ichinya/M7` =
`9510dd0767a56c0ab34b8d3c8ceb2a14db8de825`. Verified HEAD `541c11c7`
(15 commits, 19 files, +4939/-0, zero Rust changes); this document's
first revision was committed atop `f666c562`, the peer round-2 cline
review that landed while this verification was in flight — its findings
are cross-checked below. Reports verified: `issue-37-review-devin.md`
(2 major / 10 minor), `issue-37-review-codex.md` (8 major / 3 minor),
`issue-37-review-cline.md` (3 blocker / 3 major / 3 minor), and
`issue-37-review2-cline.md` (1 major / 1 minor), against the
dispositions in `issue-37-fix1.md`.

## Verdict: ISSUES

Every blocker and every major finding across all three round-1 reviews
is verified fixed — most verified live on this machine, not just by
reading code — and no gate was weakened. However, the peer round-2
review found one **major** I independently reproduce and concur with:
the `unavailable` result collapses the nonzero-probe reason to a
hard-coded literal while the fix report's evidence describes the
private outbox reason, and no committed gate leg exercises that path.
Plus two new **minor** findings of my own and three nits (below).

## What was re-run here (not taken as reported)

- Fresh build of the **pinned** upstream `ai-workspace` binary: the
  pinned source checkout `lee-to/ai-workspace@8fdf818fee75…` was built
  into an external temp target dir (the checkout itself untouched);
  `--version` reports `ai-workspace 1.5.0`.
- `node scripts/test-ai-workspace-hook.mjs` with
  `AI_WORKSPACE_BIN`=<the freshly built pinned binary> →
  `{"ok":true,"binaryPhases":"executed","hookStates":["delivered",
  "no-change","planned","refused","unavailable","unknown-delivery"]}` —
  identical to the fix report's claimed output; phases A–H all green,
  including genuine wrong-group denial, positive-control confinement,
  the real `service_changed` delivery with exact target-set readback,
  duplicate suppression, poisoned-stderr `unknown-delivery`, the keyed
  reconciliation checkpoint, and the hostile-flag refusal.
- Same gate without `AI_WORKSPACE_BIN` → `{"ok":true,"binaryPhases":
  "skipped","reason":"upstream-binary-not-configured…"}` — the CI shape.
- `node scripts/benchmark-ai-workspace-context.mjs --upstream <pinned>
  --reps 5` → green end to end. Every deterministic field reproduces
  the committed evidence exactly: `changeScopeDigest` `79f5c64a…`,
  `samples: 15` per retrieval strategy (5 reps × 3 tasks — cline 2),
  `returnedContextBytes` median 10283 / range [9664,10616],
  `unresolvedRefCount` 953, honest coverage misses
  `[true,true,false]`/`[true,false,true]`, and all asserted gates true
  (staleness surfaced, freshness after sync, deletion handled,
  revocation denied). Timings are machine-local, as documented.
- A second benchmark run with `--out <dir>`: stdout carries the literal
  `rawRowsPrivate: "private-out-directory"`, zero absolute paths on the
  public channel, and the raw rows land inside the private out dir
  (devin M2 / codex F6 / cline 6 — verified live).
- Dependency-free gates: `check-contract-versions --base
  origin/ichinya/M7` (96 artifacts), `check-authority` (12/27/15),
  `check-privacy` (accepted), `check-structure`, `check-model`,
  `test-fixture-provenance` (64 families), `test-contract-versions`,
  `test-ai-workspace-contracts` under pinned Ajv 8.17.1 — all pass.

## Live probes (adversarial, all synthetic/temp state)

- `--base HEAD` → `no-change` / `no-approved-artifact-changed` (cline 1
  root case — now the correct closed state, and the gate asserts it).
- `--base 4a084aab~1 --head 4a084aab` → `planned`; the emitted envelope
  is **byte-identical** to the committed example
  (`JSON.stringify(envelope) === JSON.stringify(example)`, eventKey
  `sha256:8f981ac4…`) — devin M4 verified: the example is captured real
  output.
- `privacyPolicyRef.digest` in the live envelope is
  `sha256:5a80966f…` — the canonical policy identity, not the raw file
  digest (devin M1 / codex F5).
- `--send` without `--manifest` → `refused/manifest-required`, exit 3
  (devin M8). Poisoned `reaction` → `manifest-reaction-unknown` before
  any output; 65 routes → `manifest-too-many-routes` (codex F1).
- Manifest with mutated `workspaceSlug` plus an injected private member
  → **identical event key** `8f981ac4…`, no private strings in output —
  the public digest binds only the neutral routing projection (F1).
- `--upstream` whose `--version` exits 7 → `unavailable` /
  `upstream-binary-missing`, exit 0, outbox records
  `upstream-probe-nonzero` (devin M7's behavioral fix is real — a
  nonzero probe no longer proceeds to send — but the emitted reason is
  the collapsed literal; see peer cross-check below).
- Insert-then-crash fake upstream (create writes the event then exits
  1): run 1 → `unknown-delivery`/`upstream-nonzero`; run 2 under the
  same key → reconciled through `workspace_events`, found the inserted
  event by key, ran full verification — **`creates` stayed at 1** (the
  round-1 bug would have created a duplicate). The run-2 result was
  `readback-targets-mismatch` because the `.cmd` launcher shim truncates
  the multi-line body argument — a harness limitation, and incidentally
  live proof that the body-equality check in `verifyDelivery` really is
  enforced (codex F2/F3/F9).
- `--outbox` pointing at an existing plain file → `refused/
  outbox-unavailable`, exit 3, **0 bytes on stderr** (codex F6 — was a
  Node stack pre-fix).
- `git status` clean; `git stash list` empty (cline 3); `git diff
  origin/ichinya/M7...HEAD --check` clean; all diff artifacts are the 19
  expected files — no unreviewed extras.

## Disposition verification, by report

**Devin (all verified fixed):** M1 canonical policy ref (live, above);
M2 `rawRowsPrivate` literal marker + probes over the complete printed
document (live `--out` run); M3 `"docs/target-protocol.md modified"`
(live); M4 example = real captured envelope (byte-identical, above); M5
hook gate wired in the Contracts job with `LEKALO_AJV_NODE_PATH` and the
header corrected; M6 closed `no-change` state before the send path
(live); M7 the behavioral fix is real — a nonzero `--version` no longer
proceeds to send — but see the round-2 major below for the collapsed
`reason` on the wire; M8 manifest required + closed validation (live,
three refusals); M9 boundary table now names exactly the committed
proofs; M10 `/.ai-workspace.local.json` ignored; M11 every
`unknown-delivery` carries a closed `reason` (asserted in phases C/G and
observed live); M12 dead `privateRoot` removed, schema indentation
fixed.

**Codex (all verified fixed):** F1 closed manifest validation + emitted
envelope self-validation + projection-scoped digest (all live); F2 keyed
reconciliation before any new create + `sending` checkpoint (live —
no duplicate create); F3 mandatory fail-closed graph read, details
required for delivery, exact target-set equality, `unverifiedDeclared
Routes` reported separately (live mismatch leg + phase F); F4 recipient
preflight before create — `unreviewed-recipients-present` /
`no-declared-consumer-linked` refusals (code inspection + structure);
F5 = devin M1; F6 closed failure boundary (uncaughtException →
`refused/internal-error`, filesystem failures → `outbox-*` bounded
codes, 0-byte stderr verified) — one residual nit, below; F7 CI wiring +
genuine wrong-group leg + slug-resolved project id + positive controls
(all executed in the binary-tier run above); F8 REPS-per-task retrieval,
bounded `codegraph_context`, returned bytes, `tokenEstimate:"unknown"`,
unresolved refs, changed/deleted phases, asserted staleness/freshness
(full benchmark reproduced); F9 bounded provenance body with
kind/title/body/key verified (body-equality enforced — live mismatch
leg); F10 real `enum` + Ajv-enforced `private-payroll-sentinel`
negative vector (contracts gate green); F11 = devin M6.

**Cline (all dispositioned):** 1 — Phase B now asserts both legs
(`no-change` for an empty range AND `planned` for `4a084aab~1..4a084aab`)
and the gate is committed green on both tiers here; 2 — retrieval
loop is `rep × task` (15 samples/strategy; reproduced); 3 — rewrite
committed in `33747176`, stash dropped, tree clean; 4 — `fix1.md` is
the disposition table for all three reviews; 5 — the gate is committed,
green, and CI-wired, and I reproduced the binary tier on this machine;
6 = devin M2; 7 — rebuttal accepted: the aggregate schema string is a
benchmark-evidence tag, and script + `.md` + `.json` were regenerated in
one commit consistent with `docs/versioning.md` (change-set version =
product version); 8 — rebuttal accepted (verified negative); 9 —
`no-change` is in the documented closed set
(`issue-37-implementation.md:68-70`) and the gate asserts every printed
state is a member — executed green in both tiers.

## Peer round-2 cross-check (`issue-37-review2-cline.md`, landed mid-review)

- **cline round-2 major (M7 reason collapse) — CONFIRMED, I concur.**
  `scripts/ai-workspace-hook.mjs:808-814` emits the literal
  `reason: "upstream-binary-missing"` for every probe failure branch
  while the outbox records the precise `upstream-probe-nonzero`. My own
  live probe of an exit-7 fake (above) produced exactly the output cline
  quotes — `unavailable`/`upstream-binary-missing` on the wire,
  `upstream-probe-nonzero` in `outbox.json`. Two consequences are real:
  the fix report's M7 evidence ("a nonzero probe closes `unavailable`
  with reason `upstream-probe-nonzero`") describes only the private
  outbox, not the emitted result the claim implies; and the committed
  gate asserts the missing-binary leg only (`test-ai-workspace-hook.mjs`
  `:252`) — no nonzero-probe leg exists, so the claimed distinction is
  unverified by the committed artifacts. Fail-closed is preserved and
  the vocabulary stays closed, so it is not a blocker; as an
  evidence-claim-vs-wire mismatch plus a missing test leg it is
  consistent with this review's major bar. Suggested fix stands: derive
  one `reason` local used by both the outbox record and the emitted
  result, and add a nonzero-probe leg asserting it.
- **cline round-2 minor (`.cmd`/`shell: true` vs "never a shell"
  comments) — CONFIRMED as a doc nit.** `runProcess` does take the
  `shell: true` branch for `.cmd`/`.bat` upstream paths on Windows
  (`scripts/ai-workspace-hook.mjs:130-139`), so the "argv array; never
  a shell" comments at `:157` and `:496` are inaccurate for that edge;
  the `DEP0190` warning cline observed is consistent with that branch.
  Benign for privacy (child stderr is quarantined; the path is
  operator-supplied), but the comments should be scoped or the
  executable resolved.

## New findings (round 2, this reviewer)

1. **minor — `source.clean` misses staged (index) modifications to
   approved paths.** `worktreeIsClean` runs `git diff --quiet`
   (worktree-vs-index) at `scripts/ai-workspace-hook.mjs:174-182`, so a
   staged-but-uncommitted edit to an approved path still yields
   `clean: true` — verified live: staged mutation of
   `docs/target-protocol.md` → `planned`, `clean: true`, same eventKey.
   The digests remain self-consistent (committed bytes only), but the
   documented guarantee ("refuse a dirty tracked worktree") is weaker
   than stated. Fix: also check `git diff --cached --quiet`, or diff
   worktree against the resolved head (`git diff --quiet <head>`).
2. **minor — usage `detail` still reflects arbitrary non-flag tokens.**
   `bad(arg)` echoes any unrecognized token
   (`scripts/ai-workspace-hook.mjs:649-654,673`); a stray positional is
   returned verbatim — verified: `"C:\Users\alice\topsecret-positional"`
   came back in `detail`. Flags and missing-value names are safe; a
   stray positional is a reflected value, which the header's "no
   arbitrary argument reflection" and the guide's "instead of a …
   reflected value" claim do not fully hold. Local-stdout only; fix by
   emitting `usage` without `detail` (or echoing the arg count/index).

## Nits (non-blocking)

- Dead `const save` closure at `scripts/ai-workspace-hook.mjs:916`
  (`resolveDelivery` uses `reconstructOutbox` instead).
- The contracts-gate leak walker (`test-ai-workspace-contracts.mjs:156`)
  still pushes nothing for primitive array entries — the tail of codex
  F7. Currently dormant: the envelope's only primitive-string array
  (`limitations`) is now a real `enum`, double-checked by the hook's own
  closed set, so no live leak path exists; if a future schema adds
  another primitive array, the gap re-opens.
- The cline round-2 `.cmd`-comment inaccuracy listed above also belongs
  in this bucket once the comments are scoped.

## Round-2 tally

- Round-1 findings: 3 blockers + 13 majors verified fixed (the deduped
  cross-report pairs counted per report), all minors dispositioned
  (fixed or rebutted with verified evidence).
- New round-2 findings: **1 major** (emitted `unavailable` reason
  collapse + missing nonzero-probe gate leg — cline's, confirmed here),
  **3 minors** (staged-modification `clean` flag, stray-positional
  reflection in usage `detail`, `.cmd` shell comment inaccuracy), 3
  nits.

## Honest limits of this review

- `cargo fmt/clippy/test` (the 1765-test claim) not re-run — zero Rust
  changes in the diff make it moot.
- `test-model-contracts.mjs` local-provisioning failure claim not
  re-verified; consistent across both prior reviews and plausibly
  pre-existing on `ichinya/M7`.
- The pinned binary was built locally from the pinned source tree
  (commit verified `8fdf818f…`), not cryptographically build-attested;
  and the `.cmd`-launcher shape of my synthetic fakes cannot faithfully
  carry multi-line `--body` argv on Windows, so the reconcile→`delivered`
  happy path was exercised through the committed gate's real-binary
  phase F rather than my shim (the critical safety property — no blind
  re-create — was proven directly).
