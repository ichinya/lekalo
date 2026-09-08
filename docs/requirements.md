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
[ADR-0026](adr/0026-requirements-traceability.md).

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

`projectId` uses the accepted Model project grammar: one lowercase segment
matching `^[a-z][a-z0-9_]{0,62}$`, excluding `lekalo` and `dev`. One- and
two-character project IDs are valid; the attachment still pins the exact
canonical Model digest.

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
new id. Distinct case-sensitive titles that map to the same id produce an
`id-collision` conflict, including when an earlier operation removed one
title. A capability is at most 63 characters, a slug at most 64, and their
combined requirement id at most 128. No identifier is truncated to fit.

### Revision digests

A revision digest is the SHA-256 over the canonical requirement body: every
line with trailing ECMAScript whitespace dropped, leading and trailing blank lines
dropped, one trailing newline. The title line is excluded, so a pure rename
(title change, unchanged body) is detectable as an id move with an identical
digest. Canonical bytes never enter Lekalo — only the digest.

The whitespace set is the pinned native parser's `\s`/String trim set at
every lexical stage, including whole-block tails: TAB, NBSP and U+FEFF are
whitespace; U+0085 NEL is content. This corrects the unpublished candidate's
Rust-whitespace digests for those boundary characters. Existing ordinary
body digests and all independent contract versions remain unchanged.

## Resolution

The provider reads `<root>/specs/<capability>/spec.md` (accepted/base) and
`<root>/changes/<change-id>/specs/<capability>/spec.md` (active change
deltas; the `archive` subtree is never active). Deltas use the OpenSpec
sections `## ADDED Requirements`, `## MODIFIED Requirements`,
`## REMOVED Requirements`, and `## RENAMED Requirements`. Preambles such as
`## Purpose` are allowed, but a delta requirement without a recognized
operation fails closed. Each operation may have **at most one unfenced
section per delta file**, including empty sections and case variants.
Repeated ADDED, MODIFIED, REMOVED or RENAMED sections reject the provider
with `requirements.provider-invalid` (`duplicate-operation-section`):
validate/report/trace exit 1 and emit no successful report or trace. Native
OpenSpec selects the last body for an exactly repeated title, and the first
matching title spelling for case variants; Lekalo deliberately rejects both
forms instead of aggregating operations that archive may discard. Distinct
operation sections remain supported in any order. This restriction is per
file; headings inside fences do not count as sections.

Backtick and tilde fences mask structural headings. Following the inspected
native parser, openers accept any leading ECMAScript whitespace (including
four spaces or tabs) and any info suffix (including backticks). Closers use
the same marker, at least the opener length, and only ECMAScript whitespace
afterward; their indentation is unrestricted. All example content and later
requirement prose remain in the revision.
Every fence must close before the end of its accepted spec or active delta,
including fences in preambles and outside requirement sections. Unterminated
fences reject the whole provider with `requirements.provider-invalid`
(`unsupported-native-grammar`): validate/report/trace exit 1 and emit no
successful catalog, gate result, or confirmed trace. Native rebuilding can
append later requirements inside an unterminated fence; accepting those inputs
would certify revisions and links that do not survive rebuilding.
Accepted specs expose requirements only inside the first unfenced
`## Requirements` section (case-insensitive), ending at the next unfenced
H2. Requirement headings before it, in appendices, in later Requirements
sections, or in a document without that section do not enter the accepted
catalog. Fenced headings inside a requirement remain body bytes.
Changes apply in ascending change-directory order. Within each change the
native order is RENAMED, REMOVED, MODIFIED, ADDED:

- ADDED — the title must not already exist (accepted or previously added);
- MODIFIED — the title must exist; at most one active change may modify it;
- REMOVED — the title must exist; at most one active change may remove it.
- RENAMED — explicit FROM/TO requirement headers move an existing title to
  a new title; a MODIFIED block may replace the body under the new title.
  Both names participate in conflict detection.

REMOVED accepts both requirement blocks and native bullets such as
``- `### Requirement: Focus task` ``. RENAMED uses paired lines:

