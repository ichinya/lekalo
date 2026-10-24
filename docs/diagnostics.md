# Diagnostics, results and exits

Status: **Implemented** result and reporting contracts. Owner: result/reporting maintainers. [ADR-0010](adr/0010-diagnostics.md), [CI reports](ci-reports.md).

The item schema is [diagnostic.v0.2.16](../contracts/diagnostic.schema.v0.2.16.json). Producers bind a registry from the admitted `0.6.4`, `0.6.5` and `0.8.0` chain; ordinary and successor features retain their own pins. Item, registry and product versions are distinct. A newer file on disk does not select a registry; [versioning](versioning.md) defines the shared policy.

## Result status owns the exit

The CLI renders the shared DomainResult verdict. Severity does not independently choose the exit or stream.

| Status | Exit | JSON result stream | Meaning |
| --- | --- | --- | --- |
| `valid` | 0 | stdout | This operation passed its bounded checks |
| `invalid` | 1 | stderr | Invalid input or failed semantic/operation checks |
| `denied` | 3 | stdout | Policy/scope denial |
| `unsupported` | 4 | stdout | Requested versioned capability is unsupported |
| `unavailable` | 4 | stdout | Required service/process/backend is unavailable |
| `unsupported-version` | 5 | stderr | Unsupported contract/registry preflight version |

Usage parsing errors belong to the CLI parser and can exit 2; they are not invented DomainResult statuses. Some commands emit family receipts rather than a status-shaped envelope: `verify` emits an orchestration verdict, and a validated `native run` plan currently exits 0 with `kind: native-run-result`, `outcome: unsupported`, `verdict: blocked`. That receipt proves plan validation, not execution. Node maintenance gates and native test runners have their own documented protocols (normally exit 0/1). Examples validate the actual family, exit and stream.

## Stable finding identity and normalization

An item carries `schema_version`, `registry_version`, stable `id`/`code`, severity, category, `message_id`, rendered message and bounded data/locations. The schema owns exact optional members. The registry owns rule identities, allowed severity/category and stable data tokens. Do not interpret free prose as a machine rule or expose arbitrary adapter stderr as public diagnostics.

Normalization validates registry identity, deduplicates deterministically and sorts independently of discovery order. Semantic symbol IDs remain stable; source paths/spans are location metadata. Missing source/native evidence stays unknown rather than being turned into a valid coverage claim. [Validation](validation.md), [inspect](inspect.md), [impact](impact.md), [context](context.md).

The [minimal example](../README.md#build-and-first-valid-example) expects exit 0/stdout. The example gate also proves rejection of malformed Model, an unknown symbol and a conflicting init; it checks refusal status, stream and preservation rather than accepting any nonzero exit.

## Versioned fix recipes

The additive product-0.8.0 `fix.*` family uses the merged 0.8.0 registry without
changing historical diagnostic items. [Versioned fixes](fixes.md) owns recipe
IDs/versions, source fingerprints, risk classes, complete previews and explicit
`lekalo fix` application. A diagnostic suggestion remains inert advice: it never
runs a patch or grants an adapter generic write access.

## Stable explanations

[Diagnostic explanations](explain.md) (issue #96) give every registered code a
stable, offline explanation in the versioned explanation catalog: causes,
consequences, closed-class fix options linked to the admitted fix recipes,
examples, related rules and waiver policy. `lekalo explain LEK-HIDDEN-009 --json`
emits the closed explain-result document; unknown codes refuse as the distinct
`explain.code-unexplained` outcome.

## CI report projections

[CI reports](ci-reports.md) owns GitHub annotation, SARIF and JUnit projections and the `0.6.4` report schema. They project the same diagnostics with source custody and output limits. Writing a report does not turn a refused operation into success; empty reports are not evidence of native execution. Provider discovery uses its own [capability contract](provider-contract.md).

Status labels for supported behavior are different from result verdicts: **Implemented** describes an available bounded producer, **Experimental** a restricted integration path, and **Planned** an unavailable feature. See [roadmap](roadmap.md).
