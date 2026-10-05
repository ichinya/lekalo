# Issue #76: independent fix review round 2 (codex)

**Verdict: ACCEPT.** R1 and R2 are resolved in the reviewed fix delta. Independent source inspection, the original live reproductions, additional boundary controls and all five required contract gates passed. No open finding remains for this fix round.

Reviewed on 2026-10-04, branch `ichinya/m7-issue-76`, candidate `278704d372f57ff1a6bde70b5be0f51ce093c87f`, fix `48b838e5`, base `be0c78a9`. Inputs: the complete fix-review brief, [fix report](issue-76-fix1.md), [Codex round-1 reproductions](issue-76-review1-codex.md), [Devin round-2 review](issue-76-review2-devin.md), research acceptance criteria, changed source/bundles/manifests, schemas, gate code and CI ordering. The verdict is based on this review's live results.

Environment: Windows; Rust/Cargo 1.98.0, Node 24.13.0, PHP 8.5.0, exact Ajv 8.17.1 at `C:/Users/User/AppData/Local/Temp/ajv/node_modules`. Rebuilt with `cargo build --workspace --locked`. SHA-256 of `target/debug/lekalo.exe`: `18931672f03c1f2753b2ca7c1b490d1a767e053d3fe728b14d6c584fbd6b0f91`.

## Finding dispositions

| Finding | Disposition | Source and independent evidence |
| --- | --- | --- |
| R1: collectors exceed the 32-location document bound; installed collection and file replay disagree | **FIXED** | Node `adapters/node-typescript/src/ai-lint.mjs:111` and PHP `adapters/php-laravel/src/ai-lint.php:103` retain complete records within the union of record/activation spans, then fill remaining capacity with unreferenced locations. Exhaustion is explicit in document and supported-rule limitations. `crates/lekalo-core/src/ai_lint/input.rs:196` enforces the existing `validate_tree` bounds on typed admission. Seven fresh shipped-bundle cases pass strict protocol/evidence validation and both CLI paths with matching findings/coverage; every oversized replay refuses with `ai-lint.input-invalid` / `wire-bound`. |
| R2: unavailable effect action becomes a false undeclared-effect finding and CI denial | **FIXED** | `crates/lekalo-core/src/ai_lint/mod.rs:602` requires known operation, resource and action before declaration comparison. `:909` takes a known action; `:545` degrades affected coverage with `effect-comparison-input-unavailable`. Observer and direct records were independently replayed for matching update, unknown, unsupported, withheld and mismatching create. Unavailable actions yield no hidden-write finding and partial coverage; known create still yields the correct high-confidence warning and denies explicit CI check. |

## R1: live boundary and admission reproduction

The standalone scratch harness does not import the implementation's contract-gate helpers. It copies the committed Model fixture into Temp, obtains fresh Model/IR pins from the rebuilt CLI, generates the original computed-call sources and sends read-only protocol `lint` requests to the actual shipped `adapter.mjs` / `adapter.php`. Requests use module scope `planner`, empty bindings and one explicit source file. Both adapters are installed into their disposable projects through the production dry-run/confirm install path.

Node sources use `export function caseN(obj: any, key: string) { obj[key](); }`; PHP uses `public function caseN($obj, $key) { $obj->$key(); }` inside `class Cases`. The exact 33-location PHP control adds an empty seventeenth method to sixteen computed-call methods. The original seventeen-call case and PHP 32/33-method cases are also exercised.

| Shipped collector input | Source bytes | Detector locations before projection | Published locations / reflection records | Document limit reported | Replay / installed scan |
| --- | ---: | ---: | --- | --- | --- |
| Node: 32 computed-call functions | 1,974 | 32 | 32 / 32 | No | valid / valid, exit 0 |
| Node: 33 computed-call functions | 2,036 | 33 | 32 / 32 | Yes | valid / valid, exit 0 |
| PHP: 16 computed-call methods | 860 | 32 | 32 / 16 | No | valid / valid, exit 0 |
| PHP: 16 computed-call methods + empty seventeenth | 887 | 33 | 32 / 16 | Yes | valid / valid, exit 0 |
| PHP: 17 computed-call methods, original failure | 913 | 34 | 32 / 17 | Yes | valid / valid, exit 0 |
| PHP: 32 computed-call methods | 1,708 | 64 | 32 / 32 | Yes | valid / valid, exit 0 |
| PHP: 33 computed-call methods, original larger case | 1,761 | 66 | 32 / 32 | Yes | valid / valid, exit 0 |

