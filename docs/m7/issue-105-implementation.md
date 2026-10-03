# Issue #105 implementation: architecture docs, quickstart and tutorials

Implemented locally on `ichinya/m7-issue-105`, 2026-10-03, from the approved [research](issue-105-research.md) (`33e2530d`) and exact requested product base `a56ee5786f1724f9ee61ef973088c7679684c14d`. Product remains `0.6.3`. Acceptance authority is [live issue #105](https://github.com/ichinya/lekalo/issues/105), re-read during implementation. This report distinguishes local replay, CI wiring and external acceptance.

Implementation commits: `1ec46afe` (P0 entry documentation and owners), `6d5fe6a6` (fixtures, replay/ownership gates and CI). The report follows in its own commit. Nothing was pushed. No tracked file under `crates/` or `contracts/` changed; no other worktree or real consumer repository was used. The approved research is unchanged.

## What was built

### Required pages and canonical terminology

Created the eight missing pages: [architecture](../architecture.md), [project layout](../project-layout.md), [adoption](../adoption.md), [security](../security.md), [integrations](../integrations.md), [greenfield tutorial](../tutorial-greenfield-planner.md), [brownfield tutorial](../tutorial-brownfield-typescript.md), [roadmap](../roadmap.md). Updated [README](../../README.md), authority, Model, target protocol and diagnostics; all thirteen required entry pages are English primary.

[Model glossary](../model.md#glossary) is the one terminology owner. The P0 pages link exact specialist references, issues and full ADR filenames. The architecture diagram identifies loader/IR, pure semantics, bounded core I/O, adapter staging, evidence joins and consumer roles. Target negotiation now explicitly admits only the current producer's singleton `0.3.2` set; stale provisional IDs, current-registry pointers and authority prose were reconciled against actual sources.

[README A-F](../../README.md#quickstart-paths-a-f) links the minimal project, adoption, contracted module, projections, generation/check/verification and trace/provider handoffs. Feature status is separate from command verdict: **Implemented** bounded behavior, **Experimental** restricted scan/external qualification, **Planned** unavailable production native execution, automatic screens and aggregate history export. Native plan validation's exit-0 blocked/unsupported receipt is documented separately from DomainResult exits.

### Complete owner inventory

[documentation-owners.json](../documentation-owners.json) records **318** exact surfaces:

| Surface | Count | Primary accuracy owner / verification |
| --- | --- | --- |
| Recursive CLI commands/groups | 151, including all 41 top-level commands | Specialist reference per command; [CLI index](../cli.md#command-index), actual recursive help and normalized source pin |
| Globals | 4 | CLI reference; exact global-set comparison |
| Contract JSON files | 110 | Exact file/family specialist owner and byte digest; presence is not version admission |
| Protocols / maintenance formats | 53 | Compiled IR, 9 target operations, 36 adapter wire identities, 5 published suite schemas and 2 documentation-maintenance formats |

[Ownership gate](../../scripts/test-docs-ownership.mjs) checks missing/duplicate/unknown owners, source and contract drift, exact protocol census, P0 pages, local links/anchors, glossary and public-content controls. Static mode runs without a binary; live mode compares help from the built CLI. [Explicit writer](../../scripts/update-docs-owners.mjs) requires `--write`; CI never repairs metadata to make a check pass. Rust source pins normalize checkout newlines, while exact-byte JSON contracts retain their own digest domain.

### Fixtures and replay

[Greenfield planner](../tutorial-greenfield-planner.md) reuses the existing synthetic orchestration/scenario/trace corpus and pinned Laravel/Vue fixtures. Actual generation uses the existing adapter dry-run/apply/verify harness; the later committed scenario corpus and trace remain explicit shared inputs, not invented outputs of earlier stages. The CLI lock/check/verify example honestly checks an initially empty artifact inventory and reports unsupported native execution components.

The [new MySQL consumer](../../tests/fixtures/pilot/brownfield-mysql/README.md) uses actual locked Hono `4.13.12`, Drizzle `0.44.7`, mysql2 `3.24.5`, TypeScript `5.9.3` and Node types `24.10.1`. It has a MySQL schema/migration, real driver-backed runtime repository and actual Hono request tests. An isolated MySQL `8.4.5` service proved create/read, exact `ER_DUP_ENTRY` refusal and transaction rollback. Offline Hono/Drizzle declarations are authored scanner aids, required to exist and excluded from runtime typechecking. Query mocks and the unscanned runtime directory are explicitly separate from persistence proof. The existing SQLite pilot is unchanged.

A [contracted tutorial variant](../../tests/fixtures/docs/contracted-module/README.md) derives only from the existing synthetic planner slice. It omits that slice's deliberately unimplemented count query and absent generated support artifact, fixes maintained TS import spelling for Node 24, records exact new fingerprints and executes two named maintained-code tests before attaching their IDs. Clean conformance and subsequent source-fingerprint drift are both replayed; this does not establish all domain effects or database coverage.

The `docs` provenance family and the `pilot` MySQL note are registered in [fixture provenance](../../tests/fixtures/fixture-provenance.json). Its closed census passes with **69 synthetic families**, zero evidence-backed/private imports.

[Example registry](../../tests/fixtures/docs/examples.json) binds **12 examples / 28 displayed commands** and **2 visible setup blocks** to documents, fixtures, required lanes, exit/stream and independent semantic checks. [Replay gate](../../scripts/test-docs-examples.mjs):

- rejects unregistered/unclassified P0 fences, changed argv, duplicate IDs, unknown fields/checks and unsafe paths;
- executes literal argv without a shell against fresh copies and actual producers;
- validates applicable published schemas, expected symbols/counts/digests, budget inclusion/exclusion, typed refusals and native test outcomes;
- repeats read-only outputs and checks source preservation, init idempotence/conflict, malformed Model, unknown symbol and controlled contracted/brownfield drift;
- preserves tracked fixture inputs under runtime-looking homes, including the native selection snapshot; excludes untracked runtime leftovers and separately provisions locked dependencies;
- overrides child temp homes beneath its owned sandbox, cleans that bounded root and emits role-neutral counts rather than raw native logs or checkout paths.

Setup is explicit: the CLI is built beforehand, Composer installs from its committed lock, Ajv/TS/Vue are pinned outside the checkout, and npm installs from the new fixture lock. A required native lane with missing prerequisites fails rather than becoming a passing skip.

## Acceptance criteria and evidence

| Issue acceptance criterion | Delivered evidence | Local result / boundary |
| --- | --- | --- |
| A contributor can run a valid minimal quickstart fixture | README `read-minimal`; layout `minimal-init`; actual loader-valid zero-module corpus and empty-directory bootstrap | IR and strict validation exit 0; read-only preservation, malformed input, conflict refusal and init repeat checked. The older structure-only placeholder was not misrepresented as loader-valid. |
| Greenfield tutorial demonstrates Laravel + Vue without a real consumer | Existing synthetic planner/runtime/client/Vue inputs; `planner-chain`, `generate-check`, `laravel-vue-native` | Six planner chain stages and all nine baseline Laravel/Vue legs pass. Maintained screen/SFC/client evidence is explicit; no browser execution or automatic screen generation claimed. |
| Brownfield tutorial uses synthetic Hono + Drizzle + MySQL | NEW `pilot/brownfield-mysql`; real-package typecheck/HTTP and actual MySQL service; `mysql-observed` | Adoption harness 20/20 stages; native HTTP and MySQL tests pass. Wire refusal/fallback is recorded as experimental; SQLite cannot satisfy the persistence lane. |
| Authority matrix and three ownership modes are explicit | Authority's 57-kind machine registry summary and observed/contracted/managed tables; adoption and artifact-lifecycle links | Exact accepted authority/privacy checkers pass. Modes, authority lifecycle and export classification remain distinct; no Model `mode` field invented. |
| Every public command/protocol/schema has a documentation owner | 318-record inventory, generated help synopsis index, suite/maintenance owners, live/static ownership checks | Both modes pass; 13 required pages and local anchors checked. Schema filename order is not admission. |
| Code examples execute/validate in CI | [ci.yml](../../.github/workflows/ci.yml): static checks on Node 18/24, portable and planner replay in the 3-OS build matrix, dedicated immutable-image MySQL job | All three lanes pass locally; YAML and lane/prerequisite wiring validated. Hosted CI has not run for these unpushed commits. |
| Implemented/planned/experimental statuses are unambiguous | Opening status records, roadmap legend/matrix, tutorial capability limits, family-specific exit documentation | Native CLI plan receipt, scan fallback, absent browser/screen generation and planned aggregate export are explicitly bounded. |
| README has concise one-model/multiple-target positioning | Opening sentence and A-F navigation | Present in English; supported target slices differ explicitly. |
| Public docs have no private consumer names or URLs | Synthetic provenance, role aliases, public-prose canary checks and manual review of changed prose/links | Checks pass across public Markdown, with no historical exclusion. Changed examples use fictional roles; automated controls do not claim detection of every unknown private identity. |

Additional issue requirements: commands show expected outputs/exits; security links bind authority, privacy, confinement and isolated native setup; exact issue/ADR links are included; stable releases use immutable matching Git tag/docs/contracts/fixture-lock snapshots. Existing tags are not rewritten or claimed to contain #105 replay. An optional Russian overview was not added, avoiding a second normative terminology set.

## Local verification

Locked build passed: `cargo build -p lekalo-cli --locked`. Local Windows binary SHA-256: `f2fae0c27aa18ed5a0be963145e6ffca79bfbd2024d4f07269f43119b8817799`. Replay used Node `24.13.0`, PHP `8.5`, Composer `2.9.2`, pinned Ajv `8.17.1` and the fixture locks. Native planner custody verified installed Composer versions against the lock plus external TS/Vue pins.

| Gate | Recorded result |
| --- | --- |
| `test-docs-ownership.mjs` / `--static` | Pass; 318 surfaces, 13 required pages |
| `test-docs-examples.mjs --static` | Pass; 12 examples, 2 setup blocks, 4 metadata controls |
| `test-docs-examples.mjs` | Pass; portable: 9 examples, 24 commands, 18 controls, source preserved |
| `test-docs-examples.mjs --lane planner` | Pass; 1 displayed orchestrator, all 9 baseline legs, source preserved |
| `test-docs-examples.mjs --lane mysql` | Pass; 2 examples, 3 commands, actual MySQL/HTTP tests, source preserved |
| New MySQL observed harness | Pass; 20 steps, 0 failed |
| `test-node-hono-bindings.mjs`, `test-node-hono-readonly.mjs` | Pass; 45 binding tests plus readonly fixture checks |
| `test-node-drizzle.mjs` | Pass; 42 tests, no skips |
| `test-pilot-brownfield-ts.mjs` | Pass; original SQLite pilot regression |
| `test-contracted-contracts.mjs` | Pass; 4 declaration symbols, 6 refusal vectors |
| `test-golden-catalog.mjs`, `test-golden-hygiene.mjs` | Pass; 21 catalog cases, 259 hygiene files / 6 controls |
| Authority/privacy/provenance checkers | Pass through displayed security commands; 69 synthetic families |
| CI YAML parse and lane wiring assertions | Pass; portable/planner on 3 OSes, 1 dedicated MySQL job |
| Node syntax checks, staged `git diff --check`, forbidden-directory diff | Pass; no crates/contracts changes |

The MySQL service was task-owned, loopback-only and checksum-pinned to the same image as CI. It is stopped after validation. Ignored local provisioning/build files are retained for reproducibility; they are not committed.

## Residual risks and limits

No remote CI, release publication, external workflow/HLV delivery, real workspace event send or production consumer acceptance was performed. Linux/macOS confinement qualification remains hosted-platform evidence; local Windows replay does not establish it. The native planner gate's confined PHP runtime qualification stays explicitly pending; its baseline leg passes planning/custody and a plan-only Rust refusal. Real Mago remains the separately provisioned existing CI job, not the tutorial's fake-Mago baseline.

Two extra probes confirmed pre-existing product/schema defects outside the new positive examples. At budget 1, the original planner's context producer exits 0 with empty `sections`, violating the capsule schema's `minProperties`; the documented example uses 5000 and the valid over-budget control uses 100. The original partial contracted corpus's absent support artifact produces `contracted.stale-artifact` with a path in `symbol`, violating the diagnostic semantic-ID constraint. Neither source/schema defect was changed under the explicit crates/contracts boundary, and neither is hidden by modifying schema validation. The new contracted variant and displayed examples pass their actual applicable schemas.

Documentation owners and public-content checks enforce bounded mechanical coverage, not correctness of arbitrary future prose or detection of unknown private identities. A subsystem/contract change still requires updating the owner content, metadata and relevant replay receipts together. The approved research's real-surface inventory, synthetic fixture strategy, EN-primary structure and exact-custody evidence bar remain the governing decisions.
