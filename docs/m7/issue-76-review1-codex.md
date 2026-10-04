# Issue #76: independent review round 1 (codex)

**Verdict: ISSUES.** Two independently reproduced P2 defects require a fix round. All five requested contract gates and the mandatory hygiene checks pass; those results do not cover the failing cases below. This review does not adopt devin's ACCEPT verdict.

Reviewed on 2026-10-04, branch `ichinya/m7-issue-76`, candidate `bc21e516d49b1faf4c218a9f4ea068dc4e4b88fb`, base `a56ee5786f1724f9ee61ef973088c7679684c14d`. The candidate includes implementation commits `d996d26f`, `2868a3b2`, `fda97b81` and the prior review document. Inputs read: `issue-76-research.md`, `issue-76-implementation.md`, `issue-76-review1-devin.md` and the complete supplied review brief. Source, published schemas, committed goldens, shipped bundles and fresh CLI results were checked independently.

Environment: Windows, Rust/Cargo 1.98.0, Node 24.13.0, PHP 8.5.0, exact Ajv 8.17.1 at `C:/Users/User/AppData/Local/Temp/ajv/node_modules`. Binary rebuilt with `cargo build -p lekalo-cli --locked`; SHA-256 of `target/debug/lekalo.exe`: `15e6f42c1f382c4ce860fa652061021b026cbac85e06465af11d920b6d4276f0`.

## Findings

### R1 — P2: shipped collectors emit evidence outside the published location bound; collection and replay disagree

Source: `adapters/node-typescript/src/ai-lint.mjs:57`, `adapters/php-laravel/src/ai-lint.php:99`, `contracts/ai-lint-evidence.schema.v0.6.4.json:63`, `scripts/gen-ai-lint-contracts.mjs:37`, `crates/lekalo-core/src/ai_lint/input.rs:69`, `crates/lekalo-cli/src/ai_lint.rs:129`, `crates/lekalo-core/src/ai_lint/collect.rs:130`.

The published evidence contract limits its **document-level** `locations` array to 32. Supplied-file decoding enforces that bound. Both collectors instead allow up to 10,000 locations and return protocol status `ok` beyond 32. The installed `--scan-target` path admits these responses without the supplied-file tree bound, so collector evidence with the same source/record layout is accepted during collection and rejected on file replay.

Reproduction used disposable copies of `tests/fixtures/ai-lint/model`, fresh Model/IR pins extracted from `lekalo.exe --json ai-lint --module planner`, and the actual shipped `adapter.mjs` / `adapter.php` entrypoints. Requests were valid against the current protocol schema, with scope `['planner']`, empty bindings and one explicit source file. No individual record exceeded its own location/witness bounds.

Generate distinct functions with a single computed call each:

```js
// src/events.ts, i = 0 .. n-1
export function case0(obj: any, key: string) { obj[key](); }
```

```php
<?php
class Cases {
    // app/events.php, i = 0 .. n-1
    public function case0($obj, $key) { $obj->$key(); }
}
```

Send the read-only `lint` request, validate `response.result.lint_evidence` with strict Ajv 8.17.1, save it as `collector-evidence.json`, and compare these CLI paths after installing the adapter into the disposable project:

```text
lekalo.exe --json ai-lint --module planner --evidence collector-evidence.json
lekalo.exe --json ai-lint --module planner --scan-target <target> --source-file <source>
```

| Actual shipped collector | Functions/methods | Source bytes | Evidence locations / records | Evidence schema | Supplied-file CLI | Installed collection CLI |
| --- | ---: | ---: | --- | --- | --- | --- |
| Node control | 32 | 1,974 | 32 / 32 | valid | valid, exit 0, raw 32 | valid, exit 0, raw 32 |
| Node failure | 33 | 2,036 | 33 / 33 | invalid: `/locations`, `maxItems: 32` | invalid, exit 1, `ai-lint.input-invalid`, detail `wire-bound` | valid, exit 0, raw 33 |
| PHP control | 16 | 860 | 32 / 16 | valid | valid, exit 0, raw 16 | valid, exit 0, raw 16 |
| PHP failure | 17 | 913 | 34 / 17 | invalid: `/locations`, `maxItems: 32` | invalid, exit 1, `ai-lint.input-invalid`, detail `wire-bound` | valid, exit 0, raw 17 |

Every collector invocation above exited 0 with response status `ok`. The failing responses also fail the target-protocol successor schema at `/result/lint_evidence/locations`. An additional PHP case with 33 methods produced 66 locations and the same refusal on replay. These are small inputs, well below source/work/record budgets. PHP creates method spans even before identifying a call, making this bound reachable especially quickly.

Impact: shipped producer output violates its own contract, and collection results cannot reliably be admitted as supplied evidence. The existing evidence gate exercises small source fixtures and misses this boundary. The producer, published bound and both admission paths must agree; exhausted bounds must have an explicit supported refusal or incomplete-coverage result. Add controls at and immediately beyond the document-level location limit without weakening the existing checks.

