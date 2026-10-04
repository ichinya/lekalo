# Status, roadmap and versioned documentation

Status: **Implemented** inventory and replay for product 0.6.3; release publication is **Planned** by the release owner. Owner: release/documentation maintainers. [Parent #1](https://github.com/ichinya/lekalo/issues/1), [#105](https://github.com/ichinya/lekalo/issues/105), [version policy](versioning.md).

## Status legend

- **Implemented:** the named bounded producer/handler and checks exist. Prerequisites, target subset and evidence limits still apply; this is not universal production qualification.
- **Experimental:** a present path has explicit qualification gaps or contributor-only fallback. Its successful test does not pass a refused primary path.
- **Planned:** no supported positive end-user path for the named feature at this revision. It must not appear as a working quickstart.

Test results (passed/failed/skipped) are separate from these feature labels. Each advertised operation must preserve its actual status/exit and completeness.

| Slice | Status | Evidence and limit |
| --- | --- | --- |
| Model/IR/validation, projections, init/adopt, contracted checks, lock/ownership | **Implemented** | [Architecture](architecture.md), [Model](model.md), [adoption](adoption.md); no automatic application/toolchain creation. |
| Registered target protocol and generate/check/verify | **Implemented** | [Protocol](target-protocol.md), [orchestration](orchestration.md); only declared capabilities/platform prerequisites. |
| Context-budget, workflow-provider discovery, CI reports/local history | **Implemented** | [Budget](context-budget.md), [provider](provider-contract.md), [reports](ci-reports.md), [history](run-history.md); no empirical AI-quality or public aggregation claim. |
| Synthetic Laravel backend/client and maintained Vue screen | **Implemented** | [Greenfield tutorial](tutorial-greenfield-planner.md); browser execution and screen generation are **Planned**. |
| Synthetic Hono/MySQL runtime and declaration extraction | **Implemented** | [Brownfield tutorial](tutorial-brownfield-typescript.md); actual persistence and offline extraction are separately checked. |
| Brownfield monorepo scan fallback | **Experimental** | Recorded protocol refusal plus direct-kernel contributor harness; not full public scan-wire qualification. |
| Real external workflow/validation/workspace acceptance | **Experimental** | [Integrations](integrations.md); discovery/trace validity is not real delivery. Workspace production event admission remains unavailable by default. |
| Production native-command execution | **Planned** | CLI currently refuses; native test harness execution is not production `native run` support. |
| Public aggregated history metrics export | **Planned** | [#102](https://github.com/ichinya/lekalo/issues/102); generic privacy export does not implement aggregate preview/retention/invalidation. |

## Release documentation

The working docs describe product `0.6.3` at source base `a56ee578` plus #105 documentation changes. Selected refs: Model/IR `0.2.16`, target protocol and authority/privacy `0.3.2`, active diagnostic registry/provider/CI report `0.6.3`. Exact refs come from accepted manifests and the producing binary; do not choose the highest filename by convention.

Stable release documentation is an immutable [Git tag snapshot](https://github.com/ichinya/lekalo/tags): select a tag, then that tag's README, `docs/`, fixture locks and replay registry. For example, [v0.6.4 docs](https://github.com/ichinya/lekalo/tree/v0.6.4/docs) are **archival/unverified for #105 replay**, which was not present in that historical tag. They are not substituted for this branch's evidence. A new release records tag, source commit, selected refs and the matching required replay results; hosted CI/release publication is not claimed by local checks.

No historical tag is rewritten. A snapshot preserves documentation history without expanding runtime accepted versions or promising support/migrations for old data. [Versioning](versioning.md) remains authoritative: changed contracts take the changing commit's product version; unchanged exact contracts retain their version, subject to explicit frozen-restore rules. Documentation updates do not change domain contracts.

## Contributor verification

Build the locked CLI, use the exact fixture dependency setup, then run [the example gate](../scripts/test-docs-examples.mjs) and [ownership gate](../scripts/test-docs-ownership.mjs). CI requires portable replay on three OSes, the planner native lane and the dedicated MySQL lane. The implementation report records local gates and their limits; green local examples alone do not establish remote CI or production acceptance.
