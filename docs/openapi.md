# OpenAPI projection (issue #46)

The OpenAPI projection renders the #70 HTTP/JSON transport attachment
into a deterministic, validator-clean OpenAPI document. It is a
**transport projection**: never the canonical domain model, never a
second semantic source. Every member derives from the validated
attachment joined with the compiled project IR; missing IR constructs
surface as `openapi.projection-partial` warnings, never silent drops
and never IR changes.

Decision record: [ADR-0043](adr/0043-openapi-projection.md).

## Where it lives

- **`lekalo_core::openapi`** (`crates/lekalo-core/src/openapi/`) owns
  every semantic decision: the document model, the `TypeRef` →
  JSON-Schema mapper, the renderer, fragment/ownership merge, the
  checked mode, and the pointer-level diff view.
- **The node-typescript adapter** claims the `generate.openapi`
  capability inside the generation composite (`adapters/node-typescript/src/generation-composite.mjs`)
  and adds only emission: canonical JSON → YAML, write planning,
  sidecars, and target-document policy resolution.

## Commands

| Command | Effect |
| --- | --- |
| `lekalo openapi render <attachment>` | Render the full document (or fragments) for one validated attachment; prints canonical bytes and digest. |
| `lekalo openapi check <document>` | Checked mode: bind an existing `openapi.yaml`/`openapi.json`, recompute fragments, report per-pointer drift plus the unbound-manual inventory. |
| `lekalo openapi diff BASE CANDIDATE` | Pointer-level view of the transport wire diff classes over two same-family attachments. |
| `lekalo openapi inspect <attachment>` | One endpoint's rendered path item. |

Shared context flags mirror `lekalo transport` (`--project`,
`--errors`, `--query-model`).

## The mapping table

`required` means presence; `optional` means nullability (the #45
orthogonality, one table shared by the Zod and OpenAPI renderers).

| IR construct | OpenAPI 3.1 | OpenAPI 3.0 |
| --- | --- | --- |
| `scalar base:string/number/boolean` | `{"type":"string"\|"number"\|"boolean"}` | same |
| `base:date/datetime/uuid/uri` | `{"type":"string","format":"date"\|"date-time"\|"uuid"\|"uri"}` | same |
| `enum` | `{"type":"string","enum":[declared order]}` | same |
| `entity`/`value-object`/input/payload | object, `required[]`, `additionalProperties:false` | same |
| `Ref(id)` | `{"$ref":"#/components/schemas/<Name>"}` | same |
| `List(T)` | `{"type":"array","items":T}` | same |
| `Optional(T)` | `{"type":[T…,"null"]}` | `{"nullable":true}` sibling |
| required field | key in parent `required[]` | same |
| absent `required` | key absent from `required[]` | same |

Reusable schemas are named `<ModulePascal><NamePascal>` (the #45
export-name rule), deduped by semantic id, and carry
`x-lekalo-symbol`.

## Semantic anchors

Every operation carries `x-lekalo-endpoint` (the endpoint semantic
id) and `x-lekalo-operation` (the invoked command/query); every
reusable schema carries `x-lekalo-symbol`; the root carries
`x-lekalo-provenance` (`modelRef`, `irRef`, `transportRef`, and the
generator identity). Checked-mode binding resolves
`x-lekalo-endpoint` first, `operationId` second.

Declaration data OpenAPI cannot express natively renders as
annotations, never dropped and never invented:
`x-lekalo-policy`, `x-lekalo-rate-limit`, `x-lekalo-cache`,
`x-lekalo-api-version`, `x-lekalo-filter`, `x-lekalo-sort`,
`x-lekalo-cursor-field`, `x-lekalo-capabilities`.

## Versions and modes

The declared version comes from the target document policy block
(the `zod:` precedent):

```yaml
openapi:
  version: "3.1"        # 3.1 in v1; 3.0 declared alternative
  mode: full            # full | fragments
  path: docs/openapi.yaml
```

- `3.1` is the emitted default. `3.0` is emitted only when declared
  and refuses constructs it cannot express (`mutual-tls` schemes) as
  `openapi.version-unsupported` — never a silent downgrade.
- `full` mode: the whole document is generator-owned.
- `fragments` mode: per-endpoint path-item fragments plus components
  slices merge through the ownership manifest
  (`lekalo/openapi-map/v0.4.0`) by JSON pointer. Generated pointers
  replace their own bytes; manually owned pointers are never
  overwritten (`openapi.merge-conflict`); a manual operation binding
  nothing is preserved informationally.

## Validation

Rendered documents validate against the official OpenAPI Initiative
3.1 meta-schema (pinned file + sha256 sidecar under
`tests/fixtures/openapi/meta/`). The document pass runs through the
plan's sanctioned fallback `@seriousme/openapi-schema-validator`
(pinned 2.8.0, Ajv 8.x family), because the upstream meta-schema's
`$dynamicRef`-based Parameter/Response discrimination is unreliable
under direct Ajv 2020-12 compilation; the official schema itself stays
compiled and pinned as the custody contract — see
`scripts/test-openapi-contracts.mjs`. Render evidence lands under
`.lekalo/cache/openapi/<project>.json` beside the transport evidence.
Import bounds `openapi.yaml` by the closed loader frontend: plain
scalars must not be float-like, so the OpenAPI version is quoted
(`openapi: "3.1.0"` — the adapter emitter's spelling), and exactly two
empty flow literals (`{}` and `[]`) are tolerated because block-style
YAML cannot spell them; any other flow content, anchors, aliases,
tags, and multi-document streams refuse as `openapi.input-invalid`.

## Diagnostics

Additive family `openapi.*` (`LEK-OAPI-001..008`):
`input-invalid`, `projection-partial` (warning), `version-unsupported`,
`schema-invalid`, `merge-conflict`, `binding-unresolved`, `drift`,
`export-limit`. Import bounds `openapi.yaml` by the closed loader
frontend (block style, no anchors/aliases/tags/flow/multi-doc);
out-of-subset input refuses as `openapi.input-invalid`, and
`openapi.json` imports natively.

## Non-goals

No runtime server/middleware generation (the #70 route surface's
job); no invented `servers`/contact/license/oauth2 URLs; no secrets
in artifacts; no second compatibility taxonomy (the transport
`DiffClass` is reused); no `contracts/` re-declaration of the OpenAPI
grammar (the upstream meta-schema is the validation contract).
