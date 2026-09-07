# ADR-0019: Semantic diff and compatibility classification

Date: 2026-09-05
Status: accepted for issue #18

Custody: issue #24 published product 0.1.27 (annotated tag `v0.1.27` on
`ef7680d`); issue #25 published product 0.1.28 (annotated tag `v0.1.28` on `967bf52`); issue #62 published product 0.1.29 (annotated tag `v0.1.29` on `de6f8a7`); issue #26 now carries the **prospective product candidate 0.1.30** in
every accepted path (workspace `Cargo.toml`, both `lekalo` packages in
`Cargo.lock`, the regenerated committed golden lock and its digest, the
`--version` behavior and its pinning tests, `README.md`, `docs/cli.md`);
this issue published product 0.1.26 (annotated tag `v0.1.26` on
`3710179`); issue #17 published product 0.1.25 (annotated tag `v0.1.25` on
`e627fe5`).
The semantic-diff contract version (`lekalo/semantic-diff/v1.0.0`, identity
`dev.lekalo.semantic-diff@1.0.0`) is independent of the product release, of
the Model/IR/graph/effect/protocol contract versions, and of the
diagnostic registry by design.

## Context

Issues #8–#14 shipped the typed IR, the version registry, semantic
validation, the dependency graph, and the effect graph. Everything
downstream of a model still lacked one primitive: answering **what
changed, by meaning, and what does it break**. Issue #18 owns that
surface: a pure semantic comparison of two accepted `CompiledProject`
values plus compatibility classification for consumers, storage, and
targets.

The research briefs (run run_088695f63032: briefs msg_de1ba1aeecd1,
msg_aba95a82d241, msg_28c39772432d, the owner matrix msg_d8e0409d6ad1,
and worker_done msg_caaaf5975590) recorded the owner decisions this ADR
adopts.

## Decision

### 1. Independent closed contract over two typed IR values

The diff publishes its own wire contract,
[`contracts/semantic-diff.schema.v1.0.0.json`](../../contracts/semantic-diff.schema.v1.0.0.json)
(discriminator `lekalo/semantic-diff/v1.0.0`, identity
`dev.lekalo.semantic-diff@1.0.0`), independent of every other contract
family. It consumes two immutable `CompiledProject` values — never YAML
text, line diffs, physical paths, source bytes, Git state, clocks, or
the filesystem. SourceMap spans stay outside the core result; they may
be linked later as explicitly requested explain references without
affecting semantic equality.

### 2. Canonical semantic projection; descriptions are not semantic

Each side projects into a typed canonical projection keyed by stable
semantic IDs and typed member keys. The narrow owner decisions:

- descriptions (definitions, fields, enum members) are documentation and
  non-semantic: a description-only change is an empty semantic diff;
- the rename/tombstone registry is provenance consumed by the history
  resolver, not compared model content;
- member and reference arrays normalize to sorted form (duplicates
  retained): the accepted Model declares no positional wire contract, so
  permutation of members or references is never observable;
- formatting, whitespace, comments, document order, CRLF/LF, and
  physical module-directory moves with unchanged IDs are all non-semantic
  (live acceptance criterion: `equal-formatting`).

### 3. Exact taxonomy; renames only from declared evidence

The v1 taxonomy is the closed 38-kind set the accepted Model can produce
(see [docs/semantic-diff.md](../semantic-diff.md)). Effect changes are
separate records from signature changes. `change_id` derives from side,
subject, kind, and canonical before/after summaries — never array
position. Renames require a declared `renamed_from` claim plus a
matching same-identity registry edge (direct or a bounded ≤16-hop,
cycle-detecting multi-hop walk); after resolution the base side's
references follow the new id so mechanical updates never fake type
changes. Replacements and deletions resolve through their tombstones;
ambiguous, cyclic, dangling, conflicting, or version-invalid history
classifies `unknown`/`history.conflicting` — never a guessed alias.
Tombstoned IDs are never reusable (`symbol.tombstone-reuse`, `unknown`).

### 4. Per-profile compatibility, five closed built-ins

