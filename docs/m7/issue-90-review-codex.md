# Issue #90 independent Codex review

**Verdict: ISSUES. Findings: 2 blocker, 7 major, 0 minor.**

Reviewed candidate: `326f8a3755d7381aedc5e61680b706dccaee726b` on
`ichinya/m7-issue-90`; implementation tip `224488a977fa2fc263747fbdd513d5afdeb5c536`.
Diff base: `origin/ichinya/M7` = `9510dd0767a56c0ab34b8d3c8ceb2a14db8de825`.
The candidate matched the remote branch when the review started. Research
(`c255c892`, `docs/m7/issue-90-research.md`) and the implementation report were
read before inspecting the implementation. The live issue #90 remains open.
The supplied review scope permits explicitly justified evidence states; even
under that allowance, fabricated or unmapped witnesses cannot satisfy coverage.

This is a review-only change. All mutation probes used external temporary copies;
no implementation, fixture, existing review, or workflow was edited. The
234-file baseline diff contains 14,272 additions and no deletions; the only
modified pre-existing implementation paths are additive CI wiring and the suite
entry in fixture provenance.

## Numbered findings

1. **blocker — Contracts CI cannot run the new gates with its current prerequisites.**
   Evidence: `.github/workflows/ci.yml:101` and `:104`,
   `scripts/test-golden-catalog.mjs:18`,
   `scripts/test-golden-update-policy.mjs:81`.
   Ajv is installed outside the checkout, but its lookup environment is scoped
   to the earlier schema-release step (`ci.yml:33`); the later contract-checker
   step has neither `LEKALO_AJV_NODE_PATH` nor `NODE_PATH`. On the exact reviewed
   SHA, both Node 18 and Node 24 contracts jobs fail with
   `ajv-8.17.1-unavailable: Cannot find module 'ajv/dist/2020.js'`.
   There is a second prerequisite failure behind that first error: the
   update-policy gate launches the real CLI, although this job has only built
   `lekalo-core` tests, not `target/debug/lekalo`. A scratch checkout without
   that binary fails the policy gate with `binary-missing`.
   Supply the pinned dependency environment to the catalog step and move the
   binary-dependent policy rehearsal to a job that builds the CLI, or provision
   that binary explicitly. Calling this policy gate Node-only in the report is
   incorrect.

2. **blocker — New runners inherit Windows temp aliases and fail the entire hosted Windows lane.**
   Evidence: `scripts/run-golden.mjs:65`, `:129`,
   `scripts/test-golden-determinism.mjs:57`,
   `scripts/update-golden-case.mjs:58`,
   `scripts/test-golden-planner-e2e.mjs:39`.
   These scripts use `mkdtempSync(tmpdir())` spellings directly as subprocess
   working directories. GitHub Windows temp paths can contain the `RUNNER~1`
   alias, which the production selection policy correctly refuses before
   semantic validation (`crates/lekalo-core/src/project_fs.rs:610`).
   The reviewed SHA's Windows CI log contains **39**
   `structure.selection-alias` results with status `denied`, exit 3, instead of
   the expected valid/invalid results; the suite step exits 1 immediately.
   This is an observed hosted failure, despite successful local Windows runs.
   Resolve created scratch directories with `realpathSync.native` before using
   them as cwd, following the existing solution in
   `scripts/test-run-history-cli.mjs:94`, and qualify every new subprocess lane
   on hosted Windows. Preserve the production alias-denial policy.

3. **major — `--verify` does not verify goldens, and declared minimal output roles/checksums are unwired.**
   Evidence: `scripts/run-golden.mjs:40`, `:126`, `:137`,
   `tests/fixtures/suite/v1/minimal/project/fixture.json:21`,
   `tests/fixtures/suite/v1/checksums/minimal.project.json:6`,
   `scripts/test-golden-catalog.mjs:121`.
   The `verify` flag is assigned and never read. The runner executes only
   default-profile `validate`; it never produces or compares the minimal case's
   load, IR, strict-validation, or graph golden roles. The catalog checks
   optional descriptor digests, but the shipped descriptor provides none, and
   its checksum sidecar contains eight literal `PLACEHOLDER` values that no
   gate checks. In a complete scratch copy, replacing the minimal IR golden
   with `{"status":"CORRUPTED-GOLDEN"}` leaves both `run-golden --case
   minimal.project --verify` and the catalog gate green.
   The determinism gate pins only validation envelopes and cannot cover those
   missing roles. It also hashes concatenated, trimmed stdout/stderr
   (`test-golden-determinism.mjs:98`), hiding trailing-newline changes and
   channel swaps despite the `cli-json-lf` byte contract.
   Execute every declared role through its actual producer, compare raw streams
   using the declared framing, and recompute real input/output checksum domains.
   Add corrupt-golden and newline/channel mutation controls.

