# Semantic diff and compatibility classification

Issue #18 gives Lekalo a way to compare two versions of a model **by
meaning, not by lines of YAML**, and to classify the consequences for
consumers, storage, and targets. The implementation is a pure, read-only,
two-project comparison service in `lekalo-core` (`crate::diff`) with a
thin `lekalo diff` handoff in the CLI.

- Contract: `contracts/semantic-diff.schema.v1.0.0.json`
- Wire discriminator: `lekalo/semantic-diff/v1.0.0`
- Contract identity: `dev.lekalo.semantic-diff@1.0.0`
- Profiles: `lekalo/diff-profile/v1.0.0`, policy revision `diff-policy/v1`
- Diagnostics: the `diff.*` rule family (`LEK-DIFF-001..008`) through the
  #11 registry, added as the wire-shape-preserving registry minor
  increment 1.4.0 → 1.5.0

The diff contract version is independent of the product release, the
Model/IR/graph/effect/protocol contract versions, and the diagnostic
registry by design.

## What is compared

Both inputs must already be accepted, normalized, immutable
`CompiledProject` values (issue #8 IR). The comparison never reads YAML
text, line diffs, physical paths, source bytes, Git state, clocks, or the
filesystem. Each side is projected into a **typed canonical semantic
projection**:

- kept: stable semantic IDs, definition kinds and versions, type wrappers
  (`list`, `optional`), presence (`required`) and nullability (`?`),
  references (reads, effects, emits, applies-to, covers, imports,
  identity, derived-from), visibility, portability, scenario summaries,
  endpoint method/path, binding targets;
- dropped: bounded descriptions (documentation), the rename/tombstone
  registry (provenance, consumed by the history resolver), source spans,
  physical paths and file boundaries, formatting, and source-revision
  metadata.

Member and reference arrays normalize to sorted form (duplicates
retained), so permutation of members, enum values, references, or
documents is never a semantic change. `equal = true` exactly when the
change set is empty.

## Change taxonomy

One closed record per independently observable semantic change, with a
stable `sha256` identity derived from side, subject, kind, and the
canonical before/after summaries — never an array position. The v1
taxonomy is exactly the 38 kinds the accepted Model can produce:
`symbol.*` (added, removed, renamed, replaced, tombstoned, kind-changed,
visibility-, portability-, version-, derived-from-changed),
`module.imports-changed`, `field.*` (added, removed, type-, nullability-,
presence-changed), `type.*` (shape-changed, member-added, member-removed),
`signature.*` (input-added/-removed/-type-changed/-presence-changed,
output-changed, reads-changed, effects-changed),
`invariant.identity-changed`, `policy.decision-changed`/
`policy.scope-changed`, `effect.operation-changed`/`entity-changed`/
`emits-changed`, `endpoint.method-changed`/`path-changed`/
`invokes-changed`, `scenario.summary-changed`/`covers-changed`, and
`binding.target-changed`. Effect changes are separate records from
signature changes even when one operation causes both. Families without a
declared source in the accepted Model (standalone invariants, error
unions, storage projections, artifact manifests) have no kinds; a
contract successor activates them.

Every record carries its own kind as its first reason plus direct
`history.*` / `symbol.*` reasons where they apply — never an inference
from a line count, filename, display name, graph path, or transitive
consumer.

Golden fixtures under `tests/fixtures/diff/cases` provoke every
producible kind — 37 of the 38. `symbol.kind-changed` alone is
intentionally fixture-less: it is unreachable by construction, because
subjects key on family and stable ID, family is one-to-one with the
definition kind, and the taxonomy arm exists only to keep the
exhaustive walk total.

## Renames, replacements, tombstones

Renames resolve only from declared evidence: exactly one live candidate
definition claims the base id through `renamed_from`, proven by a direct
same-identity history edge whose recorded version matches, or by a
bounded multi-hop walk (≤ 16 hops, cycle-detecting) through the declared
chain. Once a rename resolves, the base side's references follow the new
id, so mechanical reference updates never masquerade as independent type
changes. A replacement is a `replaced` tombstone whose successor is live;
a deletion is a `deleted` tombstone; an absent tombstone is a plain
removal. Ambiguous (two claimants), cyclic, dangling, conflicting
(claim without edge, or identity denied), and version-invalid history
classifies as `unknown`/`history.conflicting` — never a guessed alias.
Old IDs are never silently reusable: re-introducing a tombstoned id adds
`symbol.tombstone-reuse` and the `unknown` class.

## Compatibility classes and profiles

Eight closed classes: `unknown`, `data-loss-risk`,
`storage-migration-required`, `wire-breaking`, `source-breaking`,
`behavioral`, `target-specific`, `additive`. Every ordered class set
renders in this dominance order. The core result carries the
profile-independent union over the change facts.

Compatibility is decided **per explicit profile, never as one global
verdict**. Five built-in profiles ship in v1, each evaluating the same
facts through its closed policy (`diff-policy/v1`):

- `source-consumer` (strict): optional additions are additive; removals,
  required additions, narrowing, and identity rewrites are
  source-breaking; wire-only shape changes translate to source-breaking.
- `wire-consumer` (strict): optional additions (including optional event
  payload fields) stay additive while removals, tightening, and event
  payload shape changes are wire-breaking.
- `storage-consumer` (strict): requires storage projection evidence;
  without it every change classifies `unknown` with
  `profile.storage-evidence-absent` and the verdict is `blocked`.
- `target-consumer` (strict): binding/capability changes are
  `target-specific`; everything else requires verified target evidence
  and otherwise blocks as `unknown`.
- `advisory` (permissive): reports the baseline classes and degrades on
  unknown evidence without blocking.

Strict profiles block on unknown, stale, conflicting, unsupported, or
incomplete evidence; a strict profile never converts degraded evidence
into an optimistic verdict.

## Namespaced adapter contributions

Contributions are optional, namespaced, and non-authoritative: a
validated envelope (reverse-DNS namespace, adapter id, exact SemVer,
target id, profile reference, protocol reference, input IR digest,
evidence revision and digest, closed trust state) may append classes to
its own profile decision, but can never mutate canonical change facts,
stable IDs, equality, or the core seed set. A contribution whose input IR
digest does not match the candidate records as `stale` with
`adapter.stale`; contradictory duplicates record as `conflicting`; exact
duplicates collapse. Every contribution carries a computed integrity
digest.

## Seeds and provenance

The `affectedSeeds` set is bounded and direct: one seed per changed
subject with the change identities that touch it, origin
`direct-diff`. The module never materializes reverse or transitive
dependents (#13 owns graph traversal, #16 owns impact radius).

## Migration hints

Hints are non-executable and closed: `replacement-available` (with the
successor id), `storage-review`, and `wire-review`. Nothing is
automatically repaired, rewritten, or migrated (#9 owns migrations).

## Limits and determinism

Owner-approved v1 bounds: 100000 compared subjects, 250000 change
records, 50000 seeds, 32 MiB canonical result, 128 profile terms, 8
adapter contributions, 50000 effects/seeds per contribution, 16
rename-history hops. The subject, seed, profile-term, adapter, and
export-byte bounds reject with a registered `diff.*` diagnostic and no
partial result. The 250000-change bound caps migration-hint generation
in v1 — its `diff.change-limit` diagnostic is registered without an
emitter, and an oversized canonical result rejects through
`diff.export-limit` at the 32 MiB byte bound instead.
Canonical bytes are compact UTF-8 JSON with byte-sorted keys,
path-independent, byte-identical for the same inputs; ordered class
sets always render in dominance order; change
records sort by subject, kind, then identity.

## CLI

`lekalo diff OLD NEW`, `lekalo diff --base OLD NEW`, and
`--format json` only select already accepted inputs and render the same
canonical result; `--profiles` selects built-in profiles. The verdict
never computes an exit: successful comparisons are exit `0`, invalid
inputs are exit `1` with `diff.*` diagnostics. The command never parses
Git (`--base` takes a project directory), loads YAML itself (the accepted
#7 selection pipeline does), invokes adapters, or writes.