```markdown
## RENAMED Requirements
- FROM: `### Requirement: Focus task`
- TO: `### Requirement: Focus selection`
```

Heading labels are case-insensitive, but directive labels are separate:
`FROM:`, `TO:` and the reference's `Requirement:` are case-sensitive.
Directive references support either no backticks or exactly one balanced
backtick pair immediately around `### Requirement: <title>`. Whitespace
after `###` and `Requirement:` follows native ECMAScript rules. Lowercase
reference labels, multiple/unbalanced backtick wrappers, backticks within
reference names, or whitespace between an opening backtick and `###` are
unsupported. Malformed reference-like lines in REMOVED and nonempty
unrecognized lines in RENAMED reject the provider; they never silently
rename or remove a requirement that native parsing retains. Ordinary
REMOVED-block rationale remains body content.

The bounded reader also rejects accepted documents with a leading BOM,
delta documents with multiple leading BOMs, empty delta H2 headings, and
U+2028/U+2029 in unfenced structural/directive lines. Native extraction
strips exactly one leading BOM, but native accepted-file structure preflight
does not; excluding accepted BOM documents avoids certifying that ambiguous
surface. A single delta BOM is supported. U+2028/U+2029 remain permitted in
ordinary body text and native fences. Empty requirement names fail title
validation. These unsupported forms return `requirements.provider-invalid`
(usually `unsupported-native-grammar`; malformed titles/layouts use
`tree-shape`), with exit 1 and no successful catalog or trace. The reader
does not replace OpenSpec's separate archive structure/scenario checks.

The shared filesystem boundary checks the root and every directory component
before Windows enumeration, entry lookup or file reads. Empty directories
and missing children cannot hide an existing junction/reparse-point ancestor.
An absent ordinary provider stays optional, an existing ordinary empty tree
is valid, and referenced absence stays unavailable. Unix continues to use
component-relative no-follow directory handles.

Every contradiction records an explicit conflict (`duplicate-title`,
`added-existing`, `duplicate-added`, `modified-missing`, `removed-missing`,
`multiple-changes`, `id-collision`, `contradictory-operations`,
`rename-conflict`, `renamed-missing`, `renamed-existing`) and removes the
disputed requirement from the effective set. Operation history survives
removals, so remove/add contradictions and repeated edits in one change
cannot silently pass. Conflict rows contain `source`, `capability`,
`subjectId` (the bare SHA-256 of the public requirement id), and a fixed
`detail`; they never export the free-form title. Consumers can match a
known reference by hashing its `requirement` id under the same source.

Resolution statuses per reference:

| Status | Meaning |
| --- | --- |
| `fresh` | the requirement resolved and its body digest equals the pin |
| `stale` | resolved, but the body changed since the pin |
| `missing` | no such requirement; `renamedTo` is populated only by explicit active rename evidence; `renameCandidates` lists all equal-body hints when no explicit evidence exists |
| `conflict` | the requirement is disputed by active changes |

The gate (`lekalo requirements validate`) passes only when every reference
is fresh and no conflict exists; any stale, missing, conflicted reference,
or any conflict in a resolved tree denies (exit 3). Malformed attachments,
unknown symbols or sources, and invalid provider trees are invalid (exit 1).
An absent provider tree that references depend on is unavailable (exit 4).
The Model pin and project id must match the supplied project exactly, or
resolution denies before any filesystem work.

Impact labels `rename-candidate` and `ambiguous-rename` distinguish one
equal-body hint from several. Equal text alone cannot prove identity:
`renamedTo` remains null for either case, and the missing-reference gate
denies. An explicit rename plus modification still reports the rename.

## Archiving is traceability-neutral

Archiving a change applies its content into the accepted specs. The
projection of the same content yields the same ids and the same body
digests, so a fresh reference stays fresh across the archive: only the
origin (`change` → `accepted`) and the owning change id change. This is
proven for additions, removals, renames, and rename plus modification by
the integration tests. A reference updated to the new id and body stays
fresh after archiving. Old ids remain missing; once an active rename is
archived, the reader does not invent rename history from the base text.

## CLI

```sh
lekalo requirements validate ATTACHMENT --project DIR
# requirements planner
#   requirements 3; references 3; fresh 3; stale 0; missing 0; conflict 0; coverage gaps 0; conflicts 0

lekalo requirements report ATTACHMENT --project DIR > report.json
lekalo --json requirements report ATTACHMENT --project DIR
# {"status":"valid","report":{...},"reportDigest":"<64 lowercase hex characters>"} (canonical bytes embedded)

lekalo requirements query ATTACHMENT coverage-gaps --project DIR
lekalo requirements query ATTACHMENT impact --project DIR
lekalo requirements query ATTACHMENT symbol:planner.focus_task --project DIR
lekalo requirements query ATTACHMENT requirement:openspec:planner.REQ-focus-task --project DIR

lekalo requirements trace ATTACHMENT --project DIR > trace-manifest.json
```

