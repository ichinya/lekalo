# M8 acceptance status

GitHub milestone [M8](https://github.com/ichinya/lekalo/milestone/9) was
checked on 2026-10-21: **14 open issues, zero closed**. This inventory does
not close issues or publish the local branch. A merged implementation is
not, by itself, acceptance evidence for every criterion in an issue.

The integration branch is `ichinya/M8`. Local checkpoints `8befa2f0`
(project error unions and consumers) and `15ad603c` (PHP adapter input
scopes) are committed. Semantic symbol relocation is a subsequent work
increment; its limitations remain part of #73 until implemented and tested.

| Issue | Available implementation / qualification entry points | Remaining acceptance work |
|---|---|---|
| [#19](https://github.com/ichinya/lekalo/issues/19) portability | `scripts/test-portability-contracts.mjs`, core/CLI portability tests | Requalify exact final revision and check every blocker/partial/unknown criterion against the planner fixtures. |
| [#33](https://github.com/ichinya/lekalo/issues/33) MCP | stdio, scopes, subscriptions and host SDK gates; `docs/mcp.md` | Requalify final revision. An official SDK host smoke is distinct from installing or running a user's actual AI Factory extension. |
| [#41](https://github.com/ichinya/lekalo/issues/41) managed | `scripts/test-managed-contracts.mjs`, `test-managed-cli.mjs`, `test-managed-planner-e2e.mjs`; shared semantic Model validation now precedes managed authority | The query-read fixture and dependent authority pins are corrected. Contract gates, 110 CLI invocations and planner regeneration/native-handler execution pass locally. Final milestone-candidate/CI qualification remains. |
| [#51](https://github.com/ichinya/lekalo/issues/51) Worklog / Changelog | contract, admission, pilot and consumer gates | Final native/evidence qualification, keeping unsupported publication semantics distinct from executed success. |
| [#52](https://github.com/ichinya/lekalo/issues/52) TaskExternalLink | relation/uniqueness Model successor, task-link contracts and impact/portability gates | Final public/synthetic provider evidence and exact consumer impact qualification. |
| [#73](https://github.com/ichinya/lekalo/issues/73) Change API | Change contracts, CLI, transactions, bindings, error unions, symbol relocation, module ownership and Model/IR 0.8.0 field defaults | Completion of relocation/attachment cases; enum wire values, query inputs, command results and pure emissions with owner-compatible successors. Default literals retain the documented YAML-layout and legacy-consumer boundaries. Module policy transitions use the existing managed owner's granularity; per-symbol changes do not silently widen to a module. Catalog coverage alone cannot close this issue. |
| [#74](https://github.com/ichinya/lekalo/issues/74) safe fixes | Fix contracts/CLI and rollback tests | Requalify final Change catalog identity, stale/conflicting plans, replay and rollback. |
| [#93](https://github.com/ichinya/lekalo/issues/93) semantic search | search contracts/CLI and `scripts/bench-search.mjs` | Final result/ambiguity/offline gates and representative benchmark evidence; separate retrieval metrics from agent answer quality. |
| [#94](https://github.com/ichinya/lekalo/issues/94) formatter | format contracts, CLI, atomicity and portability gates | Final semantic invariance/trivia checks and Linux/Windows/macOS evidence. |
| [#95](https://github.com/ichinya/lekalo/issues/95) watch | platform, equivalence, fault, consumer and boundary gates | Linux/macOS watcher execution remains unqualified in this Windows session; repeat the final Windows lane after source completion. |
| [#96](https://github.com/ichinya/lekalo/issues/96) explain | 644 diagnostic explanations, English/Russian and contract/runtime gates | Final registry coverage, executable safe-fix links and locale-independent machine contract qualification. |
| [#106](https://github.com/ichinya/lekalo/issues/106) adapter SDK | generated codecs/templates, vectors, confined conformance and detached package gates | Requalify SDK/template/host-policy/protocol-upgrade gates against the actual issue checklist. A separate context-provider protocol is explicitly future work in the research; its Planned status alone does not prove an unmet acceptance criterion. |
| [#113](https://github.com/ichinya/lekalo/issues/113) module boundaries | Module API contracts, CLI, strict validation, diff/consumer-context gates | Final boundary/path/portability checks, distinguishing in-process and extracted-service evidence. |
| [#136](https://github.com/ichinya/lekalo/issues/136) incremental adoption | adoption contracts, hybrid/refactor/tutorial gates | Final synthetic end-to-end acceptance: confirmation preservation, selective native gates, rollback, stale evidence and hybrid boundaries. |

## Verified local corrections

- Field defaults now survive every Change field writer in the Model/IR
  0.8.0 successor, with type checks, bounded literals, null/absence
  preservation, behavioral diff records and bounded canonical-literal
  digests. Newly created kind documents inherit the source contract.
  The live CLI gate passed **11 calls and five independent JSON Schemas**;
  the final Change/IR/diff integration run passed **137 tests**, including
  **99 core Change tests**, with no failures or ignored tests. It includes
  preserved inline comments, CRLF without an EOF newline, no-op and safe
  refusals. Legacy CLI/symbol/binding/error-union gates also passed.
  Workspace/all-target Clippy passed with warnings denied after the final
  field-argument grouping; formatter and whitespace checks also passed.
  An earlier core-library run passed **1,510 tests**, with no failures and
  two ignored tests; it preceded the final source-token trivia correction,
  which the final integration run covers. Initial failed runs are retained
  in `target/m8-defaults-*.log`; they are not milestone acceptance evidence.
  Fixture copying excludes runtime caches. The one cache created by this
  session's checkout probe was moved into `target/` with matching SHA-256;
  no pre-existing user cache or native source was deleted.
- Managed authority now applies shared semantic validation after IR compilation;
  the negative regression proves that compilation alone cannot authorize an
  invalid query read. The corrected planner fixture validates with zero
  diagnostics. Its contract gate, **110 CLI invocations**, deterministic
  cross-root regeneration and executed generated-handler oracle all pass.
  Contract/CLI sandboxes exclude ignored checkout runtime state and the
  contract gate mutates only a disposable copy; no checkout cache is deleted.
- Module ownership: the scoped workspace run passed **1,611 tests**
  (CLI library 9, core library 1,506, Change integration 96), with zero
  failures and two ignored tests. It includes rollback and retained-foreign-edit
  custody cases. The live CLI/schema gate passed **18 calls**, two policy
  commits and a no-op, preserving native bytes and unselected legacy rows.
  Managed evidence uses the valid planner Model and recomputed owner pins;
  the initially detected shared-fixture issue is fixed by the managed-authority
  correction recorded above.
  Workspace/all-target Clippy, formatter checks, the 101-step transaction
  gate, Change/Fix regressions and the live 554-surface docs inventory passed.
- The symbol-relocation increment has **92 passing Change integration
  tests**, including all twelve symbol kinds, YAML/JSON rename history,
  CRLF without an EOF newline, dependency-cycle refusals, attachment
  drift and the owner's version range/overflow. The live CLI/schema gate
  passed **14 calls**, including three isolated commits, replay, CRLF and
  trailing-trivia preservation after the corresponding source correction.
- A broader core-library run passed **1,505 tests**, with zero failures
  and two ignored tests. That run preceded the final newline/version/trivia
  refinements; focused Change tests and CLI regressions qualify those
  refinements. It is not a completed full-workspace acceptance run.
- Workspace/all-target Clippy passed before the final trailing-trivia
  refinement; the core-library Clippy lane (`-D warnings`) passed on the
  refined source. The final milestone candidate still needs its complete
  workspace lane.
- The project error-registry increment passed a scoped Windows Rust run
  of **1,748 tests**, with zero failures and two ignored tests. This run
  explicitly excluded the then-failing PHP apply test; it was not a clean
  full-workspace acceptance run.
- The PHP correction then passed **23 native Rust tests with no skips**,
  including that apply test. Its adapter gate passed protocol (150 checks),
  process (40 checks), analyzer (61 checks), deterministic packaging and
  strict production conformance. The descriptor and manifest now agree on
  finite cache evidence scopes; the exclusive controller lock is outside
  those scopes.
- The earlier full Windows workspace attempt had six failures. Scoped
  corrections have evidence; a **new full-workspace run on the completed
  final candidate is still required**. An older successful revision's run
  cannot stand in for it.
- Hosted CI has not been run for these unpublished local commits. Linux
  and macOS results are **NOTRUN** in this session. Debian WSL is currently
  unreachable; the user explicitly requires continuing without restarting
  it or other services.

Before completion, record the final candidate commit, exact commands and
results, classify each required external lane as passed/failed/NOTRUN, and
compare the evidence with the live issue criteria. Keep code gaps and
unexecuted environment checks visible separately.
