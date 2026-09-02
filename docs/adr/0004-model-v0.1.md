# ADR-0004: Lekalo Model v0.1

Status: issue #3 is accepted and published as product 0.1.3 at exact commit `9c35c8f61a87e89ed15471e7f12012946916e8d5`, with immutable annotated tag `v0.1.3`; issue #6 is only the prospective product 0.1.4 candidate until fresh independent PASS, publication, remote verification and closure evidence. Product versions remain independent from Model, schema, semantic-ID, IR and other contract versions.

Date: 2026-08-31

## Context

Issue #5 must define the minimal language-neutral semantic application model
sufficient for the first pilot, without turning v0.1 into a general-purpose
programming language. ADR-0003 already fixed file homes and left contents
opaque for #5; #6 owns stable symbol ids, #7 owns parsing/imports, #10 the
lock envelope, #23 the Scenario IR and #62 the error taxonomy. The model must
be equally usable from TypeScript, PHP and Go targets.

## Decision

The proposed contract is [`contracts/model.schema.v0.1.0.json`](../../contracts/model.schema.v0.1.0.json)
(`$id https://lekalo.dev/schemas/model/0.1.0/schema.json`, `schema_version`
const `0.1.0`) with [`scripts/check-model.mjs`](../../scripts/check-model.mjs) as the reference validator,
`tests/fixtures/model/valid-planner` as the example model and twelve
invalid fixtures. The closed decisions:

1. **Fourteen closed kinds.** project, module, scalar, enum, value-object,
   entity, command, query, policy, event, effect, endpoint, scenario,
   target-binding. Kind-file placement is closed per file; a valid definition
   in the wrong file is rejected.
2. **Named types only.** Field types always reference named definitions with
   optional `list`/`optional` wrappers (depth ≤ 4). No anonymous base types;
   base scalars are named first. This is what keeps the model
   machine-checkable and identical across targets.
3. **One umbrella schema with reusable `$defs`.** Per-kind definitions are
   composed from a shared `commonFields` plus a closed kind-specific branch.
   No external `$ref` resolution is required to validate a document.
4. **Cross-references live in the semantic validator, not JSON Schema.**
   Uniqueness, module qualification, typed reference resolution, entity
   identity membership, acyclic named-type dependencies and target existence
   are checked by code with stable `model.*` reason codes. Duplicate enum
   values are likewise semantic because JSON Schema cannot require uniqueness
   by one nested object property. Import cycles, visibility enforcement and
   canonical reference normalization stay with #6/#7.
5. **Separate identity namespaces.** `project` and `module` ids live in
   their own namespaces (structure already guarantees their uniqueness);
   all other definitions share one module-qualified space
   (`<module>.<name>`). The v0.1 id grammar is provisional until #6.
6. **Provenance never mixes with identity.** `derived_from` carries
   requirement-level provenance only. Source locations and file paths are
   absent from model documents and belong to the loader (#7).
7. **Documents validated as parsed JSON.** Block-YAML parsing is owned by
   #7; fixtures use the JSON subset of YAML so #5 does not pre-empt the
   loader's parser choice.
8. **Evolution policy fixed now.** Unknown fields are rejected; additive
   optional change = minor; removal/rename/re-type/tightening/kind-file
   remapping = major; per-definition integer `version` tracks definition
   evolution separately from the schema version.
9. **Scope exclusions held.** No loops, recursion, expression language, UI
   layout or workflow orchestration; a future foreign implementation
   contract will carry complex logic.
10. **Structure and input loss fail closed.** Before any model read, the
    checker reuses ADR-0003's exported structure validator. Structure invalids
    map deterministically to `model.structure-invalid`/exit `1`; structure
    denials retain the stronger denied verdict as
    `model.structure-denied`/exit `3`, followed by the original ordered
    `structure.*` reasons. After that gate, only an absent optional kind file
    (`ENOENT`) may be skipped; other read failures are `model.scan-failed`.
    JSON Schema `minLength`/`maxLength` parity is defined over Unicode code
    points rather than JavaScript UTF-16 code units.

## Consequences

- The loader (#7) can parse block YAML into these documents and add source
  locations without a model successor, as long as the shapes stand.
- #6 can formalize stable symbol ids; if the grammar changes, that is a
  model major version, not a silent mutation.
- The Rust IR (#8) gets a closed, typed target to compile against; endpoint
  transport shapes are cross-target and carry no PHP/Node/Go specifics.
- The `planner` example doubles as the fixture proving all fourteen kinds
  and as the baseline model for later milestones.