`reportDigest` is a bare lowercase SHA-256 hex digest over the embedded
canonical report bytes; requirement revision pins retain their `sha256:` prefix.

`report` and `query` are informational and exit 0 whenever resolution
completes (the report itself documents staleness); `validate` is the gate.
`trace` projects the resolution into the neutral #22 trace contract —
requirement nodes carry the OpenSpec original ids verbatim as external
references with their exact body digests. Their neutral `requirementId`
is `<source>:<requirement-id>`, preserving namespace identity across
independent provider roots. Every resolved reference becomes an
`implements` edge (the closed #22 endpoint matrix admits exactly one
symbol→requirement kind, covering both declared relations), missing and
conflicted links become explicit gaps. Provider conflicts also produce
unanchored `conflict` gaps, including conflicts no symbol references;
their expected identity is `conflict:<source>:<subjectId>`. The one unanchored
`missing-gate` gap records that this projection carries no binding/test/gate
chain. The manifest is re-validated by the accepted #22 validator before
any byte is emitted; completeness is `partial` with the uncovered sinks
reported explicitly.

## Determinism, bounds, and diagnostics

Canonical attachment and report bytes are compact UTF-8 JSON with
byte-sorted object keys and canonically sorted collections (no trailing
LF); the report digest is `sha256:` over exactly those bytes. Bounds
(owner-approved v1, ADR-0026): 8 providers, 4096 references, 256
capabilities across accepted and active specs per provider, 10000 distinct
requirement ids per provider (including removed/disputed ids), 10000
aggregate catalog/coverage/conflict rows across providers, 256 active
changes per provider, 128 title characters, 512 ASCII characters per
logical provider root, 1 MiB per spec document, 8 MiB per attachment, and
32 MiB per canonical export. One delta contains at most 10000 operations;
references and impact rows each cap at 4096. Every bound and semantic contradiction rejects with an
explicit registered diagnostic (`LEK-REQ-001` … `LEK-REQ-010`) and no
partial result before a successful resolution, including `validate`.
Unknown keys and rejected roots use fixed diagnostic classifications;
other diagnostic subjects are SHA-256 tokens. Duplicate decoded JSON
members reject at the raw boundary, including nested and escaped keys.

The requirements report uses recursively byte-sorted object keys. The
published neutral trace contract retains its own fixed field order. The
Node gate checks those formats independently and exercises duplicate-key,
unsorted-key, trace-order, and schema-limit negative controls.

Symbol nodes retain the complete Model ID in `semanticId`. Their local
`nodeId` is `symbol:<semanticId>` when that fits the accepted trace bound;
otherwise it is `symbol-sha256:<64 lowercase hex>` over the full semantic
ID bytes. Relations and gap anchors use the same identity. These disjoint
namespaces preserve existing short-ID exports and support 191-character
semantic IDs without truncation or changes to the published trace contract.

This correction revises the still-unpublished report candidate: consumers
of the earlier candidate replace conflict `title` with `subjectId`, read
the required `renameCandidates` array, and handle the two new impact
labels. No published Model, IR, or neutral trace contract changes. The
diagnostic allocation remains 1.11.0. The prepared integration retains all
222 entries from the frozen #27 registry 1.10.0 and adds the ten
`requirements.*` entries, for 232 entries. Final acceptance depends on
confirming that the actually accepted #27 source matches the frozen base;
a changed base requires reconciliation and fresh qualification.

The committed planner fixture (`tests/fixtures/requirements/planner`)
covers the fresh gate, the report and trace goldens
(`tests/fixtures/requirements/golden`, with pinned `sha256:` sidecars), and
the invalid vector matrix; `scripts/test-requirements-contracts.mjs`
re-validates everything under exact Ajv 8.17.1 on Node 18 and 24, and the
Rust suite proves resolution semantics, archive symmetry, rename and
removal impact, the conflict gate, and the no-write boundary.
