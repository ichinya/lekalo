# Issue #90 — round-3 review (verify fix round 2)

Reviewer: independent dispatch (Devin), read-only review of branch
`ichinya/m7-issue-90` at `b7a950c2`. Base: `origin/ichinya/M7`.
Fix delta reviewed: `b86a0cb0..b7a950c2` (13 files, +857/−88).
Inputs re-verified against `docs/m7/issue-90-review2-devin.md` and
`docs/m7/issue-90-review2-cline.md` (R2-1..R2-6) and
`docs/m7/issue-90-fix2.md` (fix report).

**Verdict: ACCEPT.** Both converged blockers are fixed in code and —
decisively — confirmed by hosted CI: run **37033729116** on `b7a950c2`
is fully green (11/11 jobs), including the two contracts lanes and the
windows build-test lane that were red on run 36989364497. All four
minor findings are dispositioned correctly (three fixed, one honestly
documented residual), all 10 suite gates re-run green locally with
outputs matching the fix report, and the delta contains no gate
weakening — the only schema change restored a relaxed bound.

## Hosted CI (the two R2 blockers were hosted-CI failures)

`gh run list --branch ichinya/m7-issue-90` / run 37033729116
(`headSha = b7a950c2b08ce4d93333eab923bca847493c24c6`, conclusion
`success`, 21m33s):

| Lane | Run 36989364497 (53ed834e) | Run 37033729116 (b7a950c2) |
| --- | --- | --- |
| Contracts (Node 18.x) | failure — `normalization-producer-missing` | success |
| Contracts (Node 24.x) | failure — `normalization-producer-missing` | success |
| Build and test (windows-latest) | failure — 41 rows + 4 producers exit 3 | success |
| Build and test (ubuntu/macos), MSRV ×3, fmt, clippy, Mago | success | success |

## Findings — all R2 items verified

### R2-1. FIXED — normalization gate moved to `build-test`, after `cargo build`

`ci.yml`: `node scripts/test-golden-normalization.mjs` removed from the
contracts step (both Node lanes) and added to the build-test "Run the
golden fixture suite gates" step (`ci.yml:246`), which runs after
`cargo build --workspace --locked` (`ci.yml:192`) on every OS. Both
step comments corrected (contracts comment no longer claims
normalization is Node-only; build-test comment documents the
binary-dependent vector). The gate additionally accepts `LEKALO_BIN`
(`test-golden-normalization.mjs:130-131`), matching its siblings, and
still fails closed (`normalization-producer-missing`) when the binary
is absent — so the contracts job can no longer hit that failure
because it no longer runs the gate. Hosted contracts lanes green.

### R2-2. FIXED — `realpathSync.native` at every suite scratch root

All seven sites use `.native` (verified by grep, zero plain
`realpathSync` on any suite scratch root):
`run-golden.mjs:84`, `test-golden-determinism.mjs:57`,
`test-golden-normalization.mjs:138`, `test-golden-planner-e2e.mjs:43`,
`test-golden-update-policy.mjs:74` (now wrapped, with the honest
comment citing the 8.3 contract), `update-golden-case.mjs:59`,
`update-golden-run-manifest.mjs:42`. Hosted evidence: the windows
build-test lane — where run 36989364497 showed all 41 rows + 4
`--verify` producers denied at `structure.selection-alias` — is green
on 37033729116. (As both R2 reviews noted, the alias path is not
reproducible on a checkout with 8.3 creation disabled; the hosted A/B
is the confirmation, and it now confirms.)

### R2-3. FIXED — `minItems: 2` restored; manifest regenerated as two lanes

`run-manifest.schema.v1.0.0.json` `runs.minItems` is back to `2`
(diff confirms the only schema change in the round). The committed
`run-manifest.json` carries lanes `cold-1` + `cold-2`, 41 outcomes
each, verified byte-identical row-for-row
(`JSON.stringify(runs[0].outcomes) === JSON.stringify(runs[1].outcomes)`).
`update-golden-run-manifest.mjs` now executes both cold lanes in
independent sandbox roots and `failGate`s on `lane-divergence` before
writing; the per-row `expectation-violation` check is preserved. The
determinism gate still cross-checks the committed `cold-1` lane by
name (`test-golden-determinism.mjs:175`), so a two-lane manifest does
not weaken that check. Ajv validation is mechanical: the catalog gate
compiles the run-manifest schema and validates the committed document
(`test-golden-catalog.mjs:61,173-178`) and exits 0 locally.

### R2-4. DISPOSITIONED — stage digests emitted; stages 4–5 honestly labeled residuals

