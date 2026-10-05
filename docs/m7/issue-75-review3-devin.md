# Issue #75 — independent review, round 3 (Devin)

Reviewer: independent dispatch (Devin), verification of the fix-round-2
delta only. Branch `ichinya/m7-issue-75`, base `origin/ichinya/M7`
(merge-base `9510dd07`), HEAD `c9a531a1` (fix-2 delta
`3cb77419..c9a531a1`: 17 files, +1180/−367 — the stranded 13-file diff
completed and committed by the respawned worker, plus
`docs/m7/issue-75-fix2.md`). Verified against the round-2 reports
(`issue-75-review2-devin.md` R2-1..R2-9, `issue-75-review2-cline.md`
B1/M1/m1–m3). Live reproductions used the real binary rebuilt at this
HEAD (`cargo build -p lekalo-cli --locked`) and pinned Ajv 8.17.1
(`LEKALO_AJV_NODE_PATH`/`NODE_PATH`, Node 24.13.0, Windows).

## Verdict: ACCEPT

All twelve round-2 findings — the blocker, two majors (converged), and
nine minors/nits — are fixed and verified live. The gate now runs
where its binary exists, both provenance pins carry digests of the
exact consumed bytes, `--all` covers every non-module definition kind,
LEK-CONTEXT-004 has a real emission site with honest `unsupported`
semantics, shared cost bills to the subject's own row, the scratch
artifacts are gone, reads are bounded, and fixture tempdirs are owned
and cleaned. No gate, schema, policy threshold, or refusal was
weakened.

## Finding disposition

| Finding | Verified |
| --- | --- |
| Cline **B1** — binary-dependent gate ran in the build-free schemas job | FIXED — `ci.yml:181-188` runs the gate in `build-test` immediately after `cargo build --workspace --locked` (:180), with the job's provisioned Ajv; absent binary → exit 1 `binary-missing` (mandatory, no skip path). |
| Cline **M1** / Devin **R2-1** — `provenance.policy`/`baseline` dead on the wire | FIXED, live — `lekalo --json context-budget --symbol planner.focus_task --budget 200` emits `provenance.{policy,baseline}` with the closed `{state,digest?}` shape; `--baseline <file>` reports `state:"known"` with `digest = sha256:<consumed bytes>` (new gate legs `baseline-pin`, `baseline-pin-digest`; malformed pins refused). |
| Cline **m1/m2** / Devin **R2-2** — committed `.fix-*.mjs` codemods + `cb-gate-probe/baseline.json` | FIXED — all four files deleted; no directory, no references. |
| Devin **R2-3** — `--all` excluded definition kinds `--module` covers | FIXED, live — `--all` selects 29 subjects covering every non-module kind (effect, endpoint, entity, event, operation, policy, project, requirement, scenario, target-binding, type), a strict superset of `--module planner`'s 21; the equality assertion `all_scope_covers_every_module_kind` exists. |
| Devin **R2-4** — LEK-CONTEXT-004 never emitted | FIXED, live — `--source-context mapped-files` emits `context.artifact-evidence-incomplete` (exit 0) while the none recipe stays silently `unknown`; emission site `mod.rs:623-630` reports honest `unsupported`. |
| Devin **R2-5** — effective recipe vs profile digest disagreement | FIXED — `source_recipe_selection_binds_digest_and_warns` proves the digest binds the *measured* recipe: a declared mapped-files profile warns, explicit selection over a none profile normalizes + rebinds, none stays unknown. |
| Devin **R2-6** — no independent breakdown reconciliation | FIXED — gate legs `breakdown-reconciles`/`ledger-reconciles` on live and golden: 355 = 221 exclusive + 134 shared; mirrored by core and CLI assertions. |
| Devin **R2-7** — dead metric helpers; collector tested via supplied map | FIXED — `metrics::dependency_counts`/`semantic_counts` deleted; `supporting_requests_collector_counts_real_edge_occurrences` drives the production collector over the compiled fixture and reconciles to `edgeOccurrences`. |
| Devin **R2-8** — auxiliary unbounded policy reads | FIXED — `read_bounded` (`file.take(MAX_INPUT_BYTES + 1)`) performs the single bounded read feeding regression limits, digest, and evaluation on both policy and baseline paths; oversized-input refusal tested for all three inputs. |
| Devin **R2-9** — duplicate marker write + intentionally leaked tempdirs | FIXED — no `keep()`/marker writes remain; `FixtureCopy` owns the TempDir for the test lifetime; `fixture_copy_cleans_up_after_children_finish` asserts presence during children and removal after drop. |
| Cline **m3** — shared tokens billed to an arbitrary dependency | FIXED — `shared-bills-the-subject`/`shared-only-on-the-subject-row` gate legs; golden verified: subject row shared=134, every other row shared=0. |

## No-weakening check

Delta scope is the six-module implementation surface, the gate script
(+155 lines → 450, 65 fail legs), CI placement, the regenerated golden,
and the reports. The only schema wire change is the requested baseline
pin repair (bare state enum → closed digest-bearing object; `known`
requires the sha256 grammar, non-known forbids a digest,
additionalProperties closed — verified in the committed golden and the
gate's negative legs). Contract version stays 0.6.3 per
`docs/versioning.md`; the base-relative version gate passes (99
artifacts). Tests grow 13 → 16 (CLI) and 53 → 57 (core) with no
`#[ignore]` or relaxed assertion. `git status` clean.

## Reviewer-run gates (this worktree, Windows, Node 24.13.0, Ajv 8.17.1)

```text
cargo build -p lekalo-cli --locked                                PASS (fresh)
cargo test -p lekalo-cli --test context_budget --locked           16/16 PASS
cargo test -p lekalo-core --lib context_budget --locked           57/57 PASS
node scripts/test-context-budget-contracts.mjs                  PASS
  {"liveChecked":true,"registryEntries":457,"contextRules":8}
node scripts/test-context-contracts.mjs                          PASS (13 facts)
node scripts/test-fixture-provenance.mjs                         PASS (64 families)
node scripts/check-contract-versions.mjs --base origin/ichinya/M7 PASS (99 artifacts)
cargo fmt --all -- --check                                        PASS
cargo clippy -p lekalo-cli -p lekalo-core --all-targets
  --locked -- -D warnings                                         PASS
git diff --check origin/ichinya/M7..HEAD                          PASS
```

Live binary probes (fixture `tests/fixtures/context-budget/planner`):
symbol report emits `provenance.policy/baseline` closed pins;
`--baseline` round-trip binds `sha256` of consumed bytes; `--all`
selects the full 29-node non-module inventory; `--source-context
mapped-files` emits `context.artifact-evidence-incomplete` +
`context.budget-exceeded`; golden reconciles 355 = 221 + 134 with the
shared bucket on the subject row only.

## Bottom line

Landable. Fix round 2 completes every round-2 finding — including the
recovery of the interrupted worker's diff — with the gate moved to the
binary-producing job, honest provenance pins on the wire, full `--all`
coverage, the first real LEK-CONTEXT-004 emission site, subject-owned
shared attribution, bounded reads, and owned fixture tempdirs. All
gates re-run green on this HEAD.
