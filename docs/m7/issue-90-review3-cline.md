# Issue #90 review round 3 - verification of fix round 2 (ISSUES)

Reviewer: independent dispatch (Cline), read-only review of branch
`ichinya/m7-issue-90` at `b7a950c2` (fix-2 delta `53ed834e..b7a950c2`),
diff base `origin/ichinya/M7`. Inputs: `docs/m7/issue-90-review2-devin.md`
and `docs/m7/issue-90-review2-cline.md` (R2-1..R2-6) and
`docs/m7/issue-90-fix2.md` (fix report).

**Verdict: ISSUES.** Both converged blockers are genuinely fixed and are
now confirmed by hosted CI (run **37033729116** on `b7a950c2` is fully
green, 11/11 jobs), all four minors are correctly dispositioned, and the
delta contains no gate weakening. But focused verification of R2-5
("`apply` refreshes the case checksum sidecar in the same write") forced
me to actually exercise the update flow, and it is **unusable for 20 of
the 21 catalogued cases**: `plan` fails closed with
`plan-schema / declaredPath must be string` for every
`diagnostic.*.pair` case. This is a real functional defect in the AC5
deliberate-update path, it is invisible to every gate (the update-policy
gate rehearses only `minimal.project`, the single case that works), and
it was not part of the fix-2 scope - so it is a **pre-existing defect
that survives the fix round**, not a regression introduced by it. It does
not affect CI greenness (CI never runs the update flow), so it does not
block the two R2 blockers from being called closed; but it is a
functional gap in a shipped deliverable and should not merge silently.

## Hosted CI - both R2 blockers confirmed closed (not pending)

`gh run list --branch ichinya/m7-issue-90`, run **37033729116**,
`headSha = b7a950c2b08ce4d93333eab923bca847493c24c6`, conclusion
`success`, 16:24:35Z to 16:46:08Z:

| Lane | Run 36989364497 (`53ed834e`) | Run 37033729116 (`b7a950c2`) |
| --- | --- | --- |
| Contracts (Node 18.x) | failure: `normalization-producer-missing` | **success** (2m27s) |
| Contracts (Node 24.x) | failure: `normalization-producer-missing` | **success** (2m33s) |
| Build and test (windows-latest) | failure: 41 rows + 4 producers exit 3 | **success** (21m29s) |
| Build and test (ubuntu / macos), MSRV x3, fmt, clippy, Mago | success | success |

I read the job logs rather than trusting the rollup: the Windows
build-test lane **executed** the suite step (`node scripts/run-golden.mjs
--verify`, `test-golden-normalization.mjs`, `test-golden-determinism.mjs`,
`test-golden-diagnostic-coverage.mjs`, `test-golden-planner-e2e.mjs`,
`test-golden-update-policy.mjs` at 16:38:49Z) and printed
`"gate": "golden-normalization"` - i.e. the lane ran the gate that was red
before, and passed it. The Contracts (Node 18.x) lane now runs only
`test-golden-catalog.mjs`, `test-golden-hygiene.mjs`,
`test-golden-adapter-shared.mjs` - the normalization gate is gone from
that job, as intended. This leg is **host-confirmed, not pending**.

## Gates re-run in this checkout (Windows, debug binary, Ajv 8.17.1)

All 10 exit 0, outputs consistent with the fix report:

| Gate | Result |
| --- | --- |
| `test-fixture-provenance.mjs` | ok (64 families / 64 synthetic / 0 evidence-backed) |
| `test-golden-catalog.mjs` | ok (21 cases, 449 rules, 302/114/13/20, ajv 8.17.1) |
| `test-golden-hygiene.mjs` | ok (259 files, 6 controls, 3 host roots) |
| `test-golden-adapter-shared.mjs` | ok (4 shared evidence, 9 `fixture.rs` includes) |
| `test-golden-normalization.mjs` | ok (242 files, producer vectors `newline-crlf-equal-output`, `separator-forward-slash-equal-output`) |
| `run-golden.mjs --verify` | ok (21 cases, 4 byte-identical roles) |
| `test-golden-determinism.mjs` | ok (3 lanes, 41 rows, `manifestDigest sha256:92496524...` unchanged, so the fix did not perturb case outcomes) |
| `test-golden-diagnostic-coverage.mjs` | ok (449/20/114/302/13) |
| `test-golden-planner-e2e.mjs` | ok (6 stages, `stageDetails` with per-stage digests) |
| `test-golden-update-policy.mjs` | ok (11 plan-schema kinds, plan -> review -> apply) |