### R2 — P2: an unavailable effect action becomes an undeclared-write finding and can deny CI

Source: `crates/lekalo-core/src/ai_lint/mod.rs:582`, `crates/lekalo-core/src/ai_lint/mod.rs:894`; declaration control: `tests/fixtures/ai-lint/model/lekalo/modules/planner/commands.yaml` (`planner.edit_task_cmd` declares `planner.edit_task`, an `update` on `planner.task`).

Effect comparison requires known operation/resource, but passes an optional action from `r.key` to `declared_effect`. Its `operation.is_some_and(...)` returns false when the action is unknown or unsupported. The caller interprets that as a demonstrated missing declaration and emits `hidden.observer-write` / `hidden.undeclared-effect`. Coverage does not degrade for this unavailable action.

Independent reproduction used the committed evidence golden in a disposable Model copy. Retain only its observer-effect record, bind `subject`, `semanticSymbol` and `operation` to `planner.edit_task_cmd`, set `nativeId` to `fixture/edit-observer`, refresh Model/IR pins and recompute the record ID as the canonical SHA-256 of `[kind, subject, nativeId, locations]`. Keep the current source/span/activation evidence, resource `planner.task` and field `state`. Change only `key` between the following schema-valid states:

| Effect action `key` | Advisory result | Observer coverage | `--lint-profile ci --check` |
| --- | --- | --- | --- |
| `{state: 'known', value: 'update'}` | valid, exit 0; no hidden-write finding | complete | valid, exit 0 |
| `{state: 'unknown'}` | valid, exit 0; high-confidence warning `hidden.observer-write` for `planner.edit_task_cmd` | complete | denied, exit 3 |
| `{state: 'unsupported'}` | valid, exit 0; same high-confidence warning | complete | denied, exit 3 |

All three inputs validate with strict Ajv 8.17.1. The erroneous finding remains `possible-behavior` and `verifiedEffects` remains 0, but the missing action is still promoted into a complete undeclared-effect assessment and a policy denial. An unavailable action cannot establish whether the current declaration covers the detected effect. Preserve that uncertainty in comparison/coverage instead of treating it as a negative match; add live unknown/unsupported-action controls beside the declared-effect negative.

## Mandatory checks and evidence

All command results below were obtained in this review, rather than copied from either input report. Node family commands used `LEKALO_AJV_NODE_PATH=C:/Users/User/AppData/Local/Temp/ajv/node_modules`, without `--update`.

