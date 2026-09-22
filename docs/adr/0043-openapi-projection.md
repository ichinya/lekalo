# ADR-0043: The OpenAPI projection

Date: 2026-09-27
Status: accepted for issue #46

## Context

Issue #46 asks for deterministic, validator-clean OpenAPI documents
rendered from the #70 HTTP/JSON transport attachment:
paths/methods/operationIds, parameters and request bodies, response
schemas and status codes, typed error variants, security schemes,
pagination/filter/sort contracts, and reusable components — with
semantic ids carried in `x-lekalo-*` extension metadata, full or
fragment output, an ownership-aware merge that never overwrites
manually owned operations, a checked mode for a maintained
`openapi.yaml`, and compatibility classification through the
transport wire diff.

## Decision

### 1. OpenAPI is a transport projection, never the domain model

The document is derived data: every member renders from the validated
transport attachment joined with the compiled project IR through
`transport_http::ValidationContext` — the same validated join the
route-surface projection consumes (ADR-0042 decision 6). The Model
and the IR gain no transport/documentation construct; anything the IR
cannot express (maps, unions, constraints, defaults under Model
0.2.16) projects as an `openapi.projection-partial` warning naming the
symbol — never a silent drop, never an IR change.

### 2. Projection semantics live in core; byte emission lives in the adapter

`lekalo_core::openapi` owns the document model, the `TypeRef` →
JSON-Schema mapper, the renderer, fragments/merge/bind/check, and the
pointer-level diff view — all target-agnostic. The node-typescript
adapter claims the already-registered `generate.openapi` capability
inside the generation composite (issue #45/#70 precedent: one closed
wire `generate` operation) and adds only YAML serialization, write
planning, and target-document policy resolution — the `zod-emit`
precedent of a pure emitter over core semantics. Checked mode
(`lekalo openapi check`) is adapter-independent for the same reason
the transport CLI is.

### 3. One mapping table, two renderers

`required` is presence and `optional` is nullability — the #45
orthogonality, restated for JSON Schema: a required field lands in
the parent `required[]`; an `Optional(T)` renders the 2020-12 type
array `[T…, "null"]` at declared version 3.1 and the `nullable: true`
sibling at 3.0. Scalar bases, enums, entities/value-objects, lists,
and refs project through one closed table shared verbatim by the core
Rust renderer and the adapter's JS renderer; determinism makes the
two provably byte-identical on the same inputs.

### 4. Semantic ids travel in `x-lekalo-*`, never in prose

Every operation carries `x-lekalo-endpoint` and `x-lekalo-operation`;
every reusable schema carries `x-lekalo-symbol`; the document root
carries `x-lekalo-provenance` (model/IR/transport digests and the
generator identity). These anchors are the checked-mode binding key,
the diff subject, and the trace-chain identity. Data OpenAPI cannot
express natively (policies, rate limits, cache, api version, declared
filter/sort trees, cursor fields, capabilities) renders as
`x-lekalo-*` annotations — preserved, never invented, never dropped.

### 5. Nothing is synthesized

`servers`, `info.contact`, `info.license`, `termsOfService`,
`externalDocs`, and oauth2 flow URLs have no declared source and are
never invented; `info.{title,version}` derive from the project id and
attachment revision only. No runtime server, middleware, or handler
is generated — that stays with the #70 route surface.

### 6. Ownership-aware deterministic merge

Fragment mode merges per JSON pointer
(`/paths/<template>/<method>`, `/components/...`) through a
generated-sidecar ownership manifest (`lekalo/openapi-map/v0.4.0`).
Generated pointers replace their own bytes; pointers the manifest
does not own are left untouched and reported as
`openapi.merge-conflict`; a manual operation binding nothing is
preserved and reported informationally — never an error, never
deleted. Merge is a pure core function used identically by emission
and by verification, so merge is verified, not just performed.

### 7. Compatibility is classified once

Breaking/policy/non-breaking stays `transport_http::DiffClass`. The
OpenAPI diff is a pointer-level view of the same semantic comparison:
each transport diff path maps to the document locations it touches.
No second taxonomy exists.

### 8. Validation against the official meta-schema

The validator contract is the official OpenAPI Initiative 3.1
meta-schema (pinned file plus sha256 sidecar, the contract-golden
convention). At implementation time the plan §7 fallback was exercised:
the upstream schema's `$dynamicRef`-based Parameter/Response
discrimination is unreliable under direct Ajv 2020-12 compilation, so
the document pass runs through the pinned
`@seriousme/openapi-schema-validator@2.8.0` (Ajv 8.x family) while the
official schema stays compiled and pinned as the custody contract.
The 3.0.x meta-schema is draft-04, which the pinned Ajv 2020-12
processor does not compile; declared-3.0 renders are pinned by core
vectors and goldens instead, recorded here as the v1
validator-custody choice.

## Consequences

- The diagnostic registry gains the additive `openapi.*` family
  (`LEK-OAPI-001..008`); the registry stays `0.4.0` because the
  product version of this commit is `0.4.0`.
- The conformance `CheckId` set stays closed: generate/determinism/
  manifest checks already exercise the capability end to end (#45
  R6 precedent).
- Import mode bounds `openapi.yaml` parsing by the closed loader
  frontend (block style, no anchors/aliases/tags/flow/multi-doc);
  out-of-subset input refuses as `openapi.input-invalid`. Real-world
  documents relying on YAML anchors need `openapi.json` — flagged for
  owner review in issue #46 discussion.
- The canonical OpenAPI document home is the target-document policy
  block (`openapi: {version, mode, path}` in
  `lekalo/targets/<target-id>.yaml`, the #45 `zod:` precedent) plus
  the repo-conventional `docs/openapi.yaml` default; render evidence
  lands under `.lekalo/cache/openapi/<project>.json` beside the
  transport evidence.
