# ADR-0031: the observed mode for existing code

Status: accepted for issue #39. Custody: this issue carries the reserved
product candidate **0.2.6** (the per-issue 0.2.x tag order of the M2
release policy); the observed-index and observed-scan contract versions,
the diagnostic registry version, and every other contract family stay
independent of it.

## Context

Brownfield repositories are the second adoption path (roadmap issue #1):
an existing project must connect to Lekalo without rewriting its code and
without the semantic model claiming ownership of the implementation. The
authority matrix already reserves the adoption direction
(`lekalo.observed-model-draft` -> `lekalo.semantic-model`, evidence
`explicitAdoption`, `reviewed`, `provenance`, `genericSync: false`), the
structure contract already reserves the runtime homes
(`.lekalo/import/**`), and the artifact-ownership contract already fixes
generated-file custody. What was missing is the ownership mode itself: how
existing code is indexed, bound, verified, and gradually promoted.

## Decisions

1. **Evidence over parsing.** The core consumes typed adapter scan
   documents; it never parses source code. This keeps the Rust core
   language-neutral and mirrors the effect-graph rule that detected facts
   arrive as typed evidence envelopes, never as source detection.
2. **One derived index, one canonical store.** The observed index is a
   single derived document at `.lekalo/import/observed/index.json` with
   canonical compact bytes and a digest; every mutation recomputes the
   whole state and writes atomically through a confined, link-rejecting
   store. A present index must re-render to its exact canonical bytes.
3. **Three binding classes, wire-visible.** `explicit`/`confirmed` are
   user-owned; `inferred` is adapter-owned. Class is result data that
   gates behavior: promotion eligibility, confirmation rules, and the
   incompleteness statements all read the class. A scan never downgrades
   a user-owned fact; dropped records go stale, never silent.
4. **Stable keys, not paths.** Binding identity across scans is the
   adapter's stable key; a move re-resolves and is recorded as history. A
   path-only binding is brittle by construction and reports stale.
5. **Incompleteness is a first-class result.** The canonical impact
   degrades (state, confidence, provenance, registered warning) when
   recorded non-canonical code references the impact radius, because the
   canonical graph cannot see un-promoted code. The observed impact
   always reports the recorded graph as incomplete.
6. **Promotion is planned and confirmed.** The write targets only
   Lekalo-owned canonical model documents, is pinned by a plan digest,
   refuses inferred/unknown evidence and unsupported kinds, and records
   the authority-required adoption receipt on every promoted symbol.
   Multi-symbol plans resolve intra-plan references against the whole
   plan set, so ordering never decides eligibility.
7. **Registry 1.10.0 on this base.** The `observed.*` family
   (LEK-OBS-001..011) joins the registry as a wire-shape-preserving
   additive minor increment with full predecessor custody; the
   validation profiles re-issue against the new registry pin exactly as
   every prior minor increment did. On the integrated M2 line the family
   folds into the reserved 1.16.0 successor; the family content is the
   identity that must survive, not the base-relative number.

## Consequences

- Existing projects connect without touching source files; only
  Lekalo-owned homes are written.
- `lekalo clean` can never delete observed files: its managed root is
  disjoint and the manifest grammar cannot claim them.
- The observed card and observed impact answer the inspect/impact/context
  questions for recorded symbols with explicit completeness states;
  context-capsule wire integration for observed symbols stays with the
  capsule contract owner (issue #17 family) and is intentionally not
  forced here.
- Real adapter coverage lands with the target-protocol integration
  (issues #27/#28/#44): this issue owns the seam, the registry, and the
  semantics, and the synthetic task-domain fixture proves the full flow
  without claiming native-adapter coverage.
