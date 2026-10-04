# Artifact authority and synchronization boundaries

Status: **Implemented**, accepted authority `0.3.2` at product `0.6.4`, source base `a56ee578`. Owner: boundary maintainers. [#2](https://github.com/ichinya/lekalo/issues/2), [ADR-0001](adr/0001-artifact-authority-boundaries.md).

The accepted index is [authority-contracts.manifest.json](../contracts/authority-contracts.manifest.json). Its current exact ref selects [authority-matrix.v0.3.2.json](../contracts/authority-matrix.v0.3.2.json), digest `sha256:7ae6454ea20f7b61202d368411ef9bff4e70af96f1f2a408c209d84fe9722f80`. The predecessor `0.2.16` is accepted only at its own exact ref; its filename never makes it current. [Privacy](privacy.md) selects its separate accepted policy and grants.

## Authority matrix

| Artifact | Canonical owner | Lifecycle | Governed home / boundary |
| --- | --- | --- | --- |
| Requirements, change intent, expected behavior | OpenSpec | canonical | `openspec/specs/**`, `openspec/changes/**` |
| Plans and task state | AI Factory | canonical | `.ai-factory/plans/**` |
| Agent lifecycle and runtime state | AI Factory | runtime-only | `.ai-factory/state/**` |
| Provider envelopes and generated summaries/rules | AI Factory | derived | Governed QA/rules homes; preserve originating evidence |
| Semantic definitions, IDs, effects, scenarios, target bindings | Lekalo | canonical | `lekalo/**` |
| Import drafts, intermediates and caches | Lekalo | derived / cached | `.lekalo/import/**`, `.lekalo/generated/**`, `.lekalo/cache/**` |
| HLV validation/gate result | HLV | canonical for its own result | `.hlv/**`; conditional confirmed greenfield project home follows machine policy |
| Source/native tests and execution observations | source/native toolchain | direct-evidence | Native source/test homes |
| Generated native output | source/native toolchain | derived until explicit adoption | Manifested native paths, never silent ownership promotion |

This is the readable summary of the machine matrix's **57** registered artifact kinds. Every kind has one scalar `canonicalOwner`, an explicit lifecycle and reader/writer/path sets. The exact registry also governs privacy records, traces, repositories, fixture evidence, adapter packages and the typed attachments. [Checked documentation owners](documentation-owners.json) govern reference accuracy; they do not grant artifact read/write authority.

Reader declarations permit authority-level referencing. They do not authorize reading secrets or exporting private bytes. Canonical/derived/cached/runtime-only/direct-evidence lifecycle is distinct from sensitivity and export disposition. No layer can convert a successful receipt into ownership over another layer's canonical input.

## Three implementation ownership modes

| Mode | Source of truth | Permitted progression |
| --- | --- | --- |
| **Observed** | Maintained native source and explicitly scoped observations | Confirm a semantic mapping; preserve uncertainty and stale evidence; propose promotion |
| **Contracted** | Canonical semantics plus maintained implementation and an exact declaration | Check/verify declared inputs, outputs and gates; implementation remains maintained |
| **Managed** | Canonical semantics plus explicitly owned generated artifacts | Apply a plan only within manifested, granted homes and exact before-state |

Modes apply to symbols/artifacts and their declarations, not a new `mode` field in every Model definition. See [adoption](adoption.md), [observed mode](observed-mode.md), [contracted mode](contracted-mode.md) and [artifact manifest](artifact-manifest.md). A scan or promotion plan does not automatically turn the entire project into managed code.

## Custody, conflict and synchronization

Admission checks the exact `{contractId, version, digest}`, exact bytes and sidecar against the accepted manifest. A locally changed document plus a recomputed sidecar cannot admit a successor. Contract additions need reviewed ownership, paths, lifecycle and compatibility rules; conflicting references stop before synchronization. [Authority checker](../scripts/check-authority.mjs) verifies those trust anchors.

Derived envelopes preserve origin/ref/digest and may carry a verified result to the owning consumer. They cannot silently rewrite requirements, workflow plans or Model from generated rules, AI responses or tool transcripts. Import drafts become canonical only through explicit adoption. Generated files become maintained only through the recorded promotion/adoption boundary.

Filesystem homes obey the machine path syntax, physical-link refusal and project-root rules; overlapping authority boundaries are not resolved by guessing. Target apply requires separate scope/plan custody, and publication requires separate privacy authorization. See [project layout](project-layout.md), [target protocol](target-protocol.md) and [security](security.md).

Consumers appear publicly as role aliases such as `planner-consumer` and `brownfield-consumer`. No private checkout identity or service URL is part of a public authority example.
