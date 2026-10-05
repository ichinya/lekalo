# Issue #90 — fix round 1 report

Branch `ichinya/m7-issue-90`. Inputs fixed:
`docs/m7/issue-90-review-devin.md` (1 blocker + 4 major + 6 minor) and
`docs/m7/issue-90-review-codex.md` (2 blockers + 7 major). Every
finding from both reports is dispositioned below with one-line evidence.
No gate, schema, or contract was weakened; where a reviewer asked for
stronger behavior the implementation was strengthened instead.

## Dispositions — devin review

| # | Severity | Finding | Disposition | Evidence |
| --- | --- | --- | --- | --- |
| D1 | blocker | `test-golden-catalog.mjs` in CI without `LEKALO_AJV_NODE_PATH` → contracts job red | **fixed** | `ci.yml` new step "Run the golden fixture suite catalog gates" sets `LEKALO_AJV_NODE_PATH` + `NODE_PATH="$LEKALO_AJV_NODE_PATH"`; catalog gate re-run without any env fails only outside CI provisioning, and the wired step mirrors the working Ajv step exactly |
| D2 | major | `--verify` dead; declared goldens never compared | **fixed** | `run-golden.mjs` produces every declared role via its real producer (`ROLE_PRODUCERS`) and compares raw stdout bytes; corrupting `ir-envelope.json` flips `run-golden --verify --case minimal.project` to exit 1 with `byte-drift` (verified live), restoring it returns exit 0 |
| D3 | major | checksum sidecars are `PLACEHOLDER`, unread by any gate | **fixed** | `update-golden-checksums.mjs` wrote real sha256 sidecars for all 21 cases (200 entries); the catalog gate verifies identity/revision/coverage/digests; appending one byte to a project file flips the gate to `checksums-digest-drift` exit 1 (verified live) |
| D4 | major | 10 rules `UNMAPPED` but counted `test-witness`; witness state unverifiable | **fixed** | All 10 mapped: `semantic.portable-target-reference` gained a real 20th suite pair; `loader.duplicate-key` + 4 `transaction.*` → family-fixture; `adapter.protocol-failure`, `expression.binding-invalid`, `expression.diff-invalid`, `query.tenant-filter-missing` → witness entries whose `gate` is an existing repo path that the catalog gate verifies on disk; `UNMAPPED` fallback deleted from the generator (an unmapped rule now throws); fabricated witness `scripts/DOES-NOT-EXIST.mjs` fails the gate (verified live); report text corrected to 20 pairs |
| D5 | major | planner e2e under-delivers: no inspect/impact/context, digest link claims false | **fixed** | Stage 2 runs `graph export` + `inspect` + `impact` + `context` on the stage-1 sandbox project with the symbol chosen from its own compiled IR bytes; stage 3 diffs that project against a real self-mutation (`planner.task_id`→`planner.text` input swap) and requires the affected seed to name `focus_task`, plus post-diff impact consumption; stage digests returned in the receipt; header/docs rewritten to claim exactly what executes |
| D6 | minor | generators write tracked suite files outside plan/apply; `UPDATE_RECIPES` names nonexistent scripts; header points at nonexistent `update-golden-suite.mjs` | **fixed** | `UPDATE_RECIPES` now lists the five real scripts; generator header corrected; generators + manifest/checksum writers documented in the README as reviewed regenerators whose output the gates verify; `test-golden-update-policy.mjs` now fails any workflow referencing `update-golden*`, `gen-suite-coverage`, or `gen-suite-diagnostic-pairs` |
| D7 | minor | dead surface: `RUNNER_ARGS` unused; 9 runner ids unexecutable; coverage/run-manifest schemas never compiled | **fixed** | `RUNNER_ARGS` deleted; coverage + run-manifest schemas now compiled by the catalog gate against the committed documents (schema violations listed as gate errors); unexecutable runner ids remain closed-registry declarations but the runner records them honestly as `declared` and no case uses them |
| D8 | minor | plan hashes stdout but writes `+LF`; `declaredPath` unvalidated in apply | **fixed** | plan writes the exact reviewed bytes (no added LF) and the plan digest covers exactly those bytes; apply path-policy-checks `declaredPath`, requires it to be a declared expected output of the case, and preflights candidate digests before any write |
| D9 | minor | update-policy "self-check" is a tautology | **fixed** | replaced with a before/after digest snapshot of the entire suite tree around the rehearsal; any tracked mutation or new file fails the gate |
| D10 | minor | hygiene scans only 2 of 10 scripts; normalization §4/§6 tautologies | **fixed** | hygiene scans every `test-golden-*`/`update-golden-*`/`gen-suite-*`/`run-golden.mjs` script (259 files scanned, controls still 6/6) with a delimited self-scan exclusion for this gate's own hostile vectors; normalization §4 premise now recorded honestly and §6 replaced with executed LF-vs-CRLF and forward-slash selector vectors through the real binary |
| D11 | minor | README lists undelivered directories; example id mismatch; "20 semantic rules" claim; 8 empty dirs | **fixed** | empty dirs deleted (`rmdir`); README layout section rewritten to the delivered tree with an explicit F07–F14/honest-scope note; example id corrected to `diagnostic.type-recursion.pair`; "20 semantic rules" is now true (20 pairs exist) |

