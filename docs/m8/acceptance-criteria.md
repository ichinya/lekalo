# M8 criterion evidence

The 14 live issue checklists were read on 2026-10-22. AC numbers below follow
their order in each issue; they are not issue-closing checkboxes. The source
candidate is `087881a1175eabe3f724d25d95fa36b682eb1c7a`. The subsequent tutorial
launcher/documentation increment does not change the compiled product. Final
delivery CI and review status are recorded in [PR #161](https://github.com/ichinya/lekalo/pull/161).

## Execution record

- Windows: `cargo test --workspace --locked --offline --no-fail-fast` passed
  **3,034 tests**, zero failed, six ignored, across 122 suite summaries.
  Two ignored tests are cache benchmarks. The four opt-in TaskLinks native
  tests were executed separately by the native gate below, not credited as
  executed by this workspace command.
- Windows: all 17 Worklog, TaskLinks and watch gates in the acceptance batch
  passed. The local receipt binds the script digests and source commit to CLI
  SHA-256 `06e54752073ad3d4d6d323a4237f75fc02da8108449cb2f0c7227f201b47efcb`.
- Windows: formatter, adoption, portability, safe fixes, explain, search,
  managed regeneration and Module API gates passed as listed below. The
  adoption tutorial replayed all ten blocks using native Git Bash. Pinned
  PHP 8.3.35 used a run-owned configuration; no WSL/service restart was needed.
- The actual TaskLinks native process battery passed against Rust SQLite and
  the maintained Laravel 13.33.0/PDO SQLite port, including synchronized
  races, abrupt process termination, restart and durable retries. Its 94
  Composer dependencies came from the existing lock. This qualifies that
  profile, not arbitrary adapters or a production provider.
- SDK vectors passed 1,145 cases in each of Go, PHP, Rust and TypeScript,
  with 51 lifecycle pairs and 1,297 raw-byte production-oracle rows. Strict
  TypeScript/Rust/control conformance passed with three repeats, as did all
  six capability-limited examples. All four package qualification lanes
  passed. Hosted Linux/macOS/Windows qualification is distinct from these
  local Windows results.

Raw local records are under `target/m8-087881a1-workspace.log`,
`target/m8-087881a1-acceptance/receipt.json`,
`target/m8-087881a1-tasklinks-native-final.log`,
`target/m8-087881a1-search-benchmark.json` and
`target/m8-087881a1-sdk/receipt.json`. These ignored files are local receipts,
not portable public artifacts. The committed gates and CI logs provide the
replayable evidence.

## #19 — portability

Gate: `node scripts/test-portability-contracts.mjs` (nine schemas, 31 subjects,
48 planning rows and 36 controls), plus workspace portability tests.

| AC | Evidence / result |
| --- | --- |
| 1. Explain blockers and partial mappings | Independent classification/arithmetic oracle checks each report item and reason. Passed. |
| 2. Separate custom/foreign implementations | Fixture ownership and implementation classes are checked separately. Passed. |
| 3. Preserve different risk classes | Separate semantic, implementation, evidence and migration classifications; no single percentage substitutes for them. Passed. |
| 4. Exact target/profile versions | Schema and live planning rows require target/profile and evidence pins; stale/mismatched inputs refuse. Passed. |
| 5. Unsupported versus unknown/untested | Distinct statuses are tested, including evidence downgrade controls. Passed. |
| 6. Planner Node-to-PHP/Go examples | Both planner target projections execute in the gate. Passed within their declared mapping profiles. |
| 7. Migration planning output | Recipe/planning rows validate against the migration schemas and independent oracle. Passed. |

## #33 — MCP

Gates: `test-mcp-{contracts,stdio,host,boundaries,notifications,union,ai-factory}.mjs`
under `scripts/`, plus `test-context-successor.mjs` and workspace MCP tests.
Actual AI Factory 2.19.0 installation and MCP SDK 1.32.1 initialization passed
on three OSes at `2996132c` ([run](https://github.com/ichinya/lekalo/actions/runs/39758553648));
final candidate qualification is tracked separately in the PR.

| AC | Evidence / result |
| --- | --- |
| 1. Read scope never widens policy | Admission, scoped reads, sensitive-path and before/after snapshot controls. Passed. |
| 2. One-call context capsule | Actual SDK tool call returns the admitted capsule; successor JSON/Markdown and tiny-budget cases are included. Passed. |
| 3. Stable validate/impact diagnostics | Tool output validates through the versioned schemas and core diagnostic classifications. Passed. |
| 4. Writes disabled by default | Default discovery and rejected write requests are checked. Passed. |
| 5. AI Factory extension | Real manifest installation, initialization, seven tools and resource access; no inference or user-configuration writes. Passed for the pinned host profile. |
| 6. Schema/scope/sensitive paths | Legacy gate has 383 checks; successor host has 39 calls/four schemas and rejects owner/pin mismatches. Passed. |
| 7. Standalone CLI survives unavailable MCP | Server startup/transport failures remain separate from ordinary CLI execution. Covered by workspace/host boundary tests. |

## #41 — managed generation

Gates: `test-managed-contracts.mjs`, `test-managed-cli.mjs` and
`test-managed-planner-e2e.mjs` under `scripts/`. Seven contract families,
12 rules and 110 live CLI invocations passed.

| AC | Evidence / result |
| --- | --- |
| 1. Same IR/lock gives stable bytes | Fresh-root deterministic generation and exact output oracle. Passed. |
| 2. Complete artifact manifest | Entire generated file and directory inventory compared with the manifest. Passed. |
| 3. Detect manual drift | Exact before-state, changed output and stale authority negative controls. Passed. |
| 4. Preserve custom implementations | Explicit bindings and mixed-mode isolation preserve unselected bytes. Passed. |
| 5. Capability gaps block migration | Missing capability/evidence refuses managed authority after shared semantic validation. Passed. |
| 6. Read-only generate check | Preview/check tree snapshots cover file bytes and directory inventory. Passed. |
| 7. Delete and reproduce planner | All generated application files and custody removed in a disposable copy, then regenerated from retained inputs; generated handlers actually execute. Passed; generation digest `0f3cbe8467e8f7b083c926cc6a06390d701cd27cabeb456321f9ed0ab74e245f`. |

## #51 — Worklog / Changelog

Gates: `test-worklog-changelog-{contracts,admission,pilot,impact,neutrality}.mjs`
under `scripts/`. Pilot commands use PHP strict, Go strict with
`--expect-denied`, and Go permissive, each with `--case all`.

| AC | Evidence / result |
| --- | --- |
| 1. Independent effect declarations | Contract/scenario/engine-pin parity checks independent effects. Passed. |
| 2. Preview and approval for publication | Admission and denied-publication pilot controls. Passed. |
| 3. Local completion survives partial failure | Pilot checks successful local work separately from undeclared external transaction semantics. Passed. |
| 4. Portable snapshot semantics | Target-independent snapshot contracts and target projection checks. Passed within declared profiles. |
| 5. Privacy/security/idempotency scenarios | Contract fixtures include 17 scenarios and 24 admission cases; pilot executes the selected target cases. Passed. |
| 6. All changelog consumers in impact | Independent impact expectations and stale/omitted-consumer controls. Passed. |
| 7. PHP/Go unsupported publication visible | Go strict denial is expected and tested; permissive output retains unsupported semantics instead of reporting success. Passed. |
| 8. Public consumer neutrality | Public artifact neutrality gate. Passed. |

## #52 — TaskExternalLink

Gates: `test-task-links-{contracts,scenarios,outbox,impact-portability,public-evidence,native}.mjs`
under `scripts/`. The native gate is explicitly opt-in with
`LEKALO_TASKLINKS_NATIVE=1` and a new run-owned SQLite database.

| AC | Evidence / result |
| --- | --- |
| 1. Relation cardinality/identity | Model/IR successor, independent fixture/schema and identity-race checks. Passed. |
| 2. First-class capability/policy | Policy/capability decisions appear in typed contracts and scenario evidence. Passed. |
| 3. External writes/partial failure effects | Effect-graph, outbox, retry and unknown-after-ack controls. Passed. |
| 4. Explicit conflict commands/audit | Conflict-resolution scenarios and declared audit effects. Passed. |
| 5. Public provider plus synthetic evidence | Public-only provider and synthetic mock scenario gates pass. Additional native durable/race evidence passes only for the maintained Rust/Laravel profile. |
| 6. Provider-to-consumer impact | Exact provider contract mutation and independent impact/portability expectations. Passed. |
| 7. Core versus adapter portability | Native success is scoped to its actual adapter; other profiles retain unknown/unsupported status. Passed. |
| 8. Public consumer neutrality | Public-evidence artifact scan. Passed. |

## #73 — typed Change API

Gates: `test-change-contracts.mjs`, the `test-change-*.mjs` live family and
`test-change-transactions.mjs` under `scripts/`, plus 114 core Change integration tests
and 20 transaction unit tests. The last-home/nested-member gate executes
24 calls, five commits and three schemas; the live transaction gate executes
101 steps.

| AC | Evidence / result |
| --- | --- |
| 1. Semantic add-field | Typed operations resolve IDs against the canonical Model; field defaults and exact literals survive. Passed. |
| 2. Semantic and file preview | Plan schemas and CLI cases include semantic impact, exact file diffs, creation and explicit last-home removal. Passed. |
| 3. Invalid/breaking policy | Validation, migration evidence and explicit policy controls refuse before publication. Passed. |
| 4. Idempotency key | Apply/replay/no-op and conflicting-key controls; recreated empty files fail an absence postcondition. Passed. |
| 5. Relevant impact/generation plan | Typed dependency/signature impact and generation decisions, including query inputs, results and direct emissions. Passed. Unsupported consumer owners refuse explicitly. |
| 6. Stable unrelated source | CRLF/no-EOF/comment preservation, exact foreign bytes, CAS, rollback/recovery and nested member identity. Passed. Ambiguous or unmapped comments refuse. |
| 7. CLI / future MCP usability | Public typed plan/apply boundary is exercised through CLI; default MCP stays read-only as required by #33. A future write tool can use the same API. |

The seven ACs above do not promise arbitrary attachment relocation. Cross-module
symbol moves currently refuse when separately owned attachments or unsupported
kind homes are present. They do not rewrite transport/query/authorization or
managed owners by text substitution. Model-only supported moves, last-home
retirement and stable nested relation/uniqueness members are qualified. This is
an explicit supported-profile boundary, not a claim that every possible move
works. SDK projections likewise refuse shapes/defaults/enum literals outside
their own owner instead of silently dropping them.

## #74 — safe fixes

Gates: `test-fix-contracts.mjs` and `test-fix-cli.mjs` under `scripts/`:
two schemas, ten rules, five goldens and 26 live checks.

| AC | Evidence / result |
| --- | --- |
| 1. Repeated apply | Exact replay/no-op assertions. Passed. |
| 2. Stale source fingerprint | Drift refuses before mutation. Passed. |
| 3. Explicit unsafe/breaking selection | Risk and selection policy negative controls. Passed. |
| 4. Post-validation rollback | Failed validation restores exact preimages. Passed. |
| 5. Machine-readable result | Independent plan/receipt schemas. Passed. |
| 6. Adapter fix scope | `adapterScopeRepair` cannot widen into canonical Model edits. Passed. |
| 7. Golden safe/stale/conflict/rollback | All declared golden classes plus live controls. Passed. |

## #93 — semantic search

Gates: `test-search-contracts.mjs`, `test-search-cli.mjs` and
`bench-search.mjs` under `scripts/`. Live gate: 99 checks, 33 schemas,
1,686-card scale case; warm/status/no-op measurements stayed below the
declared ten-second bounds on this Windows host.

| AC | Evidence / result |
| --- | --- |
| 1. Field readers/writers | Semantic effect filters and graph-backed results. Passed. |
| 2. Exact ID precedes fuzzy | Explicit rank assertions. Passed. |
| 3. Requirements/scenarios/bindings | Combined result families and exact filters. Passed. |
| 4. Selection metadata | Versioned cards carry kind, provenance, confidence and relationships. Passed. |
| 5. Honest ambiguity | Ambiguous queries retain candidates; benchmark 20/20 ambiguity-honest cases. Passed. |
| 6. Offline/no repeated target scan | Warm index and cache/source-access controls. Passed. |
| 7. Representative rg comparison | Twenty task-oriented cases executed for both tools; 20/20 declared correctness cases for each. Measured latency/bytes and broad-tail precision remain separate metrics; no universal speedup, perfect precision or agent-answer-quality claim. |

## #94 — canonical formatter

Gates: `test-format-{contracts,cli,atomic,portability}.mjs` under `scripts/`,
plus Rust core/CLI formatter suites. Eleven rules, twelve CLI cases, eight
atomicity cases and three Windows portability cases passed locally.

| AC | Evidence / result |
| --- | --- |
| 1. Idempotence | Repeated YAML/JSON formatting, grouping and stdin cases. Passed. |
| 2. Same output on all three OSes | Shared canonical golden/portability gate is mandatory on all three hosted lanes; final delivery status remains in the PR. Local evidence covers Windows only. |
| 3. Comments preserved or loss reported | Trivia fixtures and explicit refusal/loss reporting before write. Passed. |
| 4. CI read-only check | Check-mode exit classes and complete tree snapshot. Passed. |
| 5. Empty semantic diff | Before/after canonical IR comparison. Passed. |
| 6. No partial invalid rewrite | Parse/version refusal, foreign temporary preservation, forged recovery policy/preimage and concurrent drift controls. Passed. |
| 7. Stable planner | Committed planner oracle and second-run no-op. Passed. |

## #95 — watch

Gates: `test-watch-{contracts,platform,boundaries,equivalence,faults,successor,consumers}.mjs`
under `scripts/`. All seven passed on Windows at the source candidate.

| AC | Evidence / result |
| --- | --- |
| 1. Affected validation | Scoped native/poll snapshot and affected-set assertions. Passed. |
| 2. No generated-output loop | Generated/cache exclusions and bounded-run controls. Passed. |
| 3. Burst reaches latest snapshot | Equal-length in-place burst plus separate atomic replacement coverage; source revision assertions retained. Passed. |
| 4. Versioned JSON events | Independent event schemas and invalid-event controls. Passed. |
| 5. Delete/rename updates | Graph/binding invalidation and source-inventory scenarios. Passed. |
| 6. Clean-command equivalence | Exact settled validation/impact/generate-check and successor projection comparisons. Passed. |
| 7. Windows/Linux/macOS behavior | Same platform/fault/equivalence gates mandatory in each hosted lane; final delivery status remains in the PR. Local execution covers Windows only. |

## #96 — diagnostic explanations

Gate: `node scripts/test-explain-contracts.mjs`, plus workspace explain tests.
The gate checks all **644/644** registered diagnostics, live output, locale
invariance, schemas and safe-fix references.

| AC | Evidence / result |
| --- | --- |
| 1. Every blocking rule explained | Registry/catalog equality and examples. Passed. |
| 2. Stable JSON schema | Live responses independently schema-validated. Passed. |
| 3. Executable fix links | Registered safe-fix IDs resolve to actual recipes. Passed. |
| 4. Breaking alternatives labelled | Fix classification and visible breaking metadata. Passed. |
| 5. Adapter provider reference | Namespaced explanation preserves provider provenance. Passed. |
| 6. Locale-invariant contract | English/Russian projection keeps code and structured meaning. Passed. |
| 7. Missing-doc CI failure | Registry coverage is mandatory in Node and full hosted lanes; missing entries fail the gate. |

## #106 — adapter SDK

Gates: `test-adapter-sdk-{contracts,vectors,conformance,packages}.mjs` under
`scripts/`, generated-code parity and Go unit smoke. The vectors gate selects
`--language all`; conformance selects `--language all --profile strict --repeats 3 --timeout-ms 60000`;
the package gate selects `--verify` and never publishes.

| AC | Evidence / result |
| --- | --- |
| 1. SDK and handwritten fake | Same actual host battery compares templates with the independent handwritten control. Passed. |
| 2. TypeScript/Rust conformance | Both compiled templates and all six scan/generate/verify examples use the confined production host; strict reference/control runs use three repeats. Passed. |
| 3. Host policy remains authority | Real lifecycle/write-plan/hostile-mode and file-transport vectors: 51 lifecycle pairs, 40 hostile pairs and native plan-builder controls, plus workspace target-protocol confinement tests. Passed. |
| 4. Invalid response refuses | Full raw-byte valid/invalid/semantic/response corpus, invalid UTF-8, duplicate keys and 1,297-row production decoder oracle. Passed. |
| 5. Upgrade compatibility | Generated compatibility descriptors and upgrade-vector families checked against the complete on-disk corpus: 1,145 cases per language. Passed. |
| 6. Lifecycle/security documentation | `docs/adapter-sdk.md` and detached examples describe process, cancellation, bounds and write-plan authority. Contract/doc gates check declared package surfaces. |
| 7. No internal crate knowledge | TypeScript packed install, Rust extracted candidate consumer, Go external module and dependency-free PHP exchange. All four package lanes passed. |

A separately designed context-provider protocol is future research in #106;
it is not an additional acceptance checkbox in this issue. SDK capability
helpers do not authorize host access.

## #113 — module boundaries

Gate: `node scripts/test-module-api-contracts.mjs`: eight schemas and 22
check families, including independently authored in-process/service mappings.

| AC | Evidence / result |
| --- | --- |
| 1. Strict internal visibility | Internal cross-module access refuses; advisory behavior stays distinct. Passed. |
| 2. Exact breaking consumers | Public API diff, consumer declarations and lock/range assertions. Passed. |
| 3. Planner uses Task public surface | Public-only dependency fixture excludes provider internals. Passed. |
| 4. No target-specific public leak | Type allowlist and portable export negative controls. Passed. |
| 5. Cycles and forbidden writes explained | Explicit graph path and write-grant checks. Passed. |
| 6. Bounded public context | Certified export/context and public dependency closure controls. Passed. |
| 7. In-process/service boundary | Both independently authored implementation mappings preserve the same declared boundary; forged role/provenance/dependency controls refuse. Passed at contract-mapping level. No remote service deployment or general runtime-equivalence claim. |

## #136 — incremental adoption

Gates: `test-adoption-{contracts,refactor,hybrid,tutorial}.mjs` under `scripts/`.
Tutorial replay uses `LEKALO_TUTORIAL_BASH=C:/Program Files/Git/bin/bash.exe`
on this host because ambient `bash` resolves to the unavailable WSL launcher.

| AC | Evidence / result |
| --- | --- |
| 1. Minimal read-only analysis | Synthetic existing-project fixture, exact source snapshots and provenance/confidence/unknown assertions. Passed. |
| 2. Selective confirmation survives | Individual definitions/bindings and repeated-analysis preservation/drift assertions. Passed. |
| 3. Scoped preview/cancel | Exact recommendation/file list, source/model/lock/adapter pins and unchanged cancellation tree. Passed. |
| 4. Actual native behavior/rollback | Baseline and candidate gates execute under confinement; failed checks never complete; exact rollback and recovery preflight tested. Forged execution/receipt, consumed-input drift and corrupt snapshot controls pass. |
| 5. Three simultaneous modes | Untouched observed, adapted legacy and new contracted areas have status/impact/trace boundaries and gaps. Passed. |
| 6. No forced repository migration | Unselected source remains unchanged; hybrid stale evidence is detected on later audit. Passed. |
| 7. Executable tutorial | All ten shell blocks replay end-to-end in a disposable copy. Passed with native Git Bash; Windows paths are normalized and shell selection is explicit. |

## Acceptance boundaries

The matrix records executed behavior and explicit limitations, not a fresh
independent adversarial review verdict. Earlier formatter/adoption defect
classes informed the custody, stale-input, recovery and confinement controls;
their historical reports do not establish a current defect or a current pass.
No production provider, remote extracted service, arbitrary native adapter or
model inference is qualified here. Issue/milestone closure and merge remain
separate GitHub actions. Final exact-head hosted results belong in the PR so
documentation does not recursively claim success for an untested next commit.
