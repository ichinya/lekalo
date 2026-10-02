# Issue #90 review round 2 — verification of fix round 1 (ISSUES)

Reviewer: independent dispatch (Cline), read-only review of branch
`ichinya/m7-issue-90` at `b86a0cb0`, diff base `origin/ichinya/M7`.
Fix commits under review: `3d3dd19d` (part 1) + `a68f9654` (part 2),
dispositioned in `docs/m7/issue-90-fix1.md` (all 20 findings D1–D11 /
C1–C9 marked **fixed**).

**Verdict: ISSUES** — the fix round is real and most of round 1 is
genuinely closed (I re-ran the round-1 *negative controls* and they now
fail correctly, which they did not before). But **two land-blocking CI
failures remain, and both are the exact defect class round 1 flagged as a
blocker (C1/D1: "a gate wired into a job that does not satisfy its
prerequisites")**. The fix round correctly recognized one binary
prerequisite and missed the second; hosted CI is red on both contracts
lanes and the whole Windows build-test lane. Four minor findings persist,
two of which are new drift introduced by the fix round itself.

## Verification performed locally (Windows checkout, debug binary, Ajv 8.17.1)

All 10 suite gates exit 0 and match the fix report's claimed outputs:

| Gate | Result |
| --- | --- |
| `test-fixture-provenance.mjs` | ok (64 families) |
| `test-golden-catalog.mjs` | ok, ajv 8.17.1 (21 cases, 449 rules, 302/114/13/20) |
| `test-golden-normalization.mjs` | ok (242 files, 2 producer vectors) |
| `test-golden-hygiene.mjs` | ok (259 files, 6 controls) |
| `test-golden-adapter-shared.mjs` | ok |
| `run-golden.mjs --verify` | ok (4 byte-identical roles) |
| `test-golden-determinism.mjs` | ok (3 lanes, 41 rows, `manifestDigest sha256:92496524…`) |
| `test-golden-diagnostic-coverage.mjs` | ok (449/20/114/302/13) |
| `test-golden-planner-e2e.mjs` | ok (6 stages) |
| `test-golden-update-policy.mjs` | ok (11 plan schema kinds) |

`git status` clean; diff still 100% additive apart from `ci.yml` and
`fixture-provenance.json`.

## Negative controls re-run (the decisive checks)

Round 1's findings were mostly "this gate stays green when it should
fail". I re-ran those exact probes:

- **D2/C3 `--verify`**: replaced `minimal/expected/ir-envelope.json` with
  `{"status":"CORRUPTED-GOLDEN"}` → `run-golden --case minimal.project
  --verify` now **exits 1** (byte-drift). In round 1 this probe stayed
  green. Restored byte-identical. **Closed.**
- **C4 fabricated witness**: set `adapter.check-failed`'s
  `testWitness[0].gate` to `node:scripts/DOES-NOT-EXIST.mjs` → catalog
  gate **exits 1** with
  `coverage-witness-unresolved: adapter.check-failed: node:scripts/DOES-NOT-EXIST.mjs`.
  In round 1 this probe passed. Restored byte-identical. **Closed.**
- **D6 dead recipe surface**: all five `UPDATE_RECIPES` names now resolve
  to real scripts (`update-golden-case`, `update-golden-run-manifest`,
  `update-golden-checksums`, `gen-suite-coverage`,
  `gen-suite-diagnostic-pairs`); the phantom `update-golden-coverage` is
  gone. **Closed.**

## Findings

### R2-1. BLOCKER (new, regression of the C1/D1 class) — the normalization gate fails closed in the `contracts` job, which never builds the CLI

`scripts/test-golden-normalization.mjs:130-133` now (correctly, for C9)
resolves the producer and pushes `normalization-producer-missing` into
`errors` when `target/debug/lekalo` is absent:

```js
const binary = join(REPO_ROOT, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
if (!existsSync(binary)) {
  errors.push("normalization-producer-missing: cargo build -p lekalo-cli --locked first");
}
```

But the gate is still wired at `.github/workflows/ci.yml:110`, inside
the `contracts` job, whose only cargo invocation is
`cargo test -p lekalo-core target_protocol_conformance` (`ci.yml:81`).
`lekalo-core` has no bin target, so `target/debug/lekalo` never exists in
that job. The step's own comment (`ci.yml:102`, "Catalog/normalization/
hygiene/adapter-shared are Node-only") is therefore false, and the gate
header's "CI runs this gate only after the build" is contradicted by the
wiring.

**Verified live and independently**: in a clean `git worktree` of `HEAD`
(no `target/`), `node scripts/test-golden-normalization.mjs` exits 1 with
exactly:

```json
{"ok": false, "gate": "golden-normalization",
 "errors": ["normalization-producer-missing: cargo build -p lekalo-cli --locked first"]}
```

The sibling gates in that same step pass without a binary
(`test-golden-hygiene.mjs` exit 0, `test-golden-adapter-shared.mjs` exit
0), isolating normalization as the only broken lane.

The fix round recognized exactly this prerequisite class for
`test-golden-update-policy.mjs` (moved to `build-test`, after
`cargo build --workspace`) but missed normalization — **the C9 fix
introduced the dependency and the wiring did not follow**. Move the gate
(or only its §6 producer-vector section) into the build-test suite step,
or provision the binary in `contracts`. Note the gate also hardcodes
`target/debug/lekalo` and ignores `LEKALO_BIN`, unlike its siblings
(`run-golden.mjs:77`, `test-golden-determinism.mjs:36`), so it cannot be
redirected by env.
### R2-2. BLOCKER (C2 persists) — `realpathSync` without `.native` does not expand the `RUNNER~1` temp alias

C2 prescribed resolving scratch roots with `realpathSync.native`,
"following the existing solution in `scripts/test-run-history-cli.mjs:94`".
That file carries the exact contract in its own comment:

> `// The libuv (native) realpath is required:`
> `// the default JS realpath keeps an 8.3-spelled input as written.`

Every scratch root in the fix round used the **plain** JS `realpathSync`:

- `scripts/run-golden.mjs:84`
- `scripts/test-golden-determinism.mjs:57`
- `scripts/update-golden-case.mjs:59`
- `scripts/test-golden-planner-e2e.mjs:39`
- `scripts/update-golden-run-manifest.mjs:38`
- `scripts/test-golden-normalization.mjs:137`

By the repo's own documented contract, plain `realpathSync` leaves an
8.3-spelled input as written, so `RUNNER~1` survives into the subprocess
cwd and the production selection policy denies every invocation with
`structure.selection-alias` (exit 3) before command logic runs.

I could not reproduce this locally: `fsutil 8dot3name query C:` reports
"8dot3 name creation is DISABLED on C:", so no short-name alias is
generated and plain/native `realpathSync` return identical strings on
this checkout — the local green Windows runs therefore prove nothing
about the hosted lane. The defect is nonetheless certain from the code
plus the repo's own stated contract; the fix is mechanical —
`realpathSync.native(...)` at each of the six sites above (plus
`test-golden-update-policy.mjs:71`, currently unwrapped, for consistency).

### R2-3. MINOR — `run-manifest.schema` relaxed `runs.minItems` 2 → 1 to fit the committed document

C7 asked to "align producers and versioned contracts". For the coverage
schema that was done correctly (`generatedBy.script` repointed at the
real producer). For run-manifest, alignment was achieved by *weakening*
the contract. Confirmed by history:

- `3d3dd19d~1`: `"minItems": 2` (then `1`)
- `HEAD`: `"minItems": 1` (then `1`)

while the committed `run-manifest.json` carries a single lane
(`runs[0].lane == "cold-1"`, 41 outcomes). Enforcement is not lost — the
determinism gate proves 3-lane equality live — but a versioned contract
was loosened to accommodate data rather than the document regenerated to
satisfy it. Regenerating a multi-lane manifest, or deliberately pinning
single-lane (documented), are both defensible; silently relaxing the
schema is not.

### R2-4. MINOR — planner-e2e stages 4–5 still run canned corpora; stage digests computed but never emitted

Stages 1–3 are now genuinely linked, as claimed: one sandbox project
flows load → IR → graph → inspect/impact/context, with the symbol chosen
from that project's own IR. The two substitutions C5 flagged remain:

- Stage 4 (`test-golden-planner-e2e.mjs:166-175`) still spawns the
  committed `scripts/test-node-scenario-tests.mjs` from `repoRoot` with
  no upstream project/IR input.
- Stage 5 (`:178-185`) still validates the committed
  `tests/fixtures/trace/golden/planner.trace.json` (`TRACE_GOLDEN`,
  `:66`) rather than a trace built from this run's model/diff/results.

Both are now labeled honestly in the header rather than overclaimed,
which is the right call for a residual, but the AC6 evidence row still
overstates the chain. Additionally, per-stage digests (`loadDigest` `:87`,
`graphDigest`/`contextDigest` `:119-120`, `diffDigest` `:161`) are
collected into `stages[].detail` while `passGate` (`:187-190`) emits only
`stageNames` — "stage digests returned in the receipt" holds only on the
failure path.

### R2-5. MINOR — `apply` publishes bytes but does not refresh revision/checksum witnesses in the same reviewed operation

C6 asked that "revisions/checksums/review evidence" update as part of the
reviewed operation. Apply now correctly binds candidate digests, confines
destinations to the descriptor's declared expected set, and preflights
all reads before any write (`update-golden-case.mjs:341-374`) — that part
is closed. But it writes only `expected/` files; `catalog.json` revision
and `checksums/<case>.json` sidecars stay stale until
`update-golden-checksums.mjs` is run as a follow-up, and the catalog gate
flags `checksums-digest-drift` in the interim. Workable as a documented
two-step regenerator flow, but the reviewed write is not self-contained.

### R2-6. MINOR (new drift, introduced by the fix round) — implementation report counts are now stale

`docs/m7/issue-90-implementation.md` was not updated when the 20th
semantic pair was added. Measured against the tree:

| Claim in report | Actual |
| --- | --- |
| `:29` "exact case inventory (20 cases)" | 21 |
| `:30`, `:136` "3 lanes x 20 cases (39 rows)" | 21 cases / 41 rows |
| `:154` "`run-golden.mjs --verify` — ok (39 rows)" | 41 |

Round 1's D11 explicitly asked that the claims match. The count that
mattered ("all 20 `semantic.*` rules") became true by adding the 20th
pair — fine — but the case/row counts went stale as a side effect.
## Acceptance-criteria checklist

| # | Criterion | Verdict |
| --- | --- | --- |
| AC1 | Byte-stable repeat runs | **Met.** `--verify` is now genuinely wired: all 4 declared minimal roles are produced by their real producer and compared as raw bytes with no trimming; my corrupt-golden probe fails the gate. Determinism pins `stdoutDigest`/`stderrDigest` separately; 3 lanes × 41 rows agree; committed manifest cross-checks. |
| AC2 | LF/CRLF + path-separator normalization | **Met in substance.** §6 now materializes a CRLF variant + a forward-slash nested selector and executes both through the real binary, requiring byte-identical output; binary absence fails closed. The gate is mis-wired in CI (R2-1). |
| AC3 | Positive AND negative fixture per rule, or justified state | **Met.** 20 real pairs, 114 family-fixture rows, 302 test-witness rows whose `gate` paths are verified on disk (fabricated-witness probe now fails), 13 justified `interaction-only` rows counted separately. Zero `UNMAPPED` strings remain. |
| AC4 | Adapter conformance reuses shared fixtures | **Met.** Unchanged and green. |
| AC5 | Deliberate update with reviewed semantic summary | **Mostly met.** Plan/apply binds candidate digests to preimages and confines destinations; the summary is now a real field-wise semantic delta printed to stderr, not a hash-only line; the policy gate rehearses an actual mutation. Residual: R2-5. |
| AC6 | Planner fixture covers P0 end-to-end chain | **Partially met.** Stages 1–3 are now a real linked chain; stages 4–5 remain canned (R2-4). |
| AC7 | No secrets / host absolute paths | **Met.** Closed pattern classes, live controls, zero exemptions; hygiene now scans all 259 suite scripts, not 2. |
| — | Fixtures registered; existing fixtures untouched | **Met.** Diff remains 100% additive apart from `ci.yml` + `fixture-provenance.json`. |
| — | CI green | **Not met.** Contracts (Node 18.x and 24.x) and build-test (windows-latest) are red (R2-1, R2-2). |

## Bottom line

The substantive engineering in this fix round is good, and I verified
the hardest claims myself rather than taking them on trust: the
`--verify` byte path, the fabricated-witness rejection, and the dead
recipe surface are all genuinely closed, and their round-1 negative
controls now correctly fail.

What blocks landing is the wiring, and it is the same wiring mistake
round 1 already flagged once: **a gate whose prerequisites the job does
not satisfy**. R2-1 is a *new* instance created by the C9 fix itself (the
normalization gate gained a binary dependency in this round and stayed in
a job with no binary); R2-2 is an incomplete application of C2 (plain
instead of `.native`, which the repo's own comment says does not work).
Both are small, mechanical, and well-specified — move normalization to
`build-test` (or provision the binary) and switch six call sites to
`realpathSync.native`. Four minors are honest residuals, two of which are
drift the fix round introduced itself and should be swept before merge.

Given the branch has already burned two review rounds and the remaining
defects are one-line-each, the efficient path is: fix R2-1/R2-2, sweep
R2-3/R2-6, record R2-4/R2-5 as documented residuals, re-land, and let the
hosted Windows lane confirm.