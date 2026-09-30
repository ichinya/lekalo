# ADR 0047: the Laravel + Vue pilot path and the Node observed baseline

Issue #114, milestone M5. The research lineage is the #50/#53
implementation notes; this ADR records the transition decision the
first contracted pilot produced.

## Context

The first product consumer is greenfield. The experimental
Node.js/TypeScript implementation carried the acting event/API
contracts and the only complete vertical slice (the planning family),
but the target architecture for the first contracted pilot is a PHP +
Laravel modular monolith with a Vue 3 + TypeScript + Vite frontend,
explicit HTTP/JSON + OpenAPI transport, and PostgreSQL storage. The
question: how does the pilot move to the target architecture without
either a big-bang rewrite or a semantic model that silently becomes
Laravel-shaped?

## Decision

1. **Laravel + Vue is the primary greenfield pilot path.** New
   vertical slices start on it: Model → Laravel command/query/policy/
   storage bindings (contracted mode) → HTTP/OpenAPI projection →
   generated/checked TypeScript client → maintained Vue screen →
   Mago/Laratesto/native gates. The roadmap states this; the tutorial
   walks it; `scripts/test-pilot-laravel-vue.mjs` proves the whole
   path in one command over the existing harnesses.
2. **The Node implementation stays, as the observed baseline.** Its
   event envelope, integration protocol (`lekalo.target/v1@0.3.2`),
   OpenAPI documents, and behavior rows are digest-pinned under
   `tests/fixtures/pilot/observed-baseline/` and re-verified against
   live bytes by `scripts/test-observed-baseline.mjs`. It remains the
   control implementation for equivalence; it defines no new backend's
   architecture.
3. **Model neutrality is a gate, not prose.** The planner model
   surfaces are scanned against a closed forbidden vocabulary
   (frameworks, languages, runtimes, package managers, storage
   engines, analysis tools) with a committed audit artifact; the one
   sanctioned target reference is a `target-binding`'s target field.
   The model stays language-independent by construction and by CI.
4. **Behavior comparison is neutral and corpus-bound.** Both backends
   execute the same committed scenario corpus (including the #114
   authorization and transaction legs); durable run records normalize
   to `(step_id, observes, kind, outcome)` tuples that must be
   identical, with a shared negative control. The comparison artifact
   is deterministic and committed as evidence.
5. **Removal of any old-backend surface is gate-gated.** The written
   gate (`docs/m5/issue-114-equivalence-report.md` §4) requires corpus
   equality, the negative control, baseline currency, a written
   surface inventory, and no consumer coupling. Absent any leg, the
   decision is "no". Big-bang removal is out of scope by definition.
6. **The strict Laravel profile is enforced, not described.**
   `declare(strict_types=1)`, final/readonly/typed contracts, explicit
   command/query entrypoints, FormRequest/DTO/Policy/Action/Resource
   boundaries, no business logic in controllers, no hidden observer
   effects, no service locator in portable core, explicit transactions
   and errors — carried by the generated surface and checked by the
   Mago toolchain leg (checksum-pinned).

## Consequences

- The pilot path is CI-enjoyable end to end; a wire change is caught
  by the checked join on the same run (client ↔ OpenAPI ↔ routes ↔
  screen).
- The Node baseline's continued existence is a stated cost (fixture
  and gate maintenance) bought for equivalence evidence and the
  scenario/adapter machinery both targets share.
- The portable corpus is the neutral behavior vocabulary; new
  target-specific enforcement proofs (HTTP authorization enforcement,
  header idempotency) are documented as target legs in the equivalence
  report rather than smuggled into the neutral comparison.
- Declared-unsupported rows (the concurrent-focus leg) are honest
  evidence: a backend that cannot prove a leg says so in the same
  vocabulary, and a future race evaluator must be answered by both
  backends or the equivalence visibly breaks.
