# ADR-0034: the contracted mode for AI-written implementation

Status: accepted for issue #40. Custody: this issue carries the reserved
product candidate **0.2.9** (the per-issue 0.2.x tag order of the M2
release policy); the conformed-declaration contract version and the
diagnostic registry version stay independent of it.

## Context

Issue #39 accepted the observed mode: source code is primary, and
Lekalo's facts about existing code become canonical only through the
planned, confirmed promotion workflow. The inverse adoption path was
still missing: a project whose semantic model **is** the contract, with
the implementation written by a human or an AI agent in the native
language. The authority matrix already reserves the direction, the
artifact-ownership contract already fixes generated-file custody, and
the scenario IR already binds behavior to native tests. What was
missing is the ownership mode itself: how maintained implementation is
bound, verified, and governed without ever being rewritten.

## Decisions

1. **The model is primary; the registry is evidence.** The conformed
   registry records adapter declarations — maintained source locations,
   typed signature claims, declared effects, support-artifact
   ownership, native tests. The gate recomputes everything comparable
   from the typed IR on every run, so a stale declaration is detected,
   never trusted.
2. **The registry home is the accepted import home.** The registry is
   derived, Lekalo-owned state; it lives at
   `.lekalo/import/contracted/registry.json` under the accepted
   `lekalo.observed-model-draft` home, mirroring the observed index.
   Canonical writes stay in `lekalo/**`; support artifacts stay in
   `.lekalo/generated/**` and refuse any other path.
3. **One registry, canonical bytes.** Like every persisted registry in
   the product, the file re-renders to its exact canonical compact
   bytes or refuses. Merges are deterministic: symbols upsert by id,
   artifacts by path, explicit attachments survive every merge.
4. **Drift is classified, registered, and blocking.** Fingerprint
   staleness, signature mismatch, effect mismatch, unimplemented
   module operations, missing native coverage, and stale support
   artifacts are per-finding registered `contracted.*` diagnostics
   (LEK-CNT-001..008, the reserved 1.19.0 registry successor over the
   frozen 1.16.0; 1.17.0/1.18.0 stay reserved for parallel issues).
   Missing evidence is `unknown`/reported, never silent success.
5. **No code is parsed, executed, or rewritten.** The declaration is
   adapter-owned evidence with its own published wire schema
   (`lekalo/contracted-declaration/v1.0.0`); accuracy is the adapter's
   custody. The generator boundary is structural: only the derived
   registry and `.lekalo/generated/**` are writable, so handler
   bodies, SQL, application services, and maintained tests cannot be
   overwritten by construction.
6. **Policy/transaction semantics stay model-side.** Lekalo never
   claims runtime enforcement; the authorization (#25), transaction
   (#24), and adapter conformance (#31) owners keep their gates. The
   contracted gate binds implementations to the contract and detects
   drift; it does not re-implement those checks.

## Consequences

- An AI agent can implement a handler in plain TypeScript, PHP, or Go:
  the model declares the contract, the implementation stays untouched,
  and conformance is a verifiable gate over evidence.
- A module moves from observed to contracted by promotion (#39) plus a
  declaration merge — additive, per symbol, no rewrite.
- The planner reference module is the first contracted slice
  (`tests/fixtures/contracted/planner-slice/`), exercised end to end by
  the CLI regression suite and the pinned-Ajv contract gate.
- Rendering support artifacts (OpenAPI, DTOs, skeletons) remains with
  the generation pipeline (#91); this issue owns the permission
  boundary, the ownership manifest, and staleness detection.