4. **major — The 449-rule coverage count accepts fabricated evidence and does not prove the stated coverage.**
   Evidence: `scripts/test-golden-catalog.mjs:208`,
   `scripts/test-golden-diagnostic-coverage.mjs:78`,
   `scripts/gen-suite-coverage.mjs:554`,
   `tests/fixtures/suite/v1/coverage/diagnostic-rules.json:150`, `:3297`, `:3806`.
   The counts are accurate: 449 active registry rules, 19 suite pairs, 109
   family-fixture rows, 308 test-witness rows, 13 interaction-only rows. Their
   meaning is overstated. Ten test-witness rows literally name `UNMAPPED —
   acceptance gap`, including `semantic.portable-target-reference`, so the
   report's claim that all 20 semantic rules are paired is false. The
   `php-routes.join-invalid` witness points to nonexistent
   `scripts/test-php-routes-contracts.mjs`. The catalog only requires a
   nonempty witness array, and the coverage gate does not execute or resolve
   those entries. Replacing `adapter.check-failed`'s witness with
   `node:scripts/DOES-NOT-EXIST.mjs` in scratch still passes the catalog gate.
   Existing family rows are checked for path existence, without a positive/
   negative pair or a receipt proving that the referenced case emits this
   specific rule; some rows reference whole invalid directories. The supposed
   drop/corrupt negative controls are ordinary checks, not injected controls.
   Resolve and run named witnesses, bind both polarities to rule-specific
   receipts, and make unjustified/unmapped states fail acceptance. Keep genuinely
   justified interaction states explicit rather than counting backlog as proof.

5. **major — The planner gate runs independent examples, not the P0 end-to-end chain.**
   Evidence: `scripts/test-golden-planner-e2e.mjs:83`, `:98`, `:107`, `:127`,
   `:137`; `docs/m7/issue-90-research.md:239`.
   Compiled IR is used only to check identity and definition count. Its actual
   bytes never feed adapter generation or scenarios. The graph stage reads an
   entity ID from committed shared IR but never invokes inspect, impact, or
   context. Diff runs separate pre-existing `diff/cases` projects; affected
   seeds are counted and never consumed. The scenario subprocess accepts no
   upstream project/IR argument and materializes its own committed corpus.
   Finally, trace export validates the pre-existing Planner trace golden rather
   than building a trace from this run's model, diff, generated files, and result
   evidence. The computed load digest is returned as local detail and never
   checked downstream. A wrong connection between any two stages can therefore
   leave the six-stage success report unchanged.
   Materialize one linked Planner chain, pass actual produced artifacts and
   revisions downstream, run the missing queries, and verify final trace/context
   links with mutation controls. Reusing shared inputs is compatible with this;
   substituting independently successful downstream examples is insufficient.

6. **major — Digest acceptance does not bind published candidate bytes or confine writes to declared outputs.**
   Evidence: `scripts/update-golden-case.mjs:132`, `:200`, `:262`, `:266`,
   `scripts/test-golden-update-policy.mjs:108`.
   Apply verifies the plan file digest and preimages, but never hashes candidate
   files against `plan.after.files[*].digest`. After planning, changing the first
   candidate to `TAMPERED AFTER REVIEW` and accepting the original unchanged
   plan digest succeeds and publishes that text in scratch. Even untampered
   plans have mismatched candidate digests: plan hashes producer stdout, then
   writes stdout plus another LF, so all four minimal candidates fail their own
   declared hash. Apply additionally trusts `file.declaredPath` instead of
   proving membership in the descriptor's expected list. A plan with the first
   destination changed to a scratch `scripts/review-marker.txt`, then explicitly
   digest-accepted, overwrites that unlisted support file successfully.
   Paths are not checked for traversal or symlink escape, and candidate reads
   are interleaved with writes, allowing partial publication on a later failure.
   Preflight every candidate's exact bytes, declared destination and preimage
   before any mutation; enforce descriptor ownership, safe path resolution and
   all-file atomic publication. Exercise tamper, unlisted-path, missing-candidate
   and interrupted-apply controls. Update revisions/checksums/review evidence as
   part of the reviewed operation instead of only replacing output files.

