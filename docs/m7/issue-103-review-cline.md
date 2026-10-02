# Issue #103 — independent review (Cline)

**Verdict: ISSUES.** Findings: **4 blocker, 8 major, 5 minor**.

Reviewed the pushed implementation `3790d337` ("test(contracts): register the
ci-report fixture family"), diff base `origin/ichinya/M7` = `9510dd07`, branch
`ichinya/m7-issue-103` (8 implementation commits, 92 files, +7,506/−120).
`origin/ichinya/m7-issue-103` additionally carries `f53c4c97` and `882b6c80`,
which add only two other reviewers' documents; neither was used as evidence
here. This document is the only tracked file added by this review; no
implementation file was modified.

**Important state note.** At review time the working tree carried seven
*uncommitted* in-progress edits from the implementer
(`.github/workflows/ci.yml`, `crates/lekalo-cli/src/main.rs`,
`crates/lekalo-cli/src/report_output.rs`, and
`crates/lekalo-core/src/ci_report/{build,provenance,sarif}.rs`,
`scripts/test-ci-report-contracts.mjs`). Those edits already address several
findings below (the `env: REPORTS:` export, the Node-18 `isSubsetOf` guard, a
protected-path destination check, a `scan_rendered` secret scan, a
lock-independent provenance path). **They are not part of the pushed head and
were deliberately excluded from this review**: every line-number citation and
every reproduction below is taken from `git show 3790d337:<path>` or from the
committed workflow text, not from the dirty files. The tree additionally
does not currently compile mid-edit (`E0063`/`E0599` on `CaseRow`), which is
expected for work in flight and is not itself charged against `3790d337`.

## Findings

### 1. [blocker] The CI report dogfood step fails on every OS leg: `REPORTS` is never exported

`.github/workflows/ci.yml:190` (at `3790d337`) declares the report directory as
a plain shell variable, `REPORTS="$RUNNER_TEMP/lekalo-ci-reports"`, then three
`node -e` assertions read `process.env.REPORTS`
(`.github/workflows/ci.yml:202`, `:210`, `:212`). A bash local assignment is not
exported, so `process.env.REPORTS` is `undefined` in the child process.
Reproduced verbatim under the step's own `set -euo pipefail`:

```
shell var = /.../target/rev103-node
env var = undefined
ERROR: MODULE_NOT_FOUND
SCRIPT_EXIT=1
```

`require("undefined/validate.json")` throws `MODULE_NOT_FOUND`, `set -e` kills
the step, and `build-test` is red on all three matrix legs on every run. The
consequences compound:

- The step fails *before* `echo "REPORTS_DIR=$REPORTS" >> "$GITHUB_ENV"`
  (`ci.yml:218`), so the upload's `path: ${{ env.REPORTS_DIR }}/*`
  (`ci.yml:220`) expands to `/*` — a glob over the runner root, with
  `if-no-files-found: error`. At best a hard upload failure; at worst it
  sweeps unrelated runner-root files into a downloadable artifact.
- The workflow change therefore deletes the value of the whole "CI dogfoods
  the report surface" claim in `docs/m7/issue-103-implementation.md:9` and the
  impl map's "registered in the `contracts` job / `build-test` job runs the
  per-OS report smoke" evidence.

Fix: export it (`env:` on the step, or `export REPORTS`), and prefer explicit
finalized filenames over a broad glob so a missing destination fails loudly
instead of widening. Note the implementer's uncommitted diff does exactly
this; it needs to land.

### 2. [blocker] The new required contract gate crashes on the supported Node 18 CI leg

`scripts/test-ci-report-contracts.mjs:104` (at `3790d337`):

```js
if (referenced.size !== indexes.length || !referenced.isSubsetOf(new Set(indexes))) {
```

`Set.prototype.isSubsetOf` is V8 12.4 / Node 20+. The `contracts` job matrix is
explicitly `[18.x, 24.x]` (`ci.yml:17`) and this gate is in its required step
list (`ci.yml:49`). Reproduced by running the committed script under Node
18.20.8 with exact Ajv 8.17.1 provisioned:

```
TypeError: referenced.isSubsetOf is not a function
    at invariants (.../test-ci-report-contracts.mjs:104:57)
    at .../test-ci-report-contracts.mjs:147:27
EXIT=1
```

`typeof Set.prototype.isSubsetOf` is `undefined` on 18.20.8 and `function` on
the host's Node 24.13.0. The same script exits 0 on Node 24, so the gate is
green locally and red on the older supported leg — exactly the failure mode a
matrix exists to catch. The implementer's uncommitted tree already replaces
this with an `every(...has)` loop; it needs to land.

### 3. [blocker] The committed SARIF golden is not valid SARIF 2.1.0 and would be rejected by code-scanning upload

`crates/lekalo-core/src/ci_report/sarif.rs:56` declares
`information_uri: &'static str` with **no** `#[serde(rename = "informationUri")]`,
so the wire field is emitted snake_case. The pinned golden
`tests/fixtures/ci-report/valid.sarif.golden.sarif` therefore contains
`"information_uri":"https://github.com/ichinya/lekalo"`.

Validated against the real OASIS schema
(`sarif-schema-2.1.0.json`, fetched from the errata01 URL the document itself
declares) with Ajv 8.17.1 + `ajv-draft-04` 1.0.0:

```
SARIF_GOLDEN_VALID=false
[{ "instancePath": "/runs/0/tool/driver",
   "keyword": "additionalProperties",
   "params": { "additionalProperty": "information_uri" },
   "message": "must NOT have additional properties" }]
```

`toolComponent` is `additionalProperties: false` and the schema defines
`informationUri` only (no `information_uri` anywhere in the file). AC1 asks for
SARIF that surfaces diagnostics inline; GitHub code scanning validates uploads
against this schema, so the delivered SARIF projection is not ingestible. The
contract gate does not catch it because
`scripts/test-ci-report-contracts.mjs:219-238` inspects six hand-picked fields
and never validates the document against the SARIF schema (see finding 8).

Two smaller SARIF defects in the same projection: `automationDetails.id` is
built as `format!("lekalo/{}/{}/", ...)` (`sarif.rs`), leaving a dangling
trailing slash; and `rule.id` is the diagnostic `code` while `name` is
`lekalo/<id>`, an inversion of the usual convention that makes the sorted
rules read as `LEK-*` ids. Also note the golden has `"rules":[]` and
`"results":[]` — it is generated from a **passing** validate run with zero
diagnostics, so the committed artifact does not exercise the inline-diagnostic
path AC1 is about at all.

### 4. [blocker] `--report-file` can destroy the analyzed project: any existing regular file is truncated

`crates/lekalo-cli/src/report_output.rs:90-105` accepts as a valid destination
*any* path whose parent is an existing directory and whose target is "absent or
a regular, non-link file" (`Ok(_) => Ok(())` at `:104`), and
`write_report` then opens it with `std::fs::File::create` (`:134`), which
truncates. There is no exclusion for model sources, `lekalo.lock`, generated
manifests, baselines, or history, no no-follow check on parent components, and
no atomic publication. So
`lekalo generate --check --report-file lekalo/project.yaml --project .`
overwrites the model it just validated, and `--report-file lekalo.lock`
destroys the lock — while the report happily records the pre-truncation
digests. This directly contradicts the research requirement to reject
destinations overlapping source/model/locks/baselines/history
(`docs/m7/issue-103-research.md:228`) and weakens AC4 ("detects drift without
writes") and "no automatic baseline/lock updates": the *report side channel*
silently becomes a writer of project state. The one existing negative control
only exercises an ordinary `out/` path.

### 5. [major] Secret redaction is asserted, not implemented: nothing scans report bytes before the sink

`docs/ci-reports.md:118` states "Secret material never enters a report because
it never enters a diagnostic" and `docs/m7/issue-103-implementation.md:111-114`
repeats it as *evidence* for the "logs/artifacts redact secrets" requirement.
That is a structural argument about registry field allow-lists, not a control.
In the committed code:

- `crates/lekalo-core/src/ci_report/build.rs` contains **no** occurrence of
  `scan`, `redact`, `privacy`, or `secret` (case-insensitive).
- `crates/lekalo-cli/src/report_output.rs` likewise contains **no** scan
  before `write_report` returns.
- `with_diagnostics` (`build.rs:430`) copies diagnostics into the report
  verbatim, and `build` sets `publication: { classification: "ci-derived",
  decision: Allowed }` unconditionally (`build.rs:421-424`) — there is no
  admission decision to be refused, so `PublicationDecision::PublicationUnavailable`
  is dead vocabulary.

A token-shaped string is a legal bounded `token`/`reference` diagnostic data
value, so user-controlled text reaches the report. I probed a synthetic
`alpha.ghp_…` canary; on the dirty tree (which has the new scan) it was
correctly refused, which confirms the sink is reachable and the committed head
lacks the check. The repository already owns the right primitive
(`crates/lekalo-core/src/privacy/redact.rs`, gated by
`scripts/test-privacy-leak-corpus.mjs`), so the fix is to call it on the
rendered bytes for **every** format before the write and refuse with a typed
diagnostic. There is also no canary test on any emitted format.

### 6. [major] Provenance collapses to `unknown/invalid` on any project without a lock, and the golden pins that lie

`provenance_block` (`crates/lekalo-core/src/ci_report/provenance.rs:137`) starts
with `GenerateService::inputs(selection)?`, whose `Prepared::prepare`
(`crates/lekalo-core/src/artifacts/check.rs:57-60`) hard-fails on
`LockState::Absent` — *after* `current_inputs` already computed the model/IR
pins. Every caller falls back to `empty_provenance_for`
(`crates/lekalo-cli/src/report_git.rs`), which labels model **and** IR
`unknown/invalid` and drops the profile and adapter pins entirely.

`lekalo validate` is deliberately lock-free, so the most common CI invocation
emits a report with no model/IR/profile binding at all. The reason is also
false: `invalid` implies the model failed to load, when the model just passed
validation. The committed golden pins exactly this
(`tests/fixtures/ci-report/valid.validate.golden.json`: `"model":{"version":
{"state":"unknown","reason":"invalid"}`, `"profiles":[]` on a `ready`/exit-0
run), so the gate ratifies the defect. AC6 ("reports bind exact git/model/lock/
profile revision") therefore holds only when a lock happens to exist. Read
each pin from its own owner — loader for model, IR compiler for IR,
`LockService` for lock, embedded bytes for profiles.

Related: `lock_block` binds `lock.request_digest()` (`provenance.rs:82`), the
resolver *request* digest, not the lock payload digest that `verify`'s receipt
and the manifest check pin. The impl map claims "lock (version + payload
digest through `LockService`)" (`docs/m7/issue-103-implementation.md:99`); the
code binds a different value than documented.

### 7. [major] The `strict` and `lenient` exit policies are unreachable from the CLI

`CiPolicy` (`build.rs:23`) is the acceptance-criteria centerpiece — "optional
unavailable is policy-driven". All four CLI call sites hardcode
`CiPolicy::Default`: `main.rs:2634` (validate), `:5091` (readiness), `:9282`
(verify), `:9347` (generate --check). No `--ci-policy` flag exists; `CiPolicy::parse`
(`build.rs:35`) has no production caller. The policy edges are therefore
exercised **only** by unit tests inside `ci_report/tests.rs`, and the real
commands can only ever produce the `default` column of the table.

Worse, the policy is applied inconsistently even in-process:
`apply_case_policy` (`build.rs:234`) takes `_policy` and ignores it, and
`CaseRow::effective_outcome` (`model.rs`) re-derives its own hardcoded table
where optional absences always `Warn` — never `Error` under `strict`, never
`Skip` under `lenient`. So even a future `--ci-policy strict` would promote
optional-absent *checks* to exit 4 while leaving optional-absent *suite cases*
(and therefore scenario rows, the AC2 surface) at `warn`/exit 0. AC3's
"policy-driven" half is unit-tested but not implemented for the shipped
surface.

### 8. [major] The new contract gate is partly nominal: a no-op JUnit section and a field-subset SARIF section

`scripts/test-ci-report-contracts.mjs:240-248` builds
`junitCandidates.push({ name: "shape", document: null })` and then iterates it
doing nothing (`const _ = candidate;`). The section header claims it validates
"XML-well-formed shape over a real parser grammar subset"; it validates
nothing. There is no committed JUnit golden at all
(`tests/fixtures/ci-report/` has `.json`, `.md`, `.sarif` goldens only), even
though the impl map cites "the Markdown and SARIF projection pins" as gate
coverage. The SARIF section (`:219-238`) checks six fields and never validates
against the SARIF schema, which is precisely why finding 3 is invisible to CI.

Empirically, the JUnit renderer is wrong in a way a real check would have
caught. `junit.rs` computes `skipped = tests - failures - errors`, so every
*passing* testcase is counted as skipped. Real output from the built binary on
a fully passing validate:

```xml
<testsuite name="lekalo.validate" tests="1" failures="0" errors="0" skipped="1">
  <testcase name="model.validation" classname="lekalo.validate">
```

A green run reports `skipped="1"`. CI test reporters surface skipped counts
prominently, so this misreports the dominant case. The `testsuites` element
also omits the `skipped` attribute its children carry.

### 9. [major] The report's `evaluation` never drives the process exit, and readiness is hand-patched after the builder

`report_output::compose` (`report_output.rs:141-152`) returns the *command*
result whenever the command exited non-zero, and the command result otherwise;
`report.evaluation.exit_code` is never consulted anywhere in
`crates/lekalo-cli/src` (`git grep 'evaluation.exit_code'` returns only the two
readiness assignments). So the evaluated policy exit and the process exit are
two independent authorities that agree only by coincidence.

`run_readiness` bridges them by mutating the built report after the fact
(`main.rs:5094-5100`): if the doctor verdict is `blocked` but the builder said
otherwise, it force-sets `verdict = Blocked`, `status = "unavailable"`,
`exit_code = 4`, `complete = false`. That is a second, untyped exit-policy
implementation living in the CLI, and it can diverge from the builder's own
arithmetic — the committed readiness golden pins
`"coverage":"complete","complete":true` alongside `verdict:"blocked"`, a
combination the builder would not produce and that no invariant check rejects.
The blocker then only reaches the process because a *separate* closure
(`gate`, `main.rs:4990`) re-parses the doctor JSON string looking for
`verdict == "blocked"`. Three mechanisms for one decision.

### 10. [major] `workingSetDigest` is pinned to the SHA-256 of the empty string, so it binds nothing

`report_git.rs::bounded_working_set` hashes a canonical `path\0digest\n`
inventory. When `git ls-files` yields nothing (a fixture copy, a subdirectory
selection, a shallow/filtered checkout), the canonical string is empty and the
digest is `sha256:` + SHA256("") = `e3b0c442…b855` — reported as
`{"state":"known"}`, indistinguishable from a real pin. Both committed
goldens carry exactly that value:

- `valid.validate.golden.json`: `"workingSetDigest":{"state":"known","value":"sha256:e3b0c442…b855"}`
- `valid.readiness.golden.json`: the same

I reproduced it on the host: a fresh validate report over a copied fixture
carries the same `e3b0c442…` "known" digest. AC6 claims the digest "binds the
sorted tracked input inventory"; for the fixture path the repository's own CI
dogfoods, it binds an empty string. An empty inventory must be
`unknown`/`not-applicable`, not `known`.

### 11. [minor] The `bounded_working_set` deadline does not bound anything

`report_git.rs:47-56` calls `Command::output()`, which blocks until the child
exits, and only *then* checks `start.elapsed() > Duration::from_secs(10)`. A
hung `git ls-files` therefore hangs the CLI indefinitely; the elapsed check
can never fire in the case it exists to catch. The doc comment claims "Bounded:
`git ls-files` output caps at 1 MiB and the deadline applies". Use a real
timeout (spawn + `try_wait` deadline + kill), or state honestly that the read
is unbounded.

### 12. [minor] `automationDetails.id` has a dangling trailing slash

`format!("lekalo/{}/{}/", command, verdict)` (`sarif.rs`) produces
`"lekalo/validate/ready/"` in the golden. The schema imposes no pattern on
`runAutomationDetails.id`, so this is cosmetic, but it is a malformed
hierarchical identifier shipped as a pinned artifact.

### 13. [minor] The composite action is described as the source of truth but does not exist

`docs/adr/0048-lekalo-action.md` says "The action is a composite (`action.yml`
with `runs.using: composite`)", "source-of-truth in this repository", and that
"the composite in this repository is the working reference". There is no
`action.yml` and no `examples/` directory anywhere in the tree at `3790d337`
(`git ls-tree` finds neither), and the ADR simultaneously documents
`examples/ci/*.yml` workflows as if present. The impl map records the public
action as out of scope, which is defensible, but then the ADR should be
describing a *planned* composite, not a delivered one. Documentation/code
mismatch.

### 14. [minor] Stale registry-version references survive the 0.4.0 → 0.6.3 successor

`docs/diagnostics.md:21` and `:55` still describe the registry as taking
"successor instance 0.4.0", and `docs/native-gates.md:19` still pins
"Diagnostic registry `dev.lekalo.diagnostic-registry@0.4.0`", while
`diagnostics/version.rs` now emits `0.6.3` and
`contracts/diagnostic-registry.v0.4.0.json` was replaced by `v0.6.3.json`.
The Rust constants, fixtures, and gates were all updated together (the
`cli.rs` and `validate_semantic.rs` test literals moved to `0.6.3`), so the
docs are the only stale surface.

### 15. [minor] Two other reviewers' verdict documents are committed on the implementation branch

`f53c4c97` and `882b6c80` add `docs/m7/issue-103-review-devin.md` and
`docs/m7/issue-103-review-codex.md` to the feature branch that is meant to be
merged into `ichinya/M7`. Review artifacts normally belong on the PR, not in
the merged tree. This review adds a third such file by the same convention; if
the maintainers prefer otherwise, all three should be dropped before merge.

## Acceptance-criteria checklist

| Criterion | Verdict | Evidence / gap |
| --- | --- | --- |
| **AC1 — SARIF surfaces diagnostics inline, repo-relative safe paths** | **FAIL** | Paths/regions/base-ids are correct (`%SRCROOT%`, one-based, `unicodeCodePoints`), but the document is invalid SARIF 2.1.0 (`information_uri`, finding 3) and the only committed golden has `rules: []`/`results: []`, so the inline path is never exercised. Annotation emission is deferred to the unpublished action. |
| **AC2 — scenario failures appear in JUnit XML** | **PARTIAL** | Suite/case rows do project, with `<failure>`=assertion and `<error>`=infrastructure. But the gate's JUnit section is a no-op (finding 8), there is no committed JUnit golden, and the `skipped` count is wrong for passing rows (reproduced). |
| **AC3 — required failure non-zero; optional-unavailable policy-driven** | **PARTIAL** | Required failure → non-zero holds, and denied/cancelled are never waived. But `strict`/`lenient` are unreachable from the CLI and ignored for suite cases (finding 7); the policy table is unit-test-only. Evaluated exit never drives the process (finding 9). |
| **AC4 — `generate --check` detects drift without writes** | **FAIL** | The check path itself is read-only and unchanged, but `--report-file` can truncate the model or lock it just validated (finding 4), so the no-write guarantee does not hold for the delivered surface. |
| **AC5 — fork/untrusted PR workflow safe by default** | **PARTIAL** | `contents: read`, no secrets, no `pull_request_target`, no persisted credentials; upload uses a runner-temp allow-list. But the unresolved `${{ env.REPORTS_DIR }}/*` glob (finding 1) is an artifact-scope hazard on every leg. |
| **AC6 — reports bind exact git/model/lock/profile revision** | **FAIL** | Collapses to `unknown/invalid` without a lock (finding 6); lock binds the request digest, not the payload digest; `workingSetDigest` is the digest of the empty string and reported `known` (finding 10). |
| **AC7 — examples for core, Node consumer, Laravel fixture** | **NOT DELIVERED (declared)** | No `action.yml`, no `examples/`. Recorded as partial in the impl map; ADR-0048 overstates what exists (finding 13). |
| **Secret redaction is real (not just claimed)** | **FAIL** | No scan exists on any sink at `3790d337`; publication is unconditionally `allowed` (finding 5). |
| **Deterministic bytes** | **PASS** | Verified empirically: two consecutive `validate --report-file` runs produced byte-identical 1555-byte output, LF-only (0 CR bytes) with exactly one trailing LF. Canonical compact JSON, sorted keys, no timestamps/UUIDs/host paths. |
| **Contract version bump consistent** | **PASS** | `node scripts/check-contract-versions.mjs --base 9510dd07` → `{"ok":true,"product":"0.6.3","contractArtifacts":96}`. `ci-report@0.6.3` matches product 0.6.3; registry successor 0.4.0 → 0.6.3 updates the constant, embeds, fixtures, and every consumer gate together. `additionalProperties: false` throughout, bounded arrays/strings, closed enums. (Stale *docs*: finding 14.) |
| **No automatic baseline/lock updates** | **PASS (caveat)** | All four report commands are read-only over the project. Caveat: the granted report path can still be a lock or model (finding 4). |
| **Timeouts / cancellation sane** | **PARTIAL** | Cancelled/denied rows fail closed and are never policy-waived (tested); the smoke step has `timeout-minutes: 5`. But the git deadline is unenforceable (finding 11), and no action child-tree cancellation exists (action deferred). |
| **Readiness exit-0-when-blocked fixed** | **PARTIAL** | Verified: `readiness --phase release --check` over a lock-free fixture exits 4 with the typed unavailable envelope and a `blocked` report. But the fix is a hand-patch plus a re-parsing closure, and the golden pins an internally inconsistent `complete: true` with `verdict: blocked` (finding 9). |
| **Verify failure-receipt retention fixed** | **PASS** | `verify_with_components` (`orchestration/verify.rs`) captures assembled component rows before `aggregate_envelopes`, and blocked reports carry every row with a failure class. Terminal result unchanged. |
| **`--json` envelope / status conventions preserved** | **PASS** | The report is a pure side channel; ordinary stdout/stderr envelopes and exit classes are untouched; typed `ci.report-write-failed` composes without masking a failing run. |

## Verification performed

- `node scripts/check-contract-versions.mjs --base 9510dd07` → ok, product 0.6.3, 96 families.
- Committed `scripts/test-ci-report-contracts.mjs` run against exact Ajv 8.17.1:
  **Node 24.13.0 → exit 0** (4 goldens, 8 refusal vectors, 3 verdicts);
  **Node 18.20.8 → exit 1**, `TypeError: referenced.isSubsetOf is not a function`
  (finding 2). The script was executed from a sandbox copy of the committed
  file with real `contracts/` and `tests/fixtures/` trees.
- SARIF golden validated against the OASIS 2.1.0 errata01 schema with Ajv
  8.17.1 + `ajv-draft-04` 1.0.0 → invalid, `information_uri` rejected (finding 3).
- `git ls-tree 3790d337` → no `action.yml`, no `examples/` (finding 13).
- `git grep 'evaluation.exit_code' 3790d337 -- crates/lekalo-cli/src` → only the
  two readiness assignments; the evaluated exit never drives the process (finding 9).
- Case-insensitive scan of committed `build.rs` and `report_output.rs` for
  `scan|redact|privacy|secret` → no matches (finding 5).
- Determinism: two consecutive real-binary `validate` reports byte-identical,
  1555 bytes, zero CR, single trailing LF.
- JUnit counts: real binary output on a passing validate shows
  `skipped="1"` with `failures="0" errors="0"` (finding 8).
- `REPORTS` export: reproduced under the step's own `set -euo pipefail` →
  `MODULE_NOT_FOUND`, exit 1 (finding 1).
- `git grep -n '0.4.0' 3790d337 -- docs/diagnostics.md docs/native-gates.md`
  → stale registry references (finding 14).
- `SHA256("")` computed locally → `e3b0c442…b855`, matching the `known`
  `workingSetDigest` in both committed goldens (finding 10).

Not run: the full `cargo test --workspace` suite and any hosted CI/fork/
consumer qualification. The `lekalo-core` ci_report unit tests could not be
executed to completion because the *dirty working tree* does not compile
mid-edit (`E0063`/`E0599` on `CaseRow::effective_outcome`); that is a property
of the in-flight work, not of `3790d337`, whose `ci_report` module is covered
by 16 unit tests and whose CLI integration suite reports 13 passing tests
locally. Prior reviewers report those focused suites green at this SHA. My
findings do not depend on them: every one is either a direct source/schema
contradiction at `3790d337` or a reproduction against committed bytes.

Probe artifacts were confined to the ignored `target/` tree and to temporary
sandbox copies of committed files, and have been removed. No fixture, lock, or
analyzed source in the repository was modified; the seven uncommitted
implementer edits were left untouched.

## Required next work

1. Land the `REPORTS` export and an explicit upload filename list (finding 1) —
   the dogfood job is red on every leg without it.
2. Land the Node-18-safe subset check in the contract gate (finding 2).
3. Rename the SARIF field to `informationUri`, regenerate the golden, and make
   the gate validate the projection against the real SARIF schema with a
   non-empty diagnostic (finding 3, 8).
4. Confine the report destination: refuse protected project homes, refuse
   non-empty pre-existing files, and publish atomically without following links
   (finding 4).
5. Scan every rendered projection for secret material before the sink and make
   the publication decision real, with canary tests per format (finding 5).
6. Read model/IR/profile/adapter pins from their own owners so a lock-free
   `validate` still binds them, and bind the lock payload digest (finding 6).
7. Make the policy reachable (`--ci-policy`) and apply it to suite cases, or
   state plainly in the issue that only the `default` policy ships (finding 7).
8. Report an empty tracked inventory as `unknown`, not `known` (finding 10), and
   replace the post-hoc `git ls-files` elapsed check with a real deadline
   (finding 11).

Then re-review the corrected SHA; findings 3, 4, 5, and 10 are the ones that
undercut the issue's headline acceptance criteria and should be treated as
merge-blocking.




