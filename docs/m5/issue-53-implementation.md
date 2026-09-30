# Issue #53: the planning-screen pilot — implementation notes and evidence

The research plan and decision live in
[issue-53-research.md](issue-53-research.md) and
[ADR 0046](../adr/0046-no-screen-view-ir-kind.md); this note records
what landed, the measured split, and the evidence a future screen
reuses. Public model, fixtures, and evidence name only `planner` — no
consumer-application name appears anywhere.

## What landed

- **The completed family in the model.** `planner.complete_planning`
  (stamps `completed_at` from the declared clock, refuses a second
  completion with the declared conflict), `planner.completed_planning`
  (newest day first), the `planner.task_completed` event, the
  `completed_at` optional timestamp on `planner.user_task_planning`,
  the `deny_foreign_planning` gate, and endpoint bindings
  `POST /planning/{planning_id}/complete` and `GET /planning/completed`.
  Four `planner.screen_*` scenario symbols join the transport endpoints
  to the screen behavior. IR recompiled: 58 definitions,
  digest `1fc2133a…`; Model envelope `233c179a…`.
- **Registry growth stays additive.** Two new operation bindings in the
  embedded #62 registry (`planner.complete_planning`,
  `planner.completed_planning`; 13 total), canonical bytes preserved;
  existing ids and retry authorizations untouched.
- **One join, five projections.** The staged evidence under
  `tests/fixtures/php-laravel/routes/inputs/` now carries the IR, the
  canonical transport attachment, the #46 OpenAPI document, the #72
  client-SDK contract (13 operations, 19 types), plus the new
  `lekalo/query-model.yaml` home with the six query plans — all
  re-pinned in one validated join by the new
  `regen-planner-routes` example (also the Model/IR digest oracle).
- **The generated/checked TypeScript client.**
  `tests/fixtures/php-laravel/routes/ui/generated/planner.client.ts` is
  the byte-verified render of the committed SDK evidence through the
  real node-typescript adapter kernel: typed buckets
  (`userTaskPlanning[]`), command inputs, the full declared error
  union per call, `Idempotency-Key` on every write, and the
  optimistic-concurrency field typed as a number. The Go client,
  compatibility sidecar, and ownership map ride along.
- **Renderer fidelity fixes (issue #72 follow-ups).** Named scalars
  project their declared mapping (`position`/`reorder_version` are
  `number`/`float64`, not strings), and the Go struct emission is
  gofmt-stable. The adapter bundle (`adapter.mjs`) and its manifest are
  rebuilt and re-pinned; the #72 fixture golden is byte-identical.
- **The maintained screen fixture.** `PlannerBoard.vue` renders the
  buckets from the declared queries, derives action availability from
  the machine-readable predicates, applies the optimistic complete with
  its declared rollback, renders loading/empty/degraded/conflict/
  read-only from declared symbols, and carries the E2E hooks and live
  region. It compiles with the pinned `@vue/compiler-sfc` and
  typechecks strict against the generated client.
- **The machine-readable UI projection.** `ui-projection.json` —
  task-card and detail-drawer DTOs (every field sourced to a declared
  entity field), six actions (endpoint, command, client method, policy,
  idempotency, availability, optimistic rollback, outcome table), five
  states with declared triggers, the timezone/date rules, and the
  accessibility expectations bound to scenarios.
- **Browser-E2E scenario bindings.** Two canonical scenario documents
  (`planner.screen_focus_flow`, `planner.screen_complete_day`) under
  the model home's `lekalo/scenarios/`, bound `native` to
  `web.runners/browser-e2e`, pinning the committed IR/Model digests.
- **Requirements and trace.** Six new OpenSpec requirements
  (completion, projections, traceable states, optimistic rollback,
  actor-local dates, accessibility); the attachment binds 33 references
  (queries, commands, endpoints, the entity, the scenarios);
  `lekalo requirements validate`: 12 requirements, 33 fresh, 0 stale,
  0 conflicts, 0 coverage gaps; the neutral trace is re-emitted
  (36 nodes, 33 relations, the known missing-gate candidate).
- **The planning battery grew S10.** The routes harness drives plan →
  complete → completed bucket → second-completion conflict (LEK-ERR-006)
  → foreign invisibility (the declared not-found, never an existence
  leak) → the other owner's empty bucket, through the real HTTP kernel.

## Measured split

- **Generated/derived:** the client module and sidecars (byte-checked),
  the OpenAPI document, the SDK evidence, the typed PHP surface, the
  scenario canonical bytes — all re-derivable from the committed join.
- **Maintained:** the screen SFC (~200 lines), the projection document,
  the two scenario documents, the completion port bodies
  (`TaskCompleter`, `CompletedReader`) and one policy adapter in the
  fixture app, plus the S10 battery stage — the same per-operation
  shape the #50 slice established: one port body, one binding line.
- **No core IR changed.** The screen never entered core; the only core
  surface touched is the additive #62 registry binding table.

## Gates

- `scripts/test-php-laravel-ui.mjs` (**new**, 6 stages): the checked
  join (fresh adapter render equals committed bytes), client ↔ OpenAPI
  sync, UI-projection traceability, canonical E2E bindings, strict
  screen typecheck, Go client compile.
- `scripts/test-php-laravel-routes.mjs`: 9/9 including the new S10
  stage. `scripts/test-php-laravel-operations.mjs`: 13/13.
- `cargo test --locked -p lekalo-core --lib` and the node contract
  gates (client-sdk, scenario, requirements, error-contracts,
  adapter-manifest golden) — all green; exact counts in the research
  note's verification block.

## Documented gaps

- The generated delegation handlers do not call the policy port inside
  the HTTP path; ownership scope is enforced by row invisibility (the
  declared not-found), and the policy ports are exercised by the
  scenario lane. The battery pins the honest behavior.
- The browser-E2E corpus stays minimal by design (two bound scenarios);
  `planner.screen_plan_conflict_rollback` and
  `planner.screen_stale_reorder` remain Model scenarios until a lane
  needs them.
- The requirements trace keeps `completeness: partial` with the
  release-verification gate gap, unchanged from #50.
