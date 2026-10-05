# Integration roles and handoffs

Status: **Implemented** local discovery/trace contracts; real external acceptance is **Experimental** until independently verified. Owner: workflow/evidence-consumer maintainers. [#22](https://github.com/ichinya/lekalo/issues/22), [#34](https://github.com/ichinya/lekalo/issues/34), [#37](https://github.com/ichinya/lekalo/issues/37), [#121](https://github.com/ichinya/lekalo/issues/121), [trace ADR](adr/0014-trace-manifest.md).

| Role | Owns | Lekalo handoff |
| --- | --- | --- |
| OpenSpec | Requirements and change intent | Stable requirement references/attachments; no requirements rewritten from diagnostics. |
| AI Factory / AIFHub Extension (`workflow-consumer`) | Plans, task state and normalized QA custody | Workflow-provider discovery and typed operation receipts; consumer constructs known argv and owns `.ai-factory/qa/`. |
| HLV (`validation-consumer`) | Its validation/traceability results | Confirmed trace relations/evidence; HLV is optional and does not replace native tests. |
| AI Workspace | Optional local navigation/shared-context registry | Opt-in bounded hook/event metadata; never canonical Model or schema storage. |

## Trace and discovery quickstart (F)

**Implemented.** In a disposable copy of `tests/fixtures/trace`:

```sh docs-example=trace-discovery
lekalo --json trace validate golden/planner.trace.json
lekalo --json trace export golden/planner.trace.json
lekalo --json provider describe
```

Expected: exit 0/stdout; a validated synthetic manifest, canonical trace plus digest, and `lekalo/workflow-provider/v0.6.4` discovery. The trace here is explicitly pre-authored. `trace collect` instead consumes actual adjudicated run ingest; this example does not fabricate that evidence. Confirmed relations, completeness and gaps remain visible.

[Trace reference](trace-manifest.md) owns relation confidence, occurrence and completeness rules. [Provider reference](provider-contract.md) owns the discovery contract and fixed consumer argv. A supported workflow operation does not imply a project lock, installed target or native toolchain. The adapter process [target protocol](target-protocol.md) is a different family.

An AIFHub Extension consumer negotiates exact identity/schema refs, invokes the known operation and evaluates its receipt/verdict, then writes its own QA envelope under authority/privacy rules. A HLV consumer owns its result; passing trace validation does not mean a HLV gate ran. See [HLV overview](hlv.md) and [authority](authority.md).

[AI Workspace integration](integrations/ai-workspace.md) is optional and local. Its production event sends remain refused by default until exact authority admission; an override/fake test is not real delivery. No cloud push or private-repository sharing is part of the quickstart.

[Local run history](run-history.md) is **Implemented** local-only storage/retention. [Public aggregate export #102](https://github.com/ichinya/lekalo/issues/102) is **Planned** and must preserve preview, sample-size/unknown states, policy pins and deletion invalidation. Generic privacy export does not implement that aggregate workflow.
