# Issue #90 — round-2 review (verify fix round 1)

Reviewer: independent dispatch (Devin), read-only review of branch
`ichinya/m7-issue-90` at `53ed834e`. Base: `origin/ichinya/M7`
(`9510dd07`). Fix commits reviewed: `3d3dd19d` + `a68f9654` (scripts,
schemas, fixtures, CI) and `53ed834e` (fix report
`docs/m7/issue-90-fix1.md`). Inputs re-verified against:
`docs/m7/issue-90-review-devin.md` (D1–D11) and
`docs/m7/issue-90-review-codex.md` (C1–C9).

**Verdict: ISSUES.** Fix round 1 genuinely fixed the substance of every
code/evidence finding — `--verify` now byte-compares real producer
output, checksums are real and gate-verified, coverage has no unverifiable
rows, the update flow is digest-bound end to end, and the planner chain
is honestly linked through stage 3. But the hosted CI on the exact fix
SHA is still red: the Windows alias fix used the non-native
`realpathSync` the codebase explicitly documents as insufficient (C2
persists — blocker), and the fix round itself made the normalization
gate require the cargo binary while leaving it in the contracts job
that never builds one (new blocker, same class as the original C1).

## Hosted CI evidence (run 36989364497 on `53ed834e`, read-only)

| Job | Failed step | Root cause |
| --- | --- | --- |
| Contracts (Node 18.x) | Run the golden fixture suite catalog gates | `normalization-producer-missing: cargo build -p lekalo-cli --locked first` |
| Contracts (Node 24.x) | Run the golden fixture suite catalog gates | identical |
| Build and test (windows-latest) | Run the golden fixture suite gates | `golden-run`: every one of the 41 case rows plus all 4 minimal producers exited 3 |

Linux/macOS build-test, fmt, clippy and the remaining jobs passed.

## Findings

### R2-1. BLOCKER (new) — normalization gate fails closed in the contracts job, which never builds the CLI

`scripts/test-golden-normalization.mjs:130-133` now (correctly, for C9)
fails closed when `target/debug/lekalo` is absent: it pushes
`normalization-producer-missing` into `errors` and `failGate` exits 1.
But the gate still runs at `.github/workflows/ci.yml:110` inside the
`contracts` job, whose only cargo invocation is
`cargo test -p lekalo-core target_protocol_conformance` (ci.yml:81) —
`lekalo-core` has no bin target, so `target/debug/lekalo` never exists
in that job. The step's own comment ("Catalog/normalization/hygiene/
adapter-shared are Node-only") is now false for normalization, and the
gate header's "CI runs this gate only after the build" is contradicted
by the wiring. Verified live: with `target/` hidden, the gate exits 1
with exactly `normalization-producer-missing`; on the hosted run both
contracts lanes died on this line. The fix correctly recognized the
binary dependency for `test-golden-update-policy.mjs` (moved to
build-test, after `cargo build --workspace`) but missed normalization —
the fix for C9 introduced the dependency, the wiring did not follow.
Move the gate (or just its §6 producer-vector section) into the
build-test suite step, or provision the binary in contracts.

### R2-2. BLOCKER (C2 persists) — `realpathSync` without `.native` does not expand the `RUNNER~1` temp alias; hosted Windows still fails every suite invocation

Codex C2 prescribed `realpathSync.native`, "following the existing
solution in `scripts/test-run-history-cli.mjs:94`". That file carries
the exact contract: "The libuv (native) realpath is required: the
default JS realpath keeps an 8.3-spelled input as written."
Every fix-round scratch root used plain `realpathSync` instead:
`run-golden.mjs:84`, `test-golden-determinism.mjs:57`,
`update-golden-case.mjs:59`, `test-golden-planner-e2e.mjs:39`,
`update-golden-run-manifest.mjs:38`, `test-golden-normalization.mjs:137`.
On run 36989364497 the Windows lane reproduces the reviewed failure:
`golden-run` exits 1 with all 41 rows and all 4 `--verify` producers at
exit 3 (the deny-class exit; `structure.selection-alias` fires before
command logic). In the same job, `test-run-history-cli.mjs` — using
`.native` — passed, a clean A/B on the same runner. Sufficient fix is
mechanical: `realpathSync.native` at every suite scratch root (and
`test-golden-update-policy.mjs:71`, currently unwrapped, for
consistency, though the inner `sandboxBase` is the CLI-facing path).