All seven receipts have protocol status `ok`, exit 0 and strict Ajv-valid protocol/evidence documents. Every record and activation reference resolves to a retained location. PHP reflection records reference their call spans; unused method spans can be omitted while retaining more than sixteen complete records.

For every row, compare:

```text
lekalo.exe --json --no-cache ai-lint --module planner --config probe-config.json --evidence collector-N.json --spans
lekalo.exe --json --no-cache ai-lint --module planner --config probe-config.json --scan-target <target> --source-file <source> --spans
```

Assertions passed for finding identity, rule, confidence, severity, disposition, semantic symbol and condition digest; all target coverage rows match between paths. Both span projections equal the shipped receipt's retained location array. Reflective-call coverage remains partial, and both envelopes emit the registered `ai-lint.coverage-incomplete`. Every beyond-bound row carries `document-location-limit` in the document and supported-rule coverage; exact-bound rows do not carry it. `verifiedEffects` remains 0. Source bytes are unchanged by collection/replay.

In each row, append one location to the 32-location receipt and replay the resulting file. Strict Ajv refuses at `/locations` with `maxItems`; the real CLI returns `invalid`, exit 1, `ai-lint.input-invalid`, detail `wire-bound`. No oversized replay is accepted.

The live evidence gate independently exercises its own exact 32/33-location controls for both targets and paths at `scripts/lib/ai-lint-contract-gate.mjs:254`. The focused Rust run also executes `protocol_evidence_and_file_replay_share_document_location_bounds` at `crates/lekalo-core/src/ai_lint/input.rs:401`: direct typed admission and file decoding produce identical refusal envelopes for 33 locations.

## R2: live action-state reproduction

The independent harness starts from committed `ai-lint-evidence/golden/evidence.json` in a fresh Temp Model copy, retains one observer or direct effect record, refreshes Model/IR pins and binds subject/semantic symbol/operation to `planner.edit_task_cmd`. Resource `planner.task`, field `state`, current source spans and activation evidence stay present. Native IDs are `fixture/edit-observer` / `fixture/edit-direct`; record IDs are recomputed as canonical SHA-256 of `[kind, subject, nativeId, locations]`.

The command declares an update on `planner.task`. Change only action `key` between the table's states. The policy starts from the committed config golden with unrelated depth/writer thresholds set to unknown to isolate the effect comparison. CI confidence and warning-denial settings remain unchanged.

| Mechanism | Action state/value | Hidden-effect findings | Affected rule coverage | Advisory | CI profile + explicit check |
| --- | --- | ---: | --- | --- | --- |
| observer | known update | 0 | complete | valid, exit 0 | valid, exit 0 |
| observer | unknown | 0 | partial + unavailable-input limitation | valid, exit 0 | valid, exit 0 |
| observer | unsupported | 0 | partial + unavailable-input limitation | valid, exit 0 | valid, exit 0 |
| observer | withheld | 0 | partial + unavailable-input limitation | valid, exit 0 | valid, exit 0 |
| observer | known create, mismatching declaration | 1: `hidden.observer-write` | complete | valid, exit 0 | denied, exit 3 |
| direct | known update | 0 | complete | valid, exit 0 | valid, exit 0 |
| direct | unknown | 0 | partial + unavailable-input limitation | valid, exit 0 | valid, exit 0 |
| direct | unsupported | 0 | partial + unavailable-input limitation | valid, exit 0 | valid, exit 0 |
| direct | withheld | 0 | partial + unavailable-input limitation | valid, exit 0 | valid, exit 0 |
| direct | known create, mismatching declaration | 1: `hidden.undeclared-effect` | complete | valid, exit 0 | denied, exit 3 |

All ten inputs and all twenty CLI reports validate with strict Ajv. Both profiles carry `effect-comparison-input-unavailable` on the affected rule for unavailable actions, emit `ai-lint.coverage-incomplete` and produce neither hidden-effect diagnostic. Known mismatch produces the appropriate high-confidence warning in both profiles and preserves the report under denial. All reports retain `verifiedEffects = 0`. The existing live evidence gate separately exercises this matrix at `scripts/lib/ai-lint-contract-gate.mjs:141`.

## Gates, no-weakening and hygiene

