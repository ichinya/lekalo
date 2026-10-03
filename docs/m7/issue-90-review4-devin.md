# Issue #90 — round-4 review (verify fix round 3, close-out)

Reviewer: independent dispatch (Devin), read-only review of branch
`ichinya/m7-issue-90` at `1509fb9a` (fix-3 delta `b4e66f9e..1509fb9a`:
3 files, +176/−3 — `update-golden-case.mjs`, `test-golden-update-policy.mjs`,
`docs/m7/issue-90-fix3.md`). Base: `origin/ichinya/M7` (merge-base diff;
the branch predates the #37 merge, so #37 files' absence is not a
finding). Inputs: `docs/m7/issue-90-review3-cline.md` (the R3-1 major),
`docs/m7/issue-90-review3-devin.md` (ACCEPT context),
`docs/m7/issue-90-fix3.md` (fix report). Only the fix-3 delta was in
scope; both R2 blockers were already host-confirmed closed on run
37033729116.

**Verdict: ACCEPT.** R3-1 is fixed and verified live: `plan` on a
`diagnostic.*.pair` case now exits 0 (pristine and after a real input
mutation), the emitted candidate rows carry **no** `declaredPath`
member, `apply`'s `unmapped-candidate` refusal still bounds all writes
(exercised, zero bytes written), and `minimal.project` plans and applies
cleanly. The update-policy gate now rehearses the pair class end to end
(section 3b), and I re-verified the negative control: with the pre-fix
producer restored in a scratch clone the extended gate fails with
exactly `pair-plan-phase-failed`. All 10 suite gates +
`run-golden.mjs --verify` re-run green, `manifestDigest` unchanged at
`sha256:92496524…`, `git status` clean, and the delta is purely additive
— no gate weakening.

## Live reproduction (scratch clone of the tracked tree at `1509fb9a`, real debug binary via `LEKALO_GOLDEN_BINARY`, Ajv 8.17.1 via `LEKALO_AJV_NODE_PATH`)

| Check | Result |
| --- | --- |
| `plan --case diagnostic.type-recursion.pair` (pristine tree) | **exit 0** — was `plan-schema / declaredPath must be string` (exit 1) pre-fix |
| plan doc `after.files` | 2 rows (`expected/trigger.envelope.json`, `expected/non-trigger.envelope.json`), both `change:"added"`, **neither carries a `declaredPath` member** (`Object.hasOwn` checked per row); `before.files` empty as expected for a descriptor with no declared outputs |
| `plan` again after mutating `trigger/lekalo/project.yaml` | **exit 0**, plan + candidates written to sandbox only; tracked tree restored and clean |
| `apply --plan <pair plan> --accept-plan-sha256 <correct>` | **refused, exit 1**, exactly `{"reason":"unmapped-candidate","path":"expected/trigger.envelope.json"}`; `git status` in the scratch clone clean — preflight refuses before any write |
| `plan --case minimal.project` | **exit 0**; `after.files` empty (pristine — no drift); `apply` with correct digest exits 0, `applied:0, checksumsRefreshed:true`, sidecar byte-identical (git clean) |

## Gate coverage and negative control

`test-golden-update-policy.mjs:184-211` (new section 3b) runs
`diagnostic.type-recursion.pair` through the real tool inside its
scratch suite copy: plan must exit 0 with a non-empty
`summary.semanticChanges` and non-empty `after.files`; **every**
candidate row is asserted to lack a `declaredPath` member
(`Object.hasOwn`); a wrong accept digest must be refused; and a correct
digest must still be refused with exactly `unmapped-candidate`. A grep
for `--case` in the gate now yields `minimal.project` (×2, the original
rehearsal + the real-mutation replan) **and**
`diagnostic.type-recursion.pair`, and the success payload reports
`pairClassFlow`, so the no-`expected` branch cannot silently regress to
exercising only the case that works.

Negative control re-verified (cheap, in the scratch clone so the tracked
tree stayed pristine): restored the pre-fix `update-golden-case.mjs`
(`git show 1509fb9a^:…`, which emits `declaredPath: null`), placed the
real binary at the scratch's `target/debug/lekalo.exe`, ran
`node scripts/test-golden-update-policy.mjs` → **exit 1** with
`pair-plan-phase-failed: …plan-schema…`. The new rehearsal is
load-bearing, not decorative.

## Gates re-run in this checkout (Windows, Node v24.13.0, debug binary, Ajv 8.17.1)

All exit 0, outputs matching `issue-90-fix3.md`:

