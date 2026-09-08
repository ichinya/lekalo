# Requirements: the OpenSpec traceability integration (issue #36)

One independent, closed, versioned attachment — `lekalo/requirements/v1.0.0`,
identity `dev.lekalo.requirements@1.0.0`, contract
[contracts/requirements.schema.v1.0.0.json](../contracts/requirements.schema.v1.0.0.json) —
binds Lekalo semantic symbols to canonical requirement identities owned by
requirement providers, and one derived, read-only report wire —
`lekalo/requirements-report/v1.0.0`, identity
`dev.lekalo.requirements-report@1.0.0`,
[contracts/requirements-report.schema.v1.0.0.json](../contracts/requirements-report.schema.v1.0.0.json) —
carries the resolution results: the effective requirement catalog with exact
revisions, per-reference resolution statuses, coverage gaps, explicit
conflicts, and changed-requirement impact. Owner decisions are recorded in
[ADR-0026](adr/0025-requirements-traceability.md).

## Authority and boundaries

OpenSpec owns requirements, change intent, delta specs, and expected
behavior (ADR-0001); Lekalo only reads. The integration is read-only by
construction: resolution walks the provider tree through the confined
no-follow filesystem capability, never writes, never copies requirement
text into the Model or the IR (the Model grammar and every published
schema are untouched), never treats a generated summary as a canonical
requirement, and never resolves a conflict silently — conflicting active
changes deny the gate until the OpenSpec owner resolves them. Requirement
identity crosses the boundary as opaque ids plus exact `sha256:` body
digests only. Absence of a provider tree is legal for standalone use: an
attachment without references resolves to an empty report.

The contract versions are independent of the product release, of the
Model/IR/trace/graph contract versions, and of the diagnostic registry
(whose v1.10.0 → v1.11.0 minor adds the `requirements.*` family only).

## Attachment shape

The closed top-level members are `schemaVersion`, `identity`, `projectId`,
`modelRef` (exact Model version + SHA-256 over the canonical Model payload
bytes), `providers`, and `references`.

- `providers[]` — at most 8. Each declares one `source` namespace id
  (`^[a-z][a-z0-9-]{0,31}$`), one `kind` (v1 ships exactly the on-disk
  `openspec` provider), and one project-relative logical `root`. Source ids
  are unique and roots may not nest: one tree must never resolve twice
  under two namespaces.
- `references[]` — at most 4096. Each binds one `symbol` (a semantic id of
  the bound project) through one `relation` — `derived_from` (provenance)
  or `implements` — to one namespaced `requirement` id, pinned to the exact
  `revision` (`sha256:<64 hex>` of the requirement body) the author
  resolved. References are a set: the same (symbol, relation, source,
  requirement) tuple twice is invalid.

### Requirement ids

The `openspec` provider derives stable ids from the artifacts:
`<capability>.REQ-<slug>`, where `capability` is the directory under
`specs/`, and `slug` is the deterministic kebab-case slug of the
requirement title (`### Requirement: <title>`), e.g. `planner.REQ-focus-task`.
Title slugs are stable under reordering and insertion; a title change is a
new id, and the resolver detects exactly that move (see rename below).

### Revision digests

A revision digest is the SHA-256 over the canonical requirement body: every
line with trailing whitespace dropped, leading and trailing blank lines
dropped, one trailing newline. The title line is excluded, so a pure rename
(title change, unchanged body) is detectable as an id move with an identical
digest. Canonical bytes never enter Lekalo — only the digest.

## Resolution

The provider reads `<root>/specs/<capability>/spec.md` (accepted/base) and
`<root>/changes/<change-id>/specs/<capability>/spec.md` (active change
deltas; the `archive` subtree is never active). Deltas use the OpenSpec
sections `## ADDED Requirements`, `## MODIFIED Requirements`, and
`## REMOVED Requirements`; a requirement block under any other section of a
delta fails closed. Changes apply in ascending change-directory order to
project the effective requirement set:

- ADDED — the title must not already exist (accepted or previously added);
- MODIFIED — the title must exist; at most one active change may modify it;
- REMOVED — the title must exist; at most one active change may remove it.