Every command/result below was obtained in this review. The five family gates use `LEKALO_AJV_NODE_PATH=C:/Users/User/AppData/Local/Temp/ajv/node_modules`, without `--update`.

| Check | Independent result |
| --- | --- |
| `cargo build --workspace --locked` | PASS, exit 0; rebuilt binary used for all probes/gates. |
| `node scripts/test-ai-lint-report-contracts.mjs` | PASS, exit 0; `ok:true`, family `ai-lint-report`, schema `0.6.4`, `live:true`. |
| `node scripts/test-ai-lint-evidence-contracts.mjs` | PASS, exit 0; corresponding `ok:true` / `live:true`, including actual shipped bundles and installed TargetClient paths. |
| `node scripts/test-ai-lint-config-contracts.mjs` | PASS, exit 0; corresponding `ok:true` / `live:true`. |
| `node scripts/test-ai-lint-waivers-contracts.mjs` | PASS, exit 0; corresponding `ok:true` / `live:true`. |
| `node scripts/test-ai-lint-comparison-contracts.mjs` | PASS, exit 0; corresponding `ok:true` / `live:true`. |
| `cargo test -p lekalo-core --locked --lib ai_lint` | PASS, exit 0: 3 passed, 0 failed; includes typed/file location-bound parity and both existing depth tests. |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS, exit 0. |
| `node scripts/check-contract-versions.mjs --base be0c78a9` | PASS, exit 0; `ok:true`, product `0.6.4`, 109 contract artifacts. |
| Byte delta under `contracts/`, base to candidate | PASS: zero changed artifacts, including registry and protocol/evidence schemas. |
| Structural snapshot comparison, base to candidate | PASS: exactly two fixture changes, each exclusively `producer.artifactDigest`; all other fields/arrays deep-equal. |
| Manifest custody / shipped fix inspection | PASS: actual bundle SHA-256 and byte lengths match manifests and snapshot pins. Source and shipped diffs contain the same location-budget logic; Node output has normal bundler formatting. Manifest diffs refresh only integrity pins/lengths; the Rust exemplar assertion refreshes its exact package pin. |
| Gate weakening check | PASS: no existing line removed from `ai-lint-contract-gate.mjs`; changes add R1/R2 controls. Contract generators, acceptance bounds and registry remain unchanged. |
| CI dependency ordering | PASS: build at `.github/workflows/ci.yml:198` precedes the five family gates at `:258`. |
| `cargo fmt --all -- --check` | PASS, exit 0. |
| `git diff --check`; `git diff --check be0c78a9..HEAD` | Both PASS, exit 0. |
| Repository/fixture hygiene | Initial status clean. All source/evidence/install mutations use Temp fixture copies. Original fixture still has only `app`, `lekalo`, `src`; no ignored runtime residue. Only the named review file is added in the checkout. |

Validated snapshot pins:

| Target | Previous artifact digest | Current artifact digest |
| --- | --- | --- |
| Node | `sha256:b37be3ad83636ae0ee07ad65667fb20527f59eda3fde8813bd87cf794e079b54` | `sha256:b4e73cfbe5b6d0203560f9d86b4836b1b8b2a8bda8be8071be547ff4d2ba3a30` |
| PHP | `sha256:dc186afaeeb4404b3eec18ad313f4d3cf9343207eea5bd607abc4d3cace56779` | `sha256:56be3a5f1643051bb12b3bf8f91218b3736a7ec65ce1a097b8c7ec18fc0c9917` |

Local scratch evidence is retained at `C:/Users/User/AppData/Local/Temp/lekalo-fix-review-76-codex-20261004/`: standalone `probes.mjs`, `r1-results.json`, `r2-results.json`, `no-weakening.json` and disposable projects. Harness SHA-256: `3ddf47fdd1772e78f6689f376413116fbd11fa80a7b76188d2e9555a03abac65`. It was run from this checkout with `node <scratch>/probes.mjs r1` and `node <scratch>/probes.mjs r2`. Fresh directory names are required for another run.

## Delivery boundary

Review only. The local documentation commit is restricted to `docs/m7/issue-76-review2-codex.md`; staged paths and whitespace are checked before committing. Implementation, contracts, fixture bytes, gates and other worktrees are unchanged by this review. No push or hosted CI execution is included. This acceptance covers the specified fix delta and local checks; full workspace tests, MSRV and non-Windows lanes were not rerun in this review.