| Brief criterion | Independent evidence | Result |
| --- | --- | --- |
| Locked CLI rebuild | `cargo build -p lekalo-cli --locked` exits 0; rebuilt binary used for subsequent probes. | PASS |
| Live `--all` inside the requested fixture | `target/debug/lekalo.exe --json ai-lint --all`, cwd `tests/fixtures/ai-lint/model`: valid/exit 0; strict report schema valid; raw/active/waived/possibleEffects/verifiedEffects all 0; all 297 coverage rows explicitly unknown with `target-or-threshold-evidence-required`; `ai-lint.coverage-incomplete` emitted. Repeated output bytes match. | PASS |
| Mismatched golden refusal | Disposable copy of `ai-lint-evidence/golden/evidence.json` with Model pin `sha256:` plus 64 zeroes, otherwise unchanged; `--module planner --evidence <copy>`: invalid/exit 1, `ai-lint.input-invalid`, detail `evidence-model-pin`. Original golden untouched. | PASS |
| Help agrees with behavior | `ai-lint --help` says `Optional evidence-bound AI readability analysis`, defaults to advisory and exposes actual options. Missing/conflicting selectors and `--source-file` without a scan target produce typed `cli.usage`, exit 1. | PASS |
| Report family | `node scripts/test-ai-lint-report-contracts.mjs`: exit 0, `ok:true`, family `ai-lint-report`, schema `0.6.4`, `live:true`. | PASS |
| Evidence family | `node scripts/test-ai-lint-evidence-contracts.mjs`: exit 0, corresponding `ok:true` / `live:true`. | PASS, with uncovered R1/R2 |
| Config family | `node scripts/test-ai-lint-config-contracts.mjs`: exit 0, corresponding `ok:true` / `live:true`. | PASS |
| Waivers family | `node scripts/test-ai-lint-waivers-contracts.mjs`: exit 0, corresponding `ok:true` / `live:true`. | PASS |
| Comparison family | `node scripts/test-ai-lint-comparison-contracts.mjs`: exit 0, corresponding `ok:true` / `live:true`. | PASS |
| Evidence gate executes shipped collectors | `scripts/lib/ai-lint-contract-gate.mjs:190` launches the absolute shipped Node/PHP entrypoints with `spawnSync(runtime, [adapter])`, validates receipts, admits fresh evidence and asserts observer/reflection positives and declared-effect negatives. It also installs both packages and repeats module/single-symbol positives through the production TargetClient path, preserving observed index/source bytes (`:223`). The evidence dispatch calls both `evidenceCases` and `adapterCases` (`:239`). | PASS |
| Registry successor | Programmatically map entries by ID, compare every predecessor object including all fields, and group additions by prefix: 459 to 476 entries, 0 missing/changed predecessors, exactly 17 additions: 6 `ai-lint.*`, 3 `ambiguity.*`, 7 `hidden.*`, 1 `indirection.*`. HIDDEN-007/008 remain unallocated. | PASS |
| Core/adapters language boundary | `ai_lint/mod.rs:225` analyzes typed Compilation, graph and admitted evidence. No PHP/TypeScript parser in the core module; AST/token recognition is in the two adapter source files. Admission does confined source reads; `collect.rs` is installed-adapter orchestration through TargetClient, not target-language analysis. | PASS for the stated language boundary |
| Advisory and coverage gating | Direct default `--all --check` remains valid/exit 0 with all 297 rows unknown and coverage diagnostic. Report gate proves a selected ci profile without `--check` remains advisory; explicit ci check retains report under denial. Config/evidence gates prove configured required-coverage denial. | PASS; R2 identifies erroneous finding input to an explicit gate |
| Static claims / verified effects | Both shipped fixture pipelines and direct probes keep `verifiedEffects = 0`; admission refuses verified-behavior/structural-effect forgeries; low-confidence vectors become info. | PASS for proof ceiling; uncertainty assessment has R2 |
| Formatting | `cargo fmt --all -- --check`: exit 0. | PASS |
| Full delta whitespace | `git diff --check a56ee578..HEAD`: exit 0. | PASS |
| Contract versions | `node scripts/check-contract-versions.mjs --base a56ee5786f1724f9ee61ef973088c7679684c14d`: exit 0, `ok:true`, product `0.6.4`, 109 contract artifacts. No predecessor contract changes. | PASS |
| Workspace hygiene | Initial status clean; original fixture still contains only `app`, `lekalo`, `src`, with no ignored runtime residue. All hostile source/evidence/install mutations were in disposable Temp projects outside this worktree. Only this review document is a repository change. | PASS |

Additional local validation: `cargo test -p lekalo-core --locked --lib` exits 0: **1,049 passed, 0 failed, 2 ignored**. The focused `ai_lint` run independently passed both depth tests. CI source places the five gates at `.github/workflows/ci.yml:253` after `cargo build --workspace --locked` at `:198`; they do not depend on a later job's binary.

## Acceptance-criterion assessment

| Criterion from the research brief | Independent evidence / limit | Assessment |
| --- | --- | --- |
| AC1: ambiguity never resolves silently | Live report/comparison vectors retain competing candidates; duplicate native joins refuse in the actual adapter gate. CLI reuses inspect selection refusal. Lint does not write bindings. | PASS for exercised cases |
| AC2: hidden observer/dynamic effects visible in PHP/Node | Evidence gate passed actual shipped-bundle and installed production positives for module/single-symbol selection, plus registration/declaration negatives. R1 shows those collectors can produce invalid larger receipts. | ISSUES: R1 |
| AC3: scoped, justified, optionally expiring suppression | Waiver gate passed exact scope, raw-count preservation, permanent/dated entries, inclusive expiry, required as-of, invalid/duplicate cases and changed-condition audit. | PASS |
| AC4: semantic symbols and current source spans linked | Evidence gate passed symbol/activation links, UTF-8 span/fingerprint refusals, source movement and stable IDs; default projection withholds source paths. R1 prevents consistent collection/replay beyond the location limit. | ISSUES: R1 |
| AC5: evidence uncertainty and confidence remain honest | Low-confidence and forged-proof gate vectors passed; stale/bounded native depth remains unknown. R2 proves unknown/unsupported effect action instead produces a high-confidence undeclared-write assessment with complete rule coverage. | ISSUES: R2 |
| AC6: revision trend measurable without waiver distortion | Comparison gate passed fresh revision pins, depth deterioration, raw ambiguity improvement, waiver/confidence churn, coverage incomparability, malformed baselines and immutable baseline bytes. | PASS for exercised cases |
| AC7: ordinary observed legacy use stays optional | Default advisory and off-profile live vectors pass; explicit check controls denial; installed fixture probes preserve source/index bytes. No new ordinary validate/observe/inspect lint invocation found. | PASS |

## Delivery boundary

Review only: implementation, contracts, fixtures, gates and other worktrees were not changed. The local documentation commit is restricted to `docs/m7/issue-76-review1-codex.md`, with staged paths and whitespace checked before commit. No push or hosted CI run is part of this review. Local green gates do not supersede R1/R2 or establish cross-platform production acceptance.