| Gate | Result |
| --- | --- |
| `test-fixture-provenance.mjs` | ok (64 families / 64 synthetic / 0 evidence-backed) |
| `test-golden-catalog.mjs` | ok (21 cases, 4 imported evidence, 449 rules, 302/114/13/20, ajv 8.17.1) |
| `test-golden-hygiene.mjs` | ok (259 files, 6 controls, 3 host roots) |
| `test-golden-adapter-shared.mjs` | ok (4 shared evidence, 9 `fixture.rs` includes, 19 shared IR defs) |
| `test-golden-diagnostic-coverage.mjs` | ok (449/20/114/302/13) |
| `run-golden.mjs --verify` | ok (21 cases, `byteCompared: 4`) |
| `test-golden-normalization.mjs` | ok (242 files, producer vectors `newline-crlf-equal-output`, `separator-forward-slash-equal-output`) |
| `test-golden-determinism.mjs` | ok (lanes cold-1/cold-2/warm-cache, 41 rows, `manifestDigest sha256:92496524bb7624a4e4e716d366d611eebb5fab2dcb85802b38b6aa0b32345c90` — **unchanged**, so the fix did not perturb case outcomes) |
| `test-golden-planner-e2e.mjs` | ok (6 stages, `stageDetails` with per-stage digests) |
| `test-golden-update-policy.mjs` | ok (11 plan-schema kinds, `pairClassFlow` reported) |

`git status` clean after all runs (the update-policy gate's own
tracked-tree snapshot check also passed inside the run).

## Hosted CI on `1509fb9a`

`gh run list --branch ichinya/m7-issue-90`: run **37106078349** exists
with `headSha = 1509fb9ab576373c930a9604bea738ef9180e481`. At commit
time of this review (~35 min after the push) the run was still
`in_progress` with **10/11 jobs `success`** (Contracts Node 18.x + 24.x,
build-test ubuntu + macos, MSRV ×3, fmt, clippy, Mago); only
`Build and test (windows-latest)` — the lane that executes the suite
gates — had not yet concluded. No leg has failed; the same gates the
pending lane runs were re-run green locally on this Windows host, so
the in-progress leg is recorded, not treated as a finding.

## Regression / gate-weakening scan (fix-3 delta only)

No weakening found. `update-golden-case.mjs` +7/−3: `declaredPath` is
now spread-omitted rather than set `null` at candidate construction and
in the `after.files` projection — strictly *tighter* conformance to the
unchanged closed schema, plus an explanatory comment. The
`row.declaredPath ?? row.path` fallbacks remain correct for rows without
the member (`undefined → path`). `test-golden-update-policy.mjs` +43:
new header bullet, new section 3b with five fresh assertions, one new
success-payload field — nothing removed, skipped, or exempted. The
schema files are untouched and no workflow change was introduced.

## Observations (not findings)

- For a pair case the trigger-role candidate envelope is **empty bytes**
  (`sha256:e3b0c44…`): `lekalo validate --json` emits the diagnostic
  envelope on **stderr** when validation fails (exit 1), and the
  producer captures stdout only. Pre-existing producer semantics,
  unchanged by this fix, and harmless to the safety property — pair
  candidates can never be published (no declared destination →
  `unmapped-candidate`). If pair cases ever gain declared outputs, the
  producer should probably capture the stderr envelope for failing
  validations; worth a deliberate decision then, not now.
- Latent pre-existing edge in the declared branch (not this fix):
  `declared[index]` is indexed by candidate row, so a descriptor
  declaring fewer `expected` entries than produced project roles would
  throw at plan. No catalogued case hits it (only `minimal.project`
  declares `expected`, with matching counts).
- `minimal.project` `apply` with a pristine plan reports `applied:0`
  and still rewrites the checksum sidecar (byte-identical here); the
  unconditional sidecar write is harmless but technically a write even
  when nothing was published.

## Acceptance-criteria delta

| # | Criterion | Verdict |
| --- | --- | --- |
| AC5 | Deliberate update with reviewed semantic summary | **Met for the pair class at the level the class supports:** plan + review + digest-bound refusal now work end to end for all 21 cases; `apply` correctly refuses pair candidates because no destination is declared (the class has nothing publishable — its goldens live in `expect.json`/`expectation`, enforced by `run-golden`). `minimal.project` writes remain fully functional. |
| All other ACs | unchanged from round 3 | unchanged — no outcome moved (`manifestDigest` identical) |

## Bottom line

R3-1 was a `null`-vs-closed-schema emission defect plus a gate blind
spot; fix-3 closes both in the smallest correct way (omit the member,
rehearse the class, keep `unmapped-candidate` as the write bound) and
documents the controls honestly. I reproduced the fix live, verified
the negative control is load-bearing, re-ran the full gate battery with
identical digests, and found no weakening. The fix-3 delta is approved;
issue #90 close-out is unblocked.
