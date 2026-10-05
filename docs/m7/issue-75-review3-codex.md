# Issue #75 — independent review, round 3 (agent: codex)

Reviewer: codex, independent verification of FIX ROUND 2 on
`ichinya/m7-issue-75`. Reviewed implementation delta
`3cb774198aac008ee4e73b08c7b653a72f13e8ac..c9a531a19fdd0c82333a65ae407b7e198b725185`
(17 files, +1180/-367). The starting worktree HEAD was
`94baa3a14df157863b42ac6304fbca3daaf13e4f`; its only change after the
candidate is `docs/m7/issue-75-review3-devin.md`, so the tested implementation
is exactly the requested candidate. Worktree was initially clean.

Read first: [fix-round-2 dispositions](issue-75-fix2.md),
[Devin round 2](issue-75-review2-devin.md), and
[Cline round 2](issue-75-review2-cline.md). Used the established
[round-3 review format](issue-75-review3-devin.md). Also read the live issue
with `gh issue view 75 --repo ichinya/lekalo`; neither another reviewer's
verdict nor the disposition report was used as verification evidence.

Method: inspect the actual delta and current production paths; run every
requested build/test/gate; validate live reports independently with pinned
Ajv; compare project scope against a separate live graph export; hash the
exact consumed policy/baseline bytes; exercise malformed and oversized
inputs; audit CI ordering, retained assertions, schema constraints and
scratch cleanup. Only this review document is changed.

## Verdict: ISSUES

All twelve claimed round-2 dispositions are verified. No regression or
weakening introduced by `c9a531a1` was found. However, the explicitly
requested live `--all` report fails the published report schema despite
exit 0 and `status: valid`. This is an inherited contract defect, present
at `3cb77419`, rather than a newly introduced fix-round-2 regression.
The finding is separated below so accepting the twelve repairs is not
mistaken for accepting every requested report as a usable contract.

## Remaining finding in the requested live checks

### Major — R3-1: project scope emits an invalid report identifier

`crates/lekalo-core/src/context_budget/mod.rs:1191-1193` serializes
`Scope::All` as `{"id":"*","kind":"project"}`. The required `scope.id`
references `semanticId` in
`contracts/context-budget-report.schema.v0.6.3.json:95-96`; its pattern at
line 192 is `^[A-Za-z0-9_][A-Za-z0-9_.:-]*$`, which excludes `*`.

Reproduced against the built candidate on the committed planner fixture:

```text
lekalo --json context-budget --all --budget 200
exit: 0
status: valid
contextBudget.scope: {"id":"*","kind":"project"}
subjects: 29

Ajv 8.17.1, strict Draft 2020-12 validation:
schemaValid: false
instancePath: /scope/id
schemaPath: #/$defs/semanticId/pattern
keyword: pattern
```

The same validator accepts the live symbol and `--module planner` reports.
All 29 project subjects equal the graph export's complete non-module node
inventory; all 21 planner module subjects occur in the project report.
Thus the R2-3 coverage repair works, while the project report remains
unusable by a consumer enforcing its advertised schema. The Node gate
passes because it validates symbol/module reports but never runs `--all`.
The new core coverage test checks subject identities, not the wire schema.

Minimal independent reproduction, from this worktree (writes no files):

```powershell
$env:NODE_PATH = 'C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
@'
const fs = require('node:fs');
const { resolve } = require('node:path');
const { execFileSync } = require('node:child_process');
const Ajv = require('ajv/dist/2020').default;
const env = { ...process.env }; delete env.LEKALO_PROJECT;
const document = JSON.parse(execFileSync(resolve('target/debug/lekalo.exe'),
  ['--json', 'context-budget', '--all', '--budget', '200'],
  { cwd: resolve('tests/fixtures/context-budget/planner'), env, encoding: 'utf8' }));
const validate = new Ajv({ strict: true, allErrors: true }).compile(JSON.parse(
  fs.readFileSync('contracts/context-budget-report.schema.v0.6.3.json', 'utf8')));
console.log(JSON.stringify({ scope: document.contextBudget.scope,
  schemaValid: validate(document.contextBudget), errors: validate.errors }, null, 2));
'@ | node
```