`passGate` now emits `stageDetails` with per-stage detail on the
success path — verified live: the local receipt carries `loadDigest`,
`graphDigest`, `contextDigest`, `diffDigest`, scenario count, and trace
identity per stage. The gate header now explicitly labels stage 4
(committed scenario corpus) and stage 5 (canonical trace golden) as
documented residuals with the reason (G05 trace-producer work from the
research). The residual substance is unchanged — acceptable as
documented, matching both R2 reviews' own recommendation ("record
R2-4/R2-5 as documented residuals" was the stated efficient path;
R2-5 went further and got fixed).

### R2-5. FIXED — apply refreshes the checksum sidecar in the same write

`update-golden-case.mjs:380-403`: after publishing the expected bytes,
apply re-walks the case directory (sorted DFS, repo-relative logical
paths) and rewrites `checksums/<caseId>.json` with
`{caseId, revision, algorithm, files:[{path, sha256}]}` — byte-for-byte
the same shape `update-golden-checksums.mjs` produces, so the catalog
gate's `checksums-digest-drift` check sees a consistent sidecar
immediately after apply. The write is exercised end-to-end: the
update-policy gate runs `apply` inside a scratch suite copy (which
includes `checksums/`) and exits 0. `checksumsRefreshed: true` is
reported in the receipt.

### R2-6. FIXED — implementation report counts corrected

`issue-90-implementation.md` now reads 21 cases / two-lane manifest /
41 rows per lane / 4 byte-identical roles. Verified against the tree:
`catalog.json` has 21 cases; `run-manifest.json` has 2 lanes × 41
outcomes; `run-golden --verify` reports `byteCompared: 4`. No stale
20/39 counts remain.

## Gates re-run in this checkout (Windows, debug binary, Ajv 8.17.1 via LEKALO_AJV_NODE_PATH/NODE_PATH)

All exit 0, outputs consistent with the fix report:

| Gate | Result |
| --- | --- |
| `test-fixture-provenance.mjs` | ok (64 families, 64 synthetic, 0 evidence-backed) |
| `test-golden-catalog.mjs` | ok (21 cases, 4 imported evidence, 449 rules, 302/114/13/20, ajv 8.17.1) |
| `test-golden-hygiene.mjs` | ok (259 files, 6 controls, 3 host roots) |
| `test-golden-adapter-shared.mjs` | ok (4 shared evidence, 9 fixture.rs includes) |
| `test-golden-diagnostic-coverage.mjs` | ok (449/20/114/302/13) |
| `test-golden-normalization.mjs` | ok (242 files, producer vectors `newline-crlf-equal-output`, `separator-forward-slash-equal-output`) |
| `run-golden.mjs --verify` | ok (21 cases, 4 byte-identical roles) |
| `test-golden-determinism.mjs` | ok (lanes cold-1/cold-2/warm-cache, 41 rows, `manifestDigest sha256:92496524…`) — unchanged digest, so the fix did not perturb case outcomes |
| `test-golden-planner-e2e.mjs` | ok (6 stages, `stageDetails` with per-stage digests) |
| `test-golden-update-policy.mjs` | ok (11 plan schema kinds, plan → review → apply) |

`git status` clean after all runs (the determinism pollution check
also passed inside the run).

## Regression / gate-weakening scan

The delta contains no weakening: every script change is additive or
strengthening (native realpath, `LEKALO_BIN` redirection, sidecar
refresh, `stageDetails`, lane-equality check); the only schema change
*restored* a bound (`minItems` 1→2); the moved gate still executes on
every OS, now where its prerequisite exists; the manifest writer kept
its per-row expectation check and added a cross-lane check. No
assertions removed, no legs skipped, no new exemptions.

## Observations (not findings)

- The apply-side sidecar re-walk duplicates the walk logic of
  `update-golden-checksums.mjs` rather than sharing it; outputs are
  identical by inspection, so this is cosmetic only.
- Apply's sidecar write happens after the expected-bytes publish and
  is not part of the preflight — inherent ordering (the sidecar
  digests the just-written bytes), and the `checksums/` directory is
  suite-owned so the write cannot fail in practice.
- `update-golden-run-manifest.mjs` rows key stdout as `outputDigest`
  while determinism rows use `stdoutDigest`; the cross-check maps
  them correctly — pre-existing naming asymmetry, unchanged this round.

## Bottom line

Both blockers are resolved in the diff and confirmed on the hosted
runner: the normalization gate now executes only where the CLI binary
exists, and every suite scratch root expands the 8.3 alias the
production selection policy denies. The minors were dispositioned
exactly as the R2 reviews recommended — the schema relaxation was
reverted rather than accepted, the apply write is self-contained, the
report matches the tree, and the one true residual (planner stages
4–5) is labeled honestly with its dependency on future G05 work.