7. **major — Three shipped metadata producers violate their own closed v1.0.0 schemas.**
   Evidence: `scripts/test-golden-catalog.mjs:58`,
   `tests/fixtures/suite/schema/coverage.schema.v1.0.0.json:19`,
   `tests/fixtures/suite/schema/run-manifest.schema.v1.0.0.json:23`,
   `tests/fixtures/suite/schema/golden-update.schema.v1.0.0.json:59`,
   `scripts/update-golden-case.mjs:183`.
   Independent Ajv 8.17.1 validation finds:
   - Diagnostic coverage declares `scripts/gen-suite-coverage.mjs`, while its
     schema requires nonexistent `scripts/update-golden-coverage.mjs`.
   - Committed run-manifest has one lane, while its schema requires at least two.
   - Fresh update plans include `declaredPath` on every after-file, forbidden by
     the closed schema.
   All normal gates nevertheless pass locally because catalog validation
   compiles only catalog/fixture schemas; coverage and manifest readers do not
   compile their schemas; the update-policy gate inspects schema property names
   without validating a produced plan. Align producers and versioned contracts,
   validate every produced/committed document, and add unknown-field controls.
   The existing product-contract version checker being green does not validate
   suite metadata outside `contracts/`.

8. **major — Golden update lacks a semantic summary and its policy gate rejects genuine changes.**
   Evidence: `scripts/update-golden-case.mjs:151`, `:160`, `:178`, `:203`,
   `scripts/test-golden-update-policy.mjs:105`.
   Changing a graph node ID in a scratch preimage yields a summary consisting
   of `bytes-changed-explained` plus truncated hashes and the generic command
   reason. No changed symbol, graph edge, severity, or before/after semantic
   value is shown; stdout prints the plan path/digest, not the semantic summary.
   This is the hash-only summary the research explicitly rejects.
   Moreover, `before.digest` and `after.digest` hash cold-1 and cold-2 candidate
   runs, so they are always equal after the determinism check, even when old
   golden bytes differ. The policy gate then rejects a genuine reason-code or
   graph change because it interprets equal digests as unchanged goldens. Its
   green baseline rehearsal only confirms the no-change case.
   Bind before/after to actual golden preimages/candidates, print readable
   semantic differences, retain review evidence, and test real semantic changes
   plus tampered summaries against the PR base.

9. **major — The normalization gate never exercises newline or separator normalization.**
   Evidence: `scripts/test-golden-normalization.mjs:64`, `:83`, `:125`.
   The gate scans committed paths and LF bytes. Its CRLF section only checks
   that a directory/file exists; the `entities.includes(CR)` branch has no
   assertion or producer execution. It never materializes equivalent LF/CRLF
   inputs, runs either one, or compares outputs; it similarly never exercises
   separator variants through the loader. The purported UTF-8 ordering control
   compares a character's first encoded byte with its code point, not sorted
   key sequences or a production canonicalizer. An invalid minimal project
   remains green in the scratch normalization gate once its ordinary source
   dependencies are present.
   Existing loader unit tests cover CRLF acceptance, and `.gitattributes` pins
   LF, but those facts do not supply the new suite's normalization proof.
   Add producer-based equivalent-input vectors, framing controls, path variants,
   and cross-platform output comparisons, preserving intentional refusal cases.

## Acceptance-criteria checklist