Base check: `git show 3cb77419:crates/lekalo-core/src/context_budget/mod.rs`
has the identical wildcard serialization at line 1159; the base report
schema has the identical semantic-id pattern at line 171. This finding
does **not** establish a new regression in the reviewed delta. Repair the
producer/contract agreement for project scope and add a live `--all`
schema assertion, preserving the semantic-id constraints for other fields.

## Round-2 finding disposition

| Finding | Independently verified disposition and evidence |
| --- | --- |
| Cline **B1** — gate before binary build | **FIXED.** `ci.yml:171-188` provisions exact Ajv, builds the workspace at line 180, then runs this gate at line 188 in `build-test`. Linux/Windows/macOS matrix, bash shell, in-step `NODE_PATH`; exactly one gate invocation, none in `contracts`. Missing-binary control returns exit 1 `binary-missing`; binary restored with an identical SHA-256. Header now describes mandatory live checks. |
| Cline **M1** / Devin **R2-1** — dead provenance pins | **FIXED.** `main.rs:5457-5473` attaches pins before JSON/Markdown projection. Live baseline, policy, combined-input, chained-baseline and metric-policy-denied reports contain the expected closed pins. Both digests equal independently computed SHA-256 of the consumed bytes. Pretty versus compact serialization of the same baseline changes its pin, proving raw-byte binding. Markdown includes both pins. Twenty malformed/missing provenance inputs are rejected by Ajv and the real CLI. |
| Cline **m1** / Devin **R2-2**, codemods | **FIXED.** All three committed `scripts/.fix-*.mjs` files are deleted; the scripts directory contains no replacement matching that pattern. |
| Cline **m2** / Devin **R2-2**, probe output | **FIXED.** `cb-gate-probe/baseline.json` is deleted and `cb-gate-probe/` is physically absent. Tests/gate/probes leave the tracked fixture clean. |
| Devin **R2-3** — all/module kind coverage | **FIXED for coverage.** Production selection includes every non-module node. Independent `graph export` comparison proves exact equality: 29 non-module nodes, 29 project subjects; planner's 21 subjects are a subset. Includes policies, scenarios, endpoints, requirements and target bindings. The separate inherited wire-contract failure is R3-1 above. |
| Devin **R2-4** — missing LEK-CONTEXT-004 emission | **FIXED.** `mod.rs:623-630` calls `artifact_evidence_incomplete`. Live mapped-files emits `context.artifact-evidence-incomplete` / `LEK-CONTEXT-004`, exit 0, with `optionalSourceTokens: unsupported`. The none recipe stays unknown and emits no such warning. |
| Devin **R2-5** — recipe/digest disagreement | **FIXED.** Generic mapped-files normalizes the profile and changes its digest. A named profile declaring mapped-files without the flag and the same profile explicitly selecting mapped-files produce identical effective digests and unsupported source metrics. A mandatory policy pinned to none still refuses the mapped-files override with exit 3 `policy-profile-pin`. |
| Devin **R2-6** — absent breakdown reconciliation checks | **FIXED.** Core, CLI and live/golden Node assertions independently sum exclusive/shared costs. Live planner: 355 required = 221 exclusive + 134 shared. Independent probes also reconcile every untruncated subject row in the 29-subject project and 21-subject module reports. |
| Devin **R2-7** — dead helpers/collector test shape | **FIXED.** Removed `metrics::dependency_counts` and `metrics::semantic_counts`; production helpers remain. `supporting_requests_collector_counts_real_edge_occurrences` drives the real compiled fixture graph, asserts repeated scalar requests and reconciles their total to `edgeOccurrences`; executed in the passing core suite. |
| Devin **R2-8** — auxiliary unbounded policy reads | **FIXED.** One `read_bounded`/parse supplies policy limits, digest and evaluation (`main.rs:5354-5374`). Baseline pin uses its consumed bounded bytes. `File::take(MAX_INPUT_BYTES + 1)` bounds allocation before the size refusal. Real CLI rejects 16 MiB + 1 byte for profile, policy and baseline; malformed baseline retains precedence over a deferred oversized policy failure. |
| Devin **R2-9** — leaked fixture copies/markers | **FIXED.** `FixtureCopy` owns its `TempDir` (`context_budget.rs:459-482`); neither `keep()` nor marker writes remain. Executed `fixture_copy_cleans_up_after_children_finish`, which verifies lifetime and removal after drop. |
| Cline **m3** — arbitrary shared-cost owner | **FIXED.** `mod.rs:366-400` bills the shared bucket to the subject at hops 0, with exclusive cost 0. Live focus-task subject row carries 134 shared tokens; every dependency row has shared cost 0. Same attribution invariant holds across the independent project/module probes and the golden. |

