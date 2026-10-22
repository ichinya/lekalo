# M8 acceptance status

GitHub milestone [M8](https://github.com/ichinya/lekalo/milestone/9) was
checked on 2026-10-22: **14 open issues**. PR #160 has merged into `main`;
remaining implementation continues in draft [PR #161](https://github.com/ichinya/lekalo/pull/161)
on `ichinya/m8-completion`. This inventory does not close issues. A merged implementation is
not, by itself, acceptance evidence for every criterion in an issue.

The current criterion-by-criterion evidence and supported-profile boundaries
are in [acceptance-criteria.md](acceptance-criteria.md). The source candidate
`087881a1` passed the complete Windows workspace run: **3,034 passed, zero
failed, six ignored**. Four opt-in native tests were then qualified separately
by the real Rust/Laravel SQLite process battery; two cache benchmarks remain
ignored. Final exact-delivery CI and review status are maintained in PR #161.

| Issue | Available implementation / qualification entry points | Current evidence / boundary |
|---|---|---|
| [#19](https://github.com/ichinya/lekalo/issues/19) portability | Live schemas, planner projections and independent classification oracle | Passed locally: nine schemas, 31 subjects, 48 planning rows, 36 controls; unsupported/unknown remain distinct. |
| [#33](https://github.com/ichinya/lekalo/issues/33) MCP | Scoped stdio, actual host SDK/AI Factory installation and source-version context | Local legacy/successor gates pass; three-OS installer evidence at `2996132c`; exact-delivery hosted status in PR. No inference. |
| [#41](https://github.com/ichinya/lekalo/issues/41) managed | Deterministic ownership, complete manifests and planner regeneration | Contract gates, 110 CLI invocations and delete/regenerate/native-handler oracle pass locally. |
| [#51](https://github.com/ichinya/lekalo/issues/51) Worklog / Changelog | Contract/admission/pilot/impact/neutrality | All local gates pass, including expected strict Go denial and explicit permissive unsupported publication. |
| [#52](https://github.com/ichinya/lekalo/issues/52) TaskExternalLink | Public/synthetic provider plus native Rust/Laravel SQLite battery | Contract/scenario/outbox/impact/public gates and actual maintained-port race/kill/restart/retry battery pass. Other adapters do not inherit native success. |
| [#73](https://github.com/ichinya/lekalo/issues/73) Change API | Typed Model/IR 0.8.0 edits and owner-compatible consumers; sealed last-home retirement | Change 114 tests, transaction 20 unit tests/101 live steps and successor gates pass. Seven live ACs are mapped; separately owned attachment relocation and unrepresentable consumer shapes explicitly refuse. Module policy changes retain the owner's granularity. |
| [#74](https://github.com/ichinya/lekalo/issues/74) safe fixes | Scoped plans, replay, stale/conflict and rollback | Two schemas, ten rules, five goldens and 26 live checks pass locally. |
| [#93](https://github.com/ichinya/lekalo/issues/93) semantic search | Exact filters, ranked cards, offline index and rg comparison | 99 live checks and 20/20 declared benchmark correctness cases pass. Retrieval precision/latency are measured, not universal answer-quality claims. |
| [#94](https://github.com/ichinya/lekalo/issues/94) formatter | Semantic/trivia invariance, atomicity and portable golden | All four local gates pass; exact-delivery three-OS evidence remains a hosted CI requirement. |
| [#95](https://github.com/ichinya/lekalo/issues/95) watch | Native/poll, boundaries, equivalence, faults, successors and consumers | All seven gates pass locally. Linux/macOS are hosted lanes, not local Windows results. |
| [#96](https://github.com/ichinya/lekalo/issues/96) explain | Diagnostic registry, locales and executable fix links | 644/644 explanations and live/schema/locale/fix-reference controls pass. |
| [#106](https://github.com/ichinya/lekalo/issues/106) adapter SDK | Four-language vectors, real host conformance and detached package verification | All pass locally: 1,145 vectors per language, 51 lifecycle pairs, 1,297 production-oracle rows, strict TS/Rust/control with three repeats, six examples and four package lanes. Context-provider research is not an additional AC. |
| [#113](https://github.com/ichinya/lekalo/issues/113) module boundaries | Strict visibility, public consumers/context and independent implementation mappings | Eight schemas/22 families pass locally. In-process/service contract mappings are qualified; no remote deployment claim. |
| [#136](https://github.com/ichinya/lekalo/issues/136) incremental adoption | Read-only proposal, actual native refactor, rollback and hybrid modes | Contracts/refactor/hybrid and all ten tutorial blocks pass locally, including hostile custody/stale-input/recovery controls. Native Git Bash selection avoids the unavailable WSL launcher. |

## Historical implementation checkpoints

The entries below retain earlier candidate-specific results and then-pending
work. They are historical snapshots; use the current matrix above and the
criterion evidence for present status. In particular, earlier native
`pending-provisioning`, unpublished-commit and context/relocation statements
have been superseded by later entries and current qualification.

- Symbol relocation now retires a last-definition kind home with an explicit
  delete plan and removed receipt, sealed preimage/provenance, backup,
  parent-confined unlink, post-inventory validation and rollback/recovery.
  Empty-file recreation never passes an absence postcondition. Document
  comments outside the moved declaration refuse before mutation. Nested
  relation/uniqueness IDs stay stable as members while their typed references
  follow the owner move; they never become fabricated definition-history rows.
  The live gate passes 24 calls, five commits and three schemas; all 20
  transaction unit tests pass, including four new fault/drift/backup controls.
  The Change integration suite passes all 114 tests after replacing obsolete
  context-refusal assertions with exact successor-fact checks. Existing live
  transaction gates pass 101 steps and the contract gate passes six schemas.
  Separately owned attachment profiles and final issue acceptance remain
  explicit qualification boundaries.
- Context 0.8.0 projects query inputs, command results/direct emissions,
  exact defaults and enum wire literals, relations and uniqueness into
  JSON and Markdown with whole-fact budget accounting. Model 0.2.16 keeps
  its old owner and golden. MCP context/graph select their contract pins
  from the admitted Model under the existing snapshot fence; schemas
  reject mismatched owner/pin pairs. A real official SDK host gate passes
  39 calls and four schemas, including both context modes, all four graph
  modes, a one-token budget and unchanged source/runtime inventory.
  Workflow-provider 0.8.0 advertises both context owners through a closed
  source-version success schema; predecessor discovery artifacts stay
  frozen. Provider core/CLI and legacy context/MCP gates pass locally.
  Final relocation and milestone acceptance remain separate work.
- The rebuilt Node bundle's static AI-lint producer receipt and package
  manifest now carry its actual artifact digest. Five AI-lint gates and
  both adapter-manifest golden checks pass without model inference.
  Hosted CI at `569730de` reached the full Rust suite and exposed one
  remaining predecessor package digest literal in its exemplar test;
  that literal is corrected to the actual independently verified package.
  A new hosted run is still required after this correction.
- Transport-http and client-SDK have exact 0.8.0 projection owners. Successor
  query parameters join canonical Model input facts through query-model;
  required wire coverage, actual IR digests and mixed-version refusals are
  checked. CLI, watch and generation bind the same immutable Model snapshot.
  OpenAPI 3.0/3.1 retains defaults and nested nullability; SDK query parameters,
  whole and explicit bodies preserve supported types and presence. Deeper
  shapes outside the SDK owner, defaults and enum literals remain explicit
  refusals. The CLI gate passes 25 calls, including clean/watch equivalence
  and a predecessor-adapter incompatibility control. Core schemas and direct
  API bypass controls pass.
  The native SDK kernel gate typechecks and executes generated TypeScript
  and runs generated Go tests without network access; absent versus present
  empty strings survive. Native parameter lists/null/cookies refuse rather
  than become strings. This does not qualify general CLI native generation
  or the still-pending context/relocation work.
- CI follow-up: Rust 1.80 Clippy rejected a query-model boolean expression
  that current Clippy accepted; the equivalent expression now passes the
  actual 1.80 all-target lane locally. Fixture provenance uses bounded Git
  calls and file-backed clean-filter canaries. A real private Git repository
  proves that forced text normalization and a corrupt custom clean filter
  still refuse, including a 256 KiB binary and preservation of a foreign
  caller index. Portable documentation replay passes nine examples, 25
  commands and 21 controls with source bytes preserved. The earlier macOS
  documentation timeout has no confirmed root cause. At `85169eb7`, hosted
  documentation replay and Rust 1.80 pass on all three OSes (run
  `39775208513`); Linux and macOS complete build/test successfully. Each later
  milestone candidate still requires its own CI evidence.
- Query-model 0.8.0 now joins each declared parameter to the canonical query
  input by name, type, explicit presence and exact typed default, including
  null versus absence. Model and IR pins both validate; mixed or stale pins
  refuse. The successor supports all 256 Model input fields while the frozen
  predecessor keeps its 64-parameter representation. A pure `resolve_model`
  validates a prospective immutable snapshot without rereading or changing
  source files. The live CLI gate passes 17 calls and the independent successor
  schema; 35 existing core unit tests and 14 legacy CLI tests pass, including
  a false-IR-pin control. The snapshot/default/presence integration test,
  Clippy and MSRV also pass. The transport/OpenAPI/SDK projection increment
  above follows this checkpoint; context remains separate work.
- Model/IR 0.8.0 commands support direct event `emits` independently of
  CRUD effects. Change add/remove, including the last emission, preserve the
  source and durable replay; a declaration and emission compose in one request.
  The CLI gate passes 15 calls and eight schemas. Graph, inspect, search,
  portability and signature diff carry the declaration. Unsupported execution
  and projection owners refuse explicitly. The effect-graph 0.8.0 successor and
  existing 0.8.0 extended-effect/transaction owners carry matching version pins;
  mixed-version joins refuse. Their legacy golden bytes stay unchanged. The
  synthetic TaskExternalLink fixtures were regenerated and their contract,
  scenario, outbox, impact/portability and public-evidence gates pass. Its local
  native gate remains `pending-provisioning`; mock evidence does not qualify
  durable database/race behavior. This is not final milestone acceptance.
- Model/IR 0.8.0 commands now carry optional typed `returns` through
  declaration, lossless first insertion/replacement, validation, canonical IR,
  graph, inspect, diff, reference renames, portability and public module exports.
  Error-output bindings compare against the actual command result; error-union
  edits preserve it. Same-type output edits are byte-identical no-ops.
  Legacy command declarations refuse the member. Consumers lacking a compatible
  result contract explicitly refuse rather than discard it. The local CLI gate
  passes 13 calls and seven schemas, including preview/apply/replay, no-op,
  CRLF/no-EOF/comment preservation and invalid-type controls. Dedicated core
  tests cover public type allowlists and exact error-output binding. This is an
  implementation increment, not final #73 or milestone acceptance.
- Model/IR 0.8.0 query inputs now support declaration, first-list insertion,
  replacement and clearing through Change. Shared validation resolves their
  types and checks defaults. Graph `query-input`/`accepts` edges, signature
  diff, inspect, Context Budget and portability dependencies carry them.
  The source writer preserves CRLF without an EOF newline and inline type /
  description comments, including quoted apostrophes. Clearing inputs and
  changing types retain the migration-evidence policy gate. Frozen query
  parameter consumers explicitly refuse this unrepresentable successor;
  their owner-compatible joins remain follow-up work, not executed generation.
  The final Windows run passed **1,693 tests**, zero failed and two ignored:
  `cargo test --workspace --lib --test change --test ir --test diff --test graph --test inspect --locked --offline -j 1`
  (`target/m8-query-input-final.log`). It includes **105 core Change tests**
  and **1,510 core library tests**. Workspace/all-target Clippy passed with
  `-- -D warnings` (`target/m8-query-input-clippy.log`). The fresh checkout
  CLI gate passed **20 calls and seven schemas**, including source-trivia,
  replay, no-op, invalid/default and migration-policy cases. Default, enum,
  legacy Change and extended CLI gates passed on that binary; graph/inspect
  live successor schemas and the 554-surface docs gate also passed. Earlier
  failed/interrupted runs remain under `target/m8-query-input-*`; the final
  successful run supersedes those results. This is a scoped local run, not
  full workspace, hosted CI, Linux/macOS or final milestone acceptance.
- Explicit enum wire values now survive Change add/change and Model/IR
  0.8.0 decoding, including structured JSON and explicit null. Effective
  encodings are unique; enum defaults use those encodings. Encoding changes
  have a dedicated breaking diff kind, bounded canonical digests and a
  migration-evidence policy gate. Core OpenAPI retains mixed JSON encodings;
  frozen consumers refuse unsupported projections. Symbol moves preserve
  literal data and now accept successor symbol rename-history claims.
  The final Change/IR/diff run passed **140 tests** (102 core Change tests),
  zero failures or ignored tests. The CLI gate passed **17 calls and five
  independent JSON Schemas**; defaults (11 calls), legacy Change CLI
  (22 steps), symbol relocation (14 calls) and extended operations (16 calls)
  also passed. Contract/schema and live 554-surface documentation gates pass.
  Workspace/all-target Clippy passed with warnings denied; formatter and
  whitespace checks passed. These are local Windows checks, not hosted CI.
  The restart interrupted compilation, with no test result; that unfinished
  run was resumed by a new process. Earlier failed runs are retained under
  `target/m8-enum-*.log` and do not establish milestone acceptance.
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