### R2-3. MINOR — `run-manifest.schema` relaxed `minItems` 2 → 1 to fit the committed document

C7 asked to "align producers and versioned contracts". For the coverage
schema that meant pointing `generatedBy.script` at the real producer
(correct). For run-manifest, alignment was achieved by weakening the
contract: the committed manifest carries one lane (`cold-1`, 41 rows)
and `runs.minItems` was lowered 2→1
(`tests/fixtures/suite/schema/run-manifest.schema.v1.0.0.json`). The
determinism gate still proves 3-lane equality live, so enforcement is
not lost — but the versioned contract was loosened to accommodate data
rather than the document regenerated to satisfy it. Recorded for
honesty; either regenerating a multi-lane manifest or accepting the
single-lane pin deliberately is defensible.

### R2-4. MINOR — planner-e2e stages 4–5 still run canned corpora, and stage digests are computed but never emitted

Stages 1–3 are now genuinely linked (fix verified): one sandbox project
flows load → IR → graph → inspect/impact/context with the symbol chosen
from that project's own IR, and the diff is a real self-mutation whose
affected seeds must name `focus_task`. But stage 4
(`scenario-lane-compile-and-run`) still executes the committed
`test-node-scenario-tests.mjs` corpus with no upstream input, and stage
5 still validates the committed `planner.trace.json` rather than a
trace built from this run — the two substitutions codex C5 flagged
remain, now honestly labeled in the header rather than overclaimed.
Also, per-stage digests (`loadDigest`, `graphDigest`, `diffDigest`,
`contextDigest`) are collected into `stages[].detail` but
`passGate` emits only stage names — "stage digests returned in the
receipt" is true only on the failure path. Residual of D5/C5, minor.

### R2-5. MINOR — `apply` publishes bytes but does not refresh the revision/checksum witnesses in the same reviewed operation

C6 asked that "revisions/checksums/review evidence" update as part of
the reviewed operation. Apply now correctly binds candidate digests,
confines destinations to declared expected outputs, and preflights all
reads before writing — but it still writes only `expected/` files;
`catalog.json` revision and `checksums/<case>.json` sidecars remain
stale until `update-golden-checksums.mjs`/catalog bump are run as a
follow-up, and the catalog gate will flag `checksums-digest-drift` in
the interim. Workable as a documented regenerator flow, but the
reviewed write is not self-contained.

### R2-6. MINOR — implementation report drift survived the fix round

`docs/m7/issue-90-implementation.md` still says "exact case inventory
(20 cases)" (now 21) and "3 lanes x 20 cases (39 rows)" (now 21
cases / 41 rows). The round-1 finding text D11 asked the claims to
match; the count that mattered ("all 20 `semantic.*` rules") became
true by adding the 20th pair rather than editing the claim — fine —
but the case/row counts are now stale.

## Verified fixed (with live evidence)

