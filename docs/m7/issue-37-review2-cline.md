# Issue #37 — review round 2 (agent: cline)

Independent verification of **fix round 1** on branch `ichinya/m7-issue-37`
(base `origin/ichinya/M7`, HEAD `541c11c7`). Review-only: no implementation,
test, fixture, or contract file was modified.

## Verdict: ISSUES

1 major, 1 minor. All 3 blockers and all majors from round 1 are genuinely
fixed, and no gate-weakening was found — but one fix-report claim (`M7`) is
**half-true against live behavior**, and it exposes an unverified gate leg.

---

## What I re-ran (this machine, real output)

| Command | Result |
| --- | --- |
| `node scripts/test-ai-workspace-contracts.mjs` (ajv 8.17.1 on `NODE_PATH`) | `{"ok":true,"gate":"ai-workspace-contracts",...}` exit 0 |
| `node scripts/test-ai-workspace-hook.mjs` | `{"ok":true,"gate":"ai-workspace-hook","binaryPhases":"skipped","reason":"upstream-binary-not-configured (set AI_WORKSPACE_BIN to run the full proof)"}` exit 0 |
| `git status --porcelain` | empty (clean) |
| `git stash list` | empty |

The hook gate's `binaryPhases: "skipped"` is the **recorded-reason skip**, not
a silent pass — matches the header contract.

---

## Round-1 findings I independently confirmed fixed

I re-derived these from behavior, not from the fix report's prose.

- **M1 / F5 — canonical policy ref.** The hook reads
  `currentAcceptedRef` from `contracts/privacy-policy.v0.3.2.manifest.json`.
  Live plan output emits
  `digest: "sha256:5a80966fa628fd4c9452325d34e7191f40ebb7a9cb49185c91d413861fb18384"`,
  byte-identical to the manifest's `currentAcceptedRef.digest` (not the raw
  `policyFileDigest: sha256:1fb9047…`). Confirmed.
- **M4 — example is real output.** Ran `--base 4a084aab~1 --head 4a084aab`.
  The produced envelope is **byte-identical** to
  `tests/fixtures/ai-workspace/event-envelope.example.json`, including
  `eventKey sha256:8f981ac4…` and `manifest.digest sha256:57194f2e…`. Confirmed.
- **M6 / F11 — zero-change range is not sendable.** `--base 4a084aab --head 4a084aab`
  returns `{"state":"no-change","reason":"no-approved-artifact-changed"}`
  (exit 0), and the same range **with `--send`** also returns `no-change` —
  the send path is never reached. Confirmed.
- **M8 — `--manifest` is required.** `--send` without `--manifest` →
  `{"state":"refused","reason":"usage"}` exit 2. No committed-fixture fallback.
  Additionally probed a manifest with a malformed `workspaceSlug`
  (`PRIVATE-ACME-INTERNAL-9931`): closed refusal
  `manifest-workspace-slug-grammar` exit 3. Grammar is enforced. Confirmed.
- **F10 — `limitation` is a real enum.** `contracts/ai-workspace-event.schema.v0.6.3.json`
  defines `$defs.limitation` as `"enum": [...]`, not a `pattern`. Confirmed.
- **F6 — outbox occupied by a non-directory.** Pointed `--outbox` at a regular
  file → `{"state":"refused","reason":"outbox-unavailable"}` exit 3, with
  **0 bytes on stderr** (no stack, no child text). Confirmed.
- **Hostile inherited widening flags.** With
  `AI_WORKSPACE_ALLOW_PROJECT_WIDE_TOOLS=1` and
  `AI_WORKSPACE_ALLOW_PROJECT_FILE_WRITE=1` inherited, the send path refuses
  `{"reason":"widening-flags-enabled"}` exit 3
  (`scripts/ai-workspace-hook.mjs:772`). Confirmed.
- **F1 — no private leakage into the event key.** I built a hostile manifest
  with a private `workspaceSlug`, an extra `privateSecretCheckoutPath`,
  `privateUpstreamProjectId: 918273`, and `privateApiToken`, then ran the
  hook. Result: `state: planned` with `eventKey sha256:8f981ac4…`
  **byte-identical** to the clean run, and a grep for every private marker
  found **no occurrence** anywhere in stdout. Confirmed.

