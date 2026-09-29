# ADR 0046: no screen/view IR kind — the planning-screen pilot

Issue #53. The research plan is `docs/m5/issue-53-research.md`; this
ADR records the decision the pilot produced.

## Context

M5's daily-planning slice (#50) grew the governed surface to thirteen
endpoints over six queries and seven commands, and the #72 client-SDK
family renders typed TypeScript/Go clients from the same validated
join that publishes the #46 OpenAPI document. Issue #53 asked whether
that surface suffices to build a real client screen (the planning
board: today/backlog/focus/completed buckets, task cards, detail
drawer, capability-gated actions, optimistic updates, the five screen
states, actor-local dates, accessibility), or whether Lekalo needs a
future `screen/view` definition kind in core IR.

## Decision

1. **No `screen/view` IR kind is added.** `Query + Command + Endpoint +
   Scenario` suffices for the pilot. The screen's complete fact set is
   a *derivation* of existing contracts, checked — not a new semantic
   source of truth.
2. **The screen is a checked projection, not declared semantics.** The
   machine-readable UI projection (task-card/detail-drawer DTOs,
   action availability, screen states, timezone rules, accessibility
   expectations) lives as a closed fixture artifact
   (`lekalo/ui-projection/v0.1.0`) under the planner routes fixture and
   is validated by `scripts/test-php-laravel-ui.mjs` against the
   compiled IR, the transport attachment, the committed client-SDK
   evidence, the OpenAPI projection, and the requirements attachment.
   Every DTO field, action gate, rollback row, and state trigger must
   resolve to a declared API or semantic symbol; an unresolvable
   reference is a gate failure, never a silent render.
3. **Action availability derives from policy/capability contracts.**
   Each UI action names its Model policy (`applies_to` must cover the
   action's command), its declared idempotency key, its availability
   predicates over declared DTO fields, and its outcome table (exact
   #62 error identities and statuses from the client union).
   Availability is machine-readable and derived; it is never hardcoded
   in the screen or in core.
4. **Browser E2E rides the existing Scenario IR.** The screen's
   scenarios are Model `scenario` symbols (`planner.screen_*`), the
   transport endpoints declare them, and the committed scenario
   documents bind them to a `native` backend runner
   (`web.runners/browser-e2e`) with the maintained Vue screen fixture
   (`tests/fixtures/php-laravel/routes/ui/`) as the pilot surface. The
   documents are canonical, schema-validated, and pin the exact IR and
   Model digests of the same join the client renders from.
5. **Optimistic updates carry machine-readable rollback contracts.**
   Each optimistic action declares the projection it patches, the
   declared conflict errors that roll it back, and the restore rule.
   The declared conflict (`planning_conflict`, `reorder_stale`,
   `focus_conflict`) restores the last consistent rendering; the
   declared authorization denial renders the board read-only; foreign
   rows stay invisible through the declared not-found.
6. **Accessibility stays in the requirement and scenario layer.** The
   OpenSpec requirements bind announced state changes, keyboard
   reachability with disabled reasons, and drawer focus management to
   the same scenarios as the behavior; no UI DSL is required or added.
7. **The screen is not P0 core.** Everything screen-shaped lives in the
   fixture tree and the gate; the reusable future piece, if a second
   pilot appears, is a checker family — not an IR kind. Revisit the
   decision only with evidence the four existing kinds cannot express.

## Consequences

- Client generation, the OpenAPI document, the routes boundary, and
  the screen all answer to one join; a wire change is caught by the
  checked join in the same gate run.
- The #72 TypeScript/Go renderers now project the declared scalar
  mapping (`number`/`boolean` leave the string domain) and emit
  gofmt-stable Go struct alignment; the adapter bundle and manifest
  are rebuilt and re-pinned.
- A screen whose semantics cannot be phrased as projections of
  Query/Command/Endpoint/Scenario fails the gate visibly — that
  failure, not a new IR kind, is the escalation path.