| # | Finding | Evidence in this checkout |
| --- | --- | --- |
| D1 | catalog gate in CI w/o Ajv env | dedicated contracts step `ci.yml:101-112` sets `LEKALO_AJV_NODE_PATH` + `NODE_PATH`; catalog ran green in the hosted contracts step before the normalization failure |
| C1 (Ajv part) | update-policy needs built CLI | moved to build-test `ci.yml:234-247`, after `cargo build --workspace` (`ci.yml:192`), env set; see R2-1 for the normalization counterpart it missed |
| D2 / C3 | `--verify` dead | `run-golden.mjs:43-49` `ROLE_PRODUCERS` executes each declared role's real argv; `cli-json-lf` compared via raw `Buffer.equals`; live control: appending one byte to `ir-envelope.json` flipped `run-golden --verify --case minimal.project` to `byte-drift`, restored → exit 0 |
| C3 | determinism hides newline/channel | `test-golden-determinism.mjs:126-127` pins `stdoutDigest`/`stderrDigest` separately on raw bytes; manifest `outputDigest` aligned to stdout |
| D3 | PLACEHOLDER sidecars unread | `update-golden-checksums.mjs` wrote real sha256 sidecars for all 21 cases; catalog §9 verifies identity/revision/membership/digests; live control: appending a byte to `minimal/project/lekalo/project.yaml` flipped the catalog gate to `checksums-digest-drift`, restored → exit 0 |
| D4 / C4 | 10 `UNMAPPED` counted as test-witness | coverage index carries 0 `UNMAPPED` strings; all 10 remapped to real paths (spot-verified on disk); `semantic.portable-target-reference` gained a real 20th suite pair (advisory, exit 0); generator now throws on unmapped rules; catalog rejects `UNMAPPED`/`node:`/`cargo:` witness gates and requires on-disk existence |
| D5 / C5 | planner under-delivers | stages 1–3 linked over one sandbox project; inspect/impact/context execute; self-mutation diff requires `focus_task` seeds; residual stages 4–5 canned (R2-4) |
| C6 / D8 | apply unbound | `update-golden-case.mjs:341-374`: per-file `declaredPath` must be in the descriptor's declared expected set + `assertRepoPath`, candidate bytes digest-preflighted, all reads before any write, single-pass publish; plan digests cover exact candidate bytes (no added LF) |
| C7 / D7 | three docs violate closed schemas | catalog gate now Ajv-compiles coverage + run-manifest schemas against committed docs (`test-golden-catalog.mjs:60-61,166-179`); produced/accepted plans validated via `lib/golden-schema-validation.mjs`; `declaredPath` added to the plan schema as an optional field with a no-traversal pattern — alignment, not loosening; see R2-3 for the one actual relaxation |
| C8 | hash-only summary / equal digests | `before.digest` binds tracked preimages, `after.digest` binds candidate bytes; `describeEnvelopeDelta` emits field-wise semantic deltas (status/reason-code/diagnostic-count/severity) printed to stderr; the policy gate rehearses a real `project.yaml` mutation and fails a digests-only summary |
| C9 / D10 | normalization tautologies | §6 executes a materialized CRLF variant + forward-slash nested selector through the real binary and requires byte-identical output (`producerVectors` in the receipt); §4 premise honestly recorded; hygiene now scans all 259 files including every `test-golden-*`/`update-golden-*`/`gen-suite-*`/`run-golden` script |
| D6 | generators outside plan/apply | `UPDATE_RECIPES` = the five real scripts; README documents them as reviewed regenerators; update-policy fails any workflow referencing `update-golden*`, `gen-suite-coverage`, `gen-suite-diagnostic-pairs` |
| D7 | dead surface | `RUNNER_ARGS` deleted; all 21 descriptors use `cli-validate` (the unexecuted ids are unused declarations); schemas now compiled |
| D9 | tautological self-check | replaced by full-suite before/after digest snapshot (`suiteTreeDigests`) |
| D11 | doc drift / empty dirs | README rewritten to the delivered tree with honest F07–F14 note; example id corrected; the 8 empty dirs are gone |

## Gates re-run in this checkout (Windows, debug binary, ajv 8.17.1 via LEKALO_AJV_NODE_PATH)

All green and consistent with the fix report's claimed outputs:
provenance (64/64/0), hygiene (259 files, 6 controls), adapter-shared,
catalog (21 cases, 449 rules, 302/114/13/20), normalization (242 files,
2 producer vectors), `run-golden --verify` (4 byte-identical roles),
determinism (3 lanes, 41 rows, `manifestDigest sha256:92496524…`),
diagnostic-coverage (449/20/114/302/13), planner-e2e (6 stages),
update-policy (11 kinds). Two negative controls reproduced live
(byte-drift, checksums-digest-drift). `git status` clean; diff vs base
is still 100% additive apart from `ci.yml` + `fixture-provenance.json`
(266 files, +16705/−0).

## Observations (not findings)

- The determinism `warm-cache` lane still runs `--no-cache` like the
  cold lanes — the name overstates what it exercises (it is a third
  independent cold run).
- `run-golden.mjs:246-258` "mutation control" is a tautological
  in-memory `Buffer.equals` self-check; the real byte-drift proof is
  what the corrupted-golden control demonstrates (verified above).
- `test-golden-update-policy.mjs:86` copies `scripts/lib` wholesale so
  `golden-schema-validation.mjs` reaches the scratch copy — correct,
  but `loadAjv`'s uncaught throw (no env, no local ajv) reports a bare
  stack rather than a typed gate error.
