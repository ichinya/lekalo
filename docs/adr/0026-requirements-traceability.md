# ADR-0026: read-only OpenSpec requirement traceability

- Status: accepted (issue #36)
- Depends on: ADR-0001 (authority boundaries), ADR-0005 (semantic IDs),
  ADR-0014 (neutral trace manifest), ADR-0010 (diagnostics)
- Custody: this issue carries the **prospective product candidate 0.2.1** in
  its candidate tree; product versioning remains independent of every
  contract version touched here (attachment `1.0.0`, report `1.0.0`,
  diagnostic registry minor `1.10.0` → `1.11.0`).

## Context

Issue #36 asks for an explicit, read-only connection between OpenSpec
requirement artifacts and Lekalo semantic symbols: `derived_from`/`implements`
references to requirement ids, resolution of active changes and accepted
specs, coverage, reverse lookup, changed-requirement impact, stale-revision
detection, missing/ambiguous diagnostics, multiple providers through
namespaced source ids — without Lekalo writing OpenSpec artifacts, without
copying requirement text into the semantic model, and while working without
an OpenSpec CLI when the artifacts are on disk.

The base already contained every accepted prerequisite: the authority
boundary (#2) names OpenSpec the canonical owner of `openspec/specs/**` and
`openspec/changes/**`, the semantic-ID contract (#6) owns symbol identity,
and the neutral trace manifest (#22) already defines requirement nodes with
verbatim external ids and the symbol→requirement `implements` relation. The
graph deliberately leaves requirements as unresolved identifiers and
records that "#36 owns any later provider resolution".

## Decision

1. **A separate attachment, not a Model or IR change.** The structured
   requirement reference (`source`, `id`, `revision`) is integration
   metadata, not semantic model content. Publishing it inside the Model
   would break the published Model 1.0.0 grammar (requirement ids there are
   uppercase provenance strings), force an IR and wire migration, and drag
   OpenSpec-shaped data through every projection. Instead, issue #36 ships
   one closed attachment (`lekalo/requirements/v1.0.0`) following the
   established #23/#24/#26/#63 attachment pattern: bound to exactly one
   project and one Model pin, closed under `additionalProperties: false`,
   strict-compiled under exact Ajv 8.17.1. The Model grammar and every
   published schema stay untouched; the graph keeps `derived_from` as
   Model-level provenance.
2. **A derived report wire, not private output shapes.** Coverage gaps,
   reference statuses, conflicts, and impact are one closed
   `lekalo/requirements-report/v1.0.0` document so AIFHub-class consumers
   validate them without Rust coupling. The report carries ids, digests,
   and bounded identifiers only — never requirement text.
3. **The `openspec` provider reads the disk, never a CLI.** Resolution
   walks `specs/**` and `changes/**` through the accepted confined
   no-follow filesystem capability, derives stable ids
   (`<capability>.REQ-<title-slug>`), and computes body digests over a
   documented canonicalization (trailing whitespace dropped, block-boundary
   blank lines dropped, one trailing newline, title excluded). No process
   is ever spawned; the tree is optional for standalone use.
4. **Effective set = accepted specs + active changes in ascending change
   order.** A deterministic projection makes archiving traceability-neutral
   by construction: applying the same content into the accepted specs
   yields the same ids and digests, so fresh references stay fresh across
   an archive. Conflicts — two active changes touching one requirement,
   adds of existing titles, modifications or removals of absent titles,
   duplicate titles — remove the disputed requirement from the effective
   set and deny the gate. A conflict is a question for the OpenSpec owner,
   never a silent last-writer win.
5. **Freshness is mandatory.** Every reference pins the exact `sha256:` body
   revision it was authored against; `validate` denies on any drift, and the
   report's impact section classifies each drift as `changed`, `removed`,
   `renamed` (same body under a new id), or `conflict`, listing the affected
   symbols.
6. **Neutrality through #22.** The integration adds no OpenSpec-specific
   wire for consumers: `lekalo requirements trace` projects the resolution
   into the accepted trace manifest (requirement nodes with verbatim
   external ids and exact digests, `implements` edges — the closed #22
   endpoint matrix admits exactly one symbol→requirement kind, covering both
   declared relations — explicit gaps for missing and conflicted links, and
   one unanchored `missing-gate` gap for the absent binding/test/gate chain).
   The projection is re-validated by the accepted #22 validator before any
   byte is emitted.
7. **Read-only is proven, not promised.** Resolution uses the loader's
   capability-based filesystem access; the suite snapshots every file
   (relative path + exact bytes) around a full resolve/report/trace run and
   asserts byte equality.

## Alternatives considered

- **Structured references inside the Model** — rejected: breaking change to
  the published Model grammar and the IR, a migration machine step, and
  OpenSpec-shaped provenance inside semantic identity (violating #22's
  "foreign formats stay out of the core IR" decision) for no capability
  gain; the attachment binds to the exact Model pin instead.
- **Resolving requirements inside the graph or IR** — rejected for the same
  boundary reason; the graph keeps recording Model-level `derived_from`
  provenance unchanged, and the provider namespace never enters the IR.
- **Digest over the whole spec file** — rejected: unrelated capability edits
  would stale every requirement; the body-scoped digest localizes drift and
  enables rename detection.
- **Silent last-writer-wins for concurrent changes** — rejected: the issue
  explicitly requires conflicts to become a question/gate; the resolver
  removes disputed requirements and denies.

## Consequences

- Lekalo gains requirement traceability with zero Model/IR/schema churn and
  zero writes to OpenSpec-owned paths.
- Consumers get two closed JSON contracts plus the existing trace contract;
  no Rust coupling, no CLI dependency.
- Staleness is loud: edits to requirement bodies deny gates until references
  are re-pinned, which is the intended workflow (impact first, re-pin
  deliberately).
- Additional provider kinds (issue tracker, HLV catalogs) require a
  reviewed successor of the attachment contract; the `kind` vocabulary is
  closed at exactly `openspec` in v1.