## No gate-weakening found
`git diff origin/ichinya/M7..HEAD -- scripts/test-ai-workspace-hook.mjs
scripts/test-ai-workspace-contracts.mjs`, filtered for removed
`assert`/`deepEqual`/`skip`/`only`/`todo`/`expected` lines, returns
**nothing** — no assertions were deleted to make a gate pass. The closed
hook-state vocabulary assertion (`scripts/test-ai-workspace-hook.mjs:84`) is
present and includes `no-change`. CI wiring is real (`.github/workflows/ci.yml`
adds both gates to the Contracts job with `LEKALO_AJV_NODE_PATH`). Stash is
empty and the tree is clean.

---

## Remaining findings

### M7 (major) — emitted `reason` collapses "nonzero probe" into "binary missing"; the nonzero leg is unverified

**Location:** `scripts/ai-workspace-hook.mjs:808-814`

```js
const probe = runProcess(args.upstream, ["--version"], { timeoutMs: 15_000 });
if (probe.kind !== "ok") {
  entry.states.push({ state: "unavailable", reason: probe.kind === "nonzero" ? "upstream-probe-nonzero" : `upstream-${probe.code ?? "failed"}` });
  saveOutbox(args.outbox, outbox);
  emit({ ok: true, hook: "ai-workspace-hook", state: "unavailable", reason: "upstream-binary-missing", eventKey: envelope.eventKey });
  return;
}
```

The outbox records the precise reason, but the **emitted** `reason` is the
hard-coded literal `"upstream-binary-missing"` on every failure branch.

**Evidence (live).** With an upstream that exits nonzero (`cmd.exe`, and a
`.cmd` fake exiting 7), stdout is:

```json
{"ok":true,"hook":"ai-workspace-hook","state":"unavailable",
 "reason":"upstream-binary-missing",
 "eventKey":"sha256:8f981ac4…"}
```

while `outbox.json` for the same run contains:

```json
{"state":"unavailable","reason":"upstream-probe-nonzero"}
```

So the fix report's M7 evidence — *"a nonzero probe closes `unavailable` with
reason `upstream-probe-nonzero`"* — is true only of the private outbox, not
of the public observable. The distinction between "the binary is absent" and
"the binary is present but unhealthy" is exactly the distinction an operator
needs, and it is discarded on the wire.

**Why the gate misses it.** The only assertion on this reason is
`scripts/test-ai-workspace-hook.mjs:252`, which exercises the *missing-binary*
case and asserts `upstream-binary-missing`. There is **no gate leg for the
nonzero probe**, so the fix report's claimed verification could not have been
produced by the committed gate.

**Not a blocker** because the boundary stays closed and fail-closed: both
conditions refuse to send and `unknown-delivery` is not reachable. This is
diagnostic honesty plus the missing test leg — the same review theme that
produced round 1's findings.

**Suggested fix.** Derive the emitted reason from the same expression already
used for the outbox entry (one `reason` local, used by both), and add a
nonzero-probe leg to the gate asserting the emitted reason.

### minor — "never a shell" comments sit next to a `shell: true` branch

**Location:** `scripts/ai-workspace-hook.mjs:130`, `:139` vs `:157`, `:496`

```js
const useShell = process.platform === "win32" && /\.(cmd|bat)$/i.test(command);
… spawnSync(command, argv, { … shell: useShell });
```

On Windows a `.cmd`/`.bat` upstream is launched through a shell. That is what
surfaced a live `DeprecationWarning [DEP0190]` on stderr in my run, and it is
the one place where a caller-supplied `--upstream` path reaches a shell. It is
currently benign (the path is operator-supplied via CLI, not
manifest-driven), but the in-file comments at `:157` ("argv only; never a
shell") and `:496` ("argv array; never a shell") are then inaccurate as
written. Either scope those comments to the git/upstream paths that are truly
shell-free, or resolve the executable and drop the shell branch.

---

## Summary

Fix round 1 is substantive and honest: the round-1 blockers are really fixed,
I reproduced M1/M4/M6/M8/F1/F6/F10 behaviorally, and I found no removed
assertions, relaxed schemas, or skipped tests. The single major is a
narrowed-diagnostic defect plus the test leg that would have caught it, and
it does not weaken any security boundary.
`git diff origin/ichinya/M7..HEAD -- scripts/test-ai-workspace-hook.mjs
scripts/test-ai-workspace-contracts.mjs`, filtered for removed
`assert`/`deepEqual`/`skip`/`only`/`todo`/`expected` lines, returns
**nothing** — no assertions were deleted to make a gate pass. The closed
hook-state vocabulary assertion (`scripts/test-ai-workspace-hook.mjs:84`) is
present and includes `no-change`. CI wiring is real (`.github/workflows/ci.yml`
adds both gates to the Contracts job with `LEKALO_AJV_NODE_PATH`). Stash is
empty and the tree is clean.

---

## Remaining findings