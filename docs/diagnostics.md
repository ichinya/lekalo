# Diagnostics, results and exits

Status: **Implemented** at product `0.6.3`, source base `a56ee578`. Owner: result/reporting maintainers. [ADR-0010](adr/0010-diagnostics.md), [#103](https://github.com/ichinya/lekalo/issues/103), [CI reports](ci-reports.md).

The closed item schema is [diagnostic.v0.2.16](../contracts/diagnostic.schema.v0.2.16.json); the active embedded rule instance is [registry.v0.6.3](../contracts/diagnostic-registry.v0.6.3.json). Item and registry versions are separate pins. Historical registry files remain exact snapshots, not current selection by filename order.

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

## CI report projections

[CI reports](ci-reports.md) owns GitHub annotation, SARIF and JUnit projections and the `0.6.3` report schema. They project the same diagnostics with source custody and output limits. Writing a report does not turn a refused operation into success; empty reports are not evidence of native execution. Provider discovery uses its own [capability contract](provider-contract.md).

Status labels for supported behavior are different from result verdicts: **Implemented** describes an available bounded producer, **Experimental** a restricted integration path, and **Planned** an unavailable feature. See [roadmap](roadmap.md).