`git status` clean after all runs.

## Findings

### R3-1. MAJOR (pre-existing, survives fix round 2) - the deliberate update flow cannot `plan` any of the 20 `diagnostic.*.pair` cases

`scripts/update-golden-case.mjs:129-135` emits `declaredPath: null` for
any case whose descriptor declares no `expected` outputs:

```js
const declared = entry.descriptor.expected ?? [];
const candidateFiles = cold1.map((row, index) => ({
  path: declared.length > 0 ? `expected/${declared[index].role}` : `expected/${row.project}.envelope.json`,
  declaredPath: declared.length > 0 ? declared[index].path : null,
  ...
```

but `tests/fixtures/suite/schema/golden-update.schema.v1.0.0.json:138-142`
constrains the field to a string:

```json
"declaredPath": { "type": "string", "pattern": "^(?!.*(^|/)\\.\\./)(?!^/)[A-Za-z0-9._][A-Za-z0-9._/ -]*$", "maxLength": 256 }
```

`declaredPath` is **not** in `after.files[].required` (so it may be
omitted), but when present it must be a string, and `null` is rejected.
The same validation runs in **both** phases
(`update-golden-case.mjs:305-309` in `plan`, and again in `apply`), so the
flow dies at plan time with:

```json
{"reason": "plan-schema", "errors": [
  {"instancePath": "/after/files/0/declaredPath", "keyword": "type",
   "params": {"type": "string"}, "message": "must be string"}, ...]}
```

**Evidence (control run against a full clone of the tracked tree in a
scratch sandbox, `LEKALO_GOLDEN_BINARY` pointed at the real debug
binary):**

- Plan **all 21** catalogued cases with a real byte change to the case's
  own trigger project: `planOk=0`, `schemaFailDeclaredPath=20`,
  `other=1` (the 21st is `minimal.project`, which has no
  `trigger/lekalo/project.yaml` and was handled by a separate control).
- Every one of the 20 failures is the identical `plan-schema` /
  `declaredPath must be string` violation.
- Descriptor survey: `casesWithDeclaredExpected=1`, `casesWithout=20`.
  Only `minimal.project` declares an `expected` array; the 20 pair
  descriptors carry only `inputs` + `expectation`.
- **A pristine plan (no byte change at all) also exits 1** for
  `diagnostic.type-recursion.pair`, because `after.files` is built from
  the digest-diff filter and still carries the `null` rows. So the flow
  is not merely fragile, it is unconditionally unusable for these cases.
- `minimal.project` plans cleanly (exit 0), which is why this survives.

**Why no gate catches it.** `scripts/test-golden-update-policy.mjs:101`
rehearses exactly one case:

```js
const plan = run(["plan", "--case", "minimal.project", "--reason", ...]);
```

A grep for `--case` in that gate yields only `minimal.project`. The gate
therefore validates the flow exclusively against the single case whose
descriptor declares `expected`, so the `null` branch is never executed.
This is the same class of blind spot as the two R2 blockers (a gate
exercising only the path that works), just non-blocking because CI never
invokes the update flow (`test-golden-update-policy.mjs:34-49` asserts
that no workflow references a writer).

**Attribution.** The `null` emission predates the fix rounds (present at
`b33a4754`); the `declaredPath` schema field was added in **fix round 1**
(`a68f9654`) to close codex C7/D7 ("three docs violate closed schemas").
So the *conflict* was introduced when the closed-schema field was added
without also widening it to admit `null`, and fix round 2 neither
touched nor mentioned it. R2's own review recorded the addition as
"alignment, not loosening", which was correct as far as it went, but
nobody exercised the no-`expected` branch.

**Impact.** AC5's deliverable is a *deliberate* update flow with a
reviewed semantic summary. For 20 of 21 registered cases there is no
working path to regenerate an expected envelope: a maintainer who
changes a pair fixture's semantics hits a schema error instead of a
reviewed diff. Note the semantic bound is intact, because `apply`
independently refuses a `null`/missing destination with
`unmapped-candidate` (`update-golden-case.mjs:342-345`). This is an
availability defect in the authoring path, not a write-escape.

**Suggested fix (either is defensible).** Either (a) omit the key rather
than setting `null` when `declared.length === 0`
(`...(declared.length > 0 ? { declaredPath: ... } : {})`), keeping the
closed schema tight and letting `apply`'s existing `unmapped-candidate`
refusal stand; or (b) widen the schema to `["string","null"]`. (a) is
preferable, since it preserves the closed contract and the
destination-binding guarantee. Either way, add a pair case to the
update-policy rehearsal so the branch is covered.

