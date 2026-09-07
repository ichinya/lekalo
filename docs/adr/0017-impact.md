# ADR-0017: Impact and change-radius analysis

Date: 2026-09-06
Status: accepted for issue #16

Custody: issue #18 published product 0.1.26 (annotated tag `v0.1.26` on
`3710179`); issue #24 published product 0.1.27 (annotated tag `v0.1.27` on
`ef7680d`); issue #25 published product 0.1.28 (annotated tag `v0.1.28` on `967bf52`); issue #62 now carries the **prospective product candidate 0.1.29** in every accepted path (workspace `Cargo.toml`, both `lekalo`
packages in `Cargo.lock` including the regenerated committed golden lock
and its digests, the `--version` behavior and its pinning tests,
`README.md`, `docs/cli.md`); issue #17 published product 0.1.25 (annotated
tag `v0.1.25` on `e627fe5`); this issue published product 0.1.24
(annotated tag `v0.1.24` on `b4109e5`); issue #20 published product
0.1.23 (annotated tag `v0.1.23` on `15be55a`, remote CI green). The impact contract version
(`lekalo/impact/v1.0.0`, identity `dev.lekalo.impact@1.0.0`) is independent
of the product release, of the Model/IR/graph/effect contract versions,
and of the diagnostic registry by design; this issue also records the
registry minor increment `1.3.0` → `1.4.0` (below).

## Context

Issues #13 and #14 shipped the dependency graph and the effect graph.
They answer what references what and what reads, writes, and emits what —
but nothing answers the question every consumer asks before editing: if
this changes, what else changes with it? Issue #16 owns that surface: a
deterministic impact and change-radius analysis over one semantic symbol
or one set of changed inputs, with explicit evidence, completeness, risks,
and targeted gate selection.

The research briefs (run run_088695f63032: worker_done msg_87eb602fdb6a
and briefs msg_c838e04c172a, msg_e169d29cc118, msg_b91b165a5d18) recorded
the owner decisions this ADR adopts.

## Decision

### 1. Independent closed contract

Impact publishes its own wire contract,
[`contracts/impact.schema.v1.0.0.json`](../../contracts/impact.schema.v1.0.0.json)
(discriminator `lekalo/impact/v1.0.0`, identity `dev.lekalo.impact@1.0.0`,
algorithm `dev.lekalo.impact.algorithm@1.0.0`), independent of every other
contract family. `contracts/` gains exactly this one new schema file plus
the registry minor increment the new diagnostics require (below).
Canonical impact bytes are compact UTF-8 JSON whose object keys follow the
frozen schema order (not byte order); set-like arrays sort by unsigned
UTF-8 of their typed keys. The digest is the SHA-256 of the canonical
bytes with the digest field empty; the CLI appends exactly one LF.

### 2. Typed Git handoff, pure core

`ChangedInputSet`/`ChangedInput` are closed typed values with validated
constructors — no public `String`/path constructors. Modes:
`committed|index|worktree|mixed`. Logical paths appear only inside the
handoff, validated against the closed path grammar, and never cross back
out into any result bytes. The only Git-aware code is the CLI-edge adapter
(`crates/lekalo-cli/src/git_input.rs`): argv API with no shell, read-only
verbs (`rev-parse`, `diff --name-status --no-renames -z`,
`rev-list --parents`, `ls-files --others`), `--no-renames` pinned so a
physical rename is deterministic delete/add and never silently aliases old
symbols. The adapter resolves changed paths through the #8 source map and
hands over only the typed set. The core never parses Git, never sees a
patch line, and never infers symbols from file names.

### 3. Deterministic traversal with a depth-free public closure

The radius is an iterative, bounded, multi-source reverse walk over the
#13 graph's own reverse index and filters — no recursion, no all-pairs
expansion, no rescan. Direct = distance 1; transitive = distance ≥ 2
within the requested depth (default 3, maximum 256). The mandatory-public
closure runs as a second, depth-free walk restricted to public and
structural nodes, so a low `--depth` can never hide public impact; it
still obeys the hard caps, and cap exhaustion there degrades completeness
visibly instead of rejecting the whole result (a mandatory-public denial
would hide exactly the impact that must not be hidden). Root retention is
unconditional. Every bound rejects with `impact.traversal-limit` /
`impact.output-limit` before allocation instead of truncating by arrival
order.

### 4. Closed risk vector and evidence discipline