Eight closed classes render in dominance order (`unknown` first,
`additive` last). Compatibility is per explicit profile: v1 ships
`source-consumer`, `wire-consumer`, `storage-consumer`,
`target-consumer` (strict) and `advisory` (permissive), all evaluating
policy revision `diff-policy/v1`. Source and wire dimensions translate
each other's breaking classes (a wire-only shape change is
source-breaking for the source profile and vice versa); optional
additions stay additive everywhere, including optional event payload
fields for the wire profile. Storage and target profiles require
verified evidence and otherwise block as `unknown`
(`profile.storage-evidence-absent` / `profile.target-evidence-absent`).
The diff verdict never computes an exit; gate policy belongs to later
owners (#16/#20).

### 5. Adapters append, never override

Namespaced adapter contributions enter only as validated typed inputs
(reverse-DNS namespace, adapter identity and SemVer, target, profile and
protocol references, input IR digest, evidence revision and digest,
closed trust). Verified contributions may append classes to their
profile decision; they can never mutate change facts, stable IDs,
equality, or the core seed set. Digest mismatch records as `stale`
(`adapter.stale`), contradictory duplicates as `conflicting`, exact
duplicates collapse. Process protocol, capability negotiation, and
confinement stay with #27/#28/#29.

### 6. Diagnostics through the #11 seam: registry minor 1.4.0 → 1.5.0

Unlike #14, this contract adds its own rule family — the brief requires
typed `diff.*` identities, and reusing `graph.*` for diff-family
failures would blur contract families. The registry therefore takes its
next wire-shape-preserving minor increment to
[`diagnostic-registry.v1.5.0.json`](../../contracts/diagnostic-registry.v1.5.0.json)
with `diff.input-invalid`, `diff.profile-invalid`,
`diff.adapter-invalid`, `diff.subject-limit`, `diff.change-limit`,
`diff.seed-limit`, `diff.export-limit`, and `diff.history-invalid`
(LEK-DIFF-001..008, category `compatibility`), assembled through the
shared registry-backed constructor with bounded fixed tokens only — no
attacker-controlled echo. The subject, seed, profile-term, adapter,
and export-byte bounds are rejections with no partial result, before
allocation where possible; the 250000-change bound caps migration-hint
generation in v1 (its `diff.change-limit` diagnostic is registered
without an emitter, and oversized canonical results reject through
`diff.export-limit`).

### 7. Limits and determinism (v1, owner-approved)

100000 subjects, 250000 changes, 50000 seeds, 32 MiB result, 128 profile
terms, 8 contributions, 50000 effects/seeds per contribution, 16 history
hops. Canonical bytes are compact UTF-8 JSON with byte-sorted keys,
path-independent, byte-identical for the same inputs; class sets render
in dominance order; changes sort by subject, kind, identity; seeds sort
by wire key; profiles and outcomes sort canonically.

### 8. CLI handoff; Git stays with #16

`lekalo diff OLD NEW` / `lekalo diff --base OLD NEW` select two accepted
project directories through the established selection policy, load and
compile them through #7/#8, and render the core result. `--base` takes a
project directory — Git refs, changed-input detection, and impact
traversal belong to #16 and are deliberately absent. `--profiles` terms
outside the closed vocabulary reject through `diff.profile-invalid`. The
envelope wraps the payload as `{"status":"valid","diff":{...}}` on the
accepted 0/1 exit contract.

## Consequences

- #16 impact and #17 context capsules can consume the stable change,
  seed, and profile-decision facts instead of re-deriving them.
- #20 gates can pair this result with profile policies without shape
  changes; the verdict stays data, never an exit code.
- Error unions (#62), storage projections, scenario IR (#23), artifact
  ownership (#21), and transaction guarantees (#24) each extend the
  taxonomy later through a Diff contract successor — this module has no
  wildcard escape hatch.
- The pinned goldens under `tests/fixtures/diff/cases` are gate-checked
  with exact Ajv 8.17.1 on Node 18 and 24 by
  `scripts/test-semantic-diff-contracts.mjs`, and byte-compared by the
  Rust fixture test. The eight fixture cases provoke 37 of the 38
  producible kinds; `symbol.kind-changed` has no fixture because it is
  unreachable by construction (subjects key on family, and family is
  one-to-one with definition kind).

## References

- [docs/semantic-diff.md](../semantic-diff.md) — the diff surface,
  taxonomy, profiles, and guarantees.
- [ADR-0013](0013-effect-graph.md) — the effect graph whose read-only
  discipline this comparison follows.
- [ADR-0007](0007-ir.md) — the typed IR the comparison consumes.
- [ADR-0005](0005-semantic-ids.md) — the stable IDs and history the
  rename resolver consumes.
- [ADR-0010](0010-diagnostics.md) — the diagnostic contract and registry.
