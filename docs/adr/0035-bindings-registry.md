# ADR-0035: the binding registry for existing code

Status: accepted for issue #42. Custody: this issue carries the reserved
product candidate **0.2.10** (the per-issue 0.2.x tag order of the M2
release policy) and the reserved diagnostic registry version **1.20.0**;
the observed-index and observed-scan contract versions, the target
protocol versions, and every other contract family stay independent of
both.

## Context

Issue #39 built the observed mode: adapter scan documents merge into one
derived index, facts are explicit/confirmed/inferred, and promotion into
the canonical model is planned and confirmed. What was missing is the
adoption loop itself (roadmap issue #42): collecting target evidence
about existing code on demand, relating native symbols, files, endpoints,
and tests to Lekalo semantic ids, and confirming inferred mappings — all
without turning a heuristic into a fact.

The base already fixed the harder halves: adapters are separate
executables behind the versioned process protocol (ADR-0025) with
deterministic capability selection (issue #28), the observed index is the
derived registry of existing code with stable keys and fingerprints
(ADR-0031), and the loader, graph, and impact engines consume the index
through an accepted seam. Rejected alternatives:

- **A second registry beside the observed index.** The index already
  binds semantic ids to native evidence; a parallel store would duplicate
  every fact and fork the freshness semantics.
- **Core-side source parsing.** The core never parses source code
  (ADR-0031); a Rust TypeScript extractor would also not generalize to
  PHP or Go.
- **Silent best-match binding in the scanner or the core.** The issue's
  first requirement is that several candidates are never chosen by
  `LIMIT 1`/first-match, so any tie-breaking heuristic is a refusal, not
  a feature.

## Decisions

1. **The observed index is the binding registry.** `lekalo scan` runs the
   read-only `scan` operation of the target protocol through discovery,
   strict selection, and the confined sandbox, and merges the produced
   inventory through the accepted #39 seam. The registry rows carry the
   issue's closed relations by section: symbol bindings `implements`,
   endpoint bindings `exposes`, native test bindings `verifies`.
2. **Additive wire, frozen predecessors.** The observed-scan and
   observed-index contracts publish additive 1.1.0 successors: the
   declared target and adapter profile (set once per registry), per-symbol
   candidate sets, and native test bindings. The frozen 1.0.0 documents
   keep their exact meanings and their fixture-era documents still decode.
3. **Ambiguity is data, never a pick.** An adapter that finds two
   plausible native identities for one semantic id reports both as
   candidates and no mapping at all; the recorded binding stays
   evidence-free (`unknown`), `bindings propose` lists every candidate,
   and confirmation refuses until the user names exactly one member of
   the set. Nothing in the pipeline ever takes the first match.
4. **Confirmation is the only promotion of an inferred mapping to a
   user-owned fact, and provenance survives it.** `bindings confirm`
   (single, by deterministic proposal id) and `bindings confirm --batch`
   (preview plan, then apply of exactly that plan id) keep the recorded
   origin, adapter, and revision; user-owned facts never generate
   proposals and never gather candidates. A stale or drifted plan id is a
   registered refusal, never a partial apply.
5. **Freshness is byte truth.** Fingerprints on bindings and test
   bindings are computed by the core from the real tree, not from adapter
   claims. `bindings audit` re-fingerprints everything after source
   changes: a changed signature or path is `stale` (gate failure with the
   registered `observed.stale-binding` diagnostics) or correctly
   re-resolved through the adapter's stable keys, never silent. Sensitive
   paths are excluded twice over: the adapter declares minimal read
   scopes and a closed skip list, and the registry can only reference
   paths the confined view actually contained.
6. **Diagnostics are additive.** Registry 1.20.0 adds exactly three
   `bindings.*` rules (`proposal-unknown`, `ambiguous`,
   `plan-mismatch`) over the frozen 1.16.0 predecessor; every accepted
   rule survives verbatim.

## Consequences

The same mechanism serves future PHP/Go adapters unchanged: an adapter
declares its target, proves the `scan.symbols` capability over the
protocol, and emits the closed per-entry detail token; everything else —
grouping, ambiguity, proposals, confirmation, freshness — is
target-neutral core behavior. Inferred mappings stay non-canonical until
confirmed, and confirmed facts reach the canonical model only through the
#39 promotion workflow.