Nine independent dimensions — `public_contract`, `migration_data`,
`transaction`, `authorization`, `portability`, `effects`,
`bindings_artifacts`, `scenarios_tests` — never one lossy severity. Facts
derive only from accepted canonical data (IR visibility, portability, and
history; graph relations; effect edges with transaction-group and
sensitivity refs). Absent, stale, or unsupported evidence is explicit:
transaction without groups is unknown, unmapped commands are
authorization-unknown (never optimistic), tests are `unsupported` in v1
(no accepted test surface), generated artifacts are unknown (no accepted
#21 manifest). Renames consume the accepted #6/#1.0.0 rename history and
registry tombstones; field removals enter only as typed member seeds —
the CLI adapter supplies none, so field-level coverage stays explicitly
unknown there until #18 exists. Confidence is the closed #13 vocabulary
with meet semantics; no numeric guesses.

### 5. Gates: neutral selection, bounded strict denial

Gates are machine-readable selection facts (`impact.gate.*`, one per fired
dimension plus always-required semantic validation). The analyzer never
runs a gate, writes a plan, or changes task state. In the `strict` profile
a required gate resting on unknown or stale evidence is `blocked` and the
whole result denies (exit 3, `impact.gate-blocked`, stdout); the `default`
profile reports the same fact as a visible `unknown` warning and never
denies.

### 6. Diagnostics: registry minor increment

This issue allocates the `impact.*` family in the diagnostic registry:
`1.3.0` → `1.4.0` (wire-shape-preserving, additive, entries sorted,
nothing retired): `impact.selector-invalid`, `impact.changed-input-invalid`,
`impact.changed-input-incomplete` (warning), `impact.symbol-unknown`,
`impact.effect-stale` (warning), `impact.evidence-unknown` (warning),
`impact.public-impact-incomplete` (warning), `impact.traversal-limit`,
`impact.output-limit`, and `impact.gate-blocked` (the one `denied` rule
the strict bounded denial requires — no registered rule carried it).
Every echo is a bounded fixed token validated by the selector/entry
grammar first; no attacker-controlled echo. The validation-profile
instances and their schema pin the registry version and follow it to
`1.4.0`; the reason vocabulary is the closed `impact.reason.*` token
family documented in the schema description.

### 7. Limits and privacy

Owner-approved v1 caps: 256 depth, 50000 items, 250000 path-edge
references, 50000 entries and seeds, 128 roots, 128 filter terms, 8
provenance records, 32 MiB export. Output must not copy raw source, patch
text, repository identity, physical paths, secrets, runtime values, or
adapter transcripts; only validated semantic ids, typed relation/effect/
provenance ids, opaque digests, and the closed reason vocabulary cross the
core/result boundary. Impact is read-only and creates nothing.

### 8. Cache seam, no cache

`ImpactResult::fingerprint()` is the immutable canonical key of the exact
typed inputs (contract/algorithm identities, model, project, changed-input
digest, mode, revisions, depth, filters, profile, roots). #20 owns
persistence; a cache hit or miss can never change semantic bytes, digest,
ordering, or completeness.

### 9. CLI handoff

`lekalo impact SYMBOL [--depth N] [--module ID] [--target ID]
[--relation KIND] [--profile default|strict]` and
`lekalo impact --changed [--base REF] [--head REF] [--worktree]` on the
accepted 0/1/3 envelope (valid 0 stdout, invalid 1 stderr, strict denial 3
stdout; loader/IR/versioning failures pass through untouched). `--changed`
is exclusive with a positional symbol; `--head` is exclusive with
`--worktree`; a bare `--base` compares with HEAD; the default changed mode
is HEAD versus index/worktree. Human and JSON are projections of the same
result.

## Consequences

- #18 semantic comparison plugs its typed seeds into `ChangedInputSet`
  member seeds and rename facts without a shape change here.
- #20 caches on `fingerprint()`; #23/#24/#25/#28/#29 successors replace
  the `unsupported`/`unknown` evidence surfaces through typed seams.
- The declared projection stays byte-stable under the same golden
  discipline as #13/#14; the pinned golden export
  (`tests/fixtures/impact/golden/planner.impact.json`) is gate-checked
  with exact Ajv 8.17.1 on Node 18 and 24.

## References

- [docs/impact.md](../impact.md) — the impact surface and guarantees.
- [ADR-0012](0012-dependency-graph.md) — the dependency graph this walks.
- [ADR-0013](0013-effect-graph.md) — the effect projection this consumes.
- [ADR-0010](0010-diagnostics.md) — the diagnostic contract and registry.
