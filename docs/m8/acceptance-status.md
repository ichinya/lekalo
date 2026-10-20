# M8 acceptance status

GitHub milestone [M8](https://github.com/ichinya/lekalo/milestone/9) was
checked on 2026-10-20: **14 open issues, zero closed**. This inventory does
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
| [#41](https://github.com/ichinya/lekalo/issues/41) managed | `scripts/test-managed-contracts.mjs`, `test-managed-cli.mjs`, `test-managed-planner-e2e.mjs` | The legacy renderer fixture's query reads a command and fails shared Model validation. Correct the fixture, qualify the managed authority against that validation, and recheck the deterministic/native evidence oracles before acceptance. |
| [#51](https://github.com/ichinya/lekalo/issues/51) Worklog / Changelog | contract, admission, pilot and consumer gates | Final native/evidence qualification, keeping unsupported publication semantics distinct from executed success. |
| [#52](https://github.com/ichinya/lekalo/issues/52) TaskExternalLink | relation/uniqueness Model successor, task-link contracts and impact/portability gates | Final public/synthetic provider evidence and exact consumer impact qualification. |
| [#73](https://github.com/ichinya/lekalo/issues/73) Change API | Change contracts, CLI, transactions, bindings, error unions, symbol relocation and module-ownership gates | Completion of relocation/attachment cases; field defaults, enum wire values, query inputs, command results and pure emissions with owner-compatible Model/IR successors. Module policy transitions use the existing managed owner's granularity; per-symbol changes do not silently widen to a module. Catalog coverage alone cannot close this issue. |
| [#74](https://github.com/ichinya/lekalo/issues/74) safe fixes | Fix contracts/CLI and rollback tests | Requalify final Change catalog identity, stale/conflicting plans, replay and rollback. |
| [#93](https://github.com/ichinya/lekalo/issues/93) semantic search | search contracts/CLI and `scripts/bench-search.mjs` | Final result/ambiguity/offline gates and representative benchmark evidence; separate retrieval metrics from agent answer quality. |
| [#94](https://github.com/ichinya/lekalo/issues/94) formatter | format contracts, CLI, atomicity and portability gates | Final semantic invariance/trivia checks and Linux/Windows/macOS evidence. |
| [#95](https://github.com/ichinya/lekalo/issues/95) watch | platform, equivalence, fault, consumer and boundary gates | Linux/macOS watcher execution remains unqualified in this Windows session; repeat the final Windows lane after source completion. |
| [#96](https://github.com/ichinya/lekalo/issues/96) explain | 644 diagnostic explanations, English/Russian and contract/runtime gates | Final registry coverage, executable safe-fix links and locale-independent machine contract qualification. |
| [#106](https://github.com/ichinya/lekalo/issues/106) adapter SDK | generated codecs/templates, vectors, confined conformance and detached package gates | Requalify SDK/template/host-policy/protocol-upgrade gates against the actual issue checklist. A separate context-provider protocol is explicitly future work in the research; its Planned status alone does not prove an unmet acceptance criterion. |
| [#113](https://github.com/ichinya/lekalo/issues/113) module boundaries | Module API contracts, CLI, strict validation, diff/consumer-context gates | Final boundary/path/portability checks, distinguishing in-process and extracted-service evidence. |
| [#136](https://github.com/ichinya/lekalo/issues/136) incremental adoption | adoption contracts, hybrid/refactor/tutorial gates | Final synthetic end-to-end acceptance: confirmation preservation, selective native gates, rollback, stale evidence and hybrid boundaries. |

## Verified local corrections

- Module ownership: the scoped workspace run passed **1,611 tests**
  (CLI library 9, core library 1,506, Change integration 96), with zero
  failures and two ignored tests. It includes rollback and retained-foreign-edit
  custody cases. The live CLI/schema gate passed **18 calls**, two policy
  commits and a no-op, preserving native bytes and unselected legacy rows.
  Managed evidence uses a valid local Model fixture with recomputed owner
  pins; the shared legacy renderer fixture issue in #41 remains explicit.
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
