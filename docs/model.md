# Lekalo Model and glossary

Status: **Implemented** language-neutral Model/IR `0.2.16` at product `0.6.4`, source base `a56ee578`. Owner: semantic-contract maintainers. [#5](https://github.com/ichinya/lekalo/issues/5), [ADR-0004](adr/0004-model-v0.1.md), [ADR-0005](adr/0005-semantic-ids.md).

The normative [Model schema](../contracts/model.schema.v0.2.16.json), [semantic-ID contract](../contracts/semantic-ids.v0.2.16.json) and real loader determine acceptance. Prose cannot broaden them. Model describes semantics shared across targets; it contains no framework class/package/runtime identity. Language-specific bindings and implementation evidence attach to those semantics.

## Documents and definition kinds

Documents carry `schema_version: 0.2.16` and a closed `definitions` array. The real loader reads JSON and block YAML, applies imports and normalization, and retains source spans as metadata; see [loader](loader.md). Unknown/mixed versions, unknown keys, wrong file homes and unresolved references refuse.

| Kind | Meaning | File in a module |
| --- | --- | --- |
| `project` | Project semantic identity and optional ID registry | `lekalo/project.yaml` (outside modules) |
| `module` | Semantic module identity and explicit imports | `module.yaml` |
| `scalar` | Named scalar type and constraints | `entities.yaml` |
| `enum` | Named enumeration | `entities.yaml` |
| `value-object` | Named structured value | `entities.yaml` |
| `entity` | Identity-bearing structured state | `entities.yaml` |
| `command` | Named state-changing operation and effects | `commands.yaml` |
| `effect` | Explicit effect description | `commands.yaml` |
| `query` | Named read operation and result | `queries.yaml` |
| `policy` | Semantic decision contract | `policies.yaml` |
| `event` | Named emitted fact | `events.yaml` |
| `scenario` | Portable behavior scenario | `scenarios.yaml` |
| `endpoint` | Transport-facing binding to a semantic operation | `bindings.yaml` |
| `target-binding` | Explicit target mapping | `bindings.yaml` |

These are the fourteen Model kinds. Authorization, transactions, expressions, query/storage/transport projections, NFRs and requirements are typed attachments owned by their [specialist references](documentation-owners.json), not additional definition kinds. Vue screens remain maintained source; automatic screen generation is **Planned**.

## Identity, references and semantics

Every definition has `id`, `kind` and positive integer `version`; optional description, requirement provenance, visibility, portability and kind-specific members follow the exact schema. Project/module IDs are one segment. A symbol is `module.name` or `module.kind_token.name`; an optional kind token must match the definition kind. The qualifier is the semantic module ID, independent of its directory name. See [stable IDs](model-1.0.md) and [ID rules](semantic-ids.md).

Only symbols can declare `renamed_from`; only the project can declare `id_registry`. History records renames/tombstones and prevents invalid convergence. Old IDs are not resolution aliases: references resolve against live definitions. Requirement IDs in `derived_from` record provenance, not symbol identity.

Fields reference named scalar/enum/value-object/entity types, with bounded list/optional wrappers. Type dependencies must be acyclic. The loader assembles references; strict validation checks types, contracts and semantics; [IR](ir.md) normalizes them. Graph/effect/impact/context projections report what they know and preserve unknown/gap states. A model span identifies Model source, not native implementation coverage.

The [synthetic planner](../tests/fixtures/model-v1/valid-planner) illustrates all kinds. The [minimal quickstart](../README.md#build-and-first-valid-example) validates a smaller input. Native operation availability depends on the selected profile/capability; valid Model alone does not prove generation or execution.

## Glossary

This is the canonical terminology location. Other pages link here.

| Term | Meaning and detailed owner |
| --- | --- |
| Model | Canonical language-neutral application semantics; this page and [schema](../contracts/model.schema.v0.2.16.json). |
| IR | Typed normalized intermediate representation, selected by exact contract version; [IR](ir.md). |
| Definition / symbol | A typed Model declaration / its stable semantic ID; [IDs](semantic-ids.md). |
| Attachment | A separately typed declaration/evidence input joined to Model/IR; it is not an extra Model definition kind. |
| Revision / pin / digest / custody | Exact source state / admitted reference / byte identity / preservation and admission of that identity; [versioning](versioning.md), [lock](lockfile.md), [authority](authority.md). A digest is not anonymization or permission. |
| Module | Semantic grouping with imports; directory placement is separate; [loader](loader.md). |
| Scenario | Portable behavior description compiled to Scenario IR; [scenario IR](scenario-ir.md). |
| Effect | Explicit semantic state/event dependency; [effect graph](effect-graph.md). |
| Target | Native-language/framework output destination; [target profile](target-profile.md). |
| Adapter | Separate process or bounded evidence implementation, never semantic authority; [protocol](target-protocol.md). |
| Profile / capability | Explicit resolved configuration / support claim with version and evidence; [profiles](target-profile.md). |
| Observed / contracted / managed | Evidence-owned / maintained implementation with contract / explicitly generated artifact ownership; [adoption](adoption.md). |
| Binding | Explicit semantic-to-native mapping with revision/fingerprint and confirmation; [bindings](bindings.md). |
| Declaration | Recorded contracted input/output/gate custody; [contracted mode](contracted-mode.md). |
| Artifact manifest | Provenance and permitted lifecycle for owned artifacts; [manifest](artifact-manifest.md). |
| Evidence | Typed observation with source/custody and limits; not automatically canonical semantics. |
| Origin / confidence / current-stale-unknown | How evidence was obtained / strength of its stated mapping / freshness against its recorded inputs; [observed mode](observed-mode.md). Inference cannot mint confirmation. |
| Declared / detected effect | Canonical effect contract / target-side observation of native behavior; [extended effects](extended-effects.md). Neither implies runtime coverage. |
| Diagnostic / result status | Registered finding / command verdict governing exit and stream; [diagnostics](diagnostics.md). |
| Trace | Provenance-preserving relationship evidence between requirements, symbols and tests; [trace](trace-manifest.md). |
| Native gate | Bounded tool plan and execution policy; production execution availability is separate; [native gates](native-gates.md). |
| Canonical / derived / cached / runtime-only / direct-evidence | Artifact authority lifecycle classes; [authority](authority.md). |
| Classification / export disposition | Data sensitivity / sharing decision independent of authority lifecycle; [security](security.md). |
| Privacy authorization | Purpose-bound permission under exact admitted policy/evidence; separate from application [authorization](authorization.md) semantics. |
| Known / unknown / withheld / unsupported | Typed evidence states; absent or denied information is not known zero; [local history](run-history.md). |
| OpenSpec / AI Factory / HLV | Requirement owner / workflow owner / validation-evidence owner; [integrations](integrations.md). |
| Implemented / experimental / planned | Verified bounded behavior / restricted integration path / unavailable future behavior; [roadmap](roadmap.md). |