Every contradiction records an explicit conflict (`duplicate-title`,
`added-existing`, `duplicate-added`, `modified-missing`, `removed-missing`,
`multiple-changes`) and removes the disputed requirement from the effective
set: a conflict is a gate, never a silent overwrite.

Resolution statuses per reference:

| Status | Meaning |
| --- | --- |
| `fresh` | the requirement resolved and its body digest equals the pin |
| `stale` | resolved, but the body changed since the pin |
| `missing` | no such requirement; `renamedTo` names the id whose body digest equals the pin, when exactly that body still exists |
| `conflict` | the requirement is disputed by active changes |

The gate (`lekalo requirements validate`) passes only when every reference
is fresh and no conflict exists; any stale, missing, conflicted reference,
or any conflict in a resolved tree denies (exit 3). Malformed attachments,
unknown symbols or sources, and invalid provider trees are invalid (exit 1).
An absent provider tree that references depend on is unavailable (exit 4).
The Model pin and project id must match the supplied project exactly, or
resolution denies before any filesystem work.

## Archiving is traceability-neutral

Archiving a change applies its content into the accepted specs. The
projection of the same content yields the same ids and the same body
digests, so a fresh reference stays fresh across the archive: only the
origin (`change` → `accepted`) and the owning change id change. This is
proven by the `archive_preserves_accepted_traceability` test.

## CLI

```sh
lekalo requirements validate ATTACHMENT --project DIR
# requirements planner
#   requirements 3; references 3; fresh 3; stale 0; missing 0; conflict 0; coverage gaps 0; conflicts 0

lekalo requirements report ATTACHMENT --project DIR > report.json
lekalo --json requirements report ATTACHMENT --project DIR
# {"status":"valid","report":{...},"reportDigest":"sha256:..."} (canonical bytes embedded)

lekalo requirements query ATTACHMENT coverage-gaps --project DIR
lekalo requirements query ATTACHMENT impact --project DIR
lekalo requirements query ATTACHMENT symbol:planner.focus_task --project DIR
lekalo requirements query ATTACHMENT requirement:openspec:planner.REQ-focus-task --project DIR

lekalo requirements trace ATTACHMENT --project DIR > trace-manifest.json
```

`report` and `query` are informational and exit 0 whenever resolution
completes (the report itself documents staleness); `validate` is the gate.
`trace` projects the resolution into the neutral #22 trace contract —
requirement nodes carry the OpenSpec original ids verbatim as external
references with their exact body digests, every reference becomes an
`implements` edge (the closed #22 endpoint matrix admits exactly one
symbol→requirement kind, covering both declared relations), missing and
conflicted links become explicit gaps, and the one unanchored
`missing-gate` gap records that this projection carries no binding/test/gate
chain. The manifest is re-validated by the accepted #22 validator before
any byte is emitted; completeness is `partial` with the uncovered sinks
reported explicitly.

## Determinism, bounds, and diagnostics

Canonical attachment and report bytes are compact UTF-8 JSON with
byte-sorted object keys and canonically sorted collections (no trailing
LF); the report digest is `sha256:` over exactly those bytes. Bounds
(owner-approved v1, ADR-0026): 8 providers, 4096 references, 256
capabilities and 10000 requirements per provider, 256 active changes, 128
title characters, 1 MiB per spec document, 8 MiB per attachment, 32 MiB per
canonical export. Every bound and semantic contradiction rejects with an
explicit registered diagnostic (`LEK-REQ-001` … `LEK-REQ-010`) and no
partial result; echoes are bounded fixed tokens — no requirement text, no
paths beyond bounded logical ids, no host data.

The committed planner fixture (`tests/fixtures/requirements/planner`)
covers the fresh gate, the report and trace goldens
(`tests/fixtures/requirements/golden`, with pinned `sha256:` sidecars), and
the invalid vector matrix; `scripts/test-requirements-contracts.mjs`
re-validates everything under exact Ajv 8.17.1 on Node 18 and 24, and the
Rust suite proves resolution semantics, archive symmetry, rename and
removal impact, the conflict gate, and the no-write boundary.