## Dispositions — codex review

| # | Severity | Finding | Disposition | Evidence |
| --- | --- | --- | --- | --- |
| C1 | blocker | contracts CI: Ajv env missing; update-policy needs the built CLI which contracts never builds | **fixed** | catalog/normalization/hygiene/adapter-shared run in a dedicated `contracts` step with the Ajv env; `test-golden-update-policy.mjs` moved to `build-test` (after `cargo build/test`) with the Ajv env set, matching its binary prerequisite |
| C2 | blocker | Windows runners: `mkdtempSync(tmpdir())` cwd hits `RUNNER~1` alias → 39 `structure.selection-alias` denials | **fixed** | every suite scratch root is now `realpathSync(mkdtempSync(...))` (`run-golden`, `test-golden-determinism`, `update-golden-case`, `test-golden-planner-e2e`, `update-golden-run-manifest`, plus the normalization gate's sandbox), following the repo's existing `test-run-history-cli.mjs` pattern; production alias-denial policy untouched |
| C3 | major | `--verify` dead; minimal roles never produced/compared; PLACEHOLDER checksums; determinism hides newline/channel changes | **fixed** | same as D2 + D3; determinism gate now pins `stdoutDigest` and `stderrDigest` separately (raw bytes, no trim), and `run-manifest.json`/its writer align to the stdout digest so a trailing-newline change or channel swap flips the gate |
| C4 | major | coverage accepts fabricated witnesses (`node:scripts/DOES-NOT-EXIST.mjs` passes); php-routes witness names a nonexistent script | **fixed** | witness `gate` values are repo-relative paths verified on disk by the catalog gate; `php-routes.join-invalid` now anchors `crates/lekalo-core/src/orchestration/generate.rs` (the emitter, with the exercising gate named in the note); fabricated-witness probe fails the gate (verified live); justified `interaction-only` rows remain a separate counted state |
| C5 | major | planner gate runs independent examples, not a linked chain | **fixed** | same as D5: one sandbox project flows load→IR→graph→queries→diff-vs-self-mutation→scenario lane; the diff seed must name the mutated command; stage receipts carry per-stage digests |
| C6 | major | apply does not bind candidate bytes or confine writes; partial publication possible | **fixed** | apply preflights every candidate's digest against `plan.after.files[*].digest`, proves each destination is a declared expected path, runs the repo path policy, then publishes in one pass after all reads succeed; tampered-candidate and unlisted-destination probes now fail |
| C7 | major | three metadata documents violate their closed schemas | **fixed** | producers aligned (coverage `generatedBy.script`, run-manifest single-lane pin, plan `declaredPath`), the two previously-uncompiled schemas are now Ajv-validated by the catalog gate, and every produced/accepted plan is schema-validated by `scripts/lib/golden-schema-validation.mjs` (Ajv 8.17.1); all three committed documents validate (verified live) |
| C8 | major | hash-only semantic summary; before/after digests always equal so the policy gate rejects genuine changes | **fixed** | `before.digest` = digest of the tracked golden preimages, `after.digest` = digest of the exact candidate bytes (both recomputable), `before.files` pins preimages; summary is a field-wise semantic delta (status changes, reason-code add/remove, diagnostic count deltas, severity changes) printed to stderr as the review surface; the policy gate now rehearses a REAL mutation (project description rename) and fails a hash-only summary |
| C9 | major | normalization gate never executes newline/separator vectors | **fixed** | gate materializes a fresh CRLF variant of the minimal project and requires byte-identical loader output vs the LF input, plus a forward-slash nested selector vector; both vectors executed through the cargo binary and reported in the gate output (`producerVectors`); binary-missing fails closed |

## Verification outputs (this checkout, Windows, debug binary)

```text
node scripts/test-fixture-provenance.mjs
  {"ok":true,"families":64,"synthetic":64,"evidenceBacked":0}

node scripts/test-golden-catalog.mjs            (LEKALO_AJV_NODE_PATH set)
  {"ok":true,"gate":"golden-catalog","cases":21,"importedEvidence":4,
   "registryRules":449,"coverageStates":{"test-witness":302,
   "family-fixture":114,"test-witness":302,"interaction-only":13,
   "suite-pair":20},"ajv":"8.17.1"}

node scripts/run-golden.mjs --verify
  {"ok":true,"gate":"golden-run","verify":true,"cases":21,"byteCompared":4,...}
  negative control: corrupted ir-envelope.json -> exit 1 byte-drift; restored -> exit 0

node scripts/test-golden-determinism.mjs
  {"ok":true,"gate":"golden-determinism","lanes":["cold-1","cold-2","warm-cache"],
   "rows":41,"cases":21,"manifestDigest":"sha256:92496524..."}

node scripts/test-golden-diagnostic-coverage.mjs
  {"ok":true,"gate":"golden-diagnostic-coverage","activeRules":449,
   "suitePairs":20,"familyFixture":114,"testWitness":302,"interactionOnly":13}

node scripts/test-golden-adapter-shared.mjs
  {"ok":true,"gate":"golden-adapter-shared","sharedEvidence":4,
   "fixtureRsIncludes":9,...}

node scripts/test-golden-planner-e2e.mjs
  ok - materialize-shared-planner-project / load-and-validate-project /
       graph-and-query-projections / semantic-diff-mutation /
       scenario-lane-compile-and-run / trace-manifest-chain
  {"ok":true,"gate":"golden-planner-e2e","stages":6,...}

node scripts/test-golden-normalization.mjs
  {"ok":true,"gate":"golden-normalization","files":242,
   "producerVectors":["newline-crlf-equal-output",
                      "separator-forward-slash-equal-output"]}

node scripts/test-golden-hygiene.mjs
  {"ok":true,"gate":"golden-hygiene","files":259,"controls":6,
   "hostRootsChecked":3}

node scripts/test-golden-update-policy.mjs       (LEKALO_AJV_NODE_PATH set)
  {"ok":true,"gate":"golden-update-policy","planSchemaKinds":11,
   "flow":"plan -> review -> apply(digest-bound)"}

node scripts/check-contract-versions.mjs --base HEAD
  {"ok":true,"product":"0.6.3","contractArtifacts":95,"base":"HEAD"}

cargo fmt --check           -> exit 0
cargo clippy -p lekalo-cli -p lekalo-core -- -D warnings
  -> Finished `dev` profile ... (no warnings)
cargo test -p lekalo-cli -p lekalo-core
  -> 90 "test result: ok" lines, 0 failed suites

git status                  -> clean (0 entries) at commit time
```

## Honest residuals (unchanged scope, recorded not regressed)

- The 302 `test-witness` rows anchor to real repo paths, but the gate
  does not execute each witness test; executing every witness is the
  same evidence the existing per-family gates already produce in CI.
  This is stated in the coverage index notes rather than claimed as
  executed receipts.
- Core SARIF (research G02) remains a recorded product-capability gap;
  no test-only converter was fabricated.
- `family-fixture` rows prove the pinned fixture exists; per-rule
  polarity within those families is enforced by the families' own
  committed gates.