## Gate/schema weakening and cleanliness

- Compared the parsed base/current report schemas after removing descriptions
  and the intentionally repaired `provenance.baseline` member: they are
  structurally identical. The baseline replacement is closed, requires a
  lowercase SHA-256 digest for known, and forbids digest/extra fields for
  non-known states. Profile/policy/comparison schemas are unchanged.
- Existing Node failure checks are retained: 42 distinct prior `fail`
  reasons remain among 63 current reasons. Removed script lines are stale
  comments. Live/golden equality, privacy, determinism, ledger checks and
  existing negative vectors remain enforced. R3-1 is a coverage omission,
  not a removed or relaxed gate.
- No test is removed or ignored. CLI suite grows 13 to 16; focused core
  suite grows 53 to 57. The removed CLI denial assertions are retained under
  a renamed local variable and supplemented with payload/pin checks. Dead
  metric-helper deletion removes no tests. No policy threshold was relaxed.
- `git status --porcelain=v1 --untracked-files=all` is empty before review
  authoring and after all verification, including the binary control.
  Scratch paths are absent. Reviewer probe inputs use owned OS temp
  directories removed in `finally`; no repository fixture is edited.

## Reviewer-run verification

Windows/PowerShell, Cargo 1.98.0, Node 24.13.0, Ajv 8.17.1. All commands
below were run here against the candidate implementation. Cargo used the
existing target cache; this was not a cold build.

```text
cargo build -p lekalo-cli --locked                              exit 0
cargo test -p lekalo-cli --test context_budget --locked          16 passed, 0 failed/ignored
cargo test -p lekalo-core --lib context_budget --locked          57 passed, 0 failed/ignored
node scripts/test-context-budget-contracts.mjs                  exit 0
  {"ok":true,"ajv":"8.17.1","registryEntries":457,
   "contextRules":8,"liveChecked":true}
cargo clippy --workspace --all-targets --locked -- -D warnings    exit 0
cargo fmt --all -- --check                                      exit 0
node scripts/check-contract-versions.mjs --base origin/ichinya/M7 exit 0; 99 artifacts
node scripts/test-fixture-provenance.mjs                         exit 0; 64 families
git diff --check                                               exit 0
```

Node commands used the requested
`NODE_PATH=C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules`.
Additional independent probes confirmed symbol/module schema validity,
project inventory coverage, mapped-files normalization, byte determinism,
both raw-byte pins, combined and denied payloads, Markdown, chained
baselines, 20 malformed-input refusals and all three oversized refusals.
The independent project-report schema probe **failed as R3-1**; it is not
represented by the green existing gate. The missing-binary probe returned
the expected exit 1; restoration was verified by hashing the binary.

Limit: structural baseline failures can prefix stderr with the legacy
`baseline serde:` line (`baseline.rs:335`, already present at base line
314). For those negative probes I verified exit 1 and the following invalid
envelope; this is not evidence that the complete stderr stream is valid
JSON. No new fix-round-2 regression is attributed to that unchanged line.

No hosted CI run, cold three-platform build or full workspace test run was
performed. The verdict concerns the requested candidate and live checks;
it does not claim remote delivery or production acceptance. All twelve
listed repairs hold, but R3-1 remains before an unqualified acceptance of
the requested project report.
