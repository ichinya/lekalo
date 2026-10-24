# Documentation guide

Use a tutorial to complete a task, then its linked reference for the exact contract. Documentation on `main` follows the source checkout. Installed binaries use the matching [release snapshot](releases.md#release-snapshots).

For the complete domain/client path, use the [MVP user workflow](mvp-workflow.md).

## Getting started

| Goal | Main page |
| --- | --- |
| Download and verify the CLI | [Installation](releases.md) |
| Create a project and first module | [Project layout](project-layout.md#minimal-project) |
| Choose observed, contracted or managed ownership | [Adoption](adoption.md) |
| Check available features and limits | [Feature status](roadmap.md) |
| Build and run contributor checks | [Contributing](contributing.md) |
| Publish binary archives | [Release maintainer guide](releasing.md) |

## Tutorials

| Workflow | Working material |
| --- | --- |
| [Greenfield Laravel/Vue planner](tutorial-greenfield-planner.md) | Separate contracted and projection fixtures, generated client and maintained screen. |
| [Brownfield TypeScript/MySQL](tutorial-brownfield-typescript.md) | Synthetic Hono/Drizzle project; setup, HTTP checks, bounded adoption and database evidence. |
| [Incremental adoption](tutorial-incremental-adoption.md) | Analyze one use case, confirm it, verify/apply a refactor, recover and inspect hybrid coverage. |

Each walkthrough states its starting directory and prerequisites. Use disposable fixture copies; do not combine roots from different steps. Expected refusals are part of the instructions, not successful checks.

## Reference

| Area | Canonical pages |
| --- | --- |
| Commands and results | [CLI](cli.md), [diagnostics](diagnostics.md), [CI reports](ci-reports.md) |
| Architecture and custody | [Architecture](architecture.md), [authority](authority.md), [structure](canonical-structure.md), [artifacts](artifact-manifest.md) |
| Model and loading | [Model and glossary](model.md), [IDs](semantic-ids.md), [loader](loader.md), [IR](ir.md), [validation](validation.md), [versions](versioning.md) |
| Bootstrap and ownership | [New projects](bootstrap.md), [existing repositories](adopt.md), [observed](observed-mode.md), [contracted](contracted-mode.md), [managed](managed-mode.md), [bindings](bindings.md) |
| Inspect and change | [Inspect](inspect.md), [graph](graph.md), [effects](effect-graph.md), [impact](impact.md), [context](context.md), [budget](context-budget.md), [diff](semantic-diff.md), [Change API](change.md), [fixes](fixes.md), [format](format.md), [search](semantic-search.md) |
| Adapter authoring | [Protocol](target-protocol.md), [profiles](target-profile.md), [SDKs](adapter-sdk.md), [conformance](adapter-conformance.md), [manifest](adapter-manifest.md), [install/trust](adapter-install.md), [confinement](adapter-confinement.md) |
| Generation and tools | [Orchestration](orchestration.md), [lockfile](lockfile.md), [cache](cache.md), [native gates](native-gates.md), [doctor](doctor.md), [custom implementation](implementation.md), [portability](portability.md) |
| Application contracts | [Authorization](authorization.md), [transactions](transaction-concurrency.md), [extended effects](extended-effects.md), [errors](error-contracts.md), [transitions](invariant-transition.md), [expressions](expressions.md), [evaluator](reference-evaluation.md), [scenarios](scenario-ir.md), [module API](module-api.md) |
| Data and transport | [Queries](query-model.md), [HTTP](transport-http.md), [OpenAPI](openapi.md), [client SDK](client-sdk.md), [Zod](zod-generation.md), [Drizzle](drizzle-bindings.md), [storage projection](storage-projection.md), [engine profile](storage-engine-profile.md), [engine admission](storage-engine.md), [introspection](storage-introspection.md), [Laravel migrations](laravel-migrations.md), [PHP operations](php-laravel-operations.md) |
| Quality and policy | [Architecture profiles](architecture-profile.md), [coupling](coupling.md), [AI lint](ai-lint.md), [NFR](nfr.md), [waivers](waivers.md), [classification](classification.md), [security](security.md), [privacy policy](privacy.md), [privacy runtime](privacy-runtime.md) |
| Workflow and evidence | [Integrations](integrations.md), [provider](provider-contract.md), [MCP](mcp.md), [AI Workspace](integrations/ai-workspace.md), [requirements](requirements.md), [trace](trace-manifest.md), [trace assessment](trace-assessment.md), [history](run-history.md), [Framework Lift](framework-lift.md), [metrics export](metrics-export.md) |
| Pilots | [Worklog/Changelog](worklog-changelog.md), [external task links](task-external-links.md) |

The [ownership inventory](documentation-owners.json) assigns public commands, schemas and protocols to reference pages. [ADRs](adr/) explain decisions; they do not override the current parser, schema or feature status.

## Documentation maintenance

Keep each explanation in its canonical reference. Other pages summarize the decision needed for their task and link to the details. Keep walkthrough commands in one place. The CLI's generated index inventories syntax; it is not a second tutorial.

Use English for maintained documentation. State the working directory, dependencies, expected exit/stream and supported profile beside runnable examples. Feature references own exact pins; [versioning](versioning.md) owns general version rules. [Installation](releases.md) and [contributor setup](contributing.md) have separate audiences.

Issue research, implementation reports, reviews, logs and temporary receipts belong outside `docs/`, under ignored `target/` when needed locally. Git preserves historical versions. Keep lasting decisions in ADRs and current behavior in references.

Run [documentation checks](contributing.md#documentation-changes) after edits. Static validation and example execution are different checks. CI verifies metadata; it does not repair it.
