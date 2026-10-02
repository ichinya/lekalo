# Issue #37 — review round 3 (agent: cline)

Independent verification of **fix round 2** on branch `ichinya/m7-issue-37`
(HEAD `1076aa02`, fix commit `1076aa024e2f94062c65f50e1e174f3ef4700c7e`).
Review-only: no implementation, test, fixture, or contract file was modified.

## Verdict: ACCEPT

The converged major **M7 is genuinely fixed and I reproduced it live**: the
probe-failure reason now reaches the public/stdout surface *precisely*, the
outbox record carries the *identical* reason, and a **real committed gate leg**
now asserts the nonzero branch. Both R2 minors are confirmed fixed and gated.
No vocabulary weakening, no fail-closed regression, no gate-weakening.

One **residual, non-blocking nit** is recorded below (flag-shaped usage
reflection). It is an echo-breadth observation only: it does not change any
exit code, does not leak a value the caller cannot already see in its own
argv, and cannot cause a send. It does not rise to an issue against this
delta, which is why the verdict is ACCEPT.

---

## 1. M7 (the converged major) — FIXED, reproduced behaviorally

Spec item 1. I did not take the fix report's word for it; I drove the hook with
three synthetic upstreams and read **both** surfaces.

| Leg | Synthetic upstream | exit | state | **stdout** `reason` | **outbox record** `reason` |
| --- | --- | --- | --- | --- | --- |
| A | `up.cmd` → `exit /b 7` | 0 | `unavailable` | `upstream-probe-nonzero` | `upstream-probe-nonzero` |
| B | `does-not-exist.exe` | 0 | `unavailable` | `upstream-binary-missing` | `upstream-binary-missing` |
| C | `up.cmd` → `exit /b 0` | 3 | `refused` | `recipient-preflight-unavailable` | `recipient-preflight-unavailable` |

- **Leg A vs B are now distinct on stdout.** In round 2 both collapsed to the
  hard-coded `upstream-binary-missing`, which was the M7 defect. The two
  failure modes are now told apart.
- **Leg A: stdout reason and outbox reason are byte-identical**
  (`upstream-probe-nonzero` on both). This is the specific gap M7 named.
- **Leg C is the important negative control**: a zero-exit upstream is *not*
  reported as a probe failure at all; it advances to the recipient preflight
  and refuses there. So `upstream-probe-nonzero` is emitted only on a genuine
  nonzero exit, not as a catch-all for "anything went wrong upstream".

### The gate leg now exists (round 2's actual gap)

Round 2's complaint was that `test-ai-workspace-hook.mjs:252` asserted **only**
the missing-binary case, so the claimed M7 verification could not have come
from the committed gate. That is now closed by a real two-surface assertion:

- `:274` — `assert.equal(probeNonzero.reason, "upstream-probe-nonzero", ...)`
---

## 2. Fail-closed semantics and closed vocabulary — INTACT

Spec item 2.

- **Exit codes unchanged and fail-closed.** No leg produced a success exit on
  a failure path: A/B exit 0 with `state: "unavailable"` (the hook's documented
  unknown-delivery shape, `ok:true` + explicit reason), C exits 3 refused.
  Nothing degraded to a silent pass.
- **Closed state vocabulary intact.** `HOOK_STATES` (`:83-85`) is still the
  same 7-member set, and the assertion at `:393` still runs
  `assert.ok(HOOK_STATES.has(state), "hook printed an undocumented state")`.
  `unavailable` and `refused` remain members, so A/B/C pass the vocabulary
  check rather than being routed around it.
- **Usage path stays fail-closed.** A usage error exits `USAGE = 2` and creates
  no persistent state (no outbox record, no db file) — I verified the refusal
  output is the only artifact.

---

## 3. The two R2 minors — CONFIRMED fixed and now gated

Spec item 3. Both were previously real but ungated; the fix added the missing
legs, and I reproduced the behavior myself.

- **Staged-taint refusal (R2 minor 1) — FIXED and gated.**
  `test-ai-workspace-hook.mjs:280-302` now stages a real modification to a
  tracked file (`git add docs/target-protocol.md`) and asserts the hook
  refuses `worktree-dirty` (exit 3) — i.e. index-staged changes taint the digest
  exactly like unstaged ones. `:302` asserts the clean state is restored after.
  I confirmed the refusal is genuine by staging my own modification and
  observing the same `worktree-dirty` refusal, then restoring the file (my
  tree is clean at `git status --porcelain`).