| Criterion | Review result and code/test evidence |
| --- | --- |
| Repeated clean runs are byte-stable | **PARTIAL.** Ran `run-golden --verify` twice: both exit 0, 39 rows, byte-identical receipt stdout. Independently executed all 39 validators twice and compared raw stdout and stderr separately: identical. Determinism gate passes three lanes. These runs do not exercise the declared load/IR/graph goldens or generated output trees; trimming hides framing changes; the warm-cache lane is another `--no-cache` run. Finding 3. |
| LF/CRLF and path-separator normalization | **FAIL.** Static LF/path checks pass for 214 files, but normalization variants are not executed. Hosted Windows fails all 39 results with selection aliases; Linux/macOS build jobs pass. Findings 2 and 9. |
| Every core rule has positive and negative fixture or explicit justified state | **FAIL.** Registry/index join and LEK code/category matching are correct for 449 active entries. Only 19 pair cases execute both polarities. Ten `UNMAPPED` rows, a nonexistent script, unchecked witness names and path-only family evidence invalidate the claimed global coverage. Finding 4. |
| Adapter conformance reuses shared fixtures | **PASS for existing reuse; new proof is limited.** Core `fixture.rs`, Node scenario tests and PHP parity reference shared adapter-conformance IR/orchestration inputs. The added adapter gate passes, and existing PHP parity remains wired at `ci.yml:320`. The new gate checks include paths/existence and same-byte copies; catalog imported evidence has no digest pins, and this gate does not itself execute Node/PHP conformance or detect a formerly equal copy after it diverges. No independent PHP rerun was performed. |
| Deliberate update command, reviewed semantic summary, no automatic CI regeneration | **FAIL overall.** Explicit `plan`/digest-accepted `apply` exists; CI never regenerates tracked fixtures. However, candidate bytes/destinations are unbound, produced plans violate the schema, the summary contains hashes rather than semantic differences, and real changes fail policy rehearsal. Findings 6–8. CI running the scratch update-policy rehearsal is verification, not regeneration of tracked goldens. |
| Planner fixture covers the P0 chain | **FAIL.** Local gate passes six named stages, including the existing six-scenario lane, but queries and stage digest links are absent and final trace is canned. Finding 5. |
| No secrets or host-specific absolute paths | **PASS for inspected committed suite content.** Hygiene gate passes over 217 inputs, with six hostile controls and three current host roots; no actual secret/host path was found in the suite. Fresh actual outputs are not scanned, and the scanner's path patterns are narrower than the research requires; this pass is not proof of every potential generated artifact. |
| Fixture provenance registration | **PASS.** Only the `suite` synthetic entry was added; unchanged provenance gate reports 64 synthetic families, zero evidence-backed families. |
| Existing fixtures/gates retained; no scope creep | **PASS.** Existing fixture bytes and scripts are unchanged. Only provenance and CI wiring modify existing implementation files. Historical compatibility fixtures remain present. |
| Closed contracts, version pins and CLI envelopes | **PARTIAL.** Suite schemas/identities are explicitly versioned 1.0.0; Model/IR 0.2.16 and registry 0.4.0 remain pinned, with no product-contract changes. `check-contract-versions --base origin/ichinya/M7` and existing versioning gate pass. Three suite metadata documents violate their schemas. Valid/invalid status and exit checks execute locally, but stdout/stderr are concatenated and full envelopes are not validated. Findings 3 and 7. |
| Other issue output classes | **INCOMPLETE.** Core SARIF remains an explicitly recorded capability gap; the report does not resolve the research's required G02 output. The 20 owned cases and four imported-evidence entries do not implement catalogued execution for every fixture/output class described in the research. Preserve that distinction when assessing issue completion. |

## Independent validation and limits

- Built the actual checkout with `cargo build -p lekalo-cli --locked`: exit 0.
- Ran provenance, contract-version and versioning gates: all pass.
- Ran all eight `test-golden-*` gates and `run-golden --verify`: all pass locally
  after explicitly supplying external Ajv 8.17.1 to the catalog gate. The initial
  catalog invocation without that environment failed as expected.
- Ran suite verification twice: receipt stdout SHA-256
  `c13fb2052633cfc2d68d029e3d93ea1df2e6eb9f23b4aa01bc4a65d972c3c58b` for both.
  Compared 39 separately hashed raw stdout/stderr outcomes per run:
  aggregate SHA-256 `39ed77769446264a91ca64b6fac0a43a71041417ffd3b3e153e3b73b524d7815`
  for both. These hashes cover the described validation lanes only.
- Determinism gate: 20 cases, 39 rows, three lanes, manifest digest
  `sha256:3c8df36b3120aaa3c4168ab4eb971d23c70c4c88a1bbe3563ced49df89eaf9c6`.
- External scratch controls reproduced corrupted-golden acceptance,
  fabricated-witness acceptance, missing-CLI policy failure, invalid-model
  normalization acceptance, candidate tampering publication, unlisted destination
  publication, schema violations and hash-only semantic summary behavior.
- Read hosted CI for exact SHA `326f8a3755d7381aedc5e61680b706dccaee726b`:
  [run 36965615902](https://github.com/ichinya/lekalo/actions/runs/36965615902)
  is completed with failure. Both contracts jobs fail on Ajv lookup; Windows
  build/test fails the new suite step with 39 alias denials. Linux/macOS build
  jobs, fmt, clippy and MSRV jobs succeeded. Hosted checks were read-only;
  no GitHub comments, reviews or issue updates were posted.

Full Rust/PHP suites were not independently rerun; product Rust code is unchanged
and the focused gates plus exact-SHA hosted evidence are sufficient to establish
the findings. Passing local gates does not satisfy the missing acceptance
guarantees. Correct the blockers and failed proofs, then independently review the
new implementation SHA before treating issue #90 as complete.
