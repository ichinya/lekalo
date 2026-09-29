# Issue #50: the daily-planning vertical slice in `contracted` mode — research and implementation plan

## Scope and evidence

Branch `ichinya/m5-issue-50`, base `a5516b00` (the tip of
`ichinya/m5-issue-60`). The issue body was retrieved with
`gh issue view 50`: describe the daily-planning (Today planning) domain
as the first full greenfield vertical slice of Lekalo on a Laravel
backend — the canonical `planner.user_task_planning` module carried
through the model, the OpenSpec requirement bindings, the operations and
routes machinery of #59/#60, the OpenAPI projection, the TypeScript
client, portable scenarios, and trace evidence. Paths are
repository-relative; **proposed** marks new work; everything else was
inspected and executed on this base.

Read: `docs/m5/issue-59-research.md`, `docs/m5/issue-60-research.md`,
`docs/php-laravel-operations.md`, `docs/client-sdk.md` (the #72 Vue
consumer), `docs/requirements.md` (the #36 OpenSpec integration), and
the #60 implementation commits `b02f0bb2` (routes join) through
`a5516b00` (routes contract docs). The #60 routes harness was executed
locally on this base: 8/8 stages green against the provisioned planner
fixture.

Recommendation: do not build a second planning authority. The
`planner` corpus from #60 is already the vertical-slice harness — the
same model module gains the planning entity and its six commands and
three queries, the embedded #62 registry gains the planning error
family, and the existing composed types+operations+routes run carries
the whole surface to generated Laravel code plus maintained port
bodies. Everything complex stays maintained Laravel by design: the
closed v0.4.0 recipe vocabulary has no create/delete/multi-row/clock
recipes, so plan, unplan, reorder and pause delegate to typed ports —
which is exactly the `contracted` mode the issue demands.

## 1. The semantic model and where each invariant lives

The Model grammar (`model.schema.v0.2.16`) has no behavioral language:
invariants cross the boundary as (a) closed declarations the joins
check, (b) declared storage constraints, and (c) maintained port bodies
the scenarios exercise. Per requirement member:

- **`planner.user_task_planning` entity** — *proposed* entity with
  `planning_id` (uuid identity), `workspace_id`/`user_id`/`task_id`
  (uuid), `planned_for` (`planner.planning_date`, base `date`),
  `position` (number), `focused_at`/`paused_at` (datetime, nullable in
  the wire, presence in the type), `reorder_version` (number). The
  per-workspace/user/task unique identity is declared twice: the
  entity `identity` list, and the fixture migration's composite unique
  index — the join checks the declaration, the runtime lane proves the
  index.
- **Commands plan/move/unplan/reorder/focus/pause** — *proposed*
  commands with typed inputs and effect lists (`derived_from` the
  planning requirements). `planner.focus_task` stays the #59
  `single-entity-update` managed recipe. The other five are managed
  `port-delegation` commands: one typed port call carries the
  maintained body (`PlanningStore::plan/move/unplan/reorder/pause`),
  each wrapped by the generated handler's policy check, idempotency
  key handling and required `TransactionPort` run. A create (plan), a
  delete (unplan), a multi-row optimistic reorder and a clock-reading
  pause (paused_at = now) are outside the closed v0.4.0 update recipe
  on purpose; widening the recipe vocabulary for them is a separate
  reviewed contract change, not a silent one.
- **Queries today/backlog/carry-over** — *proposed* queries returning
  `list<planner.user_task_planning>` over reads of the planning entity,
  each a managed `port-delegation` query (queries accept only
  port-delegation; transaction forbidden). Timezone/date semantics:
  the Model declares the date semantics in descriptions and requirement
  bindings; the *actor-local today* computation lives in the maintained
  query port, which receives the actor context the auth boundary built
  — the wire stays free of inferred parameters.
- **Tenant/user authorization** — one *proposed* deny policy per
  family (`planner.deny_foreign_planning` on the planning commands,
  mirroring `planner.deny_bulk_focus`), bound per transport endpoint
  through `auth.policyRef` and enforced by the generated handler's
  narrow `authorize` port. The maintained auth middleware binds the
  bearer principal to the actor; user ids in payloads never become
  authority.
- **One-active-focus invariant** — declared on the entity description
  and requirement binding, and enforced by a *partial unique index* on
  the fixture's tasks table (one focused row per user), not by an
  application check. The concurrent-focus scenario must trigger the
  constraint itself, so the guarantee is the database's inside the
  declared transaction, not a race-prone read-check-write.
- **Optimistic reorder version** — `reorder_version` on the entity;
  `planner.reorder_planned` input carries `expected_version`; a
  mismatch raises the declared `planner.reorder_stale` conflict inside
  the transaction port (compare-and-swap, declared storage semantics).
- **Idempotency** — every planning command endpoint declares the
  `Idempotency-Key` header (required) in the transport attachment; the
  generated request binding refuses a missing key as the typed
  validation error; the maintained port bodies deduplicate durable
  replays (same key + same canonical request → replay, same key +
  different request → conflict) inside the same transaction.
- **Provider events never mutate local planning fields** — the
  *maintained* provider webhook route (outside the generated tree)
  updates provider-owned task columns only; the scenario proves the
  planning row bytes are unchanged after a webhook delivery. Ownership
  separation is validated at three levels: the generated plan never
  writes outside `.lekalo/generated/**`, the manual routes file is
  byte-identical (the #60 scope-immunity stage), and the webhook
  scenario asserts planning-field invariance.

## 2. The embedded error registry grows additively

The operations join demands exact #62 registry-binding equality per
operation, and the OpenAPI render resolves every transport error
through the same embedded registry. The registry
(`contracts/error-registry.v0.2.16.json`) is closed but additive:
*proposed* new error ids `planner.planning_conflict` (LEK-ERR-006,
write/key-required conflict), `planner.planning_denied` (LEK-ERR-007,
auth, empty public payload), `planner.planning_not_found` (LEK-ERR-008,
not-found), `planner.reorder_stale` (LEK-ERR-009, write/key-required
conflict), plus registry bindings for the eight new operations. The
existing ids (`input_invalid`, `store_unavailable`, `task_not_found`,
`focus_*`) are reused where the semantics match — no duplicated
vocabulary. The registry release gate
(`scripts/test-error-contracts.mjs`) checks schema, canonical bytes,
and invariants but pins no entry counts; the Rust count assertions in
`crates/lekalo-core/tests/error_contract.rs` grow with the family.

## 3. The corpus: model, requirements, transport, inputs

The vertical-slice corpus is *proposed* to live in the existing
`tests/fixtures/php-laravel/routes/` model home (the planner corpus of
#60), extended in place:

- Model module (`lekalo/modules/planner/`): new scalars
  (`planning_date`, `planning_id`, `position`, `reorder_version`,
  `workspace_id`, `user_id`), the entity, six commands (focus stays),
  three queries, two deny policies, four planning events
  (planned/unplanned/reordered/paused), eight endpoint bindings, and
  the planning effects. `derived_from` requirement ids
  (`PLANNER-REQ-001..006`) mark every symbol with its requirement.
- OpenSpec bindings (*proposed*): a requirements attachment
  (`lekalo/requirements/planner.requirements.json`) plus the OpenSpec
  provider tree (`openspec/specs/planner/spec.md`) next to the corpus
  model, one reference per governed symbol, revisions pinned to exact
  body digests — validated read-only through the #36 handoff.
- The IR evidence is recompiled once with `lekalo load --ir` and
  committed; every digest in the transport/operations/routes inputs
  rides as the zero placeholder the harnesses replace (the #60
  convention).
- The transport attachment gains the eight planning endpoints (method
  and path stay single-sourced in the Model endpoint symbols; the
  attachment binds params/body/errors/status/idempotency/auth per
  endpoint). Reorder's wire body is three flat scalar fields —
  `planned_for`, `expected_version`, and the ordered id list packed as
  one declared string member — because the v0.4.0 routes decode plan
  resolves typed scalar members only; the port body splits and
  validates the order, and the limitation is recorded here instead of
  widening the closed decode grammar silently.
- The OpenAPI golden is re-rendered from the validated join with the
  embedded registry (`lekalo openapi render`) and committed; the
  operations and routes inputs name the new operations/endpoints in
  canonical id order.

## 4. The fixture and the nine scenarios

The planner Laravel fixture (the real bootable app) gains only
maintained integration code: the `user_task_plannings` migration with
the composite unique index and the partial one-active-focus index; the
`App\Lekalo` adapters behind the new ports (`EloquentPlanningStore`,
`EloquentPlanningQueries`), a planner-auth upgrade that carries the
actor workspace/timezone, container bindings for the generated port
interfaces, and the manual provider-webhook route. The runtime driver
(`tests/fixtures/php-laravel/routes/runtime/driver.php`) gains the
scenario battery, each through the real HTTP kernel and the generated
routes:

1. **plan/move/unplan** — plan (202), today lists the row, move
   repositions, unplan removes; state assertions read the database.
2. **two users plan one task independently** — two actor identities,
   one task, two planning rows; the composite unique index is
   (workspace, user, task), never task alone.
3. **carry-over without silent date change** — a row planned for a
   prior date is served by carry-over at its original `planned_for`;
   the row's stored bytes are identical before and after the query
   (read-only proof).
4. **concurrent focus requests** — the partial unique index refuses a
   second active focus for the same user inside the declared
   transaction; the proof is the database constraint firing (through a
   second connection against the same file database), plus the
   idempotent replay of one focus key.
5. **stale reorder** — reorder with a wrong `expected_version` answers
   the declared `planner.reorder_stale` conflict status; the stored
   order is untouched.
6. **midnight/timezone boundary** — the actor timezone selects the
   today set across a UTC midnight boundary; the same stored rows,
   two actor timezones, two disjoint today answers.
7. **repeated idempotency key** — one plan key twice: one planning
   row, the second response replays the first outcome.
8. **deleted/unavailable task** — plan for an unknown task answers the
   declared not-found mapping; the fixture negative-control seam
   answers the declared infrastructure failure.
9. **provider webhook does not overwrite planning** — the maintained
   webhook mutates provider-owned columns; every planning field of the
   affected rows is byte-identical after delivery.

The harness (`scripts/test-php-laravel-routes.mjs`) grows the runtime
stages; the managed custody, byte-stability, surface, error-table,
checked-custody, scope-immunity and OpenAPI-drift stages stay and now
cover the wider inventory.

## 5. Client, trace and comparison evidence

- **TypeScript client (#72).** The language-neutral client-SDK
  projection renders `.client.ts` (+ Go) from the same transport join;
  the Vue consumer fixture typechecks strict and compiles its SFC
  against the generated module. The planning endpoints extend that
  projection so the Vue client, the OpenAPI document, and the native
  tests are projections of one validated join — synchronization is
  structural, not aspirational.
- **Trace + HLV.** A committed trace manifest binds the planning
  requirements to their symbols, scenarios and gates; `lekalo trace
  validate/export/query` and the `lekalo context` capsule for
  `planner.focus_task` are the small-context evidence: the capsule
  resolves the protected facts of the operation without opening any
  consumer repository.
- **Baseline vs Lekalo-assisted metrics.** Measured after landing: the
  generated/maintained byte split of the vertical slice, the count of
  hand-written integration lines the fixture needed per governed
  operation, and the scenario count per acceptance criterion — the
  neutral denominators a Node/Go target comparison reuses.

## 6. Acceptance mapping

1. **Every source-requirement invariant explicit** — §1 maps each
   member of the issue's semantic model to its owning declaration.
2. **Maintained Laravel in `contracted` mode** — the closed recipe
   vocabulary keeps plan/unplan/reorder/pause bodies in maintained
   ports; generated code is decode/policy/transaction/encode only.
3. **`focus_task` context capsule without the consumer repo** — the
   `lekalo context` evidence plus the operations map's bounded summary.
4. **Concurrent focus proves a real transaction guarantee** — the
   partial unique index fires inside the declared transaction; the
   scenario triggers the constraint, not an application check.
5. **Provider/planning ownership separation validated** — plan-scope
   immunity plus the webhook field-invariance scenario.
6. **OpenAPI, Vue client, native tests synchronized** — one join
   projects all three; the harness compares envelope/status/error
   identities against the declared tables and the emitted document.
7. **Usable for future Node/Go comparison** — neutral corpus, IR,
   scenarios, OpenAPI and client-SDK projection carry no runtime
   specifics beyond the declared target bindings.
8. **No consumer-application name in public evidence** — the corpus
   speaks only `planner`; fixture integration code lives under the
   neutral `App\Lekalo` namespace.

Research validation: live issue retrieval, source/schema/CI inspection
on this base, the #59/#60 implementations read end-to-end, the #60
routes harness executed green locally (8/8 stages), the composer
vendor tree provisioned per the committed lock. No planning runtime
lane existed before this branch; every gate above is an implementation
requirement of the following commits.
