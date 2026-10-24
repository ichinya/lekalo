# Feature status

Status: **Implemented** inventory of supported profiles for the source checkout. Owner: release/documentation maintainers. Exact behavior belongs to the linked references; [versioning](versioning.md) owns version rules.

## Status legend

- **Implemented:** a producer or handler and its checks exist within the documented profile; prerequisites and evidence limits apply.
- **Experimental:** a present path has qualification gaps or a contributor-only fallback.
- **Planned:** no supported positive end-user path is supplied by this checkout.

These labels describe availability. Passed, failed, ignored and unavailable describe one execution. A declaration or local pass does not qualify every adapter, host or production deployment.

| Feature | Status | Scope and reference |
| --- | --- | --- |
| Model, loading, validation and projections | **Implemented** | [Model](model.md), [architecture](architecture.md), [inspect](inspect.md), [impact](impact.md), [context](context.md) |
| Bootstrap and ownership modes | **Implemented** within declared profiles | [Adoption](adoption.md); observations never grant generation permission |
| Adapter protocol and generate/check/verify | **Implemented** | [Protocol](target-protocol.md), [orchestration](orchestration.md); capabilities and platform prerequisites apply |
| Change API, fixes, formatting and search | **Implemented** | [Change](change.md), [fixes](fixes.md), [format](format.md), [search](semantic-search.md); unsupported shapes refuse |
| Watch | **Experimental** | [CLI](cli.md#lekalo-watch-experimental-issue-95), [delivery boundary](target-protocol.md#issue-95-delivery-evidence-boundary) |
| MCP, provider discovery and evidence | **Implemented** local contracts | [MCP](mcp.md), [provider](provider-contract.md), [budget](context-budget.md), [reports](ci-reports.md), [history](run-history.md) |
| Synthetic Laravel backend/client and maintained Vue screen | **Implemented** bounded fixture | [Greenfield tutorial](tutorial-greenfield-planner.md); browser execution is not demonstrated |
| Synthetic TypeScript/MySQL runtime | **Implemented** bounded fixture | [Brownfield tutorial](tutorial-brownfield-typescript.md); HTTP and real persistence are separate lanes |
| Brownfield scan fallback | **Experimental** | [Observed mode](observed-mode.md); the contributor fallback does not qualify the primary scan wire |
| External workflow/workspace delivery | **Experimental** | [Integrations](integrations.md); discovery/trace validity is not real delivery |
| Local aggregate metrics export | **Implemented** | [Metrics export](metrics-export.md); no uploader or measured Framework Lift outcome |
| Worklog/Changelog and task links | **Implemented** pilot profiles | [Worklog/Changelog](worklog-changelog.md), [task links](task-external-links.md); no general provider qualification |
| Native production command execution | **Planned** | [Native gates](native-gates.md); fixture execution is a separate path |
| Automatic screen generation | **Planned** | [Greenfield tutorial](tutorial-greenfield-planner.md), [screen decision](adr/0046-no-screen-view-ir-kind.md) |

## Release documentation

Keep the installed binary's tag documentation separate from this checkout's source documentation. [Release snapshots](releases.md#release-snapshots) explains the relationship, including the older `v0.8.0` tag and later M8 completion on `main`.

## Contributor verification

Follow [contributing](contributing.md#choose-checks-for-the-change) for setup and checks. Required CI includes portable documentation replay, the planner native lane and MySQL validation. Bind receipts to the source revision and retain failures, ignored tests and unavailable prerequisites. A prose edit is not a new runtime result.
