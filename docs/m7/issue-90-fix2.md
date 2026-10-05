# Issue #90 — fix round 2 report

Branch `ichinya/m7-issue-90`. Inputs fixed:
`docs/m7/issue-90-review2-devin.md` (2 blockers + 4 minor) and
`docs/m7/issue-90-review2-cline.md` (2 blockers + 4 minor). Both
blockers and three of four minors are fixed; R2-4's canned-stage
residual is recorded as a documented limitation rather than silently
expanded. No gate, schema, or contract was weakened; R2-3's relaxation
was reverted (contract restored, data regenerated to satisfy it).

## Dispositions

| # | Severity | Finding | Disposition | Evidence |
| --- | --- | --- | --- | --- |
| R2-1 | blocker (both reviews) | normalization gate fails closed in the `contracts` job, which never builds the CLI | **fixed** | `test-golden-normalization.mjs` removed from the contracts step and added to the build-test "Run the golden fixture suite gates" step (after `cargo build --workspace`), with the step comment corrected; the gate additionally accepts `LEKALO_BIN` so it can be redirected like its siblings. Verified locally: the gate exits 0 with the binary present and fails with exactly `normalization-producer-missing` when it is absent — that failure can no longer occur in `contracts`, which no longer invokes the gate |
| R2-2 | blocker (both reviews) | plain `realpathSync` keeps the Windows `RUNNER~1` 8.3 alias; hosted Windows exits 3 on all rows | **fixed** | all seven suite scratch roots now use `realpathSync.native` (the repo's documented requirement, per `test-run-history-cli.mjs`): `run-golden.mjs`, `test-golden-determinism.mjs`, `test-golden-normalization.mjs`, `test-golden-planner-e2e.mjs`, `update-golden-case.mjs`, `update-golden-run-manifest.mjs`, plus `test-golden-update-policy.mjs` (previously unwrapped) for consistency. **Windows-lane caveat:** 8.3 name creation is disabled on this checkout (`fsutil 8dot3name query C:`), so plain and native agree locally and the alias path cannot be reproduced here; the fix follows the exact mechanism of `test-run-history-cli.mjs`, which passed in the same hosted Windows job where the suite failed — a clean A/B on the runner. Final confirmation must come from the next hosted Windows run |
| R2-3 | minor | `run-manifest.schema` `runs.minItems` relaxed 2→1 to fit the committed doc | **fixed** (contract restored, data regenerated) | schema `minItems` restored to `2`; `update-golden-run-manifest.mjs` rewritten to execute two independent cold lanes (`cold-1`, `cold-2`), require row-for-row lane equality (including per-stream digests) before writing, and emit both lanes; the committed `run-manifest.json` now carries 2×41 rows and Ajv-validates against the restored schema; the determinism gate's three live lanes still cross-check the committed `cold-1` rows |
| R2-4 | minor | planner-e2e stages 4–5 canned; stage digests never emitted on success | **partially fixed, residual documented** | `passGate` now emits `stageDetails` (per-stage `ok` + detail including `loadDigest`/`graphDigest`/`contextDigest`/`diffDigest`) on the success path, and the gate header labels stages 4–5 as documented residuals with the reason (stage 4 uses the catalog-registered shared scenario corpus; a chain-built trace needs the G05 trace-producer product work from the research). Linking stage 4 to stage-1 bytes would require re-plumbing the scenario lane's input contract — out of proportion for a minor in this round; the labeling is honest |
| R2-5 | minor | apply publishes bytes but leaves checksum sidecars stale | **fixed** | apply now refreshes the case's `checksums/<caseId>.json` sidecar (full case-file digest re-walk) in the same reviewed write, immediately after publishing the expected bytes and before reporting success; the catalog gate's `checksums-digest-drift` window after apply is closed |
| R2-6 | minor | implementation-report counts stale (20 cases/39 rows) | **fixed** | `docs/m7/issue-90-implementation.md` updated to 21 cases / 41 rows / two-lane manifest / 4 byte-identical roles, matching the tree |

## Windows-lane verification note (required by the task)

The `RUNNER~1` alias failure cannot be reproduced on this checkout:
8.3 name creation is disabled on `C:`, so plain and native
`realpathSync` return identical strings here and every local gate is
green under both. The fix is therefore verified by (a) matching the
mechanism the repo itself documents as required
(`test-run-history-cli.mjs`: "the default JS realpath keeps an
8.3-spelled input as written"), and (b) the hosted A/B from review
round 2: in run 36989364497's Windows job, `test-run-history-cli.mjs`
(native) passed while the suite gates (plain) failed with 39–41 alias
denials. The next hosted Windows run on this branch is the decisive
confirmation.

## Verification outputs (this checkout, Windows, debug binary)

```text
node scripts/test-fixture-provenance.mjs
  {"ok":true,"families":64,"synthetic":64,"evidenceBacked":0}

node scripts/test-golden-catalog.mjs            (LEKALO_AJV_NODE_PATH set)
  {"ok":true,"gate":"golden-catalog","cases":21,"importedEvidence":4,
   "registryRules":449,"coverageStates":{...,"suite-pair":20},"ajv":"8.17.1"}

node scripts/run-golden.mjs --verify
  {"ok":true,"gate":"golden-run","verify":true,"cases":21,"byteCompared":4,...}

node scripts/update-golden-run-manifest.mjs
  {"ok":true,"gate":"golden-run-manifest","cases":21,"rows":41,
   "lanes":["cold-1","cold-2"]}
  committed run-manifest.json Ajv-validates against the restored
  minItems:2 schema (2 lanes x 41 rows)

node scripts/test-golden-determinism.mjs
  {"ok":true,"gate":"golden-determinism",
   "lanes":["cold-1","cold-2","warm-cache"],"rows":41,"cases":21,
   "manifestDigest":"sha256:92496524bb7624a4e4e716d366d611eebb5fab2dcb85802b38b6aa0b32345c90"}

node scripts/test-golden-normalization.mjs
  {"ok":true,"gate":"golden-normalization","files":242,
   "producerVectors":["newline-crlf-equal-output",
                      "separator-forward-slash-equal-output"]}
  (and without the binary present it exits 1 with
   normalization-producer-missing — the contracts job no longer runs it)

node scripts/test-golden-diagnostic-coverage.mjs
  {"ok":true,"gate":"golden-diagnostic-coverage","activeRules":449,
   "suitePairs":20,"familyFixture":114,"testWitness":302,"interactionOnly":13}

node scripts/test-golden-planner-e2e.mjs
  6 stages ok; receipt now carries stageDetails with per-stage digests

node scripts/test-golden-update-policy.mjs       (LEKALO_AJV_NODE_PATH set)
  {"ok":true,"gate":"golden-update-policy","planSchemaKinds":11,
   "flow":"plan -> review -> apply(digest-bound)"}

node scripts/test-golden-adapter-shared.mjs      -> ok
node scripts/test-golden-hygiene.mjs             -> ok (259 files, 6 controls)
node scripts/check-contract-versions.mjs --base HEAD
  {"ok":true,"product":"0.6.3","contractArtifacts":95,"base":"HEAD"}

cargo fmt --check           -> exit 0
cargo clippy -p lekalo-cli -p lekalo-core -- -D warnings
  -> Finished (no warnings)
cargo test -p lekalo-cli -p lekalo-core
  -> 90 "test result: ok" lines, 0 failed suites

git status                  -> clean at commit time
```