### R2-1. FIXED - normalization gate moved to `build-test`, after `cargo build`

`ci.yml`: `node scripts/test-golden-normalization.mjs` removed from the
contracts catalog step and added to the build-test "Run the golden
fixture suite gates" step, which follows `cargo build --workspace
--locked`. Both step comments corrected: the contracts comment no longer
claims normalization is Node-only. The gate also gained `LEKALO_BIN`
redirection (`test-golden-normalization.mjs:130-131`), matching its
siblings, and still fails closed with `normalization-producer-missing`
when the binary is absent. Hosted contracts lanes green.

### R2-2. FIXED - `realpathSync.native` at every suite scratch root

All seven sites use `.native`; a grep for `realpathSync` across the
suite scripts finds zero plain calls on any suite scratch root:
`run-golden.mjs:84`, `test-golden-determinism.mjs:57`,
`test-golden-normalization.mjs:138`, `test-golden-planner-e2e.mjs:43`,
`test-golden-update-policy.mjs:74` (now wrapped, with the honest comment
citing the 8.3 contract), `update-golden-case.mjs:59`,
`update-golden-run-manifest.mjs:42`. Hosted evidence: the Windows lane
where run 36989364497 denied all 41 rows + 4 producers at
`structure.selection-alias` is green on 37033729116. (The alias path
remains non-reproducible on a checkout with 8.3 creation disabled, as
both R2 reviews noted, but the hosted A/B now confirms it.)

### R2-3. FIXED - `minItems: 2` restored; manifest regenerated as two equal lanes

`run-manifest.schema.v1.0.0.json` `runs.minItems` is back to `2` (the
only schema change in the round, and it *restores* a bound). Verified
independently with Ajv 8.17.1: `manifestValid=true`,
`lanes=cold-1:41,cold-2:41`, `lanesRowEqual=true`. Two negative controls
confirm the restored contract is load-bearing rather than decorative: a
one-lane manifest is **refused** (`oneLaneRefused=true`), and the
writer's lane-equality rule catches a per-stream digest drift between
lanes (`laneEqualityCatchesDrift=true`). The determinism gate still
cross-checks the committed `cold-1` rows live, and its `manifestDigest`
is unchanged at `sha256:92496524...`.

### R2-4. PARTIALLY FIXED - residual honestly documented

`passGate` now emits `stageDetails` (per-stage `ok` + detail including
`loadDigest`/`graphDigest`/`contextDigest`/`diffDigest`) on the success
path, so stage digests are observable when the gate passes. Stages 4-5
remain canned and are now **labeled as documented residuals in the gate
header** with the reason (stage 4 uses the catalog-registered shared
scenario corpus; a chain-built trace needs the G05 producer work from the
research). I ran the gate: 6 stages, all `ok`, with real digests present
in `stageDetails`. The labeling is honest, which is what R2 asked for.

### R2-5. FIXED (for the one reachable case) - `apply` refreshes the sidecar in the same write

`update-golden-case.mjs:380-403` re-walks the case directory and rewrites
`checksums/<caseId>.json` immediately after publishing the expected
bytes, and reports `checksumsRefreshed: true`. I exercised it end-to-end
rather than reading it: in a scratch clone, mutating a case-owned source
file turns the catalog gate **red** (`checksums-digest-drift`), then
`plan` then `apply` with the correct digest returns `applied: 2,
checksumsRefreshed: true`, and the catalog gate goes **green** again with
the refreshed sidecar. So the fix is real and observable, but see R3-1:
this is only reachable for `minimal.project`, and its sidecar covers
`minimal/project/**` only (the `minimal/expected/**` outputs it writes are
outside the sidecar's case dir), so for that case the refresh is
belt-and-braces rather than load-bearing. It would become load-bearing
for a pair case, whose sidecar *does* pin `expect.json`, if R3-1 were
fixed.

### R2-6. MOSTLY FIXED - implementation counts corrected, three stale numbers remain

Corrected and verified against the tree: 21 cases (`:29`), two-lane
manifest with 41 rows each (`:30`), 3 lanes x 21 cases / 41 rows
(`:136`), 4 byte-identical roles. Confirmed: `catalog.json` has 21 cases,
`run-manifest.json` has 2 x 41 outcomes, `--verify` reports
`byteCompared: 4`.

Still stale in `docs/m7/issue-90-implementation.md`:

- `:62` "the 19 pairs + descriptors" - there are **20** pair cases
  (`(Get-ChildItem tests/fixtures/suite/v1/diagnostics -Directory).Count`
  = 20; the coverage gate reports `suitePairs: 20`).
- `:138` "all 19 suite pairs execute both polarities freshly" - same, 20.
- `:152` "`test-golden-normalization.mjs` - ok (214+ files)" - the gate
  reports **242** files.

Minor and cosmetic, but they are the exact drift class R2-6 was raised
for, so they should be swept in the same pass.

## Regression / gate-weakening scan

No weakening found. Every script change in `53ed834e..b7a950c2` is
additive or strengthening: native realpath at seven sites, `LEKALO_BIN`
redirection, sidecar refresh on apply, `stageDetails` in the receipt,
lane-equality plus a second cold lane in the manifest writer. The only
schema change *restored* a bound (`minItems` 1 to 2); no assertion was
removed, no leg skipped, no new exemption, and the normalization gate
still executes on every OS (now where its prerequisite exists). The
rewritten manifest writer kept its per-row expectation check
(`expectation-violation`) and *added* a cross-lane check.

## Observations (not findings)

- The determinism `warm-cache` lane still runs `--no-cache` like the cold
  lanes; the name overstates what it exercises. Pre-existing, unchanged.
- `apply`'s sidecar re-walk duplicates `update-golden-checksums.mjs`
  rather than sharing it; outputs are identical by inspection.
- `update-golden-run-manifest.mjs` rows key stdout as `outputDigest`
  while determinism rows use `stdoutDigest`; the cross-check maps them
  correctly. Pre-existing naming asymmetry.
- `minimal.project`'s sidecar does not cover `minimal/expected/**`, so
  the 4 byte-compared golden outputs are unpinned by checksums (they are
  pinned by `--verify` byte comparison, so this is defence-in-depth, not
  a hole). Worth a deliberate decision rather than an accident.

## Acceptance-criteria deltas vs round 2

| # | Criterion | Verdict |
| --- | --- | --- |
| AC1 | Byte-stable repeat runs | **Met.** Unchanged and re-verified; the manifest is now two genuinely equal lanes under a restored `minItems: 2`. |
| AC2 | LF/CRLF + path-separator normalization | **Met.** Gate correctly wired to the job that builds the CLI; hosted contracts + all three OS build-test lanes green. |
| AC3 | Positive AND negative per rule | **Met.** 20 pairs, 449 rules, 302 witness rows. |
| AC4 | Adapter conformance reuses shared fixtures | **Met.** Unchanged, green. |
| AC5 | Deliberate update with reviewed semantic summary | **Not met for 20 of 21 cases** (R3-1). The flow's safety properties are intact and `minimal.project` works end-to-end, but a maintainer cannot regenerate a pair case's expected envelope at all. |
| AC6 | Planner fixture covers P0 end-to-end chain | **Partially met**, unchanged: stages 1-3 linked, stages 4-5 canned but now honestly labeled (R2-4). |
| AC7 | No secrets / host absolute paths | **Met.** Unchanged, green. |
| - | Fixtures registered; existing fixtures untouched | **Met.** Diff still additive apart from `ci.yml` + `fixture-provenance.json`. |
| - | CI green | **Met.** 37033729116 success, 11/11. |

## Bottom line

The fix round did what it claimed: both blockers are closed in the diff
*and* confirmed on the hosted runner (no pending leg this round), the
schema relaxation was reverted rather than accepted, the apply write is
self-contained, the manifest carries two genuinely equal lanes, and the
one true residual is labeled honestly. On the round-2 scope alone this
would be an ACCEPT.

I am returning ISSUES for one reason: R3-1. Verifying R2-5 honestly meant
running the update flow, and running it revealed that the flow is dead for
20 of the 21 cases in the catalog, a defect introduced when fix round 1
added `declaredPath` to the closed plan schema without admitting the
`null` the producer emits, missed by both R2 reviews because the
update-policy gate rehearses only the one case that works. It is not a
regression from fix round 2 and it does not affect CI, so the two blockers
are genuinely closed. But it is a real gap in a shipped AC5 deliverable
and it should be fixed (one-line producer change + one added gate case)
rather than discovered later by the next maintainer who tries to update a
pair fixture. R2-6's three residual doc numbers should ride along in the
same sweep.