- **Usage marker (R2 minor 2) — FIXED for the class it targeted, and gated.**
  `test-ai-workspace-hook.mjs:304-312` asserts a stray positional is reduced
  to the bounded marker `detail: "unexpected-positional"`, with an explicit
  no-leak assert on `alice`. Reproduced: a stray positional yields
  `reason:"usage"`, `detail:"unexpected-positional"`, exit 2, with the
  positional value absent from stdout.

---

## 4. Gates re-run (this machine, real output)

| Command | Result |
| --- | --- |
| `node scripts/test-ai-workspace-contracts.mjs` (ajv 8.17.1 via `NODE_PATH`) | `{"ok":true,"gate":"ai-workspace-contracts",...}` exit 0 |
| `node scripts/test-ai-workspace-hook.mjs` | `{"ok":true,"gate":"ai-workspace-hook","binaryPhases":"skipped","reason":"upstream-binary-not-configured (set AI_WORKSPACE_BIN to run the full proof)"}` exit 0 |
| `node scripts/check-contract-versions.mjs --base HEAD^` | `{"ok":true,"product":"0.6.3","contractArtifacts":96,"base":"HEAD^"}` exit 0 |
| `git status --porcelain` | empty (clean) |

`binaryPhases: "skipped"` is the **recorded-reason skip** (binary phases A/D
need the pinned `lee-to/ai-workspace@8fdf818` checkout), not a silent pass —
it matches the gate's documented contract. The new M7 leg is in the
**dependency-free** phase (`:274/:277`), so it runs in CI without the upstream
binary.

**CI wiring confirmed** — `.github/workflows/ci.yml` runs both gates:
- `:80` `NODE_PATH="$LEKALO_AJV_NODE_PATH" node scripts/test-ai-workspace-contracts.mjs`
- `:89` `NODE_PATH="$LEKALO_AJV_NODE_PATH" node scripts/test-ai-workspace-hook.mjs`

So the new nonzero-probe leg is actually enforced in CI, not merely present.
  on the public result.
- `:277` — `assert.ok(entry3.states.some((s) => s.state === "unavailable" && s.reason === "upstream-probe-nonzero"), "outbox records the same precise reason")`
  on the outbox record.
---

## 5. Residual nit (non-blocking, recorded not raised)

**Flag-shaped usage reflection — `scripts/ai-workspace-hook.mjs:663`.**

```js
const detail = typeof argName === "string" && argName.startsWith("--") ? argName : "unexpected-positional";
```

The bound the fix applied is *positional vs flag-shaped*, so any unknown token
beginning with `--` is still echoed verbatim as `detail`. Reproduced:

| argv token | exit | `detail` |
| --- | --- | --- |
| `--acme-corp-payroll-9931` | 2 | `--acme-corp-payroll-9931` |
| `--private-api-token-xyzzy` | 2 | `--private-api-token-xyzzy` |
| `--db` | 2 | `--db` |

Why I record it but do **not** make it an issue: a caller that can pass
`--private-api-token-xyzzy` on the command line already knows that token and
could read its own argv; nothing here escalates privilege, crosses a trust
boundary, or reaches the network. It also cannot mask a real failure — the
exit code is still 2 and `reason` is still the closed `usage`. The existing
gate leg pins the positional case, which is the realistic leak vector (a path
or value pasted where a flag belongs). A future hardening pass could reduce
unknown flags to a marker or to a bounded length, but that is beyond this
delta's scope.

## 6. Observation (no action)

On Windows, invoking a `.cmd` upstream takes the `shell: true` branch
(`:132`, `:141`), which makes Node emit a `DEP0190` deprecation warning to
**stderr** (~265 bytes). This is Node's own warning, not hook output, and it
carries no private data; stdout and the JSON result are unaffected. Worth
knowing only because a test that asserts empty stderr on a `.cmd` stub would
see these bytes.

---

## Close-out

All three spec items are satisfied: M7's precise reason now reaches both public
surfaces with a real gate leg for the nonzero branch (verified live, with a
zero-exit negative control), the closed vocabulary and fail-closed semantics
are unchanged, and both R2 minors are fixed and now gated. Nothing in this
round contradicts the fix report.

**Verdict: ACCEPT.**
- `:252` — the missing-binary assertion is retained unchanged, so leg B stays
  pinned too.

I confirmed these are *additions*, not substitutions: the filter for removed
`assert`/`deepEqual`/`HOOK_STATES`/`.only`/`.skip`/`.todo` lines over both gate
scripts in `git diff 1076aa02~1 1076aa02` is **empty**.